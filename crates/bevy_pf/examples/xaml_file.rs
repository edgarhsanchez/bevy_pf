//! Load a XAML view from a separate `.xaml` file, validated at compile time
//! with `include_xaml!`.
//!
//! Run with: `cargo run -p bevy_pf --example xaml_file`

use bevy::prelude::*;
use bevy_pf::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(PfUiPlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.spawn_xaml(include_xaml!("examples/xaml/main_view.xaml"));
}
