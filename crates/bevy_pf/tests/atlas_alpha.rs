//! The shape atlas is composited by bevy_ui as a STRAIGHT-alpha image, so the
//! vector pass that fills it must leave straight pixels behind. It did not:
//! the ordinary `SrcAlpha, OneMinusSrcAlpha` blend over a transparent clear
//! stores `rgb * a`, bevy_ui multiplies by `a` again, and every translucent
//! fill showed at about `a²` — a game's 8% console-button wash rendered as
//! nothing on the GPU backend while the CPU backend (which demultiplies
//! before upload) drew it as designed. The two backends disagreed on the
//! same markup, which is the one thing they are built never to do.
//!
//! The fix is a marker on every atlas camera that switches the vector pass
//! to a blend storing straight colour and alpha. This pins the marker: the
//! blend itself is pinned in `bevy_pf_vector`'s own tests.
#![cfg(feature = "vector_gpu")]

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy_pf::prelude::*;
use bevy_pf::shapes::{PfShape, ShapeGeometry};
use bevy_pf_xaml::value as v;

fn layout_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        bevy::window::WindowPlugin {
            primary_window: None,
            exit_condition: bevy::window::ExitCondition::DontExit,
            ..Default::default()
        },
        bevy::a11y::AccessibilityPlugin,
        bevy::input::InputPlugin,
        bevy::picking::DefaultPickingPlugins,
        bevy::text::TextPlugin,
        bevy::ui::UiPlugin,
    ));
    app.init_asset::<Image>();
    app.init_asset::<bevy::image::TextureAtlasLayout>();
    app.add_plugins(PfUiPlugin);
    app.world_mut().spawn((
        bevy::window::Window {
            resolution: bevy::window::WindowResolution::new(1280, 800),
            ..Default::default()
        },
        bevy::window::PrimaryWindow,
    ));
    let mut camera = Camera::default();
    camera.computed.target_info = Some(bevy::camera::RenderTargetInfo {
        physical_size: UVec2::new(1280, 800),
        scale_factor: 1.0,
    });
    app.world_mut().spawn((Camera2d, camera, OnScreen));
    app
}

/// The one camera that draws to the screen, which must NOT be marked.
#[derive(Component)]
struct OnScreen;

/// A chamfered console button: a Path with a translucent fill, the exact
/// shape whose wash went missing.
fn console_button() -> PfShape {
    let data = bevy_pf_xaml::geometry::parse_path_data(
        "M 0,5 L 5,0 L 61,0 L 66,5 L 66,43 L 61,48 L 5,48 L 0,43 Z",
    )
    .expect("path data");
    let mut shape = PfShape::new(ShapeGeometry::Path(data));
    shape.fill = Some(v::PfBrush::Solid(v::PfColor { r: 0, g: 255, b: 212, a: 20 }));
    shape.stroke = Some(v::PfBrush::Solid(v::PfColor { r: 0, g: 255, b: 212, a: 255 }));
    shape.stroke_thickness = 1.0;
    shape
}

#[test]
fn every_atlas_camera_is_a_straight_alpha_target_and_the_screen_is_not() {
    let mut app = layout_app();
    app.world_mut().spawn((
        Node { width: Val::Px(66.0), height: Val::Px(48.0), ..Default::default() },
        console_button(),
    ));
    app.update();
    app.update();

    let world = app.world_mut();
    let mut atlas_cameras = world.query_filtered::<
        (&Camera, Option<&bevy_pf_vector::StraightAlphaTarget>),
        (With<Camera2d>, Without<OnScreen>),
    >();
    let cameras: Vec<_> = atlas_cameras.iter(world).collect();
    assert!(
        !cameras.is_empty(),
        "a slot-sized Path should have opened an atlas page with its own camera"
    );
    for (camera, marker) in cameras {
        assert!(
            matches!(camera.clear_color, bevy::camera::ClearColorConfig::Custom(c) if c == Color::NONE),
            "an atlas camera clears to transparent; that is why straight output matters"
        );
        assert!(
            marker.is_some(),
            "an atlas camera without StraightAlphaTarget leaves premultiplied pixels \
             for bevy_ui to multiply again — the a² bug this test exists to catch"
        );
    }

    let mut screen =
        world.query_filtered::<Option<&bevy_pf_vector::StraightAlphaTarget>, With<OnScreen>>();
    let on_screen: Vec<_> = screen.iter(world).collect();
    assert_eq!(on_screen.len(), 1);
    assert!(
        on_screen[0].is_none(),
        "the screen camera keeps the ordinary blend; marking it would brighten \
         every translucent shape drawn straight over the scene"
    );
}
