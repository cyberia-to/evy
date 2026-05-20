//! The layout function: `(element_tree, viewport) → (positions, sizes)`.
//!
//! Dispatches to per-container modules (`stack`, `grid`, `layer`). The
//! recursive DFS, the `LayoutResult` output type, and the resolver helpers
//! that don't depend on container type live here. Container-specific
//! placement is in the per-container module.
//!
//! Pure, deterministic, single-pass DFS — O(n) for n elements per
//! prysm/layout.md Theorem 1.

mod grid;
mod layer;
mod resolve;
mod stack;

use crate::constraint::{Constraint, OccupiedSize, Position};
use crate::container::Container;
use crate::element::Element;

pub(crate) use resolve::occupy;

/// Output of `layout()`. One entry per element, indexed in DFS pre-order
/// of the input tree (root is index 0).
#[derive(Debug, Clone, Default)]
pub struct LayoutResult {
    pub positions: Vec<Position>,
    pub sizes: Vec<OccupiedSize>,
}

impl LayoutResult {
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }
}

/// Compute layout for `root` within `viewport`.
///
/// `viewport` is the root constraint — `(width, height)` in quanta.
/// The returned `LayoutResult` has one `(position, size)` pair per
/// element in DFS pre-order. Position is measured from the root's origin.
pub fn layout(root: &Element, viewport: Constraint) -> LayoutResult {
    let n = root.count();
    let mut result = LayoutResult {
        positions: vec![Position::default(); n],
        sizes: vec![OccupiedSize::default(); n],
    };
    layout_recursive(root, viewport, Position::default(), &mut result, &mut 0);
    result
}

/// Recursive DFS implementation of the Π protocol.
///
/// Returns the occupied size of `element` after layout — needed by the
/// caller (a membrane) to size siblings and compute its own bounds.
pub(crate) fn layout_recursive(
    element: &Element,
    constraint: Constraint,
    position: Position,
    result: &mut LayoutResult,
    next_id: &mut usize,
) -> OccupiedSize {
    let my_id = *next_id;
    *next_id += 1;

    // OCCUPY (this element fits itself into `constraint`).
    let my_size = occupy(element, constraint);

    // RECORD position + size for self.
    result.positions[my_id] = position;
    result.sizes[my_id] = my_size;

    // CONSTRAIN + PLACE children, if I'm a membrane.
    if let Some(container) = &element.container {
        match container {
            Container::Stack {
                direction,
                gap,
                align,
            } => stack::layout(
                element, my_size, position, *direction, *gap, *align, result, next_id,
            ),
            Container::Grid {
                columns,
                rows,
                col_gap,
                row_gap,
            } => grid::layout(
                element, my_size, position, columns, rows, *col_gap, *row_gap, result, next_id,
            ),
            Container::Layer => {
                layer::layout(element, my_size, position, result, next_id)
            }
        }
    }

    my_size
}

#[cfg(test)]
mod tests {
    use super::layout;
    use crate::container::Container;
    use crate::element::Element;
    use crate::sizing::{Size, SizeType};
    use crate::{Constraint, OccupiedSize, Position};

    fn leaf_fix(w: u32, h: u32) -> Element {
        Element::leaf(Size::new(SizeType::Fix(w), SizeType::Fix(h)))
    }
    fn leaf_fill() -> Element {
        Element::leaf(Size::new(SizeType::fill(), SizeType::fill()))
    }

    #[test]
    fn single_leaf_fits_constraint() {
        let r = layout(&leaf_fix(10, 5), Constraint::new(100, 100));
        assert_eq!(r.len(), 1);
        assert_eq!(r.sizes[0], OccupiedSize::new(10, 5));
        assert_eq!(r.positions[0], Position::default());
    }

    #[test]
    fn fill_takes_full_constraint() {
        let r = layout(&leaf_fill(), Constraint::new(50, 30));
        assert_eq!(r.sizes[0], OccupiedSize::new(50, 30));
    }

    #[test]
    fn fix_clamps_to_constraint() {
        let r = layout(&leaf_fix(200, 200), Constraint::new(50, 30));
        assert_eq!(r.sizes[0], OccupiedSize::new(50, 30));
    }

    #[test]
    fn scale_resolves_proportionally() {
        let leaf = Element::leaf(Size::new(
            SizeType::Scale { ratio: 0.5, min: 0 },
            SizeType::Scale { ratio: 0.25, min: 0 },
        ));
        let r = layout(&leaf, Constraint::new(100, 100));
        assert_eq!(r.sizes[0], OccupiedSize::new(50, 25));
    }

    #[test]
    fn scale_respects_minimum() {
        let leaf = Element::leaf(Size::new(
            SizeType::Scale { ratio: 0.1, min: 20 },
            SizeType::Fix(5),
        ));
        let r = layout(&leaf, Constraint::new(100, 100));
        assert_eq!(r.sizes[0].w, 20);
    }

    #[test]
    fn layout_is_deterministic() {
        // Theorem 2: same input → same output, always.
        let tree = Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::vertical(),
            vec![leaf_fix(10, 5), leaf_fix(15, 5), leaf_fix(20, 5)],
        );
        let v = Constraint::new(100, 100);
        let r1 = layout(&tree, v);
        let r2 = layout(&tree, v);
        let r3 = layout(&tree, v);
        assert_eq!(r1.positions, r2.positions);
        assert_eq!(r2.positions, r3.positions);
        assert_eq!(r1.sizes, r2.sizes);
    }
}
