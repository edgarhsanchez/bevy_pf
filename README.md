# bevy_pf

A **XAML / WPF-like UI framework for [Bevy](https://bevy.org)**. Write your UI
in XAML — inline in Rust via the `xaml!` macro or in separate `.xaml` files —
style it with resources, and use the familiar WPF control and panel set inside
Bevy apps and games.

> **Avalonia XAML runs here too.** The companion repo
> [bevy_pf_avalonia](https://github.com/edgarhsanchez/bevy_pf_avalonia) loads the
> official Avalonia sample suite **unmodified** and reports what is missing —
> see [docs/avalonia-dialect.md](docs/avalonia-dialect.md) for the mapping.

```rust
use bevy::prelude::*;
use bevy_pf::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(PfUiPlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.spawn_xaml(xaml!(
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                       Margin="24" Spacing="8">
             <TextBlock Text="Hello from XAML" FontSize="28" FontWeight="Bold"/>
             <Button x:Name="Go" Content="Click me" Width="140"/>
           </StackPanel>"#
    ));
}
```

Malformed XAML is a **compile error** — both macros parse and validate the
markup at build time. `.xaml` files are tracked by the compiler, so editing
them triggers rebuilds.

## Crates

| Crate | Purpose |
|---|---|
| `bevy_pf` | The Bevy plugin: instantiation, panels, controls, styling runtime |
| `bevy_pf_xaml` | Bevy-free XAML parser + WPF value types/type converters |
| `bevy_pf_macros` | `xaml!` and `include_xaml!` proc macros |

## Status

Early but real: targeting **Bevy 0.19** and **Rust 1.95+** (edition 2024).

Working today:

- **XAML parser** with WPF semantics: namespaces (`xmlns`, `xmlns:x`,
  Avalonia's namespace as an alias), property-element syntax
  (`<Button.Content>`), attached properties (`Grid.Row="1"`), markup
  extensions (`{StaticResource}`, `{DynamicResource}`, `{x:Null}`, nested
  extensions, `{}` escapes), `x:Name`/`x:Key`/`x:Class`, `mc:Ignorable`
  design-time attribute skipping, whitespace rules, UTF-8 BOM tolerance.
- **Type converters**: all 141 named colors, `#RGB/#ARGB/#RRGGBB/#AARRGGBB`,
  `sc#`, Thickness, CornerRadius, GridLength (`Auto`/`*`/`2.5*`/px), Point,
  Size, Rect, Duration/TimeSpan, FontWeight (names + numeric), and the common
  enums — all case-insensitive like WPF.
- **Panels**: `Grid` (star/auto/pixel tracks, row/column spans, WPF overlap
  semantics, out-of-range clamping, `.NET 10`/Avalonia `RowDefinitions="Auto,*"`
  shorthand), `StackPanel` (+WinUI `Spacing`), `WrapPanel`, `Canvas`
  (absolute positioning), `DockPanel` (approximation, faithful layout planned),
  `Border`, `ScrollViewer` (basic).
- **Controls**: `TextBlock` (inline text flattening, font properties,
  wrapping/alignment), `Button` (hover/pressed chrome via `Interaction`),
  `Label`, `Image`, `Window`/`Page`/`UserControl` roots (Window `Title` is
  applied to the primary window).
- **Resources & styling**: lexically scoped `ResourceDictionary`,
  `StaticResource`, implicit styles by `TargetType`, named styles, `BasedOn`
  inheritance, `Setter`s (including attached like `DockPanel.Dock`),
  `x:Double`/`x:String` primitives, solid + linear/radial gradient brushes.
- **WPF property inheritance** for font properties (`FontSize`, `Foreground`,
  `FontFamily`, `FontWeight`, `FontStyle`) down the element tree.
- **Element identity + queries**: every XAML identity becomes a plain ECS
  component — `x:Name`/`Name` → `PfName` (plus the `XamlNames` namescope map
  on the scene root, WPF `FindName`-style), `x:Uid` → `PfUid`,
  `AutomationProperties.AutomationId` → `PfAutomationId`, and the element's
  XAML type → `PfElementKind` — so ordinary Bevy queries always work, and any
  system can locate any element. The `PfQuery` `SystemParam` bundles the
  lookups (`by_name`, `by_uid`, `by_automation_id`, `by_kind`, `named_in`,
  `scope_root`, `first_text_in`); to *change* what you find, mutate components
  directly (e.g. `PfProgress.value`, `Text`) or call
  `bevy_pf::provider::set_local` to set store-managed properties at the same
  precedence tier as a XAML attribute. Attach observers
  (`On<Pointer<Click>>`) to found entities as usual.
- **Input & hit testing** follows WPF's rules rather than bevy's defaults. A
  `Panel` hit-tests only where it renders, so a null `Background` is
  transparent to clicks and `Background="Transparent"` is the idiom for
  "invisible but clickable" — a layout `Grid` wrapped around a screen no
  longer silently eats every click aimed beneath it.
  `IsHitTestVisible="False"` opts an element *and its descendants* out, the
  way WPF's walk does. And `PfHitTest` (a `SystemParam`, the
  `VisualTreeHelper.HitTest` / `UIElement.InputHitTest` analog) answers
  "what is under this point?" from any system — for light dismiss, drag
  surfaces, hotspot cursors — getting the traps right by construction:
  `UiGlobalTransform` rather than the `GlobalTransform` bevy_ui no longer
  writes for UI nodes, logical-pointer to physical-geometry conversion, and
  `Visibility` (a closed panel keeps full layout geometry and must not
  answer for it).
- **Controls**: `Button`, `ToggleButton`, `CheckBox`, `RadioButton` (GroupName
  exclusivity), `TextBox` (native Bevy `EditableText`: typing, selection,
  clipboard, IME), `Slider`, `ProgressBar`, `Separator`,
  `ListBox`/`ListBoxItem` (selection + hover), `ItemsControl`, `GroupBox`,
  `Expander`, `Label`, `Image`, `ScrollViewer`, `ComboBox`,
  `TabControl`/`TabItem`, `TreeView`/`TreeViewItem` (expansion + selection),
  `Menu`/`MenuItem` (nested submenus) + `ContextMenu` (right-click),
  `DataGrid` (text columns bound to a view-model), `ToolTip`, `Viewbox`,
  `StatusBar`, `ToolBar`, `ListView`+`GridView` (display-member and
  template columns), `GridSplitter` (drag-resizes grid tracks), `Popup`,
  `Hyperlink` (opens `NavigateUri`), `ProgressBar IsIndeterminate`,
  `Calendar` + `DatePicker`, and `bevy_pf::dialog` MessageBox-style modals.
- **Ecosystem "toolkit" controls** (MahApps / MaterialDesignInXaml /
  Extended WPF Toolkit / HandyControl / WinUI equivalents): `ToggleSwitch`,
  `NumericUpDown`, `RatingBar`, `Badge`, `Card`/`Chip` presets,
  `BusyIndicator`, `TextBox Watermark=`, `RangeSlider`, `bevy_pf::toast`
  notifications, `TimePicker`, `ColorPicker` (palette + hex entry),
  `AutoSuggestBox`, `NavigationView` (pane items drive an embedded `Frame`),
  `bevy_pf::dialog::show_content` ContentDialog, and `PackIcon` /
  `SymbolIcon` / `FontIcon` — 40+ named icons drawn as vector paths by the
  shape engine (`bevy_pf::icons`), so they render identically on native and
  wasm with no icon font.
- **ControlTemplate**: re-template any Button-family control, CheckBox,
  RadioButton, Label, or TextBox — keyed/implicit-style/inline delivery,
  `ContentPresenter` content projection, live `{TemplateBinding}`,
  `<ControlTemplate.Triggers>` with `TargetName` setters, per-expansion
  namescopes (`GetTemplateChild` via `PfTemplateParts`), `PART_ContentHost`,
  and WPF's verbatim value-precedence tiers. Acceptance-gated against the
  real dotnet/wpf Aero2 Button theme fragment, which instantiates and
  functions unadapted (docs/controltemplate-plan.md).
- **Data binding**: `{Binding Path, Mode, StringFormat}` against any
  `#[derive(Reflect)]` view-model wrapped in a `Bindable` (the
  `INotifyPropertyChanged` analog), `DataContext` inherited down the tree,
  OneWay + TwoWay (TextBox text, CheckBox `IsChecked`, Slider `Value` write
  back), reflection paths (`Player.Name`, `Items[0].Score`). Bindable targets
  include `Text`/`Content`, `Visibility`, `Width`/`Height`, `FontSize`, and the
  paint properties `Foreground`, `Background`, `BorderBrush`, and (on shapes)
  `Stroke` / `Fill` — so a single `DataTemplate` can render each row of a list,
  or each cell of a chamfered `Path` strip, in that row's own status colour
  without a `DataTrigger` per status. Shapes re-rasterize on a paint change
  even when their size is unchanged.
- **Vector graphics / WPF Shapes**: `Rectangle`, `Ellipse`, `Line`,
  `Polyline`, `Polygon`, and `Path` with the full geometry mini-language
  (`M/L/H/V/C/S/Q/T/A/Z`, `F0/F1`, relative commands, smooth-curve
  reflection), gradient fills/strokes, `Stretch` modes — rasterized with
  tiny-skia at laid-out pixel size.
- **Runtime `.xaml` assets**: `XamlView(handle)` instantiates on load and
  rebuilds on file change (`--features hot_reload`); merged dictionaries are
  prefetched as load dependencies, so editing a theme file reloads every view
  using it.
- **Resource system**: `ResourceDictionary.MergedDictionaries` with `Source=`
  across files (all five WPF URI spellings), application-level resources,
  and real deferred `{DynamicResource}` — theme swaps re-resolve live, and
  keys that appear late (Fluent's pattern) apply when merged.
- **Value-provider store + Style.Triggers**: per-property precedence using
  WPF's verbatim `BaseValueSourceInternal` order; `Trigger`, `DataTrigger`,
  and `MultiTrigger` evaluate at runtime (hover/pressed/checked/enabled/
  selected + view-model comparisons) with structural revert — deactivating a
  trigger restores the style setter, theme value, or control chrome beneath
  it, and local values always win.
- **Popup layer**: a top-Z overlay hosting dropdowns and tooltips, positioned
  against anchors after layout, with light-dismiss backdrops; popup content
  inherits DataContext/resources through logical-parent links, like WPF's
  logical tree. `ComboBox` (static items or `ItemsSource`, `SelectedIndex`,
  `DisplayMemberPath`) and `ToolTip` build on it.
- **ItemsSource + DataTemplate**: bind any reflected `Vec` to
  `ListBox`/`ItemsControl`/`ComboBox`; templates expand per item with a
  scoped `DataContext` (`{Binding name}` inside a template reads
  `players[i].name`, TwoWay writes back into the list element).

- **Navigation**: WPF `Frame` + `Page` + the journal on Bevy — register
  routes with `app.register_page("home.xaml", xaml!(...))`, point a
  `<Frame Source="home.xaml"/>` at one, and `<Hyperlink NavigateUri>` with a
  relative URI navigates the enclosing frame (absolute `http(s)` still opens
  the browser, like WPF). Back/forward chrome, `navigate`/`go_back`/
  `go_forward`, `PfNavigated` messages, `Title` tracking, and WPF's
  `KeepAlive="False"` semantics — pages re-instantiate, state lives in the
  `DataContext`. See `--example navigation`.
- **Themes**: 12 built-in themes with palettes from the official specs —
  Fluent (Windows 11) light/dark, Material light/dark, Nord, Dracula,
  Catppuccin Latte/Mocha, Solarized light/dark, Gruvbox, Tokyo Night. One
  `Pf.*` brush-key contract + implicit styles via `{DynamicResource}`, so
  `bevy_pf::themes::apply_theme(world, "catppuccin-mocha")` re-colors a live
  UI in place. See `--example theme_gallery`.

Planned next: ControlTemplate/theme parity (the store's template tiers are
already reserved; Fluent.Light.xaml is the acceptance gate), EventTrigger/
Storyboard animations, VisualStateManager, granular list diffing
(ObservableList), GPU vector backend (vello) once it reaches Bevy 0.19.

## Vector graphics decision

Bevy 0.19's `bevy_ui` natively renders rounded-rect chrome, per-side borders,
box shadows, and linear/radial/conic gradients — enough for all standard
control visuals without a custom renderer. It does **not** rasterize arbitrary
vector paths; WPF `Shapes`/`Path` support will come from lyon tessellation
(`bevy_prototype_lyon`/`bevy_svg` are Bevy-0.19-compatible today; `bevy_vello`
still targets 0.18). See `docs/bevy-ui-research.md`.

## Examples

```sh
cargo run -p bevy_pf --example hello_xaml       # smallest app, inline xaml!
cargo run -p bevy_pf --example styling          # resources, implicit styles, BasedOn
cargo run -p bevy_pf --example grid_layout      # Grid tracks/spans + named-button click handler
cargo run -p bevy_pf --example xaml_file        # include_xaml!("....xaml")
cargo run -p bevy_pf --example controls_gallery # the whole control set
cargo run -p bevy_pf --example world_space_ui   # diegetic XAML panels in a 3D scene
cargo run -p bevy_pf --example shapes           # vector shapes + path mini-language
cargo run -p bevy_pf --example data_binding     # MVVM view-model binding, TwoWay
cargo run -p bevy_pf --example triggers_theming # Style.Triggers + light/dark theme swap
cargo run -p bevy_pf --example items_and_dropdowns # ItemsSource + DataTemplate + ComboBox + ToolTip
cargo run -p bevy_pf --example app_shell        # Menu bar, TabControl, TreeView, DataGrid, ContextMenu
cargo run -p bevy_pf --example components_showcase # every component in one app + PfQuery live updates
cargo run -p bevy_pf --example theme_gallery    # 12 built-in themes, switchable live
cargo run -p bevy_pf --example navigation       # Frame/Page journal navigation, WPF-style
cargo run -p bevy_pf --example wpf_expense_it   # the official WPF ExpenseIt walkthrough, ported
cargo run -p bevy_pf --example wpf_samples_gallery # eight more official WPF samples in one gallery
cargo run -p bevy_pf --example rpg_hud          # RPG HUD kit: vitals, action bar, quests, loot toasts
cargo run -p bevy_pf --example breakout         # the Breakout game, XAML all the way down
cargo run -p bevy_pf --example hot_reload --features hot_reload  # live .xaml editing
```

## Performance

All 42 benchmark scenes (every control, plus baselines and a composite app
shell) render at **3,000–4,100 FPS** offscreen on an Apple M4 Pro with the
production release profile (fat LTO, single codegen unit) — above the
2,000 FPS gate — and bevy_pf's own systems cost ~10 µs/frame (~1 % of the
frame; the rest is stock Bevy). Two shipping knobs:
`bevy_pf::perf::tune_schedules_for_gui` (single-threaded schedule executors —
sub-millisecond GUI frames are dominated by dispatch overhead) and
`WinitSettings::desktop_app()` reactive rendering for native-toolkit idle CPU.
Methodology, per-control tables, Tracy workflow, and the cross-framework
comparison (including honest caveats): [docs/performance.md](docs/performance.md).
Reproduce with `--example perf_bench`.

## Where the XAML lives

Three ways to hand bevy_pf markup. Only the first involves a Rust string
literal, and `r#"..."#` is plain Rust — nothing to do with this crate. XAML
is full of `"` characters, so a normal Rust string would need every one of
them escaped; a raw string lets you paste the markup verbatim.

```rust
// 1. Inline, for a snippet.
commands.spawn_xaml(xaml!(r#"<Button Content="Go"/>"#));

// 2. A separate .xaml file, validated at COMPILE time. No string literal.
commands.spawn_xaml(include_xaml!("ui/main.xaml"));

// 3. At runtime, from a String (a loaded file, a generated document).
let scene = XamlScene::parse(std::fs::read_to_string("ui/main.xaml")?)?;
```

For anything past a snippet, prefer **2**: the markup lives in a file your
editor will syntax-highlight, and the macro still fails the build if it does
not parse.

### The `r#` gotcha

A raw string ends at the first `"#`, and XAML hex colours produce exactly
that sequence — `Value="#FF0000"` closes the literal early and you get a
cascade of confusing parse errors. Add hashes until the delimiter is unique:

```rust
r##"<Setter Property="Background" Value="#FF0000"/>"##
```

One more reason to keep real UI in `.xaml` files.

## Compatibility testing

`tests/corpus/` contains verbatim open-source XAML from
[microsoft/WPF-Samples](https://github.com/microsoft/WPF-Samples) and
[AvaloniaUI/Avalonia.Samples](https://github.com/AvaloniaUI/Avalonia.Samples)
(both MIT). Every corpus file must parse, and every WPF page must instantiate
headlessly with unsupported features degrading to warnings — never errors.
The corpus grows as features land.

### External (local-only) oracles

Proprietary XAML corpora — e.g. a commercial SDK's sample and theme set —
can be swept locally without copying anything into the repo:

```sh
BEVY_PF_EXTERNAL_XAML_DIRS=/path/to/sdk \
  cargo test -p bevy_pf_xaml --test external_corpus -- --ignored --nocapture
BEVY_PF_EXTERNAL_XAML_DIRS=/path/to/sdk \
  cargo test -p bevy_pf --test external_instantiate -- --ignored --nocapture
```

The parse sweep asserts every file parses — a 172-file commercial SDK
corpus: 172/172; dotnet/wpf:
all 168 well-formed files (33 build-time preprocessor fragments are excluded
as non-XML, one type-driven scanner DRT is skipped). The instantiation sweep
prints a warning histogram that ranks missing features by real-world usage
and asserts the warnings-never-errors invariant.

Related docs: `the WPF-conformance roadmap` (feature roadmap) and
`docs/wpf-conformance-notes.md` (source-level conformance audit against
dotnet/wpf: verified-conformant behaviors, known deviations, and the
template-system design decisions settled by the reference implementation —
including the exact `BaseValueSourceInternal` precedence order and PART_
contract table). `tests/corpus/wpf-upstream/` carries MIT-licensed files
harvested from dotnet/wpf itself, including .NET 9 Fluent theme dictionaries.

## License

MIT OR Apache-2.0.
