//! WPF Shapes: Rectangle, Ellipse, Line, Polyline, Polygon, and Path with the
//! geometry mini-language — vector graphics rasterized into the UI.
//!
//! Run with: `cargo run -p bevy_pf --example shapes`

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
        r##"<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                    xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                    Title="bevy_pf shapes">
              <StackPanel Margin="32" Spacing="16">
                <TextBlock Text="WPF Shapes in Bevy" FontSize="26" FontWeight="Bold"/>

                <StackPanel Orientation="Horizontal" Spacing="16">
                  <Rectangle Width="120" Height="80" RadiusX="12" RadiusY="12">
                    <Rectangle.Fill>
                      <LinearGradientBrush StartPoint="0,0" EndPoint="1,1">
                        <GradientStop Offset="0" Color="#0078D7"/>
                        <GradientStop Offset="1" Color="#00B294"/>
                      </LinearGradientBrush>
                    </Rectangle.Fill>
                  </Rectangle>

                  <Ellipse Width="80" Height="80" Fill="Gold" Stroke="DarkOrange" StrokeThickness="3"/>

                  <Path Fill="MediumSeaGreen" Stroke="DarkGreen" StrokeThickness="2"
                        Data="M 10,50 C 30,0 70,0 90,50 A 20,20 0 1 1 50,90 Z"/>

                  <Polygon Points="30,0 60,60 0,60" Fill="IndianRed"/>
                </StackPanel>

                <StackPanel Orientation="Horizontal" Spacing="16">
                  <Path Width="120" Height="120" Stretch="Uniform" Fill="#E81123"
                        Data="M 50,90 C 20,70 0,50 0,30 A 20,20 0 0 1 40,20 L 50,30 L 60,20 A 20,20 0 0 1 100,30 C 100,50 80,70 50,90 Z"/>
                  <Polyline Points="0,40 20,10 40,35 60,5 80,30 100,0"
                            Stroke="SteelBlue" StrokeThickness="4"/>
                  <Line X1="0" Y1="0" X2="120" Y2="60" Stroke="Gray" StrokeThickness="2"/>
                </StackPanel>

                <TextBlock Text="Rectangle + gradient, Ellipse, Path (beziers + arcs), Polygon, a Uniform-stretched heart, Polyline, Line."
                           TextWrapping="Wrap" Foreground="#666666"/>
              </StackPanel>
            </Window>"##
    ));
}
