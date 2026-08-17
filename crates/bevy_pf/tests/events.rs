//! E2E: XAML code-behind event attributes (MouseDown="OnPickUp", ...) route
//! to `app.on_ui_event` handlers with pointer info; unregistered names are
//! silent no-ops (verbatim WPF markup instantiates warning-free).

use bevy::asset::AssetPlugin;
use bevy::picking::backend::HitData;
use bevy::picking::events::{Click, Drag, Out, Over, Pointer, Press};
use bevy::picking::pointer::{Location, PointerButton, PointerId};
use bevy::prelude::*;
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
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    root
}

fn named(app: &App, root: Entity, name: &str) -> Entity {
    app.world()
        .get::<XamlNames>(root)
        .unwrap()
        .get(name)
        .unwrap()
}

/// A window-backed pointer location (the picking backend normally fills
/// this in; tests fabricate it).
fn location(world: &mut World, position: Vec2) -> Location {
    let window = world.spawn(bevy::window::Window::default()).id();
    Location {
        target: bevy::camera::RenderTarget::Window(bevy::window::WindowRef::Entity(window))
            .normalize(None)
            .expect("explicit window ref always normalizes"),
        position,
    }
}

fn hit() -> HitData {
    HitData::new(Entity::PLACEHOLDER, 0.0, None, None)
}

#[derive(Resource, Default, Clone)]
struct Fired(std::sync::Arc<std::sync::Mutex<Vec<(String, Vec2, Vec2)>>>);

#[test]
fn pointer_events_route_to_named_handlers() {
    let mut app = test_app();
    app.init_resource::<Fired>();
    let record = |name: &'static str| {
        move |world: &mut World, _e: Entity, info: bevy_pf::PfPointerInfo| {
            world.resource::<Fired>().0.lock().unwrap().push((
                name.into(),
                info.position,
                info.delta,
            ));
        }
    };
    app.on_ui_event("OnPick", record("OnPick"))
        .on_ui_event("OnEnter", record("OnEnter"))
        .on_ui_event("OnLeave", record("OnLeave"));

    let root = spawn(
        &mut app,
        r##"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                  xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
              <Border x:Name="Target" Width="100" Height="100"
                      MouseDown="OnPick" MouseEnter="OnEnter" MouseLeave="OnLeave"
                      Click="NeverRegistered"/>
            </Grid>"##,
    );
    app.update();
    let target = named(&app, root, "Target");
    let loc = location(app.world_mut(), Vec2::new(40.0, 60.0));

    app.world_mut().trigger(Pointer::new(
        PointerId::Mouse,
        loc.clone(),
        Press {
            button: PointerButton::Primary,
            hit: hit(),
            count: 1,
        },
        target,
    ));
    app.world_mut().trigger(Pointer::new(
        PointerId::Mouse,
        loc.clone(),
        Over { hit: hit() },
        target,
    ));
    app.world_mut().trigger(Pointer::new(
        PointerId::Mouse,
        loc.clone(),
        Out { hit: hit() },
        target,
    ));
    // Unregistered handler name: must be silent, not a panic.
    app.world_mut().trigger(Pointer::new(
        PointerId::Mouse,
        loc,
        Click {
            button: PointerButton::Primary,
            hit: hit(),
            duration: std::time::Duration::from_millis(50),
            count: 1,
        },
        target,
    ));
    app.update();

    let fired = app.world().resource::<Fired>().0.lock().unwrap().clone();
    let names: Vec<&str> = fired.iter().map(|(n, _, _)| n.as_str()).collect();
    assert_eq!(names, vec!["OnPick", "OnEnter", "OnLeave"]);
    assert_eq!(
        fired[0].1,
        Vec2::new(40.0, 60.0),
        "position reaches handler"
    );
}

#[test]
fn drag_event_carries_delta() {
    let mut app = test_app();
    app.init_resource::<Fired>();
    app.on_ui_event("OnMove", |world, _e, info| {
        world.resource::<Fired>().0.lock().unwrap().push((
            "OnMove".into(),
            info.position,
            info.delta,
        ));
    });

    let root = spawn(
        &mut app,
        r##"<Canvas xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                    xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
              <Border x:Name="Piece" Width="50" Height="50" Drag="OnMove"/>
            </Canvas>"##,
    );
    app.update();
    let piece = named(&app, root, "Piece");
    let loc = location(app.world_mut(), Vec2::new(200.0, 120.0));

    app.world_mut().trigger(Pointer::new(
        PointerId::Mouse,
        loc,
        Drag {
            button: PointerButton::Primary,
            distance: Vec2::new(15.0, -5.0),
            delta: Vec2::new(3.0, -2.0),
        },
        piece,
    ));
    app.update();

    let fired = app.world().resource::<Fired>().0.lock().unwrap().clone();
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].1, Vec2::new(200.0, 120.0));
    assert_eq!(
        fired[0].2,
        Vec2::new(3.0, -2.0),
        "drag delta reaches handler"
    );
}
