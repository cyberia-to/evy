//! evy ECS storage adapter.
//!
//! Bridges Bevy ECS-style typed components to bbg's `ShardStore` trait.
//! All component instances live in one of 13 namespaces (12 BBG dimensions
//! + EPHEMERAL); the backend (`memory`/`unimem`/`ssd`/`hdd`/`network`) is
//! selected per-deployment from `PlatformCapabilities`.
//!
//! See `evy/specs/evy.md` §4 for the storage model.

mod codec;
mod component;
mod namespace;
mod particle;
mod storage;

pub use codec::GoldilocksCodec;
pub use component::EvyComponent;
pub use namespace::Namespace;
pub use particle::ParticleId;
pub use storage::{ReservePoolError, ShardStorage};

// Re-export the field element type so callers don't need a direct nebu dep.
pub use nebu::Goldilocks;
