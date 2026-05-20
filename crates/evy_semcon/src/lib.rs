//! Semantic conventions — particle-identified schemas for component types.
//!
//! A semcon is a [[particle]] whose content describes a component type's
//! structure. Two machines that register the same Rust type get the same
//! semcon; two machines that disagree about the schema (different field
//! names or types) get different semcons, and the mismatch is detectable.
//!
//! Per `evy/specs/evy.md` §4.4 and `cyber/root/neural.md`, semcons are
//! the grammar of the cybergraph — mutual agreement to use the same
//! particles for structuring thought.
//!
//! Session 1 scope:
//! - `Semcon` type alias for `ParticleId` (32-byte hemera hash)
//! - `HasSemcon` trait for types that declare a semcon
//! - `SemconRegistry` for runtime registration and lookup
//! - Helpers to derive a semcon deterministically from a type signature
//!   string or a struct field signature
//!
//! Session 2 will add a `#[derive(EvySemcon)]` macro that synthesizes
//! `HasSemcon` from the derive input, and bevy_reflect integration
//! that registers semcons for every reflectable type.

mod registry;
mod signature;

pub use registry::{HasSemcon, RegistryEntry, SemconRegistry};
pub use signature::{semcon_from_signature, semcon_from_struct, FieldSignature};

use evy_ecs_storage::ParticleId;

/// A semantic convention: 32-byte particle identifying a component
/// type's structure. Same alias as `ParticleId` — semcons are particles.
pub type Semcon = ParticleId;
