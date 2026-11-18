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
    ) -> Result<(), DispatchError> {
        // Attach engine pool references into the ctx for this frame's
        // dispatches. Pools live on the scheduler; ctx borrows them.
        ctx.amx_pool = self.amx_pool.as_ref();
        ctx.ane_pool = self.ane_pool.as_ref();

        for &idx in &plan.order {
            if let Err(error) = nodes[idx].dispatch(ctx) {
                ctx.amx_pool = None;
                ctx.ane_pool = None;
                return Err(error);
            }
        }

        // Drop the borrows when the frame ends. (Bevy systems wouldn't
        // typically read ctx after dispatch returns; this is defensive.)
        ctx.amx_pool = None;
        ctx.ane_pool = None;
        Ok(())
    }

    /// Commit the storage layer at a tick boundary. Returns its change identity
    /// or a storage error. BBG computes authenticated state roots independently.
    /// Caller decides when to call this (typically every Nth frame; see
    /// `PlatformCapabilities::tick_rate_hz`).
    pub fn commit_tick(&mut self, ctx: &mut DispatchCtx<'_>) -> evy_ecs_storage::StorageResult<[u8; 32]> {
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
#[path = "scheduler_tests.rs"]
mod tests;
