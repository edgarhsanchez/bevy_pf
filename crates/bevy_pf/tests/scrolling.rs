//! E2E: wheel/trackpad scrolling — `Pointer<Scroll>` on a descendant walks
//! up to the nearest scrollable ancestor and moves its `ScrollPosition`.

use bevy::asset::AssetPlugin;
use bevy::picking::backend::HitData;
use bevy::picking::events::{Pointer, Scroll};
use bevy::picking::pointer::{Location, PointerId};
use bevy::prelude::*;
use bevy_pf::prelude::*;
use bevy_pf::{XamlEnv, instantiate_document_env};

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()));
    app.add_plugins(PfUiPlugin);
    app
}

fn location(world: &mut World, position: Vec2) -> Location {
    let window = world.spawn(bevy::window::Window::default()).id();
    Location {
        target: bevy::camera::RenderTarget::Window(bevy::window::WindowRef::Entity(window))
            .normalize(None)
            .expect("explicit window ref always normalizes"),
        position,
    }
}

#[test]
fn wheel_scrolls_nearest_scrollable_ancestor() {
    let mut app = test_app();
    let doc = bevy_pf_xaml::parse(
        r##"<ScrollViewer xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                          x:Name="SV" Height="100">
              <StackPanel>
                <TextBlock x:Name="Row" Text="row 0"/>
                <TextBlock Text="row 1"/>
                <TextBlock Text="row 2"/>
              </StackPanel>
            </ScrollViewer>"##,
    )
    .expect("parses");
    let world = app.world_mut();
    let root = world.spawn_empty().id();
    let result =
        instantiate_document_env(world, root, &doc, &XamlEnv::default()).expect("instantiates");
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    app.update();

    let names = app.world().get::<XamlNames>(root).unwrap();
    let viewer = names.get("SV").unwrap();
    let row = names.get("Row").unwrap();

    // Scrolling starts at the top.
    let start = app
        .world()
        .get::<bevy::ui::ScrollPosition>(viewer)
        .map(|p| p.0.y)
        .unwrap_or(0.0);
    assert_eq!(start, 0.0);

    // A wheel tick over an inner row (3 lines down = -3 in wheel units).
    let loc = location(app.world_mut(), Vec2::new(10.0, 10.0));
    app.world_mut().trigger(Pointer::new(
        PointerId::Mouse,
        loc,
        Scroll {
            unit: bevy::input::mouse::MouseScrollUnit::Line,
            x: 0.0,
            y: -3.0,
            hit: HitData::new(Entity::PLACEHOLDER, 0.0, None, None),
            phase: bevy::input::touch::TouchPhase::Moved,
        },
        row,
    ));
    app.update();

    // The walk found the ScrollViewer (not the row or the stack) and moved
    // it down by 3 lines x 20px.
    let pos = app
        .world()
        .get::<bevy::ui::ScrollPosition>(viewer)
        .expect("scroll position on the viewer");
    assert_eq!(pos.0.y, 60.0, "3 wheel lines scroll 60px down");
}
