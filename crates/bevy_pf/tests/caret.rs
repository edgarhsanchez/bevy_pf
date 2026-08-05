//! Caret blink: the caret's alpha must actually MOVE over time, at the
//! configured cadence, without eroding the authored colour.
//!
//! XAML-authored like every UI test here: a real `<TextBox>` instantiates the
//! caret (`TextCursorStyle` + `PfCaretBase`); the test then drives virtual
//! time and samples alpha at known phases. Sampling at phases, not "after a
//! while", is deliberate — an animation test that only checks the end state
//! cannot tell a working ramp from a snap (that false positive has already
//! shipped once, see storyboards.rs).

use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy::text::TextCursorStyle;
use bevy_pf::caret::PfCaretBlink;
use bevy_pf::prelude::*;
use bevy_pf::{XamlEnv, instantiate_document_env};

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()));
    app.add_plugins(PfUiPlugin);
    app
}

fn spawn(app: &mut App, xaml: &str) -> Entity {
    let doc = bevy_pf_xaml::parse(xaml).expect("parses");
    let world = app.world_mut();
    let root = world.spawn_empty().id();
    let result =
        instantiate_document_env(world, root, &doc, &XamlEnv::default()).expect("instantiates");
    assert!(result.warnings.is_empty(), "warnings: {:?}", result.warnings);
    root
}

fn advance(app: &mut App, secs: f32) {
    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .advance_by(Duration::from_secs_f32(secs));
    app.update();
}

/// The caret entity: the one carrying `TextCursorStyle` under the root.
fn caret_alpha(app: &mut App, root: Entity) -> f32 {
    let mut q = app.world_mut().query::<(Entity, &TextCursorStyle)>();
    let world = app.world();
    let (_, style) = q
        .iter(world)
        .find(|(e, _)| {
            // Under our root (the test world holds nothing else, but be
            // explicit rather than lucky).
            let mut cur = *e;
            loop {
                if cur == root {
                    break true;
                }
                match world.get::<ChildOf>(cur) {
                    Some(p) => cur = p.parent(),
                    None => break false,
                }
            }
        })
        .expect("a TextBox instantiates a caret style");
    style.color.alpha()
}

#[test]
fn caret_pulses_smoothly_at_the_configured_cadence() {
    let mut app = test_app();
    // Obsidian MK-II policy: 0.3 -> 1.0, one second each direction.
    app.insert_resource(PfCaretBlink {
        period_secs: 2.0,
        min_alpha: 0.3,
        smooth: true,
    });
    let root = spawn(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <TextBox x:Name="Input" Text="SYS_AUTH"/>
           </StackPanel>"#,
    );
    app.update();

    // Phase 0.25 of the cycle = halfway up the first ramp: mid alpha.
    advance(&mut app, 0.5);
    let mid = caret_alpha(&mut app, root);
    assert!(
        (0.45..=0.85).contains(&mid),
        "quarter-cycle alpha should sit mid-ramp (~0.65), got {mid}"
    );

    // Phase 0.5 = top of the ramp.
    advance(&mut app, 0.5);
    let top = caret_alpha(&mut app, root);
    assert!(top > 0.95, "half-cycle alpha should be ~1.0, got {top}");

    // Phase 1.0 = back at the bottom: min_alpha, not zero — the MK-II caret
    // never disappears.
    advance(&mut app, 1.0);
    let bottom = caret_alpha(&mut app, root);
    assert!(
        (0.25..=0.4).contains(&bottom),
        "full-cycle alpha should return to min (0.3), got {bottom}"
    );
}

#[test]
fn default_blink_is_the_classic_hard_square_wave() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <TextBox x:Name="Input" Text="hello"/>
           </StackPanel>"#,
    );
    app.update();

    // Default: 1.06 s period, hard. First half fully visible…
    advance(&mut app, 0.25);
    assert!(caret_alpha(&mut app, root) > 0.95, "first half: visible");
    // …second half fully hidden.
    advance(&mut app, 0.53);
    assert!(caret_alpha(&mut app, root) < 0.05, "second half: hidden");
}
