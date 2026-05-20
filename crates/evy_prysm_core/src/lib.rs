//! prysm layout protocol — core types and the layout algorithm.
//!
//! Implements [prysm/specs/layout.md](../../../../prysm/root/layout.md)
//! §§3–6: the three axioms (Π protocol, Φ sizing, K containers) and the
//! `layout(tree, viewport) → coordinates` pure function.
//!
//! Pure-Rust algorithm with no dependencies. ECS integration lives in
//! `evy_prysm_atoms` and downstream crates that map these types to
//! Bevy components.
//!
//! Session 1 scope:
//! - Core types (Quantum, Constraint, OccupiedSize, Position, SizeType)
//! - Stack container (horizontal + vertical)
//! - The Π protocol (constrain → occupy → place) for stacks
//! - Determinism + O(n) properties verified by tests
//!
//! Session 2 will add Grid and Layer containers. Session 3 adds Fold
//! (responsive conformations). Sessions 4–5 add atoms (glass, text,
//! ion, saber, images) as Bevy bundles on top of these primitives.

mod constraint;
mod container;
mod element;
mod emotion;
mod fold;
mod layout;
mod motion;
mod sizing;

pub use constraint::{Constraint, OccupiedSize, Position};
pub use container::{Align, Container, Direction};
pub use element::{Element, ElementId};
pub use emotion::{
    apply_freshness, continuous, polarity, semantic, threshold, threshold_multi, EmotionColor,
    Freshness, SemanticAction,
};
pub use fold::{Conformation, FoldSet};
pub use layout::{layout, LayoutResult};
pub use motion::{cubic_bezier, ease, Interpolate, MotionState, EASE_X1, EASE_X2, EASE_Y1, EASE_Y2, MOTION_DURATION_MS};
pub use sizing::{Size, SizeType};

/// The spatial quantum `g` per prysm/layout.md §3.2.
///
/// All sizes, positions, and constraints in the protocol are expressed
/// in integer multiples of this constant. The renderer translates
/// `k * QUANTUM` into native pixel units.
pub const QUANTUM: u32 = 8;
