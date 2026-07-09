//! Style.Triggers + live theming: hover/checked property triggers, a
//! DataTrigger driven by a view-model, and a light/dark theme swap that
//! re-resolves every `{DynamicResource}`.
//!
//! Run with: `cargo run -p bevy_pf --example triggers_theming`

use bevy::prelude::*;
use bevy::reflect::Reflect;
use bevy_pf::prelude::*;
use bevy_pf::resources::{PfValue, ResourceDictionary, ResourceKey};
use bevy_pf::set_application_resources_dict;
use bevy_pf_xaml::value::{PfBrush, PfColor};

#[derive(Reflect, Default)]
struct Vm {
    health: u32,
}

#[derive(Resource, Clone)]
struct SharedVm(Bindable);

#[derive(Resource)]
struct DarkMode(bool);

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(PfUiPlugin)
        .insert_resource(DarkMode(false))
        .add_systems(Startup, setup)
        .add_systems(Update, health_pulse)
        .run();
}

fn theme(dark: bool) -> ResourceDictionary {
    let mut dict = ResourceDictionary::new();
    let (bg, card, text) = if dark {
        (PfColor::rgb(0x1E, 0x1E, 0x1E), PfColor::rgb(0x2D, 0x2D, 0x2D), PfColor::rgb(0xEE, 0xEE, 0xEE))
    } else {
        (PfColor::rgb(0xFF, 0xFF, 0xFF), PfColor::rgb(0xF3, 0xF3, 0xF3), PfColor::rgb(0x22, 0x22, 0x22))
    };
    dict.insert(
        ResourceKey::Explicit("Theme.WindowBg".into()),
        PfValue::Brush(PfBrush::Solid(bg)),
    );
    dict.insert(
        ResourceKey::Explicit("Theme.CardBg".into()),
        PfValue::Brush(PfBrush::Solid(card)),
    );
    dict.insert(
        ResourceKey::Explicit("Theme.Text".into()),
        PfValue::Brush(PfBrush::Solid(text)),
    );
    dict
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let vm = Bindable::new(Vm { health: 100 });
    commands.insert_resource(SharedVm(vm.clone()));

    let root = commands.spawn_xaml_bound(
        xaml!(
            r##"<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                        Title="bevy_pf triggers + theming"
                        Background="{DynamicResource Theme.WindowBg}">
                  <Window.Resources>
                    <!-- Hover/press feedback via property triggers. -->
                    <Style x:Key="HotCard" TargetType="Border">
                      <Setter Property="Background" Value="{DynamicResource Theme.CardBg}"/>
                      <Setter Property="CornerRadius" Value="10"/>
                      <Setter Property="Padding" Value="16"/>
                      <Setter Property="Margin" Value="0,6"/>
                      <Style.Triggers>
                        <Trigger Property="IsMouseOver" Value="True">
                          <Setter Property="Background" Value="#3D0078D7"/>
                          <Setter Property="BorderBrush" Value="#0078D7"/>
                          <Setter Property="BorderThickness" Value="2"/>
                        </Trigger>
                      </Style.Triggers>
                    </Style>
                    <!-- Health bar goes red when the view-model says so. -->
                    <Style x:Key="HealthCard" TargetType="Border"
                           BasedOn="{StaticResource HotCard}">
                      <Style.Triggers>
                        <DataTrigger Binding="{Binding health}" Value="0">
                          <Setter Property="Background" Value="#66E81123"/>
                        </DataTrigger>
                      </Style.Triggers>
                    </Style>
                    <Style TargetType="TextBlock">
                      <Setter Property="Foreground" Value="{DynamicResource Theme.Text}"/>
                    </Style>
                  </Window.Resources>
                  <StackPanel Margin="40" MaxWidth="520" HorizontalAlignment="Center" Spacing="4">
                    <TextBlock Text="Triggers + theming" FontSize="28" FontWeight="Bold"/>
                    <Border Style="{StaticResource HotCard}">
                      <TextBlock Text="Hover me — property triggers with structural revert."/>
                    </Border>
                    <Border Style="{StaticResource HealthCard}">
                      <StackPanel Spacing="6">
                        <TextBlock Text="{Binding health, StringFormat='Health: {0}'}"/>
                        <ProgressBar Minimum="0" Maximum="100" Value="{Binding health}"/>
                        <TextBlock Text="DataTrigger turns this card red at zero."/>
                      </StackPanel>
                    </Border>
                    <Button x:Name="ThemeToggle" Content="Toggle light / dark"
                            HorizontalAlignment="Left" Margin="0,10"/>
                  </StackPanel>
                </Window>"##
        ),
        vm,
    );

    // Light theme initially; the named button swaps it.
    commands.queue(move |world: &mut World| {
        set_application_resources_dict(world, theme(false));
        let names = world.get::<XamlNames>(root).expect("instantiated");
        let toggle = names.get("ThemeToggle").expect("ThemeToggle exists");
        world.entity_mut(toggle).observe(
            move |_click: On<Pointer<Click>>, mut commands: Commands| {
                commands.queue(|world: &mut World| {
                    let dark = !world.resource::<DarkMode>().0;
                    world.insert_resource(DarkMode(dark));
                    set_application_resources_dict(world, theme(dark));
                });
            },
        );
    });
}

/// The health value cycles so the DataTrigger fires periodically.
fn health_pulse(vm: Res<SharedVm>, time: Res<Time>, mut acc: Local<f32>) {
    *acc += time.delta_secs();
    if *acc >= 0.15 {
        *acc = 0.0;
        vm.0.update(|m: &mut Vm| {
            m.health = m.health.saturating_sub(2);
            if m.health == 0 {
                // Linger at zero briefly via wrap-around.
                m.health = 100;
            }
        });
    }
}
