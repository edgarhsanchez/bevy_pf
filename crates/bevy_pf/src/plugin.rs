//! The Bevy plugin and the `Commands` extension for spawning XAML.

use bevy::prelude::*;
use bevy::ui::{BorderColor, Checked};
use bevy::ui_widgets::{SliderRange, SliderValue};

use crate::XamlScene;
use crate::components::*;
use crate::instantiate::instantiate_document;

/// Selected list item background (Windows 10 style).
pub(crate) const LIST_SELECTED_BG: Color = Color::srgb(0.796, 0.909, 0.964); // #CBE8F6
const LIST_HOVER_BG: Color = Color::srgb(0.898, 0.953, 1.0); // #E5F3FF

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
                crate::toast::expire_toasts,
            ),
        );
        app.init_resource::<crate::animation::PfRunningAnimations>();
        if !app.world().contains_resource::<crate::binding::PfConverters>() {
            app.insert_resource(crate::binding::builtin_converters());
        }
        app.add_systems(
            Update,
            (
                crate::animation::start_pending_storyboards,
                crate::behaviors::run_pending_actions,
                crate::animation::drive_visual_states,
                crate::animation::tick_animations,
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
        }
        app.add_systems(Update, crate::triggers::evaluate_triggers);
        // Items generation + popup layer.
        app.init_resource::<crate::overlay::PfActiveTooltip>()
            .add_systems(
                Update,
                (
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
                crate::binding::apply_bindings,
            )
                .chain()
                .after(crate::items::sync_items_sources),
        );
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
) {
    for (entity, watermark) in &watermarks {
        // Hide the placeholder as soon as the editable child has content.
        let has_text = children_q.get(entity).ok().and_then(|kids| {
            kids.iter()
                .find_map(|k| editables.get(k).ok())
                .map(|e| !e.editor().text().to_string().is_empty())
        });
        if let Some(has_text) = has_text
            && let Ok(mut n) = nodes.get_mut(watermark.overlay)
        {
            let target = if has_text { Display::None } else { Display::Flex };
            if n.display != target {
                n.display = target;
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
            let target = if on {
                crate::components::ACCENT
            } else {
                Color::srgb_u8(0xB6, 0xB6, 0xB6)
            };
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
                bg.0 = if checked { ACCENT } else { Color::WHITE };
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
    lists: Query<(&PfListBox, &Children), Changed<PfListBox>>,
    mut items: Query<&mut BackgroundColor, With<PfListBoxItem>>,
) {
    for (list, children) in &lists {
        for child in children.iter() {
            if let Ok(mut bg) = items.get_mut(child) {
                bg.0 = if Some(child) == list.selected {
                    LIST_SELECTED_BG
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
fn listbox_item_hover(mut items: HoveredListItems, lists: Query<&PfListBox>) {
    for (entity, interaction, parent, mut bg) in &mut items {
        let selected = lists
            .get(parent.parent())
            .ok()
            .and_then(|l| l.selected)
            .is_some_and(|s| s == entity);
        if selected {
            continue;
        }
        bg.0 = match interaction {
            Interaction::Hovered | Interaction::Pressed => LIST_HOVER_BG,
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
