//! ItemsSource + DataTemplate, ComboBox dropdowns, and ToolTips: a small
//! data-driven roster where the list, the template bindings, and the combo
//! all read from one view-model.
//!
//! Run with: `cargo run -p bevy_pf --example items_and_dropdowns`

use bevy::prelude::*;
use bevy::reflect::Reflect;
use bevy_pf::prelude::*;

#[derive(Reflect, Default)]
struct Player {
    name: String,
    score: u32,
}

#[derive(Reflect, Default)]
struct Vm {
    players: Vec<Player>,
}

#[derive(Resource, Clone)]
struct SharedVm(Bindable);

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(PfUiPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, score_ticks)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let vm = Bindable::new(Vm {
        players: vec![
            Player { name: "Ada".into(), score: 120 },
            Player { name: "Grace".into(), score: 95 },
            Player { name: "Alan".into(), score: 88 },
        ],
    });
    commands.insert_resource(SharedVm(vm.clone()));

    commands.spawn_xaml_bound(
        xaml!(
            r##"<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                        Title="bevy_pf items + dropdowns">
                  <StackPanel Margin="40" MaxWidth="480" HorizontalAlignment="Center" Spacing="10">
                    <TextBlock Text="Scoreboard" FontSize="26" FontWeight="Bold"
                               ToolTip="Live from the view-model"/>

                    <ListBox ItemsSource="{Binding players}">
                      <ListBox.ItemTemplate>
                        <DataTemplate>
                          <Grid ColumnDefinitions="*, Auto" Width="380">
                            <TextBlock Text="{Binding name}" FontSize="16"/>
                            <TextBlock Grid.Column="1" Foreground="#0078D7"
                                       Text="{Binding score, StringFormat='{}{0} pts'}"/>
                          </Grid>
                        </DataTemplate>
                      </ListBox.ItemTemplate>
                    </ListBox>

                    <StackPanel Orientation="Horizontal" Spacing="8">
                      <TextBlock Text="Focus player:" VerticalAlignment="Center"/>
                      <ComboBox Width="180" SelectedIndex="0"
                                ItemsSource="{Binding players}"
                                DisplayMemberPath="name"
                                ToolTip="Dropdown in the popup overlay"/>
                    </StackPanel>

                    <TextBlock Text="Scores tick live; hover things for tooltips."
                               Foreground="#666666" TextWrapping="Wrap"/>
                  </StackPanel>
                </Window>"##
        ),
        vm,
    );
}

fn score_ticks(vm: Res<SharedVm>, time: Res<Time>, mut acc: Local<f32>) {
    *acc += time.delta_secs();
    if *acc >= 1.0 {
        *acc = 0.0;
        vm.0.update(|m: &mut Vm| {
            for (i, p) in m.players.iter_mut().enumerate() {
                p.score += (i as u32 + 1) * 3;
            }
        });
    }
}
