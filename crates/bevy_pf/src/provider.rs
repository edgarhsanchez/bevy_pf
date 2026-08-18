//! The per-entity value-provider store: WPF's dependency-property precedence
//! model, sized for the properties bevy_pf manages dynamically.
//!
//! Sources use WPF's `BaseValueSourceInternal` order **verbatim**
//! (`EffectiveValueEntry.cs:613-641`; see `docs/wpf-conformance-notes.md`
//! §4.1). A property's *effective* value is the entry with the highest
//! source; writers (style setters, triggers, dynamic resources, local
//! attributes) each write at their own tier and never clobber a higher one —
//! reverting a trigger structurally restores whatever the next tier holds.
//!
//! Animation is deliberately reserved as a *modifier* above any base value
//! (WPF stores it as a flag bit, not a slot); when Storyboards land they
//! layer on top of this store without changing it.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::ui::BorderColor;
use bevy_pf_xaml::value as v;

use crate::convert;
use crate::resources::PfValue;

/// WPF `BaseValueSourceInternal`, verbatim order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ValueSource {
    Default = 1,
    Inherited = 2,
    ThemeStyle = 3,
    ThemeStyleTrigger = 4,
    Style = 5,
    TemplateTrigger = 6,
    StyleTrigger = 7,
    ImplicitReference = 8,
    ParentTemplate = 9,
    ParentTemplateTrigger = 10,
    Local = 11,
    /// MAUI `<VisualState.Setters>`: ABOVE a local value, unlike every
    /// other trigger tier.
    ///
    /// The whole point of a visual state is to restyle a control that has
    /// already been styled — a Pressed state exists to override the
    /// Background the button was authored with. At the ordinary trigger
    /// tiers it loses to that inline value, which is not a subtle
    /// mis-ordering: it makes the feature do nothing at all in the case
    /// everyone writes first. MAUI puts visual states above local values
    /// and so does this.
    ///
    /// Still below `Animation`, so a running storyboard wins — a state
    /// that carries BOTH setters and a storyboard therefore resolves the
    /// way WPF would, with the animation on top.
    VisualState = 12,
    /// The animation layer: composes ABOVE every base tier (WPF: an active
    /// animation beats even a local value; clearing it reverts structurally).
    Animation = 13,
}

/// The properties the store manages (the dynamically-writable set).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropertyTarget {
    Background,
    BorderBrush,
    BorderThickness,
    Foreground,
    /// Shape fill brush (Path/Ellipse/Rectangle/...): writes repaint the
    /// rasterized image at the current size.
    Fill,
    /// Shape stroke brush. The sibling of [`PropertyTarget::Fill`] — without
    /// it a template could bind a shape's fill to the templated parent but
    /// not its outline, so `Stroke="{TemplateBinding BorderBrush}"` silently
    /// dropped the outline and the shape rendered unstroked.
    Stroke,
    FontSize,
    Margin,
    Padding,
    CornerRadius,
    Width,
    Height,
    Visibility,
    /// WPF `UIElement.Opacity`, approximated subtree-wide (see [`PfOpacity`]).
    Opacity,
}

/// Map a XAML property name to a store-managed target.
pub fn property_target_for(property: &str) -> Option<PropertyTarget> {
    Some(match property {
        "Background" => PropertyTarget::Background,
        "BorderBrush" => PropertyTarget::BorderBrush,
        "BorderThickness" => PropertyTarget::BorderThickness,
        "Foreground" => PropertyTarget::Foreground,
        "Fill" => PropertyTarget::Fill,
        "Stroke" => PropertyTarget::Stroke,
        "FontSize" => PropertyTarget::FontSize,
        "Margin" => PropertyTarget::Margin,
        "Padding" => PropertyTarget::Padding,
        "CornerRadius" => PropertyTarget::CornerRadius,
        "Width" => PropertyTarget::Width,
        "Height" => PropertyTarget::Height,
        "Opacity" => PropertyTarget::Opacity,
        "Visibility" => PropertyTarget::Visibility,
        _ => return None,
    })
}

/// A stored value: `None` is an explicit `{x:Null}` (masks lower tiers and
/// applies as "cleared").
pub type StoredValue = Option<PfValue>;

/// Per-entity property store: one value per (property, source tier).
#[derive(Component, Debug, Default, Clone)]
pub struct PfPropertyStore {
    entries: HashMap<PropertyTarget, Vec<(ValueSource, StoredValue)>>,
}

impl PfPropertyStore {
    /// Set the value a tier provides for a property (replacing that tier's
    /// previous entry, if any).
    pub fn set(&mut self, target: PropertyTarget, source: ValueSource, value: StoredValue) {
        let slot = self.entries.entry(target).or_default();
        slot.retain(|(s, _)| *s != source);
        slot.push((source, value));
    }

    /// Remove a tier's entry for a property.
    pub fn clear(&mut self, target: PropertyTarget, source: ValueSource) {
        if let Some(slot) = self.entries.get_mut(&target) {
            slot.retain(|(s, _)| *s != source);
        }
    }

    /// Remove a tier's entries across all properties, returning the affected
    /// targets (trigger deactivation).
    pub fn clear_tier(&mut self, source: ValueSource) -> Vec<PropertyTarget> {
        let mut affected = Vec::new();
        for (target, slot) in self.entries.iter_mut() {
            let before = slot.len();
            slot.retain(|(s, _)| *s != source);
            if slot.len() != before {
                affected.push(*target);
            }
        }
        affected
    }

    /// The effective value: highest tier wins.
    pub fn effective(&self, target: PropertyTarget) -> Option<&(ValueSource, StoredValue)> {
        self.entries.get(&target)?.iter().max_by_key(|(s, _)| *s)
    }

    /// The tier of the effective value, if any.
    pub fn effective_source(&self, target: PropertyTarget) -> Option<ValueSource> {
        self.effective(target).map(|(s, _)| *s)
    }

    /// The effective entry considering only tiers strictly below `ceiling`
    /// (the animation layer snapshots its From value from the base).
    pub fn effective_below(
        &self,
        target: PropertyTarget,
        ceiling: ValueSource,
    ) -> Option<&(ValueSource, StoredValue)> {
        self.entries
            .get(&target)?
            .iter()
            .filter(|(s, _)| *s < ceiling)
            .max_by_key(|(s, _)| *s)
    }
}

/// Set a property from application code at the `Local` tier — the same
/// precedence a literal XAML attribute has. Styles, triggers, and dynamic
/// resources keep their values in lower tiers, so they revert correctly if
/// the local value is later cleared. Call from an exclusive system or via
/// `commands.queue(move |world| ...)`.
pub fn set_local(world: &mut World, entity: Entity, target: PropertyTarget, value: PfValue) {
    store_and_apply(world, entity, target, ValueSource::Local, Some(value));
}

/// Write into an entity's store at a tier and immediately apply the new
/// effective value to components.
pub fn store_and_apply(
    world: &mut World,
    entity: Entity,
    target: PropertyTarget,
    source: ValueSource,
    value: StoredValue,
) {
    {
        // Cross-entity forwarding (TemplateBinding dependents) can race a
        // despawn — degrade, never panic.
        let Ok(mut e) = world.get_entity_mut(entity) else {
            return;
        };
        if let Some(mut store) = e.get_mut::<PfPropertyStore>() {
            store.set(target, source, value);
        } else {
            let mut store = PfPropertyStore::default();
            store.set(target, source, value);
            e.insert(store);
        }
    }
    apply_effective(world, entity, target);
}

/// Apply a property's current effective value to the entity's components
/// (or its "unset" state when nothing provides a value / `{x:Null}` wins).
pub fn apply_effective(world: &mut World, entity: Entity, target: PropertyTarget) {
    let value = world
        .get::<PfPropertyStore>(entity)
        .and_then(|s| s.effective(target).cloned());
    match value {
        Some((_, Some(v))) => {
            apply_value(world, entity, target, &v);
            after_apply_value(world, entity, target);
        }
        Some((_, None)) | None => apply_unset(world, entity, target),
    }
    // TemplateBinding: forward the (possibly suppressed-on-root) effective
    // value into template children reading this property. Runs regardless
    // of paint suppression — the store value is authoritative.
    forward_template_dependents(world, entity, target);
}

/// Push a templated parent's effective `target` value to every template
/// child bound to it via `{TemplateBinding}`. Stale (despawned) children
/// are pruned; the parent==child degenerate case is skipped.
fn forward_template_dependents(world: &mut World, entity: Entity, target: PropertyTarget) {
    let Some(deps) = world.get::<PfTemplateBindingDependents>(entity) else {
        return;
    };
    let matching: Vec<(Entity, PropertyTarget)> = deps
        .0
        .iter()
        .filter(|(src, child, _)| *src == target && *child != entity)
        .map(|(_, child, dst)| (*child, *dst))
        .collect();
    if matching.is_empty() {
        return;
    }
    let value = world
        .get::<PfPropertyStore>(entity)
        .and_then(|s| s.effective(target))
        .map(|(_, v)| v.clone());
    let mut stale: Vec<Entity> = Vec::new();
    for (child, dst) in matching {
        if world.get_entity(child).is_err() {
            stale.push(child);
            continue;
        }
        match &value {
            Some(stored) => {
                store_and_apply(
                    world,
                    child,
                    dst,
                    ValueSource::ParentTemplate,
                    stored.clone(),
                );
            }
            None => {
                if let Some(mut store) = world.get_mut::<PfPropertyStore>(child) {
                    store.clear(dst, ValueSource::ParentTemplate);
                }
                apply_effective(world, child, dst);
            }
        }
    }
    if !stale.is_empty()
        && let Some(mut deps) = world.get_mut::<PfTemplateBindingDependents>(entity)
    {
        deps.0.retain(|(_, child, _)| !stale.contains(child));
    }
}

/// The entity itself if it has `TextColor`, otherwise every descendant that
/// does (content controls hold references; generated text children hold the
/// text components).
pub(crate) fn collect_text_entities(world: &World, root: Entity) -> Vec<Entity> {
    if world.get::<bevy::text::TextColor>(root).is_some() {
        return vec![root];
    }
    let mut out = Vec::new();
    let mut stack: Vec<Entity> = world
        .get::<Children>(root)
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    while let Some(e) = stack.pop() {
        if world.get::<bevy::text::TextColor>(e).is_some() {
            out.push(e);
        }
        if let Some(children) = world.get::<Children>(e) {
            stack.extend(children.iter());
        }
    }
    out
}

/// WPF `UIElement.Opacity` state. bevy_ui has no per-node opacity, so the
/// value is approximated by alpha-multiplying every color component in the
/// subtree (Background, BorderColor sides, TextColor). `originals` caches
/// each entity's UNSCALED alphas: re-application and unset are exact, and
/// the color apply-arms below keep the cache fresh when a trigger or style
/// rewrites a color while opacity is active. Children spawned after the
/// last (re-)application pick the scale up on the next Opacity write, and
/// nested Opacity holders compose approximately rather than multiplying
/// exactly — documented v1 limitations.
#[derive(Component, Debug, Default)]
pub struct PfOpacity {
    pub value: f32,
    originals: bevy::platform::collections::HashMap<Entity, OriginalAlphas>,
}

#[derive(Debug, Clone, Copy, Default)]
struct OriginalAlphas {
    bg: Option<f32>,
    border: Option<[f32; 4]>,
    /// Caret colour + selection colour alphas: `bevy_ui_render` draws the
    /// text cursor at whatever alpha `TextCursorStyle` carries, ignoring UI
    /// opacity entirely, so a faded control would otherwise keep a fully
    /// opaque caret floating over it.
    cursor: Option<[f32; 2]>,
    text: Option<f32>,
}

/// Cheap global gate so color writes skip the ancestor walk when no
/// opacity is active anywhere (the common case).
#[derive(Resource, Default)]
pub(crate) struct PfOpacityCount(pub(crate) usize);

fn set_alpha(color: &mut Color, alpha: f32) {
    let mut c = color.to_srgba();
    c.alpha = alpha;
    *color = Color::Srgba(c);
}

/// (Re-)apply a subtree opacity from `root` with factor `value`.
fn apply_subtree_opacity(world: &mut World, root: Entity, value: f32) {
    let had = world.get::<PfOpacity>(root).is_some();
    let mut state = world
        .entity_mut(root)
        .take::<PfOpacity>()
        .unwrap_or_default();
    if !had {
        let mut count = world.get_resource_or_insert_with(PfOpacityCount::default);
        count.0 += 1;
    }
    state.value = value;

    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        let orig = state.originals.entry(e).or_default();
        if let Some(mut bg) = world.get_mut::<bevy::ui::BackgroundColor>(e) {
            let base = *orig.bg.get_or_insert_with(|| bg.0.alpha());
            set_alpha(&mut bg.0, base * value);
        }
        if let Some(mut border) = world.get_mut::<bevy::ui::BorderColor>(e) {
            let base = *orig.border.get_or_insert([
                border.top.alpha(),
                border.right.alpha(),
                border.bottom.alpha(),
                border.left.alpha(),
            ]);
            set_alpha(&mut border.top, base[0] * value);
            set_alpha(&mut border.right, base[1] * value);
            set_alpha(&mut border.bottom, base[2] * value);
            set_alpha(&mut border.left, base[3] * value);
        }
        if let Some(mut text) = world.get_mut::<bevy::text::TextColor>(e) {
            let base = *orig.text.get_or_insert_with(|| text.0.alpha());
            set_alpha(&mut text.0, base * value);
        }
        if let Some(mut cursor) = world.get_mut::<bevy::text::TextCursorStyle>(e) {
            let base = *orig
                .cursor
                .get_or_insert([cursor.color.alpha(), cursor.selection_color.alpha()]);
            set_alpha(&mut cursor.color, base[0] * value);
            set_alpha(&mut cursor.selection_color, base[1] * value);
        }
        if let Some(children) = world.get::<Children>(e) {
            stack.extend(children.iter());
        }
    }
    world.entity_mut(root).insert(state);
}

/// Re-run an entity's own active opacity over its (possibly new) subtree —
/// instantiation applies the attribute before content children spawn.
pub(crate) fn reapply_opacity(world: &mut World, root: Entity) {
    if let Some(value) = world.get::<PfOpacity>(root).map(|o| o.value) {
        apply_subtree_opacity(world, root, value);
    }
}

/// Remove a subtree opacity: restore unscaled alphas.
fn unset_subtree_opacity(world: &mut World, root: Entity) {
    let Some(state) = world.entity_mut(root).take::<PfOpacity>() else {
        return;
    };
    if let Some(mut count) = world.get_resource_mut::<PfOpacityCount>() {
        count.0 = count.0.saturating_sub(1);
    }
    for (e, orig) in state.originals {
        if world.get_entity(e).is_err() {
            continue;
        }
        if let (Some(alpha), Some(mut bg)) =
            (orig.bg, world.get_mut::<bevy::ui::BackgroundColor>(e))
        {
            set_alpha(&mut bg.0, alpha);
        }
        if let Some(sides) = orig.border
            && let Some(mut border) = world.get_mut::<bevy::ui::BorderColor>(e)
        {
            set_alpha(&mut border.top, sides[0]);
            set_alpha(&mut border.right, sides[1]);
            set_alpha(&mut border.bottom, sides[2]);
            set_alpha(&mut border.left, sides[3]);
        }
        if let (Some(alpha), Some(mut text)) =
            (orig.text, world.get_mut::<bevy::text::TextColor>(e))
        {
            set_alpha(&mut text.0, alpha);
        }
    }
}

/// After UNSCALED color writes land on `changed` (all under `entity`),
/// refresh the nearest active opacity holder's cache for them and re-scale
/// — so trigger/style color changes respect an active Opacity immediately.
fn rescale_after_color_writes(world: &mut World, entity: Entity, changed: &[Entity]) {
    if world
        .get_resource::<PfOpacityCount>()
        .is_none_or(|c| c.0 == 0)
    {
        return;
    }
    // Nearest ancestor-or-self holding PfOpacity.
    let mut holder = entity;
    loop {
        if world.get::<PfOpacity>(holder).is_some() {
            break;
        }
        match world.get::<ChildOf>(holder) {
            Some(parent) => holder = parent.parent(),
            None => return,
        }
    }
    let value = world
        .get::<PfOpacity>(holder)
        .map(|o| o.value)
        .unwrap_or(1.0);
    // Recapture the freshly written (unscaled) channels.
    if let Some(mut state) = world.entity_mut(holder).take::<PfOpacity>() {
        for e in changed {
            state.originals.remove(e);
        }
        world.entity_mut(holder).insert(state);
    }
    // Re-apply via the shared walk (correctness over micro-optimization).
    apply_subtree_opacity(world, holder, value);
}

/// TemplateBinding liveness links on a templated control: when `src`'s
/// effective value changes on this entity, forward it to `(child, dst)` at
/// the `ParentTemplate` tier.
#[derive(Component, Debug, Default)]
pub struct PfTemplateBindingDependents(pub Vec<(PropertyTarget, Entity, PropertyTarget)>);

/// Properties whose rendering a `ControlTemplate` takes over: on a templated
/// control they reach the visuals only through TemplateBinding, never by
/// painting the root. Foreground/FontSize are NOT template-consumed (they
/// inherit to descendant text), nor are Margin/Width/Height/Visibility
/// (they lay out the control itself in WPF too).
pub fn is_template_consumed(target: PropertyTarget) -> bool {
    matches!(
        target,
        PropertyTarget::Background
            | PropertyTarget::BorderBrush
            | PropertyTarget::BorderThickness
            | PropertyTarget::Padding
            | PropertyTarget::CornerRadius
    )
}

/// Component/Node application is suppressed for template-consumed targets on
/// templated roots. The STORE write stays untouched — the stored effective
/// value is what TemplateBinding forwarding reads.
fn template_suppressed(world: &World, entity: Entity, target: PropertyTarget) -> bool {
    is_template_consumed(target)
        && world
            .get::<crate::components::PfTemplatedControl>(entity)
            .is_some()
}

/// Record whether a `Background` brush is currently assigned, and for the
/// elements WPF hit-tests purely by their Background, keep pickability in
/// step with it.
///
/// Only elements listed by [`crate::hit_test::background_governs_hit_testing`]
/// are touched, and only through the marker this function owns — a control
/// or an application that set its own `Pickable` is never overwritten.
pub(crate) fn mark_background_assigned(world: &mut World, entity: Entity, assigned: bool) {
    let governed = world
        .get::<crate::components::PfElementKind>(entity)
        .is_some_and(|k| crate::hit_test::background_governs_hit_testing(&k.0));
    let mut e = world.entity_mut(entity);
    if assigned {
        e.insert(crate::hit_test::PfBackgroundSet);
        if governed {
            e.insert(bevy::picking::Pickable::default());
        }
    } else {
        e.remove::<crate::hit_test::PfBackgroundSet>();
        if governed {
            e.insert(bevy::picking::Pickable::IGNORE);
        }
    }
}

/// Apply a concrete value to components (the single component-writer for
/// store-managed properties).
pub(crate) fn apply_value(
    world: &mut World,
    entity: Entity,
    target: PropertyTarget,
    value: &PfValue,
) {
    if template_suppressed(world, entity, target) {
        return;
    }
    let as_brush = || -> Option<v::PfBrush> {
        match value {
            PfValue::Brush(b) => Some(b.clone()),
            PfValue::Color(c) => Some(v::PfBrush::Solid(*c)),
            PfValue::String(s) => s.parse().ok(),
            _ => None,
        }
    };
    let as_f32 = || -> Option<f32> {
        match value {
            PfValue::Double(d) => Some(*d as f32),
            PfValue::String(s) => s.trim().parse().ok(),
            _ => None,
        }
    };
    let as_thickness = || -> Option<v::Thickness> {
        match value {
            PfValue::Thickness(t) => Some(*t),
            PfValue::Double(d) => Some(v::Thickness::uniform(*d as f32)),
            PfValue::String(s) => s.parse().ok(),
            _ => None,
        }
    };

    match target {
        PropertyTarget::Background => {
            let Some(brush) = as_brush() else { return };
            // A brush was assigned — non-null, even if it paints nothing.
            // For a Panel that is the whole of WPF's hit-test rule.
            mark_background_assigned(world, entity, true);
            match convert::brush_to_background(&brush) {
                Ok(bg) => {
                    if let Some(mut visual) =
                        world.get_mut::<crate::components::ButtonVisual>(entity)
                    {
                        visual.normal_bg = bg.0;
                    }
                    world
                        .entity_mut(entity)
                        .remove::<bevy::ui::BackgroundGradient>()
                        .insert(bg);
                }
                Err(gradient) => {
                    world.entity_mut(entity).insert(gradient);
                }
            }
        }
        PropertyTarget::BorderBrush => {
            // A GRADIENT OUTLINE IS A REAL BRUSH, not an unsupported one.
            // This arm used to match `Solid` alone, so a LinearGradientBrush
            // on a Border fell straight through and the outline drew nothing
            // — no warning, no fallback, the signature of every Aero and
            // Fluent theme simply absent. bevy_ui already models it:
            // BorderGradient is the same Vec<Gradient> as BackgroundGradient
            // and its renderer honours border_radius and per-side widths.
            match as_brush() {
                Some(v::PfBrush::Solid(c)) => {
                    let color = convert::color(c);
                    if let Some(mut visual) =
                        world.get_mut::<crate::components::ButtonVisual>(entity)
                    {
                        visual.normal_border = color;
                    }
                    world
                        .entity_mut(entity)
                        // A solid outline and a gradient one are mutually
                        // exclusive; leaving a stale gradient behind would
                        // paint over the colour that just replaced it.
                        .remove::<bevy::ui::BorderGradient>()
                        .insert(BorderColor::all(color));
                }
                Some(brush) => {
                    if let Some(stages) = convert::brush_to_gradients(&brush) {
                        world
                            .entity_mut(entity)
                            // BorderColor::DEFAULT is transparent: the solid
                            // layer must get out of the gradient's way rather
                            // than tint it.
                            .insert((
                                BorderColor::all(Color::NONE),
                                bevy::ui::BorderGradient(stages),
                            ));
                    }
                }
                None => {}
            }
        }
        PropertyTarget::BorderThickness => {
            if let Some(t) = as_thickness()
                && let Some(mut node) = world.get_mut::<Node>(entity)
            {
                node.border = convert::thickness(t);
            }
        }
        PropertyTarget::Fill => {
            if let Some(brush) = as_brush() {
                let brush = brush.clone();
                let mut e = world.entity_mut(entity);
                // See WRITE ONLY ON A REAL CHANGE above.
                let differs = e
                    .get::<crate::shapes::PfShape>()
                    .is_some_and(|s| s.fill.as_ref() != Some(&brush));
                if differs && let Some(mut shape) = e.get_mut::<crate::shapes::PfShape>() {
                    shape.fill = Some(brush);
                    // Dropping the rendered marker forces re-rasterization
                    // at the unchanged layout size.
                    e.remove::<crate::shapes::PfShapeRendered>();
                }
            }
        }
        PropertyTarget::Stroke => {
            if let Some(brush) = as_brush() {
                let brush = brush.clone();
                let mut e = world.entity_mut(entity);
                let differs = e
                    .get::<crate::shapes::PfShape>()
                    .is_some_and(|s| s.stroke.as_ref() != Some(&brush));
                if differs && let Some(mut shape) = e.get_mut::<crate::shapes::PfShape>() {
                    shape.stroke = Some(brush);
                    e.remove::<crate::shapes::PfShapeRendered>();
                }
            }
        }
        PropertyTarget::Foreground => {
            if let Some(v::PfBrush::Solid(c)) = as_brush() {
                let color = convert::color(c);
                for text_entity in collect_text_entities(world, entity) {
                    world
                        .entity_mut(text_entity)
                        .insert(bevy::text::TextColor(color));
                }
            }
        }
        PropertyTarget::FontSize => {
            let Some(px) = as_f32() else { return };
            for text_entity in collect_text_entities(world, entity) {
                if let Some(mut font) = world.get_mut::<bevy::text::TextFont>(text_entity) {
                    font.font_size = bevy::text::FontSize::Px(px);
                }
            }
        }
        PropertyTarget::Margin => {
            if let Some(t) = as_thickness()
                && let Some(mut node) = world.get_mut::<Node>(entity)
            {
                node.margin = convert::thickness(t);
            }
        }
        PropertyTarget::Padding => {
            if let Some(t) = as_thickness()
                && let Some(mut node) = world.get_mut::<Node>(entity)
            {
                node.padding = convert::thickness(t);
            }
        }
        PropertyTarget::CornerRadius => {
            let radius = match value {
                PfValue::CornerRadius(r) => Some(*r),
                PfValue::Double(d) => Some(v::CornerRadius::uniform(*d as f32)),
                PfValue::String(s) => s.parse().ok(),
                _ => None,
            };
            if let Some(r) = radius
                && let Some(mut node) = world.get_mut::<Node>(entity)
            {
                node.border_radius = convert::corner_radius(r);
            }
        }
        PropertyTarget::Width | PropertyTarget::Height => {
            let Some(px) = as_f32() else { return };
            if let Some(mut node) = world.get_mut::<Node>(entity) {
                // WPF: an explicit Width/Height is a HARD size. Without the
                // min/max clamp a control's default minimums win (a TextBox
                // carries min_height 24), so `Height="1"` silently laid out
                // 24px tall.
                let dim = convert::dimension(px);
                if target == PropertyTarget::Width {
                    node.width = dim;
                    node.min_width = dim;
                    node.max_width = dim;
                } else {
                    node.height = dim;
                    node.min_height = dim;
                    node.max_height = dim;
                }
            }
        }
        PropertyTarget::Opacity => {
            let Some(v) = as_f32() else { return };
            apply_subtree_opacity(world, entity, v.clamp(0.0, 1.0));
        }
        PropertyTarget::Visibility => {
            let vis = match value {
                PfValue::String(s) => s.parse::<v::Visibility>().ok(),
                PfValue::Bool(b) => Some(if *b {
                    v::Visibility::Visible
                } else {
                    v::Visibility::Collapsed
                }),
                _ => None,
            };
            if let Some(vis) = vis {
                apply_wpf_visibility(world, entity, vis);
            }
        }
    }
}

/// Apply a WPF `Visibility` to an entity: the shared path for static
/// attributes, styles, triggers, and runtime bindings.
///
/// Collapsing stashes the node's own `display` in [`PfCollapsedDisplay`] so
/// restoring puts the real value back; a visible node's `display` is never
/// touched. The old behavior wrote `Display::DEFAULT` (Flex) on every
/// Visible application, which silently converted Grid containers into flex
/// rows — a ScrollViewer with a bound `Visibility` shrink-wrapped its whole
/// content tree.
pub(crate) fn apply_wpf_visibility(world: &mut World, entity: Entity, vis: v::Visibility) {
    use crate::components::PfCollapsedDisplay;
    let (visibility, display) = convert::visibility(vis);
    world.entity_mut(entity).insert(visibility);
    if display == Some(Display::None) {
        let current = world.get::<Node>(entity).map(|n| n.display);
        if let Some(current) = current
            && current != Display::None
        {
            world.entity_mut(entity).insert(PfCollapsedDisplay(current));
        }
        if let Some(mut node) = world.get_mut::<Node>(entity) {
            node.display = Display::None;
        }
    } else {
        let saved = world
            .entity_mut(entity)
            .take::<PfCollapsedDisplay>()
            .map(|s| s.0);
        if let Some(mut node) = world.get_mut::<Node>(entity)
            && node.display == Display::None
        {
            node.display = saved.unwrap_or(Display::DEFAULT);
        }
    }
}

/// Post-write hook: keep active subtree opacities exact when a color
/// target is rewritten (the write above landed UNSCALED).
fn after_apply_value(world: &mut World, entity: Entity, target: PropertyTarget) {
    let changed: Vec<Entity> = match target {
        PropertyTarget::Background | PropertyTarget::BorderBrush => vec![entity],
        PropertyTarget::Foreground => collect_text_entities(world, entity),
        _ => Vec::new(),
    };
    if !changed.is_empty() {
        rescale_after_color_writes(world, entity, &changed);
    }
}

/// Apply the "unset" state (nothing provides a value, or `{x:Null}` wins).
// WRITE ONLY ON A REAL CHANGE.
//
// `get_mut` marks the component changed the moment it is dereferenced,
// whether or not the value differs. These setters are re-applied every
// frame by the trigger and theme passes — re-asserting the SAME brush —
// so writing unconditionally marked every shape in the tree dirty on
// every frame. The CPU rasterizer merely re-rasterized needlessly; the
// GPU atlas backend released and re-reserved a slot and respawned its
// draw entities each frame, which is why templated chrome flickered or
// vanished and only reappeared when a hover happened to re-sync it at
// the right moment.
//
// Comparing first costs one brush equality test and turns a per-frame
// storm into nothing at all when the value is genuinely unchanged.
fn apply_unset(world: &mut World, entity: Entity, target: PropertyTarget) {
    if template_suppressed(world, entity, target) {
        return;
    }
    match target {
        PropertyTarget::Fill => {
            let mut e = world.entity_mut(entity);
            let differs = e
                .get::<crate::shapes::PfShape>()
                .is_some_and(|s| s.fill.is_some());
            if differs && let Some(mut shape) = e.get_mut::<crate::shapes::PfShape>() {
                shape.fill = None;
                e.remove::<crate::shapes::PfShapeRendered>();
            }
        }
        PropertyTarget::Stroke => {
            let mut e = world.entity_mut(entity);
            let differs = e
                .get::<crate::shapes::PfShape>()
                .is_some_and(|s| s.stroke.is_some());
            if differs && let Some(mut shape) = e.get_mut::<crate::shapes::PfShape>() {
                shape.stroke = None;
                e.remove::<crate::shapes::PfShapeRendered>();
            }
        }
        PropertyTarget::Background => {
            // Back to a null brush (`{x:Null}`, or a cleared tier): a Panel
            // stops being hit-testable again, exactly as in WPF.
            mark_background_assigned(world, entity, false);
            world
                .entity_mut(entity)
                .remove::<bevy::ui::BackgroundGradient>()
                .insert(BackgroundColor(Color::NONE));
        }
        PropertyTarget::BorderBrush => {
            world
                .entity_mut(entity)
                .insert(BorderColor::all(Color::NONE));
        }
        PropertyTarget::BorderThickness => {
            if let Some(mut node) = world.get_mut::<Node>(entity) {
                node.border = UiRect::ZERO;
            }
        }
        PropertyTarget::Margin => {
            if let Some(mut node) = world.get_mut::<Node>(entity) {
                node.margin = UiRect::ZERO;
            }
        }
        PropertyTarget::Padding => {
            if let Some(mut node) = world.get_mut::<Node>(entity) {
                node.padding = UiRect::ZERO;
            }
        }
        PropertyTarget::Opacity => {
            unset_subtree_opacity(world, entity);
        }
        PropertyTarget::CornerRadius => {
            if let Some(mut node) = world.get_mut::<Node>(entity) {
                node.border_radius = BorderRadius::ZERO;
            }
        }
        PropertyTarget::Width => {
            if let Some(mut node) = world.get_mut::<Node>(entity) {
                node.width = Val::Auto;
                node.min_width = Val::Auto;
                node.max_width = Val::Auto;
            }
        }
        PropertyTarget::Height => {
            if let Some(mut node) = world.get_mut::<Node>(entity) {
                node.height = Val::Auto;
                node.min_height = Val::Auto;
                node.max_height = Val::Auto;
            }
        }
        PropertyTarget::Visibility => {
            apply_wpf_visibility(world, entity, v::Visibility::Visible);
        }
        // No sane "unset" for text properties without re-deriving
        // inheritance; keep the current value (the Inherited tier is seeded
        // at spawn, so this only happens when nothing was ever set).
        PropertyTarget::Foreground | PropertyTarget::FontSize => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precedence_order_is_wpf_verbatim() {
        assert!(ValueSource::Local > ValueSource::ParentTemplateTrigger);
        assert!(ValueSource::ParentTemplateTrigger > ValueSource::ParentTemplate);
        assert!(ValueSource::ParentTemplate > ValueSource::ImplicitReference);
        assert!(ValueSource::ImplicitReference > ValueSource::StyleTrigger);
        // The one everyone gets backwards: StyleTrigger beats TemplateTrigger.
        assert!(ValueSource::StyleTrigger > ValueSource::TemplateTrigger);
        assert!(ValueSource::TemplateTrigger > ValueSource::Style);
        assert!(ValueSource::Style > ValueSource::ThemeStyleTrigger);
        assert!(ValueSource::ThemeStyleTrigger > ValueSource::ThemeStyle);
        assert!(ValueSource::ThemeStyle > ValueSource::Inherited);
        assert!(ValueSource::Inherited > ValueSource::Default);
    }

    #[test]
    fn store_effective_and_revert() {
        let mut store = PfPropertyStore::default();
        let bg = PropertyTarget::Background;
        let red = Some(PfValue::String("Red".into()));
        let blue = Some(PfValue::String("Blue".into()));
        let green = Some(PfValue::String("Green".into()));

        store.set(bg, ValueSource::Style, red.clone());
        assert_eq!(store.effective_source(bg), Some(ValueSource::Style));

        store.set(bg, ValueSource::StyleTrigger, blue.clone());
        assert_eq!(store.effective_source(bg), Some(ValueSource::StyleTrigger));

        // Local always wins.
        store.set(bg, ValueSource::Local, green.clone());
        assert_eq!(store.effective_source(bg), Some(ValueSource::Local));

        // Removing the trigger tier reverts to the next-highest.
        store.clear(bg, ValueSource::Local);
        assert_eq!(store.effective_source(bg), Some(ValueSource::StyleTrigger));
        store.clear(bg, ValueSource::StyleTrigger);
        assert_eq!(store.effective_source(bg), Some(ValueSource::Style));
        store.clear(bg, ValueSource::Style);
        assert_eq!(store.effective_source(bg), None);
    }

    #[test]
    fn same_tier_overwrites() {
        let mut store = PfPropertyStore::default();
        let bg = PropertyTarget::Background;
        store.set(bg, ValueSource::StyleTrigger, Some(PfValue::Bool(true)));
        store.set(bg, ValueSource::StyleTrigger, Some(PfValue::Bool(false)));
        let (_, v) = store.effective(bg).unwrap();
        assert!(matches!(v, Some(PfValue::Bool(false))));
    }

    #[test]
    fn explicit_null_masks_lower_tiers() {
        let mut store = PfPropertyStore::default();
        let bg = PropertyTarget::Background;
        store.set(bg, ValueSource::Style, Some(PfValue::Bool(true)));
        store.set(bg, ValueSource::StyleTrigger, None); // {x:Null}
        let (source, v) = store.effective(bg).unwrap();
        assert_eq!(*source, ValueSource::StyleTrigger);
        assert!(v.is_none());
    }

    #[test]
    fn clear_tier_reports_affected() {
        let mut store = PfPropertyStore::default();
        store.set(
            PropertyTarget::Background,
            ValueSource::StyleTrigger,
            Some(PfValue::Bool(true)),
        );
        store.set(
            PropertyTarget::Width,
            ValueSource::StyleTrigger,
            Some(PfValue::Double(1.0)),
        );
        store.set(
            PropertyTarget::Height,
            ValueSource::Local,
            Some(PfValue::Double(2.0)),
        );
        let mut affected = store.clear_tier(ValueSource::StyleTrigger);
        affected.sort_by_key(|t| format!("{t:?}"));
        assert_eq!(affected.len(), 2);
        assert_eq!(
            store.effective_source(PropertyTarget::Height),
            Some(ValueSource::Local)
        );
    }
}
