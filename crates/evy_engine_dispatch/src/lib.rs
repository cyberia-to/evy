//! evy unified dispatch DAG.
//!
//! One trait, one scheduler. Render-graph nodes and BBG signal-handler
//! nodes are the same shape (`DispatchNode`) and live in the same
//! topology. They differ only in `CommitPolicy`:
//!
//! - `CommitPolicy::None` — render and scratch nodes; writes stay local
//! - `CommitPolicy::BbgDimension(_)` — gameplay-state nodes; writes
//!   recommit BBG_poly + emit zheng proof at tick boundary
//! - `CommitPolicy::BbgPrivate` — private-record nodes (A(x) extension)
//!
//! See `evy/specs/evy.md` §6.

mod access;
mod ctx;
mod error;
mod node;
mod scheduler;

pub use access::AccessSet;
pub use ctx::DispatchCtx;
pub use error::DispatchError;
pub use node::{CommitPolicy, DispatchNode, ShardRef};
pub use scheduler::{DispatchScheduler, SchedulePlan};

// Re-export so callers don't need to reach into evy_platform_caps.
pub use evy_platform_caps::{Engine, FallbackPolicy, GpuPath, PlatformCapabilities};
// Re-export storage types that DispatchNodes consume.
pub use evy_ecs_storage::{Namespace, ParticleId, ShardStorage};
