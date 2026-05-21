//! Hello-world for evy. Builds an Engine, registers a component schema,
//! adds a dispatch node that writes per-frame, runs 5 frames, prints state.
//!
//! ```
//! cargo run -p evy_engine_core --example hello
//! ```

use evy_engine_core::{
    CommitPolicy, DispatchCtx, DispatchNode, Engine, EngineKind, EvyComponent, Goldilocks,
    HasSemcon, Namespace, ParticleId, Semcon, ShardRef,
};

/// One game state component — a tick counter for a single particle.
struct GameState {
    tick: u64,
}

impl HasSemcon for GameState {
    // Real impl would call `semcon_from_struct(...)` at init; for a const
    // we just pick a fingerprint byte.
    const SEMCON: Semcon = ParticleId::from_hash([0x01; 32]);
}

impl EvyComponent for GameState {
    const NAMESPACE: Namespace = Namespace::Ephemeral;

    fn to_goldilocks(&self) -> Vec<Goldilocks> {
        vec![Goldilocks::new(self.tick)]
    }

    fn from_goldilocks(slice: &[Goldilocks]) -> Self {
        Self {
            tick: slice[0].as_u64(),
        }
    }
}

/// Dispatch node that bumps the tick counter for one particle every frame.
struct TickCounterNode {
    particle: ParticleId,
}

impl DispatchNode for TickCounterNode {
    fn engine(&self) -> EngineKind {
        EngineKind::Cpu
    }
    fn reads(&self) -> &[ShardRef] {
        &[]
    }
    fn writes(&self) -> &[ShardRef] {
        // Coarse: declare we write the whole ephemeral namespace.
        // Future refinement: ShardRef::Particle(...) for finer parallelism.
        &[]
    }
    fn commit_policy(&self) -> CommitPolicy {
        CommitPolicy::None
    }

    fn dispatch(&mut self, ctx: &mut DispatchCtx<'_>) {
        let next = ctx
            .storage
            .get::<GameState>(self.particle)
            .map(|s| s.tick + 1)
            .unwrap_or(0);
        ctx.storage.put(self.particle, &GameState { tick: next });
    }
}

fn main() {
    println!("evy hello — building engine...");
    let mut engine = Engine::builder()
        .without_radio() // headless example; no networking
        .build()
        .expect("engine build");

    println!("  platform engines: {:?}", engine.capabilities().engines);
    println!(
        "  thermal class:    {:?}",
        engine.capabilities().thermal_budget
    );
    println!(
        "  default tick_hz:  {}",
        engine.capabilities().tick_rate_hz
    );

    // Register the component schema.
    engine
        .semcon_registry_mut()
        .register::<GameState>()
        .expect("semcon registration");
    println!("\nregistered semcons: {}", engine.semcon_registry().len());

    // Add the dispatch node.
    let particle = ParticleId::from_entity(42, 0);
    engine.add_node(Box::new(TickCounterNode { particle }));
    println!("dispatch nodes:     {}", engine.node_count());

    // Run 5 frames.
    println!("\nrunning 5 frames...");
    for _ in 0..5 {
        engine.run_frame().expect("dispatch");
    }
    println!("  frame counter:    {}", engine.frame());

    // Read back the state.
    let state = engine.storage().get::<GameState>(particle).expect("state");
    println!("  particle tick:    {}", state.tick);

    // Commit at tick boundary.
    let root = engine.commit_tick();
    println!("\ntick committed, shard sub-root = {:02x}{:02x}{:02x}{:02x}…",
        root[0], root[1], root[2], root[3]);
    println!("  tick counter:     {}", engine.tick());

    println!("\ndone.");
}
