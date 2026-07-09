# Bevy 0.19 UI & vector graphics research (July 2026)

Condensed findings that drive bevy_pf's architecture. Full API details verified
against the crates.io sources of bevy 0.19.0.

## Versions

- **bevy 0.19.0** (2026-06-19), requires **rustc 1.95+**. Taffy 0.10 layout
  (flexbox + CSS grid + block), parley 0.9 text (replaced cosmic-text in 0.19).
- Vector crates: `bevy_prototype_lyon 0.17` (bevy ^0.19), `bevy_svg 0.19`
  (bevy ^0.19), `bevy_vello 0.13.1` (bevy ^0.18 — NOT yet on 0.19),
  `vello 0.9`.

## What bevy_ui 0.19 can render (no custom renderer needed)

- Rounded rects: `Node.border_radius` (a *field* of `Node` since 0.18).
- Borders: `Node.border: UiRect` + `BorderColor` (per-side, `BorderColor::all`).
- `BoxShadow(Vec<ShadowStyle>)`, `TextShadow`, `Outline`.
- Gradients: `BackgroundGradient`/`BorderGradient` with
  `Gradient::{Linear,Radial,Conic}`, `ColorStop { color, point: Val, hint }`.
- Custom shaders: `UiMaterial` + `MaterialNode(Handle<M>)`.
- Scrolling: `Overflow::scroll_y()`, `ScrollPosition`, scrollbar widgets.
- Text editing: native `bevy::text::EditableText` (0.19) — typing, selection,
  clipboard, IME, multiline. No placeholder/undo/password yet.
- Focus: `bevy_input_focus` (`InputFocus`, `TabIndex`, tab navigation);
  `InputDispatchPlugin` is in DefaultPlugins since 0.19.
- Headless widgets: `bevy_ui_widgets` (experimental, first-party): Button,
  Checkbox, RadioButton/Group, Slider, Scrollbar, ListBox, Menu*, with
  `Activate`/`ValueChange<T>` events and `Pressed`/`Checked`/
  `InteractionDisabled` state components.
- Styled widget set: `bevy_feathers` (experimental) with `UiTheme` tokens.

**Not supported**: arbitrary vector paths/tessellation in UI nodes. WPF
Shapes/`Path` need lyon tessellation (meshes are world-space; UI integration
needs custom extraction) or a vello texture target. Decision: control chrome
uses native bevy_ui; a Shapes layer comes later via tessellation.

## Key 0.19 API shapes (verified from source)

- `Node { width: Val, border: UiRect, border_radius: BorderRadius,
  grid_template_rows: Vec<RepeatedGridTrack>, grid_row: GridPlacement,
  align_self, justify_self, justify_items, row_gap, ... }`
- `GridTrack::{auto,fr,px}::<RepeatedGridTrack>()`,
  `GridPlacement::start_span(start: i16, span: u16)` (1-based).
- `TextFont { font: FontSource, font_size: FontSize::Px(f32),
  weight: FontWeight(u16), style: FontStyle::{Normal,Italic,Oblique(Option<f32>)},
  ... }`, `FontSource::{Handle,Family(SmolStr),Serif,SansSerif,Monospace}`.
- `Text::new(...)` (UI), `TextColor`, `TextLayout { justify: Justify,
  linebreak: LineBreak }`.
- Picking: `.observe(|click: On<Pointer<Click>>| ...)`; `Interaction`
  (`None/Hovered/Pressed`) still exists for polling.
- Events split (0.17): `Event` / `EntityEvent` / `Message`
  (`MessageReader`/`MessageWriter`).
- BSN (`bsn!`) is the forward-looking scene DSL; classic Bundle spawning is
  not deprecated.

## Migration traps (0.15 → 0.19)

- 0.15: `NodeBundle`→`Node` (required components).
- 0.16: `Pointer<Down/Up>`→`Pointer<Pressed/Released>`.
- 0.17: `Trigger`→`On`, `EventReader/Writer`→`MessageReader/Writer`,
  UI `Transform`→`UiTransform`, gradients added, `px()/percent()` helpers.
- 0.18: `BorderRadius` component → `Node` field; `LineHeight` split from
  `TextFont`; only glyphs of text nodes are pickable.
- 0.19: cosmic-text→parley (`FontSource`, `FontSize` enum, byte span indices
  gone), `Font::from_bytes`, feathers/widgets folded into DefaultPlugins.
