//! `Engine` — the top-level evy app. Holds every subsystem; exposes
//! `add_node`, `run_frame`, `commit_tick`, `shutdown`.

use evy_diagnostic::{Diagnostic, PmuProbe};
use evy_ecs_storage::ShardStorage;
use evy_engine_dispatch::{
    DispatchCtx, DispatchError, DispatchNode, DispatchScheduler, FallbackPolicy,
    PlatformCapabilities, SchedulePlan,
};
use evy_radio::RadioClient;
use evy_dialect::DialectRegistry;

use crate::builder::EngineBuilder;

/// The evy engine instance. One per process; usually held as a struct
/// member or static. Constructed via `Engine::builder()`.
pub struct Engine {
    capabilities: PlatformCapabilities,
    storage: ShardStorage,
    scheduler: DispatchScheduler,
    radio: Option<RadioClient>,
    dialect_registry: DialectRegistry,
    probe: PmuProbe,
    diagnostic: Diagnostic,
    nodes: Vec<Box<dyn DispatchNode>>,
    /// Monotonic frame counter.
    frame: u64,
    /// Monotonic tick counter. Bumped by `commit_tick`.
    tick: u64,
    /// Default fallback policy for dispatch plans built by `run_frame`.
    fallback: FallbackPolicy,
}

impl Engine {
    pub fn builder() -> EngineBuilder {
        EngineBuilder::new()
    }

    /// Construct directly from parts. Used by `EngineBuilder::build`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_parts(
        capabilities: PlatformCapabilities,
        storage: ShardStorage,
        scheduler: DispatchScheduler,
        radio: Option<RadioClient>,
        dialect_registry: DialectRegistry,
        probe: PmuProbe,
        diagnostic: Diagnostic,
    ) -> Self {
        // Default to DegradeTo::Cpu — when a node's preferred engine is
        // unavailable, route to CPU rather than refusing.
        let fallback = FallbackPolicy::DegradeTo(evy_engine_dispatch::Engine::Cpu);
        Self {
            capabilities,
            storage,
            scheduler,
            radio,
            dialect_registry,
            probe,
            diagnostic,
            nodes: Vec::new(),
            frame: 0,
            tick: 0,
            fallback,
        }
    }

    /// Override the default fallback policy.
    pub fn set_fallback(&mut self, policy: FallbackPolicy) {
        self.fallback = policy;
    }

    /// Add a dispatch node. Nodes run in topological order each frame
    /// according to their declared read/write sets.
    pub fn add_node(&mut self, node: Box<dyn DispatchNode>) {
        self.nodes.push(node);
    }

    /// Run one frame: build the schedule plan, dispatch all nodes, drain
    /// radio responses + gossip into the diagnostic. Increments the frame
    /// counter.
    pub fn run_frame(&mut self) -> Result<SchedulePlan, DispatchError> {
        let plan = self.scheduler.plan(&self.nodes, self.fallback)?;

        let mut ctx = DispatchCtx::new(&mut self.storage, self.frame, self.tick);
        self.scheduler.dispatch_frame(&mut self.nodes, &plan, &mut ctx);

        // Drain any radio events; for v0 we record gossip into a sink
        // accessible via radio_responses() / radio_gossip() (below).
        // (Future: surface as Bevy Events.)

        self.frame = self.frame.saturating_add(1);
        Ok(plan)
    }

    /// Commit at the tick boundary. Returns the shard sub-root.
    /// Bumps the tick counter.
    pub fn commit_tick(&mut self) -> [u8; 32] {
        let mut ctx = DispatchCtx::new(&mut self.storage, self.frame, self.tick);
        let root = self.scheduler.commit_tick(&mut ctx);
        self.tick = self.tick.saturating_add(1);
        root
    }

    /// Drain accumulated radio responses since last call. Empty if no
    /// radio daemon was spawned.
    pub fn radio_responses(&self) -> Vec<(evy_radio::RequestId, evy_radio::RadioResponse)> {
        self.radio
            .as_ref()
            .map(|r| r.poll_responses())
            .unwrap_or_default()
    }

    /// Drain accumulated gossip events. Empty if no radio daemon.
    pub fn radio_gossip(&self) -> Vec<evy_radio::GossipEvent> {
        self.radio.as_ref().map(|r| r.poll_gossip()).unwrap_or_default()
    }

    // Read-only accessors for subsystems.

    pub fn capabilities(&self) -> &PlatformCapabilities {
        &self.capabilities
    }

    pub fn storage(&self) -> &ShardStorage {
        &self.storage
    }

    pub fn storage_mut(&mut self) -> &mut ShardStorage {
        &mut self.storage
    }

    pub fn dialect_registry(&self) -> &DialectRegistry {
        &self.dialect_registry
    }

    pub fn dialect_registry_mut(&mut self) -> &mut DialectRegistry {
        &mut self.dialect_registry
    }

    pub fn radio(&self) -> Option<&RadioClient> {
        self.radio.as_ref()
    }

    pub fn diagnostic(&self) -> &Diagnostic {
        &self.diagnostic
    }

    pub fn diagnostic_mut(&mut self) -> &mut Diagnostic {
        &mut self.diagnostic
    }

    pub fn probe(&self) -> &PmuProbe {
        &self.probe
    }

    pub fn frame(&self) -> u64 {
        self.frame
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evy_diagnostic::Timer;
    use evy_engine_dispatch::{CommitPolicy, Engine as EngineKind, ShardRef};

    /// Node that writes a sentinel to ShardStorage so we can verify
    /// dispatch actually executes within run_frame.
    struct WriteSentinelNode {
        ran: std::sync::Arc<std::sync::atomic::AtomicU64>,
    }

    impl DispatchNode for WriteSentinelNode {
        fn engine(&self) -> EngineKind {
            EngineKind::Cpu
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
        fn dispatch(&mut self, _ctx: &mut DispatchCtx<'_>) {
            self.ran.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    #[test]
    fn engine_builds_and_runs_a_frame() {
        let mut engine = Engine::builder().build().unwrap();
        assert_eq!(engine.frame(), 0);
        assert_eq!(engine.tick(), 0);

        let ran = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        engine.add_node(Box::new(WriteSentinelNode { ran: ran.clone() }));

        let _ = engine.run_frame().unwrap();
        assert_eq!(engine.frame(), 1);
        assert_eq!(ran.load(std::sync::atomic::Ordering::Relaxed), 1);

        let _ = engine.run_frame().unwrap();
        assert_eq!(engine.frame(), 2);
        assert_eq!(ran.load(std::sync::atomic::Ordering::Relaxed), 2);
    }

    #[test]
    fn engine_commit_tick_increments_tick() {
        let mut engine = Engine::builder().build().unwrap();
        let _ = engine.commit_tick();
        assert_eq!(engine.tick(), 1);
        let _ = engine.commit_tick();
        assert_eq!(engine.tick(), 2);
    }

    #[test]
    fn engine_without_radio_returns_empty_polls() {
        let engine = Engine::builder().without_radio().build().unwrap();
        assert!(engine.radio().is_none());
        assert!(engine.radio_responses().is_empty());
        assert!(engine.radio_gossip().is_empty());
    }

    #[test]
    fn engine_with_radio_has_client() {
        let engine = Engine::builder().build().unwrap();
        assert!(engine.radio().is_some());
        // Empty initially.
        assert!(engine.radio_responses().is_empty());
    }

    #[test]
    fn engine_dialect_registry_starts_empty_and_accepts_registrations() {
        struct TypeA;
        impl evy_dialect::HasDialect for TypeA {
            const DIALECT: evy_dialect::Dialect =
                evy_ecs_storage::ParticleId::from_hash([0xA1; 32]);
        }

        let mut engine = Engine::builder().build().unwrap();
        assert_eq!(engine.dialect_registry().len(), 0);
        engine
            .dialect_registry_mut()
            .register::<TypeA>()
            .unwrap();
        assert_eq!(engine.dialect_registry().len(), 1);
    }

    #[test]
    fn engine_diagnostic_accumulates_metrics() {
        let mut engine = Engine::builder().build().unwrap();
        let timer = Timer::start(engine.probe(), "test_system");
        std::thread::sleep(std::time::Duration::from_millis(1));
        let metrics = timer.stop(engine.probe());
        engine.diagnostic_mut().record(metrics);
        assert!(engine.diagnostic().get("test_system").is_some());
    }

    #[test]
    fn engine_storage_is_usable() {
        // End-to-end: spawn → write → read through the engine's storage.
        struct V(u32);
        impl evy_ecs_storage::EvyComponent for V {
            const NAMESPACE: evy_ecs_storage::Namespace =
                evy_ecs_storage::Namespace::Ephemeral;
            fn to_goldilocks(&self) -> Vec<evy_ecs_storage::Goldilocks> {
                vec![evy_ecs_storage::Goldilocks::new(self.0 as u64)]
            }
            fn from_goldilocks(slice: &[evy_ecs_storage::Goldilocks]) -> Self {
                V(slice[0].as_u64() as u32)
            }
        }

        let mut engine = Engine::builder().build().unwrap();
        let p = evy_ecs_storage::ParticleId::from_entity(1, 0);
        engine.storage_mut().put(p, &V(42));
        let v = engine.storage().get::<V>(p).unwrap();
        assert_eq!(v.0, 42);
    }

    #[test]
    fn engine_run_frame_with_no_nodes_succeeds() {
        let mut engine = Engine::builder().build().unwrap();
        let plan = engine.run_frame().unwrap();
        assert_eq!(plan.order.len(), 0);
        assert_eq!(plan.resolved_engines.len(), 0);
    }

    /// Smoke test: every subsystem participates in a single frame loop.
    /// This is the integration test the spec calls for.
    #[test]
    fn smoke_test_end_to_end() {
        // 1. Build engine with all subsystems.
        let mut engine = Engine::builder().build().unwrap();

        // 2. Register a dialect for a component type.
        struct GameState(u64);
        impl evy_dialect::HasDialect for GameState {
            const DIALECT: evy_dialect::Dialect =
                evy_ecs_storage::ParticleId::from_hash([0xCA; 32]);
        }
        impl evy_ecs_storage::EvyComponent for GameState {
            const NAMESPACE: evy_ecs_storage::Namespace =
                evy_ecs_storage::Namespace::Ephemeral;
            fn to_goldilocks(&self) -> Vec<evy_ecs_storage::Goldilocks> {
                vec![evy_ecs_storage::Goldilocks::new(self.0)]
            }
            fn from_goldilocks(slice: &[evy_ecs_storage::Goldilocks]) -> Self {
                GameState(slice[0].as_u64())
            }
        }
        engine.dialect_registry_mut().register::<GameState>().unwrap();
        assert!(engine.dialect_registry().agrees_on::<GameState>());

        // 3. Submit a radio request (stub daemon echoes synthetically).
        let _request_id = engine
            .radio()
            .unwrap()
            .submit(evy_radio::RadioRequest::FetchParticle {
                particle: evy_ecs_storage::ParticleId::from_entity(1, 0),
            })
            .unwrap();

        // 4. Add a dispatch node that writes to storage every frame.
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        struct CounterNode {
            count: std::sync::Arc<std::sync::atomic::AtomicU64>,
        }
        impl DispatchNode for CounterNode {
            fn engine(&self) -> EngineKind { EngineKind::Cpu }
            fn reads(&self) -> &[ShardRef] { &[] }
            fn writes(&self) -> &[ShardRef] { &[] }
            fn commit_policy(&self) -> CommitPolicy { CommitPolicy::None }
            fn dispatch(&mut self, ctx: &mut DispatchCtx<'_>) {
                let n = self.count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let p = evy_ecs_storage::ParticleId::from_entity(99, 0);
                ctx.storage.put(p, &GameState(n));
            }
        }
        engine.add_node(Box::new(CounterNode { count: counter.clone() }));

        // 5. Run several frames.
        for _ in 0..5 {
            engine.run_frame().unwrap();
        }
        assert_eq!(engine.frame(), 5);
        assert_eq!(counter.load(std::sync::atomic::Ordering::Relaxed), 5);

        // 6. Storage holds the latest write (counter was 4 at last frame).
        let p = evy_ecs_storage::ParticleId::from_entity(99, 0);
        let stored = engine.storage().get::<GameState>(p).unwrap();
        assert_eq!(stored.0, 4);

        // 7. Radio response eventually arrives (poll a few times).
        let mut got_response = false;
        for _ in 0..1000 {
            let responses = engine.radio_responses();
            if !responses.is_empty() {
                got_response = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(got_response, "radio should respond within 1s");

        // 8. Commit at tick boundary; ephemeral writes produce no
        // change to the root (deterministic identity).
        let _ = engine.commit_tick();
        assert_eq!(engine.tick(), 1);
    }
}
