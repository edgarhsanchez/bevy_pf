//! The Bevy plugin and the `Commands` extension for spawning XAML.

use bevy::prelude::*;
use bevy::ui::{BorderColor, Checked};
use bevy::ui_widgets::{SliderRange, SliderValue};

use crate::XamlScene;
use crate::components::*;
use crate::instantiate::instantiate_document;

/// Selected list item background (Windows 10 style).
/// Stock WPF selection fill — the [`crate::components::PfControlTheme`]
/// default; kept as a fallback for `World`-only paths.
pub(crate) const LIST_SELECTED_BG: Color = Color::srgb(0.796, 0.909, 0.964); // #CBE8F6

/// Adds bevy_pf's runtime systems (control interaction visuals, and later
/// bindings, triggers, and asset loading).
///
/// The headless widget behavior (checkbox toggling, slider dragging, text
/// input) comes from Bevy's first-party `UiWidgetsPlugins`, which
/// `DefaultPlugins` already includes when the `bevy_ui_widgets` feature is
/// enabled (bevy_pf enables it). Missing plugins are added here defensively.
pub struct PfUiPlugin;

impl Plugin for PfUiPlugin {
    fn build(&self, app: &mut App) {
        crate::fonts::register_builtin_fonts(app);
        app.add_message::<crate::dialog::PfDialogResult>();
        app.add_message::<crate::binding::PfCommandInvoked>();
        app.add_message::<crate::navigation::PfNavigated>();
        app.init_resource::<crate::navigation::PfPages>();
        app.add_systems(Update, crate::navigation::init_pending_frames);
        app.add_systems(
            Update,
            (
                toolkit_control_sync,
                auto_suggest_watch,
                color_hex_watch,
                password_watch,
                repeat_buttons,
                crate::toast::expire_toasts,
            ),
        );
        app.init_resource::<crate::animation::PfRunningAnimations>();
        app.init_resource::<crate::caret::PfCaretBlink>();
        // blink_carets reads this to gate its ancestor walk; the opacity path
        // only creates it lazily, so an app that never fades anything must
        // still find it or the system fails parameter validation.
        app.init_resource::<crate::provider::PfOpacityCount>();
        if !app.world().contains_resource::<crate::binding::PfConverters>() {
            app.insert_resource(crate::binding::builtin_converters());
        }
        app.add_systems(
            Update,
            (
                crate::animation::start_pending_storyboards,
                crate::behaviors::run_pending_actions,
                crate::behaviors::run_key_triggers,
                crate::animation::drive_visual_states,
                crate::animation::tick_animations,
                crate::caret::blink_carets,
            )
                .chain(),
        );
        app.init_asset::<crate::asset::XamlAsset>()
            .register_asset_loader(crate::asset::XamlAssetLoader)
            .init_resource::<crate::asset::PendingXamlViews>()
            .init_resource::<crate::dynamic::PfApplicationResources>()
            .init_resource::<crate::dynamic::LastDynRevision>()
            .add_systems(
                Update,
                (crate::asset::queue_xaml_views, crate::asset::apply_xaml_views).chain(),
            )
            .add_systems(
                Update,
                crate::dynamic::refresh_dynamic_resources
                    .after(crate::asset::apply_xaml_views),
            );
        // The headless widget plugins need input/focus infrastructure; only
        // add them defensively when input exists (with DefaultPlugins and the
        // `bevy_ui_widgets` feature they are already present anyway).
        if app.is_plugin_added::<bevy::input::InputPlugin>() {
            if !app.is_plugin_added::<bevy::ui_widgets::CheckboxPlugin>() {
                app.add_plugins(bevy::ui_widgets::CheckboxPlugin);
            }
            if !app.is_plugin_added::<bevy::ui_widgets::SliderPlugin>() {
                app.add_plugins(bevy::ui_widgets::SliderPlugin);
            }
            if !app.is_plugin_added::<bevy::ui_widgets::EditableTextInputPlugin>() {
                app.add_plugins(bevy::ui_widgets::EditableTextInputPlugin);
            }
            if !app
                .is_plugin_added::<bevy::input_focus::tab_navigation::TabNavigationPlugin>()
            {
                app.add_plugins(bevy::input_focus::tab_navigation::TabNavigationPlugin);
            }
        }
        app.init_resource::<bevy::input_focus::InputFocus>();
        app.init_resource::<PfFocusRingColor>();
        app.init_resource::<crate::components::PfControlTheme>();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<PfFocusVisual>();
        app.add_systems(Update, (focus_visuals, keyboard_interaction));
        app.add_systems(Update, crate::triggers::evaluate_triggers);
        // Mouse-wheel / trackpad scrolling for every scrollable node
        // (ScrollViewer, ListBox, ComboBox dropdowns, ...).
        app.add_observer(scroll_on_wheel);
        // Items generation + popup layer.
        app.init_resource::<crate::overlay::PfActiveTooltip>()
            .add_systems(
                Update,
                (
                    crate::items::sync_content_sources,
                    crate::items::sync_items_sources,
                    combo_popup_sync,
                    crate::overlay::sync_popup_visibility,
                    crate::overlay::tooltip_system,
                )
                    .chain(),
            )
            .add_systems(
                PostUpdate,
                crate::overlay::position_popups.after(bevy::ui::UiSystems::Layout),
            );
        app.add_systems(
            Update,
            (
                button_interaction_visuals,
                toggle_button_checked_visuals,
                checked_visual_sync,
                slider_thumb_sync,
                scrollbar_thumb_sync,
                progress_fill_sync,
                progress_indeterminate_anim,
                resolve_popup_sources,
                listbox_selection_visuals,
                listbox_item_hover,
                expander_sync,
            ),
        );
        // After layout, so shapes see their final size (re-rasterizes only
        // when the pixel size changes). The explicit ordering matters:
        // ui_layout_system also runs in PostUpdate and writes ComputedNode.
        app.add_systems(
            PostUpdate,
            (crate::shapes::rasterize_shapes, viewbox_scale)
                .after(bevy::ui::UiSystems::Layout),
        );
        // Native bevy_ui styling, when compiled in: runs BEFORE the
        // rasterizer so whatever it claims never reaches it.
        #[cfg(feature = "native_shapes")]
        app.add_systems(
            PostUpdate,
            crate::shapes::style_native_shapes
                .after(bevy::ui::UiSystems::Layout)
                .before(crate::shapes::rasterize_shapes),
        );
        // GPU shape backend, when compiled in: claims what it can render and
        // leaves the rest to the CPU rasterizer above.
        #[cfg(feature = "vector_gpu")]
        app.add_plugins(crate::shapes_gpu::PfShapeGpuPlugin);
        // Data binding: write control state back to sources, then apply
        // source changes to targets. Runs after item generation so freshly
        // templated rows bind in the same frame (deterministically, not by
        // schedule luck).
        app.add_systems(
            Update,
            (
                crate::binding::textbox_write_back,
                crate::binding::checked_write_back,
                crate::binding::slider_write_back,
                // Element-source write-back runs BEFORE apply, or the
                // per-frame element re-apply echoes over the user's change.
                crate::binding::element_write_back,
                crate::binding::apply_bindings,
                // Selection write-back runs AFTER apply: a source change
                // retargets the selection first, so the stale-selection
                // echo self-cancels; a user click still writes back
                // (apply was a no-op that frame).
                crate::binding::selection_write_back,
            )
                .chain()
                .after(crate::items::sync_items_sources),
        );
    }
}

/// RepeatButton: while pressed, fire synthetic `Pointer<Click>` events after
/// `delay` ms, then every `interval` ms. The release still raises bevy's
/// natural Click, which covers the plain tap (WPF raises on press instead —
/// same count, shifted timing).
fn repeat_buttons(
    time: Res<Time<Virtual>>,
    mut buttons: Query<(
        Entity,
        &mut crate::components::PfRepeatButton,
        &Interaction,
    )>,
    windows: Query<Entity, With<bevy::window::Window>>,
    mut commands: Commands,
) {
    let now_ms = time.elapsed_secs() * 1000.0;
    for (entity, mut rb, interaction) in &mut buttons {
        if *interaction != Interaction::Pressed {
            if rb.pressed_at.is_some() {
                rb.pressed_at = None;
                rb.fired = 0;
            }
            continue;
        }
        let pressed_at = *rb.pressed_at.get_or_insert(now_ms);
        let held = now_ms - pressed_at;
        let due = if held < rb.delay {
            0
        } else {
            1 + ((held - rb.delay) / rb.interval.max(1.0)) as u32
        };
        if due <= rb.fired {
            continue;
        }
        // One fire per frame is plenty; don't burst-catch-up after a hitch.
        rb.fired = due;
        let Some(window) = windows.iter().next() else {
            continue;
        };
        let Some(target) = bevy::camera::RenderTarget::Window(bevy::window::WindowRef::Entity(
            window,
        ))
        .normalize(None) else {
            continue;
        };
        commands.queue(move |world: &mut World| {
            synthetic_click(world, target, entity);
        });
    }
}

/// Fire a synthetic `Pointer<Click>` at an entity — what RepeatButton
/// repeats and Space/Enter keyboard activation deliver.
pub(crate) fn synthetic_click(
    world: &mut World,
    target: bevy::camera::NormalizedRenderTarget,
    entity: Entity,
) {
    use bevy::picking::backend::HitData;
    use bevy::picking::events::{Click, Pointer};
    use bevy::picking::pointer::{Location, PointerButton, PointerId};
    world.trigger(Pointer::new(
        PointerId::Mouse,
        Location {
            target,
            position: Vec2::ZERO,
        },
        Click {
            button: PointerButton::Primary,
            hit: HitData::new(Entity::PLACEHOLDER, 0.0, None, None),
            duration: std::time::Duration::ZERO,
            count: 1,
        },
        entity,
    ));
}

/// The control currently wearing the focus border, with its original
/// border colors for restoration on blur.
#[derive(Resource, Default)]
struct PfFocusVisual {
    control: Option<Entity>,
    saved: Option<BorderColor>,
    /// The focus mark was an `Outline` (non-text controls), not a border.
    outlined: bool,
}

/// WPF focus feedback for default chrome: when keyboard focus moves (click
/// or Tab), the focused control's border takes the accent focus color and
/// the previous control's border is restored. Templated controls express
/// focus through FocusStates instead; controls without a border are
/// left untouched.
fn focus_visuals(
    focus: Res<bevy::input_focus::InputFocus>,
    ring: Res<PfFocusRingColor>,
    visibilities: Query<&Visibility>,
    mut state: ResMut<PfFocusVisual>,
    parents: Query<&ChildOf>,
    kinds: Query<&crate::components::PfElementKind>,
    templated: Query<&crate::components::PfTemplatedControl>,
    mut borders: Query<&mut BorderColor>,
    mut commands: Commands,
) {
    if !focus.is_changed() {
        return;
    }
    // The focused entity is usually the inner editable; the border lives on
    // the nearest control-root ancestor.
    const FOCUSABLE: &[&str] = &[
        "TextBox",
        "PasswordBox",
        "ComboBox",
        "AutoSuggestBox",
        "NumericUpDown",
        "DatePicker",
        "TimePicker",
        "ColorPicker",
    ];
    // Non-text tab stops get a focus OUTLINE (the WPF focus-rectangle
    // adorner analog) so their interaction borders stay untouched.
    const OUTLINED: &[&str] = &[
        "Button",
        "RepeatButton",
        "ToggleButton",
        "CheckBox",
        "RadioButton",
        "Slider",
        "ScrollBar",
        "ListBox",
        "ListView",
        "ToggleSwitch",
        "Hyperlink",
    ];
    let mut control = None;
    let mut outlined = false;
    if let Some(mut cursor) = focus.get() {
        loop {
            if let Ok(kind) = kinds.get(cursor) {
                if FOCUSABLE.contains(&kind.0.as_str()) {
                    control = Some(cursor);
                    break;
                }
                if OUTLINED.contains(&kind.0.as_str()) {
                    control = Some(cursor);
                    outlined = true;
                    break;
                }
            }
            match parents.get(cursor) {
                Ok(parent) => cursor = parent.parent(),
                Err(_) => break,
            }
        }
    }
    // A control that is not visibly rendered must not get a focus ring: a
    // hidden buffer (a screen's off-screen text field) would otherwise flash
    // an opaque border over whatever covers it. The AUTHORED `Visibility` is
    // checked, ancestors included, rather than `InheritedVisibility` — the
    // computed form is only filled in by the render-world propagation systems,
    // which headless apps (and tests) never run.
    if let Some(entity) = control {
        let mut cursor = entity;
        loop {
            if visibilities
                .get(cursor)
                .is_ok_and(|v| *v == Visibility::Hidden)
            {
                control = None;
                break;
            }
            match parents.get(cursor) {
                Ok(parent) => cursor = parent.parent(),
                Err(_) => break,
            }
        }
    }
    if state.control == control {
        return;
    }
    // Restore the previously-focused control.
    if let Some(prev) = state.control.take() {
        if state.outlined {
            if let Ok(mut e) = commands.get_entity(prev) {
                e.try_remove::<bevy::ui::Outline>();
            }
        } else if let Some(saved) = state.saved.take()
            && let Ok(mut border) = borders.get_mut(prev)
        {
            *border = saved;
        }
        state.saved = None;
    }
    // Mark the newly-focused control (unless a template owns its look).
    if let Some(next) = control
        && templated.get(next).is_err()
    {
        if outlined {
            if let Ok(mut e) = commands.get_entity(next) {
                e.insert(bevy::ui::Outline {
                    width: Val::Px(2.0),
                    offset: Val::Px(1.0),
                    // Same themable ring as the border variant below.
                    color: ring.0,
                });
                state.control = Some(next);
                state.outlined = true;
            }
        } else if let Ok(mut border) = borders.get_mut(next) {
            state.saved = Some(*border);
            state.control = Some(next);
            state.outlined = false;
            // Themable: apps whose palette is not WPF-blue override the
            // resource; the Aero2 TextBox.Focus.Border stays the default.
            *border = BorderColor::all(ring.0);
        }
    }
}

/// The colour `focus_visuals` paints on a focused control's border.
///
/// Defaults to WPF Aero2's `TextBox.Focus.Border` blue. An app with its own
/// palette overrides it once at startup:
///
/// ```ignore
/// app.insert_resource(PfFocusRingColor(Color::srgb_u8(0x2F, 0xE9, 0xFF)));
/// ```
#[derive(Resource, Clone, Copy, Debug)]
pub struct PfFocusRingColor(pub Color);

impl Default for PfFocusRingColor {
    fn default() -> Self {
        Self(Color::srgb_u8(0x56, 0x9D, 0xE5))
    }
}

/// WPF keyboard interaction for the focused control: Space/Enter activate
/// button-family controls, arrows adjust Slider/ScrollBar and move
/// ListBox/ComboBox selection.
fn keyboard_interaction(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Res<bevy::input_focus::InputFocus>,
    kinds: Query<&crate::components::PfElementKind>,
    windows: Query<Entity, With<bevy::window::Window>>,
    mut commands: Commands,
) {
    let Some(entity) = focus.get() else { return };
    let Ok(kind) = kinds.get(entity) else { return };
    let kind = kind.0.as_str();

    let pressed = |k: KeyCode| keys.just_pressed(k);
    match kind {
        "Button" | "RepeatButton" | "ToggleButton" | "CheckBox" | "RadioButton"
        | "ToggleSwitch" | "Hyperlink" => {
            if pressed(KeyCode::Space) || pressed(KeyCode::Enter) {
                let Some(window) = windows.iter().next() else {
                    return;
                };
                let Some(target) = bevy::camera::RenderTarget::Window(
                    bevy::window::WindowRef::Entity(window),
                )
                .normalize(None) else {
                    return;
                };
                commands.queue(move |world: &mut World| {
                    synthetic_click(world, target, entity);
                });
            }
        }
        "Slider" | "ScrollBar" => {
            let dir = if pressed(KeyCode::ArrowRight) || pressed(KeyCode::ArrowDown) {
                1.0f32
            } else if pressed(KeyCode::ArrowLeft) || pressed(KeyCode::ArrowUp) {
                -1.0
            } else {
                return;
            };
            let scroll_bar = kind == "ScrollBar";
            commands.queue(move |world: &mut World| {
                if scroll_bar {
                    crate::instantiate::scroll_bar_nudge(world, entity, dir);
                    return;
                }
                let (min, max) = match world.get::<SliderRange>(entity) {
                    Some(r) => (r.start(), r.end()),
                    None => (0.0, 100.0),
                };
                // WPF Slider arrow step: SmallChange, defaulting to 1% of
                // the range so any range feels responsive.
                let step = ((max - min) / 100.0).max(f32::EPSILON) * dir;
                if let Some(v) = world.get::<SliderValue>(entity).map(|v| v.0) {
                    let next = (v + step).clamp(min, max);
                    if next != v {
                        world.entity_mut(entity).insert(SliderValue(next));
                    }
                }
            });
        }
        "ListBox" | "ListView" => {
            let delta: i32 = if pressed(KeyCode::ArrowDown) {
                1
            } else if pressed(KeyCode::ArrowUp) {
                -1
            } else {
                return;
            };
            commands.queue(move |world: &mut World| {
                let container = crate::items::items_container(world, entity);
                let children: Vec<Entity> = world
                    .get::<Children>(container)
                    .map(|c| c.iter().collect())
                    .unwrap_or_default();
                if children.is_empty() {
                    return;
                }
                let Some(mut list) = world.get_mut::<PfListBox>(entity) else {
                    return;
                };
                let current = list
                    .selected
                    .and_then(|sel| children.iter().position(|&c| c == sel));
                let next = match current {
                    Some(i) => (i as i32 + delta).clamp(0, children.len() as i32 - 1) as usize,
                    None => 0,
                };
                if list.selected != Some(children[next]) {
                    list.selected = Some(children[next]);
                }
            });
        }
        "ComboBox" => {
            let delta: i32 = if pressed(KeyCode::ArrowDown) {
                1
            } else if pressed(KeyCode::ArrowUp) {
                -1
            } else if pressed(KeyCode::Escape) {
                commands.queue(move |world: &mut World| {
                    if let Some(mut combo) = world.get_mut::<PfComboBox>(entity) {
                        combo.open = false;
                    }
                });
                return;
            } else {
                return;
            };
            commands.queue(move |world: &mut World| {
                let Some(combo) = world.get::<PfComboBox>(entity) else {
                    return;
                };
                let count = world
                    .get::<Children>(combo.popup)
                    .map(|c| c.iter().count())
                    .unwrap_or(0);
                if count == 0 {
                    return;
                }
                let next = match combo.selected {
                    Some(i) => (i as i32 + delta).clamp(0, count as i32 - 1) as usize,
                    None => 0,
                };
                if combo.selected != Some(next) {
                    crate::instantiate::select_combo_index(world, entity, next);
                }
            });
        }
        _ => {}
    }
}

/// Wheel/trackpad scrolling: walk from the hovered element up to the
/// nearest scrollable ancestor and move its `ScrollPosition` (layout
/// clamps to the content extent). Line units are converted at a WPF-ish
/// 20px per line; pixel units (trackpads, wasm) pass through.
fn scroll_on_wheel(
    ev: On<Pointer<bevy::picking::events::Scroll>>,
    parents: Query<&ChildOf>,
    nodes: Query<&Node>,
    mut scrollables: Query<&mut bevy::ui::ScrollPosition>,
) {
    // The event propagates up the hierarchy and a global observer fires at
    // every hop; handle only the original target (the walk covers ancestors).
    if ev.entity != ev.original_event_target() {
        return;
    }
    const LINE_HEIGHT: f32 = 20.0;
    let (dx, dy) = match ev.unit {
        bevy::input::mouse::MouseScrollUnit::Line => {
            (ev.x * LINE_HEIGHT, ev.y * LINE_HEIGHT)
        }
        bevy::input::mouse::MouseScrollUnit::Pixel => (ev.x, ev.y),
    };
    let mut cursor = ev.entity;
    loop {
        let scrolls = nodes.get(cursor).is_ok_and(|n| {
            n.overflow.x == bevy::ui::OverflowAxis::Scroll
                || n.overflow.y == bevy::ui::OverflowAxis::Scroll
        });
        if scrolls && let Ok(mut pos) = scrollables.get_mut(cursor) {
            pos.0.y -= dy;
            pos.0.x -= dx;
            return;
        }
        match parents.get(cursor) {
            Ok(parent) => cursor = parent.parent(),
            Err(_) => return,
        }
    }
}

/// Size and place the ScrollBar thumb: proportional to ViewportSize, at
/// the value's fraction of the track (WPF Track arrangement math).
fn scrollbar_thumb_sync(
    bars: Query<(
        &crate::components::PfScrollBar,
        &SliderValue,
        &SliderRange,
    )>,
    mut nodes: Query<&mut Node>,
) {
    for (sb, value, range) in &bars {
        let span = (range.end() - range.start()).max(f32::EPSILON);
        let frac_len = (sb.viewport / (span + sb.viewport)).clamp(0.05, 1.0);
        let frac_pos = ((value.0 - range.start()) / span).clamp(0.0, 1.0) * (1.0 - frac_len);
        let Ok(mut n) = nodes.get_mut(sb.thumb) else {
            continue;
        };
        if sb.horizontal {
            n.width = Val::Percent(frac_len * 100.0);
            n.height = Val::Percent(100.0);
            n.left = Val::Percent(frac_pos * 100.0);
            n.top = Val::Px(0.0);
        } else {
            n.height = Val::Percent(frac_len * 100.0);
            n.width = Val::Percent(100.0);
            n.top = Val::Percent(frac_pos * 100.0);
            n.left = Val::Px(0.0);
        }
    }
}

/// Keep toolkit-control visuals in sync with their state components.
#[allow(clippy::too_many_arguments)] // one query set per toolkit control family
fn toolkit_control_sync(
    watermarks: Query<(Entity, &crate::components::PfWatermark)>,
    editables: Query<&bevy::text::EditableText>,
    children_q: Query<&Children>,
    switches: Query<(Entity, &crate::components::PfToggleSwitch)>,
    checked: Query<Has<bevy::ui::Checked>>,
    numerics: Query<&crate::components::PfNumericUpDown, Changed<crate::components::PfNumericUpDown>>,
    ratings: Query<&crate::components::PfRatingBar, Changed<crate::components::PfRatingBar>>,
    busys: Query<&crate::components::PfBusyIndicator, Changed<crate::components::PfBusyIndicator>>,
    ranges: Query<&crate::components::PfRangeSlider, Changed<crate::components::PfRangeSlider>>,
    mut nodes: Query<&mut Node>,
    mut colors: Query<&mut BackgroundColor>,
    mut texts: Query<&mut bevy::ui::widget::Text>,
    control_theme: Res<crate::components::PfControlTheme>,
) {
    for (entity, watermark) in &watermarks {
        // Hide the placeholder as soon as the editable child has content.
        let has_text = children_q.get(entity).ok().and_then(|kids| {
            kids.iter()
                .find_map(|k| editables.get(k).ok())
                .map(|e| !e.editor().text().to_string().is_empty())
        });
        // The overlay mirrors the control's padding so the placeholder sits
        // exactly where typed text will: its absolute containing block is the
        // control's padding box, which does NOT include the padding itself.
        // Copied per frame rather than at build because style-driven Padding
        // arrives through the property store after instantiation.
        let padding = nodes.get(entity).map(|n| n.padding);
        if let Ok(mut n) = nodes.get_mut(watermark.overlay) {
            if let Ok(padding) = padding
                && n.padding != padding
            {
                n.padding = padding;
            }
            if let Some(has_text) = has_text {
                let target = if has_text { Display::None } else { Display::Flex };
                if n.display != target {
                    n.display = target;
                }
            }
        }
    }
    for (entity, switch) in &switches {
        let on = checked.get(entity).unwrap_or(false);
        if let Ok(mut n) = nodes.get_mut(switch.thumb) {
            let target = Val::Px(if on { 22.0 } else { 2.0 });
            if n.left != target {
                n.left = target;
            }
        }
        if let Ok(mut c) = colors.get_mut(switch.track) {
            let target = if on { control_theme.accent } else { control_theme.track };
            if c.0 != target {
                c.0 = target;
            }
        }
    }
    for numeric in &numerics {
        if let Ok(mut t) = texts.get_mut(numeric.text) {
            t.0 = format!("{}", numeric.value);
        }
    }
    for rating in &ratings {
        for (i, pip) in rating.pips.iter().enumerate() {
            if let Ok(mut c) = colors.get_mut(*pip) {
                c.0 = if (i as u32) < rating.value {
                    Color::srgb_u8(0xF2, 0xB0, 0x24)
                } else {
                    Color::srgb_u8(0xD6, 0xD6, 0xD6)
                };
            }
        }
    }
    for rs in &ranges {
        let span = (rs.maximum - rs.minimum).max(f32::EPSILON);
        let lo = (rs.lower - rs.minimum) / span;
        let hi = (rs.upper - rs.minimum) / span;
        if let Ok(mut n) = nodes.get_mut(rs.thumb_lower) {
            n.left = Val::Percent(lo * 100.0);
        }
        if let Ok(mut n) = nodes.get_mut(rs.thumb_upper) {
            n.left = Val::Percent(hi * 100.0);
        }
        if let Ok(mut n) = nodes.get_mut(rs.fill) {
            n.left = Val::Percent(lo * 100.0);
            n.right = Val::Percent((1.0 - hi) * 100.0);
        }
    }
    for busy in &busys {
        if let Ok(mut n) = nodes.get_mut(busy.overlay) {
            n.display = if busy.busy { Display::Flex } else { Display::None };
        }
    }
}

/// Button chrome query: interaction + palette + the colors it drives.
type ButtonChrome<'w, 's> = Query<
    'w,
    's,
    (
        &'static Interaction,
        &'static ButtonVisual,
        Has<Checked>,
        &'static mut BackgroundColor,
        &'static mut BorderColor,
    ),
    (Changed<Interaction>, Without<crate::triggers::PfTriggers>),
>;

/// Slider parts whose value or range changed this frame.
type ChangedSliders<'w, 's> = Query<
    'w,
    's,
    (&'static SliderValue, &'static SliderRange, &'static PfSliderVisual),
    Or<(Changed<SliderValue>, Changed<SliderRange>)>,
>;

/// List items whose hover state changed this frame.
type HoveredListItems<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Interaction, &'static ChildOf, &'static mut BackgroundColor),
    (With<PfListBoxItem>, Changed<Interaction>),
>;

/// Swap button chrome colors on interaction changes (hover/pressed states).
/// Checked toggle buttons stay in the pressed color while at rest.
fn button_interaction_visuals(mut buttons: ButtonChrome) {
    for (interaction, visual, checked, mut bg, mut border) in &mut buttons {
        let (bg_color, border_color) = match (interaction, checked) {
            (Interaction::Pressed, _) | (Interaction::None, true) => {
                (visual.pressed_bg, visual.pressed_border)
            }
            (Interaction::Hovered, _) => (visual.hover_bg, visual.hover_border),
            (Interaction::None, false) => (visual.normal_bg, visual.normal_border),
        };
        *bg = BackgroundColor(bg_color);
        *border = BorderColor::all(border_color);
    }
}

/// Latch ToggleButton visuals when `Checked` is added/removed.
fn toggle_button_checked_visuals(
    added: Query<Entity, (With<PfToggleButton>, Added<Checked>)>,
    mut removed: RemovedComponents<Checked>,
    mut buttons: Query<
        (&ButtonVisual, &mut BackgroundColor, &mut BorderColor),
        With<PfToggleButton>,
    >,
) {
    for entity in &added {
        if let Ok((visual, mut bg, mut border)) = buttons.get_mut(entity) {
            *bg = BackgroundColor(visual.pressed_bg);
            *border = BorderColor::all(visual.pressed_border);
        }
    }
    for entity in removed.read() {
        if let Ok((visual, mut bg, mut border)) = buttons.get_mut(entity) {
            *bg = BackgroundColor(visual.normal_bg);
            *border = BorderColor::all(visual.normal_border);
        }
    }
}

/// Reflect `Checked` state onto CheckBox / RadioButton glyph + box visuals.
fn checked_visual_sync(
    added: Query<&PfCheckVisual, Added<Checked>>,
    mut removed: RemovedComponents<Checked>,
    visuals: Query<&PfCheckVisual>,
    mut vis_q: Query<&mut Visibility>,
    mut bg_q: Query<&mut BackgroundColor>,
    control_theme: Res<crate::components::PfControlTheme>,
) {
    let mut apply = |visual: &PfCheckVisual, checked: bool| {
        if let Ok(mut vis) = vis_q.get_mut(visual.glyph) {
            *vis = if checked {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
        if visual.accent_fills_box
            && let Ok(mut bg) = bg_q.get_mut(visual.box_node) {
                bg.0 = if checked { control_theme.accent } else { control_theme.control_face };
            }
    };
    for visual in &added {
        apply(visual, true);
    }
    for entity in removed.read() {
        if let Ok(visual) = visuals.get(entity) {
            apply(visual, false);
        }
    }
}

/// Keep the slider thumb positioned according to its value.
fn slider_thumb_sync(sliders: ChangedSliders, mut nodes: Query<&mut Node>) {
    for (value, range, visual) in &sliders {
        if let Ok(mut node) = nodes.get_mut(visual.thumb) {
            node.left = Val::Percent(range.thumb_position(value.0) * 100.0);
        }
    }
}

/// Keep the progress fill width in sync with the value.
fn progress_fill_sync(
    bars: Query<(&PfProgress, &PfProgressVisual), Changed<PfProgress>>,
    mut nodes: Query<&mut Node>,
) {
    for (progress, visual) in &bars {
        if let Ok(mut node) = nodes.get_mut(visual.fill) {
            node.width = Val::Percent(progress.fraction() * 100.0);
            // Clear any horizontal offset the indeterminate sweep left behind, so
            // a bar that switches from indeterminate back to determinate fills
            // from the track origin instead of the last swept position.
            node.margin.left = Val::Px(0.0);
        }
    }
}

/// Animate indeterminate progress bars: a 30% band sweeping back and forth.
fn progress_indeterminate_anim(
    time: Res<Time>,
    bars: Query<(&PfProgress, &PfProgressVisual)>,
    mut nodes: Query<&mut Node>,
) {
    for (progress, visual) in &bars {
        if !progress.indeterminate {
            continue;
        }
        if let Ok(mut node) = nodes.get_mut(visual.fill) {
            let t = (time.elapsed_secs() * 0.9).fract();
            let pos = if t < 0.5 { t * 2.0 } else { (1.0 - t) * 2.0 };
            node.width = Val::Percent(30.0);
            node.margin.left = Val::Percent(pos * 70.0);
        }
    }
}

/// A `<Popup>` element anchors to its XAML parent; the parent link only
/// exists after the tree is assembled, so resolve it here (once each).
fn resolve_popup_sources(
    sources: Query<(Entity, &crate::components::PfPopupSource, &ChildOf)>,
    mut popups: Query<&mut crate::overlay::PfPopup>,
) {
    for (placeholder, source, child_of) in &sources {
        if let Ok(mut popup) = popups.get_mut(source.popup)
            && popup.anchor == placeholder
        {
            popup.anchor = child_of.parent();
        }
    }
}

/// Repaint list items when the selection changes.
fn listbox_selection_visuals(
    lists: Query<
        (Entity, &PfListBox, Option<&crate::components::PfItemsPanel>),
        Changed<PfListBox>,
    >,
    children_q: Query<&Children>,
    mut items: Query<&mut BackgroundColor, With<PfListBoxItem>>,
    control_theme: Res<crate::components::PfControlTheme>,
) {
    for (entity, list, panel) in &lists {
        let container = panel.map(|p| p.panel).unwrap_or(entity);
        let Ok(children) = children_q.get(container) else {
            continue;
        };
        for child in children.iter() {
            if let Ok(mut bg) = items.get_mut(child) {
                bg.0 = if Some(child) == list.selected {
                    control_theme.selection_fill
                } else {
                    Color::NONE
                };
            }
        }
    }
}

/// Expand/collapse Expander content when its `Checked` state flips.
fn expander_sync(
    added: Query<&PfExpander, Added<Checked>>,
    mut removed: RemovedComponents<Checked>,
    expanders: Query<&PfExpander>,
    mut nodes: Query<&mut Node>,
    mut texts: Query<&mut bevy::ui::widget::Text>,
) {
    let mut apply = |exp: &PfExpander, expanded: bool| {
        if let Ok(mut node) = nodes.get_mut(exp.content) {
            node.display = if expanded { Display::Flex } else { Display::None };
        }
        if let Ok(mut text) = texts.get_mut(exp.arrow) {
            text.0 = if expanded { "−" } else { "+" }.to_string();
        }
    };
    for exp in &added {
        apply(exp, true);
    }
    for entity in removed.read() {
        if let Ok(exp) = expanders.get(entity) {
            apply(exp, false);
        }
    }
}

/// Scale a Viewbox's child to fit, per its Stretch mode. Runs after layout;
/// the scale is visual-only (`UiTransform`), an accepted deviation from
/// WPF's measure-participating scale.
fn viewbox_scale(
    viewboxes: Query<(&PfViewbox, &bevy::ui::ComputedNode, &Children)>,
    children_nodes: Query<&bevy::ui::ComputedNode, Without<PfViewbox>>,
    mut transforms: Query<&mut bevy::ui::UiTransform>,
    mut commands: Commands,
) {
    use bevy_pf_xaml::value::Stretch;
    for (viewbox, computed, children) in &viewboxes {
        let Some(&child) = children.first() else {
            continue;
        };
        let Ok(child_computed) = children_nodes.get(child) else {
            continue;
        };
        let outer = computed.size();
        let inner = child_computed.size();
        if inner.x <= 0.0 || inner.y <= 0.0 || outer.x <= 0.0 || outer.y <= 0.0 {
            continue;
        }
        let (sx, sy) = match viewbox.stretch {
            Stretch::None => (1.0, 1.0),
            Stretch::Fill => (outer.x / inner.x, outer.y / inner.y),
            Stretch::Uniform => {
                let s = (outer.x / inner.x).min(outer.y / inner.y);
                (s, s)
            }
            Stretch::UniformToFill => {
                let s = (outer.x / inner.x).max(outer.y / inner.y);
                (s, s)
            }
        };
        let scale = Vec2::new(sx, sy);
        if let Ok(mut transform) = transforms.get_mut(child) {
            if (transform.scale - scale).length_squared() > 1e-6 {
                transform.scale = scale;
            }
        } else {
            commands.entity(child).insert(bevy::ui::UiTransform {
                scale,
                ..Default::default()
            });
        }
    }
}

/// Mirror `PfComboBox::open` onto the dropdown's `PfPopup`.
fn combo_popup_sync(
    combos: Query<&PfComboBox, Changed<PfComboBox>>,
    mut popups: Query<&mut crate::overlay::PfPopup>,
) {
    for combo in &combos {
        if let Ok(mut popup) = popups.get_mut(combo.popup)
            && popup.open != combo.open {
                popup.open = combo.open;
            }
    }
}

/// Hover highlight for unselected list items.
fn listbox_item_hover(
    mut items: HoveredListItems,
    lists: Query<&PfListBox>,
    panels: Query<&crate::components::PfGeneratedItemsHost>,
    control_theme: Res<crate::components::PfControlTheme>,
) {
    for (entity, interaction, parent, mut bg) in &mut items {
        // The item's parent is the list itself, or its ItemsPanel.
        let list = panels
            .get(parent.parent())
            .map(|host| host.owner)
            .unwrap_or(parent.parent());
        let selected = lists
            .get(list)
            .ok()
            .and_then(|l| l.selected)
            .is_some_and(|s| s == entity);
        if selected {
            continue;
        }
        bg.0 = match interaction {
            Interaction::Hovered | Interaction::Pressed => control_theme.hover_fill,
            Interaction::None => Color::NONE,
        };
    }
}

/// Extension methods for spawning XAML scenes.
pub trait PfCommandsExt {
    /// Spawn a XAML scene, returning the root entity immediately. The tree is
    /// instantiated when commands are applied. Instantiation problems are
    /// logged as warnings; the returned entity always exists.
    fn spawn_xaml(&mut self, scene: XamlScene) -> Entity;

    /// Spawn a XAML scene with a data context for its `{Binding}`s.
    fn spawn_xaml_bound(
        &mut self,
        scene: XamlScene,
        context: crate::binding::Bindable,
    ) -> Entity;
}

impl PfCommandsExt for Commands<'_, '_> {
    fn spawn_xaml(&mut self, scene: XamlScene) -> Entity {
        let root = self.spawn_empty().id();
        self.queue(move |world: &mut World| {
            let doc = scene.document();
            match instantiate_document(world, root, &doc) {
                Ok(result) => {
                    for w in &result.warnings {
                        warn!("bevy_pf: {w}");
                    }
                }
                Err(e) => {
                    error!("bevy_pf: failed to instantiate XAML: {e}");
                }
            }
        });
        root
    }

    fn spawn_xaml_bound(
        &mut self,
        scene: XamlScene,
        context: crate::binding::Bindable,
    ) -> Entity {
        let root = self.spawn_xaml(scene);
        self.entity(root)
            .insert(crate::binding::DataContext(context));
        root
    }
}

/// Refilter AutoSuggestBox dropdowns when their text changes.
fn auto_suggest_watch(
    changed: Query<
        (&crate::components::PfAutoSuggestInput, Ref<bevy::text::EditableText>),
        Changed<bevy::text::EditableText>,
    >,
    mut commands: Commands,
) {
    for (marker, editable) in &changed {
        if editable.is_added() {
            continue; // initial spawn, not typing
        }
        let owner = marker.owner;
        commands.queue(move |world: &mut World| {
            crate::instantiate::rebuild_suggestions(world, owner);
        });
    }
}

/// Apply typed `#RRGGBB` hex values to the owning ColorPicker.
fn color_hex_watch(
    changed: Query<
        (&crate::components::PfColorHexInput, Ref<bevy::text::EditableText>),
        Changed<bevy::text::EditableText>,
    >,
    mut commands: Commands,
) {
    for (marker, editable) in &changed {
        if editable.is_added() {
            continue;
        }
        let text = editable.editor().text().to_string();
        let Ok(color) = text.trim().parse::<bevy_pf_xaml::value::PfColor>() else {
            continue; // incomplete/invalid hex while typing
        };
        let owner = marker.owner;
        let color = crate::convert::color(color);
        commands.queue(move |world: &mut World| {
            crate::instantiate::color_picker_set(world, owner, color, false);
        });
    }
}

/// Diff PasswordBox edits back into the real password and re-mask the
/// display. All-mask prefixes/suffixes are kept characters; the unmasked
/// middle is the insertion (the classic masked-input diff).
fn password_watch(
    changed: Query<
        (&crate::components::PfPasswordInput, Ref<bevy::text::EditableText>),
        Changed<bevy::text::EditableText>,
    >,
    mut commands: Commands,
) {
    for (marker, editable) in &changed {
        if editable.is_added() {
            continue;
        }
        let shown = editable.editor().text().to_string();
        let owner = marker.owner;
        commands.queue(move |world: &mut World| {
            let Some(pb) = world.get::<crate::components::PfPasswordBox>(owner).cloned()
            else {
                return;
            };
            let mask = pb.mask;
            let shown_chars: Vec<char> = shown.chars().collect();
            let old_chars: Vec<char> = pb.password.chars().collect();

            let mut prefix = 0;
            while prefix < shown_chars.len()
                && prefix < old_chars.len()
                && shown_chars[prefix] == mask
            {
                prefix += 1;
            }
            let mut suffix = 0;
            while suffix < shown_chars.len() - prefix
                && suffix < old_chars.len() - prefix
                && shown_chars[shown_chars.len() - 1 - suffix] == mask
            {
                suffix += 1;
            }
            let middle: String = shown_chars[prefix..shown_chars.len() - suffix]
                .iter()
                .collect();
            let new_password: String = old_chars[..prefix]
                .iter()
                .chain(middle.chars().collect::<Vec<_>>().iter())
                .chain(old_chars[old_chars.len() - suffix..].iter())
                .collect();

            let masked: String =
                std::iter::repeat_n(mask, new_password.chars().count()).collect();
            if let Some(mut pb) = world.get_mut::<crate::components::PfPasswordBox>(owner) {
                pb.password = new_password;
            }
            if shown != masked
                && let Some(mut et) = world.get_mut::<bevy::text::EditableText>(pb.input)
            {
                et.editor.set_text(&masked);
            }
        });
    }
}
