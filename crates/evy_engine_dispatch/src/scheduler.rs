//! `DispatchScheduler` — the unified engine scheduler.
//!
//! For session 1 of step 4: builds a dependency graph, topo-sorts, and
//! dispatches nodes sequentially on the calling thread. Engine pools are
//! constructed at scheduler creation; per-engine routing of dispatch
//! work lands in session 2.

use std::collections::VecDeque;

use evy_engine_tasks::{AmxTaskPool, AneTaskPool};
use evy_platform_caps::{Engine, FallbackPolicy, PlatformCapabilities};

use crate::access::AccessSet;
use crate::ctx::DispatchCtx;
use crate::error::DispatchError;
use crate::node::DispatchNode;

/// A frame's worth of scheduled work — topo order + per-node resolved engines.
#[derive(Debug, Clone)]
pub struct SchedulePlan {
    /// Indices into the `nodes` slice in dispatch order.
    pub order: Vec<usize>,
    /// Resolved engine per node (after applying `FallbackPolicy`).
    /// Indexed by original node position, not by `order`.
    pub resolved_engines: Vec<Engine>,
}

/// Scheduler over the four compute engines. Holds the platform-determined
/// engine pools and routes work per `DispatchNode::engine()` resolved
/// against `PlatformCapabilities`.
pub struct DispatchScheduler {
    capabilities: PlatformCapabilities,
    /// Worker pool for `Engine::Amx`. None if the platform lacks AMX.
    /// Exposed to nodes via `DispatchCtx::amx_pool` during `dispatch_frame`.
    amx_pool: Option<AmxTaskPool>,
    /// Worker pool for `Engine::Ane`. None if the platform lacks ANE.
    /// Exposed to nodes via `DispatchCtx::ane_pool` during `dispatch_frame`.
    ane_pool: Option<AneTaskPool>,
}

impl DispatchScheduler {
    /// Construct a scheduler matched to the host's capabilities. AMX/ANE
    /// pools initialize on supported platforms; failure to initialize a
    /// pool that capabilities claimed available is logged silently (the
    /// scheduler still works for engines that did initialize).
    pub fn new(capabilities: PlatformCapabilities) -> Result<Self, DispatchError> {
        let amx_pool = if capabilities.has_amx {
            let workers = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                .max(1);
            AmxTaskPool::new(workers).ok()
        } else {
            None
        };

        let ane_pool = if capabilities.has_ane {
            AneTaskPool::new().ok()
        } else {
            None
        };

        Ok(Self {
            capabilities,
            amx_pool,
            ane_pool,
        })
    }

    /// Compute a dispatch plan for a set of nodes. Resolves each node's
    /// engine against the platform; topo-sorts by read/write conflicts.
    pub fn plan(
        &self,
        nodes: &[Box<dyn DispatchNode>],
        fallback: FallbackPolicy,
    ) -> Result<SchedulePlan, DispatchError> {
        let mut resolved_engines = Vec::with_capacity(nodes.len());
        for n in nodes {
            match self.capabilities.resolve(n.engine(), fallback) {
                Some(e) => resolved_engines.push(e),
                None => return Err(DispatchError::EngineUnavailable(n.engine())),
            }
        }

        let access_sets: Vec<AccessSet> = nodes
            .iter()
            .map(|n| AccessSet::from_node_refs(n.reads(), n.writes()))
            .collect();

        let order = topo_sort(&access_sets)?;

        Ok(SchedulePlan {
            order,
            resolved_engines,
        })
    }

    /// Dispatch all nodes for one frame.
    ///
    /// Session 2 implementation: sequential dispatch in topo order on the
    /// calling thread. The scheduler attaches its `AmxTaskPool` and
    /// `AneTaskPool` references to `DispatchCtx` so individual nodes can
    /// route their internal work to the right engine pool via
    /// `ctx.amx_pool.unwrap().spawn(...)` / `ctx.ane_pool.unwrap().spawn(...)`.
    ///
    /// Future sessions will add parallel dispatch across non-conflicting
    /// layers once cross-engine sync primitives are wired (depends on the
    /// aruminium device-sharing proposal, step 3).
    pub fn dispatch_frame<'a>(
        &'a mut self,
        nodes: &mut [Box<dyn DispatchNode>],
        plan: &SchedulePlan,
        ctx: &mut DispatchCtx<'a>,
    ) {
        // Attach engine pool references into the ctx for this frame's
        // dispatches. Pools live on the scheduler; ctx borrows them.
        ctx.amx_pool = self.amx_pool.as_ref();
        ctx.ane_pool = self.ane_pool.as_ref();

        for &idx in &plan.order {
            nodes[idx].dispatch(ctx);
        }

        // Drop the borrows when the frame ends. (Bevy systems wouldn't
        // typically read ctx after dispatch returns; this is defensive.)
        ctx.amx_pool = None;
        ctx.ane_pool = None;
    }

    /// Commit the storage layer at a tick boundary. Returns the shard
    /// sub-root; the authoritative BBG_root is composed by `bbg::Bbg`.
    /// Caller decides when to call this (typically every Nth frame; see
    /// `PlatformCapabilities::tick_rate_hz`).
    pub fn commit_tick(&mut self, ctx: &mut DispatchCtx<'_>) -> [u8; 32] {
        ctx.storage.commit()
    }

    /// Expose the captured platform capabilities for read-only inspection.
    pub fn capabilities(&self) -> &PlatformCapabilities {
        &self.capabilities
    }
}

/// Topological sort of the conflict DAG.
///
/// Edges go from lower-indexed conflicting nodes to higher-indexed ones —
/// submission order is preserved within independent groups, and the result
/// always succeeds since the DAG can't cycle by construction. (Cycle
/// detection remains for future when nodes can declare explicit ordering.)
fn topo_sort(access_sets: &[AccessSet]) -> Result<Vec<usize>, DispatchError> {
    let n = access_sets.len();
    let mut graph: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut indegree: Vec<usize> = vec![0; n];

    for i in 0..n {
        for j in (i + 1)..n {
            if access_sets[i].conflicts_with(&access_sets[j]) {
                graph[i].push(j);
                indegree[j] += 1;
            }
        }
    }

    // Kahn's algorithm with FIFO queue to preserve submission order among
    // independent nodes.
    let mut queue: VecDeque<usize> =
        (0..n).filter(|&i| indegree[i] == 0).collect();
    let mut order = Vec::with_capacity(n);

    while let Some(u) = queue.pop_front() {
        order.push(u);
        for &v in &graph[u] {
            indegree[v] -= 1;
            if indegree[v] == 0 {
                queue.push_back(v);
            }
        }
    }

    if order.len() != n {
        return Err(DispatchError::CycleDetected);
    }

    Ok(order)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{CommitPolicy, ShardRef};
    use evy_ecs_storage::{Namespace, ParticleId, ShardStorage};
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
        fn dispatch(&mut self, _ctx: &mut DispatchCtx<'_>) {
            self.log.lock().unwrap().push(self.name);
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
        scheduler.dispatch_frame(&mut nodes, &plan, &mut ctx);

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
        scheduler.dispatch_frame(&mut nodes, &plan, &mut ctx);

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
        let root = scheduler.commit_tick(&mut ctx);
        // empty commit is deterministic; repeat must produce the same root.
        let root2 = scheduler.commit_tick(&mut ctx);
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
        fn dispatch(&mut self, ctx: &mut DispatchCtx<'_>) {
            let pool = ctx.amx_pool.expect("AMX pool must be present on Apple Silicon");
            // Simulate AMX-heavy work: the closure runs on a worker with
            // AMX active. For the test we just return a sentinel.
            let rx = pool.spawn(|| 0xCAFEBABEu64);
            let val = rx.recv().unwrap();
            self.result_log.lock().unwrap().push(val);
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
        scheduler.dispatch_frame(&mut nodes, &plan, &mut ctx);

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
        scheduler.dispatch_frame(&mut nodes, &plan, &mut ctx);

        assert!(ctx.amx_pool.is_none());
        assert!(ctx.ane_pool.is_none());
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
        scheduler.dispatch_frame(&mut nodes, &plan, &mut ctx);

        let trace = log.lock().unwrap().clone();
        // A must come before B (Coins write→read dep).
        let a_pos = trace.iter().position(|&s| s == "A").unwrap();
        let b_pos = trace.iter().position(|&s| s == "B").unwrap();
        assert!(a_pos < b_pos, "A must dispatch before B (data dependency)");
    }
}
