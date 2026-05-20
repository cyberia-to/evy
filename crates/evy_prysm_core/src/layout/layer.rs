//! Layer container layout: organelles share the same spatial region,
//! ordered by depth.
//!
//! Per prysm/layout.md §6.3. Layer is the depth container — every child
//! gets the same `(x, y)` as the parent and the full constraint; they
//! differ in `z`.
//!
//! Session 7.2 scope: child index is the depth. The urgency function U
//! (urgency tiers: ambient/persistent/active/interrupting/guiding/blocking)
//! is a higher-level concern that maps onto z; the core protocol just
//! exposes z and lets atoms/molecules write it from urgency.

use super::{layout_recursive, LayoutResult};
use crate::constraint::{Constraint, OccupiedSize, Position};
use crate::element::Element;

pub(super) fn layout(
    element: &Element,
    parent_size: OccupiedSize,
    parent_pos: Position,
    result: &mut LayoutResult,
    next_id: &mut usize,
) {
    let constraint = Constraint::new(parent_size.w, parent_size.h);
    for (i, child) in element.children.iter().enumerate() {
        let z = i as u32;
        let child_pos = Position {
            x: parent_pos.x,
            y: parent_pos.y,
            z: parent_pos.z + z,
        };
        layout_recursive(child, constraint, child_pos, result, next_id);
    }
}

#[cfg(test)]
mod tests {
    use crate::container::Container;
    use crate::element::Element;
    use crate::layout::layout;
    use crate::sizing::{Size, SizeType};
    use crate::Constraint;

    fn leaf(w: u32, h: u32) -> Element {
        Element::leaf(Size::new(SizeType::Fix(w), SizeType::Fix(h)))
    }

    #[test]
    fn layer_stacks_children_by_index() {
        let l = Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::Layer,
            vec![leaf(10, 10), leaf(10, 10), leaf(10, 10)],
        );
        let r = layout(&l, Constraint::new(100, 100));
        assert_eq!(r.len(), 4);
        // All three children at parent origin (0,0) in 2D.
        assert_eq!(r.positions[1].x, 0);
        assert_eq!(r.positions[2].x, 0);
        assert_eq!(r.positions[3].x, 0);
        // z increments with child index.
        assert_eq!(r.positions[1].z, 0);
        assert_eq!(r.positions[2].z, 1);
        assert_eq!(r.positions[3].z, 2);
    }

    #[test]
    fn layer_gives_full_constraint_to_each_child() {
        let l = Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::Layer,
            vec![Element::leaf(Size::new(SizeType::fill(), SizeType::fill()))],
        );
        let r = layout(&l, Constraint::new(80, 60));
        // The single child gets the full 80x60.
        assert_eq!(r.sizes[1].w, 80);
        assert_eq!(r.sizes[1].h, 60);
    }

    #[test]
    fn layer_inherits_parent_z() {
        // Nested layers should accumulate z.
        let inner = Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::Layer,
            vec![leaf(5, 5), leaf(5, 5)],
        );
        let outer = Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::Layer,
            vec![leaf(5, 5), inner],
        );
        let r = layout(&outer, Constraint::new(50, 50));
        // outer:0, leaf at outer's child 0 → z=0, inner at outer's child 1 → z=1
        // inner's children inherit z=1 + their own index 0,1 → z=1, z=2
        // Indices: outer=0, leaf=1, inner=2, inner-leaf1=3, inner-leaf2=4
        assert_eq!(r.positions[1].z, 0);
        assert_eq!(r.positions[2].z, 1);
        assert_eq!(r.positions[3].z, 1);
        assert_eq!(r.positions[4].z, 2);
    }

    #[test]
    fn layer_empty_lays_out_only_self() {
        let l = Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::Layer,
            vec![],
        );
        let r = layout(&l, Constraint::new(100, 100));
        assert_eq!(r.len(), 1);
    }
}
