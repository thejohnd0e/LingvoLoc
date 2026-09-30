//! Layout-preserving translation of text PDFs.
//!
//! Analysis reads every page with PDFium, groups text objects into paragraphs
//! (`layout`), and stores each translatable paragraph as a block with the
//! stable id `pdf#pNNNN-KKK`. Export repeats the same deterministic analysis on
//! the unchanged source, removes the original text objects of every translated
//! paragraph, and draws the translation with an embedded font inside the
//! original region, keeping vector graphics, images and untouched text.

mod engine;
mod fonts;
pub mod layout;

use pdfium_render::prelude::*;

use crate::documents::{BlockType, DocumentBlock};
use crate::domain::RuntimeError;

use fonts::FontSet;
use layout::{Background, Fragment, Kind, Paragraph, Rect};

pub const PARSER_VERSION: &str = "pdf-v1";

/// Pages chosen by the user, e.g. `5-12, 20, 30-` (1-based, inclusive, `N-` runs
/// to the last page). No selection means the whole book.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageSelection {
    ranges: Vec<(usize, Option<usize>)>,
}

impl PageSelection {
    pub fn parse(spec: &str) -> Result<Option<Self>, RuntimeError> {
        let mut ranges = Vec::new();
        for part in spec
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
        {
            let invalid = || RuntimeError::InvalidInput(format!("invalid page range \"{part}\""));
            let number = |text: &str| {
                text.trim()
                    .parse::<usize>()
                    .ok()
                    .filter(|value| *value >= 1)
                    .ok_or_else(invalid)
            };
            let range = match part.split_once('-') {
                None => {
                    let page = number(part)?;
                    (page, Some(page))
                }
                Some((start, "")) => (number(start)?, None),
                Some((start, end)) => {
                    let (start, end) = (number(start)?, number(end)?);
                    if end < start {
                        return Err(invalid());
                    }
                    (start, Some(end))
                }
            };
            ranges.push(range);
        }
        Ok((!ranges.is_empty()).then_some(Self { ranges }))
    }

    /// Canonical text, stored in the job configuration version.
    pub fn canonical(&self) -> String {
        self.ranges
            .iter()
            .map(|(start, end)| match end {
                Some(end) if end == start => start.to_string(),
                Some(end) => format!("{start}-{end}"),
                None => format!("{start}-"),
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Zero-based page numbers inside a document of `count` pages.
    fn pages(&self, count: usize) -> Result<Vec<usize>, RuntimeError> {
        let pages: Vec<usize> = (0..count)
            .filter(|page| {
                self.ranges
                    .iter()
                    .any(|(start, end)| page + 1 >= *start && end.is_none_or(|end| *page < end))
            })
            .collect();
        if pages.is_empty() {
            return Err(RuntimeError::InvalidInput(format!(
                "pages {} are outside the document ({count} pages)",
                self.canonical()
            )));
        }
        Ok(pages)
    }
}

/// Job configuration version for an optional page selection.
pub fn configuration_version(pages: Option<&PageSelection>) -> String {
    match pages {
        Some(pages) => format!("{PARSER_VERSION};pages={}", pages.canonical()),
        None => PARSER_VERSION.to_string(),
    }
}

/// Recovers the selection from a job's configuration version.
pub fn selection_from_configuration(version: &str) -> Result<Option<PageSelection>, RuntimeError> {
    match version.split_once(";pages=") {
        Some((_, spec)) => PageSelection::parse(spec),
        None => Ok(None),
    }
}

#[derive(Debug, Clone)]
pub struct Analysis {
    pub blocks: Vec<DocumentBlock>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Exported {
    pub bytes: Vec<u8>,
    pub diagnostics: Vec<String>,
}

const MAX_PAGES: usize = 3000;
const UNIFORM_FLOOR: f32 = 0.8;

struct PageModel {
    fragments: Vec<Fragment>,
    paragraphs: Vec<Paragraph>,
    backgrounds: Vec<Background>,
    /// Thin horizontal lines (underlines, table borders).
    rules: Vec<Rect>,
    width: f32,
    supported: bool,
}

fn pdfium_error(context: &str, error: PdfiumError) -> RuntimeError {
    RuntimeError::InvalidInput(format!("{context}: {error}"))
}

fn rect_of(bounds: PdfRect) -> Rect {
    Rect {
        left: bounds.left().value,
        bottom: bounds.bottom().value,
        right: bounds.right().value,
        top: bounds.top().value,
    }
}

fn color_of(color: PdfColor) -> [u8; 4] {
    [color.red(), color.green(), color.blue(), color.alpha()]
}

fn read_page(page: &PdfPage) -> PageModel {
    let width = page.width().value;
    let rotated = page
        .rotation()
        .map(|rotation| rotation != PdfPageRenderRotation::None)
        .unwrap_or(false);
    let mut fragments = Vec::new();
    let mut backgrounds = Vec::new();
    let mut rules = Vec::new();
    for (index, object) in page.objects().iter().enumerate() {
        let Ok(bounds) = object.bounds() else {
            continue;
        };
        let bounds = rect_of(bounds.to_rect());
        if let Some(text) = object.as_text_object() {
            if !text.is_visible() {
                continue;
            }
            let font = text.font();
            let name = font.name().to_ascii_lowercase();
            let baseline = object
                .matrix()
                .map(|matrix| matrix.f())
                .unwrap_or(bounds.bottom);
            let size = text.scaled_font_size().value;
            fragments.push(Fragment {
                object: index,
                text: text.text(),
                bounds,
                baseline,
                size: if size > 0.0 { size } else { 10.0 },
                bold: name.contains("bold")
                    || name.contains("black")
                    || font
                        .weight()
                        .map(|weight| {
                            matches!(
                                weight,
                                PdfFontWeight::Weight700Bold
                                    | PdfFontWeight::Weight800
                                    | PdfFontWeight::Weight900
                            ) || matches!(weight, PdfFontWeight::Custom(value) if value >= 700)
                        })
                        .unwrap_or(false),
                italic: name.contains("italic") || name.contains("oblique") || font.is_italic(),
                serif: font.is_serif() || (name.contains("serif") && !name.contains("sans")),
                color: object.fill_color().map(color_of).unwrap_or([0, 0, 0, 255]),
            });
        } else if let Some(path) = object.as_path_object() {
            if bounds.height() <= 3.0 && bounds.width() >= 20.0 {
                rules.push(bounds);
            }
            let filled = path
                .fill_mode()
                .map(|mode| mode != PdfPathFillMode::None)
                .unwrap_or(false);
            if filled && bounds.width() > 8.0 && bounds.height() > 8.0 {
                if let Ok(color) = object.fill_color() {
                    backgrounds.push(Background {
                        bounds,
                        color: color_of(color),
                    });
                }
            }
        }
    }
    let body = layout::body_font_size(&fragments);
    let lines = layout::build_lines(&fragments);
    let paragraphs = layout::build_paragraphs(lines, &backgrounds, body);
    backgrounds.retain(|background| background.bounds.height() <= layout::MAX_PANEL_HEIGHT);
    PageModel {
        fragments,
        paragraphs,
        backgrounds,
        rules,
        width,
        supported: !rotated,
    }
}

fn block_id(page: usize, index: usize) -> String {
    format!("pdf#p{:04}-{:03}", page + 1, index)
}

fn translatable(paragraph: &Paragraph) -> bool {
    matches!(paragraph.kind, Kind::Heading | Kind::Body)
}

pub fn analyze(source: &[u8], pages: Option<&PageSelection>) -> Result<Analysis, RuntimeError> {
    engine::with_pdfium(|pdfium| analyze_with(pdfium, source, pages))
}

fn analyze_with(
    pdfium: &Pdfium,
    source: &[u8],
    selection: Option<&PageSelection>,
) -> Result<Analysis, RuntimeError> {
    let document = pdfium
        .load_pdf_from_byte_slice(source, None)
        .map_err(|error| pdfium_error("open PDF", error))?;
    let pages = document.pages();
    let count = pages.len() as usize;
    if count == 0 || count > MAX_PAGES {
        return Err(RuntimeError::InvalidInput(format!(
            "PDF has {count} pages; supported range is 1..={MAX_PAGES}"
        )));
    }

    let selected = match selection {
        Some(selection) => selection.pages(count)?,
        None => (0..count).collect(),
    };
    let mut blocks = Vec::new();
    let mut diagnostics = Vec::new();
    let mut rotated = Vec::new();
    let mut without_text = Vec::new();
    for number in selected {
        let page = pages
            .get(number as PdfPageIndex)
            .map_err(|error| pdfium_error("read PDF page", error))?;
        let model = read_page(&page);
        if !model.supported {
            rotated.push(number + 1);
            continue;
        }
        if model.fragments.iter().all(|f| f.text.trim().is_empty()) {
            without_text.push(number + 1);
            continue;
        }
        let mut index = 0;
        for paragraph in &model.paragraphs {
            match paragraph.kind {
                Kind::Skip(_) => {}
                Kind::Heading | Kind::Body => {
                    blocks.push(DocumentBlock {
                        id: block_id(number, index),
                        ordinal: blocks.len() as i64,
                        block_type: if paragraph.kind == Kind::Heading {
                            BlockType::Heading
                        } else {
                            BlockType::Paragraph
                        },
                        source_text: paragraph.text.clone(),
                        translated_text: None,
                    });
                    index += 1;
                }
            }
        }
    }
    if !without_text.is_empty() {
        diagnostics.push(format!(
            "Pages without extractable text stay unchanged (scanned or image-only): {}",
            page_list(&without_text)
        ));
    }
    if !rotated.is_empty() {
        diagnostics.push(format!(
            "Rotated pages are not supported and stay unchanged: {}",
            page_list(&rotated)
        ));
    }
    Ok(Analysis {
        blocks,
        diagnostics,
    })
}

fn page_list(pages: &[usize]) -> String {
    let shown: Vec<String> = pages.iter().take(30).map(usize::to_string).collect();
    if pages.len() > shown.len() {
        format!(
            "{} and {} more",
            shown.join(", "),
            pages.len() - shown.len()
        )
    } else {
        shown.join(", ")
    }
}

/// Vertical room below a paragraph: up to the next paragraph that overlaps it
/// horizontally, the bottom of its background panel, or the page text margin.
fn bottom_limit(
    index: usize,
    paragraphs: &[Paragraph],
    backgrounds: &[Background],
    rules: &[Rect],
    page_bottom: f32,
) -> f32 {
    let paragraph = &paragraphs[index];
    let mut limit = page_bottom;
    for (other_index, other) in paragraphs.iter().enumerate() {
        if other_index != index
            && other.lines[0].baseline < paragraph.lines[paragraph.lines.len() - 1].baseline - 0.1
            && other.bounds.overlaps_horizontally(&paragraph.bounds)
        {
            limit = limit.max(other.bounds.top + paragraph.lines[0].size * 0.2);
        }
    }
    // A panel or table cell that starts below the paragraph is an obstacle.
    for background in backgrounds {
        if background.bounds.top <= paragraph.bounds.bottom + 1.0
            && background.bounds.overlaps_horizontally(&paragraph.bounds)
            && paragraph.container != Some(background.bounds)
        {
            limit = limit.max(background.bounds.top + 1.5);
        }
    }
    for rule in rules {
        if rule.top <= paragraph.bounds.bottom + 1.0
            && rule.top >= paragraph.bounds.top - paragraph.bounds.height() * 2.0 - 40.0
            && rule.overlaps_horizontally(&paragraph.bounds)
            && rule.top < paragraph.lines[0].baseline
        {
            limit = limit.max(rule.top + 1.5);
        }
    }
    if let Some(container) = paragraph.container {
        let padding = (paragraph.bounds.left - container.left)
            .clamp(2.0, 8.0)
            .max(paragraph.lines[0].size * 0.3);
        limit = limit.max(container.bottom + padding);
    }
    limit.min(paragraph.bounds.bottom)
}

/// How far the first baseline may move up: to the panel top, the paragraph or
/// panel edge above it, whichever comes first.
fn shift_room(
    index: usize,
    paragraphs: &[Paragraph],
    backgrounds: &[Background],
    rules: &[Rect],
) -> f32 {
    let paragraph = &paragraphs[index];
    let first = &paragraph.lines[0];
    let mut ceiling = paragraph.bounds.top + 30.0;
    if let Some(container) = paragraph.container {
        let padding = (paragraph.bounds.left - container.left).clamp(2.0, 8.0);
        ceiling = ceiling.min(container.top - padding);
    }
    for (other_index, other) in paragraphs.iter().enumerate() {
        if other_index != index
            && other.lines[other.lines.len() - 1].baseline > first.baseline + 0.1
            && other.bounds.overlaps_horizontally(&paragraph.bounds)
        {
            ceiling = ceiling.min(other.bounds.bottom - first.size * 0.1);
        }
    }
    for background in backgrounds {
        if background.bounds.bottom >= paragraph.bounds.top - 0.5
            && background.bounds.overlaps_horizontally(&paragraph.bounds)
            && paragraph.container != Some(background.bounds)
        {
            ceiling = ceiling.min(background.bounds.bottom - 3.0);
        }
    }
    for rule in rules {
        if rule.bottom >= paragraph.bounds.top - 0.5
            && rule.bottom <= paragraph.bounds.top + 40.0
            && rule.overlaps_horizontally(&paragraph.bounds)
        {
            ceiling = ceiling.min(rule.bottom - 1.5);
        }
    }
    (ceiling - paragraph.bounds.top - first.size * 0.2).max(0.0)
}

/// How far the region may grow to the left (table cell, header): up to the
/// left neighbour on the same rows or the panel edge; zero for plain text.
fn left_slack(index: usize, paragraphs: &[Paragraph]) -> f32 {
    let paragraph = &paragraphs[index];
    let size = paragraph.lines[0].size;
    let mut edge = None;
    if let Some(container) = paragraph.container {
        edge = Some(container.left + (paragraph.bounds.left - container.left).clamp(2.0, 8.0));
    }
    for (other_index, other) in paragraphs.iter().enumerate() {
        if other_index != index
            && other.bounds.right <= paragraph.bounds.left + 0.5
            && other.bounds.bottom < paragraph.bounds.top
            && other.bounds.top > paragraph.bounds.bottom
        {
            let limit = other.bounds.right + size * 0.5;
            edge = Some(edge.map_or(limit, |current: f32| current.max(limit)));
        }
    }
    edge.map_or(0.0, |edge| (paragraph.bounds.left - edge).max(0.0))
}

fn region(index: usize, paragraphs: &[Paragraph], page_right: f32, page_width: f32) -> (f32, f32) {
    let paragraph = &paragraphs[index];
    let lines = &paragraph.lines;
    let size = lines[0].size;
    // The leftmost line: a list item's first line starts left of its
    // continuation, an indented first line starts right of the rest.
    let left = if lines[0].item_start {
        // Continuations of a list item wrap back to the margin, under the bullet.
        lines[0].bounds.left
    } else {
        lines
            .iter()
            .map(|line| line.bounds.left)
            .fold(f32::MAX, f32::min)
    };
    let mut allowed = match paragraph.container {
        Some(container) => {
            container.right - (paragraph.bounds.left - container.left).clamp(2.0, 8.0)
        }
        None => page_right.min(page_width - 20.0),
    };
    // A neighbour on the same rows (table cell, label column) bounds the width.
    for (other_index, other) in paragraphs.iter().enumerate() {
        if other_index != index
            && other.bounds.left >= paragraph.bounds.right - 0.5
            && other.bounds.bottom < paragraph.bounds.top
            && other.bounds.top > paragraph.bounds.bottom
        {
            allowed = allowed.min(other.bounds.left - size * 0.5);
        }
    }
    let right = paragraph.bounds.right.max(allowed);
    (left, (right - left).max(size))
}

/// With a page selection the output holds only the selected pages, which keeps
/// manual review fast; block ids keep the original page numbers.
pub fn export(
    source: &[u8],
    blocks: &[DocumentBlock],
    pages: Option<&PageSelection>,
) -> Result<Exported, RuntimeError> {
    engine::with_pdfium(|pdfium| export_with(pdfium, source, blocks, pages))
}

fn export_with(
    pdfium: &Pdfium,
    source: &[u8],
    blocks: &[DocumentBlock],
    selection: Option<&PageSelection>,
) -> Result<Exported, RuntimeError> {
    let mut document = pdfium
        .load_pdf_from_byte_slice(source, None)
        .map_err(|error| pdfium_error("open PDF", error))?;
    let count = document.pages().len() as usize;
    let selected = match selection {
        Some(selection) => selection.pages(count)?,
        None => (0..count).collect(),
    };
    let fonts = FontSet::load()?;
    let mut tokens = std::collections::HashMap::new();
    let mut diagnostics = Vec::new();
    let mut review_pages = Vec::new();

    for &number in &selected {
        let mut page = document
            .pages()
            .get(number as PdfPageIndex)
            .map_err(|error| pdfium_error("read PDF page", error))?;
        let model = read_page(&page);
        if !model.supported {
            continue;
        }
        // Regenerating the content stream after every object is very slow.
        page.set_content_regeneration_strategy(PdfPageContentRegenerationStrategy::Manual);
        let page_right = model
            .paragraphs
            .iter()
            .map(|paragraph| paragraph.bounds.right)
            .fold(0.0_f32, f32::max);
        let page_bottom = model
            .paragraphs
            .iter()
            .map(|paragraph| paragraph.bounds.bottom)
            .fold(f32::MAX, f32::min)
            .min(60.0)
            - 4.0;

        struct Job {
            objects: Vec<usize>,
            text: String,
            left: f32,
            width: f32,
            limit: f32,
            baseline: f32,
            size: f32,
            pitch: f32,
            room: f32,
            slack: f32,
            /// Original paragraph box, for neighbour checks.
            top: f32,
            box_left: f32,
            box_right: f32,
            lines: usize,
            placement: Option<layout::Placement>,
            serif: bool,
            bold: bool,
            italic: bool,
            color: [u8; 4],
        }
        let mut jobs = Vec::new();
        let mut index = 0;
        for (position, paragraph) in model.paragraphs.iter().enumerate() {
            if !translatable(paragraph) {
                continue;
            }
            let id = block_id(number, index);
            index += 1;
            let Some(translation) = blocks
                .iter()
                .find(|block| block.id == id)
                .and_then(|block| block.translated_text.as_deref())
                .filter(|text| !text.trim().is_empty())
            else {
                continue;
            };
            let first = &paragraph.lines[0];
            let pitch = if paragraph.lines.len() > 1 {
                (first.baseline - paragraph.lines[paragraph.lines.len() - 1].baseline)
                    / (paragraph.lines.len() - 1) as f32
            } else {
                first.size * 1.2
            };
            let (left, width) = region(position, &model.paragraphs, page_right, model.width);
            let limit = bottom_limit(
                position,
                &model.paragraphs,
                &model.backgrounds,
                &model.rules,
                page_bottom,
            );
            let objects = paragraph
                .lines
                .iter()
                .flat_map(|line| line.fragments.iter())
                .map(|&fragment| model.fragments[fragment].object)
                .collect();
            jobs.push(Job {
                objects,
                text: translation.to_string(),
                left,
                width,
                limit,
                baseline: first.baseline,
                size: first.size,
                pitch,
                room: shift_room(
                    position,
                    &model.paragraphs,
                    &model.backgrounds,
                    &model.rules,
                ),
                slack: left_slack(position, &model.paragraphs),
                top: paragraph.bounds.top,
                box_left: paragraph.bounds.left,
                box_right: paragraph.bounds.right,
                lines: paragraph.lines.len(),
                placement: None,
                serif: first.serif,
                bold: first.bold,
                italic: first.italic,
                color: first.color,
            });
        }
        if jobs.is_empty() {
            continue;
        }

        // Fit each paragraph, then give paragraphs of the same original size the
        // same scale so neighbouring text does not differ in size.
        // Top to bottom: a paragraph may move up only into space that the
        // (already placed) paragraphs above it have not taken.
        let place_all = |jobs: &mut [Job], caps: &dyn Fn(&Job) -> f32| {
            for index in 0..jobs.len() {
                let mut room = jobs[index].room;
                for earlier in &jobs[..index] {
                    let job = &jobs[index];
                    if earlier.box_right > job.box_left && job.box_right > earlier.box_left {
                        if let Some(placed) = earlier.placement.as_ref() {
                            if let Some(last) = placed.lines.last() {
                                let bottom = last.baseline - placed.size * 0.3;
                                room = room.min(bottom - job.top - job.size * 0.15);
                            }
                        }
                    }
                }
                let job = &jobs[index];
                let placement = layout::place(
                    &job.text,
                    job.left,
                    job.width,
                    job.baseline,
                    job.limit,
                    job.size,
                    job.pitch,
                    job.bold,
                    job.italic,
                    caps(job),
                    room.max(0.0),
                    job.slack,
                    job.lines,
                    &fonts.measure(job.serif),
                );
                jobs[index].placement = Some(placement);
            }
        };
        // One line-spacing ratio per group of same-sized text on the page.
        let ratio_of = |job: &Job| job.pitch / job.size;
        let mut ratios: std::collections::HashMap<(i32, bool), Vec<f32>> =
            std::collections::HashMap::new();
        for job in jobs.iter().filter(|job| job.lines > 1) {
            ratios
                .entry(((job.size * 2.0).round() as i32, job.bold))
                .or_default()
                .push(ratio_of(job));
        }
        let page_ratio = {
            let mut all: Vec<f32> = ratios.values().flatten().copied().collect();
            all.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            all.get(all.len() / 2).copied().unwrap_or(1.25)
        };
        let ratio_by_group: std::collections::HashMap<(i32, bool), f32> = ratios
            .into_iter()
            .map(|(key, mut values)| {
                values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                (key, values[values.len() / 2])
            })
            .collect();
        for job in &mut jobs {
            let ratio = ratio_by_group
                .get(&((job.size * 2.0).round() as i32, job.bold))
                .copied()
                .unwrap_or(page_ratio);
            job.pitch = ratio * job.size;
        }
        place_all(&mut jobs, &|_| 1.0);
        let mut groups: std::collections::HashMap<(i32, bool), Vec<f32>> =
            std::collections::HashMap::new();
        for job in &jobs {
            if let Some(placement) = job.placement.as_ref().filter(|p| !p.overflow) {
                groups
                    .entry(((job.size * 2.0).round() as i32, job.bold))
                    .or_default()
                    .push(placement.scale);
            }
        }
        // The smallest scale in the group, but not below UNIFORM_FLOOR: a single
        // very long paragraph must not shrink the whole page.
        let medians: std::collections::HashMap<(i32, bool), f32> = groups
            .into_iter()
            .map(|(key, scales)| {
                let smallest = scales.into_iter().fold(1.0_f32, f32::min);
                (key, smallest.max(UNIFORM_FLOOR))
            })
            .collect();
        place_all(&mut jobs, &|job| {
            medians
                .get(&((job.size * 2.0).round() as i32, job.bold))
                .copied()
                .unwrap_or(1.0)
        });
        if jobs.iter().any(|job| {
            job.placement
                .as_ref()
                .is_some_and(layout::Placement::needs_review)
        }) {
            review_pages.push(number + 1);
        }

        let mut remove: Vec<usize> = jobs.iter().flat_map(|job| job.objects.clone()).collect();
        remove.sort_unstable();
        remove.dedup();
        for object in remove.into_iter().rev() {
            page.objects_mut()
                .remove_object_at_index(object)
                .map_err(|error| pdfium_error("remove original text", error))?;
        }
        for job in &jobs {
            let key = (job.serif, job.bold, job.italic);
            let token = match tokens.get(&key) {
                Some(token) => *token,
                None => {
                    let bytes = fonts.bytes(job.serif, job.bold, job.italic);
                    let token = document
                        .fonts_mut()
                        .load_true_type_from_bytes(bytes, true)
                        .map_err(|error| pdfium_error("embed translation font", error))?;
                    tokens.insert(key, token);
                    token
                }
            };
            let placement = job.placement.as_ref().expect("placed above");
            for line in &placement.lines {
                let mut text = PdfPageTextObject::new(
                    &document,
                    &line.text,
                    token,
                    PdfPoints::new(placement.size),
                )
                .map_err(|error| pdfium_error("create translated text", error))?;
                text.set_fill_color(PdfColor::new(
                    job.color[0],
                    job.color[1],
                    job.color[2],
                    job.color[3],
                ))
                .map_err(|error| pdfium_error("colour translated text", error))?;
                text.translate(PdfPoints::new(line.x), PdfPoints::new(line.baseline))
                    .map_err(|error| pdfium_error("position translated text", error))?;
                page.objects_mut()
                    .add_text_object(text)
                    .map_err(|error| pdfium_error("add translated text", error))?;
            }
        }
        page.regenerate_content()
            .map_err(|error| pdfium_error("write translated page", error))?;
    }
    if !review_pages.is_empty() {
        review_pages.dedup();
        diagnostics.push(format!(
            "Translated text was shrunk a lot or did not fit its region; review pages: {}",
            page_list(&review_pages)
        ));
    }
    if selection.is_some() {
        for number in (0..count).rev().filter(|number| !selected.contains(number)) {
            document
                .pages()
                .get(number as PdfPageIndex)
                .and_then(PdfPage::delete)
                .map_err(|error| pdfium_error("drop unselected page", error))?;
        }
    }
    let bytes = document
        .save_to_bytes()
        .map_err(|error| pdfium_error("save PDF", error))?;
    Ok(Exported { bytes, diagnostics })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a one-page PDF with a heading, two body lines and a code line.
    fn fixture() -> Option<Vec<u8>> {
        engine::with_pdfium(|pdfium| {
            let mut document = pdfium
                .create_new_pdf()
                .map_err(|error| pdfium_error("create", error))?;
            let times = document.fonts_mut().times_roman();
            let bold = document.fonts_mut().times_bold();
            let mut page = document
                .pages_mut()
                .create_page_at_end(PdfPagePaperSize::a4())
                .map_err(|error| pdfium_error("page", error))?;
            let lines = [
                ("Introduction to the system", bold, 20.0, 760.0),
                (
                    "The system reads every page of the book",
                    times,
                    12.0,
                    720.0,
                ),
                ("and keeps the original layout intact.", times, 12.0, 706.0),
                ("client.ask(prompt=user_question)", times, 12.0, 660.0),
            ];
            for (text, font, size, y) in lines {
                let mut object =
                    PdfPageTextObject::new(&document, text, font, PdfPoints::new(size))
                        .map_err(|error| pdfium_error("text", error))?;
                object
                    .translate(PdfPoints::new(72.0), PdfPoints::new(y))
                    .map_err(|error| pdfium_error("move", error))?;
                page.objects_mut()
                    .add_text_object(object)
                    .map_err(|error| pdfium_error("add", error))?;
            }
            document
                .save_to_bytes()
                .map_err(|error| pdfium_error("save", error))
        })
        .ok()
    }

    #[test]
    fn translates_paragraphs_in_place_and_keeps_code() {
        let Some(source) = fixture() else {
            eprintln!("skipped: pdfium.dll is not available");
            return;
        };
        if FontSet::load().is_err() {
            eprintln!("skipped: Windows fonts are not available");
            return;
        }
        let analysis = analyze(&source, None).expect("analysis");
        let texts: Vec<&str> = analysis
            .blocks
            .iter()
            .map(|block| block.source_text.as_str())
            .collect();
        assert_eq!(
            texts,
            [
                "Introduction to the system",
                "The system reads every page of the book and keeps the original layout intact."
            ]
        );
        assert_eq!(analysis.blocks[0].block_type, BlockType::Heading);

        let mut blocks = analysis.blocks.clone();
        blocks[0].translated_text = Some("Введение в систему".into());
        blocks[1].translated_text =
            Some("Система читает каждую страницу книги и сохраняет исходную вёрстку.".into());
        let exported = export(&source, &blocks, None).expect("export");
        assert!(
            exported.diagnostics.is_empty(),
            "{:?}",
            exported.diagnostics
        );

        let text = engine::with_pdfium(|pdfium| {
            let document = pdfium
                .load_pdf_from_byte_slice(&exported.bytes, None)
                .map_err(|error| pdfium_error("open", error))?;
            let page = document
                .pages()
                .get(0)
                .map_err(|error| pdfium_error("page", error))?;
            // PDFium reports the embedded font's space as U+00A0.
            let text = page
                .text()
                .map_err(|error| pdfium_error("text", error))?
                .all()
                .replace('\u{a0}', " ");
            Ok(text)
        })
        .expect("read output");
        assert!(text.contains("Введение в систему"));
        assert!(text.contains("Система читает каждую страницу"));
        assert!(text.contains("client.ask(prompt=user_question)"));
        assert!(!text.contains("Introduction"));
        assert!(!text.contains("original layout"));
    }

    #[test]
    fn parses_page_selections() {
        assert_eq!(PageSelection::parse("  ").unwrap(), None);
        let selection = PageSelection::parse("5-12, 20 ,30-").unwrap().unwrap();
        assert_eq!(selection.canonical(), "5-12,20,30-");
        assert_eq!(
            selection.pages(32).unwrap(),
            (4..12).chain([19, 29, 30, 31]).collect::<Vec<_>>()
        );
        assert!(selection.pages(3).is_err());
        assert!(PageSelection::parse("0").is_err());
        assert!(PageSelection::parse("9-3").is_err());
        assert!(PageSelection::parse("a").is_err());
        assert_eq!(
            selection_from_configuration(&configuration_version(Some(&selection))).unwrap(),
            Some(selection)
        );
        assert_eq!(selection_from_configuration(PARSER_VERSION).unwrap(), None);
    }

    #[test]
    fn overflow_is_reported_for_review() {
        let Some(source) = fixture() else {
            return;
        };
        if FontSet::load().is_err() {
            return;
        }
        let mut blocks = analyze(&source, None).expect("analysis").blocks;
        blocks[1].translated_text = Some("очень длинный перевод ".repeat(80));
        let exported = export(&source, &blocks, None).expect("export");
        assert!(exported
            .diagnostics
            .iter()
            .any(|line| line.contains("review pages: 1")));
    }
}
