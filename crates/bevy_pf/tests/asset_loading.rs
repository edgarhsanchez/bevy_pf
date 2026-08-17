//! Integration test: load a `.xaml` file through the Bevy asset pipeline and
//! verify the XamlView subtree is instantiated (and re-instantiated on
//! asset modification).

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy_pf::prelude::*;

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin {
            // Use the repo's corpus directory as the asset root.
            file_path: format!("{}/../../tests/corpus", env!("CARGO_MANIFEST_DIR")),
            ..Default::default()
        },
    ));
    app.add_plugins(PfUiPlugin);
    app
}

fn update_until<F: FnMut(&mut World) -> bool>(app: &mut App, mut done: F, max_frames: usize) {
    for _ in 0..max_frames {
        app.update();
        if done(app.world_mut()) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("condition not met after {max_frames} frames");
}

#[test]
fn xaml_view_instantiates_from_asset() {
    let mut app = test_app();
    let handle = app
        .world()
        .resource::<AssetServer>()
        .load::<bevy_pf::XamlAsset>("wpf/hello.xaml");
    let view = app.world_mut().spawn(XamlView(handle)).id();

    update_until(
        &mut app,
        move |world| world.get::<Children>(view).is_some(),
        600,
    );

    let world = app.world_mut();
    // The Window root was instantiated onto the view entity itself.
    assert_eq!(world.get::<PfElementKind>(view).unwrap().0, "Window");
    assert!(world.get::<XamlNames>(view).is_some());
    // And the XamlView component survived the rebuild.
    assert!(world.get::<XamlView>(view).is_some());

    // Re-trigger via a manual asset mutation (simulates hot reload).
    let handle = world.get::<XamlView>(view).unwrap().0.clone();
    world
        .resource_mut::<Assets<bevy_pf::XamlAsset>>()
        .get_mut(&handle)
        .unwrap()
        .source = r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation">
                       <TextBlock Text="reloaded"/>
                     </Grid>"#
        .to_string();

    update_until(
        &mut app,
        move |world| {
            world
                .get::<PfElementKind>(view)
                .is_some_and(|k| k.0 == "Grid")
        },
        600,
    );
}
