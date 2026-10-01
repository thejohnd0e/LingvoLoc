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
mod reflow;
mod regions;
mod slots;

use pdfium_render::prelude::*;

use crate::documents::{BlockType, DocumentBlock};
use crate::domain::RuntimeError;

use fonts::FontSet;
use layout::{Background, Fragment, Kind, Paragraph, Rect};
use regions::PageRegions;

pub const PARSER_VERSION: &str = "pdf-v2";

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

#[derive(Debug, Clone)]
struct PageModel {
    page_height: f32,
    fragments: Vec<Fragment>,
    paragraphs: Vec<Paragraph>,
    regions: PageRegions,
    supported: bool,
}

#[derive(Debug, Clone)]
struct LogicalSegment {
    page: usize,
    paragraph: usize,
    source_length: usize,
}

#[derive(Debug, Clone)]
struct LogicalBlock {
    id: String,
    block_type: BlockType,
    source_text: String,
    segments: Vec<LogicalSegment>,
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
    let mut vertical_rules = Vec::new();
    let mut images = Vec::new();
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
        } else if object.as_image_object().is_some() {
            images.push(bounds);
        } else if let Some(path) = object.as_path_object() {
            if bounds.height() <= 3.0 && bounds.width() >= 20.0 {
                rules.push(bounds);
            }
            if bounds.width() <= 3.0 && bounds.height() >= 12.0 {
                vertical_rules.push(bounds);
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
    let page_bounds = Rect {
        left: 0.0,
        bottom: 0.0,
        right: width,
        top: page.height().value,
    };
    let mut regions =
        regions::detect_regions(&paragraphs, &backgrounds, &rules, &images, page_bounds);
    // Panels are often drawn as one strip per line; for placement they are one.
    let mut obstacles = merged_backgrounds(&backgrounds);
    obstacles.extend(rules.iter().copied());
    obstacles.extend(images.iter().copied());
    obstacles.extend(vertical_rules);
    regions.obstacles = obstacles;
    PageModel {
        page_height: page.height().value,
        fragments,
        paragraphs,
        regions,
        supported: !rotated,
    }
}

/// Joins vertically touching fills of the same colour and width.
fn merged_backgrounds(backgrounds: &[Background]) -> Vec<Rect> {
    let mut sorted: Vec<Background> = backgrounds.to_vec();
    sorted.sort_by(|a, b| {
        b.bounds
            .top
            .partial_cmp(&a.bounds.top)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut merged: Vec<Background> = Vec::new();
    for background in sorted {
        let joined = merged.iter_mut().find(|existing| {
            existing.color == background.color
                && (existing.bounds.left - background.bounds.left).abs() <= 1.0
                && (existing.bounds.right - background.bounds.right).abs() <= 1.0
                && (existing.bounds.bottom - background.bounds.top).abs() <= 1.5
        });
        match joined {
            Some(existing) => existing.bounds.bottom = background.bounds.bottom,
            None => merged.push(background),
        }
    }
    merged
        .into_iter()
        .map(|background| background.bounds)
        .collect()
}

fn block_id(page: usize, index: usize) -> String {
    format!("pdf#p{:04}-{:03}", page + 1, index)
}

fn translatable(paragraph: &Paragraph) -> bool {
    matches!(paragraph.kind, Kind::Heading | Kind::Body)
}

fn ordered_translatable(paragraphs: &[Paragraph], regions: &PageRegions) -> Vec<usize> {
    regions
        .reading_order
        .iter()
        .copied()
        .filter(|&index| translatable(&paragraphs[index]))
        .collect()
}

fn lane_for(regions: &PageRegions, paragraph: usize) -> usize {
    regions
        .flows
        .iter()
        .find(|flow| flow.paragraph_indices.contains(&paragraph))
        .map(|flow| flow.id.lane)
        .unwrap_or(0)
}

fn block_type_for(regions: &PageRegions, paragraph: usize, kind: Kind) -> BlockType {
    if kind == Kind::Heading {
        return BlockType::Heading;
    }
    match regions
        .flows
        .iter()
        .find(|flow| flow.paragraph_indices.contains(&paragraph))
        .map(|flow| flow.kind)
    {
        Some(regions::RegionKind::Caption) => BlockType::Caption,
        Some(regions::RegionKind::TableCell) => BlockType::TableCell,
        _ => BlockType::Paragraph,
    }
}

fn collect_logical_blocks(models: &[(usize, PageModel)]) -> Vec<LogicalBlock> {
    let mut blocks: Vec<LogicalBlock> = Vec::new();
    let mut previous: Option<(usize, Paragraph, usize, f32)> = None;
    for &(number, ref model) in models {
        if !model.supported || model.fragments.iter().all(|f| f.text.trim().is_empty()) {
            previous = None;
            continue;
        }
        for (index, paragraph_index) in ordered_translatable(&model.paragraphs, &model.regions)
            .into_iter()
            .enumerate()
        {
            let paragraph = &model.paragraphs[paragraph_index];
            let lane = lane_for(&model.regions, paragraph_index);
            let joined = index == 0
                && previous.as_ref().is_some_and(
                    |(page, previous, previous_lane, previous_height)| {
                        *page + 1 == number
                            && regions::can_join_cross_page(
                                previous,
                                paragraph,
                                *previous_lane,
                                lane,
                                *previous_height,
                                model.page_height,
                            )
                    },
                );
            if joined {
                if let Some(previous_block) = blocks.last_mut() {
                    previous_block.source_text =
                        regions::join_cross_page_text(&previous_block.source_text, &paragraph.text);
                    previous_block.segments.push(LogicalSegment {
                        page: number,
                        paragraph: paragraph_index,
                        source_length: paragraph.text.chars().count(),
                    });
                }
            } else {
                blocks.push(LogicalBlock {
                    id: block_id(number, index),
                    block_type: block_type_for(&model.regions, paragraph_index, paragraph.kind),
                    source_text: paragraph.text.clone(),
                    segments: vec![LogicalSegment {
                        page: number,
                        paragraph: paragraph_index,
                        source_length: paragraph.text.chars().count(),
                    }],
                });
            }
            previous = Some((number, paragraph.clone(), lane, model.page_height));
        }
    }
    blocks
}

fn split_translation(text: &str, source_lengths: &[usize]) -> Vec<String> {
    if source_lengths.len() <= 1 {
        return vec![text.to_string()];
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    let total_source = source_lengths.iter().sum::<usize>().max(1);
    let mut remaining = words.as_slice();
    let mut result = Vec::with_capacity(source_lengths.len());
    for (index, source_length) in source_lengths.iter().enumerate() {
        if index + 1 == source_lengths.len() {
            result.push(remaining.join(" "));
            break;
        }
        let target = ((words.len() * *source_length) as f32 / total_source as f32).round() as usize;
        let take = target.clamp(
            1,
            remaining
                .len()
                .saturating_sub(source_lengths.len() - index - 1),
        );
        let (part, rest) = remaining.split_at(take);
        result.push(part.join(" "));
        remaining = rest;
    }
    result
}

fn segment_translations(
    logical_blocks: &[LogicalBlock],
    blocks: &[DocumentBlock],
) -> std::collections::HashMap<(usize, usize), (String, String)> {
    let translations: std::collections::HashMap<&str, &str> = blocks
        .iter()
        .filter_map(|block| {
            block
                .translated_text
                .as_deref()
                .map(|text| (block.id.as_str(), text))
        })
        .collect();
    let mut result = std::collections::HashMap::new();
    for logical in logical_blocks {
        let Some(translation) = translations.get(logical.id.as_str()) else {
            continue;
        };
        let lengths: Vec<usize> = logical
            .segments
            .iter()
            .map(|segment| segment.source_length)
            .collect();
        for (segment, text) in logical
            .segments
            .iter()
            .zip(split_translation(translation, &lengths))
        {
            result.insert(
                (segment.page, segment.paragraph),
                (logical.id.clone(), text),
            );
        }
    }
    result
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
    let mut diagnostics = Vec::new();
    let mut rotated = Vec::new();
    let mut without_text = Vec::new();
    let mut models = Vec::new();
    for &number in &selected {
        let page = pages
            .get(number as PdfPageIndex)
            .map_err(|error| pdfium_error("read PDF page", error))?;
        let model = read_page(&page);
        diagnostics.extend(model.regions.diagnostics.iter().cloned());
        if !model.supported {
            rotated.push(number + 1);
        } else if model.fragments.iter().all(|f| f.text.trim().is_empty()) {
            without_text.push(number + 1);
        }
        models.push((number, model));
    }
    let logical_blocks = collect_logical_blocks(&models);
    let blocks: Vec<DocumentBlock> = logical_blocks
        .iter()
        .enumerate()
        .map(|(ordinal, block)| DocumentBlock {
            id: block.id.clone(),
            ordinal: ordinal as i64,
            block_type: block.block_type,
            source_text: block.source_text.clone(),
            translated_text: None,
        })
        .collect();
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
    let mut models = Vec::new();
    for &number in &selected {
        let page = document
            .pages()
            .get(number as PdfPageIndex)
            .map_err(|error| pdfium_error("read PDF page", error))?;
        models.push((number, read_page(&page)));
    }
    let logical_blocks = collect_logical_blocks(&models);
    let segment_texts = segment_translations(&logical_blocks, blocks);

    for &number in &selected {
        let mut page = document
            .pages()
            .get(number as PdfPageIndex)
            .map_err(|error| pdfium_error("read PDF page", error))?;
        let model = models
            .iter()
            .find(|(page_number, _)| *page_number == number)
            .map(|(_, model)| model)
            .expect("selected page model exists");
        if !model.supported {
            continue;
        }
        // Regenerating the content stream after every object is very slow.
        page.set_content_regeneration_strategy(PdfPageContentRegenerationStrategy::Manual);
        struct Job {
            objects: Vec<usize>,
            lines: Vec<reflow::PlannedLine>,
            serif: bool,
            bold: bool,
            italic: bool,
            color: [u8; 4],
            /// Source object, text, x, baseline: list markers that follow their item.
            markers: Vec<(usize, String, f32, f32)>,
            marker_size: f32,
        }
        let mut jobs = Vec::new();
        let active: Vec<usize> = (0..model.paragraphs.len())
            .filter(|&position| {
                translatable(&model.paragraphs[position])
                    && segment_texts
                        .get(&(number, position))
                        .is_some_and(|(_, text)| !text.trim().is_empty())
            })
            .collect();
        /// Source objects, font traits and list markers of one translated block.
        type PendingBlock = (
            String,
            Vec<usize>,
            bool,
            bool,
            bool,
            [u8; 4],
            Vec<(usize, String, f32, f32)>,
            f32,
        );
        struct PendingRun {
            items: Vec<reflow::FlowItem>,
            frame: reflow::FlowFrame,
            blocks: Vec<PendingBlock>,
        }
        let mut pending: Vec<PendingRun> = Vec::new();
        for run in slots::runs(&model.paragraphs, &model.regions.obstacles, &active) {
            let mut items = Vec::new();
            let mut objects_by_block = Vec::new();
            for (slot_index, slot) in run.iter().enumerate() {
                let paragraph = &model.paragraphs[slot.paragraph];
                let (id, translation) = &segment_texts[&(number, slot.paragraph)];
                let first = &paragraph.lines[0];
                let pitch = if paragraph.lines.len() > 1 {
                    (first.baseline - paragraph.lines[paragraph.lines.len() - 1].baseline)
                        / (paragraph.lines.len() - 1) as f32
                } else {
                    first.size * 1.2
                };
                let gap_after = run
                    .get(slot_index + 1)
                    .map(|next| {
                        (paragraph.bounds.bottom - model.paragraphs[next.paragraph].bounds.top)
                            .max(0.0)
                    })
                    .unwrap_or(0.0);
                let objects: Vec<usize> = paragraph
                    .lines
                    .iter()
                    .flat_map(|line| line.fragments.iter())
                    .map(|&fragment| model.fragments[fragment].object)
                    .collect();
                let markers: Vec<(usize, String, f32, f32)> = if first.item_start {
                    model
                        .fragments
                        .iter()
                        .filter(|fragment| {
                            layout::is_marker(&fragment.text)
                                && (fragment.baseline - first.baseline).abs() <= 0.3 * first.size
                                && fragment.bounds.right <= first.bounds.left + 1.0
                                && first.bounds.left - fragment.bounds.right < 2.5 * first.size
                        })
                        .map(|fragment| {
                            (
                                fragment.object,
                                fragment.text.trim().to_string(),
                                fragment.bounds.left,
                                fragment.baseline,
                            )
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                objects_by_block.push((
                    id.clone(),
                    objects,
                    first.serif,
                    first.bold,
                    first.italic,
                    first.color,
                    markers,
                    first.size,
                ));
                items.push(reflow::FlowItem {
                    block_id: id.clone(),
                    text: translation.clone(),
                    block_type: if paragraph.kind == Kind::Heading {
                        BlockType::Heading
                    } else {
                        BlockType::Paragraph
                    },
                    style: reflow::TextStyle {
                        size: first.size,
                        pitch,
                        bold: first.bold,
                        italic: first.italic,
                        serif: first.serif,
                        color: first.color,
                    },
                    original_bounds: paragraph.bounds,
                    original_lines: paragraph.lines.len(),
                    preferred_gap_after: gap_after,
                    left: slot.left,
                    right: slot.right,
                    first_baseline: first.baseline,
                    center: slot.center,
                    right_aligned: slot.right_aligned,
                });
            }
            let frame = reflow::FlowFrame {
                page: number,
                bounds: Rect {
                    left: run[0].left,
                    bottom: run[0].bottom,
                    right: run[0].right,
                    top: run[0].top,
                },
            };
            pending.push(PendingRun {
                items,
                frame,
                blocks: objects_by_block,
            });
        }
        let mut plans: Vec<reflow::FlowPlan> = pending
            .iter()
            .map(|run| {
                reflow::place_flow(
                    &run.items,
                    &[run.frame],
                    &fonts.measure(run.items[0].style.serif),
                    1.0,
                )
            })
            .collect();
        // Cells and rows of one table or list share a font size when they can.
        let mut groups: std::collections::HashMap<(i32, bool, bool, bool), Vec<usize>> =
            std::collections::HashMap::new();
        for (index, run) in pending.iter().enumerate() {
            let style = run.items[0].style;
            if run.items.len() == 1 && run.frame.bounds.height() <= style.size * 8.0 {
                groups
                    .entry((
                        (style.size * 2.0).round() as i32,
                        style.bold,
                        style.italic,
                        style.serif,
                    ))
                    .or_default()
                    .push(index);
            }
        }
        for members in groups.values().filter(|members| members.len() >= 3) {
            let shared = members
                .iter()
                .map(|&index| plans[index].scale)
                .fold(1.0_f32, f32::min);
            if shared < 0.6 || shared >= 1.0 {
                continue;
            }
            for &index in members {
                if plans[index].scale > shared + 1e-4 {
                    let run = &pending[index];
                    plans[index] = reflow::place_flow(
                        &run.items,
                        &[run.frame],
                        &fonts.measure(run.items[0].style.serif),
                        shared,
                    );
                }
            }
        }
        for (run, plan) in pending.into_iter().zip(plans) {
            if plan.scale < layout::REVIEW_SCALE || !plan.overflow_blocks.is_empty() {
                review_pages.push(number + 1);
            }
            for (id, mut objects, serif, bold, italic, color, markers, marker_size) in run.blocks {
                let lines: Vec<reflow::PlannedLine> = plan
                    .lines
                    .iter()
                    .filter(|line| line.block_id == id)
                    .cloned()
                    .collect();
                let first_baseline = lines
                    .first()
                    .map(|line: &reflow::PlannedLine| line.baseline);
                let markers = match first_baseline {
                    Some(baseline) => markers
                        .into_iter()
                        .filter(|marker| (marker.3 - baseline).abs() > 0.5)
                        .map(|marker| (marker.0, marker.1, marker.2, baseline))
                        .collect(),
                    None => Vec::new(),
                };
                objects.extend(
                    markers
                        .iter()
                        .map(|marker: &(usize, String, f32, f32)| marker.0),
                );
                jobs.push(Job {
                    objects,
                    lines,
                    serif,
                    bold,
                    italic,
                    color,
                    markers,
                    marker_size,
                });
            }
        }
        if jobs.is_empty() {
            continue;
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
            let marker_lines = job
                .markers
                .iter()
                .map(|(_, text, x, baseline)| (text.as_str(), *x, *baseline, job.marker_size));
            let text_lines = job
                .lines
                .iter()
                .map(|line| (line.text.as_str(), line.x, line.baseline, line.size));
            for (line_text, x, baseline, size) in marker_lines.chain(text_lines) {
                let mut text =
                    PdfPageTextObject::new(&document, line_text, token, PdfPoints::new(size))
                        .map_err(|error| pdfium_error("create translated text", error))?;
                text.set_fill_color(PdfColor::new(
                    job.color[0],
                    job.color[1],
                    job.color[2],
                    job.color[3],
                ))
                .map_err(|error| pdfium_error("colour translated text", error))?;
                text.translate(PdfPoints::new(x), PdfPoints::new(baseline))
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
            eprintln!("skipped: bundled fonts are not available");
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
            // PDFium's CID TrueType path currently reports embedded spaces as U+00A0.
            let page_text = page.text().map_err(|error| pdfium_error("text", error))?;
            let chars: Vec<u32> = page_text
                .chars()
                .iter()
                .map(|character| character.unicode_value())
                .collect();
            let text: String = chars
                .iter()
                .filter_map(|value| char::from_u32(*value))
                .collect();
            assert!(text.contains(' '));
            assert!(!text.contains('\u{a0}'));
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
    fn split_translation_keeps_all_words_across_joined_segments() {
        let parts = split_translation("one two three four five six", &[2, 4]);
        assert_eq!(parts, vec!["one two", "three four five six"]);
        assert_eq!(parts.join(" "), "one two three four five six");
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

    #[test]
    fn authored_fixture_has_stable_complex_region_analysis() {
        let source = include_bytes!("../../../../../../docs/fixtures/pdf/technical-fixture.pdf");
        let first = match analyze(source, None) {
            Ok(analysis) => analysis,
            Err(error) if error.to_string().contains("PDFium") => {
                eprintln!("skipped: pdfium.dll is not available: {error}");
                return;
            }
            Err(error) => panic!("fixture analysis failed: {error}"),
        };
        let second = analyze(source, None).expect("repeat fixture analysis");
        let first_blocks: Vec<_> = first
            .blocks
            .iter()
            .map(|block| {
                (
                    block.id.clone(),
                    block.source_text.clone(),
                    block.block_type,
                )
            })
            .collect();
        let second_blocks: Vec<_> = second
            .blocks
            .iter()
            .map(|block| {
                (
                    block.id.clone(),
                    block.source_text.clone(),
                    block.block_type,
                )
            })
            .collect();
        assert_eq!(first_blocks, second_blocks);
        assert!(first_blocks.len() > 10);
    }
}
