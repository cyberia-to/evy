//! Stack container layout: organelles along a single axis (horizontal/vertical).

use super::{layout_recursive, resolve::resolve_dim, LayoutResult};
use crate::constraint::{Constraint, OccupiedSize, Position};
use crate::container::{Align, Direction};
use crate::element::Element;
use crate::sizing::SizeType;

/// Place children in a stack. Resolves sizes (fix → scale → fill) along
/// the main axis, then walks children cumulating offset by `gap`.
#[allow(clippy::too_many_arguments)]
pub(super) fn layout(
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

    // Apply fold selection if this membrane has a fold set. Non-active
    // children are skipped during layout but still consume LayoutResult
    // indices (advanced via next_id += child.count()) to keep the result
    // tree-aligned.
    let active: Vec<bool> = match &element.fold_set {
        Some(fs) => match fs.active(main_avail) {
            Some(conf) => {
                let mut a = vec![false; n];
                for &i in &conf.child_indices {
                    if i < n {
                        a[i] = true;
                    }
                }
                a
            }
            None => vec![true; n],
        },
        None => vec![true; n],
    };
    let active_count = active.iter().filter(|&&b| b).count();

    let total_gap = gap.saturating_mul((active_count as u32).saturating_sub(1));
    let usable_main = main_avail.saturating_sub(total_gap);

    // Pass 1: resolve fix + scale on main axis, accumulate consumption,
    // record fills for the remainder.
    let mut sizes_main: Vec<u32> = vec![0; n];
    let mut fill_indices: Vec<usize> = Vec::new();
    let mut fill_weight_total: f32 = 0.0;
    let mut fixed_consumed: u32 = 0;

    for (i, child) in element.children.iter().enumerate() {
        if !active[i] {
            continue;
        }
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

    // Pass 2: distribute remainder across fills by weight; give the
    // last fill the residual to absorb rounding loss.
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
                remainder_main.saturating_sub(distributed)
            } else {
                ((remainder_main as f32) * weight / fill_weight_total).round() as u32
            };
            sizes_main[i] = share;
            distributed = distributed.saturating_add(share);
        }
    }

    // Pass 3: place. Walk children, cumulating offsets along main axis
    // for active children. Inactive children are skipped, but their
    // subtree IDs are still consumed so that LayoutResult indexing stays
    // aligned with the tree's DFS pre-order traversal.
    let mut cursor: u32 = 0;
    for (i, child) in element.children.iter().enumerate() {
        if !active[i] {
            *next_id += child.count();
            continue;
        }
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
    use crate::container::Container;
    use crate::element::Element;
    use crate::layout::layout;
    use crate::sizing::{Size, SizeType};
    use crate::{Constraint, OccupiedSize, Position};

    fn leaf_fix(w: u32, h: u32) -> Element {
        Element::leaf(Size::new(SizeType::Fix(w), SizeType::Fix(h)))
    }
    fn leaf_fill() -> Element {
        Element::leaf(Size::new(SizeType::fill(), SizeType::fill()))
    }

    #[test]
    fn vertical_places_children_in_sequence() {
        let stack = Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::vertical(),
            vec![leaf_fix(20, 10), leaf_fix(20, 5), leaf_fix(20, 8)],
        );
        let r = layout(&stack, Constraint::new(100, 100));
        assert_eq!(r.len(), 4);
        assert_eq!(r.sizes[0], OccupiedSize::new(100, 100));
        assert_eq!(r.positions[1], Position::new(0, 0));
        assert_eq!(r.positions[2], Position::new(0, 11));
        assert_eq!(r.positions[3], Position::new(0, 17));
    }

    #[test]
    fn horizontal_places_along_x() {
        let stack = Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::horizontal(),
            vec![leaf_fix(10, 5), leaf_fix(15, 5)],
        );
        let r = layout(&stack, Constraint::new(100, 20));
        assert_eq!(r.positions[1], Position::new(0, 0));
        assert_eq!(r.positions[2], Position::new(11, 0));
    }

    #[test]
    fn fill_children_split_remainder() {
        let stack = Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::horizontal(),
            vec![leaf_fix(20, 5), leaf_fill(), leaf_fill()],
        );
        let r = layout(&stack, Constraint::new(100, 20));
        assert_eq!(r.sizes[1].w, 20);
        assert_eq!(r.sizes[2].w + r.sizes[3].w, 78);
        assert!((r.sizes[2].w as i64 - 39).abs() <= 1);
    }

    #[test]
    fn nested_composes_correctly() {
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
        let r = layout(&outer, Constraint::new(100, 50));
        assert_eq!(r.len(), 7);
        assert_eq!(r.positions[2], Position::new(0, 0));
        assert_eq!(r.positions[3], Position::new(21, 0));
        assert_eq!(r.positions[4].y, 11);
        assert_eq!(r.positions[5], Position::new(0, 11));
    }

    #[test]
    fn empty_stack_lays_out_only_self() {
        let stack = Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::vertical(),
            vec![],
        );
        let r = layout(&stack, Constraint::new(100, 100));
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn fold_selects_widest_conformation_when_constraint_is_loose() {
        use crate::fold::FoldSet;
        let stack = Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::horizontal(),
            vec![leaf_fix(10, 5), leaf_fix(20, 5), leaf_fix(30, 5)],
        )
        .with_fold_set(FoldSet::derive_optimal(&[10, 20, 30], &[1.0, 2.0, 3.0], 1));

        // Generous constraint: all three should be visible.
        let r = layout(&stack, Constraint::new(100, 20));
        assert_eq!(r.len(), 4);
        assert!(r.sizes[1].w > 0);
        assert!(r.sizes[2].w > 0);
        assert!(r.sizes[3].w > 0);
    }

    #[test]
    fn fold_drops_least_important_when_constraint_shrinks() {
        use crate::fold::FoldSet;
        let stack = Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::horizontal(),
            // Three children — child 0 is least important, child 2 most.
            vec![leaf_fix(10, 5), leaf_fix(20, 5), leaf_fix(30, 5)],
        )
        .with_fold_set(FoldSet::derive_optimal(&[10, 20, 30], &[1.0, 2.0, 3.0], 1));

        // c_w = 55 — fits only [1, 2] (w_min = 20+30+1 = 51).
        let r = layout(&stack, Constraint::new(55, 20));
        assert_eq!(r.sizes[1].w, 0, "least-important child 0 must be hidden");
        assert!(r.sizes[2].w > 0, "child 1 must be visible");
        assert!(r.sizes[3].w > 0, "most-important child 2 must be visible");
    }

    #[test]
    fn fold_keeps_most_important_at_narrowest() {
        use crate::fold::FoldSet;
        let stack = Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::horizontal(),
            vec![leaf_fix(10, 5), leaf_fix(20, 5), leaf_fix(30, 5)],
        )
        .with_fold_set(FoldSet::derive_optimal(&[10, 20, 30], &[1.0, 2.0, 3.0], 1));

        // c_w = 30 — only child 2 (the most important, width 30) fits.
        let r = layout(&stack, Constraint::new(30, 20));
        assert_eq!(r.sizes[1].w, 0);
        assert_eq!(r.sizes[2].w, 0);
        assert!(r.sizes[3].w > 0);
    }

    #[test]
    fn fold_preserves_total_node_count_in_result() {
        use crate::fold::FoldSet;
        let stack = Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::horizontal(),
            vec![leaf_fix(10, 5), leaf_fix(20, 5), leaf_fix(30, 5)],
        )
        .with_fold_set(FoldSet::derive_optimal(&[10, 20, 30], &[1.0, 2.0, 3.0], 1));

        // No matter the constraint, LayoutResult must have one entry per
        // tree node so that downstream code can index by tree position.
        for cw in [10u32, 30, 50, 80, 100] {
            let r = layout(&stack, Constraint::new(cw, 20));
            assert_eq!(r.len(), 4, "tree node count must be preserved at cw={cw}");
        }
    }
}
