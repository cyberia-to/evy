//! Dialects — particle-identified schemas for component types.
//!
//! A dialect is a [[particle]] whose content describes a component type's
//! structure. Two machines that register the same Rust type get the same
//! dialect; two machines that disagree about the schema (different field
//! names or types) get different dialects, and the mismatch is detectable.
//!
//! Per `evy/specs/evy.md` §4.4 and `cyber/root/neural.md`, dialects are
//! the grammar of the cybergraph — mutual agreement to use the same
//! particles for structuring thought.
//!
//! Session 1 scope:
//! - `Dialect` type alias for `ParticleId` (32-byte hemera hash)
//! - `HasDialect` trait for types that declare a dialect
//! - `DialectRegistry` for runtime registration and lookup
//! - Helpers to derive a dialect deterministically from a type signature
//!   string or a struct field signature
//!
//! Session 2 will add a `#[derive(EvyDialect)]` macro that synthesizes
//! `HasDialect` from the derive input, and bevy_reflect integration
//! that registers dialects for every reflectable type.

mod registry;
mod signature;

pub use registry::{HasDialect, RegistryEntry, DialectRegistry};
pub use signature::{dialect_from_signature, dialect_from_struct, FieldSignature};

use evy_ecs_storage::ParticleId;

/// A dialect: 32-byte particle identifying a component
/// type's structure. Same alias as `ParticleId` — dialects are particles.
pub type Dialect = ParticleId;
