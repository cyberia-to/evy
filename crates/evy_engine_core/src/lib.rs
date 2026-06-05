//! evy_engine_core — the assembly. Single entry point that wires every
//! other evy crate into a usable engine.
//!
//! ```ignore
//! use evy_engine_core::Engine;
//!
//! let mut engine = Engine::builder().build()?;
//! engine.add_node(my_dispatch_node);
//! engine.run_frame()?;
//! engine.commit_tick();
//! ```
//!
//! The engine holds:
//! - `PlatformCapabilities` — what hardware features are available
//! - `ShardStorage` — the unified storage layer (bbg::ShardStore impl)
//! - `DispatchScheduler` — the multi-engine compute DAG scheduler
//! - `DialectRegistry` — component schema agreement
//! - `Diagnostic` — per-system measurements
//! - `Option<RadioClient>` — networking, when the daemon is spawned
//!
//! For the v0 assembly we hold ownership of all subsystems and expose
//! coarse methods (`run_frame`, `commit_tick`, `shutdown`). Future
//! versions integrate with Bevy `App` as a resource plugin.

mod builder;
mod engine;

pub use builder::EngineBuilder;
pub use engine::Engine;

// Re-export the foundational types so callers can `use evy_engine_core::*`
// and get everything they need.
pub use evy_diagnostic::{Diagnostic, PmuProbe, ProbeMode, SystemMetrics, Timer};
pub use evy_ecs_storage::{
    EvyComponent, Goldilocks, GoldilocksCodec, Namespace, ParticleId, ShardStorage,
};
pub use evy_engine_dispatch::{
    AccessSet, CommitPolicy, DispatchCtx, DispatchError, DispatchNode, DispatchScheduler, Engine as EngineKind,
    FallbackPolicy, GpuPath, PlatformCapabilities, SchedulePlan, ShardRef,
};
pub use evy_engine_tasks::{AmxTaskPool, AneTaskPool, PoolError};
pub use evy_radio::{
    DaemonHandle, GossipEvent, RadioClient, RadioError, RadioRequest, RadioResponse, RequestId,
};
pub use evy_dialect::{
    dialect_from_signature, dialect_from_struct, FieldSignature, HasDialect, RegistryEntry, Dialect,
    DialectRegistry,
};
