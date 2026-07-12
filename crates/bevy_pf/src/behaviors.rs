//! Interactivity behaviors — the Microsoft.Xaml.Behaviors surface
//! (`<b:Interaction.Triggers>`), Noesis-plan increment 6.
//!
//! Supported triggers: `EventTrigger EventName="Click|MouseEnter|MouseLeave
//! |Loaded"`. Supported actions: `InvokeCommandAction`,
//! `ControlStoryboardAction` (Play), `GoToStateAction`,
//! `ChangePropertyAction`, `LaunchUriOrFileAction`. `KeyTrigger` and
//! `PlaySoundAction` warn as tracked work (task #39 tail).

use bevy::prelude::*;

use crate::animation::PfStoryboard;
use crate::binding::PfCommandParameter;

/// One parsed action.
#[derive(Debug, Clone)]
pub enum PfAction {
    InvokeCommand {
        name: String,
        parameter: Option<PfCommandParameter>,
    },
    ControlStoryboard {
        storyboard: std::sync::Arc<PfStoryboard>,
    },
    GoToState {
        state: String,
    },
    ChangeProperty {
        target_name: Option<String>,
        property: String,
        value: String,
    },
    LaunchUri {
        path: String,
    },
}

/// Run a trigger's actions against its host element.
pub fn run_actions(world: &mut World, host: Entity, actions: &[PfAction]) {
    for action in actions {
        match action {
            PfAction::InvokeCommand { name, parameter } => {
                crate::binding::invoke_command(world, host, name, parameter.as_ref());
            }
            PfAction::ControlStoryboard { storyboard } => {
                crate::animation::begin_storyboard(world, host, host, storyboard);
            }
            PfAction::GoToState { state } => {
                // Nearest ancestor-or-self carrying visual states; search
                // every group for the named state (WPF resolves the group).
                let mut e = host;
                loop {
                    if let Some(states) = world.get::<crate::animation::PfVisualStates>(e) {
                        let group = states
                            .groups
                            .iter()
                            .find(|g| g.states.iter().any(|s| s.name == *state))
                            .map(|g| g.name.clone());
                        if let Some(group) = group {
                            crate::animation::go_to_state(world, e, &group, state);
                        } else {
                            warn!("bevy_pf: GoToStateAction: no state `{state}` on the control");
                        }
                        break;
                    }
                    match world.get::<ChildOf>(e) {
                        Some(p) => e = p.parent(),
                        None => {
                            warn!("bevy_pf: GoToStateAction found no visual states in scope");
                            break;
                        }
                    }
                }
            }
            PfAction::ChangeProperty {
                target_name,
                property,
                value,
            } => {
                let target = match target_name {
                    None => Some(host),
                    Some(name) => resolve_in_scope(world, host, name),
                };
                let Some(target) = target else {
                    warn!("bevy_pf: ChangePropertyAction target not found");
                    continue;
                };
                let Some(prop) = crate::provider::property_target_for(property) else {
                    warn!(
                        "bevy_pf: ChangePropertyAction `{property}` is not a writable property"
                    );
                    continue;
                };
                crate::provider::set_local(
                    world,
                    target,
                    prop,
                    crate::resources::PfValue::String(value.clone()),
                );
            }
            PfAction::LaunchUri { path } => crate::util::open_url(path),
        }
    }
}

/// Resolve a name against the host's template parts, then the scene
/// namescope (nearest ancestor with `XamlNames`).
fn resolve_in_scope(world: &World, host: Entity, name: &str) -> Option<Entity> {
    let mut e = host;
    loop {
        if let Some(parts) = world.get::<crate::components::PfTemplateParts>(e)
            && let Some(found) = parts.get(name)
        {
            return Some(found);
        }
        if let Some(names) = world.get::<crate::components::XamlNames>(e)
            && let Some(found) = names.get(name)
        {
            return Some(found);
        }
        match world.get::<ChildOf>(e) {
            Some(p) => e = p.parent(),
            None => return None,
        }
    }
}

/// `Loaded` behavior triggers recorded at instantiation, fired next frame.
#[derive(Component, Debug, Default)]
pub struct PfPendingActions(pub Vec<Vec<PfAction>>);

pub(crate) fn run_pending_actions(world: &mut World) {
    let mut q = world.query_filtered::<Entity, With<PfPendingActions>>();
    let hosts: Vec<Entity> = q.iter(world).collect();
    for host in hosts {
        let Some(pending) = world.entity_mut(host).take::<PfPendingActions>() else {
            continue;
        };
        for actions in pending.0 {
            run_actions(world, host, &actions);
        }
    }
}
