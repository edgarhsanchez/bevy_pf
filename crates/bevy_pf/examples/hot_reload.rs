//! Load a `.xaml` file as a runtime asset and hot-reload it on save.
//!
//! Run with: `cargo run -p bevy_pf --example hot_reload --features hot_reload`
//! then edit `crates/bevy_pf/assets/ui/live_view.xaml` and save.

use bevy::prelude::*;
use bevy_pf::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(PfUiPlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    commands.spawn(Camera2d);
    commands.spawn(XamlView(assets.load("ui/live_view.xaml")));
}
