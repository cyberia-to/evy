//! Element tree — input to the layout algorithm.
//!
//! Elements are nested by `children: Vec<Element>`. The tree is the
//! input; `layout(tree, viewport) → LayoutResult` is the pure function
//! that produces positions and sizes for every node.

use crate::container::Container;
use crate::sizing::Size;

/// Index of an element in a layout result, assigned by depth-first
/// pre-order traversal of the input tree.
pub type ElementId = usize;

/// One node in the element tree.
///
/// Leaves have `container: None` and empty `children`. Membranes (any
/// element that contains organelles) have `container: Some(_)` and one
/// or more children.
#[derive(Debug, Clone)]
pub struct Element {
    pub size: Size,
    /// Minimum viable size in quanta — below this, fold triggers
    /// (session 3). For session 1, used as a floor when Scale resolves
    /// below the requested ratio.
    pub min_size: Size,
    pub container: Option<Container>,
    pub children: Vec<Element>,
}

impl Element {
    /// Construct a leaf with the given sizing.
    pub fn leaf(size: Size) -> Self {
        Self {
            size,
            min_size: Size::new(crate::SizeType::Fix(1), crate::SizeType::Fix(1)),
            container: None,
            children: Vec::new(),
        }
    }

    /// Construct a membrane (container with children).
    pub fn membrane(size: Size, container: Container, children: Vec<Element>) -> Self {
        Self {
            size,
            min_size: Size::new(crate::SizeType::Fix(1), crate::SizeType::Fix(1)),
            container: Some(container),
            children,
        }
    }

    /// Override `min_size` for fold thresholds. Builder-style.
    pub fn with_min(mut self, min: Size) -> Self {
        self.min_size = min;
        self
    }

    /// Count of elements in the subtree rooted at `self`, including self.
    pub fn count(&self) -> usize {
        1 + self.children.iter().map(Element::count).sum::<usize>()
    }
}
