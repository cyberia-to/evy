//! Constraint, OccupiedSize, Position — the three messages flowing
//! through the membrane–organelle protocol (Π).
//!
//! See prysm/layout.md §4.

/// Maximum size a membrane permits its organelle. Sent downward (membrane → organelle).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Constraint {
    /// Maximum width in quanta.
    pub w: u32,
    /// Maximum height in quanta.
    pub h: u32,
}

impl Constraint {
    pub const fn new(w: u32, h: u32) -> Self {
        Self { w, h }
    }
}

/// Size the organelle actually occupies within its constraint.
/// Sent upward (organelle → membrane). Invariant I4: `w <= constraint.w` and `h <= constraint.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct OccupiedSize {
    pub w: u32,
    pub h: u32,
}

impl OccupiedSize {
    pub const fn new(w: u32, h: u32) -> Self {
        Self { w, h }
    }
}

/// Position assigned by membrane. Origin is membrane-local. Sent downward.
///
/// `z` is the depth index for Layer containers (and the only field that
/// can be nonzero for non-Layer placements — Stack and Grid leave `z = 0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Position {
    pub x: u32,
    pub y: u32,
    pub z: u32,
}

impl Position {
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y, z: 0 }
    }
}
