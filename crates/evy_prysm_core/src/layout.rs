//! The layout function: `(element_tree, viewport) → (positions, sizes)`.
//!
//! Pure, deterministic, single-pass DFS — O(n) for n elements per
//! prysm/layout.md Theorem 1. Implements the membrane–organelle
//! protocol Π (§4): constrain → occupy → place, recursively.

use crate::constraint::{Constraint, OccupiedSize, Position};
use crate::container::{Align, Container, Direction};
use crate::element::Element;
use crate::sizing::SizeType;

/// Output of `layout()`. One entry per element, indexed in DFS pre-order
/// of the input tree (root is index 0).
#[derive(Debug, Clone, Default)]
pub struct LayoutResult {
    pub positions: Vec<Position>,
    pub sizes: Vec<OccupiedSize>,
}

impl LayoutResult {
    /// Total entries — `tree.count()` for the input tree.
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }
}

/// Compute layout for `root` within `viewport`.
///
/// `viewport` is the root constraint — `(width, height)` in quanta. The
/// returned `LayoutResult` has one `(position, size)` pair per element
/// in DFS pre-order. Position is measured from the root's origin in
/// quanta.
pub fn layout(root: &Element, viewport: Constraint) -> LayoutResult {
    let n = root.count();
    let mut result = LayoutResult {
        positions: vec![Position::default(); n],
        sizes: vec![OccupiedSize::default(); n],
    };
    layout_recursive(root, viewport, Position::new(0, 0), &mut result, &mut 0);
    result
}

/// Recursive DFS implementation of the Π protocol.
///
/// Returns the occupied size of `element` after layout — needed by the
/// caller (a membrane) to size siblings and compute its own bounds.
fn layout_recursive(
    element: &Element,
    constraint: Constraint,
    position: Position,
    result: &mut LayoutResult,
    next_id: &mut usize,
) -> OccupiedSize {
    let my_id = *next_id;
    *next_id += 1;

    // 1. OCCUPY (this element fits itself into `constraint`).
    let my_size = occupy(element, constraint);

    // 2. RECORD position + size for self.
    result.positions[my_id] = position;
    result.sizes[my_id] = my_size;

    // 3. CONSTRAIN children (if I am a membrane).
    if let Some(container) = &element.container {
        match container {
            Container::Stack {
                direction,
                gap,
                align,
            } => layout_stack(
                element,
                my_size,
                position,
                *direction,
                *gap,
                *align,
                result,
                next_id,
            ),
            Container::Grid { .. } => {
                // Session 2.
                for child in &element.children {
                    layout_recursive(child, Constraint::new(0, 0), position, result, next_id);
                }
            }
            Container::Layer => {
                // Session 2.
                for child in &element.children {
                    layout_recursive(child, Constraint::new(0, 0), position, result, next_id);
                }
            }
        }
    }

    my_size
}

/// Resolve this element's own occupied size against the constraint.
///
/// For session 1 we ignore element padding (no Padding field on Element
/// yet — added when atoms land). Membranes occupy the size derived from
/// their own sizing primitives; their children get a constraint equal
/// to the membrane's own occupied size (no padding subtraction yet).
fn occupy(element: &Element, c: Constraint) -> OccupiedSize {
    let w = resolve_dim(&element.size.width, c.w, &element.min_size.width);
    let h = resolve_dim(&element.size.height, c.h, &element.min_size.height);
    OccupiedSize { w, h }
}

/// Resolve one dimension given a `SizeType`, an axis constraint, and the
/// minimum-size hint for that dimension.
///
/// `Fill` resolves to the full constraint here — siblings within a
/// container distribute `Fill` proportionally (see `layout_stack`).
/// When `occupy` is called for a non-Stack context (root, leaf), `Fill`
/// behaves as "take all available."
fn resolve_dim(s: &SizeType, c: u32, min: &SizeType) -> u32 {
    let raw = match s {
        SizeType::Fix(k) => *k,
        SizeType::Fill { .. } => c,
        SizeType::Scale { ratio, min: m } => {
            let scaled = ((c as f32) * ratio).round() as u32;
            scaled.max(*m)
        }
    };
    // Enforce the s_min floor for Scale/Fill; Fix is honored literally.
    let floor = match min {
        SizeType::Fix(k) => *k,
        _ => 0,
    };
    raw.max(floor).min(c)
}

/// Stack layout. Resolves sizes (fix → scale → fill) along the main axis,
/// then places organelles by cumulative offset with `gap` between them.
fn layout_stack(
    element: &Element,
    parent_size: OccupiedSize,
    parent_pos: Position,
    direction: Direction,
    gap: u32,
    align: Align,
    result: &mut LayoutResult,
    next_id: &mut usize,
) {
    let n = element.children.len();
    if n == 0 {
        return;
    }

    // Main-axis budget = parent's main-axis size minus inter-element gaps.
    let (main_avail, cross_avail) = match direction {
        Direction::Horizontal => (parent_size.w, parent_size.h),
        Direction::Vertical => (parent_size.h, parent_size.w),
    };
    let total_gap = gap.saturating_mul((n as u32).saturating_sub(1));
    let usable_main = main_avail.saturating_sub(total_gap);

    // Pass 1: resolve fix + scale on main axis, sum their consumption,
    // count fill weights for the remainder.
    let mut sizes_main: Vec<u32> = vec![0; n];
    let mut fill_indices: Vec<usize> = Vec::new();
    let mut fill_weight_total: f32 = 0.0;
    let mut fixed_consumed: u32 = 0;

    for (i, child) in element.children.iter().enumerate() {
        let main_sizing = match direction {
            Direction::Horizontal => &child.size.width,
            Direction::Vertical => &child.size.height,
        };
        match main_sizing {
            SizeType::Fix(k) => {
                let s = (*k).min(usable_main.saturating_sub(fixed_consumed));
                sizes_main[i] = s;
                fixed_consumed = fixed_consumed.saturating_add(s);
            }
            SizeType::Scale { ratio, min } => {
                let scaled = (usable_main as f32 * ratio).round() as u32;
                let s = scaled.max(*min).min(usable_main.saturating_sub(fixed_consumed));
                sizes_main[i] = s;
                fixed_consumed = fixed_consumed.saturating_add(s);
            }
            SizeType::Fill { weight } => {
                fill_indices.push(i);
                fill_weight_total += *weight;
            }
        }
    }

    // Pass 2: distribute remainder across fills by weight.
    let remainder_main = usable_main.saturating_sub(fixed_consumed);
    if !fill_indices.is_empty() && fill_weight_total > 0.0 {
        let mut distributed: u32 = 0;
        for (k, &i) in fill_indices.iter().enumerate() {
            let weight = match element.children[i].size {
                crate::Size {
                    width: SizeType::Fill { weight },
                    ..
                } if direction == Direction::Horizontal => weight,
                crate::Size {
                    height: SizeType::Fill { weight },
                    ..
                } if direction == Direction::Vertical => weight,
                _ => 0.0,
            };
            let share = if k + 1 == fill_indices.len() {
                // Give the last fill the residual to avoid rounding loss.
                remainder_main.saturating_sub(distributed)
            } else {
                ((remainder_main as f32) * weight / fill_weight_total).round() as u32
            };
            sizes_main[i] = share;
            distributed = distributed.saturating_add(share);
        }
    }

    // Pass 3: place. Walk children, cumulating offsets along main axis.
    let mut cursor: u32 = 0;
    for (i, child) in element.children.iter().enumerate() {
        let main_size = sizes_main[i];
        let cross_sizing = match direction {
            Direction::Horizontal => &child.size.height,
            Direction::Vertical => &child.size.width,
        };
        let cross_size = resolve_dim(
            cross_sizing,
            cross_avail,
            &match direction {
                Direction::Horizontal => child.min_size.height,
                Direction::Vertical => child.min_size.width,
            },
        );

        let cross_offset = match align {
            Align::Start => 0,
            Align::Center => cross_avail.saturating_sub(cross_size) / 2,
            Align::End => cross_avail.saturating_sub(cross_size),
            Align::Stretch => 0,
        };

        let (child_pos, child_constraint) = match direction {
            Direction::Horizontal => (
                Position::new(parent_pos.x + cursor, parent_pos.y + cross_offset),
                Constraint::new(
                    main_size,
                    if align == Align::Stretch {
                        cross_avail
                    } else {
                        cross_size
                    },
                ),
            ),
            Direction::Vertical => (
                Position::new(parent_pos.x + cross_offset, parent_pos.y + cursor),
                Constraint::new(
                    if align == Align::Stretch {
                        cross_avail
                    } else {
                        cross_size
                    },
                    main_size,
                ),
            ),
        };

        layout_recursive(child, child_constraint, child_pos, result, next_id);

        cursor = cursor.saturating_add(main_size).saturating_add(gap);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Size;

    fn leaf_fix(w: u32, h: u32) -> Element {
        Element::leaf(Size::new(SizeType::Fix(w), SizeType::Fix(h)))
    }

    fn leaf_fill() -> Element {
        Element::leaf(Size::new(SizeType::fill(), SizeType::fill()))
    }

    #[test]
    fn single_leaf_fits_constraint() {
        let leaf = leaf_fix(10, 5);
        let result = layout(&leaf, Constraint::new(100, 100));
        assert_eq!(result.len(), 1);
        assert_eq!(result.sizes[0], OccupiedSize::new(10, 5));
        assert_eq!(result.positions[0], Position::new(0, 0));
    }

    #[test]
    fn fill_takes_full_constraint() {
        let leaf = leaf_fill();
        let result = layout(&leaf, Constraint::new(50, 30));
        assert_eq!(result.sizes[0], OccupiedSize::new(50, 30));
    }

    #[test]
    fn fix_size_clamps_to_constraint() {
        let leaf = leaf_fix(200, 200);
        let result = layout(&leaf, Constraint::new(50, 30));
        // Fix is clamped down to the constraint, not honored beyond it.
        assert_eq!(result.sizes[0], OccupiedSize::new(50, 30));
    }

    #[test]
    fn vertical_stack_places_children_in_sequence() {
        let stack = Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::vertical(),
            vec![leaf_fix(20, 10), leaf_fix(20, 5), leaf_fix(20, 8)],
        );
        let result = layout(&stack, Constraint::new(100, 100));
        // 4 nodes: stack + 3 children.
        assert_eq!(result.len(), 4);
        // Stack itself takes full constraint.
        assert_eq!(result.sizes[0], OccupiedSize::new(100, 100));
        // Children stack vertically with default gap = 1.
        assert_eq!(result.positions[1], Position::new(0, 0));
        assert_eq!(result.positions[2], Position::new(0, 11)); // 10 + 1 gap
        assert_eq!(result.positions[3], Position::new(0, 17)); // 10 + 1 + 5 + 1
    }

    #[test]
    fn horizontal_stack_places_children_along_x() {
        let stack = Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::horizontal(),
            vec![leaf_fix(10, 5), leaf_fix(15, 5)],
        );
        let result = layout(&stack, Constraint::new(100, 20));
        assert_eq!(result.positions[1], Position::new(0, 0));
        assert_eq!(result.positions[2], Position::new(11, 0)); // 10 + 1 gap
    }

    #[test]
    fn fill_children_split_remainder() {
        let stack = Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::horizontal(),
            vec![leaf_fix(20, 5), leaf_fill(), leaf_fill()],
        );
        // constraint w=100; gaps = 2; fixed = 20; remainder = 78; two fills → 39 each
        let result = layout(&stack, Constraint::new(100, 20));
        assert_eq!(result.sizes[1].w, 20);
        assert_eq!(result.sizes[2].w + result.sizes[3].w, 78);
        // Rounding-correct split: each ~39, last gets residual.
        assert!((result.sizes[2].w as i64 - 39).abs() <= 1);
    }

    #[test]
    fn nested_stacks_compose_correctly() {
        // Vertical outer stack: two horizontal rows.
        let row = Element::membrane(
            Size::new(SizeType::fill(), SizeType::Fix(10)),
            Container::horizontal(),
            vec![leaf_fix(20, 10), leaf_fix(30, 10)],
        );
        let outer = Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::vertical(),
            vec![row.clone(), row],
        );
        let result = layout(&outer, Constraint::new(100, 50));
        // outer at index 0; row1 at 1; row1 children at 2,3; row2 at 4; row2 children at 5,6.
        assert_eq!(result.len(), 7);
        assert_eq!(result.sizes[0], OccupiedSize::new(100, 50));
        // row1 horizontal: child positions absolute within outer
        assert_eq!(result.positions[2], Position::new(0, 0));
        assert_eq!(result.positions[3], Position::new(21, 0));
        // row2 starts after row1 + gap = 10 + 1 = 11
        assert_eq!(result.positions[4].y, 11);
        assert_eq!(result.positions[5], Position::new(0, 11));
    }

    #[test]
    fn layout_is_deterministic() {
        // Theorem 2: same input → same output, every time.
        let tree = Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::vertical(),
            vec![leaf_fix(10, 5), leaf_fix(15, 5), leaf_fix(20, 5)],
        );
        let viewport = Constraint::new(100, 100);
        let r1 = layout(&tree, viewport);
        let r2 = layout(&tree, viewport);
        let r3 = layout(&tree, viewport);
        assert_eq!(r1.positions, r2.positions);
        assert_eq!(r2.positions, r3.positions);
        assert_eq!(r1.sizes, r2.sizes);
    }

    #[test]
    fn empty_stack_lays_out_only_self() {
        let stack = Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::vertical(),
            vec![],
        );
        let result = layout(&stack, Constraint::new(100, 100));
        assert_eq!(result.len(), 1);
        assert_eq!(result.sizes[0], OccupiedSize::new(100, 100));
    }

    #[test]
    fn scale_resolves_proportionally() {
        let leaf = Element::leaf(Size::new(
            SizeType::Scale { ratio: 0.5, min: 0 },
            SizeType::Scale { ratio: 0.25, min: 0 },
        ));
        let result = layout(&leaf, Constraint::new(100, 100));
        assert_eq!(result.sizes[0], OccupiedSize::new(50, 25));
    }

    #[test]
    fn scale_respects_minimum() {
        let leaf = Element::leaf(Size::new(
            SizeType::Scale { ratio: 0.1, min: 20 },
            SizeType::Fix(5),
        ));
        let result = layout(&leaf, Constraint::new(100, 100));
        // 0.1 * 100 = 10, but min is 20.
        assert_eq!(result.sizes[0].w, 20);
    }
}
