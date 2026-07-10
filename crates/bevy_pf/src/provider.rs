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
}

/// The properties the store manages (the dynamically-writable set).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropertyTarget {
    Background,
    BorderBrush,
    BorderThickness,
    Foreground,
    FontSize,
    Margin,
    Padding,
    CornerRadius,
    Width,
    Height,
    Visibility,
}

/// Map a XAML property name to a store-managed target.
pub fn property_target_for(property: &str) -> Option<PropertyTarget> {
    Some(match property {
        "Background" => PropertyTarget::Background,
        "BorderBrush" => PropertyTarget::BorderBrush,
        "BorderThickness" => PropertyTarget::BorderThickness,
        "Foreground" => PropertyTarget::Foreground,
        "FontSize" => PropertyTarget::FontSize,
        "Margin" => PropertyTarget::Margin,
        "Padding" => PropertyTarget::Padding,
        "CornerRadius" => PropertyTarget::CornerRadius,
        "Width" => PropertyTarget::Width,
        "Height" => PropertyTarget::Height,
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
        self.entries
            .get(&target)?
            .iter()
            .max_by_key(|(s, _)| *s)
    }

    /// The tier of the effective value, if any.
    pub fn effective_source(&self, target: PropertyTarget) -> Option<ValueSource> {
        self.effective(target).map(|(s, _)| *s)
    }
}

/// Set a property from application code at the `Local` tier — the same
/// precedence a literal XAML attribute has. Styles, triggers, and dynamic
/// resources keep their values in lower tiers, so they revert correctly if
/// the local value is later cleared. Call from an exclusive system or via
/// `commands.queue(move |world| ...)`.
pub fn set_local(
    world: &mut World,
    entity: Entity,
    target: PropertyTarget,
    value: PfValue,
) {
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
        let mut e = world.entity_mut(entity);
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
        Some((_, Some(v))) => apply_value(world, entity, target, &v),
        Some((_, None)) | None => apply_unset(world, entity, target),
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

/// Apply a concrete value to components (the single component-writer for
/// store-managed properties).
pub(crate) fn apply_value(world: &mut World, entity: Entity, target: PropertyTarget, value: &PfValue) {
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
            if let Some(v::PfBrush::Solid(c)) = as_brush() {
                let color = convert::color(c);
                if let Some(mut visual) =
                    world.get_mut::<crate::components::ButtonVisual>(entity)
                {
                    visual.normal_border = color;
                }
                world.entity_mut(entity).insert(BorderColor::all(color));
            }
        }
        PropertyTarget::BorderThickness => {
            if let Some(t) = as_thickness()
                && let Some(mut node) = world.get_mut::<Node>(entity) {
                    node.border = convert::thickness(t);
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
                && let Some(mut node) = world.get_mut::<Node>(entity) {
                    node.margin = convert::thickness(t);
                }
        }
        PropertyTarget::Padding => {
            if let Some(t) = as_thickness()
                && let Some(mut node) = world.get_mut::<Node>(entity) {
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
                && let Some(mut node) = world.get_mut::<Node>(entity) {
                    node.border_radius = convert::corner_radius(r);
                }
        }
        PropertyTarget::Width | PropertyTarget::Height => {
            let Some(px) = as_f32() else { return };
            if let Some(mut node) = world.get_mut::<Node>(entity) {
                if target == PropertyTarget::Width {
                    node.width = convert::dimension(px);
                } else {
                    node.height = convert::dimension(px);
                }
            }
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
                let (visibility, display) = convert::visibility(vis);
                world.entity_mut(entity).insert(visibility);
                if let Some(mut node) = world.get_mut::<Node>(entity) {
                    node.display = display.unwrap_or(Display::DEFAULT);
                }
            }
        }
    }
}

/// Apply the "unset" state (nothing provides a value, or `{x:Null}` wins).
fn apply_unset(world: &mut World, entity: Entity, target: PropertyTarget) {
    match target {
        PropertyTarget::Background => {
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
        PropertyTarget::CornerRadius => {
            if let Some(mut node) = world.get_mut::<Node>(entity) {
                node.border_radius = BorderRadius::ZERO;
            }
        }
        PropertyTarget::Width => {
            if let Some(mut node) = world.get_mut::<Node>(entity) {
                node.width = Val::Auto;
            }
        }
        PropertyTarget::Height => {
            if let Some(mut node) = world.get_mut::<Node>(entity) {
                node.height = Val::Auto;
            }
        }
        PropertyTarget::Visibility => {
            world.entity_mut(entity).insert(Visibility::Inherited);
            if let Some(mut node) = world.get_mut::<Node>(entity) {
                node.display = Display::DEFAULT;
            }
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
        store.set(PropertyTarget::Height, ValueSource::Local, Some(PfValue::Double(2.0)));
        let mut affected = store.clear_tier(ValueSource::StyleTrigger);
        affected.sort_by_key(|t| format!("{t:?}"));
        assert_eq!(affected.len(), 2);
        assert_eq!(store.effective_source(PropertyTarget::Height), Some(ValueSource::Local));
    }
}
