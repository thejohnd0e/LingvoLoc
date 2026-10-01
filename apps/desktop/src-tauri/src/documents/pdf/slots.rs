//! Obstacle-aware placement slots. Every translated paragraph keeps its source
//! position; it may only grow into free space below it, up to the nearest
//! paragraph, rule, panel edge or image. Paragraphs that are stacked in one
//! column with nothing between them form a run that is laid out together.

use super::layout::{Paragraph, Rect};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slot {
    pub paragraph: usize,
    pub left: f32,
    pub right: f32,
    /// Highest point the text may reach; it starts at its source position and
    /// is raised only when it does not otherwise fit.
    pub top: f32,
    /// Lowest point the text may reach.
    pub bottom: f32,
    /// Horizontal centre when the source text is centred in its region.
    pub center: Option<f32>,
    /// Right edge when the source text is right-aligned.
    pub right_aligned: Option<f32>,
}

const PAGE_BOTTOM_MARGIN: f32 = 36.0;

fn size_of(paragraph: &Paragraph) -> f32 {
    paragraph
        .lines
        .first()
        .map(|line| line.size)
        .unwrap_or(10.0)
}

fn overlap(a_left: f32, a_right: f32, b_left: f32, b_right: f32) -> bool {
    a_left < b_right - 0.5 && b_left < a_right - 0.5
}

/// The panel behind a paragraph. Text boxes can be taller than a clipped panel,
/// so a background that spans the paragraph's first line counts as well.
fn container_of(paragraph: &Paragraph, obstacles: &[Rect]) -> Option<Rect> {
    let bounds = paragraph.bounds;
    let size = size_of(paragraph);
    obstacles
        .iter()
        .filter(|obstacle| {
            obstacle.width() > 20.0
                && obstacle.height() > 8.0
                && (obstacle.contains(&bounds)
                    || (obstacle.left <= bounds.left + 1.0
                        && obstacle.right >= bounds.right - 1.0
                        && obstacle.top >= bounds.top - 1.0
                        && obstacle.bottom < bounds.top - size * 0.5))
        })
        .min_by(|a, b| {
            (a.width() * a.height())
                .partial_cmp(&(b.width() * b.height()))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .copied()
}

/// Region the paragraph is laid out in: its panel (inset by the text's own
/// padding) or the page's text column.
fn region_of(
    paragraph: &Paragraph,
    container: Option<Rect>,
    text_left: f32,
    text_right: f32,
) -> (f32, f32) {
    match container {
        Some(container) => {
            let pad = (paragraph.bounds.left - container.left).max(4.0);
            (container.left + pad, container.right - pad)
        }
        None => (text_left, text_right),
    }
}

fn centered_in(paragraph: &Paragraph, region: (f32, f32)) -> Option<f32> {
    let center = (region.0 + region.1) * 0.5;
    let centered = paragraph
        .lines
        .iter()
        .all(|line| ((line.bounds.left + line.bounds.right) * 0.5 - center).abs() <= 8.0);
    let narrow = paragraph.bounds.width() < (region.1 - region.0) * 0.85;
    (centered && narrow).then_some(center)
}

fn right_aligned_in(paragraph: &Paragraph, region: (f32, f32)) -> Option<f32> {
    let edge = paragraph.bounds.right;
    let aligned = paragraph
        .lines
        .iter()
        .all(|line| (line.bounds.right - edge).abs() <= 2.0);
    let narrow = paragraph.bounds.width() < (region.1 - region.0) * 0.85;
    (aligned && narrow && edge >= region.1 - 8.0).then_some(edge)
}

fn same_container(a: Option<Rect>, b: Option<Rect>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.contains(&b) && b.contains(&a),
        _ => false,
    }
}

/// Right edge available to `paragraph`.
fn right_limit(
    index: usize,
    container: Option<Rect>,
    paragraphs: &[Paragraph],
    obstacles: &[Rect],
    text_right: f32,
) -> f32 {
    let paragraph = &paragraphs[index];
    let bounds = paragraph.bounds;
    let size = size_of(paragraph);
    let mut right = match container {
        Some(container) => {
            let pad = (bounds.left - container.left).max(4.0);
            container.right - pad
        }
        None => text_right,
    }
    .max(bounds.right);
    for (other_index, other) in paragraphs.iter().enumerate() {
        if other_index == index {
            continue;
        }
        let beside = other.bounds.bottom < bounds.top && other.bounds.top > bounds.bottom;
        if beside && other.bounds.left >= bounds.right - 1.0 {
            right = right.min((other.bounds.left - size * 0.8).max(bounds.right));
        }
    }
    for obstacle in obstacles {
        let beside = obstacle.bottom < bounds.top - 1.0 && obstacle.top > bounds.bottom + 1.0;
        if beside && !obstacle.contains(&bounds) && obstacle.left >= bounds.right - 1.0 {
            right = right.min((obstacle.left - 2.0).max(bounds.right));
        }
    }
    right
}

struct Limits {
    /// Highest point the text may be raised to when it needs more room.
    up: f32,
    /// Nearest obstacle/panel edge below (page margin when none).
    solid: f32,
    /// Nearest paragraph below, with its index.
    paragraph: Option<(usize, f32)>,
}

fn limits(
    index: usize,
    container: Option<Rect>,
    right: f32,
    paragraphs: &[Paragraph],
    obstacles: &[Rect],
) -> Limits {
    let paragraph = &paragraphs[index];
    let bounds = paragraph.bounds;
    let size = size_of(paragraph);
    let mut solid = f32::MIN;
    let mut up = bounds.top + size * 3.0;
    if let Some(container) = container {
        up = up.min(container.top - size * 0.25);
    }
    for obstacle in obstacles {
        if !obstacle.contains(&bounds)
            && obstacle.bottom >= bounds.top - 1.0
            && overlap(bounds.left, right, obstacle.left, obstacle.right)
        {
            up = up.min(obstacle.bottom - size * 0.25);
        }
    }
    for (other_index, other) in paragraphs.iter().enumerate() {
        if other_index != index
            && other.bounds.bottom >= bounds.top - 1.0
            && overlap(bounds.left, right, other.bounds.left, other.bounds.right)
        {
            up = up.min(other.bounds.bottom - size * 0.25);
        }
    }
    up = up.max(bounds.top);
    let mut nearest: Option<(usize, f32)> = None;
    if let Some(container) = container {
        // A panel running to the page edge is cut by a page break: the visible
        // part ends with the source text.
        solid = if container.bottom <= PAGE_BOTTOM_MARGIN + 4.0 {
            bounds.bottom
        } else {
            container.bottom + size * 0.25
        };
    }
    for obstacle in obstacles {
        if obstacle.contains(&bounds) || obstacle.top > bounds.bottom + 1.0 {
            continue;
        }
        if overlap(bounds.left, right, obstacle.left, obstacle.right) {
            solid = solid.max(obstacle.top + size * 0.25);
        }
    }
    for (other_index, other) in paragraphs.iter().enumerate() {
        if other_index == index
            || other.bounds.top > bounds.bottom + 1.0
            || other.bounds.bottom >= bounds.bottom
        {
            continue;
        }
        if overlap(bounds.left, right, other.bounds.left, other.bounds.right) {
            let top = other.bounds.top;
            if nearest.is_none_or(|(_, best)| top > best) {
                nearest = Some((other_index, top));
            }
        }
    }
    if solid == f32::MIN && nearest.is_none() {
        solid = PAGE_BOTTOM_MARGIN;
    }
    Limits {
        up,
        solid,
        paragraph: nearest,
    }
}

/// Slots for the `active` paragraphs, grouped into runs (top to bottom).
pub fn runs(paragraphs: &[Paragraph], obstacles: &[Rect], active: &[usize]) -> Vec<Vec<Slot>> {
    let text_right = paragraphs
        .iter()
        .filter(|paragraph| container_of(paragraph, obstacles).is_none())
        .map(|paragraph| paragraph.bounds.right)
        .fold(f32::MIN, f32::max);
    let text_right = if text_right == f32::MIN {
        paragraphs
            .iter()
            .map(|paragraph| paragraph.bounds.right)
            .fold(0.0, f32::max)
    } else {
        text_right
    };

    let text_left = paragraphs
        .iter()
        .filter(|paragraph| container_of(paragraph, obstacles).is_none())
        .map(|paragraph| paragraph.bounds.left)
        .fold(f32::MAX, f32::min);
    let mut order: Vec<usize> = active.to_vec();
    order.sort_by(|&a, &b| {
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

    struct Info {
        right: f32,
        limits: Limits,
    }
    let containers: Vec<Option<Rect>> = paragraphs
        .iter()
        .map(|paragraph| container_of(paragraph, obstacles))
        .collect();
    let infos: Vec<Info> = order
        .iter()
        .map(|&index| {
            let container = containers[index];
            let right = right_limit(index, container, paragraphs, obstacles, text_right);
            let limits = limits(index, container, right, paragraphs, obstacles);
            Info { right, limits }
        })
        .collect();

    let position = |index: usize| order.iter().position(|&candidate| candidate == index);
    let mut next: Vec<Option<usize>> = vec![None; order.len()];
    let mut has_previous = vec![false; order.len()];
    for (slot, &index) in order.iter().enumerate() {
        let info = &infos[slot];
        let Some((below, top)) = info.limits.paragraph else {
            continue;
        };
        let Some(below_slot) = position(below) else {
            continue;
        };
        let a = &paragraphs[index];
        let b = &paragraphs[below];
        let size = size_of(a);
        if has_previous[below_slot]
            || info.limits.solid > top
            || !same_container(containers[index], containers[below])
            || (a.bounds.left - b.bounds.left).abs() > size * 4.0
            || a.bounds.bottom - top > size * 3.0
        {
            continue;
        }
        next[slot] = Some(below_slot);
        has_previous[below_slot] = true;
    }

    let mut result = Vec::new();
    for start in (0..order.len()).filter(|&slot| !has_previous[slot]) {
        let mut run = Vec::new();
        let mut cursor = Some(start);
        while let Some(slot) = cursor {
            let index = order[slot];
            let bounds = paragraphs[index].bounds;
            let region = region_of(&paragraphs[index], containers[index], text_left, text_right);
            let center = centered_in(&paragraphs[index], region);
            let right_aligned = if center.is_none() {
                right_aligned_in(&paragraphs[index], region)
            } else {
                None
            };
            run.push(Slot {
                paragraph: index,
                left: if let Some(center) = center {
                    center - (region.1 - region.0) * 0.5
                } else if right_aligned.is_some() {
                    region.0
                } else if paragraphs[index].lines[0].item_start {
                    paragraphs[index].lines[0].bounds.left
                } else {
                    bounds.left
                },
                right: match center {
                    Some(center) => center + (region.1 - region.0) * 0.5,
                    None if right_aligned.is_some() => region.1.max(bounds.right),
                    None => infos[slot].right,
                },
                top: infos[slot].limits.up,
                bottom: 0.0,
                center,
                right_aligned,
            });
            cursor = next[slot];
        }
        // The run may reach down to where its last paragraph is blocked.
        let last_slot = position(run.last().expect("run is not empty").paragraph)
            .expect("run paragraphs are active");
        let last = &paragraphs[order[last_slot]];
        let limits = &infos[last_slot].limits;
        let size = size_of(last);
        let bottom = match limits.paragraph {
            Some((_, top)) => limits.solid.max(top + size * 0.35),
            None => limits.solid,
        }
        .min(last.bounds.bottom);
        for slot in &mut run {
            slot.bottom = bottom;
        }
        result.push(run);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documents::pdf::layout::{Kind, Line};

    fn paragraph(left: f32, right: f32, top: f32, bottom: f32) -> Paragraph {
        let bounds = Rect {
            left,
            bottom,
            right,
            top,
        };
        Paragraph {
            lines: vec![Line {
                fragments: Vec::new(),
                text: "text".into(),
                bounds,
                baseline: bottom + 2.0,
                size: 10.0,
                bold: false,
                italic: false,
                serif: true,
                color: [0, 0, 0, 255],
                item_start: false,
            }],
            kind: Kind::Body,
            text: "text".into(),
            bounds,
            container: None,
        }
    }

    #[test]
    fn stacked_paragraphs_form_one_run_that_reaches_the_page_margin() {
        let paragraphs = vec![
            paragraph(72.0, 500.0, 700.0, 660.0),
            paragraph(72.0, 480.0, 650.0, 600.0),
        ];
        let runs = runs(&paragraphs, &[], &[0, 1]);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].len(), 2);
        assert!(runs[0][0].bottom <= 600.0);
        assert_eq!(runs[0][0].bottom, PAGE_BOTTOM_MARGIN);
    }

    #[test]
    fn rule_between_paragraphs_splits_runs() {
        let paragraphs = vec![
            paragraph(72.0, 500.0, 700.0, 660.0),
            paragraph(72.0, 500.0, 640.0, 600.0),
        ];
        let rule = Rect {
            left: 60.0,
            bottom: 649.0,
            right: 520.0,
            top: 651.0,
        };
        let runs = runs(&paragraphs, &[rule], &[0, 1]);
        assert_eq!(runs.len(), 2);
        assert!(runs[0][0].bottom >= 651.0);
    }

    #[test]
    fn neighbour_column_limits_the_right_edge() {
        let paragraphs = vec![
            paragraph(72.0, 150.0, 700.0, 660.0),
            paragraph(250.0, 500.0, 700.0, 660.0),
        ];
        let runs = runs(&paragraphs, &[], &[0, 1]);
        let first = runs
            .iter()
            .flatten()
            .find(|slot| slot.paragraph == 0)
            .unwrap();
        assert!(first.right < 250.0 && first.right >= 150.0);
    }
}
