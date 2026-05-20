//! Sizing axiom Φ: fix, fill, scale.
//!
//! See prysm/layout.md §5.

/// Sizing primitive for one dimension.
///
/// - `Fix(k)` — absolute, intrinsic size `k * QUANTUM`. Does not depend
///   on membrane.
/// - `Fill { weight }` — absorbs remaining space after fix and scale
///   siblings, in proportion to its weight.
/// - `Scale { ratio, min }` — proportional to membrane, clamped to a
///   minimum in quanta.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizeType {
    Fix(u32),
    Fill { weight: f32 },
    Scale { ratio: f32, min: u32 },
}

impl SizeType {
    /// Convenient constructor for unweighted fill.
    pub const fn fill() -> Self {
        Self::Fill { weight: 1.0 }
    }
}

/// 2D sizing for an element. Each dimension is independently fix/fill/scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: SizeType,
    pub height: SizeType,
}

impl Size {
    pub const fn new(width: SizeType, height: SizeType) -> Self {
        Self { width, height }
    }
}
