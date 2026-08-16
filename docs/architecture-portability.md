# Architecture: XAML on Rust, with or without Bevy

Written 2026-08-06 from a measured audit of this codebase (dependency trees,
a module-by-module coupling map, and the benchmark history in
bevy_pf_vector's project log), plus a survey of the 2026 Rust
graphics/UI ecosystem. This is the **portability track**; it composes with
the WPF-conformance track rather than
competing with it — one decision below (the property store) serves both.

## The position

bevy_pf's niche is real and unoccupied: nobody else does actual WPF-dialect
XAML on Rust. Slint has its own DSL, Xilem is code-first, Dioxus is RSX over
a webview. The closest prior art is a commercial native XAML renderer — C++
XAML middleware embedded in engines — and the lesson from its architecture
is exactly the one this document adopts: **the XAML runtime is a library
with pluggable hosts and renderers, not a feature of one engine.**

"Works anywhere Rust works" is approached bottom-up, not by abstracting
Bevy away. Layers become engine-free in dependency order, each layer
useful and shippable on its own, with the Bevy path remaining the primary,
measured product throughout.

## Ground truth (audited 2026-08-06)

Where the engine-independence boundary falls **today**:

| layer | status |
|---|---|
| `bevy_pf_xaml` — parser, AST, markup extensions, value types, path geometry, URIs | **Engine-free now.** Full dependency tree: roxmltree, memchr, thiserror (+ proc-macro build deps). 90+ unit tests, corpus tests. CI guards this (`.github/workflows/ci.yml`, `engine-free` job). |
| `bevy_pf_macros` — compile-time XAML | **Engine-free now.** Guarded by the same CI job. |
| `resources.rs` (1.8k lines) — ResourceDictionary/Style/Setter/Trigger/Storyboard parsing | **Already pure** — zero `bevy::` references. Movable to the engine-free layer with import rewrites once `PfStoryboard`/`PfEasing`/`PropertyTarget` move with it. |
| `provider.rs` property store — WPF dependency-property precedence | **The keystone, already pure.** `PfPropertyStore`/`PropertyTarget` are an enum-keyed precedence map with unit tests and no ECS coupling. The a commercial XAML renderer roadmap's "load-bearing decision" (2.6) is built, and built portable. Needs a `store`/`apply` file split, nothing more. |
| `binding.rs` core — paths, formatting, converters | **Pure over `bevy_reflect`**, which is a standalone ECS-free crate. The element-source half (`ElementName=`, write-back systems) is World-walking and stays with the host. |
| `shapes.rs` / `shapes_gpu.rs` geometry+raster cores | **Pure at the data boundary**: `rasterize_shape(&PfShape, w, h) -> pixels` and `shape_to_vector(&PfShape, size) -> (Vec<PathCommand>, PathStyle)`. `PfShape` needs only glam vectors. Everything around them is a thin Query/Assets shell. |
| easing, icons, util, error | Pure now. |
| `instantiate.rs` (8.5k), `plugin.rs`, items/overlay/dialog/toast/navigation/events/behaviors/caret (~13k lines) | **Inseparably Bevy**: the spawn path and runtime systems. Their entire job is `&mut World`. This is the Bevy *host*, and it stays that way. |

Honest ceiling without a rewrite: roughly 20% of the framework crate
(3.5–4.5k lines) plus everything already in `bevy_pf_xaml` becomes the
engine-free core. Cracking `instantiate.rs` beyond that requires a retained
visual-tree IR with an emit-to-host step — a rewrite, deliberately **not**
planned until the engine-free core has a second consumer proving the seam.

Two more load-bearing audit facts:

- **Layout is expressed exclusively through `bevy_ui::Node` styling** — no
  custom measure/arrange pass anywhere. bevy_ui computes it with **taffy**,
  a standalone crate. A non-Bevy host using taffy directly inherits
  identical layout semantics; `ComputedNode` is read back in only four
  places (shape sizing, overlay placement, ActualWidth/Height, viewbox).
- **Text is Parley underneath** (`fonts.rs` documents the stack). Parley is
  Linebender's engine-independent rich-text layout crate. A non-Bevy host
  uses parley directly — the same shaping and layout engine, not a clone.

## Target architecture

```
L0  bevy_pf_xaml (+ macros)      XAML → typed tree. Engine-free, CI-guarded. SHIPS TODAY.
L1  engine-free core (grown, not invented):
      resources: dictionaries, styles, setters, triggers, storyboard data
      property store: WPF precedence (local > animation > trigger > setter > …)
      binding core: paths + converters + formatting over bevy_reflect
      draw model: PfShape/PathCommand/Brush/StrokeStyle + rasterize_shape
      easing, icons, geometry math
L2  renderer backends (per-shape claim contract, shapes.rs module docs):
      tiny-skia   CPU, runs anywhere incl. wasm; measured fastest at UI scale
      vector_gpu  bevy_pf_vector instancing; measured crossover ~1000 shapes
      bevy_ui     native nodes for the rect/ellipse subset; zero-cost
      (vello / vello_cpu: candidate, only if a measured gap appears)
L3  hosts:
      bevy (instantiate.rs + systems)   — the primary product, unchanged
      headless render-to-image          — first non-Bevy consumer (M2)
      winit + wgpu/softbuffer runner    — full standalone (M4, gated on M2/M3)
```

## Rust ecosystem choices, with reasons

- **taffy** for layout everywhere. Not "compatible with" bevy_ui — it *is*
  bevy_ui's engine, used standalone by the non-Bevy hosts. One layout
  implementation, every host.
- **parley** for text in non-Bevy hosts — the same engine Bevy uses here.
  (The ecosystem is consolidating on it: egui is migrating, and it is the
  Linebender standard. Do not adopt cosmic-text and maintain two stacks.)
- **bevy_reflect** stays the binding substrate even outside Bevy — it is a
  standalone crate, and the reflection-path work in `binding.rs` ports
  as-is. Do not invent a property system.
- **glam** vector types in the core (what `PfShape` already needs); no
  Bevy math dependency.
- **tiny-skia** remains the default rasterizer. This is a measured
  position, not a default: CPU rasterization beat the GPU atlas below
  ~1000 shapes in-app, and the wider ecosystem agrees (Blend2D's results,
  vello_cpu's existence). Revisit only against the benchmark discipline
  below.
- **wgpu** remains the GPU substrate (bevy_pf_vector is wgpu-portable, no
  exotic features). vello's sparse-strips rewrite (vello_cpu/hybrid) is
  the most interesting external development — API still unstable; watch,
  don't adopt yet.
- **winit + accesskit** when the standalone host lands (M4): the standard
  windowing and accessibility layers, both Bevy-compatible, so nothing
  forks.
- **wasm stays first-class**: the pure layers keep compiling to
  wasm32-unknown-unknown (the site already proves the whole stack there).

## Renderer seam (landed 2026-08-06)

Backends inside the Bevy host follow the claim contract in `shapes.rs`:
claim shapes in `PfShapeSystems::Claim` (priority-ordered, cheapest first),
render them into whatever bevy_ui composites; unclaimed shapes fall through
to the CPU rasterizer in `PfShapeSystems::Rasterize`. tiny-skia is the
fallback of last resort, not "the" renderer. The same `PfShape` data model
is what L1 exposes to non-Bevy hosts, so a renderer written against it
serves both worlds.

## Milestones

Each is independently shippable; none breaks the Bevy product. Do not
start a milestone before the previous one is verified **in the game**
(friginrain2) and in CI — the project log records three regressions from
skipping that step.

- **M0 — seams and guards (DONE, this commit).** Backend claim contract;
  CI: engine-free guard on `bevy_pf_xaml`/`bevy_pf_macros`, all feature
  combos type-checked, test suite green.
- **M1 — carve the core.** Move `resources.rs`, the pure property store,
  binding core, draw model, easing/icons into an engine-free `bevy_pf_core`
  crate (bevy_pf re-exports; zero behavior change; the diff is imports).
  Acceptance: `cargo tree -p bevy_pf_core` shows no bevy; bevy_pf tests
  unchanged; CI guard extended to the new crate.
- **M2 — first non-Bevy consumer: headless XAML→image.** A small binary
  crate: parse XAML (L0), resolve styles/resources (L1), lay out with
  taffy, draw shapes with tiny-skia, text with parley, emit PNG. Scope is
  the static subset (panels, shapes, text, styles) — no interactivity.
  This is the "XAML anywhere" proof and doubles as a golden-image test
  harness for the conformance track.
- **M3 — binding + property store driven headlessly.** The store and
  binding core running against plain Rust view-models outside Bevy
  (they are pure already; this is wiring plus tests). Unlocks server-side
  or test-driven UI logic with no engine.
- **M4 — standalone interactive host.** winit + input + focus + parley
  editing + accesskit over L1/L2. Large; only starts once M2's consumer
  has forced the L1 API into shape. The Bevy host's 13k lines are the
  spec for what this host must provide.

## Rules

1. The real application is the benchmark. No renderer or architecture
   claim ships on microbenchmarks; A/B in the consuming app, curve not
   point, transients included.
2. No abstraction without a second consumer. The visual-tree IR (the
   only path to porting `instantiate.rs`) waits until M2's consumer
   exists to prove what the IR must express.
3. Purity is CI-enforced, not aspirational. Any crate documented as
   engine-free has a `cargo tree` guard in CI from the day it exists.
4. The Bevy host is the product. Portability work that regresses it —
   performance, behavior, or ergonomics — reverts, per the project log's
   standing precedent.
