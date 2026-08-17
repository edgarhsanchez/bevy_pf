//! An app-shell layout using the last wave of controls: a Menu bar with
//! submenus, a TabControl, a TreeView, a DataGrid bound to a view-model, and
//! a right-click ContextMenu.
//!
//! Run with: `cargo run -p bevy_pf --example app_shell`

use bevy::prelude::*;
use bevy::reflect::Reflect;
use bevy_pf::prelude::*;

#[derive(Reflect, Default)]
struct FileRow {
    name: String,
    size: u32,
    kind: String,
}

#[derive(Reflect, Default)]
struct Vm {
    files: Vec<FileRow>,
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(PfUiPlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let vm = Bindable::new(Vm {
        files: vec![
            FileRow {
                name: "main.rs".into(),
                size: 1240,
                kind: "Rust".into(),
            },
            FileRow {
                name: "app.xaml".into(),
                size: 862,
                kind: "XAML".into(),
            },
            FileRow {
                name: "theme.xaml".into(),
                size: 311,
                kind: "XAML".into(),
            },
            FileRow {
                name: "README.md".into(),
                size: 5720,
                kind: "Markdown".into(),
            },
        ],
    });

    commands.spawn_xaml_bound(
        xaml!(
            r##"<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                        Title="bevy_pf app shell">
                  <DockPanel>
                    <Menu DockPanel.Dock="Top">
                      <MenuItem Header="File">
                        <MenuItem Header="New" x:Name="FileNew"/>
                        <MenuItem Header="Open Recent">
                          <MenuItem Header="project_a"/>
                          <MenuItem Header="project_b"/>
                        </MenuItem>
                        <Separator/>
                        <MenuItem Header="Exit" x:Name="FileExit"/>
                      </MenuItem>
                      <MenuItem Header="Edit">
                        <MenuItem Header="Copy"/>
                        <MenuItem Header="Paste"/>
                      </MenuItem>
                      <MenuItem Header="Help">
                        <MenuItem Header="About bevy_pf"/>
                      </MenuItem>
                    </Menu>

                    <Grid Margin="8" ColumnDefinitions="240, 8, *">
                      <TreeView ToolTip="Project explorer (right-click me)">
                        <TreeView.ContextMenu>
                          <ContextMenu>
                            <MenuItem Header="Refresh"/>
                            <MenuItem Header="Collapse All"/>
                          </ContextMenu>
                        </TreeView.ContextMenu>
                        <TreeViewItem Header="bevy_pf" IsExpanded="True">
                          <TreeViewItem Header="crates" IsExpanded="True">
                            <TreeViewItem Header="bevy_pf"/>
                            <TreeViewItem Header="bevy_pf_xaml"/>
                            <TreeViewItem Header="bevy_pf_macros"/>
                          </TreeViewItem>
                          <TreeViewItem Header="docs">
                            <TreeViewItem Header="conformance notes"/>
                          </TreeViewItem>
                          <TreeViewItem Header="README.md"/>
                        </TreeViewItem>
                      </TreeView>

                      <TabControl Grid.Column="2">
                        <TabItem Header="Files">
                          <DataGrid ItemsSource="{Binding files}">
                            <DataGrid.Columns>
                              <DataGridTextColumn Header="Name" Binding="{Binding name}" Width="2*"/>
                              <DataGridTextColumn Header="Type" Binding="{Binding kind}" Width="*"/>
                              <DataGridTextColumn Header="Bytes" Binding="{Binding size}" Width="*"/>
                            </DataGrid.Columns>
                          </DataGrid>
                        </TabItem>
                        <TabItem Header="Preview">
                          <StackPanel Spacing="8">
                            <TextBlock Text="Second tab content" FontSize="18"/>
                            <ProgressBar Minimum="0" Maximum="100" Value="60"/>
                          </StackPanel>
                        </TabItem>
                        <TabItem Header="Settings">
                          <StackPanel Spacing="6">
                            <CheckBox Content="Auto-save" IsChecked="True"/>
                            <CheckBox Content="Word wrap"/>
                          </StackPanel>
                        </TabItem>
                      </TabControl>
                    </Grid>
                  </DockPanel>
                </Window>"##
        ),
        vm,
    );
}
