//! MVVM-style data binding: a reflected view-model drives the UI, and
//! controls write back (TwoWay) — TextBox text, CheckBox state, Slider value.
//!
//! Run with: `cargo run -p bevy_pf --example data_binding`

use bevy::prelude::*;
use bevy::reflect::Reflect;
use bevy_pf::prelude::*;

#[derive(Reflect, Default)]
struct DashboardVm {
    player: String,
    score: u32,
    volume: f32,
    muted: bool,
}

/// The shared view-model handle, cloned into systems that mutate it.
#[derive(Resource, Clone)]
struct Vm(Bindable);

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(PfUiPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, score_ticker)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);

    let vm = Bindable::new(DashboardVm {
        player: "Player One".into(),
        volume: 40.0,
        ..Default::default()
    });
    commands.insert_resource(Vm(vm.clone()));

    commands.spawn_xaml_bound(
        xaml!(
            r##"<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                        Title="bevy_pf data binding">
                  <StackPanel Margin="40" MaxWidth="460" HorizontalAlignment="Center" Spacing="10">
                    <TextBlock Text="Live dashboard" FontSize="26" FontWeight="Bold"/>

                    <TextBlock Text="{Binding player, StringFormat='Hello, {0}!'}" FontSize="18"/>
                    <TextBlock Text="{Binding score, StringFormat=Score: {0} pts}"
                               Foreground="#0078D7" FontSize="18"/>

                    <Border Background="#F3F3F3" CornerRadius="8" Padding="14">
                      <StackPanel Spacing="8">
                        <TextBlock Text="Player name (TwoWay TextBox):"/>
                        <TextBox Text="{Binding player}"/>
                        <CheckBox Content="Mute audio" IsChecked="{Binding muted}"/>
                        <TextBlock Text="Volume:"/>
                        <Slider Minimum="0" Maximum="100" Value="{Binding volume}"/>
                        <ProgressBar Minimum="0" Maximum="100" Value="{Binding volume}"/>
                      </StackPanel>
                    </Border>

                    <TextBlock Text="The score ticks up from a Bevy system; edits flow back into the view-model."
                               TextWrapping="Wrap" Foreground="#666666"/>
                  </StackPanel>
                </Window>"##
        ),
        vm,
    );
}

/// A plain Bevy system mutating the view-model — the UI follows.
fn score_ticker(vm: Res<Vm>, time: Res<Time>, mut acc: Local<f32>) {
    *acc += time.delta_secs();
    if *acc >= 1.0 {
        *acc = 0.0;
        vm.0.update(|m: &mut DashboardVm| m.score += 10);
    }
}
