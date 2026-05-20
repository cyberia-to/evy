//! Sizing resolution helpers shared by all container types.

use crate::constraint::{Constraint, OccupiedSize};
use crate::element::Element;
use crate::sizing::SizeType;

/// Resolve one dimension given a `SizeType`, an axis constraint, and the
/// minimum-size hint for that dimension.
///
/// `Fill` resolves to the full constraint here — siblings within a
/// container distribute `Fill` proportionally (each container's pass
/// does its own redistribution). When `occupy` is called for a non-
/// container context (root, leaf), `Fill` behaves as "take all available."
pub(crate) fn resolve_dim(s: &SizeType, c: u32, min: &SizeType) -> u32 {
    let raw = match s {
        SizeType::Fix(k) => *k,
        SizeType::Fill { .. } => c,
        SizeType::Scale { ratio, min: m } => {
            let scaled = ((c as f32) * ratio).round() as u32;
            scaled.max(*m)
        }
    };
    let floor = match min {
        SizeType::Fix(k) => *k,
        _ => 0,
    };
    raw.max(floor).min(c)
}

/// Resolve this element's own occupied size against the constraint.
pub(crate) fn occupy(element: &Element, c: Constraint) -> OccupiedSize {
    let w = resolve_dim(&element.size.width, c.w, &element.min_size.width);
    let h = resolve_dim(&element.size.height, c.h, &element.min_size.height);
    OccupiedSize { w, h }
}
