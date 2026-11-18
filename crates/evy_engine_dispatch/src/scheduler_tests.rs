//! Tests for the dispatch scheduler. Included by `scheduler.rs`.

use super::*;
use crate::node::{CommitPolicy, ShardRef};
use evy_ecs_storage::{Namespace, ShardStorage, StorageError};
use evy_platform_caps::Engine;

/// Test node that records its dispatch index in an arc-mutex log.
struct LoggingNode {
    engine: Engine,
    reads: Vec<ShardRef>,
    writes: Vec<ShardRef>,
    commit_policy: CommitPolicy,
    log: std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>,
    name: &'static str,
}

impl LoggingNode {
    fn new(
        name: &'static str,
        engine: Engine,
        reads: Vec<ShardRef>,
        writes: Vec<ShardRef>,
        commit_policy: CommitPolicy,
        log: std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>,
    ) -> Self {
        Self {
            name,
            engine,
            reads,
            writes,
            commit_policy,
            log,
        }
    }
}

impl DispatchNode for LoggingNode {
    fn engine(&self) -> Engine {
        self.engine
    }
    fn reads(&self) -> &[ShardRef] {
        &self.reads
    }
    fn writes(&self) -> &[ShardRef] {
        &self.writes
    }
    fn commit_policy(&self) -> CommitPolicy {
        self.commit_policy
    }
    fn dispatch(&mut self, _ctx: &mut DispatchCtx<'_>) -> Result<(), DispatchError> {
        self.log.lock().unwrap().push(self.name);
        Ok(())
    }
    fn label(&self) -> &str {
        self.name
    }
}

fn fresh_scheduler() -> DispatchScheduler {
    let caps = PlatformCapabilities::probe();
    DispatchScheduler::new(caps).expect("scheduler init")
}

fn fresh_storage() -> ShardStorage {
    ShardStorage::with_backend(Box::new(bbg::storage::MemStore::new()))
}

fn make_log() -> std::sync::Arc<std::sync::Mutex<Vec<&'static str>>> {
    std::sync::Arc::new(std::sync::Mutex::new(Vec::new()))
}

#[test]
fn independent_nodes_run_in_submission_order() {
    let log = make_log();
    let scheduler = fresh_scheduler();

    let nodes: Vec<Box<dyn DispatchNode>> = vec![
        Box::new(LoggingNode::new(
            "a", Engine::Cpu, vec![], vec![ShardRef::Namespace(Namespace::Particles)],
            CommitPolicy::None, log.clone(),
        )),
        Box::new(LoggingNode::new(
            "b", Engine::Cpu, vec![], vec![ShardRef::Namespace(Namespace::Coins)],
            CommitPolicy::None, log.clone(),
        )),
        Box::new(LoggingNode::new(
            "c", Engine::Cpu, vec![], vec![ShardRef::Namespace(Namespace::Cards)],
            CommitPolicy::None, log.clone(),
        )),
    ];

    let plan = scheduler
        .plan(&nodes, FallbackPolicy::DegradeTo(Engine::Cpu))
        .unwrap();
    let mut storage = fresh_storage();
    let mut ctx = DispatchCtx::new(&mut storage, 0, 0);
    let mut scheduler = scheduler;
    let mut nodes = nodes;
    scheduler.dispatch_frame(&mut nodes, &plan, &mut ctx).unwrap();

    assert_eq!(*log.lock().unwrap(), vec!["a", "b", "c"]);
}

#[test]
fn write_dependency_orders_reader_after_writer() {
    let log = make_log();
    let scheduler = fresh_scheduler();

    // node "reader" reads Coins; node "writer" writes Coins.
    // Reader is submitted FIRST but must execute AFTER writer.
    //
    // Wait — actually the rule is: in topo sort, the WRITER (earlier
    // submission index) precedes the READER (later submission index)
    // because the edge points from writer to reader. Submission order
    // would be writer-first; let's flip it: reader-first, writer-second.
    // The topo sort should THEN reorder them: writer runs first because
    // its access edge from itself to the reader does NOT exist
    // (we only add edges i->j for i < j when they conflict). So with
    // reader at index 0 and writer at index 1, the edge points from
    // reader (0) to writer (1), and topo gives [0, 1] — reader first.
    //
    // To genuinely test ordering, submit writer first then reader,
    // verify the topo respects the data dependency. With our directional
    // edge convention (i->j when i<j and they conflict), this gives
    // edge 0->1, and topo yields [0, 1] = writer, reader. Correct.
    let nodes: Vec<Box<dyn DispatchNode>> = vec![
        Box::new(LoggingNode::new(
            "writer", Engine::Cpu,
            vec![],
            vec![ShardRef::Namespace(Namespace::Coins)],
            CommitPolicy::BbgDimension(Namespace::Coins),
            log.clone(),
        )),
        Box::new(LoggingNode::new(
            "reader", Engine::Cpu,
            vec![ShardRef::Namespace(Namespace::Coins)],
            vec![],
            CommitPolicy::None,
            log.clone(),
        )),
    ];

    let plan = scheduler
        .plan(&nodes, FallbackPolicy::DegradeTo(Engine::Cpu))
        .unwrap();

    let mut storage = fresh_storage();
    let mut ctx = DispatchCtx::new(&mut storage, 0, 0);
    let mut scheduler = scheduler;
    let mut nodes = nodes;
    scheduler.dispatch_frame(&mut nodes, &plan, &mut ctx).unwrap();

    assert_eq!(*log.lock().unwrap(), vec!["writer", "reader"]);
}

#[test]
fn engine_unavailable_with_required_policy_errors() {
    // Construct fake caps that lack ANE.
    let no_ane = PlatformCapabilities {
        engines: [Engine::Cpu].into_iter().collect(),
        has_unimem: false,
        has_amx: false,
        has_ane: false,
        npu_kind: None,
        gpu_paths: [evy_platform_caps::GpuPath::Wgpu].into_iter().collect(),
        memory_class: evy_platform_caps::MemoryClass::NonUma,
        thermal_budget: evy_platform_caps::ThermalBudget::Desktop,
        tick_rate_hz: 30,
    };
    let scheduler = DispatchScheduler::new(no_ane).unwrap();

    let nodes: Vec<Box<dyn DispatchNode>> = vec![Box::new(LoggingNode::new(
        "ane_inference",
        Engine::Ane,
        vec![],
        vec![],
        CommitPolicy::None,
        make_log(),
    ))];

    let result = scheduler.plan(&nodes, FallbackPolicy::Required);
    assert!(matches!(
        result,
        Err(DispatchError::EngineUnavailable(Engine::Ane))
    ));
}

#[test]
fn engine_unavailable_with_degrade_falls_back() {
    let no_ane = PlatformCapabilities {
        engines: [Engine::Cpu].into_iter().collect(),
        has_unimem: false,
        has_amx: false,
        has_ane: false,
        npu_kind: None,
        gpu_paths: [evy_platform_caps::GpuPath::Wgpu].into_iter().collect(),
        memory_class: evy_platform_caps::MemoryClass::NonUma,
        thermal_budget: evy_platform_caps::ThermalBudget::Desktop,
        tick_rate_hz: 30,
    };
    let scheduler = DispatchScheduler::new(no_ane).unwrap();

    let nodes: Vec<Box<dyn DispatchNode>> = vec![Box::new(LoggingNode::new(
        "ane_inference",
        Engine::Ane,
        vec![],
        vec![],
        CommitPolicy::None,
        make_log(),
    ))];

    let plan = scheduler
        .plan(&nodes, FallbackPolicy::DegradeTo(Engine::Cpu))
        .unwrap();
    assert_eq!(plan.resolved_engines, vec![Engine::Cpu]);
}

#[test]
fn commit_tick_round_trips_storage() {
    let scheduler = fresh_scheduler();
    let mut scheduler = scheduler;
    let mut storage = fresh_storage();
    let mut ctx = DispatchCtx::new(&mut storage, 0, 0);
    let root = scheduler.commit_tick(&mut ctx).unwrap();
    // empty commit is deterministic; repeat must produce the same root.
    let root2 = scheduler.commit_tick(&mut ctx).unwrap();
    assert_eq!(root, root2);
}

/// Node that spawns AMX work via the pool reference in ctx and writes
/// the result into ShardStorage. Demonstrates session 2 routing.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
struct AmxComputeNode {
    result_log: std::sync::Arc<std::sync::Mutex<Vec<u64>>>,
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
impl DispatchNode for AmxComputeNode {
    fn engine(&self) -> Engine {
        Engine::Amx
    }
    fn reads(&self) -> &[ShardRef] {
        &[]
    }
    fn writes(&self) -> &[ShardRef] {
        &[]
    }
    fn commit_policy(&self) -> CommitPolicy {
        CommitPolicy::None
    }
    fn dispatch(&mut self, ctx: &mut DispatchCtx<'_>) -> Result<(), DispatchError> {
        let pool = ctx.amx_pool.expect("AMX pool must be present on Apple Silicon");
        // Simulate AMX-heavy work: the closure runs on a worker with
        // AMX active. For the test we just return a sentinel.
        let rx = pool.spawn(|| 0xCAFEBABEu64);
        let val = rx.recv().unwrap();
        self.result_log.lock().unwrap().push(val);
        Ok(())
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn node_can_spawn_work_on_amx_pool_via_ctx() {
    let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let nodes: Vec<Box<dyn DispatchNode>> =
        vec![Box::new(AmxComputeNode { result_log: log.clone() })];

    let mut scheduler = fresh_scheduler();
    let plan = scheduler
        .plan(&nodes, FallbackPolicy::Required)
        .unwrap();
    let mut storage = fresh_storage();
    let mut ctx = DispatchCtx::new(&mut storage, 0, 0);
    let mut nodes = nodes;
    scheduler.dispatch_frame(&mut nodes, &plan, &mut ctx).unwrap();

    assert_eq!(*log.lock().unwrap(), vec![0xCAFEBABEu64]);
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn ctx_pools_are_none_outside_dispatch_frame() {
    // After dispatch_frame returns, the ctx's pool references are
    // cleared. Calling dispatch_frame again re-attaches them.
    let mut scheduler = fresh_scheduler();
    let mut storage = fresh_storage();
    let mut ctx = DispatchCtx::new(&mut storage, 0, 0);
    let nodes: Vec<Box<dyn DispatchNode>> = vec![];
    let plan = scheduler.plan(&nodes, FallbackPolicy::Skip).unwrap();
    let mut nodes = nodes;
    scheduler.dispatch_frame(&mut nodes, &plan, &mut ctx).unwrap();

    assert!(ctx.amx_pool.is_none());
    assert!(ctx.ane_pool.is_none());
}

/// Node that reports whether the AMX pool was attached, then fails.
struct FailingNode {
    log: std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>,
}

impl DispatchNode for FailingNode {
    fn engine(&self) -> Engine {
        Engine::Cpu
    }
    fn reads(&self) -> &[ShardRef] {
        &[]
    }
    fn writes(&self) -> &[ShardRef] {
        &[]
    }
    fn commit_policy(&self) -> CommitPolicy {
        CommitPolicy::None
    }
    fn dispatch(&mut self, ctx: &mut DispatchCtx<'_>) -> Result<(), DispatchError> {
        if ctx.amx_pool.is_some() {
            self.log.lock().unwrap().push("amx");
        }
        Err(StorageError::Io("injected dispatch failure".into()).into())
    }
}

#[test]
fn ctx_pools_are_released_when_dispatch_fails() {
    // The error path returns early and must clear the pool borrows the
    // success path clears. This is a sibling of
    // `ctx_pools_are_none_outside_dispatch_frame` rather than an extension
    // of it: `dispatch_frame<'a>(&'a mut self, .., &mut DispatchCtx<'a>)`
    // holds the scheduler borrowed for the ctx's own lifetime, so one ctx
    // cannot serve two dispatch_frame calls.
    let log = make_log();
    let mut scheduler = fresh_scheduler();
    let mut storage = fresh_storage();
    let mut ctx = DispatchCtx::new(&mut storage, 0, 0);
    let mut nodes: Vec<Box<dyn DispatchNode>> =
        vec![Box::new(FailingNode { log: log.clone() })];
    let plan = scheduler.plan(&nodes, FallbackPolicy::Skip).unwrap();

    assert!(matches!(
        scheduler.dispatch_frame(&mut nodes, &plan, &mut ctx),
        Err(DispatchError::Storage(StorageError::Io(_)))
    ));
    assert!(ctx.amx_pool.is_none());
    assert!(ctx.ane_pool.is_none());
    // On Apple Silicon the pools exist, so the node saw them attached and
    // the assertions above record a release, not a pool that was never set.
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    assert_eq!(*log.lock().unwrap(), vec!["amx"]);
}

#[test]
fn parallel_chains_preserve_independent_order() {
    // Two independent write chains. Schedule must respect within-chain
    // ordering (A writes Coins, then B reads Coins → B after A) but
    // doesn't care across chains (X writes Cards, can be anywhere).
    let log = make_log();
    let scheduler = fresh_scheduler();

    let nodes: Vec<Box<dyn DispatchNode>> = vec![
        Box::new(LoggingNode::new("A", Engine::Cpu, vec![],
            vec![ShardRef::Namespace(Namespace::Coins)],
            CommitPolicy::BbgDimension(Namespace::Coins), log.clone())),
        Box::new(LoggingNode::new("X", Engine::Cpu, vec![],
            vec![ShardRef::Namespace(Namespace::Cards)],
            CommitPolicy::BbgDimension(Namespace::Cards), log.clone())),
        Box::new(LoggingNode::new("B", Engine::Cpu,
            vec![ShardRef::Namespace(Namespace::Coins)], vec![],
            CommitPolicy::None, log.clone())),
    ];

    let plan = scheduler
        .plan(&nodes, FallbackPolicy::DegradeTo(Engine::Cpu))
        .unwrap();
    let mut storage = fresh_storage();
    let mut ctx = DispatchCtx::new(&mut storage, 0, 0);
    let mut scheduler = scheduler;
    let mut nodes = nodes;
    scheduler.dispatch_frame(&mut nodes, &plan, &mut ctx).unwrap();

    let trace = log.lock().unwrap().clone();
    // A must come before B (Coins write→read dep).
    let a_pos = trace.iter().position(|&s| s == "A").unwrap();
    let b_pos = trace.iter().position(|&s| s == "B").unwrap();
    assert!(a_pos < b_pos, "A must dispatch before B (data dependency)");
}
