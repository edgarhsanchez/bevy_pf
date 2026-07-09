//! WPF-style navigation: a Frame, three Pages, the journal, and Hyperlinks.
//!
//! Pages are routes registered at startup; `<Hyperlink NavigateUri="...">`
//! inside a page navigates the enclosing Frame (web-style), the ◀ ▶ chrome
//! drives the journal, and a `PfNavigated` message fires on every hop —
//! this example uses it to wire up each page's buttons after it loads.
//! The counter lives in the frame's `DataContext`, so it survives page
//! re-creation exactly like WPF's `KeepAlive="False"` navigation.
//!
//! Run with: `cargo run -p bevy_pf --example navigation`

use bevy::prelude::*;
use bevy::reflect::Reflect;
use bevy_pf::prelude::*;

#[derive(Reflect, Default)]
struct Vm {
    count: u32,
}

#[derive(Resource)]
struct VmHandle(Bindable);

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(PfUiPlugin)
        .register_page(
            "home.xaml",
            xaml!(
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="Home">
                      <StackPanel Margin="24">
                        <TextBlock Text="Home" FontSize="28" FontWeight="Bold"/>
                        <TextBlock Text="This app is three XAML pages inside one Frame." Margin="0,6,0,0"/>
                        <StackPanel Orientation="Horizontal" Margin="0,4,0,0">
                          <TextBlock Text="Counter (lives in the Frame DataContext): "/>
                          <TextBlock Text="{Binding count}" FontWeight="Bold"/>
                        </StackPanel>
                        <Hyperlink NavigateUri="settings.xaml" Margin="0,16,0,0">Open Settings</Hyperlink>
                        <Hyperlink NavigateUri="about.xaml" Margin="0,4,0,0">About this demo</Hyperlink>
                      </StackPanel>
                    </Page>"##
            ),
        )
        .register_page(
            "settings.xaml",
            xaml!(
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="Settings">
                      <StackPanel Margin="24">
                        <TextBlock Text="Settings" FontSize="28" FontWeight="Bold"/>
                        <StackPanel Orientation="Horizontal" Margin="0,10,0,0">
                          <Button x:Name="Bump" Content="count += 1" Width="120"/>
                          <TextBlock Text="{Binding count}" FontWeight="Bold" Margin="10,4,0,0"/>
                        </StackPanel>
                        <TextBlock Text="Navigate away and back - the counter persists because state lives in the DataContext, not the page." Width="420" TextWrapping="Wrap" Margin="0,10,0,0"/>
                        <Hyperlink NavigateUri="home.xaml" Margin="0,16,0,0">Back home</Hyperlink>
                      </StackPanel>
                    </Page>"##
            ),
        )
        .register_page(
            "about.xaml",
            xaml!(
                r##"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="About">
                      <StackPanel Margin="24">
                        <TextBlock Text="About" FontSize="28" FontWeight="Bold"/>
                        <TextBlock Text="Frame + Page + journal + Hyperlink navigation, WPF semantics on Bevy." Margin="0,6,0,0"/>
                        <Hyperlink NavigateUri="https://github.com/edgarhsanchez/bevy_pf" Margin="0,10,0,0">Project on GitHub (external link)</Hyperlink>
                        <Hyperlink NavigateUri="home.xaml" Margin="0,4,0,0">Back home</Hyperlink>
                      </StackPanel>
                    </Page>"##
            ),
        )
        .add_systems(Startup, setup)
        .add_systems(Update, wire_new_pages)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let vm = Bindable::new(Vm::default());
    commands.spawn_xaml_bound(
        xaml!(
            r##"<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                        Title="bevy_pf navigation">
                  <DockPanel>
                    <StatusBar DockPanel.Dock="Bottom">
                      <StatusBarItem><TextBlock x:Name="Status" Text="page: home.xaml"/></StatusBarItem>
                    </StatusBar>
                    <Frame Source="home.xaml"/>
                  </DockPanel>
                </Window>"##
        ),
        vm.clone(),
    );
    commands.insert_resource(VmHandle(vm));
}

/// Pages re-instantiate on every visit; wire their buttons as they appear
/// and keep the status bar showing the current page title.
fn wire_new_pages(
    mut navigated: MessageReader<PfNavigated>,
    ui: PfQuery,
    mut texts: Query<&mut bevy::ui::widget::Text>,
    mut commands: Commands,
) {
    for nav in navigated.read() {
        if let Some(status) = ui.by_name("Status")
            && let Some(text_entity) = ui.first_text_in(status)
            && let Ok(mut text) = texts.get_mut(text_entity)
        {
            text.0 = format!(
                "page: {}{}",
                nav.source,
                nav.title.as_deref().map(|t| format!("  ({t})")).unwrap_or_default()
            );
        }
        // The settings page's button is new on every visit.
        if nav.source == "settings.xaml"
            && let Some(bump) = ui.by_name("Bump")
        {
            commands.entity(bump).observe(
                |_: On<Pointer<Click>>, vm: Res<VmHandle>| {
                    vm.0.update(|m: &mut Vm| m.count += 1);
                },
            );
        }
    }
}
