//! `DispatchCtx` — the runtime context handed to each node's `dispatch`.
//!
//! Holds the shared `ShardStorage` plus per-frame/tick counters. Future
//! revisions will add engine-specific handles (Gpu device, AMX pool slot,
//! ANE program pool) once those crates land.

use evy_ecs_storage::ShardStorage;

/// Per-dispatch context. One per scheduler tick; passed to every node.
pub struct DispatchCtx<'a> {
    /// The unified storage that every node reads and writes through.
    pub storage: &'a mut ShardStorage,
    /// Monotonic frame counter. Increments per render frame (e.g. 60–120 Hz).
    pub frame: u64,
    /// Monotonic tick counter. Increments per gameplay tick (10–30 Hz);
    /// at tick boundary `BbgDimension` / `BbgPrivate` writes commit.
    pub tick: u64,
}

impl<'a> DispatchCtx<'a> {
    pub fn new(storage: &'a mut ShardStorage, frame: u64, tick: u64) -> Self {
        Self {
            storage,
            frame,
            tick,
        }
    }
}
