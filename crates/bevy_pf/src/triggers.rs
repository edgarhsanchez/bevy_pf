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
    /// `IsFocused`: true while this control OR A DESCENDANT holds keyboard
    /// focus, matching WPF. The descendant walk matters because a TextBox's
    /// editable child is what actually takes focus, so testing the control
    /// alone would never fire.
    Focused(bool),
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
    /// `None` = the entity holding `PfTriggers`; `Some` = a template child
    /// (a `TargetName` setter in a ControlTemplate trigger).
    pub dest: Option<Entity>,
    /// `StyleTrigger` (7) for Style.Triggers; `TemplateTrigger` (6) for
    /// root-targeted / `ParentTemplateTrigger` (10) for TargetName-targeted
    /// ControlTemplate trigger setters.
    pub tier: ValueSource,
}

#[derive(Debug, Clone, Default)]
pub struct ResolvedTrigger {
    pub conditions: Vec<ResolvedCondition>,
    pub setters: Vec<ResolvedTriggerSetter>,
    /// `Trigger.EnterActions` storyboards, begun on activation.
    pub enter_storyboards: Vec<std::sync::Arc<crate::animation::PfStoryboard>>,
    /// `Trigger.ExitActions` storyboards, begun on deactivation.
    pub exit_storyboards: Vec<std::sync::Arc<crate::animation::PfStoryboard>>,
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
        ResolvedCondition::Focused(expected) => {
            let focused = world
                .get_resource::<bevy::input_focus::InputFocus>()
                .and_then(|f| f.get())
                .is_some_and(|f| {
                    let mut cursor = f;
                    loop {
                        if cursor == entity {
                            break true;
                        }
                        match world.get::<ChildOf>(cursor) {
                            Some(parent) => cursor = parent.parent(),
                            None => break false,
                        }
                    }
                });
            focused == *expected
        }
        ResolvedCondition::Selected(expected) => {
            // The item's parent is the ListBox itself, or its ItemsPanel.
            let list = world.get::<ChildOf>(entity).map(|p| {
                world
                    .get::<crate::components::PfGeneratedItemsHost>(p.parent())
                    .map(|host| host.owner)
                    .unwrap_or(p.parent())
            });
            let selected = list
                .and_then(|l| world.get::<crate::components::PfListBox>(l))
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

/// Re-evaluate every entity's triggers; on any activation change, rebuild
/// each `(dest entity, tier)` pair this component's setters own from all
/// currently-active triggers in declaration order (last active wins) and
/// re-apply affected properties. Style-trigger setters live only at tier 7
/// and template-trigger setters only at tiers 6/10, so per-(entity, tier)
/// clearing keeps merged style+template blocks from clobbering each other.
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

        // Every (dest, tier) pair owned by this component's setters — owned
        // tiers only, so a template rebuild never clears style-trigger
        // entries on the same entity (and vice versa).
        let mut pairs: Vec<(Entity, ValueSource)> = Vec::new();
        for trigger in &list {
            for setter in &trigger.setters {
                let dest = setter.dest.unwrap_or(entity);
                if !pairs.contains(&(dest, setter.tier)) {
                    pairs.push((dest, setter.tier));
                }
            }
        }
        let mut affected: Vec<(Entity, PropertyTarget)> = Vec::new();
        let mut stale: Vec<Entity> = Vec::new();
        for &(dest, tier) in &pairs {
            // Cross-entity dests can be despawned (template swap, partial
            // hot-reload rebuilds) — degrade, never panic.
            let Ok(mut e) = world.get_entity_mut(dest) else {
                stale.push(dest);
                continue;
            };
            if let Some(mut store) = e.get_mut::<PfPropertyStore>() {
                for target in store.clear_tier(tier) {
                    if !affected.contains(&(dest, target)) {
                        affected.push((dest, target));
                    }
                }
            }
        }

        for (trigger, active) in list.iter().zip(&new_active) {
            if !active {
                continue;
            }
            for setter in &trigger.setters {
                let dest = setter.dest.unwrap_or(entity);
                if stale.contains(&dest) {
                    continue;
                }
                let value: StoredValue = match &setter.value {
                    TriggerValue::Static(v) => v.clone(),
                    TriggerValue::Dynamic(key) => {
                        // Resolved against the trigger owner: that is where
                        // the lexical PfResources chain lives.
                        match crate::dynamic::resolve_dynamic(world, entity, key) {
                            Some(v) => Some(v),
                            None => continue, // key absent; leave lower tiers
                        }
                    }
                };
                {
                    let Ok(mut e) = world.get_entity_mut(dest) else {
                        stale.push(dest);
                        continue;
                    };
                    if let Some(mut store) = e.get_mut::<PfPropertyStore>() {
                        store.set(setter.target, setter.tier, value);
                    } else {
                        let mut store = PfPropertyStore::default();
                        store.set(setter.target, setter.tier, value);
                        e.insert(store);
                    }
                }
                if !affected.contains(&(dest, setter.target)) {
                    affected.push((dest, setter.target));
                }
            }
        }

        // Enter/ExitActions: begin storyboards on the flips.
        let mut flips: Vec<std::sync::Arc<crate::animation::PfStoryboard>> = Vec::new();
        for ((trigger, was), is) in list.iter().zip(&old_active).zip(&new_active) {
            if was == is {
                continue;
            }
            let launchers = if *is {
                &trigger.enter_storyboards
            } else {
                &trigger.exit_storyboards
            };
            flips.extend(launchers.iter().cloned());
        }

        if let Some(mut triggers) = world.get_mut::<PfTriggers>(entity) {
            triggers.active = new_active;
            if !stale.is_empty() {
                for trigger in &mut triggers.triggers {
                    trigger
                        .setters
                        .retain(|s| !s.dest.is_some_and(|d| stale.contains(&d)));
                }
            }
        }
        for (dest, target) in affected {
            provider::apply_effective(world, dest, target);
        }
        for sb in flips {
            crate::animation::begin_storyboard(world, entity, entity, &sb);
        }
    }
}
