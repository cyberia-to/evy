---
tags: evy, cyber, article, core
alias: cyb-engine
crystal-type: pattern
crystal-domain: cyber
crystal-size: deep
---

# evy

a unified-memory, multi-engine, neural-first game engine. selectively forks 16 [[bevy]] crates (in `evy/forks/`); adopts ~20 bevy crates intact from crates.io; replaces 8 with cyber-native primitives. `[patch.crates-io]` redirects transitive deps on forked crates to our versions. powers [[cyb]] and any future cyber-stack game

primary target: [[Apple Silicon]] (M-series). portable targets: any UMA platform with NPU + matrix coprocessor (AMD Strix Halo, Qualcomm X Elite, future ARM SoCs). discrete-GPU PC is a degraded-mode target, not a first-class one

---

## 0. abstract

evy inverts a decades-old game engine architecture. traditional engines treat the GPU as a remote device behind a copy boundary; evy treats memory as the single shared substrate and compute engines (CPU, AMX, GPU, ANE) as plural peers that operate on it concurrently. presentation is one formally specified protocol ([[prysm]]) across 2D and 3D. content is [[content-addressed]] [[particles]] reached over [[radio]]. trust-critical state lives in [[bbg]]'s polynomial commit. on Apple Silicon, [[aruminium]] is the renderer — raster + compute, zero copy, native MTLSharedEvent sync; wgpu is the portable fallback on platforms where Metal is unavailable. intelligence ([[glia]] per [[neuron]]) is a frame-time-affordable resource

what evy buys from [[bevy]]: ~100K LOC of rendering machinery (PBR, post-process, anti-alias, animation, gizmos) that would take a multi-engineer year to reproduce. what evy replaces in [[bevy]]: every assumption below the visual stack — storage, scheduler, presentation, scenes, assets, networking, audio. the result is a new engine that uses [[bevy]] as its rendering substrate

---

## 1. axioms

three axioms generate the engine. change any axiom and the entire system reconfigures

| axiom | symbol | statement |
|-------|--------|-----------|
| memory | $\mathcal{M}$ | one physical pool. all engines see the same pages. zero-copy is the default, not an optimization |
| engines | $\mathcal{E}$ | $\{\text{Cpu}, \text{Amx}, \text{Gpu}, \text{Ane}\}$. plural, peer, concurrent. Gpu is one engine, one driver per platform: aruminium on Apple Silicon, wgpu on portable targets |
| presentation | $\mathcal{P}$ | one protocol ([[prysm]]) for 2D and 3D. layout is renderer-independent. mir is prysm's 3D-mode renderer |

derived: the engine is a multi-engine dispatch graph $\mathcal{D}$ over a single memory pool, with prysm as the universal presentation layer above it

---

## 2. invariants

testable constraints. an implementation is not evy until all pass

| # | invariant | formal statement |
|---|-----------|------------------|
| M1 | zero-copy default | shader-visible component data is read by Gpu/Ane without intermediate copy |
| M2 | shared address stability | a shader-visible cell in the `unimem` backend has stable address for its lifetime; despawn returns to free list with generation bump |
| M3 | unified pool | every buffer used by ≥2 engines lives in a single [[unimem]] [[IOSurface]] page set |
| E1 | engine concurrency | engines $e_1, e_2$ with disjoint write sets run in parallel within a frame |
| E2 | cross-engine sync | every data dependency across engines is encoded as a DAG edge with explicit sync primitive |
| E3 | no engine monopoly | render graph nodes declare their engine kind; no kind is privileged |
| P1 | prysm conformance | UI passes all 10 invariants of [[prysm/layout]] §3.3 (I1–I10) |
| P2 | 2D/3D parity | every prysm atom has identical semantics in 2D and 3D modes |
| C1 | particle identity | every content unit is a [[particle]] (hemera hash of content); entity identity is the particle |
| C2 | signal provability | every state transition on a [[bbg]]-backed component produces a [[zheng]] proof or is rejected |
| N1 | content addressing | asset references use `radio://<particle>` scheme; file paths are an authoring-only convenience |
| N2 | offline tolerance | every networked operation degrades to local cache; UI never blocks on network |
| R1 | budget honesty | per-frame compute budget is allocated explicitly across engines; overrun is logged, not silent |

---

## 3. hardware target

evy supports mobile, desktop, console, and XR out of the box. capability varies by platform; the runtime detects what each device offers and the dispatch scheduler routes work to available engines, degrading features that require absent silicon. four target tiers

### 3.1 first-class (full stack)

every honeycrisp engine available; every feature enabled

| platform | UMA | bandwidth | NPU | matrix | notes |
|----------|-----|-----------|-----|--------|-------|
| Apple M1 Pro/Max desktop/laptop | yes | 200/400 GB/s | ANE 11 TOPS | AMX | reference platform |
| Apple M2/M3/M4 desktop/laptop | yes | 100–800 GB/s | ANE 16–38 TOPS | AMX / SME on M4 | first-class |

macOS distribution is direct (DMG, Homebrew, .app) — no App Store dependency. Apple mobile (iOS/iPadOS) and Vision Pro are not targets (§3.6)

### 3.2 portable (degraded, supported)

partial engine availability; features that require missing silicon disable

| platform | available engines | unavailable | degradation |
|----------|-------------------|-------------|-------------|
| AMD Strix Halo (Windows/Linux) | Cpu (+AVX-512), Gpu/wgpu(Vulkan), Ane (via XDNA driver, TBD) | AMX, aruminium path | matrix work routes to Cpu/AVX-512; Gpu only via wgpu |
| Qualcomm X Elite (Windows ARM) | Cpu (+SME), Gpu/wgpu(D3D12), Ane (via Hexagon NN, TBD) | AMX, aruminium | similar to Strix Halo, ARM CPU |
| Snapdragon 8 Gen 2+ Android | Cpu (+NEON), Gpu/wgpu(Vulkan), Ane (NNAPI/Hexagon NN) | AMX, aruminium, IOSurface | shared memory via AHardwareBuffer; ANE via NNAPI |
| mid-tier Android (no flagship NPU) | Cpu (+NEON), Gpu/wgpu(Vulkan) | AMX, aruminium, Ane, IOSurface | neural materials disabled or fall back to GPU; per-NPC inference reduced count |
| PS5 / Steam Deck APU | Cpu, Gpu/wgpu | Ane, AMX | neural features disabled |
| discrete-GPU PC (Intel/AMD CPU + NVIDIA/AMD GPU) | Cpu, Gpu/wgpu | Ane, AMX, IOSurface | non-UMA: pays staging-buffer cost; neural features disabled |

degraded platforms still run the same game content. the engine reports `PlatformCapabilities` at startup; systems that require absent engines skip, fall back, or fail loudly per `FallbackPolicy` declared by the system author. content authors target the capability set, not specific platforms

### 3.3 capability matrix

quick reference for what each platform tier offers

| capability | macOS M-series | Android flagship | Android mid | Windows ARM | Windows x86 desktop | console APU | discrete PC |
|------------|----------------|------------------|-------------|-------------|---------------------|-------------|-------------|
| unimem zero-copy | ✓ | partial (AHardwareBuffer) | — | — | — | — | — |
| AMX matrix coprocessor | ✓ | — | — | — | — | — | — |
| ANE neural engine | ✓ | NNAPI | — | NNAPI | NNAPI (NPU SKUs) | — | — |
| Gpu driver | aruminium (raster + compute, hand MSL) | wgpu (Vulkan) | wgpu (Vulkan) | wgpu (D3D12) | wgpu (D3D12) | wgpu | wgpu |
| radio P2P | ✓ | ✓ | ✓ | ✓ | ✓ | restricted | ✓ |
| neural materials | ✓ | ✓ (slower) | fallback to baked | — | — | — | — |
| generative cache | ✓ | ✓ (slower) | disabled | — | — | — | — |
| per-NPC inference at scale | thousands | dozens | tens (CPU) | dozens | dozens | — | — |

### 3.4 runtime capability detection

at startup, evy probes the platform and constructs a `PlatformCapabilities` resource:

```rust
struct PlatformCapabilities {
    engines: EnumSet<Engine>,           // which engines available
    has_unimem: bool,                   // IOSurface / AHardwareBuffer / none
    has_amx: bool,
    has_ane: bool,
    npu_kind: Option<NpuKind>,          // ANE | NNAPI | XDNA | Hexagon
    gpu_paths: EnumSet<GpuPath>,        // Wgpu always; Aruminium only on Apple
    memory_class: MemoryClass,          // UMA | UMA_BandwidthLimited | NonUMA
    thermal_budget: ThermalBudget,      // Desktop | Laptop | Tablet | Phone | XR
    tick_rate_hz: u32,                  // suggested tick rate from thermal/battery policy
}
```

systems and dispatch nodes register fallback policy:

```rust
enum FallbackPolicy {
    Required,           // refuse to run if preferred engine missing
    DegradeTo(Engine),  // run on alternate engine with reduced quality
    Skip,               // omit from dispatch if missing
}
```

a neural material with `FallbackPolicy::DegradeTo(Cpu)` runs on CPU with reduced batch size when ANE is absent. one with `FallbackPolicy::Skip` simply doesn't render its neural component, falling through to a baked texture. one with `FallbackPolicy::Required` refuses to load on platforms without ANE — useful for AI-driven game logic that has no acceptable degradation

### 3.5 thermal and power policy

mobile targets and laptops on battery have thermal/power limits. evy respects these via:

- `tick_rate_hz` from `PlatformCapabilities` — gameplay state cadence (BBG commits) drops on battery
- φ*-budget allocator scales per-particle inference budget by current thermal headroom
- ANE/NPU prefers idle-time burst over sustained dispatch (NPU power efficiency wins when intermittent)
- Android Activity lifecycle: engine checkpoints state on `onPause`, restores on `onResume`. checkpoint format = BBG root + ephemeral ShardStore snapshot. background-killed processes restore from checkpoint on next launch

### 3.6 non-targets

- Apple mobile (iOS / iPadOS) — distribution requires App Store; deployment policy and review process are unacceptable. macOS desktop direct-distribution path is the Apple-side target
- Apple Vision Pro — same App Store constraint as iOS; deferred indefinitely
- WebGPU browser deployment — possible but unimem doesn't survive translation; deferred
- single-threaded environments — the engine assumes parallel engine dispatch
- discrete-GPU desktop as a *first-class* target — supported (degraded mode per §3.2) but architectural choices favor UMA platforms

---

## 4. memory model (axiom $\mathcal{M}$)

### 4.1 the pool

all buffers shared between ≥2 engines are allocated via [[unimem]] primitives:

| primitive | role | source |
|-----------|------|--------|
| `unimem::Block` | IOSurface-backed pinned page. stable address for lifetime | honeycrisp/unimem/src/block.rs |
| `unimem::Grid` | slot-pool of typed cells. stable address per cell | honeycrisp/unimem/src/grid.rs |
| `unimem::Tape` | bump allocator, ~1ns take, frame-reset | honeycrisp/unimem/src/tape.rs |

cells in a `Grid<T>` are simultaneously:

- a `&T` in Rust
- a slot in an `MTLBuffer` for [[aruminium]] raster + compute (Apple Silicon)
- an input surface for [[rane]] ANE inference
- a tile source for [[acpu]] AMX/NEON ops

no copy, no upload, no extract. one IOSurface, many views

### 4.2 the storage interface

evy adopts [[bbg]]'s `ShardStore` trait verbatim (bbg/specs/storage.md). there is no separate "ECS storage." components live in shards. the same trait serves authenticated cybergraph state, ephemeral render buffers, and everything between

```rust
trait ShardStore {
    fn get(&self, dimension: u8, key: &[u8; 32]) -> Option<&[FieldElement]>;
    fn put(&mut self, dimension: u8, key: &[u8; 32], value: &[FieldElement]);
    fn dirty_entries(&self) -> impl Iterator<Item = (u8, [u8; 32], &[FieldElement])>;
    fn commit(&mut self) -> [u8; 32];
}
```

quoting bbg's spec: "the polynomial commitment provides authentication. the local data structure provides access. they are INDEPENDENT — the store doesn't know about polynomials, the polynomial doesn't know about storage." this separation is the keystone. evy inherits it

### 4.3 backends

five backend implementations of `ShardStore`, selected per-shard by hardware capability + access pattern + tier policy:

| backend | implementation | optimal for | latency |
|---------|----------------|-------------|---------|
| memory | `HashMap` + `bitvec` | small shards (≤64 GB) | 50 ns read |
| unimem | [[unimem]] IOSurface-pinned Blocks | Apple Silicon hot path; zero-copy CPU/AMX/GPU/ANE | ~1 ns alloc |
| ssd | `fjall` LSM-tree | shards exceeding RAM | 20 μs read |
| hdd | `redb` B-tree MVCC | full history, cold | sequential 200 MB/s |
| network | `NetworkStore` trait (injected by [[radio]]) | content not held locally; DAS sampling | seconds |

backend selection is per-shard, not per-component-class. on [[Apple Silicon]], a component touched by GPU defaults to unimem; on non-Apple-Silicon, the same component defaults to memory with explicit upload at access. the component definition does not change. there are no storage classes — only backends and dimensions

### 4.4 the canonical state schema

evy state partitions into 13 namespaces: 10 public dimensions of BBG_poly + 2 private polynomial commitments + 1 ephemeral domain. components register into one

| namespace | source | committed | role |
|-----------|--------|-----------|------|
| particles | BBG_poly dim 0 | yes | content nodes, energy, φ* |
| axons_out | BBG_poly dim 1 | yes | outgoing edge index |
| axons_in | BBG_poly dim 2 | yes | incoming edge index |
| neurons | BBG_poly dim 3 | yes | agent state: focus, karma, stake |
| locations | BBG_poly dim 4 | yes | spatial proofs |
| coins | BBG_poly dim 5 | yes | fungible token denominations |
| cards | BBG_poly dim 6 | yes | non-fungible knowledge assets, names |
| files | BBG_poly dim 7 | yes | content availability commitments |
| time | BBG_poly dim 8 | yes | temporal index |
| signals | BBG_poly dim 9 | yes | finalization log |
| A(x) | commitment polynomial | yes (private) | individual cyberlinks (record-level) |
| N(x) | nullifier polynomial | yes (private) | spent-record markers |
| ephemeral | none | no | local-only state: Transform, animation, render scratch, VFX |

component registration declares one namespace. ephemeral is the default for engine-internal components. authenticated namespaces (everything except ephemeral) participate in BBG_root and emit [[zheng]] proofs at tick boundaries (§6.2)

### 4.5 component → namespace mapping

| component | namespace | backend on M-series |
|-----------|-----------|---------------------|
| Transform | ephemeral | unimem (GPU reads it) |
| AnimationPhase | ephemeral | memory (CPU only) |
| Currency balance | coins | unimem (AMX Poseidon2) |
| OwnedItem | cards | unimem |
| HistoricalLink | time | hdd (cold replay) |
| OpenAsset | files | network (radio fetch) |
| NeuronProfile | neurons | unimem (GPU render) |
| IndividualCyberlink | A(x) | unimem (private record) |
| SpentNullifier | N(x) | unimem (double-spend check) |
| VFXLifetime | ephemeral | memory |
| RenderScratch | ephemeral | unimem (Gpu/Aruminium kernel target) |

plain english: pick the namespace your data semantically belongs to (or ephemeral); the backend is resolved from hardware + tier policy. no class hierarchy, no storage taxonomy beyond what [[bbg]] already defines

### 4.6 stable addresses come from the backend

[[bevy_ecs]]'s `Table` storage uses `swap_remove` on archetype change (storage/table/mod.rs:214), which moves component bytes. this is incompatible with GPU buffer descriptors holding addresses

resolution: backend choice. the `unimem` backend implements `ShardStore` over slot-pooled IOSurface cells where addresses do not move. the `memory` backend implements `ShardStore` over `BlobVec` with swap-remove semantics. components do not pick a class — they pick a backend (or accept auto-selection), and the backend's allocator determines address stability

cells in the unimem backend carry a generation tag `(index: u32, gen: u16)`. cell reuse bumps the generation. GPU buffers holding stale `(index, gen)` pairs fail validation on read. this prevents the swap-remove hazard without sacrificing GPU access stability

cost: 6 bytes per shader-visible cell. one extra indirection per query (L1 cache hit, single-digit ns)

### 4.7 IOSurface lifetimes

per [[unimem]] design: IOSurface is locked at creation, unlocked at drop. cells within an IOSurface-backed shard are addressable for the shard's lifetime. cross-process IOSurface sharing works (zero-copy IPC) but is out of v1 scope. cross-engine sharing within process is the load-bearing case

### 4.8 tier routing

[[cyb/soma]]-equivalent policy layer (deferred to v2) sets routing: which shards stay hot (unimem/memory), which demote to ssd, which archive to hdd. evy executes the mechanism; [[focus]] (φ*) IS the cache priority. high focus = stay hot; low focus = migrate cold. for v1, default policy: ephemeral + recently-touched authenticated = hot; rest = ssd

---

## 5. engine model (axiom $\mathcal{E}$)

### 5.1 the four engines

an engine is a distinct dispatch context the scheduler must track, not distinct silicon. NEON is part of Cpu because any thread issues NEON instructions in the same stream as scalar, sharing the SIMD register file (v0–v31). AMX is its own engine because per-thread `AmxCtx` must be initialized before AMX instructions can issue. ANE is its own engine because dispatch goes through [[rane]]'s MIL bytecode queue, not through the issuing thread. Gpu is one engine with one driver per platform: [[aruminium]] (raster + compute, hand-written MSL, zero copy) on Apple Silicon; wgpu (cross-vendor Vulkan/D3D12) on portable targets

| engine | hardware | dispatch | workloads |
|--------|----------|----------|-----------|
| Cpu | P-cores + E-cores. scalar + NEON SIMD share register file | direct call; any thread | ECS systems, layout, input, audio DSP (NEON via [[acpu]]::vector), blake3 hashing for BAO, FFT, vector math, crypto SHA/AES/PMULL (via [[acpu]]::crypto), small inference below ANE dispatch threshold |
| Amx | Apple AMX coprocessor (per-cluster). M4 introduces SME as future replacement | inline asm via [[acpu]]; per-thread `AmxCtx` required | transform propagation, skinning palettes, eigensolvers (LOBPCG), Poseidon2 hashing, SGEMM ≥32×32, batched matrix math |
| Gpu | Apple GPU (on macOS) or vendor GPU (elsewhere) | [[aruminium]] on Apple Silicon — its own `MTLDevice` + `MTLCommandQueue`, raster + compute, zero copy via unimem. wgpu elsewhere — Vulkan/D3D12 fallback | raster, post, custom MSL compute kernels, mir tier passes, neural material vertex transforms |
| Ane | Apple Neural Engine | [[rane]] MIL bytecode dispatch queue | batched neural inference (≥1M params), neural material per-pixel feature maps, NeRF eval, diffusion, per-NPC behavior models |

each engine has its own dispatch primitive and (where applicable) its own worker context. engines share memory but not state

### 5.2 one renderer per platform

within engine Gpu, exactly one driver is active per platform — they do not coexist:

| platform | driver | what |
|----------|--------|------|
| Apple Silicon (macOS) | [[aruminium]] | hand-written MSL, raster + compute, zero copy via unimem, native `MTLSharedEvent` cross-engine sync |
| portable (Android, Windows, Linux, console) | wgpu | cross-vendor Vulkan/D3D12; no zero-copy guarantee |

aruminium is the renderer on Apple Silicon because the wgpu abstraction taxes ~9–13 ms of frame budget for the same Metal hardware (per-draw-call CPU overhead 5–8 µs vs 1–2 µs; mesh upload BW 25 GB/s vs 70–100 GB/s direct IOSurface; lost `MTLSharedEvent` cross-engine sync; WGSL→MSL translation cost). on a 16.6 ms frame, that's the difference between 60 FPS and 30. on Apple Silicon evy is Apple-first; portability overhead is paid only where Metal isn't available

wgpu does not coexist with aruminium on Apple Silicon. an earlier draft of this spec proposed sharing `MTLDevice` between the two drivers; that was reverted (commit `48f1bc8` in honeycrisp/aruminium). on every platform evy ships, exactly one Gpu driver is active

### 5.3 engine routing rules

a workload routes to an engine by these defaults:

| workload | engine | reason |
|----------|--------|--------|
| batched matrix multiply (≥32×32) | Amx | tile-shaped, no GPU dispatch overhead |
| small dense vector ops (length < 1024) | Cpu/NEON | dispatch overhead dominates |
| dense vector ops (≥ 1024) | Amx or Gpu compute | depending on shape |
| raster pass | Gpu (aruminium on Apple, wgpu elsewhere) | what GPUs do |
| compute over shared memory | Gpu (aruminium on Apple, wgpu compute on others) | zero translation on Apple Silicon |
| neural inference (≥ 1M params) | Ane | 50× more efficient per watt than Gpu |
| neural inference (tiny, < 100K params) | Cpu or Gpu | dispatch overhead matters |
| graph eigensolve (LOBPCG) | Amx | repeated mat-vec, AMX shape |
| Poseidon2 hashing | Amx (via [[acpu]] field jets) | matrix-shaped permutation |
| BAO content verification | Cpu (blake3 via acpu) | I/O bound |

routing is per-node in the dispatch graph, not per-system. one logical Bevy system can spawn work on multiple engines

---

## 6. dispatch model (the unified DAG)

### 6.1 one DAG, two execution semantics

evy's dispatch DAG and [[bbg]]'s signal DAG are the same DAG. they share infrastructure (`ShardStore` reads/writes, scheduler, topology). they differ in execution semantics: each node declares a commit policy. some nodes' writes trigger BBG commitment (polynomial recommit + zheng proof + radio gossip). others stay ephemeral and never touch the commit path

```rust
trait DispatchNode {
    fn engine(&self) -> Engine;
    fn reads(&self) -> &[ShardRef];
    fn writes(&self) -> &[ShardRef];
    fn dispatch(&mut self, ctx: &mut DispatchCtx);
    fn commit_policy(&self) -> CommitPolicy;
}

enum CommitPolicy {
    None,                  // ephemeral: render output, scratch compute, frame-local state
    BbgDimension(u8),      // writes update a BBG_poly dimension; recommit at tick boundary
    BbgPrivate,            // writes extend A(x) commitment polynomial
}

enum Engine { Cpu, Amx, Gpu(GpuPath), Ane }
enum GpuPath { Wgpu, Aruminium }
```

[[bevy_render]]'s existing `RenderGraphNode`s map to `DispatchNode` with `engine() == Engine::Gpu(Wgpu)` and `commit_policy() == None`. mir's tier passes implement `Engine::Gpu(Aruminium)` with `None`. [[glia]] per-particle inferences implement `Engine::Ane`. AMX-batched transform propagation implements `Engine::Amx`. a gameplay signal handler (currency transfer, cyberlink creation, ownership change) implements `CommitPolicy::BbgDimension(d)` or `BbgPrivate`

the render graph is the subset of the dispatch DAG where `commit_policy() == None`. it is not a separate construct

### 6.2 the scheduler

per frame, the scheduler:

1. collects all enabled `DispatchNode`s (render + gameplay)
2. topo-sorts by `ShardRef` read/write dependencies
3. batches by engine: Wgpu nodes record into one command buffer; Aruminium nodes dispatch to its queue; Ane nodes submit to [[rane]] queue; Amx nodes dispatch on pinned threads
4. inserts cross-engine sync primitives at DAG edges that cross engine kinds (§6.3)
5. submits per-engine work in parallel
6. at tick boundary (subset of frames; e.g. every 6th frame at 60Hz → 10Hz tick): collects writes from nodes with `CommitPolicy::BbgDimension`/`BbgPrivate`, calls `ShardStore::commit()` on affected shards, recomputes BBG_root, generates [[zheng]] proof, gossips via [[radio]]
7. awaits cross-engine sync barriers before next frame

frame ≠ tick. frame is render swap cadence (60–120Hz). tick is gameplay state cadence (10–30Hz). render nodes run per frame; BBG-committing nodes run per tick. one scheduler, two cadences

### 6.3 cross-engine synchronization

[[Apple Silicon]] has no universal barrier. each engine pair has its own sync primitive

| from → to | mechanism |
|-----------|-----------|
| Cpu → Gpu | `MTLCommandBuffer` enqueue, fence |
| Gpu → Cpu | `MTLCommandBuffer` completed handler or `waitUntilCompleted` |
| Ane → Cpu | `IOSurface` lock/unlock semantics |
| Cpu → Ane | rane program input ready |
| Amx → any | thread-local fence (AMX context is per-thread) |
| Gpu → Ane | uncommon; routes via Cpu fence today |

scheduler encodes all seven pairs. nodes whose engine pair has no direct sync route through Cpu

### 6.4 ExtractSchedule collapses

[[bevy]]'s `ExtractSchedule` copies `MainWorld` components into `RenderWorld` each frame. for `unimem`-backed shards, the bytes are already shared between worlds via IOSurface. extract becomes a no-op for these shards. for `memory`-backed shards used across worlds, the original extract path remains valid. result: extract cost scales with the number of shader-visible-but-not-unimem-backed shards, which v1 expects to be near zero

### 6.5 frame budget

a 60Hz frame is 16.6ms. allocation across engines, default policy:

| engine | typical share | notes |
|--------|---------------|-------|
| Cpu | 4 ms | ECS, layout, input, scripting, NEON DSP |
| Amx | 3 ms | transform, skinning, physics matrices, Poseidon2 at tick |
| Gpu | 8 ms | raster + post + compute kernels |
| Ane | full 16.6 ms | inference is async to other engines; rate-limited by ANE bandwidth |

engines run concurrently. the 31.6ms theoretical sum compresses to 16.6ms wall time because they execute on independent silicon. Ane runs without contending with Gpu — that is the structural advantage evy extracts from [[Apple Silicon]]

at tick boundary (every Nth frame), an additional commit cost: ~50μs for BBG polynomial recommit + proof generation. amortized across frames it is invisible; if a single tick batches many signals it can spike. tick rate is tunable per game (10–30Hz typical)

---

## 7. presentation model (axiom $\mathcal{P}$)

### 7.1 prysm is the UI engine

[[prysm]] is a formal layout protocol with 14 theorems formalized in Lean 4. evy adopts prysm directly. [[bevy_ui]] is replaced

prysm provides:

- layout algorithm: pure function (element tree × viewport → coordinates). O(n), single-pass, deterministic, ≤2ms for 10K organelles
- atoms: glass, text, ion, saber, images
- molecules: 22 composed widgets (button, hud, input, content, neuron-card, ...)
- cells: 3 regional compositions (oracle-cell, sphere-cell, ...)
- aips: 10 full-screen applications
- emotion: computed color signal layer, 4 evaluation rules, formal thresholds, fallback policy
- responsive: fold protocol with optimal conformation selection (theorem T6)
- motion: one duration, one curve, single-pass
- semantic: accessibility layer with formal `Semantic` role tagging

every [[prysm]] spec includes a Bevy-shaped ECS section (Components, Systems). implementation is mechanical transcription of those sections into Rust

### 7.2 2D and 3D as modes

prysm spans 2D and 3D in one protocol ([[prysm/layout]] §11). atoms have identical semantics in both modes; what differs is the renderer

| mode | renderer | use |
|------|----------|-----|
| 2D | bevy-mesh quad batcher + cosmic-text + sprite atlas | HUD, menus, content surfaces, traditional UI |
| 3D | [[mir]] (Bevy mesh + aruminium tier compute) | spatial cybergraph navigation, T0–T∞ tiers, neural radiance |

[[mir]] is not a separate UI system. mir is prysm's 3D-mode renderer. the same `glass` atom in 2D draws a translucent quad; in 3D, the same atom occupies depth $s_d$ and z-position $p_z$ driven by gravity. one protocol, two drawers

### 7.3 prysm crate layout

| crate | role | LOC estimate |
|-------|------|--------------|
| `evy_prysm_core` | Π protocol, Φ sizing, K containers, layout function | ~2K |
| `evy_prysm_atoms` | 5 atom implementations (glass, text, ion, saber, images) | ~1.5K |
| `evy_prysm_molecules` | 22 molecule bundles | ~3K |
| `evy_prysm_cells` | 3 cells + 10 aip scaffolds | ~2K |
| `evy_prysm_emotion` | emotion function with 4 evaluation rules | ~500 |
| `evy_prysm_motion` | motion system | ~300 |
| `evy_prysm_2d` | 2D draw backend (quads, text, images via bevy mesh) | ~1K |
| `evy_prysm_3d` | 3D draw backend (mir tier integration) | ~1K |

total: ~11K LOC. implementation work is bounded by the spec, not by design

---

## 8. content model

### 8.1 particle = entity

entity identity IS its [[particle]]. spawning a new entity for content that is already a particle returns the existing entity (idempotent). entities without explicit content get an anonymous local particle ([[hemera]] hash of session UUID + spawn counter)

implications:

- save/load by particle is structural: `World::checkpoint() → root_particle`, `World::restore(root_particle)`
- cross-machine entity reference works without an entity mapper
- duplicate spawn detection is automatic
- entity ID stability across sessions and machines

### 8.2 component = cyberlink

a component instance is conceptually a [[cyberlink]] `ask(ν, p, q, τ, a, v, t)`:
- ν: neuron — who set this component
- p: from-particle — the entity
- q: to-particle — the component value
- τ: type — semcon for this component
- a: stake — 0 for ephemeral components
- v: valence — +1 default
- t: time — tick

`Bbg`-backed components store the full cyberlink record. `Grid` and `Table` components store only the value; the cyberlink metadata is implicit

### 8.3 .cyb particles as scene format

[[bevy_scene]] is deleted. scenes are `.cyb` particles containing a section of cyberlinks. loading a scene = fetching the particle and replaying its cyberlinks as ECS commands. `.cyb` format is fixed (TOML frontmatter + `~~~name` separators) and supports binary asset sections in-line

### 8.4 assets are particles

[[bevy_asset]] is replaced. asset references use `radio://<particle>` URIs. the [[radio]] adapter (see §9) resolves the particle → bytes via local cache, peers, or fallback relay. `.cyb` is the universal container; loaders inspect frontmatter and dispatch to type-specific sub-loaders (image, mesh, model, glTF as a section type)

| asset kind | source | loader |
|------------|--------|--------|
| texture | radio://particle | `evy_particle_loader::image` |
| mesh | radio://particle | `evy_particle_loader::mesh` |
| neural model | radio://particle | `evy_particle_loader::model` (yields `glia::Model`) |
| audio clip | radio://particle | `evy_particle_loader::audio` |
| scene | radio://particle | `evy_particle_loader::scene` (replays cyberlinks) |
| font | radio://particle | `evy_particle_loader::font` |

hot-reload happens when a reference resolves to a new particle: gossip notifies subscribers, asset server invalidates handles

---

## 9. networking model

### 9.1 radio as content + replication

[[radio]] (built on iroh: QUIC + BAO + gossip + Willow) provides content addressing, streaming verification, P2P discovery, and CRDT-style replication. evy consumes radio as an out-of-engine service

architecture:

```
[ Bevy app, single thread, deterministic ]
       │
       │ channel handles
       ▼
[ evy_radio adapter, owned tokio runtime ]
       │
       │ iroh API
       ▼
[ iroh endpoint: blobs, gossip, docs, willow ]
       │
       │ QUIC
       ▼
[ peer mesh ]
```

`evy_radio` runs its own tokio runtime in a dedicated thread. Bevy holds `Res<RadioClient>` which is a channel handle. async iroh operations are submitted as messages; completions arrive on a return channel polled at frame start. this isolates iroh's `select!`-heavy async code from Bevy's executor (compliance with established `select!` cancel-safety lessons)

### 9.2 what radio handles

| concern | radio primitive |
|---------|-----------------|
| asset fetch | `iroh-blobs` by particle, with BAO incremental verify |
| asset hot-reload | gossip notification on particle-reference change |
| state replication | `iroh-gossip` for signals; `iroh-docs` for derived CRDT state |
| peer discovery | `iroh-relay` + DERP |
| late-join state sync | fetch state root + missing particles |
| chat / events with anti-spam | signals with stake (cost = stake) |

### 9.3 multiplayer model

no client/server distinction. every player is an iroh node. world canonicality is determined by [[foculus]] (collective focus theorem) — the φ* distribution over signals converges to a stable attractor that defines consensus. signals from any neuron propagate via gossip; each node validates and applies independently

cheating is structurally detectable: invalid signals fail verification on receipt; deviating state diverges from the consensus root. recovery is local — out-of-consensus nodes resync from the canonical root

### 9.4 offline tolerance

every networked operation has a local-cache fallback. fetch returns from cache if available; gossip subscriptions buffer offline and replay on reconnect; signals queue and submit on reconnect. UI never blocks waiting for network. this is invariant N2

---

## 10. rendering model

### 10.1 hybrid raster + inference

a frame composes raster, compute, and neural paths into the same framebuffer. on Apple Silicon all three share one Metal device (aruminium); on portable targets, raster + compute go through wgpu and neural is unavailable

| path | Apple Silicon | portable |
|------|---------------|----------|
| raster | aruminium (hand MSL, native MTLBuffer + MTLTexture from unimem) | wgpu (WGSL/naga, copy-staged) |
| compute | aruminium (custom MSL kernels, same queue as raster) | wgpu compute (degraded; no zero copy) |
| neural | ANE via rane (zero-copy IOSurface inputs) | NNAPI on Android, otherwise none |

on Apple Silicon, all three paths share buffers via [[unimem]]. raster reads positions from a Grid; compute writes positions into the same Grid; neural reads features from a Grid that mir's compute populated. one memory, three pipelines, one device — no cross-driver sync. on portable targets the model degrades per §3.2: same pipelines, no zero copy, fewer engines

### 10.2 neural materials

a `NeuralMaterial` is a [[bevy]] `Material` trait impl that, instead of a shader-only pipeline, takes:

- standard inputs (UV, view, normal, light)
- a `Model` handle (glia model loaded from a .model particle)
- graph context inputs (nearby cyberlinks, [[cyberank]], current path)

and produces radiance via a glia forward pass on ANE. mesh raster (wgpu) handles vertex transform; per-pixel evaluation routes to a precomputed feature map produced by an ANE pass earlier in the frame DAG

constraint: ANE inferences batch per draw call, not per pixel. one inference produces a feature map that the raster shader samples. the feature map is a Grid texture. ~100μs ANE dispatch overhead is amortized across millions of pixels

### 10.3 φ*-driven render budget

[[tru]] produces φ*, the focus distribution over particles. evy consumes φ* as a render budget allocator:

- particles in top focus quantile: full neural material + tier-3 detail + per-frame inference
- mid quantile: cached neural material (inference every N frames) + tier-2 detail
- low quantile: impostor billboard + tier-1 detail
- edge-of-frustum: label-only atom

implementation: a `PhiBudget` resource holds per-particle render policy. systems consult it before scheduling work. `evy_phi_budget` crate

### 10.4 generative cache

an asset that resolves to "particle exists but has no bytes" triggers generation:

| missing asset | generator | engine |
|---------------|-----------|--------|
| texture | diffusion model on Ane + Gpu | Ane primary, Gpu sampling steps |
| mesh | shape generator (TBD) | Gpu |
| audio | sound model | Ane |
| 3D environment | neural radiance from cyberlink context | Ane + Gpu |

generated assets are published back to [[radio]] as new particles. peers exploring the same context retrieve the cached generation. `evy_generative_cache` crate

### 10.5 speculative graph inference

unexplored regions of the [[cybergraph]] get LLM-speculated cyberlinks, signed with low stake. two players exploring the same region see identical speculations (deterministic from context). canonical content from neurons overrides speculations. unresolved game-design question: what does a player who explored a speculation later see when canonical content arrives (open question OQ-6)

---

## 11. what bevy provides — adopted intact

these crates are used unmodified from crates.io. ~52K LOC of substrate

| crate | role |
|-------|------|
| bevy_app | App, Plugin, SubApp, schedules |
| bevy_winit | macOS event loop, window event integration |
| bevy_window | window component model |
| bevy_input | OS input event translation |
| bevy_gilrs | gamepad |
| bevy_state | states, transitions |
| bevy_time | time, timer, fixed timestep |
| bevy_log | tracing-based logging |
| bevy_math | NEON wrapper added below; primitives intact |
| bevy_color | color spaces |
| bevy_reflect | type reflection (extended via wrapper for semcons) |
| bevy_utils, bevy_macro_utils, bevy_derive, bevy_ptr, bevy_platform | substrate utilities |
| bevy_picking | ray picking |
| bevy_camera | camera component model |
| bevy_light | light component model |
| bevy_text | cosmic-text glyph layout (used by prysm as font shaper) |
| bevy_input_focus | OS focus integration (prysm uses its own NavigationOrder above this) |

---

## 12. what bevy provides — adopted with modification

these crates are forked. upstream tracking is loose

| crate | LOC | level | what changes |
|-------|-----|-------|--------------|
| bevy_ecs | 104K | EXTEND (+~1.5K LOC) | adds StorageType::Grid backed by unimem; scheduler gains Engine routing metadata |
| bevy_render | 26K | REPLACE-internal | RenderGraph generalized to multi-engine; ExtractSchedule collapses for Grid components; shared MTLDevice between wgpu and aruminium |
| bevy_pbr | 36K | REWRITE | neural material support; MSL shader path alongside WGSL; clustered-light compute via aruminium |
| bevy_core_pipeline | 7K | REWRITE | post-process compute via aruminium; shared-memory textures |
| bevy_anti_alias | 3K | REWRITE | MSL TAA/SMAA/FXAA over shared textures |
| bevy_post_process | 4K | REWRITE | DOF/chromatic/motion blur via aruminium compute |
| bevy_animation | 4.5K | EXTEND | AMX skinning palette mul; NEON curve eval |
| bevy_transform | 2.5K | EXTEND | AMX batched propagation |
| bevy_mesh | 9K | REWRITE | MeshAllocator backed by Grid<Vertex>; meshes do not upload |
| bevy_image | 6K | REWRITE | image data on IOSurface; decoders write into IOSurface pages |
| bevy_tasks | 3.7K | EXTEND | AmxTaskPool (pinned threads with AmxCtx); AneTaskPool |
| bevy_diagnostic | 1.3K | EXTEND | PMU-backed diagnostic via acpu probe |
| bevy_gizmos, bevy_gizmos_render | 7.7K | REWRITE | MSL shaders, shared-memory vertex buffer |
| bevy_sprite, bevy_sprite_render | 7.2K | REWRITE | shared-memory atlas |

---

## 13. what bevy provides — replaced or deleted

| crate | LOC | action | replacement |
|-------|-----|--------|-------------|
| bevy_ui | 12K | DELETE | [[prysm]] |
| bevy_ui_widgets | 2.4K | DELETE | prysm molecules |
| bevy_feathers | 3.3K | DELETE | prysm emotion |
| bevy_ui_render | 5.5K | REPURPOSE | becomes prysm 2D quad/text/image batcher |
| bevy_scene | 4.3K | DELETE | .cyb particles via radio |
| bevy_asset | 21K | REPLACE | evy_radio_asset (radio:// scheme) |
| bevy_gltf | 4.7K | REPLACE | evy_particle_loader (glTF as a .cyb section type) |
| bevy_audio | 1.9K | REPLACE | evy_audio (acpu NEON DSP + Ane spatial HRTF) |

---

## 14. new crates — no bevy equivalent

these crates do not exist in [[bevy]]. they are the substance of evy

| crate | role | est | status |
|-------|------|----:|--------|
| evy_engine_core | assembly, `Engine::builder()` wires every subsystem | ~1K | ✓ landed (340 LOC, 9 tests + end-to-end smoke) |
| evy_platform_caps | runtime probe (engines, NPU, thermal class) + FallbackPolicy | ~400 | ✓ landed (280 LOC, 6 tests) |
| evy_ecs_storage | typed component adapter over `bbg::ShardStore` | ~1.5K | ✓ landed (600 LOC, 25 tests) |
| evy_engine_tasks | AmxTaskPool + AneTaskPool (Apple Silicon; stub elsewhere) | — | ✓ landed (280 LOC, 7 tests) — spec deviation: was bevy_tasks EXTEND in §12, became separate crate |
| evy_engine_dispatch | DispatchNode trait + scheduler + commit-policy + engine pool routing | ~2.5K | ✓ session 1+2 landed (520 LOC, 18 tests). sessions 3-4 (parallel layer + cross-engine sync) blocked on aruminium step 3 |
| evy_prysm_core | Π + Φ + K + fold + motion + emotion (core protocol, no atoms) | ~2K | ✓ landed (1500 LOC, 47 tests) |
| evy_radio | channel bridge to iroh daemon | ~2K | ✓ session 1 landed (330 LOC, 6 tests) with stub daemon. session 2 = real iroh; session 3 = radio:// AssetSource |
| evy_diagnostic | wall + PMU measurement + Diagnostic accumulator | — | ✓ landed (370 LOC, 11 tests) — extends spec; was implicit |
| evy_semcon | semantic conventions + registry | ~800 | ✓ landed (300 LOC, 14 tests) |
| evy_nnapi_runtime | Android NPU dispatch parallel to [[rane]]; glia .model → NNAPI translation | ~2K | pending Android bring-up |
| evy_render_compute | aruminium compute passes as DispatchNodes | ~1K | blocked on aruminium step 3 |
| evy_neural_material | model-driven Material trait + [[glia]] bridge | ~1.5K | blocked on steps 3+4 |
| evy_phi_budget | φ*-driven per-particle render allocator | ~500 | blocked on step 8 (glia) |
| evy_generative_cache | diffusion-on-miss for textures/meshes | ~2K | blocked on step 8 |
| evy_glia_runtime | per-particle Ane inference dispatcher | ~1.5K | blocked on steps 3+4 |
| evy_mir_nodes | mir tier passes as DispatchNodes | ~3K (mir core lives in [[mir]] repo) | blocked on step 3 |
| evy_particle_loader | .cyb particle loaders (image, mesh, model, scene, audio, font) | ~2K | needs step 11.2 |
| evy_radio_asset | ShardStore network-backend impl serving the radio:// scheme | ~500 | needs step 11.2 |
| evy_audio | acpu NEON DSP + Ane spatial audio | ~2K | replaces bevy_audio |
| evy_prysm_* (atoms, molecules, cells, aips) | prysm implementation above core (§7.3) | ~11K | atoms blocked on render path (step 5+); molecules/cells/aips depend on atoms |

total landed: 9 crates, ~4.5K LOC, 143 tests. total planned new: ~33K LOC.

---

## 15. crate inventory roll-up

| category | crates | LOC |
|----------|--------|-----|
| bevy adopted intact | 19 | ~52K |
| bevy forked (EXTEND or REWRITE) | 14 | ~221K |
| bevy deleted | 4 | ~22K |
| bevy replaced | 4 | ~32K |
| new (evy) | 24 planned | ~33K |
| total bevy code involved | 52 | ~327K |
| total evy new code | 24 planned, 9 landed | ~33K planned, 4.5K landed |

ratio: ~10× more bevy code is involved than evy new code. the engine is mostly bevy with surgical replacements at load-bearing seams. that is what makes the project feasible

landed crates have 143 tests passing across 9 crates as of 2026-05-21; end-to-end smoke test in `evy_engine_core` exercises every subsystem

---

## 16. dependencies on soft3 components

| component | maturity | engine role |
|-----------|----------|-------------|
| [[unimem]] | working | memory foundation (axiom $\mathcal{M}$) |
| [[acpu]] | working | Amx + NEON dispatch |
| [[aruminium]] | working | Gpu compute path |
| [[rane]] | working | Ane dispatch |
| [[hemera]] | working | particle hashing (entity identity is its particle) |
| [[bbg]] | working | `ShardStore` trait + 12 namespaces — the storage substrate for ALL components |
| [[cybergraph]] | working | signal chain + VDF |
| [[nox]] | working | provable scripting compile target |
| [[mir]] | partial | 3D-mode prysm renderer |
| [[glia]] | partial | neural material + per-NPC inference |
| [[prysm]] | spec done, implementation partial | UI engine |
| [[radio]] | working | networking |
| [[tru]] | partial | φ* generation |
| [[trident]] | working | .tri → .nox compile |
| [[zheng]] | working | proof gen/verify |
| [[mudra]] | stub | crypto (defer to v2) |
| [[foculus]] | spec | consensus (defer to v2) |

v1 ships with: working soft3 components + spec-done prysm. partial components (mir, glia, tru) ship in their current form; defer foculus + mudra to v2

---

## 17. open questions

honest gaps that need resolution before or during implementation

| # | question | impact |
|---|----------|--------|
| OQ-1 | unimem backend allocation policy: static per-shard cap vs growable via chained IOSurfaces; rebind GPU descriptors on growth | affects bundle insertion failure mode |
| OQ-2 | cross-world `ShardStore` sharing: how do RenderApp + MainApp reference the same shard without violating bevy's world isolation; lifetime + Arc semantics | architectural, blocks step 3 of attack order |
| OQ-3 | per-frame inference batching: glia API needs to support batched per-particle eval, not arbitrary .model | blocks neural material payoff |
| OQ-4 | bbg verification cost vs framerate: 10–50μs per particle is too slow at 120Hz; cache+invalidate strategy needed; ties to tick boundary policy (OQ-11) | affects bbg storage UX |
| OQ-5 | deterministic neural eval across M1/M2/M3/M4 ANE: float differences break consensus when inference drives state | gameplay-impacting inference outputs need quantization rule |
| OQ-6 | speculation horizon: when canonical content arrives after a player saw speculation, what does the player see | game-design problem, not just engineering |
| OQ-7 | radio bootstrap on cold start: needs peer seed list or HTTP fallback | UX cliff for first launch |
| OQ-8 | glia .model format vs commercial models: safetensors → .model converter is a prerequisite | blocks third-party model use |
| OQ-9 | zheng proof generation latency: seconds-per-program today, not realtime; gameplay enforcement deferred to v2 | constrains v1 trust model |
| OQ-10 | `ShardStore` backend auto-selection policy: priority order (unimem → memory → ssd) and fallback semantics when preferred backend unavailable | runtime hardware probe + per-shard hint resolution |
| OQ-11 | tick rate vs frame rate: at what cadence do BBG commits run; mid-tick read of write-pending state needs explicit consistency rule (snapshot at tick start, or read-your-writes) | scheduler-level decision, affects all gameplay code |
| OQ-12 | engine boundary for inference: glia inference results are non-deterministic across hardware. when do those results become BBG signals (with quantization) vs stay ephemeral (visual only) | the determinism/expressiveness tradeoff |
| OQ-13 | A(x) / N(x) component semantics in ECS: an individual cyberlink IS a private record; the ECS shape for an "owned cyberlink as component" needs design | private-state ergonomics |
| OQ-14 | Android shared-memory model: AHardwareBuffer is the closest IOSurface analogue but has different lifetime + import semantics. is the `unimem` backend ported (with AHardwareBuffer impl) or does Android fall back to `memory` backend with explicit GPU upload? | platform-shim scope decision |
| OQ-15 | thermal throttling policy: when NPU/Gpu hit thermal limits, who decides to throttle? lower tick rate, reduce φ*-budget, drop NPC count, or all of the above? policy needs to be per-game-configurable | UX-critical on mobile, deferred to v1.1 acceptable |
| OQ-16 | NNAPI as ANE substitute on Android: rane's MIL bytecode doesn't translate to NNAPI; need a separate `evy_nnapi_runtime` crate parallel to rane, with model-import path from glia's .model | unblocks Android neural materials |
| OQ-17 | Android Activity lifecycle: checkpoint format on `onPause` and restore on `onResume` (or cold start after kill). BBG root + ephemeral ShardStore snapshot — ephemeral data (Transforms, animation phases) doesn't survive process death gracefully; needs deterministic restoration policy | Android correctness issue |

---

## 18. non-goals

what evy explicitly is not

- a cross-platform game engine with feature parity on every platform — capability tiers (§3.3) make platform differences explicit, not hidden
- a drop-in replacement for [[bevy]] that bevy games can port to
- compatible with the bevy plugin ecosystem
- optimal for AAA discrete-GPU PC rasterization workloads
- a research project on every layer simultaneously — adopts soft3 components as-is, forks bevy at chosen seams
- a WebGPU browser engine — unimem doesn't survive WebGPU translation
- self-contained — depends on [[unimem]]/[[acpu]]/[[aruminium]]/[[rane]] from honeycrisp and ~15 soft3 components

---

## 19. order of attack

implementation sequence. earlier steps unblock later ones

1. ✓ evy_platform_caps + evy_ecs_storage: probe + FallbackPolicy + `bbg::ShardStore` adapter. memory + unimem backends. ~3 sessions, **landed**
2. ✓ bevy_tasks extension: AmxTaskPool + AneTaskPool. ~1 session, **landed as separate crate `evy_engine_tasks`** (spec deviation; bevy_tasks moves from FORK to INTACT)
3. ⏳ aruminium gains raster support — `RenderPipelineState` + `RenderPassDescriptor` + `RenderEncoder` + render-target `Texture`, so aruminium is a complete renderer (not just compute) on Apple Silicon. ~3–4 sessions. **proposal drafted** at `~/cyber/honeycrisp/aruminium/.claude/plans/raster-support.md`. The earlier device-sharing constructors (commit `26ba16b`) were reverted (`48f1bc8`) because the wgpu+aruminium coexistence model they enabled costs 9–13 ms/frame on Apple Silicon
4. ◐ evy_engine_dispatch: `DispatchNode` trait, scheduler, commit-policy, capability-aware fallback. ~4 sessions, **sessions 1-2 landed** (trait + scheduler skeleton + engine pool routing); sessions 3-4 (parallel layer + real cross-engine sync) blocked on step 3
5. ⛔ bevy_mesh + bevy_image on unimem `ShardStore`; meshes/textures stop uploading. ~3 sessions, blocked on step 3
6. ⛔ bevy_transform with AMX propagation. first visible perf win. ~2 sessions, blocked on step 4 complete
7. ◐ prysm core + atoms implementation. ~5 sessions, **core protocol landed** (Π + Φ + K + fold + motion + emotion in `evy_prysm_core`, 47 tests). atoms blocked on render path
8. ⛔ evy_glia_runtime + evy_neural_material — the wow demo. ~6 sessions, blocked on steps 3+4
9. ⛔ mir tier passes as DispatchNodes. ~4 sessions, blocked on step 3
10. ⛔ BBG signal handlers as DispatchNodes; tick-boundary commit scheduling; cybergraph + nox integration. ~5 sessions, blocked on step 4 complete
11. ◐ evy_radio adapter + ShardStore network-backend impl serving radio://. ~3 sessions, **session 1 landed** (channel bridge + stub daemon, 6 tests); session 2 = real iroh; session 3 = AssetSource wiring
12. ⛔ prysm molecules + cells + aips. ~10 sessions, blocked on step 7 atoms
13. ⛔ evy_phi_budget + evy_generative_cache. ~4 sessions, blocked on step 8
14. ⛔ bevy_pbr + post-process REWRITE. ~6 sessions, blocked on steps 3+4+5
15. ⛔ Android bring-up. ~5 sessions, blocked on step 4 complete
16. ⛔ evy_nnapi_runtime. ~5 sessions, blocked on step 4 + Android
17. ✓ PMU-backed `evy_diagnostic` + thermal/power telemetry. ~3 sessions, **landed** (370 LOC, 11 tests, PMU upgrade path with Wall fallback)

plus landed extras: evy_semcon (300 LOC, 14 tests; particle-identified component schemas), evy_engine_core (340 LOC, 9 tests; assembly + end-to-end smoke test)

**progress: 11 of 17 steps complete or substantially started. step 3 reframed: aruminium gains raster (proposal drafted) — the earlier device-sharing path was abandoned after numbers showed wgpu+aruminium coexistence taxes 9–13 ms/frame on Apple Silicon. ~25 sessions of downstream work (steps 4.3-4.4, 5, 6, 8, 9, 10, 14) unblock once aruminium raster lands.**

estimated total: ~69 sessions (~180–210 pomodoros, 35–42 days of focused work). this is the substantial-but-tractable scope of v1 including Android-out-of-the-box

---

## 20. license

cyber license: don't trust. don't fear. don't beg
