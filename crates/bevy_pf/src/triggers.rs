//! Runtime evaluation of `Style.Triggers`.
//!
//! WPF rules implemented (dotnet/wpf `StyleHelper.cs:2618`, conformance notes
//! §4.2): all conditions of a trigger must hold; among simultaneously active
//! triggers on the same property, the **last declared wins**; an active
//! trigger's setters apply at the `StyleTrigger` tier of the value-provider
//! store, so deactivation structurally reverts to whatever the next tier
//! holds (style setter, default chrome, ...) and a local value always wins.

use bevy::prelude::*;
use bevy::ui::{Checked, InteractionDisabled};

use crate::binding::find_context;
use crate::provider::{self, PfPropertyStore, PropertyTarget, StoredValue, ValueSource};
use crate::resources::ResourceKey;

/// A trigger condition resolved to a runtime-checkable form.
#[derive(Debug, Clone)]
pub enum ResolvedCondition {
    MouseOver(bool),
    Pressed(bool),
    Checked(bool),
    Enabled(bool),
    Selected(bool),
    /// DataTrigger: `DataContext` path compared (string form) to a value.
    Data { path: String, expected: String },
}

/// A trigger setter value: resolved statically at instantiation, or a
/// `{DynamicResource}` looked up at activation time.
#[derive(Debug, Clone)]
pub enum TriggerValue {
    Static(StoredValue),
    Dynamic(ResourceKey),
}

#[derive(Debug, Clone)]
pub struct ResolvedTriggerSetter {
    pub target: PropertyTarget,
    pub value: TriggerValue,
}

#[derive(Debug, Clone)]
pub struct ResolvedTrigger {
    pub conditions: Vec<ResolvedCondition>,
    pub setters: Vec<ResolvedTriggerSetter>,
}

/// The triggers attached to an entity, with their current activation state.
#[derive(Component, Debug, Default, Clone)]
pub struct PfTriggers {
    pub triggers: Vec<ResolvedTrigger>,
    pub active: Vec<bool>,
}

fn eval_condition(world: &World, entity: Entity, cond: &ResolvedCondition) -> bool {
    match cond {
        ResolvedCondition::MouseOver(expected) => {
            // WPF IsMouseOver is true while pressed too.
            let over = matches!(
                world.get::<Interaction>(entity),
                Some(Interaction::Hovered) | Some(Interaction::Pressed)
            );
            over == *expected
        }
        ResolvedCondition::Pressed(expected) => {
            let pressed = matches!(world.get::<Interaction>(entity), Some(Interaction::Pressed));
            pressed == *expected
        }
        ResolvedCondition::Checked(expected) => {
            (world.get::<Checked>(entity).is_some()) == *expected
        }
        ResolvedCondition::Enabled(expected) => {
            (world.get::<InteractionDisabled>(entity).is_none()) == *expected
        }
        ResolvedCondition::Selected(expected) => {
            let selected = world
                .get::<ChildOf>(entity)
                .and_then(|p| world.get::<crate::components::PfListBox>(p.parent()))
                .and_then(|l| l.selected)
                .is_some_and(|s| s == entity);
            selected == *expected
        }
        ResolvedCondition::Data { path, expected } => {
            let Some(ctx) = find_context(world, entity) else {
                return false;
            };
            let Some(value) = ctx.read_path(path) else {
                return false;
            };
            let actual = value.to_display();
            actual == *expected || actual.eq_ignore_ascii_case(expected)
        }
    }
}

/// Re-evaluate every entity's triggers; on any activation change, rebuild the
/// `StyleTrigger` tier from all currently-active triggers in declaration
/// order (last active wins) and re-apply affected properties.
pub(crate) fn evaluate_triggers(world: &mut World) {
    let mut query = world.query_filtered::<Entity, With<PfTriggers>>();
    let entities: Vec<Entity> = query.iter(world).collect();

    for entity in entities {
        let Some(triggers) = world.get::<PfTriggers>(entity) else {
            continue;
        };
        let list = triggers.triggers.clone();
        let old_active = triggers.active.clone();

        let new_active: Vec<bool> = list
            .iter()
            .map(|t| t.conditions.iter().all(|c| eval_condition(world, entity, c)))
            .collect();
        if new_active == old_active {
            continue;
        }

        // Rebuild the StyleTrigger tier.
        let mut affected: Vec<PropertyTarget> = Vec::new();
        if let Some(mut store) = world.get_mut::<PfPropertyStore>(entity) {
            affected = store.clear_tier(ValueSource::StyleTrigger);
        }
        for (trigger, active) in list.iter().zip(&new_active) {
            if !active {
                continue;
            }
            for setter in &trigger.setters {
                let value: StoredValue = match &setter.value {
                    TriggerValue::Static(v) => v.clone(),
                    TriggerValue::Dynamic(key) => {
                        match crate::dynamic::resolve_dynamic(world, entity, key) {
                            Some(v) => Some(v),
                            None => continue, // key absent; leave lower tiers
                        }
                    }
                };
                {
                    let mut e = world.entity_mut(entity);
                    if let Some(mut store) = e.get_mut::<PfPropertyStore>() {
                        store.set(setter.target, ValueSource::StyleTrigger, value);
                    } else {
                        let mut store = PfPropertyStore::default();
                        store.set(setter.target, ValueSource::StyleTrigger, value);
                        e.insert(store);
                    }
                }
                if !affected.contains(&setter.target) {
                    affected.push(setter.target);
                }
            }
        }

        if let Some(mut triggers) = world.get_mut::<PfTriggers>(entity) {
            triggers.active = new_active;
        }
        for target in affected {
            provider::apply_effective(world, entity, target);
        }
    }
}
