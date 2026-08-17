//! E2E: RepeatButton re-raises Click while held — first repeat after
//! `Delay` ms, then one per `Interval` ms — and resets on release.

use bevy::asset::AssetPlugin;
use bevy::picking::events::{Click, Pointer};
use bevy::prelude::*;
use bevy_pf::prelude::*;
use bevy_pf::{XamlEnv, instantiate_document_env};
use std::time::Duration;

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()));
    app.add_plugins(PfUiPlugin);
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    // The synthetic Click needs a window to anchor its pointer Location.
    app.world_mut().spawn(bevy::window::Window::default());
    app
}

fn advance(app: &mut App, ms: u64) {
    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .advance_by(Duration::from_millis(ms));
    app.update();
}

#[derive(Resource, Default)]
struct Clicks(std::sync::Arc<std::sync::atomic::AtomicU32>);

#[test]
fn repeat_button_fires_after_delay_then_every_interval() {
    let mut app = test_app();
    app.init_resource::<Clicks>();

    let doc = bevy_pf_xaml::parse(
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
              <RepeatButton x:Name="Up" Delay="100" Interval="50" Content="+"/>
            </StackPanel>"##,
    )
    .expect("parses");
    let world = app.world_mut();
    let root = world.spawn_empty().id();
    let result =
        instantiate_document_env(world, root, &doc, &XamlEnv::default()).expect("instantiates");
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    let button = app
        .world()
        .get::<XamlNames>(root)
        .unwrap()
        .get("Up")
        .unwrap();

    let counter = app.world().resource::<Clicks>().0.clone();
    app.world_mut()
        .entity_mut(button)
        .observe(move |_: On<Pointer<Click>>| {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });
    let clicks = |app: &App| {
        app.world()
            .resource::<Clicks>()
            .0
            .load(std::sync::atomic::Ordering::SeqCst)
    };

    // Held, but before the delay elapses: no repeats.
    app.world_mut()
        .entity_mut(button)
        .insert(Interaction::Pressed);
    advance(&mut app, 0);
    advance(&mut app, 60);
    assert_eq!(clicks(&app), 0, "no repeat before Delay");

    // Crossing the delay fires the first repeat.
    advance(&mut app, 60); // held ~120ms
    assert_eq!(clicks(&app), 1, "first repeat after Delay");

    // Each further interval fires one more (one per frame).
    advance(&mut app, 50); // ~170ms -> due 2
    advance(&mut app, 50); // ~220ms -> due 3
    assert_eq!(clicks(&app), 3, "one repeat per interval");

    // Release resets; no more fires while idle.
    app.world_mut().entity_mut(button).insert(Interaction::None);
    advance(&mut app, 200);
    assert_eq!(clicks(&app), 3, "release stops repeats");

    // A new press restarts the delay from zero.
    app.world_mut()
        .entity_mut(button)
        .insert(Interaction::Pressed);
    advance(&mut app, 0);
    advance(&mut app, 60);
    assert_eq!(clicks(&app), 3, "delay restarts on re-press");
    advance(&mut app, 60);
    assert_eq!(clicks(&app), 4, "second press repeats again");
}
