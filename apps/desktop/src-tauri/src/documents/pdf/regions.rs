use super::layout::{Background, Kind, Paragraph, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    Body,
    Panel,
    TableCell,
    Caption,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FlowId {
    pub section: usize,
    pub lane: usize,
}

#[derive(Debug, Clone)]
pub struct FlowRegion {
    pub id: FlowId,
    pub kind: RegionKind,
    #[allow(dead_code)]
    pub bounds: Rect,
    pub paragraph_indices: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct PageRegions {
    pub flows: Vec<FlowRegion>,
    pub reading_order: Vec<usize>,
    pub obstacles: Vec<Rect>,
    pub diagnostics: Vec<String>,
}

pub fn can_join_cross_page(
    previous: &Paragraph,
    next: &Paragraph,
    previous_lane: usize,
    next_lane: usize,
    previous_page_height: f32,
    next_page_height: f32,
) -> bool {
    let Some(previous_line) = previous.lines.last() else {
        return false;
    };
    let Some(next_line) = next.lines.first() else {
        return false;
    };
    if previous_lane != next_lane
        || !matches!(previous.kind, Kind::Body)
        || !matches!(next.kind, Kind::Body)
        || previous
            .text
            .trim_end()
            .ends_with(['.', '!', '?', ':', ';'])
        || previous.bounds.bottom > 80.0
        || next.bounds.top < next_page_height - 80.0
        || (previous_line.size - next_line.size).abs() > previous_line.size * 0.05
        || previous_line.bold != next_line.bold
        || previous_line.italic != next_line.italic
        || previous_line.serif != next_line.serif
        || !same_color(previous_line.color, next_line.color)
    {
        return false;
    }
    let left_delta = (previous.bounds.left - next.bounds.left).abs();
    let width_delta = (previous.bounds.width() - next.bounds.width()).abs();
    let _ = previous_page_height;
    left_delta <= previous_line.size * 2.0 && width_delta <= previous.bounds.width() * 0.35
}

pub fn join_cross_page_text(previous: &str, next: &str) -> String {
    let previous = previous.trim_end();
    let next = next.trim_start();
    if previous.ends_with('-')
        && previous
            .chars()
            .nth_back(1)
            .is_some_and(char::is_alphabetic)
        && next.chars().next().is_some_and(char::is_alphabetic)
    {
        format!("{}{}", previous.trim_end_matches('-'), next)
    } else {
        format!("{previous} {next}")
    }
}

fn same_color(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter()
        .zip(b.iter())
        .all(|(left, right)| left.abs_diff(*right) <= 12)
}

pub fn detect_regions(
    paragraphs: &[Paragraph],
    backgrounds: &[Background],
    rules: &[Rect],
    images: &[Rect],
    _page_bounds: Rect,
) -> PageRegions {
    let mut diagnostics = Vec::new();
    let mut ordered: Vec<usize> = (0..paragraphs.len()).collect();
    ordered.sort_by(|&a, &b| {
        paragraphs[b]
            .bounds
            .top
            .partial_cmp(&paragraphs[a].bounds.top)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                paragraphs[a]
                    .bounds
                    .left
                    .partial_cmp(&paragraphs[b].bounds.left)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    let mut flows = Vec::new();
    let mut body: Vec<usize> = Vec::new();
    let max_size = paragraphs
        .iter()
        .flat_map(|paragraph| paragraph.lines.iter().map(|line| line.size))
        .fold(0.0_f32, f32::max)
        .max(8.0);
    let gutter = max_size * 1.5;
    let mut left_lane = Vec::new();
    let mut right_lane = Vec::new();
    let mut split_x = None;

    for pair in ordered.windows(2) {
        let left = &paragraphs[pair[0]];
        let right = &paragraphs[pair[1]];
        let gap = right.bounds.left - left.bounds.right;
        if gap >= gutter {
            split_x = Some((left.bounds.right + right.bounds.left) * 0.5);
            break;
        }
    }

    for &index in &ordered {
        let paragraph = &paragraphs[index];
        let kind = paragraph_kind(paragraph, backgrounds, rules, images);
        match kind {
            RegionKind::Body if split_x.is_some_and(|x| paragraph.bounds.left < x) => {
                left_lane.push(index)
            }
            RegionKind::Body if split_x.is_some() => right_lane.push(index),
            RegionKind::Body => body.push(index),
            kind => {
                let bounds = paragraph.bounds;
                flows.push(FlowRegion {
                    id: FlowId {
                        section: flows.len(),
                        lane: 0,
                    },
                    kind,
                    bounds,
                    paragraph_indices: vec![index],
                });
            }
        }
    }

    if !left_lane.is_empty() && !right_lane.is_empty() {
        left_lane.sort_by(|&a, &b| top_then_left(&paragraphs[a], &paragraphs[b]));
        right_lane.sort_by(|&a, &b| top_then_left(&paragraphs[a], &paragraphs[b]));
        flows.push(flow(0, 0, RegionKind::Body, &left_lane, paragraphs));
        flows.push(flow(0, 1, RegionKind::Body, &right_lane, paragraphs));
    } else if !body.is_empty() {
        body.sort_by(|&a, &b| top_then_left(&paragraphs[a], &paragraphs[b]));
        flows.push(flow(0, 0, RegionKind::Body, &body, paragraphs));
    } else if split_x.is_some() {
        diagnostics.push("ambiguous column geometry; using one reading-order flow".into());
        let mut fallback = left_lane;
        fallback.extend(right_lane);
        fallback.sort_by(|&a, &b| top_then_left(&paragraphs[a], &paragraphs[b]));
        if !fallback.is_empty() {
            flows.push(flow(0, 0, RegionKind::Body, &fallback, paragraphs));
        }
    }

    let paragraph_indices: Vec<usize> = flows
        .iter()
        .flat_map(|flow| flow.paragraph_indices.iter().copied())
        .collect();
    let obstacles = backgrounds
        .iter()
        .map(|background| background.bounds)
        .chain(rules.iter().copied())
        .chain(images.iter().copied())
        .collect();
    PageRegions {
        flows,
        reading_order: paragraph_indices,
        obstacles,
        diagnostics,
    }
}

fn flow(
    section: usize,
    lane: usize,
    kind: RegionKind,
    paragraph_indices: &[usize],
    paragraphs: &[Paragraph],
) -> FlowRegion {
    let bounds = paragraph_indices
        .iter()
        .map(|&index| paragraphs[index].bounds)
        .reduce(union)
        .expect("flow has paragraphs");
    FlowRegion {
        id: FlowId { section, lane },
        kind,
        bounds,
        paragraph_indices: paragraph_indices.to_vec(),
    }
}

fn top_then_left(a: &Paragraph, b: &Paragraph) -> std::cmp::Ordering {
    b.bounds
        .top
        .partial_cmp(&a.bounds.top)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| {
            a.bounds
                .left
                .partial_cmp(&b.bounds.left)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

fn paragraph_kind(
    paragraph: &Paragraph,
    backgrounds: &[Background],
    rules: &[Rect],
    images: &[Rect],
) -> RegionKind {
    if paragraph.container.is_some() {
        return RegionKind::Panel;
    }
    if images.iter().any(|image| {
        let horizontal =
            paragraph.bounds.left.max(image.left) < paragraph.bounds.right.min(image.right);
        let overlap = (paragraph.bounds.right.min(image.right)
            - paragraph.bounds.left.max(image.left))
            / paragraph.bounds.width().min(image.width()).max(1.0);
        let vertical_gap = (paragraph.bounds.bottom - image.top)
            .abs()
            .min((image.bottom - paragraph.bounds.top).abs());
        horizontal && overlap >= 0.6 && vertical_gap <= paragraph.lines[0].size * 2.0
    }) {
        return RegionKind::Caption;
    }
    if rules.iter().any(|rule| {
        rule.overlaps_horizontally(&paragraph.bounds)
            && (rule.top - paragraph.bounds.bottom).abs() <= paragraph.lines[0].size * 1.5
    }) && paragraph.bounds.width()
        < backgrounds
            .iter()
            .map(|background| background.bounds.width())
            .fold(f32::MAX, f32::min)
    {
        return RegionKind::TableCell;
    }
    RegionKind::Body
}

fn union(a: Rect, b: Rect) -> Rect {
    Rect {
        left: a.left.min(b.left),
        bottom: a.bottom.min(b.bottom),
        right: a.right.max(b.right),
        top: a.top.max(b.top),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documents::pdf::layout::{Kind, Line};

    fn paragraph(left: f32, top: f32) -> Paragraph {
        let line = Line {
            fragments: Vec::new(),
            text: "text".into(),
            bounds: Rect {
                left,
                bottom: top - 12.0,
                right: left + 180.0,
                top,
            },
            baseline: top - 3.0,
            size: 12.0,
            bold: false,
            italic: false,
            serif: true,
            color: [0, 0, 0, 255],
            item_start: false,
        };
        Paragraph {
            lines: vec![line],
            kind: Kind::Body,
            text: "text".into(),
            bounds: Rect {
                left,
                bottom: top - 12.0,
                right: left + 180.0,
                top,
            },
            container: None,
        }
    }

    #[test]
    fn detect_regions_starts_with_one_body_flow() {
        let page = Rect {
            left: 0.0,
            bottom: 0.0,
            right: 612.0,
            top: 792.0,
        };
        let regions = detect_regions(
            &[paragraph(72.0, 700.0), paragraph(72.0, 650.0)],
            &[],
            &[],
            &[],
            page,
        );
        assert_eq!(regions.reading_order, vec![0, 1]);
        assert_eq!(regions.flows.len(), 1);
        assert_eq!(regions.flows[0].kind, RegionKind::Body);
    }

    #[test]
    fn two_columns_read_down_left_then_down_right() {
        let page = Rect {
            left: 0.0,
            bottom: 0.0,
            right: 612.0,
            top: 792.0,
        };
        let paragraphs = vec![
            paragraph(72.0, 700.0),
            paragraph(72.0, 650.0),
            paragraph(350.0, 700.0),
            paragraph(350.0, 650.0),
        ];
        let regions = detect_regions(&paragraphs, &[], &[], &[], page);
        assert_eq!(regions.flows.len(), 2);
        assert_eq!(regions.reading_order, vec![0, 1, 2, 3]);
    }

    #[test]
    fn page_sized_background_is_an_obstacle_not_a_panel() {
        let page = Rect {
            left: 0.0,
            bottom: 0.0,
            right: 612.0,
            top: 792.0,
        };
        let background = Background {
            bounds: page,
            color: [255, 255, 255, 255],
        };
        let regions = detect_regions(&[paragraph(72.0, 700.0)], &[background], &[], &[], page);
        assert_eq!(regions.flows[0].kind, RegionKind::Body);
        assert_eq!(regions.obstacles, vec![page]);
    }

    #[test]
    fn contained_paragraph_is_a_panel_flow() {
        let mut panel = paragraph(90.0, 700.0);
        panel.container = Some(Rect {
            left: 72.0,
            bottom: 650.0,
            right: 300.0,
            top: 720.0,
        });
        let page = Rect {
            left: 0.0,
            bottom: 0.0,
            right: 612.0,
            top: 792.0,
        };
        let regions = detect_regions(&[panel], &[], &[], &[], page);
        assert_eq!(regions.flows[0].kind, RegionKind::Panel);
    }

    #[test]
    fn non_body_columns_do_not_create_an_empty_flow() {
        let page = Rect {
            left: 0.0,
            bottom: 0.0,
            right: 612.0,
            top: 792.0,
        };
        let mut left = paragraph(72.0, 700.0);
        left.container = Some(Rect {
            left: 60.0,
            bottom: 650.0,
            right: 280.0,
            top: 720.0,
        });
        let mut right = paragraph(350.0, 700.0);
        right.container = Some(Rect {
            left: 340.0,
            bottom: 650.0,
            right: 560.0,
            top: 720.0,
        });
        let regions = detect_regions(&[left, right], &[], &[], &[], page);
        assert_eq!(regions.flows.len(), 2);
        assert_eq!(regions.reading_order, vec![0, 1]);
        assert!(regions
            .diagnostics
            .iter()
            .all(|diagnostic| !diagnostic.contains("empty")));
    }

    #[test]
    fn nearby_image_aligned_text_is_a_caption_flow() {
        let page = Rect {
            left: 0.0,
            bottom: 0.0,
            right: 612.0,
            top: 792.0,
        };
        let image = Rect {
            left: 72.0,
            bottom: 100.0,
            right: 300.0,
            top: 300.0,
        };
        let caption = paragraph(90.0, 290.0);
        let regions = detect_regions(&[caption], &[], &[], &[image], page);
        assert_eq!(regions.flows[0].kind, RegionKind::Caption);
    }

    #[test]
    fn ruled_narrow_text_is_a_table_cell_flow() {
        let page = Rect {
            left: 0.0,
            bottom: 0.0,
            right: 612.0,
            top: 792.0,
        };
        let table = Background {
            bounds: Rect {
                left: 60.0,
                bottom: 600.0,
                right: 550.0,
                top: 720.0,
            },
            color: [240, 240, 240, 255],
        };
        let cell = paragraph(72.0, 700.0);
        let rule = Rect {
            left: 60.0,
            bottom: 0.0,
            right: 550.0,
            top: 688.0,
        };
        let regions = detect_regions(&[cell], &[table], &[rule], &[], page);
        assert_eq!(regions.flows[0].kind, RegionKind::TableCell);
    }

    #[test]
    fn joins_only_a_visual_body_continuation() {
        let previous = paragraph(72.0, 70.0);
        let next = paragraph(72.0, 780.0);
        assert!(can_join_cross_page(&previous, &next, 0, 0, 792.0, 792.0));

        let mut ended = previous.clone();
        ended.text.push('.');
        assert!(!can_join_cross_page(&ended, &next, 0, 0, 792.0, 792.0));
        assert!(!can_join_cross_page(&previous, &next, 0, 1, 792.0, 792.0));
    }

    #[test]
    fn cross_page_join_rejects_style_and_margin_mismatches() {
        let previous = paragraph(72.0, 70.0);
        let next = paragraph(72.0, 780.0);

        let mut different_style = next.clone();
        different_style.lines[0].size = 16.0;
        assert!(!can_join_cross_page(
            &previous,
            &different_style,
            0,
            0,
            792.0,
            792.0
        ));

        let not_at_top = paragraph(72.0, 700.0);
        assert!(!can_join_cross_page(
            &previous,
            &not_at_top,
            0,
            0,
            792.0,
            792.0
        ));

        let not_at_bottom = paragraph(72.0, 150.0);
        assert!(!can_join_cross_page(
            &not_at_bottom,
            &next,
            0,
            0,
            792.0,
            792.0
        ));
    }

    #[test]
    fn cross_page_join_rejects_non_body_paragraphs() {
        let mut heading = paragraph(72.0, 70.0);
        heading.kind = Kind::Heading;
        let next = paragraph(72.0, 780.0);
        assert!(!can_join_cross_page(&heading, &next, 0, 0, 792.0, 792.0));
    }

    #[test]
    fn joins_hyphenated_words_without_an_extra_space() {
        assert_eq!(
            join_cross_page_text("inter-", "national text"),
            "international text"
        );
        assert_eq!(
            join_cross_page_text("first paragraph", "next paragraph"),
            "first paragraph next paragraph"
        );
    }
}
