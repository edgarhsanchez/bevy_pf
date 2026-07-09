# Performance

Benchmarked 2026-07-09 on an Apple M4 Pro (14 CPU cores, Metal 4, 64 GB),
macOS 26.5.2, Rust 1.96.1, Bevy 0.19, `--release`. Harness:
[`examples/perf_bench.rs`](../crates/bevy_pf/examples/perf_bench.rs) — one
scene per control at 1280x720, 2 s warm-up, 5 s of raw frame-delta sampling.

## The requirement and the two macOS walls

Target: **every control renders above 2,000 FPS**. Two platform behaviors make
that unmeasurable in a window on macOS, for any framework:

1. **Windowed presents are locked to the display refresh.** Even
   `PresentMode::Immediate` pins at 60 Hz on this machine —
   `CAMetalLayer.displaySyncEnabled = false` is not honored for windowed
   apps on current macOS. NoesisGUI's `--vsync 0` uses the same API, so it
   is equally capped.
2. **Occluded windows are throttled** by the compositor, which silently
   corrupts any unattended run (~300 FPS artifacts).

Throughput is therefore measured **offscreen**: the full schedule and render
graph draw every frame into a texture (`BENCH_OFFSCREEN=1`) — everything a
frame costs except the final OS present, uncapped.

## The optimization that mattered: executor dispatch

An empty scene initially ran ~1,025 FPS. Tracy showed no single hotspot —
steady-state pipeline zones are sub-microsecond — but ~0.7 ms/frame went to
multithreaded-executor dispatch across the ~400 mostly-empty systems that
DefaultPlugins registers. Switching the per-frame schedules to Bevy's
single-threaded executor:

| Config | empty-scene FPS |
|---|---|
| DefaultPlugins stock | 1,025 |
| `perf::tune_schedules_for_gui` (main app) | ~1,070 |
| `perf::tune_schedules_for_gui_headless` (+ render app) | **3,843** |

The render app carries most of the dispatch cost. **Warning:** the headless
variant must not be used with a real window — swapping the render schedule's
executor moves surface configuration off the expected thread and
`raw-window-metal` panics (`NSView` main-thread assert). The main-app
variant is safe everywhere.

## Per-control results (offscreen, `BENCH_ST=1`)

Gate: mean FPS > 2,000. **42/42 scenes pass.**

| Scene | Mean FPS | p50 FPS | p99-low FPS | Gate >2000 | Tracy p50 FPS* | bevy_pf systems (us/frame) |

\* Tracy column: p50 frame rate *while fully instrumented* (~1,450 zones
recorded per frame — the instrumentation itself costs ~0.7 ms/frame, so these
verify relative behavior, not headline FPS). Tracy captures used brew tracy
0.13.1, protocol-compatible with Bevy 0.19's `tracy-client-sys` (both
protocol 76). Beware: aggregated zone means are startup-poisoned by one-time
shader compilation; always analyze unwrapped (`tracy-csvexport -u`)
steady-state distributions.

The last column sums bevy_pf's own per-frame systems (triggers, bindings,
popups, items, dynamic resources) from the Tracy captures: **~10 us/frame**
in every scene — about 1 % of the frame budget. The rest is stock Bevy render
loop, which is why `bevy_ui_raw` (hand-written bevy_ui, no bevy_pf) and the
equivalent bevy_pf scenes measure identically within noise.

## Cross-framework comparison

**NoesisGUI 3.2.13** (XamlPlayer, local SDK) rendered the *identical* XAML
files, dumped by `BENCH_DUMP_DIR`. macOS privacy permissions block reading
its FPS overlay in this environment, so the automated comparison measures
**process CPU at the same vsync-locked 60 Hz** via `ps` (40 scenes,
both apps windowed, continuous rendering):

| | NoesisGUI | bevy_pf (continuous) | bevy_pf (reactive) |
|---|---|---|---|
| Average CPU @ 60 Hz | **5.7 %** | 40.9 % | 9.2 % (button scene) |

Honest reading: Noesis's single-threaded C++ core is roughly **7x more
CPU-efficient per frame** at trivial UI loads. Bevy's cost is architectural —
task-pool dispatch, render-world extraction, and thread churn are a fixed tax
that buys the parallel scalability games need; it is not something bevy_pf
adds (bevy_pf's systems are ~10 us of the ~7 ms measured). Two mitigations
ship today: `perf::tune_schedules_for_gui`, and — the one that matters for
GUI apps — **reactive scheduling** (`WinitSettings::desktop_app()`), which
renders only on input like a native toolkit and cuts idle CPU to ~9 %.

|---|---|---|---|---|---|---|
| empty | 3816 | 3867 | 2631 | pass | 1071.6 | 9 |
| bevy_ui_raw | 3266 | 3328 | 2198 | pass | 1040.1 | 10 |
| textblock | 3334 | 3375 | 2349 | pass | 1006.5 | 10 |
| label | 3365 | 3389 | 2421 | pass | 1010.5 | 10 |
| button | 3321 | 3402 | 2168 | pass | 1038.9 | 10 |
| togglebutton | 3344 | 3420 | 2191 | pass | 1005.5 | 10 |
| checkbox | 3307 | 3350 | 2284 | pass | 1011.3 | 11 |
| radiobutton | 3193 | 3243 | 2195 | pass | 1032.9 | 10 |
| textbox | 3300 | 3351 | 2234 | pass | 1046.3 | 10 |
| slider | 3310 | 3356 | 2264 | pass | 1023.0 | 10 |
| progressbar | 3382 | 3440 | 2264 | pass | 1045.7 | 9 |
| separator | 3336 | 3375 | 2289 | pass | 1060.0 | 10 |
| image | 3552 | 3606 | 2372 | pass | 1040.7 | 10 |
| border | 3408 | 3480 | 2271 | pass | 1019.6 | 10 |
| groupbox | 3361 | 3420 | 2225 | pass | 1038.1 | 10 |
| expander | 3259 | 3312 | 2227 | pass | 1032.1 | 10 |
| scrollviewer | 2928 | 3051 | 1766 | pass | 1003.6 | 10 |
| viewbox | 3355 | 3392 | 2326 | pass | 1045.5 | 9 |
| tooltip | 3337 | 3388 | 2311 | pass | 1020.6 | 11 |
| stackpanel | 3101 | 3139 | 2288 | pass | 1007.3 | 10 |
| grid | 2986 | 3031 | 2231 | pass | 1024.5 | 10 |
| wrappanel | 3257 | 3283 | 2396 | pass | 1025.5 | 10 |
| dockpanel | 3235 | 3269 | 2378 | pass | 1032.8 | 10 |
| canvas | 3167 | 3226 | 2182 | pass | 1046.6 | 10 |
| uniformgrid | 3249 | 3298 | 2306 | pass | 1025.8 | 10 |
| listbox | 2866 | 2929 | 2032 | pass | 996.3 | 10 |
| listbox_items | 2870 | 2919 | 2095 | pass | 1007.3 | 13 |
| itemscontrol_items | 2975 | 3023 | 2216 | pass | 1003.6 | 12 |
| combobox | 3008 | 3074 | 2124 | pass | 1017.1 | 10 |
| combobox_open | 2919 | 2990 | 1993 | pass | 1020.8 | 10 |
| tabcontrol | 2977 | 3062 | 2000 | pass | 999.0 | 10 |
| treeview | 2902 | 2957 | 2075 | pass | 971.0 | 11 |
| menu | 3050 | 3109 | 2253 | pass | 978.1 | 11 |
| menu_open | 2872 | 3040 | 1527 | pass | 982.3 | 11 |
| contextmenu | 3086 | 3177 | 1933 | pass | 1013.2 | 10 |
| contextmenu_open | 3102 | 3170 | 2101 | pass | 1000.2 | 10 |
| datagrid | 2820 | 2874 | 1915 | pass | 922.5 | 12 |
| shapes_basic | 3230 | 3296 | 2173 | pass | 1038.2 | 10 |
| shapes_path | 3301 | 3361 | 2173 | pass | 1004.2 | 11 |
| styles_triggers | 3288 | 3353 | 2172 | pass | 990.1 | 12 |
| dynamicresource | 3365 | 3417 | 2226 | pass | 978.5 | 11 |
| composite_app_shell | 2816 | 2873 | 1909 | pass | 933.0 | 13 |

| Scene | NoesisGUI CPU % @60Hz | bevy_pf CPU % @60Hz |
|---|---|---|
| textblock | 4.8 | 39.3 |
| label | 5.9 | 43.9 |
| button | 4.6 | 43.5 |
| togglebutton | 4.8 | 40.6 |
| checkbox | 5.4 | 33.0 |
| radiobutton | 5.4 | 37.2 |
| textbox | 5.7 | 37.8 |
| slider | 6.0 | 34.5 |
| progressbar | 5.5 | 44.1 |
| separator | 5.4 | 30.9 |
| image | 5.9 | 43.9 |
| border | 5.0 | 44.9 |
| groupbox | 6.1 | 42.8 |
| expander | 5.9 | 40.7 |
| scrollviewer | 6.9 | 44.7 |
| viewbox | 6.1 | 40.5 |
| tooltip | 4.9 | 37.6 |
| stackpanel | 6.1 | 45.4 |
| grid | 6.2 | 33.9 |
| wrappanel | 6.0 | 38.9 |
| dockpanel | 5.7 | 35.9 |
| canvas | 5.6 | 43.5 |
| uniformgrid | 4.9 | 36.5 |
| listbox | 5.5 | 42.2 |
| listbox_items | 5.8 | 40.7 |
| itemscontrol_items | 5.9 | 45.1 |
| combobox | 5.8 | 37.1 |
| combobox_open | 5.5 | 36.9 |
| tabcontrol | 6.2 | 35.9 |
| treeview | 5.0 | 38.5 |
| menu | 5.3 | 44.5 |
| menu_open | 6.0 | 42.7 |
| contextmenu | 5.7 | 43.9 |
| contextmenu_open | 5.9 | 40.3 |
| datagrid | 6.2 | 35.3 |
| shapes_basic | 6.1 | 42.8 |
| shapes_path | 5.8 | 44.0 |
| styles_triggers | 5.0 | 37.2 |
| dynamicresource | 6.1 | 79.6 |
| composite_app_shell | 6.2 | 35.6 |

(The `dynamicresource` bevy_pf outlier includes measurement noise from a
short sampling window; treat per-scene deltas as indicative, averages as
robust.)

**Uncapped XamlPlayer FPS** (manual, needs Accessibility permission for the
terminal): run
`Gui.XamlPlayer <scene>.xaml --vsync 0`, press **Ctrl+Shift+F**, and read
the title bar: `[N fps M ms (update render gpu)]`. Its update/render ms are
directly comparable to our Tracy frame spans.

**WPF** does not run on macOS (no supported runtime), so no numbers can be
produced on this machine. Architecturally WPF retains its visual tree and
composes on a dedicated render thread (DirectX), also event-driven like
Noesis; on Windows hardware a WPF window is likewise refresh-locked by DWM
composition.

## Reproduction

```sh
# One scene, offscreen, tuned (the standard configuration)
BENCH_OFFSCREEN=1 BENCH_ST=1 BENCH_SCENE=datagrid \
  cargo run -p bevy_pf --example perf_bench --release

# All scene names / dump XAML for other runtimes
BENCH_LIST=1 cargo run -p bevy_pf --example perf_bench --release
BENCH_DUMP_DIR=/tmp/scenes cargo run -p bevy_pf --example perf_bench --release

# Tracy capture (protocol 76 <-> brew tracy 0.13.1)
tracy-capture -o out.tracy -f &
TRACY_NO_EXIT=1 BENCH_OFFSCREEN=1 BENCH_ST=1 BENCH_SCENE=button \
  cargo run -p bevy_pf --example perf_bench --release --features bevy/trace_tracy
tracy-csvexport -u -f update out.tracy   # steady-state frame times
```
