//! Every built-in theme, switchable live.
//!
//! Pick a theme in the ComboBox and the whole window re-colors in place —
//! each theme is a `ResourceDictionary` of `Pf.*` brushes plus implicit
//! styles referencing them via `{DynamicResource}`, so a swap re-resolves
//! every reference without rebuilding the UI.
//!
//! Run with: `cargo run -p bevy_pf --example theme_gallery`

use bevy::prelude::*;
use bevy::reflect::Reflect;
use bevy_pf::components::PfComboBox;
use bevy_pf::prelude::*;
use bevy_pf::themes::{THEMES, apply_theme};

#[derive(Reflect, Default)]
struct Row {
    name: String,
    kind: String,
}

#[derive(Reflect, Default)]
struct Vm {
    rows: Vec<Row>,
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(PfUiPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, switch_theme)
        .run();
}

fn scene() -> String {
    let items: String = THEMES
        .iter()
        .map(|t| format!(r##"<ComboBoxItem Content="{}"/>"##, t.name))
        .collect();
    format!(
        r##"<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                    xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                    Title="bevy_pf theme gallery">
              <DockPanel>
                <ToolBar DockPanel.Dock="Top">
                  <TextBlock Text="Theme:" VerticalAlignment="Center" Margin="4,0,8,0"/>
                  <ComboBox x:Name="ThemePick" Width="200" SelectedIndex="0">{items}</ComboBox>
                </ToolBar>

                <StatusBar DockPanel.Dock="Bottom">
                  <StatusBarItem><TextBlock x:Name="Status" Text="theme: fluent-light"/></StatusBarItem>
                </StatusBar>

                <Grid Margin="16" ColumnDefinitions="280, 16, *">
                  <StackPanel>
                    <TextBlock Text="Inputs" FontSize="18" FontWeight="Bold" Margin="0,0,0,8"/>
                    <Button Content="A themed button" Width="220" HorizontalAlignment="Left"/>
                    <ToggleButton Content="Toggle" Width="220" Margin="0,6,0,0" HorizontalAlignment="Left"/>
                    <TextBox Text="Themed input" Width="220" Margin="0,6,0,0" HorizontalAlignment="Left"/>
                    <CheckBox Content="Themed check" IsChecked="True" Margin="0,8,0,0"/>
                    <DatePicker Width="220" Margin="0,8,0,0" HorizontalAlignment="Left"/>
                    <ProgressBar Minimum="0" Maximum="100" Value="64" Width="220" Height="12" Margin="0,10,0,0" HorizontalAlignment="Left"/>
                    <ProgressBar IsIndeterminate="True" Width="220" Height="12" Margin="0,6,0,0" HorizontalAlignment="Left"/>
                    <Hyperlink Content="A themed hyperlink" NavigateUri="https://github.com/edgarhsanchez/bevy_pf" Margin="0,10,0,0"/>
                  </StackPanel>

                  <StackPanel Grid.Column="2">
                    <TextBlock Text="Data" FontSize="18" FontWeight="Bold" Margin="0,0,0,8"/>
                    <ListView ItemsSource="{{Binding rows}}" Height="220">
                      <ListView.View>
                        <GridView>
                          <GridViewColumn Header="Name" DisplayMemberBinding="{{Binding name}}" Width="2*"/>
                          <GridViewColumn Header="Kind" DisplayMemberBinding="{{Binding kind}}" Width="*"/>
                        </GridView>
                      </ListView.View>
                    </ListView>
                    <Calendar Margin="0,12,0,0" HorizontalAlignment="Left"/>
                  </StackPanel>
                </Grid>
              </DockPanel>
            </Window>"##
    )
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.queue(|world: &mut World| {
        apply_theme(world, THEMES[0].slug).expect("built-in theme applies");
    });
    let vm = Bindable::new(Vm {
        rows: vec![
            Row { name: "fluent-light".into(), kind: "light".into() },
            Row { name: "nord".into(), kind: "dark".into() },
            Row { name: "dracula".into(), kind: "dark".into() },
            Row { name: "catppuccin-mocha".into(), kind: "dark".into() },
            Row { name: "solarized-light".into(), kind: "light".into() },
        ],
    });
    let scene = XamlScene::parse(scene()).expect("gallery scene is valid XAML");
    commands.spawn_xaml_bound(scene, vm);
}

/// Apply the picked theme whenever the ComboBox selection changes.
fn switch_theme(
    combos: Query<&PfComboBox, Changed<PfComboBox>>,
    ui: PfQuery,
    mut texts: Query<&mut bevy::ui::widget::Text>,
    mut commands: Commands,
    mut last: Local<Option<usize>>,
) {
    let Some(combo) = combos.iter().next() else {
        return;
    };
    let Some(index) = combo.selected else {
        return;
    };
    if *last == Some(index) {
        return;
    }
    *last = Some(index);
    let theme = THEMES[index.min(THEMES.len() - 1)];
    commands.queue(move |world: &mut World| {
        if let Err(e) = apply_theme(world, theme.slug) {
            warn!("bevy_pf: theme swap failed: {e}");
        }
    });
    if let Some(status) = ui.by_name("Status")
        && let Some(text_entity) = ui.first_text_in(status)
        && let Ok(mut text) = texts.get_mut(text_entity)
    {
        text.0 = format!("theme: {}", theme.slug);
    }
}
