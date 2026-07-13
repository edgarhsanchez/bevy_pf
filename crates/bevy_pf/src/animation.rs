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
//! element's `<X.Triggers>`, plus [`begin_storyboard`] from Rust, with the
//! full WPF easing-function set. Keyframe collections and indexed transform
//! property paths remain (phase 2b tail); VisualStates build on this in a
//! later increment.

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
    pub easing: PfEasing,
}

#[derive(Debug, Clone)]
pub enum PfAnimKind {
    Double { from: Option<f64>, to: f64 },
    Color { from: Option<v::PfColor>, to: v::PfColor },
    /// `<DoubleAnimationUsingKeyFrames>`.
    DoubleKeyFrames { frames: Vec<PfKeyFrame> },
    /// `<ColorAnimationUsingKeyFrames>`.
    ColorKeyFrames { frames: Vec<PfKeyFrame> },
}

/// One keyframe. `time: None` = `KeyTime="Uniform"` (distributed across the
/// duration at start). The value is typed by the owning animation kind.
#[derive(Debug, Clone)]
pub struct PfKeyFrame {
    pub time: Option<f32>,
    pub value: PfKeyValue,
    pub interp: PfKeyInterp,
}

#[derive(Debug, Clone, Copy)]
pub enum PfKeyValue {
    Double(f64),
    Color(v::PfColor),
}

/// How a keyframe interpolates INTO its value from the previous one
/// (WPF semantics: the frame owns the segment that ends at it).
#[derive(Debug, Clone, Copy)]
pub enum PfKeyInterp {
    Discrete,
    Linear,
    Easing(PfEasing),
    /// `KeySpline="x1,y1 x2,y2"` — a cubic bezier through (0,0) and (1,1).
    Spline { x1: f32, y1: f32, x2: f32, y2: f32 },
}

impl PfKeyInterp {
    /// Map segment progress `p` in [0,1] through the interpolation.
    pub fn ease(self, p: f32) -> f32 {
        match self {
            PfKeyInterp::Discrete => {
                if p >= 1.0 {
                    1.0
                } else {
                    0.0
                }
            }
            PfKeyInterp::Linear => p,
            PfKeyInterp::Easing(e) => e.ease(p),
            PfKeyInterp::Spline { x1, y1, x2, y2 } => {
                let bez = |a: f32, b: f32, s: f32| {
                    3.0 * (1.0 - s) * (1.0 - s) * s * a + 3.0 * (1.0 - s) * s * s * b + s * s * s
                };
                // Solve bezier_x(s) = p (Newton with bisection fallback).
                let p = p.clamp(0.0, 1.0);
                let mut s = p;
                for _ in 0..8 {
                    let x = bez(x1, x2, s) - p;
                    if x.abs() < 1e-4 {
                        break;
                    }
                    let dx = 3.0 * (1.0 - s) * (1.0 - s) * x1
                        + 6.0 * (1.0 - s) * s * (x2 - x1)
                        + 3.0 * s * s * (1.0 - x2);
                    if dx.abs() < 1e-6 {
                        break;
                    }
                    s = (s - x / dx).clamp(0.0, 1.0);
                }
                bez(y1, y2, s)
            }
        }
    }
}

/// WPF easing functions (`<DoubleAnimation.EasingFunction>`), plus the
/// Acceleration/DecelerationRatio trapezoid.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum PfEasing {
    #[default]
    Linear,
    Quadratic(EasingMode),
    Cubic(EasingMode),
    Quartic(EasingMode),
    Quintic(EasingMode),
    Sine(EasingMode),
    Circle(EasingMode),
    Back(EasingMode),
    Elastic(EasingMode),
    Bounce(EasingMode),
    Exponential(EasingMode),
    AccelDecel { accel: f32, decel: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EasingMode {
    In,
    #[default]
    Out,
    InOut,
}

impl PfEasing {
    /// Map linear progress `t` in [0,1] through the curve.
    pub fn ease(self, t: f32) -> f32 {
        fn with_mode(mode: EasingMode, f: impl Fn(f32) -> f32, t: f32) -> f32 {
            match mode {
                EasingMode::In => f(t),
                EasingMode::Out => 1.0 - f(1.0 - t),
                EasingMode::InOut => {
                    if t < 0.5 {
                        f(t * 2.0) / 2.0
                    } else {
                        1.0 - f((1.0 - t) * 2.0) / 2.0
                    }
                }
            }
        }
        let t = t.clamp(0.0, 1.0);
        match self {
            PfEasing::Linear => t,
            PfEasing::Quadratic(m) => with_mode(m, |x| x * x, t),
            PfEasing::Cubic(m) => with_mode(m, |x| x * x * x, t),
            PfEasing::Quartic(m) => with_mode(m, |x| x * x * x * x, t),
            PfEasing::Quintic(m) => with_mode(m, |x| x * x * x * x * x, t),
            PfEasing::Sine(m) => {
                with_mode(m, |x| 1.0 - (x * std::f32::consts::FRAC_PI_2).cos(), t)
            }
            PfEasing::Circle(m) => with_mode(m, |x| 1.0 - (1.0 - x * x).max(0.0).sqrt(), t),
            PfEasing::Back(m) => with_mode(
                m,
                |x| x * x * x - x * 1.0 * (std::f32::consts::PI * x).sin(),
                t,
            ),
            PfEasing::Elastic(m) => with_mode(
                m,
                |x| {
                    if x <= 0.0 {
                        0.0
                    } else {
                        // WPF ElasticEase defaults: Oscillations=3, Springiness=3.
                        let osc = 3.0_f32;
                        let spring = 3.0_f32;
                        ((spring * x).exp() - 1.0) / (spring.exp() - 1.0)
                            * (x * std::f32::consts::TAU * (osc + 0.5)).sin()
                    }
                },
                t,
            ),
            PfEasing::Bounce(m) => with_mode(
                m,
                |x| {
                    // WPF BounceEase defaults: Bounces=3, Bounciness=2 —
                    // use the common piecewise-parabola approximation.
                    let x = 1.0 - x;
                    let v = if x < 1.0 / 2.75 {
                        7.5625 * x * x
                    } else if x < 2.0 / 2.75 {
                        let x = x - 1.5 / 2.75;
                        7.5625 * x * x + 0.75
                    } else if x < 2.5 / 2.75 {
                        let x = x - 2.25 / 2.75;
                        7.5625 * x * x + 0.9375
                    } else {
                        let x = x - 2.625 / 2.75;
                        7.5625 * x * x + 0.984375
                    };
                    1.0 - v
                },
                t,
            ),
            PfEasing::Exponential(m) => with_mode(
                m,
                |x| {
                    // WPF ExponentialEase default Exponent=2.
                    ((2.0_f32 * x).exp() - 1.0) / (2.0_f32.exp() - 1.0)
                },
                t,
            ),
            PfEasing::AccelDecel { accel, decel } => {
                // WPF's Accel/DecelRatio trapezoidal velocity profile.
                let a = accel.clamp(0.0, 1.0);
                let d = decel.clamp(0.0, 1.0 - a);
                let denom = 1.0 - a / 2.0 - d / 2.0;
                if denom <= 0.0 {
                    return t;
                }
                let v = 1.0 / denom;
                if t < a {
                    v * t * t / (2.0 * a)
                } else if t <= 1.0 - d {
                    v * (t - a / 2.0)
                } else {
                    let tail = 1.0 - t;
                    1.0 - v * tail * tail / (2.0 * d)
                }
            }
        }
    }
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

/// A `<VisualStateGroup>` parsed from a ControlTemplate.
#[derive(Debug, Clone)]
pub struct PfVisualStateGroup {
    pub name: String,
    pub states: Vec<PfVisualState>,
}

/// A `<VisualState>`: entering it starts its storyboard; leaving stops it
/// and reverts the animated values structurally.
#[derive(Debug, Clone)]
pub struct PfVisualState {
    pub name: String,
    pub storyboard: Option<std::sync::Arc<PfStoryboard>>,
}

/// Visual states attached to a templated control; `current` per group.
#[derive(Component, Debug, Clone)]
pub struct PfVisualStates {
    pub groups: Vec<PfVisualStateGroup>,
    pub current: Vec<Option<String>>,
    /// Per group: the (target, property) pairs the current state's
    /// storyboard touched — exact revert on exit, including values a
    /// HoldEnd retirement left composing after the animation finished.
    pub touched: Vec<Vec<(Entity, PfAnimTarget, Option<PfValue>)>>,
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

/// What an animation writes into: a store-managed property (composes at the
/// Animation tier, revert = structural) or a `UiTransform` field (written
/// directly; revert = restore the captured base — RenderTransform is a
/// visual-only component, not store-managed).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PfAnimTarget {
    Store(PropertyTarget),
    Transform(TransformField),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformField {
    ScaleX,
    ScaleY,
    Rotation,
    TranslateX,
    TranslateY,
}

/// Map the tail of a `Storyboard.TargetProperty` path onto a transform
/// field (`(TransformGroup.Children)[0].(ScaleTransform.ScaleX)` strips
/// down to `ScaleX` before this).
pub fn transform_field_for(name: &str) -> Option<TransformField> {
    Some(match name {
        "ScaleX" => TransformField::ScaleX,
        "ScaleY" => TransformField::ScaleY,
        "Angle" | "Rotation" => TransformField::Rotation,
        "X" | "TranslateX" => TransformField::TranslateX,
        "Y" | "TranslateY" => TransformField::TranslateY,
        _ => return None,
    })
}

fn read_transform_field(world: &World, entity: Entity, field: TransformField) -> f64 {
    let Some(tf) = world.get::<bevy::ui::UiTransform>(entity) else {
        return match field {
            TransformField::ScaleX | TransformField::ScaleY => 1.0,
            _ => 0.0,
        };
    };
    f64::from(match field {
        TransformField::ScaleX => tf.scale.x,
        TransformField::ScaleY => tf.scale.y,
        TransformField::Rotation => tf.rotation.as_degrees(),
        TransformField::TranslateX => match tf.translation.x {
            Val::Px(px) => px,
            _ => 0.0,
        },
        TransformField::TranslateY => match tf.translation.y {
            Val::Px(px) => px,
            _ => 0.0,
        },
    })
}

fn write_transform_field(world: &mut World, entity: Entity, field: TransformField, value: f64) {
    let Ok(mut e) = world.get_entity_mut(entity) else {
        return;
    };
    if e.get::<bevy::ui::UiTransform>().is_none() {
        e.insert(bevy::ui::UiTransform::default());
    }
    let Some(mut tf) = e.get_mut::<bevy::ui::UiTransform>() else {
        return;
    };
    let v = value as f32;
    match field {
        TransformField::ScaleX => tf.scale.x = v,
        TransformField::ScaleY => tf.scale.y = v,
        TransformField::Rotation => tf.rotation = bevy::math::Rot2::degrees(v),
        TransformField::TranslateX => {
            let y = tf.translation.y;
            tf.translation = bevy::ui::Val2::new(Val::Px(v), y);
        }
        TransformField::TranslateY => {
            let x = tf.translation.x;
            tf.translation = bevy::ui::Val2::new(x, Val::Px(v));
        }
    }
}

/// One live animation instance.
#[derive(Debug)]
struct Running {
    target: Entity,
    prop: PfAnimTarget,
    track: Track,
    /// Transform targets restore this on Stop/state-exit (no store tier).
    revert: Option<PfValue>,
    /// `Time::elapsed_secs` when progress 0 begins (start + BeginTime).
    begin: f32,
    duration: f32,
    repeat: PfRepeat,
    auto_reverse: bool,
    fill: PfFill,
    /// Whole-animation easing (Simple tracks only; keyframes ease per frame).
    easing: PfEasing,
    /// VSM ownership: (control, group index) — a state change stops and
    /// reverts everything its group previously started.
    tag: Option<(Entity, usize)>,
}

#[derive(Debug)]
enum Track {
    Simple {
        from: PfValue,
        to: PfValue,
    },
    /// Resolved keyframes: times in seconds, sorted, with the base value as
    /// the implicit frame at t=0.
    KeyFrames {
        base: PfValue,
        frames: Vec<(f32, PfValue, PfKeyInterp)>,
    },
}

impl Track {
    /// Sample at eased whole-animation progress (Simple) or raw cycle
    /// progress (KeyFrames, which ease per segment).
    fn sample(&self, progress: f32, duration: f32, easing: PfEasing) -> Option<PfValue> {
        match self {
            Track::Simple { from, to } => {
                lerp_value(from, to, easing.ease(progress))
            }
            Track::KeyFrames { base, frames } => {
                let t = progress * duration;
                let mut prev_time = 0.0_f32;
                let mut prev_value = base;
                for (time, value, interp) in frames {
                    if t <= *time {
                        let span = (*time - prev_time).max(1e-6);
                        let local = ((t - prev_time) / span).clamp(0.0, 1.0);
                        return lerp_value(prev_value, value, interp.ease(local));
                    }
                    prev_time = *time;
                    prev_value = value;
                }
                Some(prev_value.clone())
            }
        }
    }
}

fn lerp_value(a: &PfValue, b: &PfValue, t: f32) -> Option<PfValue> {
    match (a, b) {
        (PfValue::Double(x), PfValue::Double(y)) => {
            Some(PfValue::Double(x + (y - x) * f64::from(t)))
        }
        (PfValue::Color(x), PfValue::Color(y)) => Some(PfValue::Color(lerp_color(*x, *y, t))),
        _ => None,
    }
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
    begin_storyboard_tagged(world, host, scope_root, storyboard, None);
}

/// [`begin_storyboard`] with a VSM ownership tag. Returns the resolved
/// (target, property) pairs actually started.
pub(crate) fn begin_storyboard_tagged(
    world: &mut World,
    host: Entity,
    scope_root: Entity,
    storyboard: &PfStoryboard,
    tag: Option<(Entity, usize)>,
) -> Vec<(Entity, PfAnimTarget, Option<PfValue>)> {
    let mut started = Vec::new();
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
                // Template-scope storyboards (VSM) resolve through the
                // control's per-expansion parts first, then the namescope.
                let found = world
                    .get::<crate::components::PfTemplateParts>(scope_root)
                    .and_then(|p| p.get(name))
                    .or_else(|| {
                        names_root
                            .and_then(|r| world.get::<crate::components::XamlNames>(r))
                            .and_then(|n| n.get(name))
                    });
                match found {
                    Some(e) => e,
                    None => {
                        warn!("bevy_pf: storyboard TargetName `{name}` not found; skipped");
                        continue;
                    }
                }
            }
        };
        let prop = match provider::property_target_for(&spec.target_property) {
            Some(p) => PfAnimTarget::Store(p),
            None => match transform_field_for(&spec.target_property) {
                Some(f) => PfAnimTarget::Transform(f),
                None => {
                    warn!(
                        "bevy_pf: storyboard TargetProperty `{}` is not animatable yet; skipped",
                        spec.target_property
                    );
                    continue;
                }
            },
        };
        // `From` defaults to the current base value (the effective entry
        // below the Animation tier; the live field for transforms), like
        // WPF's snapshot-and-replace.
        let base = match prop {
            PfAnimTarget::Store(p) => world
                .get::<provider::PfPropertyStore>(target)
                .and_then(|s| s.effective_below(p, ValueSource::Animation))
                .and_then(|(_, v)| v.clone()),
            PfAnimTarget::Transform(f) => {
                Some(PfValue::Double(read_transform_field(world, target, f)))
            }
        };
        // Transform targets revert by restoring this captured base.
        let revert = match prop {
            PfAnimTarget::Transform(_) => base.clone(),
            PfAnimTarget::Store(_) => None,
        };
        let resolve_frames = |frames: &[PfKeyFrame], duration: f32, color: bool| {
            // Uniform (time=None) keyframes distribute evenly across the
            // duration (an approximation of WPF's remaining-span split).
            let n = frames.len().max(1) as f32;
            let mut out: Vec<(f32, PfValue, PfKeyInterp)> = frames
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let time = f.time.unwrap_or((i as f32 + 1.0) / n * duration);
                    let value = match (f.value, color) {
                        (PfKeyValue::Double(d), _) if !color => PfValue::Double(d),
                        (PfKeyValue::Color(c), _) if color => PfValue::Color(c),
                        (PfKeyValue::Double(d), _) => PfValue::Double(d),
                        (PfKeyValue::Color(c), _) => PfValue::Color(c),
                    };
                    (time, value, f.interp)
                })
                .collect();
            out.sort_by(|a, b| a.0.total_cmp(&b.0));
            out
        };
        let duration = spec.duration.max(1.0 / 240.0);
        let track = match &spec.kind {
            PfAnimKind::Double { from, to } => {
                let from = from
                    .or_else(|| base.as_ref().and_then(pf_as_f64))
                    .unwrap_or(0.0);
                Track::Simple {
                    from: PfValue::Double(from),
                    to: PfValue::Double(*to),
                }
            }
            PfAnimKind::Color { from, to } => {
                let from = from
                    .or_else(|| base.as_ref().and_then(pf_as_color))
                    .unwrap_or(v::PfColor::TRANSPARENT);
                Track::Simple {
                    from: PfValue::Color(from),
                    to: PfValue::Color(*to),
                }
            }
            PfAnimKind::DoubleKeyFrames { frames } => Track::KeyFrames {
                base: PfValue::Double(
                    base.as_ref().and_then(pf_as_f64).unwrap_or(0.0),
                ),
                frames: resolve_frames(frames, duration, false),
            },
            PfAnimKind::ColorKeyFrames { frames } => Track::KeyFrames {
                base: PfValue::Color(
                    base.as_ref()
                        .and_then(pf_as_color)
                        .unwrap_or(v::PfColor::TRANSPARENT),
                ),
                frames: resolve_frames(frames, duration, true),
            },
        };
        let touch_revert = revert.clone();
        world
            .get_resource_or_insert_with(PfRunningAnimations::default)
            .0
            .push(Running {
                tag,
                target,
                prop,
                track,
                revert,
                begin: now + spec.begin_time,
                duration,
                repeat: spec.repeat,
                auto_reverse: spec.auto_reverse,
                fill: spec.fill,
                easing: spec.easing,
            });
        if !started.iter().any(|(t, p, _)| *t == target && *p == prop) {
            started.push((target, prop, touch_revert));
        }
    }
    started
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
    let mut writes: Vec<(Entity, PfAnimTarget, PfValue)> = Vec::new();
    let mut stops: Vec<(Entity, PfAnimTarget, Option<PfValue>)> = Vec::new();

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

        let Some(value) = anim.track.sample(progress, anim.duration, anim.easing) else {
            continue;
        };
        if done {
            retire.push(i);
            match anim.fill {
                PfFill::HoldEnd => writes.push((anim.target, anim.prop, value)),
                PfFill::Stop => stops.push((anim.target, anim.prop, anim.revert.clone())),
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
        apply_anim_value(world, target, prop, value);
    }
    for (target, prop, revert) in stops {
        if world.get_entity(target).is_err() {
            continue;
        }
        revert_anim_target(world, target, prop, revert.as_ref());
    }
}

/// Write an animated value to its target kind.
fn apply_anim_value(world: &mut World, target: Entity, prop: PfAnimTarget, value: PfValue) {
    match prop {
        PfAnimTarget::Store(p) => {
            provider::store_and_apply(world, target, p, ValueSource::Animation, Some(value));
        }
        PfAnimTarget::Transform(f) => {
            if let Some(v) = pf_as_f64(&value) {
                write_transform_field(world, target, f, v);
            }
        }
    }
}

/// Structurally revert an animation target: clear the store's Animation
/// tier, or restore a transform field's captured base.
fn revert_anim_target(
    world: &mut World,
    target: Entity,
    prop: PfAnimTarget,
    revert: Option<&PfValue>,
) {
    match prop {
        PfAnimTarget::Store(p) => {
            if let Some(mut store) = world.get_mut::<provider::PfPropertyStore>(target) {
                store.clear(p, ValueSource::Animation);
            }
            provider::apply_effective(world, target, p);
        }
        PfAnimTarget::Transform(f) => {
            let base = revert.and_then(pf_as_f64).unwrap_or(match f {
                TransformField::ScaleX | TransformField::ScaleY => 1.0,
                _ => 0.0,
            });
            write_transform_field(world, target, f, base);
        }
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

/// Stop every animation a `(control, group)` tag owns and revert its
/// touched targets structurally.
fn stop_tagged(world: &mut World, tag: (Entity, usize)) {
    let Some(mut running) = world.remove_resource::<PfRunningAnimations>() else {
        return;
    };
    let mut touched: Vec<(Entity, PfAnimTarget, Option<PfValue>)> = Vec::new();
    running.0.retain(|a| {
        if a.tag == Some(tag) {
            if !touched
                .iter()
                .any(|(t, p, _)| *t == a.target && *p == a.prop)
            {
                touched.push((a.target, a.prop, a.revert.clone()));
            }
            false
        } else {
            true
        }
    });
    world.insert_resource(running);
    for (target, prop, revert) in touched {
        if world.get_entity(target).is_err() {
            continue;
        }
        revert_anim_target(world, target, prop, revert.as_ref());
    }
}

/// WPF `VisualStateManager.GoToState`: leave the group's current state
/// (stop + revert its storyboard) and enter the named one.
pub fn go_to_state(world: &mut World, control: Entity, group: &str, state: &str) -> bool {
    let Some(states) = world.get::<PfVisualStates>(control) else {
        return false;
    };
    let Some(gi) = states.groups.iter().position(|g| g.name == group) else {
        return false;
    };
    if states.current[gi].as_deref() == Some(state) {
        return true; // already there
    }
    let Some(target_state) = states.groups[gi].states.iter().find(|s| s.name == state) else {
        return false;
    };
    let storyboard = target_state.storyboard.clone();
    let previously_touched = states.touched.get(gi).cloned().unwrap_or_default();

    // Leave the old state: drop its running animations AND revert every
    // property it touched (a HoldEnd value outlives its animation).
    stop_tagged(world, (control, gi));
    for (target, prop, revert) in previously_touched {
        if world.get_entity(target).is_err() {
            continue;
        }
        revert_anim_target(world, target, prop, revert.as_ref());
    }

    let touched = match storyboard {
        Some(sb) => begin_storyboard_tagged(world, control, control, &sb, Some((control, gi))),
        None => Vec::new(),
    };
    if let Some(mut states) = world.get_mut::<PfVisualStates>(control) {
        states.current[gi] = Some(state.to_string());
        if states.touched.len() <= gi {
            states.touched.resize(gi + 1, Vec::new());
        }
        states.touched[gi] = touched;
    }
    true
}

/// Drive `CommonStates` (Normal/MouseOver/Pressed/Disabled) and
/// `CheckStates` (Checked/Unchecked) from ECS state, WPF-priority order.
pub(crate) fn drive_visual_states(world: &mut World) {
    let mut q = world.query_filtered::<Entity, With<PfVisualStates>>();
    let controls: Vec<Entity> = q.iter(world).collect();
    for control in controls {
        let disabled = world
            .get::<bevy::ui::InteractionDisabled>(control)
            .is_some();
        let interaction = world.get::<Interaction>(control).copied();
        let checked = world.get::<bevy::ui::Checked>(control).is_some();

        let common = if disabled {
            "Disabled"
        } else {
            match interaction {
                Some(Interaction::Pressed) => "Pressed",
                Some(Interaction::Hovered) => "MouseOver",
                _ => "Normal",
            }
        };
        let check = if checked { "Checked" } else { "Unchecked" };
        go_to_state(world, control, "CommonStates", common);
        go_to_state(world, control, "CheckStates", check);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_endpoints_and_shape() {
        use EasingMode::*;
        let curves = [
            PfEasing::Linear,
            PfEasing::Quadratic(In),
            PfEasing::Cubic(Out),
            PfEasing::Sine(InOut),
            PfEasing::Circle(Out),
            PfEasing::Bounce(Out),
            PfEasing::Exponential(In),
            PfEasing::AccelDecel { accel: 0.3, decel: 0.3 },
        ];
        for c in curves {
            assert!(c.ease(0.0).abs() < 1e-4, "{c:?} starts at 0");
            assert!((c.ease(1.0) - 1.0).abs() < 1e-3, "{c:?} ends at 1");
        }
        // EaseOut runs ahead of linear; EaseIn lags behind.
        assert!(PfEasing::Cubic(Out).ease(0.25) > 0.25);
        assert!(PfEasing::Cubic(In).ease(0.25) < 0.25);
    }

    #[test]
    fn durations_parse() {
        assert_eq!(parse_duration("0:0:0.5"), Some(0.5));
        assert_eq!(parse_duration("0:1:2"), Some(62.0));
        assert_eq!(parse_duration("1:0:0"), Some(3600.0));
        assert_eq!(parse_duration("0.25"), Some(0.25));
        assert_eq!(parse_duration("bogus"), None);
    }
}
