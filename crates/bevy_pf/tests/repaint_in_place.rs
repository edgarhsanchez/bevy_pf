//! A repaint must not mint a new texture.
//!
//! `rasterize_shapes` allocates a fresh `Image` asset for every render. That is
//! unavoidable when a shape RESIZES -- the dimensions differ -- but a shape
//! that merely changes colour keeps the same pixel dimensions, and minting a
//! whole texture for it is pure waste on the most common interaction there is:
//! hover.
//!
//! It went unnoticed because the guard used to skip repaints entirely (it
//! compared rendered SIZE only), so a brush change never reached the allocation
//! at all -- it never repainted either, which was the bug that hid this one.
//! Measured on the resize harness: 105,500 image assets and 19 GB of texture
//! churned in 540 frames.

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy_pf::prelude::*;
use bevy_pf::shapes::{PfShape, PfShapeClaim, ShapeGeometry};
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
fn solid(r: u8, g: u8, b: u8) -> v::PfBrush {
    v::PfBrush::Solid(v::PfColor { r, g, b, a: 255 })
}

#[test]
fn a_colour_change_repaints_the_texture_it_already_owns() {
    let mut app = layout_app();

    // A non-square ellipse cannot be represented by bevy_ui's rounded-box
    // primitive, so it deliberately exercises the tiny-skia fallback even
    // when the native-shapes feature is enabled by default.
    let mut shape = PfShape::new(ShapeGeometry::Ellipse);
    shape.fill = Some(solid(0, 255, 212));
    let entity = app
        .world_mut()
        .spawn((
            Node {
                // Larger than the vector atlas so this remains a true CPU
                // fallback test when `vector_gpu` is also enabled.
                width: Val::Px(4096.0),
                height: Val::Px(48.0),
                ..Default::default()
            },
            shape,
        ))
        .id();

    app.update();
    app.update();

    let first = app
        .world()
        .entity(entity)
        .get::<ImageNode>()
        .expect("the CPU rasterizer should have installed an image")
        .image
        .clone();
    let before = app
        .world()
        .resource::<Assets<Image>>()
        .get(&first)
        .and_then(|i| i.data.clone())
        .expect("rasterized pixels");

    // Repaint only: same geometry, same size, different brush.
    app.world_mut()
        .entity_mut(entity)
        .get_mut::<PfShape>()
        .expect("shape")
        .fill = Some(solid(255, 112, 67));
    app.update();
    app.update();

    let second = app
        .world()
        .entity(entity)
        .get::<ImageNode>()
        .expect("image node")
        .image
        .clone();
    assert_eq!(
        first.id(),
        second.id(),
        "a colour change kept the same size, so it must repaint the existing \
         texture rather than allocate another one"
    );

    let after = app
        .world()
        .resource::<Assets<Image>>()
        .get(&second)
        .and_then(|i| i.data.clone())
        .expect("rasterized pixels");
    assert_ne!(
        before, after,
        "the repaint has to actually reach the pixels -- same handle with \
         unchanged data means the brush change was dropped"
    );
}

#[test]
fn a_native_rectangle_updates_without_allocating_a_texture() {
    let mut app = layout_app();

    let mut shape = PfShape::new(ShapeGeometry::Rectangle {
        radius_x: 6.0,
        radius_y: 6.0,
    });
    shape.fill = Some(solid(0, 255, 212));
    let entity = app
        .world_mut()
        .spawn((
            Node {
                width: Val::Px(64.0),
                height: Val::Px(48.0),
                ..Default::default()
            },
            shape,
        ))
        .id();

    app.update();
    app.update();

    assert_eq!(
        app.world()
            .get::<PfShapeClaim>(entity)
            .map(|claim| claim.backend),
        Some("bevy_ui_native")
    );
    assert!(app.world().get::<ImageNode>(entity).is_none());

    app.world_mut()
        .entity_mut(entity)
        .get_mut::<PfShape>()
        .expect("shape")
        .fill = Some(solid(255, 112, 67));
    app.update();
    app.update();

    assert_eq!(
        app.world().get::<BackgroundColor>(entity),
        Some(&BackgroundColor(Color::srgb_u8(255, 112, 67)))
    );
    assert!(app.world().get::<ImageNode>(entity).is_none());
}

#[test]
fn native_shapes_relinquish_and_reclaim_backend_ownership() {
    let mut app = layout_app();

    let mut shape = PfShape::new(ShapeGeometry::Rectangle {
        radius_x: 4.0,
        radius_y: 4.0,
    });
    shape.fill = Some(solid(0, 255, 212));
    let entity = app
        .world_mut()
        .spawn((
            Node {
                width: Val::Px(64.0),
                height: Val::Px(48.0),
                ..Default::default()
            },
            shape,
        ))
        .id();

    app.update();
    app.update();
    assert_eq!(
        app.world()
            .get::<PfShapeClaim>(entity)
            .map(|claim| claim.backend),
        Some("bevy_ui_native")
    );

    // A non-square ellipse is an oval, which Bevy's rounded box cannot
    // represent. It must leave native ownership instead of becoming a
    // capsule or retaining stale native styling. With vector_gpu enabled the
    // atlas claims it; otherwise it falls through to tiny-skia.
    app.world_mut()
        .entity_mut(entity)
        .get_mut::<PfShape>()
        .expect("shape")
        .geometry = ShapeGeometry::Ellipse;
    app.update();
    app.update();

    #[cfg(feature = "vector_gpu")]
    assert_eq!(
        app.world()
            .get::<PfShapeClaim>(entity)
            .map(|claim| claim.backend),
        Some("vector_gpu")
    );
    #[cfg(not(feature = "vector_gpu"))]
    assert!(app.world().get::<PfShapeClaim>(entity).is_none());
    assert!(app.world().get::<ImageNode>(entity).is_some());
    assert_eq!(
        app.world().get::<BackgroundColor>(entity),
        Some(&BackgroundColor(Color::NONE))
    );
    assert_eq!(
        app.world().get::<BorderColor>(entity),
        Some(&BorderColor::all(Color::NONE))
    );

    // Returning to native-compatible geometry removes the fallback texture
    // and restores ownership without remounting the UI entity.
    app.world_mut()
        .entity_mut(entity)
        .get_mut::<PfShape>()
        .expect("shape")
        .geometry = ShapeGeometry::Rectangle {
        radius_x: 4.0,
        radius_y: 4.0,
    };
    app.update();
    app.update();

    assert_eq!(
        app.world()
            .get::<PfShapeClaim>(entity)
            .map(|claim| claim.backend),
        Some("bevy_ui_native")
    );
    assert!(app.world().get::<ImageNode>(entity).is_none());
    #[cfg(feature = "vector_gpu")]
    assert!(
        app.world()
            .get::<bevy_pf::shapes_gpu::PfShapeGpu>(entity)
            .is_none()
    );
}
