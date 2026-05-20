//! `DispatchCtx` — the runtime context handed to each node's `dispatch`.
//!
//! Holds the shared `ShardStorage`, per-frame/tick counters, and shared
//! references to the engine task pools (`AmxTaskPool`, `AneTaskPool`).
//! Nodes that want to do work on a specific engine call
//! `ctx.amx_pool.unwrap().spawn(...)` or `ctx.ane_pool.unwrap().spawn(...)`
//! directly; the scheduler doesn't force-route work.
//!
//! Future revisions will add a Gpu device handle once the aruminium
//! device-sharing proposal lands (step 3).

use evy_ecs_storage::ShardStorage;
use evy_engine_tasks::{AmxTaskPool, AneTaskPool};

/// Per-dispatch context. One per scheduler tick; passed to every node.
pub struct DispatchCtx<'a> {
    /// The unified storage that every node reads and writes through.
    pub storage: &'a mut ShardStorage,
    /// Monotonic frame counter. Increments per render frame (e.g. 60–120 Hz).
    pub frame: u64,
    /// Monotonic tick counter. Increments per gameplay tick (10–30 Hz);
    /// at tick boundary `BbgDimension` / `BbgPrivate` writes commit.
    pub tick: u64,
    /// AMX worker pool, if the platform supports it. None on non-Apple-Silicon
    /// or when the scheduler fails to initialize the pool.
    pub amx_pool: Option<&'a AmxTaskPool>,
    /// ANE worker pool, if the platform supports it. None on non-Apple-Silicon
    /// or when the scheduler fails to initialize the pool.
    pub ane_pool: Option<&'a AneTaskPool>,
}

impl<'a> DispatchCtx<'a> {
    /// Construct with no engine pools. Use `with_pools` to add them, or
    /// let the `DispatchScheduler` build the ctx for you.
    pub fn new(storage: &'a mut ShardStorage, frame: u64, tick: u64) -> Self {
        Self {
            storage,
            frame,
            tick,
            amx_pool: None,
            ane_pool: None,
        }
    }

    /// Builder: attach the AMX pool reference.
    pub fn with_amx_pool(mut self, pool: &'a AmxTaskPool) -> Self {
        self.amx_pool = Some(pool);
        self
    }

    /// Builder: attach the ANE pool reference.
    pub fn with_ane_pool(mut self, pool: &'a AneTaskPool) -> Self {
        self.ane_pool = Some(pool);
        self
    }
}
