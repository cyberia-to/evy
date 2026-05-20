# evy

a unified-memory, multi-engine, neural-first game engine. forks 16 [[bevy]] crates selectively (in `forks/`); adopts ~20 bevy crates intact from crates.io; replaces 8 with cyber-native primitives ([[prysm]], [[radio]], [[bbg]], [[honeycrisp]]).

evy is the engine; [[cyb]] is one game built on it. evy is not a general-purpose bevy replacement — it is a substrate inversion for unified-memory hardware (Apple Silicon primary; Android, Strix Halo, X Elite portable).

## what evy buys from bevy

~100K LOC of rendering machinery — PBR, post-process, anti-alias, animation, gizmos, render graph — that would take a multi-engineer year to reproduce. evy adopts ~20 bevy crates intact, forks 16, replaces 8, and adds ~24 new crates of its own. `[patch.crates-io]` in `Cargo.toml` ensures transitive deps on forked crates resolve to our versions.

## what evy provides that bevy doesn't

- memory is unified; all engines (CPU, AMX, GPU, ANE) see the same pages via [[unimem]] IOSurface
- four compute engines, not one — the render graph generalizes to a multi-engine dispatch DAG
- ECS storage IS [[bbg]]'s `ShardStore` — components live in 13 namespaces (10 BBG_poly + A + N + ephemeral)
- presentation is one formal protocol ([[prysm]]) across 2D and 3D
- content is content-addressed [[particles]] reached over [[radio]]
- rendering is hybrid raster + neural inference, with [[glia]] models on ANE driving neural materials
- trust-critical state is verifiable via [[zheng]] proofs at tick boundaries
- mobile (Android) supported out of the box with capability-aware degradation

## structure

```
evy/
├── forks/                 16 bevy crates we modify (forked from v0.18.1)
├── crates/                ~24 evy-specific crates (~33K LOC)
├── specs/                 architecture spec (evy.md is canonical)
├── roadmap/               open proposals
├── .claude/plans/         agent state
├── Cargo.toml             workspace + [patch.crates-io] redirecting forked crates
├── CLAUDE.md              agent instructions
└── README.md              this file
```

intact bevy crates come from crates.io as versioned deps (~20 crates: bevy_app, bevy_winit, bevy_window, bevy_input, bevy_math, bevy_reflect, bevy_camera, bevy_light, bevy_color, bevy_text, ...). they are not stored in tree — `cargo doc` and the registry cache at `~/.cargo/registry/src/` cover any source-reading need. replaced crates (bevy_ui, bevy_scene, bevy_asset, bevy_audio, bevy_gltf, bevy_ui_widgets, bevy_feathers, bevy_ui_render) do not appear in our dep graph at all.

## specification

[[evy/specs/evy]] is the foundation document. 20 sections covering:

- three axioms (unified memory, plural engines, one presentation protocol)
- 13 invariants
- four hardware tiers (first-class macOS M-series; portable Android, Strix Halo, X Elite, console APU, discrete PC)
- multi-engine dispatch DAG with commit-policy unification of BBG signals + render
- 13-namespace storage schema via [[bbg]] `ShardStore`
- [[prysm]] as UI engine; [[mir]] as prysm's 3D mode
- radio:// as content + replication backbone
- neural materials, φ*-driven render budget, generative cache

## status

spec-complete. implementation begins with the bbg `ShardStore` extensions (see [[bbg/roadmap/cyb-engine-shardstore]]) and the `evy_ecs_storage` adapter that consumes them.

## license

cyber license: don't trust. don't fear. don't beg.
