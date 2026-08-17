//! Interactivity behaviors — the Microsoft.Xaml.Behaviors surface
//! (`<b:Interaction.Triggers>`).
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
    /// `SetFocusAction`: give keyboard focus to the target (or the host).
    SetFocus {
        target_name: Option<String>,
    },
    /// `PlaySoundAction Source=.. Volume=..` (native; no-op without audio).
    PlaySound {
        path: String,
        volume: f32,
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
                    warn!("bevy_pf: ChangePropertyAction `{property}` is not a writable property");
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
            PfAction::SetFocus { target_name } => {
                let target = match target_name {
                    None => Some(host),
                    Some(name) => resolve_in_scope(world, host, name),
                };
                let Some(target) = target else {
                    warn!("bevy_pf: SetFocusAction target not found");
                    continue;
                };
                // Focus the inner editable if the target is a TextBox-like
                // control, so typing lands where WPF users expect.
                focus_control(world, target);
            }
            PfAction::PlaySound { path, volume } => {
                // AudioPlugin inserts GlobalVolume; without it (headless,
                // wasm demos with audio disabled) the action is a no-op.
                if world.get_resource::<bevy::audio::GlobalVolume>().is_none() {
                    continue;
                }
                let Some(assets) = world.get_resource::<bevy::asset::AssetServer>() else {
                    continue;
                };
                let source: Handle<bevy::audio::AudioSource> = assets.load(path.clone());
                world.spawn((
                    bevy::audio::AudioPlayer(source),
                    bevy::audio::PlaybackSettings::DESPAWN
                        .with_volume(bevy::audio::Volume::Linear(*volume)),
                ));
            }
        }
    }
}

/// The editable entity inside a control, if it has one.
///
/// A `TextBox` is a *control* — chrome, watermark, padding — whose actual text
/// buffer lives on a child carrying `EditableText`. Focus has to land on that
/// child, so anything driving focus programmatically (rather than by click)
/// needs this walk: `PfQuery::by_name` finds the control, this finds the thing
/// that can receive keystrokes.
///
/// Returns `Some(root)` when `root` is itself editable, so it is safe to call
/// with either.
///
/// See [`focus_control`] for the common case.
pub fn find_editable_in(world: &World, root: Entity) -> Option<Entity> {
    if world.get::<bevy::text::EditableText>(root).is_some() {
        return Some(root);
    }
    let children = world.get::<Children>(root)?;
    let kids: Vec<Entity> = children.iter().collect();
    kids.into_iter().find_map(|c| find_editable_in(world, c))
}

/// Move keyboard focus to a control, resolving to its editable child.
///
/// The programmatic counterpart of clicking a `TextBox`. Use it when focus is
/// driven by application state rather than by the pointer — an initial field on
/// screen open, a "next field" key, or restoring focus after a modal closes.
///
/// Setting [`InputFocus`] directly to the control entity does *not* work: the
/// control is not the thing carrying `EditableText`, so keystrokes go nowhere.
/// That silent failure is the reason this is public.
///
/// [`InputFocus`]: bevy::input_focus::InputFocus
pub fn focus_control(world: &mut World, control: Entity) {
    let target = find_editable_in(world, control).unwrap_or(control);
    // `set`, not `insert_resource(InputFocus::from_entity(..))`: the latter
    // replaces the resource wholesale and drops any focus changes buffered this
    // frame, which upstream documents as causing missed FocusGained/FocusLost.
    world
        .resource_mut::<bevy::input_focus::InputFocus>()
        .set(target, bevy::input_focus::FocusCause::Navigated);
}

/// Clear keyboard focus, so no editable receives keystrokes.
///
/// Needed whenever a UI tree is hidden rather than despawned: hiding stops the
/// pointer but not the keyboard, so a focused `TextBox` under a hidden screen
/// keeps swallowing input.
pub fn clear_focus(world: &mut World) {
    world
        .resource_mut::<bevy::input_focus::InputFocus>()
        .clear();
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
pub struct PfKeyTriggers(pub Vec<(KeyCode, bool, Vec<PfAction>)>);

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
                'A' => KeyCode::KeyA,
                'B' => KeyCode::KeyB,
                'C' => KeyCode::KeyC,
                'D' => KeyCode::KeyD,
                'E' => KeyCode::KeyE,
                'F' => KeyCode::KeyF,
                'G' => KeyCode::KeyG,
                'H' => KeyCode::KeyH,
                'I' => KeyCode::KeyI,
                'J' => KeyCode::KeyJ,
                'K' => KeyCode::KeyK,
                'L' => KeyCode::KeyL,
                'M' => KeyCode::KeyM,
                'N' => KeyCode::KeyN,
                'O' => KeyCode::KeyO,
                'P' => KeyCode::KeyP,
                'Q' => KeyCode::KeyQ,
                'R' => KeyCode::KeyR,
                'S' => KeyCode::KeyS,
                'T' => KeyCode::KeyT,
                'U' => KeyCode::KeyU,
                'V' => KeyCode::KeyV,
                'W' => KeyCode::KeyW,
                'X' => KeyCode::KeyX,
                'Y' => KeyCode::KeyY,
                'Z' => KeyCode::KeyZ,
                '0' => KeyCode::Digit0,
                '1' => KeyCode::Digit1,
                '2' => KeyCode::Digit2,
                '3' => KeyCode::Digit3,
                '4' => KeyCode::Digit4,
                '5' => KeyCode::Digit5,
                '6' => KeyCode::Digit6,
                '7' => KeyCode::Digit7,
                '8' => KeyCode::Digit8,
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
    let hosts: Vec<(Entity, PfKeyTriggers)> = q.iter(world).map(|(e, t)| (e, t.clone())).collect();
    let focused = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|f| f.get());
    for (host, triggers) in hosts {
        for (key, active_on_focus, actions) in &triggers.0 {
            if !pressed.contains(key) {
                continue;
            }
            // ActiveOnFocus: only while the host (or a descendant, e.g. a
            // TextBox's inner editable) holds keyboard focus.
            if *active_on_focus {
                let in_scope = focused.is_some_and(|mut f| {
                    loop {
                        if f == host {
                            break true;
                        }
                        match world.get::<ChildOf>(f) {
                            Some(p) => f = p.parent(),
                            None => break false,
                        }
                    }
                });
                if !in_scope {
                    continue;
                }
            }
            run_actions(world, host, actions);
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
