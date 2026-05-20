# evy — claude code instructions

## project

evy is a unified-memory, multi-engine, neural-first game engine. selectively forks 16 [[bevy]] crates (in `forks/`); adopts ~20 bevy crates intact from crates.io; replaces 8 with cyber-native primitives. powers [[cyb]] and any future cyber-stack game.

## canonical spec

`specs/evy.md` is the source of truth for architecture. read it before any non-trivial work. 20 sections + open-questions list + 17-step attack order. if code and spec disagree, fix spec first then propagate.

## structure

```
evy/
├── forks/            16 bevy crates we modify (forked from v0.18.1 source)
├── crates/           evy-specific crates (see §14 of spec for the ~24-crate inventory)
├── specs/            architecture spec (evy.md is canonical)
├── roadmap/          open proposals (proposal lifecycle: draft → accepted → migrate to specs)
├── .claude/plans/    persistent agent state
└── Cargo.toml        workspace + [patch.crates-io] redirecting forked deps
```

intact bevy crates (~20: bevy_app, bevy_winit, bevy_math, etc.) come from crates.io. they are not stored in tree. for reading their source: `~/.cargo/registry/src/index.crates.io-*/bevy_<name>-0.18.1/` (Cargo cache) or `cargo doc --open`. replaced crates (bevy_ui, bevy_scene, bevy_asset, bevy_audio, bevy_gltf, bevy_ui_widgets, bevy_feathers, bevy_ui_render) do not appear in our workspace or dep graph.

## boundary with other cyber projects

- [[bbg]] provides `ShardStore` trait — evy's `ShardStore` consumer is `crates/evy_ecs_storage/`. blocked on bbg/roadmap/cyb-engine-shardstore landing.
- [[honeycrisp]] provides `unimem`, `acpu`, `aruminium`, `rane` — evy depends on these for memory + four engines
- [[prysm]] provides the UI protocol spec — `crates/evy_prysm_*` implement it
- [[mir]] provides 3D rendering — `crates/evy_mir/` adapts mir's tier passes as engine dispatch nodes
- [[glia]] provides neural inference runtime — `crates/evy_glia/` adapts
- [[radio]] provides P2P transport — `crates/evy_radio/` runs in an out-of-engine tokio runtime with channel bridge to evy's main thread (NEVER embed radio's tokio runtime in evy's executor)
- [[cyb]] is the canonical evy consumer — when evy ships, cyb/bevy/ migrates to evy/crates/

## what we adopt from bevy intact

~20 crates listed in §11 of spec. these come from crates.io via versioned deps in `Cargo.toml` workspace.dependencies (e.g. `bevy_winit = "=0.18.1"`). they are NOT stored in tree. examples: bevy_winit, bevy_window, bevy_input, bevy_app, bevy_math, bevy_reflect, bevy_camera, bevy_light. for reading their source: Cargo cache or `cargo doc --open`.

## what we fork

16 crates listed in §12 of spec. these live in `forks/<crate_name>/` and ARE modified. extensions and rewrites must be additive where possible; document the divergence in `specs/` (one file per forked crate, eventually). examples: bevy_ecs (Grid storage extension), bevy_render (multi-engine dispatch DAG), bevy_pbr (neural material support), bevy_mesh+bevy_image (IOSurface backing), bevy_animation+bevy_transform (AMX), bevy_tasks (Amx/Ane pools), bevy_diagnostic (PMU). `[patch.crates-io]` redirects transitive deps on these crates from any crates.io-sourced bevy crate to our forks.

## what we replace

8 crates listed in §13 of spec. these do not appear in our workspace, dep graph, or `[patch.crates-io]`. examples: bevy_ui → prysm; bevy_scene → particles; bevy_asset → radio://; bevy_audio → acpu DSP; bevy_gltf → particle loader; bevy_ui_widgets/bevy_feathers/bevy_ui_render → prysm impl.

## what's new

24 crates listed in §14 of spec. these live in `crates/`. examples: evy_ecs_storage (the keystone), evy_engine_dispatch (multi-engine DAG), evy_prysm_* (prysm impl), evy_glia, evy_radio.

## platform targets

- first-class: macOS M-series (direct distribution, no App Store)
- portable: Android (NDK + wgpu Vulkan + NNAPI), Windows ARM, Windows x86 desktop, AMD Strix Halo, Qualcomm X Elite, console APU, discrete-GPU PC
- explicit non-targets: iOS / iPadOS / Vision Pro (App Store unacceptable), WebGPU browser, single-threaded environments

## build (placeholder until workspace stabilizes)

```bash
cargo build --release --workspace
cargo test --workspace
```

target triples for cross-compile:
- aarch64-apple-darwin (Mac)
- aarch64-linux-android (Android NDK)
- x86_64-unknown-linux-gnu (Linux desktop)
- x86_64-pc-windows-msvc (Windows desktop)
- aarch64-pc-windows-msvc (Windows ARM)

## git workflow

- atomic commits — one logical change per commit
- conventional prefixes: feat:, fix:, refactor:, docs:, test:, chore:
- commit by default after completing a change
- never push without explicit request

## writing style

state what something is directly. never define by negation. never use bold (`**text**`) — bold is banned per cyber convention; use headings, tables, code, wiki-links.

## graph vocabulary

use root terms from the cybergraph, never aliases. particle not CID, neuron not user, cyberlink not edge. see [[cyber/cyberia/midao/dev]] for the full rule + substitution table.

## shell: nushell

use `nu -c '...'` or `nu script.nu` for scripting. reserve bash only for git and system tools.

## do not touch zones

- `Cargo.toml` `[patch.crates-io]` section — the dep redirection topology; modifications can break the whole workspace silently
- `forks/<crate>/Cargo.toml` `version` field — keep at "0.18.1" for crates.io patch compatibility (revisit when bumping bevy)
- LICENSE.md — cyber license, not editable
- `specs/evy.md` — canonical spec; discuss before structural changes

## license

cyber license: don't trust. don't fear. don't beg.
