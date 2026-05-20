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
mod layout;
mod sizing;

pub use constraint::{Constraint, OccupiedSize, Position};
pub use container::{Align, Container, Direction};
pub use element::{Element, ElementId};
pub use layout::{layout, LayoutResult};
pub use sizing::{Size, SizeType};

/// The spatial quantum `g` per prysm/layout.md §3.2.
///
/// All sizes, positions, and constraints in the protocol are expressed
/// in integer multiples of this constant. The renderer translates
/// `k * QUANTUM` into native pixel units.
pub const QUANTUM: u32 = 8;
