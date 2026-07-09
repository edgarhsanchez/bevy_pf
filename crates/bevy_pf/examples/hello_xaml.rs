//! The smallest bevy_pf app: inline XAML with the `xaml!` macro.
//!
//! Run with: `cargo run -p bevy_pf --example hello_xaml`

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
    commands.spawn_xaml(xaml!(
        r#"<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                   xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                   Title="Hello bevy_pf">
             <Grid>
               <TextBlock Text="Hello, World!" FontSize="36" FontWeight="Bold"
                          Foreground="MidnightBlue"
                          HorizontalAlignment="Center" VerticalAlignment="Center"/>
             </Grid>
           </Window>"#
    ));
}
