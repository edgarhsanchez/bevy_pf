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

/// `KeyTrigger Key="Return"` bindings on an element: pressing the key runs
/// the actions (v1 scope: global while the element exists — WPF's default
/// ActiveOnFocus=false behavior; focus scoping is tracked work).
#[derive(Component, Debug, Clone)]
pub struct PfKeyTriggers(pub Vec<(KeyCode, Vec<PfAction>)>);

/// Map a WPF `Key=` name onto a Bevy `KeyCode` (the common set).
pub fn key_from_name(name: &str) -> Option<KeyCode> {
    Some(match name.trim() {
        "Return" | "Enter" => KeyCode::Enter,
        "Escape" | "Esc" => KeyCode::Escape,
        "Space" => KeyCode::Space,
        "Tab" => KeyCode::Tab,
        "Back" | "Backspace" => KeyCode::Backspace,
        "Delete" | "Del" => KeyCode::Delete,
        "Up" => KeyCode::ArrowUp,
        "Down" => KeyCode::ArrowDown,
        "Left" => KeyCode::ArrowLeft,
        "Right" => KeyCode::ArrowRight,
        "F1" => KeyCode::F1,
        "F2" => KeyCode::F2,
        "F3" => KeyCode::F3,
        "F4" => KeyCode::F4,
        "F5" => KeyCode::F5,
        single if single.len() == 1 => {
            let c = single.chars().next().unwrap().to_ascii_uppercase();
            match c {
                'A' => KeyCode::KeyA, 'B' => KeyCode::KeyB, 'C' => KeyCode::KeyC,
                'D' => KeyCode::KeyD, 'E' => KeyCode::KeyE, 'F' => KeyCode::KeyF,
                'G' => KeyCode::KeyG, 'H' => KeyCode::KeyH, 'I' => KeyCode::KeyI,
                'J' => KeyCode::KeyJ, 'K' => KeyCode::KeyK, 'L' => KeyCode::KeyL,
                'M' => KeyCode::KeyM, 'N' => KeyCode::KeyN, 'O' => KeyCode::KeyO,
                'P' => KeyCode::KeyP, 'Q' => KeyCode::KeyQ, 'R' => KeyCode::KeyR,
                'S' => KeyCode::KeyS, 'T' => KeyCode::KeyT, 'U' => KeyCode::KeyU,
                'V' => KeyCode::KeyV, 'W' => KeyCode::KeyW, 'X' => KeyCode::KeyX,
                'Y' => KeyCode::KeyY, 'Z' => KeyCode::KeyZ,
                '0' => KeyCode::Digit0, '1' => KeyCode::Digit1, '2' => KeyCode::Digit2,
                '3' => KeyCode::Digit3, '4' => KeyCode::Digit4, '5' => KeyCode::Digit5,
                '6' => KeyCode::Digit6, '7' => KeyCode::Digit7, '8' => KeyCode::Digit8,
                '9' => KeyCode::Digit9,
                _ => return None,
            }
        }
        _ => return None,
    })
}

/// Run key-trigger actions on just-pressed keys.
pub(crate) fn run_key_triggers(world: &mut World) {
    let pressed: Vec<KeyCode> = match world.get_resource::<ButtonInput<KeyCode>>() {
        Some(input) => input.get_just_pressed().copied().collect(),
        None => return,
    };
    if pressed.is_empty() {
        return;
    }
    let mut q = world.query::<(Entity, &PfKeyTriggers)>();
    let hosts: Vec<(Entity, PfKeyTriggers)> =
        q.iter(world).map(|(e, t)| (e, t.clone())).collect();
    for (host, triggers) in hosts {
        for (key, actions) in &triggers.0 {
            if pressed.contains(key) {
                run_actions(world, host, actions);
            }
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
