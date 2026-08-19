//! A gradient shape must be painted by bevy_ui, not by a texture.
//!
//! bevy_ui ships `BackgroundGradient` and `BorderGradient`, drawn by its own UI
//! pass with the node's `border_radius` and per-side border widths already
//! applied. Until now the native backend treated any non-solid brush as
//! "not expressible as node styling" and handed it to a rasterizer, so every
//! gradient-filled rectangle and circle took an atlas slot it did not need --
//! competing for space with shapes that genuinely need one, and getting
//! demoted (and blinking) when the atlas filled.
//!
//! The assertion that matters is the ABSENCE of an `ImageNode`: not merely
//! "the gradient renders", but "it renders without costing a texture".

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
    // Font atlases live in image/atlas assets; headless needs them
    // registered by hand (no ImagePlugin without a renderer).
    app.init_asset::<Image>();
    app.init_asset::<bevy::image::TextureAtlasLayout>();
    app.add_plugins(PfUiPlugin);
    // A primary window entity gives layout its viewport (no real OS window).
    app.world_mut().spawn((
        bevy::window::Window {
            resolution: bevy::window::WindowResolution::new(1280, 800),
            ..Default::default()
        },
        bevy::window::PrimaryWindow,
    ));
    // No renderer runs headless, so the camera never learns its target
    // size; stamp it by hand and percent-sized UI roots resolve normally.
    let mut camera = Camera::default();
    camera.computed.target_info = Some(bevy::camera::RenderTargetInfo {
        physical_size: UVec2::new(1280, 800),
        scale_factor: 1.0,
    });
    app.world_mut().spawn((Camera2d, camera));
    app
}

fn stop(r: u8, g: u8, b: u8, offset: f32) -> v::GradientStop {
    v::GradientStop {
        color: v::PfColor { r, g, b, a: 255 },
        offset,
    }
}

fn pt(x: f32, y: f32) -> v::Point {
    v::Point { x, y }
}

fn spawn_rect(app: &mut App, shape: PfShape) -> Entity {
    app.world_mut()
        .spawn((
            Node {
                width: Val::Px(120.0),
                height: Val::Px(40.0),
                ..Default::default()
            },
            shape,
        ))
        .id()
}

#[test]
fn a_gradient_fill_is_painted_by_bevy_ui_without_a_texture() {
    let mut app = layout_app();
    let mut shape = PfShape::new(ShapeGeometry::Rectangle {
        radius_x: 4.0,
        radius_y: 4.0,
    });
    shape.fill = Some(v::PfBrush::LinearGradient {
        start: pt(0.0, 0.0),
        end: pt(1.0, 0.0),
        stops: vec![stop(0, 0, 255, 0.0), stop(255, 255, 0, 1.0)],
    });
    let entity = spawn_rect(&mut app, shape);
    app.update();
    app.update();

    let e = app.world().entity(entity);
    assert!(
        e.get::<bevy::ui::BackgroundGradient>().is_some(),
        "a gradient fill should be painted by bevy_ui's own gradient component"
    );
    assert!(
        e.get::<ImageNode>().is_none(),
        "painting it natively is the whole point -- an ImageNode means it still \
         cost a rasterized texture and an atlas slot"
    );
}

#[test]
fn swapping_a_gradient_for_a_solid_removes_the_gradient() {
    let mut app = layout_app();
    let mut shape = PfShape::new(ShapeGeometry::Rectangle {
        radius_x: 0.0,
        radius_y: 0.0,
    });
    shape.fill = Some(v::PfBrush::LinearGradient {
        start: pt(0.0, 0.0),
        end: pt(1.0, 0.0),
        stops: vec![stop(0, 0, 255, 0.0), stop(255, 255, 0, 1.0)],
    });
    let entity = spawn_rect(&mut app, shape);
    app.update();
    app.update();
    assert!(
        app.world()
            .entity(entity)
            .get::<bevy::ui::BackgroundGradient>()
            .is_some(),
        "precondition: the gradient is applied"
    );

    app.world_mut()
        .entity_mut(entity)
        .get_mut::<PfShape>()
        .expect("shape")
        .fill = Some(v::PfBrush::Solid(v::PfColor {
        r: 255,
        g: 112,
        b: 67,
        a: 255,
    }));
    app.update();
    app.update();

    let e = app.world().entity(entity);
    assert!(
        e.get::<bevy::ui::BackgroundGradient>().is_none(),
        "a stale gradient component outlives the brush that made it and keeps \
         painting over the solid colour that replaced it -- it has to be REMOVED, \
         not merely set to an empty stop list"
    );
    assert_eq!(
        e.get::<BackgroundColor>().map(|c| c.0),
        Some(Color::srgba_u8(255, 112, 67, 255)),
        "the solid should now be the paint"
    );
}
