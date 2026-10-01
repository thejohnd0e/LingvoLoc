//! Pure page geometry for PDF translation: text fragments become lines, lines
//! become paragraphs, and translated paragraphs are re-wrapped into their
//! original regions. Nothing here touches PDFium, so it is unit-testable.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
}

impl Rect {
    pub fn width(&self) -> f32 {
        self.right - self.left
    }

    pub fn height(&self) -> f32 {
        self.top - self.bottom
    }

    pub fn contains(&self, other: &Rect) -> bool {
        const TOLERANCE: f32 = 1.0;
        other.left >= self.left - TOLERANCE
            && other.right <= self.right + TOLERANCE
            && other.bottom >= self.bottom - TOLERANCE
            && other.top <= self.top + TOLERANCE
    }

    pub fn overlaps_horizontally(&self, other: &Rect) -> bool {
        self.left < other.right && other.left < self.right
    }
}

/// One PDF text object as PDFium reports it.
#[derive(Debug, Clone)]
pub struct Fragment {
    /// Index of the object on its page, used to remove it on export.
    pub object: usize,
    pub text: String,
    pub bounds: Rect,
    pub baseline: f32,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub serif: bool,
    pub color: [u8; 4],
}

/// A filled rectangle behind the text (callout boxes, banners, code panels).
#[derive(Debug, Clone, Copy)]
pub struct Background {
    pub bounds: Rect,
    pub color: [u8; 4],
}

#[derive(Debug, Clone)]
pub struct Line {
    pub fragments: Vec<usize>,
    pub text: String,
    pub bounds: Rect,
    pub baseline: f32,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub serif: bool,
    pub color: [u8; 4],
    /// A list marker (bullet or number) stands left of this line.
    pub item_start: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Heading,
    Body,
    /// Not translated; the reason is reported as a diagnostic.
    Skip(SkipReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    Code,
    NoLetters,
}

#[derive(Debug, Clone)]
pub struct Paragraph {
    pub lines: Vec<Line>,
    pub kind: Kind,
    pub text: String,
    pub bounds: Rect,
    /// Background rectangle that contains the paragraph, if any.
    pub container: Option<Rect>,
}

pub const MAX_PANEL_HEIGHT: f32 = 400.0;
const WORD_GAP_LIMIT: f32 = 0.8;
const LINE_JOIN_PITCH: f32 = 1.45;

fn luminance(color: [u8; 4]) -> f32 {
    (0.2126 * f32::from(color[0]) + 0.7152 * f32::from(color[1]) + 0.0722 * f32::from(color[2]))
        / 255.0
}

fn saturation(color: [u8; 4]) -> u8 {
    let max = color[0].max(color[1]).max(color[2]);
    let min = color[0].min(color[1]).min(color[2]);
    max - min
}

fn same_color(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter().zip(b.iter()).all(|(x, y)| x.abs_diff(*y) <= 12)
}

fn union(a: Rect, b: Rect) -> Rect {
    Rect {
        left: a.left.min(b.left),
        bottom: a.bottom.min(b.bottom),
        right: a.right.max(b.right),
        top: a.top.max(b.top),
    }
}

fn join_fragments(fragments: &[&Fragment]) -> String {
    let mut text = String::new();
    let mut previous: Option<&Fragment> = None;
    for fragment in fragments {
        if fragment.text.trim().is_empty() {
            if let Some(last) = previous {
                if fragment.bounds.left - last.bounds.right > last.size * 0.1
                    && !text.ends_with(char::is_whitespace)
                {
                    text.push(' ');
                }
            }
            continue;
        }
        if let Some(last) = previous {
            let gap = fragment.bounds.left - last.bounds.right;
            if gap > last.size * 0.15
                && !text.ends_with(char::is_whitespace)
                && !fragment.text.starts_with(char::is_whitespace)
            {
                text.push(' ');
            }
        }
        text.push_str(&fragment.text);
        previous = Some(fragment);
    }
    let cleaned = text.split_whitespace().collect::<Vec<_>>().join(" ");
    cleaned
}

/// Groups fragments into visual lines, splitting a baseline group where the
/// horizontal gap is too large to be a word space (label columns, tables).
pub fn build_lines(fragments: &[Fragment]) -> Vec<Line> {
    let markers: Vec<usize> = (0..fragments.len())
        .filter(|&index| is_marker(&fragments[index].text))
        .collect();
    let mut order: Vec<usize> = (0..fragments.len())
        .filter(|&index| {
            (fragments[index].bounds.width() > 0.0 || !fragments[index].text.is_empty())
                && !markers.contains(&index)
        })
        .collect();
    order.sort_by(|&a, &b| {
        fragments[b]
            .baseline
            .partial_cmp(&fragments[a].baseline)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut groups: Vec<Vec<usize>> = Vec::new();
    for index in order {
        let fragment = &fragments[index];
        let joined = groups.last_mut().filter(|group| {
            let first = &fragments[group[0]];
            (first.baseline - fragment.baseline).abs() <= 0.3 * first.size.max(fragment.size)
        });
        match joined {
            Some(group) => group.push(index),
            None => groups.push(vec![index]),
        }
    }

    let mut lines = Vec::new();
    for mut group in groups {
        group.sort_by(|&a, &b| {
            fragments[a]
                .bounds
                .left
                .partial_cmp(&fragments[b].bounds.left)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut segment: Vec<usize> = Vec::new();
        let mut right = f32::MIN;
        for index in group {
            let fragment = &fragments[index];
            if !segment.is_empty() && fragment.bounds.left - right > WORD_GAP_LIMIT * fragment.size
            {
                lines.extend(make_line(fragments, &segment));
                segment.clear();
            }
            right = right.max(fragment.bounds.right);
            segment.push(index);
        }
        lines.extend(make_line(fragments, &segment));
    }
    for line in &mut lines {
        line.item_start = markers.iter().any(|&index| {
            let marker = &fragments[index];
            (marker.baseline - line.baseline).abs() <= 0.3 * line.size
                && marker.bounds.right <= line.bounds.left + 1.0
                && line.bounds.left - marker.bounds.right < 2.5 * line.size
        });
    }
    lines
}

/// A lone bullet or list number; it stays on the page untouched.
pub fn is_marker(text: &str) -> bool {
    let text = text.trim();
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some('•' | '·' | '▪' | '●' | '◦' | '■' | '○' | '▸' | '►'), None) => true,
        _ => {
            let digits = text.trim_end_matches(['.', ')']);
            digits.len() < text.len()
                && !digits.is_empty()
                && digits.len() <= 3
                && digits.chars().all(|c| c.is_ascii_digit())
        }
    }
}

fn make_line(fragments: &[Fragment], indexes: &[usize]) -> Option<Line> {
    let members: Vec<&Fragment> = indexes.iter().map(|&index| &fragments[index]).collect();
    let text = join_fragments(&members);
    if text.is_empty() {
        return None;
    }
    let visible: Vec<&&Fragment> = members
        .iter()
        .filter(|fragment| !fragment.text.trim().is_empty())
        .collect();
    let dominant = visible
        .iter()
        .max_by_key(|fragment| fragment.text.chars().count())?;
    let bounds = visible
        .iter()
        .map(|fragment| fragment.bounds)
        .reduce(union)?;
    Some(Line {
        fragments: indexes.to_vec(),
        text,
        bounds,
        baseline: dominant.baseline,
        size: dominant.size,
        bold: dominant.bold,
        italic: dominant.italic,
        serif: dominant.serif,
        color: dominant.color,
        item_start: false,
    })
}

fn looks_like_code(text: &str) -> bool {
    let trimmed = text.trim_start();
    const STARTS: [&str; 12] = [
        "#",
        "//",
        "def ",
        "import ",
        "from ",
        "class ",
        "return ",
        "$ ",
        "> ",
        "async def ",
        "const ",
        "npm ",
    ];
    if STARTS.iter().any(|start| trimmed.starts_with(start))
        && trimmed.chars().filter(|c| c.is_alphabetic()).count() > 0
        && (trimmed.starts_with('#') || trimmed.contains(['(', '=', ':', '.']))
    {
        return true;
    }
    let symbols = text.chars().filter(|c| "{}[]=<>;|\\_".contains(*c)).count();
    let letters = text.chars().filter(|c| c.is_alphabetic()).count().max(1);
    symbols >= 2 && symbols * 5 >= letters
        || text.contains("==")
        || text.contains("->")
        || text.contains("=>")
        || text.contains("()")
        || has_call(text)
        || text.split_whitespace().any(is_snake_case)
        || text.contains("::")
}

/// `name(` with no space before the parenthesis, closed later: a call.
fn has_call(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    chars.windows(2).enumerate().any(|(index, pair)| {
        (pair[0].is_alphanumeric() || pair[0] == '_')
            && pair[1] == '('
            && chars[index + 2..].contains(&')')
    })
}

fn is_snake_case(word: &str) -> bool {
    let word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
    word.contains('_')
        && word
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && word.chars().any(|c| c.is_ascii_lowercase())
}

fn containing_background(bounds: &Rect, backgrounds: &[Background]) -> Option<Background> {
    backgrounds
        .iter()
        .filter(|background| {
            background.bounds.contains(bounds)
                && background.bounds.height() > bounds.height()
                // A page-sized fill is the paper, not a panel.
                && background.bounds.height() <= MAX_PANEL_HEIGHT
        })
        .min_by(|a, b| {
            (a.bounds.width() * a.bounds.height())
                .partial_cmp(&(b.bounds.width() * b.bounds.height()))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .copied()
}

/// Groups lines into paragraphs and classifies each of them.
pub fn build_paragraphs(
    lines: Vec<Line>,
    backgrounds: &[Background],
    body_size: f32,
) -> Vec<Paragraph> {
    let mut ordered = lines;
    ordered.sort_by(|a, b| {
        b.baseline
            .partial_cmp(&a.baseline)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                a.bounds
                    .left
                    .partial_cmp(&b.bounds.left)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });

    let mut groups: Vec<Vec<Line>> = Vec::new();
    for line in ordered {
        // Label columns (a "HOOK" tag beside a text) interleave with the
        // paragraph next to them, so look at a few recent groups, not only the last.
        let target = groups.iter().rposition(|group| continues(group, &line));
        match target {
            Some(index) if groups.len() - index <= 6 => groups[index].push(line),
            _ => groups.push(vec![line]),
        }
    }

    groups
        .into_iter()
        .map(|lines| finish_paragraph(lines, backgrounds, body_size))
        .collect()
}

fn continues(group: &[Line], line: &Line) -> bool {
    let previous = group.last().expect("group is never empty");
    let pitch = previous.baseline - line.baseline;
    let first_left = group[0].bounds.left;
    (previous.size - line.size).abs() <= 0.6
        && previous.bold == line.bold
        && same_color(previous.color, line.color)
        && pitch > 0.6 * line.size
        && pitch <= LINE_JOIN_PITCH * line.size
        && (line.bounds.left - first_left).abs() <= 2.5 * line.size
        && line.bounds.left < previous.bounds.right
        && !line.item_start
        // A stray one- or two-letter line far to the left is a label, not a continuation.
        && !(line.text.chars().count() <= 2
            && (line.bounds.left - first_left).abs() > 0.5 * line.size)
        && !starts_list_item(&line.text)
        && !ends_sentence_at_margin(previous, group)
}

fn starts_list_item(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some('•' | '·' | '▪' | '–' | '-' | '*') => chars.next() == Some(' '),
        Some(first) if first.is_ascii_digit() => {
            let rest = text.trim_start_matches(|c: char| c.is_ascii_digit());
            (rest.starts_with(". ") || rest.starts_with(") ")) && text.len() - rest.len() <= 3
        }
        _ => false,
    }
}

/// A line that is much shorter than its siblings and ends a sentence ends the
/// paragraph even when the next line is close.
fn ends_sentence_at_margin(previous: &Line, group: &[Line]) -> bool {
    if group.len() < 2 {
        return false;
    }
    let widest = group
        .iter()
        .map(|line| line.bounds.width())
        .fold(0.0_f32, f32::max);
    previous.bounds.width() < widest * 0.45 && previous.text.ends_with(['.', '!', '?', ':'])
}

fn finish_paragraph(lines: Vec<Line>, backgrounds: &[Background], body_size: f32) -> Paragraph {
    let bounds = lines
        .iter()
        .map(|line| line.bounds)
        .reduce(union)
        .expect("paragraph has lines");
    let text = lines
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let background = containing_background(&bounds, backgrounds);
    let letters = text.chars().filter(|c| c.is_alphabetic()).count();
    let color = lines[0].color;

    let code_lines = lines
        .iter()
        .filter(|line| looks_like_code(&line.text))
        .count();
    let terminal_style = background.is_some_and(|b| {
        luminance(b.color) < 0.25 && saturation(color) > 80 && luminance(color) < 0.85
    });
    let short_label = letters <= 12
        && text
            .chars()
            .filter(|c| c.is_alphabetic())
            .all(char::is_uppercase);
    let kind = if letters < 3 || short_label {
        Kind::Skip(SkipReason::NoLetters)
    } else if terminal_style || code_lines * 2 >= lines.len() && code_lines > 0 {
        Kind::Skip(SkipReason::Code)
    } else if lines[0].size >= body_size * 1.2
        || (lines[0].bold && lines.len() <= 2 && lines[0].size >= body_size)
    {
        Kind::Heading
    } else {
        Kind::Body
    };

    Paragraph {
        lines,
        kind,
        text,
        bounds,
        container: background.map(|b| b.bounds),
    }
}

/// The most common font size, weighted by character count.
pub fn body_font_size(fragments: &[Fragment]) -> f32 {
    let mut weights: Vec<(i32, usize)> = Vec::new();
    for fragment in fragments {
        let key = (fragment.size * 2.0).round() as i32;
        let weight = fragment.text.chars().count();
        match weights.iter_mut().find(|(size, _)| *size == key) {
            Some(entry) => entry.1 += weight,
            None => weights.push((key, weight)),
        }
    }
    weights
        .into_iter()
        .max_by_key(|(_, weight)| *weight)
        .map(|(key, _)| key as f32 / 2.0)
        .unwrap_or(12.0)
}

// --- Re-wrapping translated text -------------------------------------------

pub trait Measure {
    /// Width of `text` in points at `size`.
    fn width(&self, text: &str, size: f32, bold: bool, italic: bool) -> f32;
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlacedLine {
    pub text: String,
    pub x: f32,
    pub baseline: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub lines: Vec<PlacedLine>,
    pub size: f32,
    /// Font size relative to the original.
    pub scale: f32,
    /// True when even the smallest allowed size did not fit; the text is then
    /// placed at the smallest size and the block is reported for review.
    pub overflow: bool,
}

impl Placement {
    /// Overflowing or shrunk so far that a person should look at the page.
    pub fn needs_review(&self) -> bool {
        self.overflow || self.scale < REVIEW_SCALE
    }
}

pub const MIN_SCALE: f32 = 0.55;
pub const REVIEW_SCALE: f32 = 0.68;
const SCALE_STEPS: [f32; 12] = [
    1.0, 0.94, 0.89, 0.85, 0.81, 0.78, 0.74, 0.70, 0.66, 0.62, 0.58, MIN_SCALE,
];

pub fn wrap(
    text: &str,
    width: f32,
    size: f32,
    bold: bool,
    italic: bool,
    measure: &dyn Measure,
) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };
        if current.is_empty() || measure.width(&candidate, size, bold, italic) <= width {
            current = candidate;
        } else {
            lines.push(std::mem::take(&mut current));
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// Chooses a font size, spacing and line breaks so `text` fits its region.
///
/// Tighter line spacing is tried before a smaller font. `first_baseline` is the
/// original first baseline, `pitch` the line pitch at full size, `limit_bottom`
/// the lowest y the text may reach, `shift_room` how far the first baseline may
/// move up so a taller block can be centred inside its panel, and `left_slack`
/// how far the region may grow to the left when a single word is too wide. A
/// translation that needs no more lines than the original never counts as
/// overflowing vertically: the original already occupied that space.
#[allow(clippy::too_many_arguments)]
pub fn place(
    text: &str,
    region_left: f32,
    region_width: f32,
    first_baseline: f32,
    limit_bottom: f32,
    size: f32,
    pitch: f32,
    bold: bool,
    italic: bool,
    max_scale: f32,
    shift_room: f32,
    left_slack: f32,
    original_lines: usize,
    measure: &dyn Measure,
) -> Placement {
    const TIGHTEN: [f32; 3] = [1.0, 0.92, 0.85];
    let mut best: Option<Placement> = None;
    for scale in SCALE_STEPS.into_iter().filter(|&scale| scale <= max_scale) {
        let scaled = size * scale;
        let widest_word = text
            .split_whitespace()
            .map(|word| measure.width(word, scaled, bold, italic))
            .fold(0.0_f32, f32::max);
        let grow = (widest_word - region_width).max(0.0).min(left_slack);
        let (left, width) = (region_left - grow, region_width + grow);
        let wrapped = wrap(text, width, scaled, bold, italic, measure);
        for tighten in TIGHTEN {
            let scaled_pitch = (pitch * scale * tighten).max(scaled * 1.08);
            let extra = wrapped.len().saturating_sub(original_lines) as f32 * scaled_pitch;
            // Centre the taller block first, then use all the room above.
            for shift in [extra * 0.5, extra] {
                let shift = shift.min(shift_room).max(0.0);
                let lines: Vec<PlacedLine> = wrapped
                    .iter()
                    .enumerate()
                    .map(|(index, text)| PlacedLine {
                        text: text.clone(),
                        x: left,
                        baseline: first_baseline + shift - scaled_pitch * index as f32,
                    })
                    .collect();
                let fits = widest_word <= width
                    && (wrapped.len() <= original_lines
                        || lines
                            .last()
                            .is_none_or(|last| last.baseline - scaled * 0.3 >= limit_bottom));
                let placement = Placement {
                    lines,
                    size: scaled,
                    scale,
                    overflow: !fits,
                };
                if fits {
                    return placement;
                }
                best = Some(placement);
            }
        }
    }
    best.expect("max_scale allows at least one step")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed;

    impl Measure for Fixed {
        fn width(&self, text: &str, size: f32, _bold: bool, _italic: bool) -> f32 {
            text.chars().count() as f32 * size * 0.5
        }
    }

    fn fragment(text: &str, left: f32, baseline: f32, size: f32) -> Fragment {
        let width = text.chars().count() as f32 * size * 0.5;
        Fragment {
            object: 0,
            text: text.into(),
            bounds: Rect {
                left,
                bottom: baseline - size * 0.2,
                right: left + width,
                top: baseline + size * 0.8,
            },
            baseline,
            size,
            bold: false,
            italic: false,
            serif: true,
            color: [0, 0, 0, 255],
        }
    }

    #[test]
    fn glyph_fragments_join_into_one_line() {
        let fragments = vec![
            fragment("W", 100.0, 500.0, 20.0),
            fragment("e ", 110.0, 500.0, 20.0),
            fragment("go", 130.0, 500.0, 20.0),
        ];
        let lines = build_lines(&fragments);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "We go");
    }

    #[test]
    fn large_gap_splits_a_line_into_columns() {
        let fragments = vec![
            fragment("HO", 87.0, 590.0, 13.0),
            fragment("Imagine text", 126.0, 590.0, 13.0),
        ];
        assert_eq!(build_lines(&fragments).len(), 2);
    }

    #[test]
    fn close_lines_form_a_paragraph_and_blank_gap_splits() {
        let fragments = vec![
            fragment("First line of the text is here", 72.0, 700.0, 12.0),
            fragment("second line continues the text", 72.0, 685.0, 12.0),
            fragment("New paragraph starts far below", 72.0, 660.0, 12.0),
        ];
        let paragraphs = build_paragraphs(build_lines(&fragments), &[], 12.0);
        assert_eq!(paragraphs.len(), 2);
        assert_eq!(paragraphs[0].lines.len(), 2);
        assert_eq!(paragraphs[0].kind, Kind::Body);
    }

    #[test]
    fn numbers_and_code_are_skipped() {
        let fragments = vec![
            fragment("42", 300.0, 40.0, 10.0),
            fragment("client.ask(prompt=user_question)", 72.0, 600.0, 12.0),
        ];
        let paragraphs = build_paragraphs(build_lines(&fragments), &[], 12.0);
        assert!(paragraphs
            .iter()
            .all(|paragraph| matches!(paragraph.kind, Kind::Skip(_))));
    }

    #[test]
    fn bigger_text_is_a_heading() {
        let fragments = vec![fragment("What is Claude Code?", 72.0, 700.0, 20.0)];
        let paragraphs = build_paragraphs(build_lines(&fragments), &[], 12.0);
        assert_eq!(paragraphs[0].kind, Kind::Heading);
    }

    #[test]
    fn green_text_on_dark_panel_is_code() {
        let mut text = fragment("Before answering, think through", 80.0, 600.0, 12.0);
        text.color = [0, 255, 136, 255];
        let panel = Background {
            bounds: Rect {
                left: 70.0,
                bottom: 560.0,
                right: 500.0,
                top: 620.0,
            },
            color: [30, 30, 46, 255],
        };
        let paragraphs = build_paragraphs(build_lines(&[text]), &[panel], 12.0);
        assert_eq!(paragraphs[0].kind, Kind::Skip(SkipReason::Code));
    }

    #[test]
    fn white_heading_on_dark_banner_is_translated() {
        let mut text = fragment("Welcome to Claude Code", 80.0, 600.0, 22.0);
        text.color = [255, 255, 255, 255];
        let banner = Background {
            bounds: Rect {
                left: 70.0,
                bottom: 560.0,
                right: 500.0,
                top: 640.0,
            },
            color: [26, 26, 46, 255],
        };
        let paragraphs = build_paragraphs(build_lines(&[text]), &[banner], 12.0);
        assert_eq!(paragraphs[0].kind, Kind::Heading);
        assert!(paragraphs[0].container.is_some());
    }

    #[test]
    fn wrap_breaks_at_width() {
        let lines = wrap("aaaa bbbb cccc", 45.0, 10.0, false, false, &Fixed);
        assert_eq!(lines, vec!["aaaa bbbb", "cccc"]);
    }

    #[test]
    fn place_shrinks_before_overflowing() {
        let text = "слово ".repeat(30);
        let roomy = place(
            &text, 72.0, 200.0, 700.0, 0.0, 12.0, 15.0, false, false, 1.0, 0.0, 0.0, 3, &Fixed,
        );
        assert!(!roomy.overflow);
        assert_eq!(roomy.size, 12.0);

        let tight = place(
            &text, 72.0, 200.0, 700.0, 640.0, 12.0, 15.0, false, false, 1.0, 0.0, 0.0, 3, &Fixed,
        );
        assert!(tight.size < 12.0);

        let hopeless = place(
            &text, 72.0, 200.0, 700.0, 690.0, 12.0, 15.0, false, false, 1.0, 0.0, 0.0, 1, &Fixed,
        );
        assert!(hopeless.overflow);
        assert!((hopeless.size - 12.0 * MIN_SCALE).abs() < 0.01);

        let capped = place(
            &"слово ".repeat(3),
            72.0,
            200.0,
            700.0,
            0.0,
            12.0,
            15.0,
            false,
            false,
            0.85,
            0.0,
            0.0,
            3,
            &Fixed,
        );
        assert!(capped.scale <= 0.85);
    }
}
