//! Every bevy_pf component in one app — and the element-query API that lets
//! any Bevy system find and change any of them.
//!
//! Run with: `cargo run -p bevy_pf --example components_showcase`
//!
//! The shell is a menu bar over a `TabControl` whose tabs cover the whole
//! control set (basics, inputs, lists, layout panels, shapes, styling).
//! A plain Bevy system (`live_updates`) demonstrates cross-system UI access:
//! every frame it locates elements via `PfQuery` — by `x:Name`, by `x:Uid`,
//! and by `AutomationProperties.AutomationId` — and rewrites a clock label,
//! advances a progress bar, and cycles a swatch color through the value
//! store's Local tier (so styles/triggers revert correctly beneath it).

use bevy::prelude::*;
use bevy::reflect::Reflect;
use bevy::ui::widget::Text;
use bevy_pf::prelude::*;

#[derive(Reflect, Default)]
struct FileRow {
    name: String,
    kind: String,
    size: u32,
}

#[derive(Reflect, Default)]
struct Profile {
    name: String,
    role: String,
}

#[derive(Reflect, Default)]
struct Vm {
    files: Vec<FileRow>,
    players: Vec<String>,
    count: f64,
    profile: Profile,
}

/// Menu-driven vsync state. The menu observer flips it; `apply_vsync`
/// reconfigures the window surface and relabels the menu item.
#[derive(Resource)]
struct Vsync {
    on: bool,
    dirty: bool,
}

fn primary_window() -> Window {
    #[allow(unused_mut)]
    let mut window = Window {
        title: "bevy_pf components showcase".to_string(),
        ..Default::default()
    };
    #[cfg(target_arch = "wasm32")]
    {
        window.canvas = Some("#bevy-canvas".to_string());
        window.fit_canvas_to_parent = true;
    }
    window
}

fn main() {
    App::new()
.add_plugins({
            let plugins = DefaultPlugins.set(WindowPlugin {
                primary_window: Some(primary_window()),
                ..Default::default()
            });
            // No demo plays audio; skipping the plugin on the web avoids the
            // browser's AudioContext autoplay warning.
            #[cfg(target_arch = "wasm32")]
            let plugins = plugins.disable::<bevy::audio::AudioPlugin>();
            plugins
        })
        .add_plugins(bevy::diagnostic::FrameTimeDiagnosticsPlugin::default())
        .add_plugins(PfUiPlugin)
        .insert_resource(Vsync { on: true, dirty: false })
        .register_page("nav-dashboard", xaml!(
            r#"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Title="Dashboard">
                 <StackPanel>
                   <TextBlock Text="Dashboard" FontWeight="Bold"/>
                   <TextBlock Text="This page was navigated to by the NavigationView pane." FontSize="12" Margin="0,4,0,0"/>
                 </StackPanel>
               </Page>"#
        ))
        .register_page("nav-reports", xaml!(
            r#"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Title="Reports">
                 <StackPanel>
                   <TextBlock Text="Reports" FontWeight="Bold"/>
                   <TextBlock Text="Each item's Tag routes the embedded Frame." FontSize="12" Margin="0,4,0,0"/>
                 </StackPanel>
               </Page>"#
        ))
        .register_page("nav-settings", xaml!(
            r#"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Title="Settings">
                 <StackPanel>
                   <TextBlock Text="Settings" FontWeight="Bold"/>
                   <TextBlock Text="Pages are plain registered XAML scenes." FontSize="12" Margin="0,4,0,0"/>
                 </StackPanel>
               </Page>"#
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, (wire_vsync_menu, apply_vsync, live_updates))
        .run();
}

/// One-shot: attach a click observer to the "VsyncToggle" menu item once the
/// scene has instantiated. The built-in menu behavior (close on leaf click)
/// still runs — observers stack.
fn wire_vsync_menu(mut wired: Local<bool>, ui: PfQuery, mut commands: Commands) {
    if *wired {
        return;
    }
    let Some(toggle) = ui.by_name("VsyncToggle") else {
        return;
    };
    commands
        .entity(toggle)
        .observe(|_click: On<Pointer<Click>>, mut vsync: ResMut<Vsync>| {
            vsync.on = !vsync.on;
            vsync.dirty = true;
        });
    if let Some(dialog_btn) = ui.by_name("DialogBtn") {
        commands.entity(dialog_btn).observe(
            |_: On<Pointer<Click>>, mut commands: Commands| {
                commands.queue(|world: &mut World| {
                    bevy_pf::dialog::show_content(
                        world,
                        "About this dialog",
                        &xaml!(
                            r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation">
                                 <TextBlock Text="ContentDialog hosts any XAML scene." FontSize="13"/>
                                 <StackPanel Orientation="Horizontal" Margin="0,8,0,0">
                                   <PackIcon Kind="Info" Width="18" Height="18" Foreground="#FF3366CC" Margin="0,0,8,0"/>
                                   <TextBlock Text="Buttons report via PfDialogResult." FontSize="12"/>
                                 </StackPanel>
                               </StackPanel>"##
                        ),
                        &["OK", "Cancel"],
                    );
                });
            },
        );
    }
    if let Some(toast_btn) = ui.by_name("ToastBtn") {
        commands
            .entity(toast_btn)
            .observe(|_: On<Pointer<Click>>, mut commands: Commands| {
                commands.queue(|world: &mut World| {
                    bevy_pf::toast::show_with(
                        world,
                        "Saved! (a toast from bevy_pf::toast)",
                        bevy_pf::toast::Severity::Success,
                        4.0,
                    );
                });
            });
    }
    *wired = true;
}

fn apply_vsync(
    mut vsync: ResMut<Vsync>,
    mut windows: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
    ui: PfQuery,
    mut texts: Query<&mut Text>,
) {
    if !vsync.dirty {
        return;
    }
    vsync.dirty = false;
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    // Changing Window.present_mode makes bevy reconfigure the surface live.
    // Note: current macOS ignores vsync-off for windowed apps (presents stay
    // locked to the display refresh); on Windows/Linux this uncaps the FPS.
    window.present_mode = if vsync.on {
        bevy::window::PresentMode::AutoVsync
    } else {
        bevy::window::PresentMode::AutoNoVsync
    };
    if let Some(item) = ui.by_name("VsyncToggle")
        && let Some(text_entity) = ui.first_text_in(item)
        && let Ok(mut text) = texts.get_mut(text_entity)
    {
        text.0 = if vsync.on { "Vsync: On" } else { "Vsync: Off" }.to_string();
    }
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let vm = Bindable::new(Vm {
        files: vec![
            FileRow {
                name: "main.rs".into(),
                kind: "Rust".into(),
                size: 1240,
            },
            FileRow {
                name: "app.xaml".into(),
                kind: "XAML".into(),
                size: 862,
            },
            FileRow {
                name: "theme.xaml".into(),
                kind: "XAML".into(),
                size: 311,
            },
        ],
        players: vec!["Ada".into(), "Bevy".into(), "Cleo".into(), "Дмитрий".into()],
        count: 0.0,
        profile: Profile {
            name: "Grace Hopper".into(),
            role: "Rear Admiral, compiler pioneer".into(),
        },
    });
    // RepeatButton demo: each Click (initial tap + repeats while held)
    // invokes this command.
    {
        let counter = vm.clone();
        vm.on_command("bump", move |_world, _param| {
            counter.update(|m: &mut Vm| m.count += 1.0);
        });
    }

    commands.spawn_xaml_bound(
        xaml!(
            r##"<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                        Title="bevy_pf components showcase">
                  <DockPanel>
                    <Menu DockPanel.Dock="Top">
                      <MenuItem Header="File">
                        <MenuItem Header="New"/>
                        <MenuItem Header="Open Recent">
                          <MenuItem Header="showcase.xaml"/>
                        </MenuItem>
                        <Separator/>
                        <MenuItem Header="Exit"/>
                      </MenuItem>
                      <MenuItem Header="View">
                        <MenuItem Header="Vsync: On" x:Name="VsyncToggle"/>
                      </MenuItem>
                      <MenuItem Header="Help">
                        <MenuItem Header="About bevy_pf"/>
                      </MenuItem>
                    </Menu>

                    <Border DockPanel.Dock="Bottom" Background="#FFECECEC" Padding="8,4,8,4">
                      <TextBlock x:Name="Fps" Text="fps: --" FontSize="12"/>
                    </Border>

                    <TabControl Margin="8">
                      <TabItem Header="Basics">
                        <StackPanel Margin="12">
                          <TextBlock Text="TextBlock: styles, wrapping, inlines" FontSize="18" FontWeight="Bold"/>
                          <Label Content="Label content"/>
                          <TextBlock x:Name="Clock" x:Uid="clock.uid" Text="clock: --" Margin="0,6,0,6"/>
                          <StackPanel Orientation="Horizontal">
                            <Button Content="Button" Width="110" Margin="0,0,8,0" ToolTip="A tooltip on the popup layer"/>
                            <ToggleButton Content="ToggleButton" Width="120" Margin="0,0,8,0"/>
                            <CheckBox Content="CheckBox" IsChecked="True" Margin="0,0,8,0"/>
                          </StackPanel>
                          <StackPanel Orientation="Horizontal" Margin="0,6,0,0">
                            <RadioButton GroupName="opt" Content="Radio A" IsChecked="True" Margin="0,0,8,0"/>
                            <RadioButton GroupName="opt" Content="Radio B"/>
                          </StackPanel>
                          <Separator Margin="0,8,0,8"/>
                          <Image Source="ui/bench.png" Width="48" Height="48" HorizontalAlignment="Left"/>
                        </StackPanel>
                      </TabItem>

                      <TabItem Header="Inputs">
                        <StackPanel Margin="12">
                          <TextBox Text="TextBox: type, select, paste" Width="280" HorizontalAlignment="Left"/>
                          <Slider Minimum="0" Maximum="100" Value="40" Width="280" Margin="0,10,0,0" HorizontalAlignment="Left"/>
                          <ProgressBar AutomationProperties.AutomationId="LiveProgress"
                                       Minimum="0" Maximum="100" Value="0" Width="280" Height="14"
                                       Margin="0,10,0,0" HorizontalAlignment="Left"/>
                          <ComboBox SelectedIndex="0" Width="200" Margin="0,10,0,0" HorizontalAlignment="Left">
                            <ComboBoxItem Content="ComboBox choice 1"/>
                            <ComboBoxItem Content="Choice 2"/>
                            <ComboBoxItem Content="Choice 3"/>
                          </ComboBox>
                          <StackPanel Orientation="Horizontal" Margin="0,10,0,0">
                            <RepeatButton Content="RepeatButton: hold me" Delay="400" Interval="80"
                                          Command="bump" Width="200"/>
                            <TextBlock Text="{Binding count, StringFormat='{}count: {0}'}"
                                       Margin="10,0,0,0" VerticalAlignment="Center"/>
                          </StackPanel>
                        </StackPanel>
                      </TabItem>

                      <TabItem Header="Lists + Data">
                        <Grid Margin="12" ColumnDefinitions="220, 12, 220, 12, *">
                          <StackPanel>
                            <TextBlock Text="ListBox (ItemsSource)" FontWeight="Bold"/>
                            <ListBox ItemsSource="{Binding players}"/>
                            <TextBlock Text="ItemsPanel + ItemContainerStyle" FontWeight="Bold" Margin="0,10,0,0"/>
                            <ListBox ItemsSource="{Binding players}">
                              <ListBox.ItemsPanel>
                                <ItemsPanelTemplate>
                                  <WrapPanel/>
                                </ItemsPanelTemplate>
                              </ListBox.ItemsPanel>
                              <ListBox.ItemContainerStyle>
                                <Style TargetType="ListBoxItem">
                                  <Setter Property="Padding" Value="8,4"/>
                                  <Setter Property="Margin" Value="2"/>
                                  <Setter Property="Foreground" Value="#FF7FB4E8"/>
                                </Style>
                              </ListBox.ItemContainerStyle>
                            </ListBox>
                          </StackPanel>
                          <StackPanel Grid.Column="2">
                            <TextBlock Text="TreeView" FontWeight="Bold"/>
                            <TreeView>
                              <TreeViewItem Header="workspace" IsExpanded="True">
                                <TreeViewItem Header="crates" IsExpanded="True">
                                  <TreeViewItem Header="bevy_pf"/>
                                  <TreeViewItem Header="bevy_pf_xaml"/>
                                </TreeViewItem>
                                <TreeViewItem Header="docs"/>
                              </TreeViewItem>
                            </TreeView>
                          </StackPanel>
                          <StackPanel Grid.Column="4">
                            <StackPanel.Resources>
                              <DataTemplate DataType="{x:Type Profile}">
                                <Border Background="#FF26303F" CornerRadius="6" Padding="10,6">
                                  <StackPanel>
                                    <TextBlock Text="{Binding name}" FontWeight="Bold" Foreground="#FFEDF2FA"/>
                                    <TextBlock Text="{Binding role}" FontSize="11" Foreground="#FF9FB4CE"/>
                                  </StackPanel>
                                </Border>
                              </DataTemplate>
                            </StackPanel.Resources>
                            <TextBlock Text="ContentControl (DataType template)" FontWeight="Bold"/>
                            <ContentControl Content="{Binding profile}" Margin="0,0,0,10"/>
                            <TextBlock Text="DataGrid" FontWeight="Bold"/>
                            <DataGrid ItemsSource="{Binding files}">
                              <DataGrid.Columns>
                                <DataGridTextColumn Header="Name" Binding="{Binding name}" Width="2*"/>
                                <DataGridTextColumn Header="Type" Binding="{Binding kind}" Width="*"/>
                                <DataGridTextColumn Header="Bytes" Binding="{Binding size}" Width="*"/>
                              </DataGrid.Columns>
                            </DataGrid>
                          </StackPanel>
                        </Grid>
                      </TabItem>

                      <TabItem Header="Layout">
                        <ScrollViewer Margin="12">
                          <StackPanel>
                            <GroupBox Header="GroupBox">
                              <WrapPanel Width="360">
                                <Border Width="52" Height="30" Margin="2" Background="#FF44AA88"/>
                                <Border Width="52" Height="30" Margin="2" Background="#FF55BB99"/>
                                <Border Width="52" Height="30" Margin="2" Background="#FF66CCAA"/>
                                <Border Width="52" Height="30" Margin="2" Background="#FF77DDBB"/>
                                <Border Width="52" Height="30" Margin="2" Background="#FF88EECC"/>
                              </WrapPanel>
                            </GroupBox>
                            <Expander Header="Expander (UniformGrid inside)" IsExpanded="True" Margin="0,8,0,0">
                              <UniformGrid Rows="2" Columns="4" Width="320" Height="90">
                                <Border Margin="2" Background="#FF6688AA"/>
                                <Border Margin="2" Background="#FF7799BB"/>
                                <Border Margin="2" Background="#FF88AACC"/>
                                <Border Margin="2" Background="#FF99BBDD"/>
                                <Border Margin="2" Background="#FFAACCEE"/>
                                <Border Margin="2" Background="#FF6688AA"/>
                                <Border Margin="2" Background="#FF7799BB"/>
                                <Border Margin="2" Background="#FF88AACC"/>
                              </UniformGrid>
                            </Expander>
                            <Canvas Width="360" Height="90" Margin="0,8,0,0">
                              <Border Canvas.Left="10" Canvas.Top="10" Width="80" Height="34" Background="#FFAA6688"/>
                              <Border Canvas.Left="120" Canvas.Top="40" Width="80" Height="34" Background="#FF88AA66"/>
                            </Canvas>
                            <Viewbox Width="360" Height="60">
                              <TextBlock Text="Viewbox scales me"/>
                            </Viewbox>
                          </StackPanel>
                        </ScrollViewer>
                      </TabItem>

                      <TabItem Header="Shapes">
                        <Canvas Margin="12" Width="520" Height="260">
                          <Rectangle Canvas.Left="10" Canvas.Top="10" Width="110" Height="70" Fill="#FF3366AA" Stroke="#FF112233" StrokeThickness="2"/>
                          <Ellipse Canvas.Left="140" Canvas.Top="10" Width="90" Height="70" Fill="#FFAA6633"/>
                          <Line X1="10" Y1="110" X2="230" Y2="140" Stroke="#FF333333" StrokeThickness="3"/>
                          <Polygon Points="270,20 330,60 310,110 250,95" Fill="#FF66AA66" Stroke="#FF224422" StrokeThickness="2"/>
                          <Path Canvas.Left="350" Canvas.Top="15" Width="150" Height="150" Stretch="Fill"
                                Fill="#FF88AA33" Stroke="#FF224400" StrokeThickness="2"
                                Data="M 10,100 C 10,50 90,50 90,100 S 170,150 170,100 L 170,40 A 30,30 0 1 1 110,40 Z"/>
                        </Canvas>
                      </TabItem>

                      <TabItem Header="Animation">
                        <StackPanel Margin="12">
                          <TextBlock Text="Storyboards, visual states, and behaviors" FontWeight="Bold" Margin="0,0,0,10"/>

                          <TextBlock Text="Loaded storyboard: this card fades in (DoubleAnimation on Opacity, CubicEase)." FontSize="12"/>
                          <Border Background="#FF2B3342" CornerRadius="8" Padding="14" Width="360" HorizontalAlignment="Left" Margin="0,4,0,0">
                            <Border.Triggers>
                              <EventTrigger RoutedEvent="Loaded">
                                <BeginStoryboard>
                                  <Storyboard>
                                    <DoubleAnimation Storyboard.TargetProperty="Opacity" From="0" To="1" Duration="0:0:1.2">
                                      <DoubleAnimation.EasingFunction>
                                        <CubicEase EasingMode="EaseOut"/>
                                      </DoubleAnimation.EasingFunction>
                                    </DoubleAnimation>
                                  </Storyboard>
                                </BeginStoryboard>
                              </EventTrigger>
                            </Border.Triggers>
                            <TextBlock Text="faded in on load" Foreground="#FFDCE6F5"/>
                          </Border>

                          <TextBlock Text="Forever pulse: AutoReverse + RepeatBehavior=Forever." FontSize="12" Margin="0,14,0,0"/>
                          <Border Background="#FF35E0E8" CornerRadius="4" Width="220" Height="10" HorizontalAlignment="Left" Margin="0,4,0,0">
                            <Border.Triggers>
                              <EventTrigger RoutedEvent="Loaded">
                                <BeginStoryboard>
                                  <Storyboard>
                                    <DoubleAnimation Storyboard.TargetProperty="Opacity" From="1" To="0.15"
                                                     Duration="0:0:0.8" AutoReverse="True" RepeatBehavior="Forever">
                                      <DoubleAnimation.EasingFunction>
                                        <SineEase EasingMode="EaseInOut"/>
                                      </DoubleAnimation.EasingFunction>
                                    </DoubleAnimation>
                                  </Storyboard>
                                </BeginStoryboard>
                              </EventTrigger>
                            </Border.Triggers>
                          </Border>

                          <TextBlock Text="VisualStateManager: hover and press this templated button." FontSize="12" Margin="0,14,0,0"/>
                          <Button Content="stateful button" Width="180" Margin="0,4,0,0" HorizontalAlignment="Left">
                            <Button.Template>
                              <ControlTemplate TargetType="Button">
                                <Border x:Name="chrome" Background="#FF3C4658" CornerRadius="8" Padding="10,6">
                                  <VisualStateManager.VisualStateGroups>
                                    <VisualStateGroup x:Name="CommonStates">
                                      <VisualState x:Name="Normal"/>
                                      <VisualState x:Name="MouseOver">
                                        <Storyboard>
                                          <ColorAnimation Storyboard.TargetName="chrome" Storyboard.TargetProperty="Background"
                                                          To="#FF5B8DEF" Duration="0:0:0.15"/>
                                        </Storyboard>
                                      </VisualState>
                                      <VisualState x:Name="Pressed">
                                        <Storyboard>
                                          <ColorAnimation Storyboard.TargetName="chrome" Storyboard.TargetProperty="Background"
                                                          To="#FF2C628B" Duration="0:0:0.05"/>
                                        </Storyboard>
                                      </VisualState>
                                    </VisualStateGroup>
                                  </VisualStateManager.VisualStateGroups>
                                  <ContentPresenter/>
                                </Border>
                              </ControlTemplate>
                            </Button.Template>
                          </Button>

                          <TextBlock Text="Behaviors: clicking runs ChangePropertyAction + a keyed storyboard." FontSize="12" Margin="0,14,0,0"/>
                          <StackPanel.Resources>
                            <Storyboard x:Key="Nudge">
                              <DoubleAnimation Storyboard.TargetName="BehaviorBar" Storyboard.TargetProperty="Width"
                                               From="40" To="260" Duration="0:0:0.6" FillBehavior="HoldEnd">
                                <DoubleAnimation.EasingFunction>
                                  <BackEase EasingMode="EaseOut"/>
                                </DoubleAnimation.EasingFunction>
                              </DoubleAnimation>
                            </Storyboard>
                          </StackPanel.Resources>
                          <Border x:Name="BehaviorCard" Background="#FF303030" CornerRadius="6" Padding="10" Width="300" HorizontalAlignment="Left" Margin="0,4,0,0">
                            <Interaction.Triggers>
                              <EventTrigger EventName="Click">
                                <ChangePropertyAction TargetName="BehaviorCard" PropertyName="Background" Value="#FF224422"/>
                                <ControlStoryboardAction Storyboard="{StaticResource Nudge}"/>
                              </EventTrigger>
                            </Interaction.Triggers>
                            <StackPanel>
                              <TextBlock Text="click me" Foreground="#FFB5E61D"/>
                              <Border x:Name="BehaviorBar" Background="#FFB5E61D" Height="6" Width="40" CornerRadius="3" HorizontalAlignment="Left" Margin="0,6,0,0"/>
                            </StackPanel>
                          </Border>
                        </StackPanel>
                      </TabItem>

                      <TabItem Header="Toolkit">
                        <StackPanel Margin="12">
                          <TextBlock Text="Ecosystem controls (MahApps / MaterialDesign / Extended Toolkit / HandyControl equivalents)" FontWeight="Bold" Margin="0,0,0,10"/>
                          <StackPanel Orientation="Horizontal">
                            <ToggleSwitch Content="Wi-Fi" IsOn="True" Margin="0,0,16,0"/>
                            <ToggleSwitch Content="Bluetooth" Margin="0,0,16,0"/>
                            <NumericUpDown Value="5" Minimum="0" Maximum="10" Margin="0,0,16,0"/>
                            <RatingBar Value="3"/>
                          </StackPanel>
                          <StackPanel Orientation="Horizontal" Margin="0,14,0,0">
                            <Badge Badge="12"><Button Content="Inbox" Width="90"/></Badge>
                            <Badge Badge="3" Margin="18,0,0,0"><Button Content="Alerts" Width="90"/></Badge>
                            <Chip Margin="18,0,0,0"><TextBlock Text="bevy" FontSize="12"/></Chip>
                            <Chip Margin="6,0,0,0"><TextBlock Text="xaml" FontSize="12"/></Chip>
                            <Chip Margin="6,0,0,0"><TextBlock Text="rust" FontSize="12"/></Chip>
                          </StackPanel>
                          <Card Width="300" Margin="0,16,0,0" HorizontalAlignment="Left">
                            <StackPanel>
                              <TextBlock Text="Card" FontWeight="Bold"/>
                              <TextBlock Text="An elevated surface: radius, shadow, padding." FontSize="12" Margin="0,4,0,0"/>
                            </StackPanel>
                          </Card>
                          <StackPanel Orientation="Horizontal" Margin="0,16,0,0">
                            <TextBlock Text="RangeSlider:" VerticalAlignment="Center" Margin="0,0,10,0"/>
                            <RangeSlider Width="220" Minimum="0" Maximum="100" LowerValue="25" UpperValue="70"/>
                            <Button x:Name="ToastBtn" Content="Show toast" Width="110" Margin="16,0,0,0"/>
                          </StackPanel>
                          <StackPanel Orientation="Horizontal" Margin="0,16,0,0">
                            <TextBlock Text="TimePicker:" VerticalAlignment="Center" Margin="0,0,10,0"/>
                            <TimePicker SelectedTime="09:30" Margin="0,0,16,0"/>
                            <TextBlock Text="ColorPicker:" VerticalAlignment="Center" Margin="0,0,10,0"/>
                            <ColorPicker SelectedColor="#3366CC" Margin="0,0,16,0"/>
                            <Button x:Name="DialogBtn" Content="ContentDialog" Width="120"/>
                          </StackPanel>
                          <StackPanel Orientation="Horizontal" Margin="0,16,0,0">
                            <TextBlock Text="AutoSuggestBox:" VerticalAlignment="Center" Margin="0,0,10,0"/>
                            <AutoSuggestBox Width="200" Suggestions="Amsterdam,Athens,Berlin,Bern,Bogota,Boston,Brasilia,Brussels,Budapest,Buenos Aires,Cairo,Canberra,Caracas,Copenhagen"/>
                            <TextBlock Text="(type a city: a, b, c...)" FontSize="11" Foreground="#FF8A8A8A" VerticalAlignment="Center" Margin="10,0,0,0"/>
                          </StackPanel>
                          <StackPanel Orientation="Horizontal" Margin="0,16,0,0">
                            <TextBlock Text="PackIcon:" VerticalAlignment="Center" Margin="0,0,10,0"/>
                            <PackIcon Kind="Home" Width="20" Height="20" Margin="0,0,8,0"/>
                            <PackIcon Kind="Settings" Width="20" Height="20" Margin="0,0,8,0"/>
                            <PackIcon Kind="Search" Width="20" Height="20" Margin="0,0,8,0"/>
                            <PackIcon Kind="Check" Width="20" Height="20" Foreground="#FF33A033" Margin="0,0,8,0"/>
                            <PackIcon Kind="Close" Width="20" Height="20" Foreground="#FFC03030" Margin="0,0,8,0"/>
                            <PackIcon Kind="Star" Width="20" Height="20" Foreground="#FFE8B400" Margin="0,0,8,0"/>
                            <PackIcon Kind="Heart" Width="20" Height="20" Foreground="#FFD6336C" Margin="0,0,8,0"/>
                            <PackIcon Kind="Warning" Width="20" Height="20" Foreground="#FFCC8800" Margin="0,0,8,0"/>
                            <PackIcon Kind="Info" Width="20" Height="20" Foreground="#FF3366CC" Margin="0,0,8,0"/>
                            <PackIcon Kind="Bell" Width="20" Height="20" Margin="0,0,8,0"/>
                            <PackIcon Kind="Folder" Width="20" Height="20" Margin="0,0,8,0"/>
                            <PackIcon Kind="Mail" Width="20" Height="20" Margin="0,0,8,0"/>
                            <PackIcon Kind="Lock" Width="20" Height="20" Margin="0,0,8,0"/>
                            <PackIcon Kind="Person" Width="20" Height="20" Margin="0,0,8,0"/>
                            <PackIcon Kind="Clock" Width="20" Height="20" Margin="0,0,8,0"/>
                            <PackIcon Kind="Refresh" Width="20" Height="20"/>
                          </StackPanel>
                          <TextBlock Text="NavigationView (items drive the embedded Frame):" FontWeight="Bold" Margin="0,16,0,6"/>
                          <Border BorderBrush="#FFC8C8C8" BorderThickness="1" Width="520" Height="150" HorizontalAlignment="Left">
                            <NavigationView>
                              <NavigationViewItem Content="Dashboard" Icon="Home" Tag="nav-dashboard"/>
                              <NavigationViewItem Content="Reports" Icon="Document" Tag="nav-reports"/>
                              <NavigationViewItem Content="Settings" Icon="Settings" Tag="nav-settings"/>
                            </NavigationView>
                          </Border>
                          <BusyIndicator IsBusy="True" BusyContent="Loading..." Width="300" Height="70" Margin="0,16,0,0" HorizontalAlignment="Left">
                            <Border Background="#FFEDEFF4" Width="300" Height="70">
                              <TextBlock Text="content dimmed by the busy overlay" HorizontalAlignment="Center" VerticalAlignment="Center"/>
                            </Border>
                          </BusyIndicator>
                        </StackPanel>
                      </TabItem>

                      <TabItem Header="Styling">
                        <StackPanel Margin="12">
                          <StackPanel.Resources>
                            <SolidColorBrush x:Key="SwatchBrush" Color="#FF2288CC"/>
                            <Style x:Key="HotButton" TargetType="Button">
                              <Setter Property="Background" Value="#FF4477AA"/>
                              <Setter Property="Foreground" Value="White"/>
                              <Style.Triggers>
                                <Trigger Property="IsMouseOver" Value="True">
                                  <Setter Property="Background" Value="#FF66AACC"/>
                                </Trigger>
                              </Style.Triggers>
                            </Style>
                          </StackPanel.Resources>
                          <Button Style="{StaticResource HotButton}" Content="Hover me (Style.Triggers)" Width="220" HorizontalAlignment="Left"/>
                          <Border x:Name="Swatch" Background="{DynamicResource SwatchBrush}"
                                  Width="220" Height="60" CornerRadius="6" Margin="0,10,0,0" HorizontalAlignment="Left"/>
                          <TextBlock Text="The swatch above is recolored every second by a plain Bevy system through the Local value tier." Width="360" TextWrapping="Wrap" Margin="0,8,0,0"/>
                        </StackPanel>
                      </TabItem>
                    </TabControl>
                  </DockPanel>
                </Window>"##
        ),
        vm,
    );
}

/// Cross-system UI updates: no handles were saved at spawn time — every
/// element is found fresh through `PfQuery` (by name, uid, or automation id).
fn live_updates(
    time: Res<Time>,
    diagnostics: Res<bevy::diagnostic::DiagnosticsStore>,
    ui: PfQuery,
    mut texts: Query<&mut Text>,
    mut bars: Query<&mut bevy_pf::components::PfProgress>,
    mut commands: Commands,
) {
    let secs = time.elapsed_secs();

    // 0. FPS status bar (bottom). Windowed macOS presents are locked to the
    //    display refresh, so this reads ~60 by design — the bench example
    //    measures uncapped throughput offscreen.
    if let Some(fps) = diagnostics
        .get(&bevy::diagnostic::FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        && let Some(label) = ui.by_name("Fps")
        && let Some(text_entity) = ui.first_text_in(label)
        && let Ok(mut text) = texts.get_mut(text_entity)
    {
        let next = format!(
            "fps: {fps:.0}  (vsync-locked at the display refresh; uncapped numbers: docs/performance.md)"
        );
        if text.0 != next {
            text.0 = next;
        }
    }

    // 1. x:Uid lookup -> rewrite the clock TextBlock's text child.
    if let Some(clock) = ui.by_uid("clock.uid")
        && let Some(text_entity) = ui.first_text_in(clock)
        && let Ok(mut text) = texts.get_mut(text_entity)
    {
        let next = format!("clock: {secs:.1}s (updated via PfQuery::by_uid)");
        if text.0 != next {
            text.0 = next;
        }
    }

    // 2. AutomationId lookup -> advance the ProgressBar. Its `Changed`-driven
    //    sync system resizes the fill; mutating the component is all it takes.
    if let Some(bar) = ui.by_automation_id("LiveProgress")
        && let Ok(mut progress) = bars.get_mut(bar)
    {
        progress.value = (secs * 10.0) % 100.0;
    }

    // 3. x:Name lookup -> recolor the swatch once a second through the value
    //    store's Local tier (same precedence as a XAML attribute), so the
    //    DynamicResource beneath it reverts correctly if cleared.
    if let Some(swatch) = ui.by_name("Swatch") {
        let hue = (secs as u32 % 6) as f32 / 6.0 * 360.0;
        let srgb = Color::hsl(hue, 0.6, 0.5).to_srgba();
        let color = bevy_pf::xaml_ast::value::PfColor {
            r: (srgb.red * 255.0) as u8,
            g: (srgb.green * 255.0) as u8,
            b: (srgb.blue * 255.0) as u8,
            a: 255,
        };
        commands.queue(move |world: &mut World| {
            bevy_pf::provider::set_local(
                world,
                swatch,
                bevy_pf::PropertyTarget::Background,
                bevy_pf::resources::PfValue::Brush(bevy_pf::xaml_ast::value::PfBrush::Solid(color)),
            );
        });
    }
}
