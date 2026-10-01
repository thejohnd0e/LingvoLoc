use super::layout::{self, Measure, Rect};
use crate::documents::BlockType;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub size: f32,
    pub pitch: f32,
    pub bold: bool,
    pub italic: bool,
    pub serif: bool,
    pub color: [u8; 4],
}

#[derive(Debug, Clone)]
pub struct FlowItem {
    pub block_id: String,
    pub text: String,
    pub block_type: BlockType,
    pub style: TextStyle,
    pub original_bounds: Rect,
    pub original_lines: usize,
    pub preferred_gap_after: f32,
    /// Horizontal extent available to this item.
    pub left: f32,
    pub right: f32,
    /// Baseline of the first source line; the item never starts above it.
    pub first_baseline: f32,
    /// Centre line for centred text.
    pub center: Option<f32>,
    pub right_aligned: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlowFrame {
    pub page: usize,
    pub bounds: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlannedLine {
    pub block_id: String,
    pub page: usize,
    pub text: String,
    pub x: f32,
    pub baseline: f32,
    pub size: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FlowPlan {
    pub lines: Vec<PlannedLine>,
    pub scale: f32,
    pub spacing_ratio: f32,
    pub overflow_blocks: Vec<String>,
}

/// `max_scale` caps the font scale so that similar items can share one size.
pub fn place_flow(
    items: &[FlowItem],
    frames: &[FlowFrame],
    measure: &dyn Measure,
    max_scale: f32,
) -> FlowPlan {
    let Some(frame) = frames.first().copied() else {
        return FlowPlan {
            lines: Vec::new(),
            scale: 1.0,
            spacing_ratio: 1.25,
            overflow_blocks: items.iter().map(|item| item.block_id.clone()).collect(),
        };
    };
    let scales = [
        1.0,
        0.94,
        0.89,
        0.85,
        0.81,
        0.78,
        0.74,
        0.70,
        0.66,
        0.62,
        0.58,
        layout::MIN_SCALE,
    ];
    let gap_factors = [1.0, 0.7, 0.35, 0.0];
    let leading_factors = [1.0, 0.92, 0.85];

    for scale in scales
        .into_iter()
        .filter(|scale| *scale <= max_scale + 1e-4)
    {
        // Keep paragraph spacing until the font has been shrunk a little.
        for gap_factor in gap_factors
            .into_iter()
            .filter(|gap| *gap >= 0.7 || scale <= 0.88)
        {
            for leading_factor in leading_factors {
                let candidate =
                    build_candidate(items, frame, scale, gap_factor, leading_factor, measure);
                if candidate.fits {
                    return candidate.plan;
                }
            }
        }
    }

    build_candidate(
        items,
        frame,
        layout::MIN_SCALE.min(max_scale),
        0.0,
        *leading_factors.last().expect("leading list is non-empty"),
        measure,
    )
    .plan
}

struct Candidate {
    plan: FlowPlan,
    fits: bool,
}

/// One wrapped item laid out from the top of the frame.
struct Packed {
    item: usize,
    lines: Vec<String>,
    size: f32,
    pitch: f32,
    first_baseline: f32,
}

fn build_candidate(
    items: &[FlowItem],
    frame: FlowFrame,
    scale: f32,
    gap_factor: f32,
    leading_factor: f32,
    measure: &dyn Measure,
) -> Candidate {
    let mut cursor = frame.bounds.top;
    let mut fits = true;
    let mut overflow_blocks = Vec::new();
    let mut packed: Vec<Packed> = Vec::new();
    let spacing_ratio = items
        .first()
        .map(|item| (item.style.pitch / item.style.size).max(1.08) * leading_factor)
        .unwrap_or(1.25);

    // Pass 1: pack everything as high as the frame allows to see what fits.
    for (index, item) in items.iter().enumerate() {
        let _source_geometry = (item.original_bounds, item.original_lines);
        let size = item.style.size * scale;
        let pitch = (item.style.pitch * scale * leading_factor).max(size * 1.08);
        let width = (item.right - item.left).max(size);
        let wrapped = layout::wrap(
            &item.text,
            width,
            size,
            item.style.bold,
            item.style.italic,
            measure,
        );
        let gap = if index + 1 == items.len() {
            0.0
        } else {
            let minimum = match item.block_type {
                BlockType::Heading => pitch * 0.6,
                _ => pitch * 0.35,
            };
            item.preferred_gap_after.max(minimum) * gap_factor
        };
        if wrapped.is_empty() {
            continue;
        }
        let first_baseline = cursor - size * 0.8;
        let last_baseline = first_baseline - pitch * (wrapped.len() - 1) as f32;
        if item
            .text
            .split_whitespace()
            .any(|word| measure.width(word, size, item.style.bold, item.style.italic) > width)
            || last_baseline - size * 0.3 < frame.bounds.bottom
        {
            fits = false;
        }
        if !fits && !overflow_blocks.contains(&item.block_id) {
            overflow_blocks.push(item.block_id.clone());
        }
        cursor = last_baseline - size * 0.3 - gap;
        packed.push(Packed {
            item: index,
            lines: wrapped,
            size,
            pitch,
            first_baseline,
        });
    }

    // Pass 2: the free space left at the bottom moves each item back towards
    // its source position, never reordering or overlapping items.
    let slack = packed
        .last()
        .map(|last| {
            last.first_baseline
                - last.pitch * (last.lines.len() - 1) as f32
                - last.size * 0.3
                - frame.bounds.bottom
        })
        .unwrap_or(0.0)
        .max(0.0);
    let mut lines = Vec::new();
    let mut offset = 0.0_f32;
    for entry in packed {
        let item = &items[entry.item];
        let wanted = (entry.first_baseline - item.first_baseline).max(0.0);
        offset = offset.max(wanted).min(slack);
        for (line_index, text) in entry.lines.into_iter().enumerate() {
            let x = match item.center {
                Some(center) => {
                    center
                        - measure.width(&text, entry.size, item.style.bold, item.style.italic) * 0.5
                }
                None => match item.right_aligned {
                    Some(edge) => {
                        edge - measure.width(&text, entry.size, item.style.bold, item.style.italic)
                    }
                    None => item.left,
                },
            };
            lines.push(PlannedLine {
                block_id: item.block_id.clone(),
                page: frame.page,
                text,
                x,
                baseline: entry.first_baseline - offset - entry.pitch * line_index as f32,
                size: entry.size,
            });
        }
    }

    Candidate {
        plan: FlowPlan {
            lines,
            scale,
            spacing_ratio,
            overflow_blocks,
        },
        fits,
    }
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

    fn item(id: &str) -> FlowItem {
        FlowItem {
            block_id: id.into(),
            text: "translated text".into(),
            block_type: BlockType::Paragraph,
            style: TextStyle {
                size: 12.0,
                pitch: 15.0,
                bold: false,
                italic: false,
                serif: true,
                color: [0, 0, 0, 255],
            },
            original_bounds: Rect {
                left: 72.0,
                bottom: 700.0,
                right: 300.0,
                top: 720.0,
            },
            original_lines: 1,
            preferred_gap_after: 5.0,
            left: 72.0,
            right: 300.0,
            first_baseline: 718.0,
            center: None,
            right_aligned: None,
        }
    }

    #[test]
    fn place_flow_returns_lines_for_one_item() {
        let plan = place_flow(
            &[item("pdf#p0001-000")],
            &[FlowFrame {
                page: 0,
                bounds: Rect {
                    left: 72.0,
                    bottom: 72.0,
                    right: 540.0,
                    top: 720.0,
                },
            }],
            &Fixed,
            1.0,
        );
        assert_eq!(plan.lines.len(), 1);
        assert_eq!(plan.lines[0].block_id, "pdf#p0001-000");
    }

    #[test]
    fn longer_translation_moves_following_paragraph_before_shrinking() {
        let mut first = item("first");
        first.text = "word ".repeat(24);
        let second = item("second");
        let plan = place_flow(
            &[first, second],
            &[FlowFrame {
                page: 0,
                bounds: Rect {
                    left: 72.0,
                    bottom: 560.0,
                    right: 300.0,
                    top: 720.0,
                },
            }],
            &Fixed,
            1.0,
        );
        assert_eq!(plan.scale, 1.0);
        assert!(plan.lines[1].baseline < plan.lines[0].baseline);
    }

    #[test]
    fn gaps_compress_before_shared_font_scale_changes() {
        let mut first = item("first");
        first.preferred_gap_after = 20.0;
        let second = item("second");
        let plan = place_flow(
            &[first, second],
            &[FlowFrame {
                page: 0,
                bounds: Rect {
                    left: 72.0,
                    bottom: 679.0,
                    right: 300.0,
                    top: 720.0,
                },
            }],
            &Fixed,
            1.0,
        );
        assert_eq!(plan.scale, 1.0);
    }

    #[test]
    fn long_list_item_wraps_to_two_lines_instead_of_becoming_one_tiny_line() {
        let mut list = item("list");
        list.block_type = BlockType::ListItem;
        list.text = "word ".repeat(40);
        let plan = place_flow(
            &[list],
            &[FlowFrame {
                page: 0,
                bounds: Rect {
                    left: 72.0,
                    bottom: 500.0,
                    right: 300.0,
                    top: 720.0,
                },
            }],
            &Fixed,
            1.0,
        );
        assert!(plan.lines.len() >= 2);
        assert_eq!(plan.scale, 1.0);
    }

    #[test]
    fn compatible_body_items_share_scale_and_leading() {
        let plan = place_flow(
            &[item("first"), item("second")],
            &[FlowFrame {
                page: 0,
                bounds: Rect {
                    left: 72.0,
                    bottom: 650.0,
                    right: 300.0,
                    top: 720.0,
                },
            }],
            &Fixed,
            1.0,
        );
        assert_eq!(plan.scale, 1.0);
        assert_eq!(plan.spacing_ratio, 1.25);
        assert!(plan.lines.iter().all(|line| line.size == 12.0));
    }
}
