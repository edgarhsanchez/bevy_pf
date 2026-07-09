//! Resources and styles: implicit styles, BasedOn inheritance, StaticResource.
//!
//! Run with: `cargo run -p bevy_pf --example styling`

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
                    Title="bevy_pf styling">
              <Window.Resources>
                <SolidColorBrush x:Key="AccentBrush" Color="#0078D7"/>
                <SolidColorBrush x:Key="CardBrush" Color="#F3F3F3"/>
                <x:Double x:Key="BodySize">16</x:Double>

                <!-- Implicit style: applies to every TextBlock in scope. -->
                <Style TargetType="TextBlock">
                  <Setter Property="FontSize" Value="{StaticResource BodySize}"/>
                  <Setter Property="Foreground" Value="#333333"/>
                  <Setter Property="Margin" Value="0,2"/>
                </Style>

                <!-- Named style extending the implicit one. -->
                <Style x:Key="Heading" TargetType="TextBlock"
                       BasedOn="{StaticResource {x:Type TextBlock}}">
                  <Setter Property="FontSize" Value="30"/>
                  <Setter Property="FontWeight" Value="Bold"/>
                  <Setter Property="Foreground" Value="{StaticResource AccentBrush}"/>
                </Style>

                <Style TargetType="Border">
                  <Setter Property="Background" Value="{StaticResource CardBrush}"/>
                  <Setter Property="CornerRadius" Value="10"/>
                  <Setter Property="Padding" Value="16"/>
                  <Setter Property="Margin" Value="0,8"/>
                </Style>
              </Window.Resources>

              <StackPanel Margin="32" MaxWidth="560" HorizontalAlignment="Center">
                <TextBlock Text="Styled with resources" Style="{StaticResource Heading}"/>
                <Border>
                  <StackPanel>
                    <TextBlock Text="This card's chrome comes from an implicit Border style."/>
                    <TextBlock Text="Text sizes come from an x:Double resource."/>
                  </StackPanel>
                </Border>
                <Border>
                  <StackPanel>
                    <TextBlock Text="BasedOn chains work like WPF:"/>
                    <TextBlock Text="Heading inherits the implicit style, then overrides."
                               FontStyle="Italic"/>
                  </StackPanel>
                </Border>
                <Button Content="A styled scene" HorizontalAlignment="Left"/>
              </StackPanel>
            </Window>"##
    ));
}
