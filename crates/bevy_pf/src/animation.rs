//! Storyboards: WPF's animation system, phase 2a of the Noesis-samples plan
//! (docs/noesis-samples-gap-analysis.md).
//!
//! `DoubleAnimation` and `ColorAnimation` values write through the value
//! provider store at the `Animation` tier — WPF semantics: an animated value
//! composes above every base tier including Local, and removing it reverts
//! structurally to whatever the base holds (conformance notes §4.1's
//! "animation is a modifier over the base value").
//!
//! Delivery in this phase: `<EventTrigger RoutedEvent="Loaded|MouseEnter|
//! MouseLeave|Click">` with `<BeginStoryboard>` at style scope or in an
//! element's `<X.Triggers>`, plus [`begin_storyboard`] from Rust. Keyframes,
//! easing functions, and indexed property paths are phase 2b; VisualStates
//! build on this in a later increment.

use bevy::prelude::*;

use crate::provider::{self, PropertyTarget, ValueSource};
use crate::resources::PfValue;
use bevy_pf_xaml::value as v;

/// A parsed `<Storyboard>`.
#[derive(Debug, Clone, Default)]
pub struct PfStoryboard {
    pub children: Vec<PfAnimationSpec>,
}

/// One `<DoubleAnimation>` / `<ColorAnimation>` inside a storyboard.
#[derive(Debug, Clone)]
pub struct PfAnimationSpec {
    /// `Storyboard.TargetName` — resolved through the scene namescope at
    /// start; `None` animates the storyboard's host element.
    pub target_name: Option<String>,
    /// `Storyboard.TargetProperty` (store-managed property names in 2a).
    pub target_property: String,
    pub kind: PfAnimKind,
    /// Seconds.
    pub duration: f32,
    /// `BeginTime=` offset in seconds.
    pub begin_time: f32,
    pub repeat: PfRepeat,
    pub auto_reverse: bool,
    pub fill: PfFill,
}

#[derive(Debug, Clone)]
pub enum PfAnimKind {
    Double { from: Option<f64>, to: f64 },
    Color { from: Option<v::PfColor>, to: v::PfColor },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PfRepeat {
    Once,
    Count(f32),
    Forever,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PfFill {
    /// WPF default: the final value keeps composing over the base.
    HoldEnd,
    /// Clear the animation layer on completion (structural revert).
    Stop,
}

/// An `EventTrigger` carrying a storyboard (style scope or element scope).
#[derive(Debug, Clone)]
pub struct PfEventTrigger {
    /// `Loaded`, `MouseEnter`, `MouseLeave`, or `Click` in this phase.
    pub event: String,
    pub storyboard: std::sync::Arc<PfStoryboard>,
}

/// `Loaded` storyboards recorded at instantiation; a startup-ish system
/// begins them on the next update, when the scene's namescope exists.
#[derive(Component, Debug, Default)]
pub struct PfPendingStoryboards(pub Vec<std::sync::Arc<PfStoryboard>>);

/// One live animation instance.
#[derive(Debug)]
struct Running {
    target: Entity,
    prop: PropertyTarget,
    from: PfValue,
    to: PfValue,
    /// `Time::elapsed_secs` when progress 0 begins (start + BeginTime).
    begin: f32,
    duration: f32,
    repeat: PfRepeat,
    auto_reverse: bool,
    fill: PfFill,
}

/// Every animation currently playing.
#[derive(Resource, Default)]
pub struct PfRunningAnimations(Vec<Running>);

/// Start a storyboard: `host` is the element the trigger lives on (the
/// default target), `scope_root` the scene root whose `XamlNames` resolves
/// `Storyboard.TargetName` (pass the host again to search upward).
pub fn begin_storyboard(
    world: &mut World,
    host: Entity,
    scope_root: Entity,
    storyboard: &PfStoryboard,
) {
    let now = world
        .get_resource::<Time>()
        .map(|t| t.elapsed_secs())
        .unwrap_or(0.0);
    // Namescope: the given root, or the nearest ancestor carrying XamlNames.
    let names_root = {
        let mut e = scope_root;
        loop {
            if world.get::<crate::components::XamlNames>(e).is_some() {
                break Some(e);
            }
            match world.get::<ChildOf>(e) {
                Some(p) => e = p.parent(),
                None => break None,
            }
        }
    };

    for spec in &storyboard.children {
        let target = match &spec.target_name {
            None => host,
            Some(name) => {
                let found = names_root
                    .and_then(|r| world.get::<crate::components::XamlNames>(r))
                    .and_then(|n| n.get(name));
                match found {
                    Some(e) => e,
                    None => {
                        warn!("bevy_pf: storyboard TargetName `{name}` not found; skipped");
                        continue;
                    }
                }
            }
        };
        let Some(prop) = provider::property_target_for(&spec.target_property) else {
            warn!(
                "bevy_pf: storyboard TargetProperty `{}` is not animatable yet; skipped",
                spec.target_property
            );
            continue;
        };
        // `From` defaults to the current base value (the effective entry
        // below the Animation tier), like WPF's snapshot-and-replace.
        let base = world
            .get::<provider::PfPropertyStore>(target)
            .and_then(|s| s.effective_below(prop, ValueSource::Animation))
            .and_then(|(_, v)| v.clone());
        let (from, to) = match &spec.kind {
            PfAnimKind::Double { from, to } => {
                let from = from
                    .or_else(|| base.as_ref().and_then(pf_as_f64))
                    .unwrap_or(0.0);
                (PfValue::Double(from), PfValue::Double(*to))
            }
            PfAnimKind::Color { from, to } => {
                let from = from
                    .or_else(|| base.as_ref().and_then(pf_as_color))
                    .unwrap_or(v::PfColor::TRANSPARENT);
                (PfValue::Color(from), PfValue::Color(*to))
            }
        };
        world
            .get_resource_or_insert_with(PfRunningAnimations::default)
            .0
            .push(Running {
                target,
                prop,
                from,
                to,
                begin: now + spec.begin_time,
                duration: spec.duration.max(1.0 / 240.0),
                repeat: spec.repeat,
                auto_reverse: spec.auto_reverse,
                fill: spec.fill,
            });
    }
}

fn pf_as_f64(v: &PfValue) -> Option<f64> {
    match v {
        PfValue::Double(d) => Some(*d),
        PfValue::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn pf_as_color(value: &PfValue) -> Option<v::PfColor> {
    match value {
        PfValue::Color(c) => Some(*c),
        PfValue::Brush(v::PfBrush::Solid(c)) => Some(*c),
        PfValue::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn lerp_color(a: v::PfColor, b: v::PfColor, t: f32) -> v::PfColor {
    let l = |x: u8, y: u8| -> u8 { (x as f32 + (y as f32 - x as f32) * t).round() as u8 };
    v::PfColor::rgba(l(a.r, b.r), l(a.g, b.g), l(a.b, b.b), l(a.a, b.a))
}

/// Advance every running animation; write current values at the Animation
/// tier; retire completed ones per their FillBehavior.
pub(crate) fn tick_animations(world: &mut World) {
    let now = match world.get_resource::<Time>() {
        Some(t) => t.elapsed_secs(),
        None => return,
    };
    let Some(mut running) = world.remove_resource::<PfRunningAnimations>() else {
        return;
    };
    if running.0.is_empty() {
        world.insert_resource(running);
        return;
    }

    let mut retire: Vec<usize> = Vec::new();
    let mut writes: Vec<(Entity, PropertyTarget, PfValue)> = Vec::new();
    let mut stops: Vec<(Entity, PropertyTarget)> = Vec::new();

    for (i, anim) in running.0.iter().enumerate() {
        let local = now - anim.begin;
        if local < 0.0 {
            continue; // BeginTime not reached
        }
        let cycles = local / anim.duration;
        let total = match anim.repeat {
            PfRepeat::Once => 1.0,
            PfRepeat::Count(n) => n,
            PfRepeat::Forever => f32::INFINITY,
        } * if anim.auto_reverse { 2.0 } else { 1.0 };
        let done = cycles >= total;
        let cycle_pos = if done { total } else { cycles };
        // Ping-pong within a forward+reverse pair when AutoReverse.
        let raw = cycle_pos.fract();
        let progress = if anim.auto_reverse {
            let phase = (cycle_pos.floor() as i64) % 2;
            if done {
                // A completed auto-reverse pair rests at the start value.
                0.0
            } else if phase == 1 {
                1.0 - raw
            } else {
                raw
            }
        } else if done {
            1.0
        } else {
            raw
        };

        let value = match (&anim.from, &anim.to) {
            (PfValue::Double(a), PfValue::Double(b)) => {
                PfValue::Double(a + (b - a) * f64::from(progress))
            }
            (PfValue::Color(a), PfValue::Color(b)) => {
                PfValue::Color(lerp_color(*a, *b, progress))
            }
            _ => continue,
        };
        if done {
            retire.push(i);
            match anim.fill {
                PfFill::HoldEnd => writes.push((anim.target, anim.prop, value)),
                PfFill::Stop => stops.push((anim.target, anim.prop)),
            }
        } else {
            writes.push((anim.target, anim.prop, value));
        }
    }

    for i in retire.into_iter().rev() {
        running.0.remove(i);
    }
    // Drop entries whose target despawned.
    running.0.retain(|a| world.get_entity(a.target).is_ok());
    world.insert_resource(running);

    for (target, prop, value) in writes {
        if world.get_entity(target).is_err() {
            continue;
        }
        provider::store_and_apply(world, target, prop, ValueSource::Animation, Some(value));
    }
    for (target, prop) in stops {
        if world.get_entity(target).is_err() {
            continue;
        }
        if let Some(mut store) = world.get_mut::<provider::PfPropertyStore>(target) {
            store.clear(prop, ValueSource::Animation);
        }
        provider::apply_effective(world, target, prop);
    }
}

/// Begin `Loaded` storyboards recorded at instantiation (deferred one frame
/// so the scene's namescope component exists).
pub(crate) fn start_pending_storyboards(world: &mut World) {
    let mut q = world.query_filtered::<Entity, With<PfPendingStoryboards>>();
    let hosts: Vec<Entity> = q.iter(world).collect();
    for host in hosts {
        let Some(pending) = world.entity_mut(host).take::<PfPendingStoryboards>() else {
            continue;
        };
        for sb in pending.0 {
            begin_storyboard(world, host, host, &sb);
        }
    }
}

/// Parse `Duration="0:0:0.3"` / `"0:0:2"` (h:m:s.fff) or a bare seconds
/// number into seconds.
pub fn parse_duration(s: &str) -> Option<f32> {
    let t = s.trim();
    if let Ok(secs) = t.parse::<f32>() {
        return Some(secs);
    }
    let parts: Vec<&str> = t.split(':').collect();
    match parts.as_slice() {
        [h, m, sec] => Some(
            h.parse::<f32>().ok()? * 3600.0
                + m.parse::<f32>().ok()? * 60.0
                + sec.parse::<f32>().ok()?,
        ),
        [m, sec] => Some(m.parse::<f32>().ok()? * 60.0 + sec.parse::<f32>().ok()?),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_parse() {
        assert_eq!(parse_duration("0:0:0.5"), Some(0.5));
        assert_eq!(parse_duration("0:1:2"), Some(62.0));
        assert_eq!(parse_duration("1:0:0"), Some(3600.0));
        assert_eq!(parse_duration("0.25"), Some(0.25));
        assert_eq!(parse_duration("bogus"), None);
    }
}
