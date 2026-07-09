//! WPF Grid layout: star/auto/pixel tracks, spans, and attached placement —
//! plus a click handler wired up through x:Name.
//!
//! Run with: `cargo run -p bevy_pf --example grid_layout`

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
    let root = commands.spawn_xaml(xaml!(
        r##"<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                    xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                    Title="bevy_pf grid">
              <Grid Margin="12" RowDefinitions="Auto, *, Auto" ColumnDefinitions="220, *">
                <Border Grid.ColumnSpan="2" Background="#0078D7" Padding="12">
                  <TextBlock Text="Header (spans both columns)" Foreground="White" FontSize="20"/>
                </Border>
                <Border Grid.Row="1" Background="#E8E8E8" Padding="8" Margin="0,8,8,8">
                  <StackPanel>
                    <TextBlock Text="Sidebar" FontWeight="Bold"/>
                    <TextBlock Text="220px column"/>
                  </StackPanel>
                </Border>
                <Border Grid.Row="1" Grid.Column="1" Background="#F8F8F8" Padding="8" Margin="0,8,0,8">
                  <TextBlock Text="Content area (star sized)" />
                </Border>
                <Border Grid.Row="2" Grid.ColumnSpan="2" Background="#DDDDDD" Padding="8">
                  <StackPanel Orientation="Horizontal" Spacing="8">
                    <Button x:Name="ClickMe" Content="Click me"/>
                    <TextBlock x:Name="Status" Text="No clicks yet" VerticalAlignment="Center"/>
                  </StackPanel>
                </Border>
              </Grid>
            </Window>"##
    ));

    // Wire a click handler to the named button once the scene exists.
    commands.queue(move |world: &mut World| {
        let names = world.get::<XamlNames>(root).expect("scene instantiated");
        let (button, status) = (
            names.get("ClickMe").expect("ClickMe exists"),
            names.get("Status").expect("Status exists"),
        );
        let mut count = 0u32;
        world.entity_mut(button).observe(
            move |_click: On<Pointer<Click>>, mut texts: Query<&mut bevy::ui::widget::Text>| {
                count += 1;
                if let Ok(mut text) = texts.get_mut(status) {
                    text.0 = format!("Clicked {count} times");
                }
            },
        );
    });
}
