//! Directional focus navigation and programmatic activation — the
//! input-agnostic layer that lets a gamepad (or a TV remote, or a test)
//! drive a bevy_pf UI.
//!
//! The keyboard path (Tab ring + Space/Enter in `keyboard_interaction`)
//! is unusable from a device that cannot press those keys: a gamepad
//! synthesizes key *state* into `ButtonInput<KeyCode>`, while bevy's tab
//! navigation observes keyboard *events* — the two never meet. This
//! module speaks intent instead of keystrokes: an app writes
//! [`PfFocusNav`] messages and the framework moves [`InputFocus`]
//! geometrically, activates the focused control with the same synthetic
//! click Space/Enter deliver, and keeps the focused control scrolled
//! into view.
//!
//! Scoping: apps that keep every screen instantiated and toggle
//! `Visibility` (rather than despawning) set [`PfFocusScope`] to the
//! open screen's root, and navigation is contained to that subtree —
//! `KeyboardNavigation.TabNavigation="Contained"`, in Avalonia terms.
//! Hidden candidates are excluded by walking the *authored* `Visibility`
//! (ancestors included), the same rule `focus_visuals` applies, so
//! headless apps and tests behave identically to rendered ones.

use bevy::input_focus::tab_navigation::TabIndex;
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::prelude::*;
use bevy::ui::{ComputedNode, OverflowAxis, ScrollPosition, UiGlobalTransform};

/// A direction to move focus in, in screen terms (y grows downward).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PfFocusDir {
    Up,
    Down,
    Left,
    Right,
}

impl PfFocusDir {
    /// Unit vector in logical UI space (y down).
    fn axis(self) -> Vec2 {
        match self {
            PfFocusDir::Up => Vec2::new(0.0, -1.0),
            PfFocusDir::Down => Vec2::new(0.0, 1.0),
            PfFocusDir::Left => Vec2::new(-1.0, 0.0),
            PfFocusDir::Right => Vec2::new(1.0, 0.0),
        }
    }
}

/// UI intent, decoupled from any input device. Write these from a
/// gamepad system (or anything else); the framework resolves them
/// against the current focus and [`PfFocusScope`].
#[derive(Message, Debug, Clone, Copy)]
pub enum PfFocusNav {
    /// Move focus to the nearest tab stop in the given direction. With
    /// no current focus — or focus outside the active scope — the
    /// top-left-most candidate is focused instead, so the first press
    /// after opening a screen lands somewhere visible and sensible.
    Move(PfFocusDir),
    /// Activate the focused control: the same entity-targeted synthetic
    /// `Pointer<Click>` that Space/Enter and `RepeatButton` deliver. It
    /// bypasses hit-testing, so it works on controls whose subtree has
    /// pointer picking disabled.
    Activate,
}

/// Restrict directional navigation to one subtree (an open panel, a
/// dialog). `None` navigates the whole tree. The app owns this: set it
/// when a screen opens, clear it when the screen closes. A stale root
/// (despawned by a rebuild) simply yields no candidates — set it again
/// with the new root.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct PfFocusScope(pub Option<Entity>);

/// Programmatically activate a control — the exact synthetic
/// `Pointer<Click>` keyboard activation uses, resolved against the
/// first window. Public so apps can wire non-focus shortcuts (a
/// gamepad bumper pressing a named tab button) to the same path a
/// click takes, handlers and all.
pub fn activate_control(world: &mut World, control: Entity) {
    let mut windows = world.query_filtered::<Entity, With<bevy::window::Window>>();
    let Some(window) = windows.iter(world).next() else {
        return;
    };
    let Some(target) =
        bevy::camera::RenderTarget::Window(bevy::window::WindowRef::Entity(window)).normalize(None)
    else {
        return;
    };
    crate::plugin::synthetic_click(world, target, control);
}

/// True when `entity` or any ancestor is authored `Visibility::Hidden`.
/// The computed form is deliberately not used: render-world propagation
/// never runs headless, and focus rules must not differ under test.
fn hidden(
    entity: Entity,
    visibilities: &Query<&Visibility>,
    parents: &Query<&ChildOf>,
) -> bool {
    let mut cursor = entity;
    loop {
        if visibilities
            .get(cursor)
            .is_ok_and(|v| *v == Visibility::Hidden)
        {
            return true;
        }
        match parents.get(cursor) {
            Ok(parent) => cursor = parent.parent(),
            Err(_) => return false,
        }
    }
}

/// True when `entity` is `root` or a descendant of it.
fn in_subtree(entity: Entity, root: Entity, parents: &Query<&ChildOf>) -> bool {
    let mut cursor = entity;
    loop {
        if cursor == root {
            return true;
        }
        match parents.get(cursor) {
            Ok(parent) => cursor = parent.parent(),
            Err(_) => return false,
        }
    }
}

/// Consume [`PfFocusNav`] messages: geometric focus movement over the
/// visible tab stops, and synthetic activation of the focused control.
pub(crate) fn focus_nav(
    mut messages: MessageReader<PfFocusNav>,
    scope: Res<PfFocusScope>,
    mut focus: ResMut<InputFocus>,
    stops: Query<(Entity, &ComputedNode, &UiGlobalTransform), With<TabIndex>>,
    visibilities: Query<&Visibility>,
    parents: Query<&ChildOf>,
    windows: Query<Entity, With<bevy::window::Window>>,
    mut commands: Commands,
) {
    for message in messages.read() {
        match *message {
            PfFocusNav::Activate => {
                let Some(entity) = focus.get() else { continue };
                let Some(window) = windows.iter().next() else {
                    continue;
                };
                let Some(target) =
                    bevy::camera::RenderTarget::Window(bevy::window::WindowRef::Entity(window))
                        .normalize(None)
                else {
                    continue;
                };
                commands.queue(move |world: &mut World| {
                    crate::plugin::synthetic_click(world, target, entity);
                });
            }
            PfFocusNav::Move(dir) => {
                // Candidates: visible tab stops, inside the scope if one
                // is set. Logical-space centers, per the hit-test rule
                // (ComputedNode and UiGlobalTransform are physical).
                let mut candidates: Vec<(Entity, Vec2)> = Vec::new();
                for (entity, node, transform) in &stops {
                    if let Some(root) = scope.0
                        && !in_subtree(entity, root, &parents)
                    {
                        continue;
                    }
                    if hidden(entity, &visibilities, &parents) {
                        continue;
                    }
                    let scale = node.inverse_scale_factor();
                    if scale <= 0.0 {
                        continue;
                    }
                    candidates.push((entity, transform.translation * scale));
                }
                if candidates.is_empty() {
                    continue;
                }
                // Entity order is stable within a run; sort so ties
                // (unlaid-out geometry in headless tests, perfectly
                // aligned rows) resolve deterministically.
                candidates.sort_by_key(|(e, _)| *e);

                // The current anchor is the candidate that IS the focus
                // or contains it (focus may sit on an inner child).
                let current = focus.get().and_then(|f| {
                    candidates
                        .iter()
                        .find(|(c, _)| *c == f || in_subtree(f, *c, &parents))
                        .copied()
                });

                let next = match current {
                    None => {
                        // Nothing focused here yet: land top-left.
                        candidates
                            .iter()
                            .min_by(|(_, a), (_, b)| {
                                (a.y, a.x)
                                    .partial_cmp(&(b.y, b.x))
                                    .unwrap_or(std::cmp::Ordering::Equal)
                            })
                            .map(|(e, _)| *e)
                    }
                    Some((cur, cur_center)) => {
                        // Nearest candidate strictly in the pressed
                        // direction; off-axis drift is penalized so a
                        // near-aligned control beats a nearer diagonal.
                        let axis = dir.axis();
                        candidates
                            .iter()
                            .filter(|(e, _)| *e != cur)
                            .filter_map(|(e, center)| {
                                let delta = *center - cur_center;
                                let ahead = delta.dot(axis);
                                if ahead <= 0.5 {
                                    return None;
                                }
                                let off_axis = (delta - axis * ahead).length();
                                Some((*e, ahead + off_axis * 2.0))
                            })
                            .min_by(|(_, a), (_, b)| {
                                a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
                            })
                            .map(|(e, _)| e)
                    }
                };
                if let Some(next) = next
                    && focus.get() != Some(next)
                {
                    focus.set(next, FocusCause::Navigated);
                }
            }
        }
    }
}

/// When focus moves, keep it visible: walk from the focused control to
/// the nearest scrollable ancestor and nudge its `ScrollPosition` the
/// minimum distance that brings the control fully into the viewport.
/// Without this, directional navigation walks focus straight off-screen
/// in any list taller than its viewer.
pub(crate) fn scroll_focus_into_view(
    focus: Res<InputFocus>,
    geometry: Query<(&ComputedNode, &UiGlobalTransform)>,
    nodes: Query<&Node>,
    parents: Query<&ChildOf>,
    mut scrollables: Query<&mut ScrollPosition>,
) {
    if !focus.is_changed() {
        return;
    }
    let Some(entity) = focus.get() else { return };
    let Ok((node, transform)) = geometry.get(entity) else {
        return;
    };
    let scale = node.inverse_scale_factor();
    if scale <= 0.0 {
        return;
    }
    let center = transform.translation * scale;
    let half = node.size() * scale / 2.0;

    // Nearest scrollable ancestor that actually scrolls.
    let mut cursor = entity;
    let container = loop {
        match parents.get(cursor) {
            Ok(parent) => {
                cursor = parent.parent();
                let scrolls_here = nodes.get(cursor).is_ok_and(|n| {
                    n.overflow.x == OverflowAxis::Scroll || n.overflow.y == OverflowAxis::Scroll
                }) && scrollables.get(cursor).is_ok();
                if scrolls_here {
                    break cursor;
                }
            }
            Err(_) => return,
        }
    };
    let Ok((c_node, c_transform)) = geometry.get(container) else {
        return;
    };
    let c_scale = c_node.inverse_scale_factor();
    if c_scale <= 0.0 {
        return;
    }
    let c_center = c_transform.translation * c_scale;
    let c_half = c_node.size() * c_scale / 2.0;

    // A little air between the control and the viewport edge, so the
    // NEXT row is already peeking in — the affordance that there is one.
    const MARGIN: f32 = 4.0;
    let axes = nodes
        .get(container)
        .map(|n| (n.overflow.x == OverflowAxis::Scroll, n.overflow.y == OverflowAxis::Scroll))
        .unwrap_or((false, true));
    let Ok(mut position) = scrollables.get_mut(container) else {
        return;
    };
    if axes.1 {
        let above = (c_center.y - c_half.y) - (center.y - half.y - MARGIN);
        let below = (center.y + half.y + MARGIN) - (c_center.y + c_half.y);
        if above > 0.0 {
            position.y -= above;
        } else if below > 0.0 {
            position.y += below;
        }
    }
    if axes.0 {
        let left = (c_center.x - c_half.x) - (center.x - half.x - MARGIN);
        let right = (center.x + half.x + MARGIN) - (c_center.x + c_half.x);
        if left > 0.0 {
            position.x -= left;
        } else if right > 0.0 {
            position.x += right;
        }
    }
}
