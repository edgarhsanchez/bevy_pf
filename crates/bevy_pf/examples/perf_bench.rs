//! Per-control FPS benchmark harness.
//!
//! One scene per control/panel/feature, selected via env var:
//!
//! ```sh
//! BENCH_SCENE=button cargo run -p bevy_pf --example perf_bench --release
//! ```
//!
//! Runs a 1280x720 window with vsync off (`PresentMode::AutoNoVsync`),
//! warms up, samples raw frame deltas for a fixed wall-clock window, prints a
//! machine-readable `BENCH_RESULT` line, and exits. Other modes:
//!
//! - `BENCH_LIST=1` — print every scene name and exit.
//! - `BENCH_DUMP_DIR=<dir>` — write each scene's XAML to `<dir>/<name>.xaml`
//!   so the *identical markup* can be fed to other XAML runtimes
//!   (e.g. NoesisGUI's XamlPlayer) for apples-to-apples comparison.
//! - `BENCH_WARMUP_SECS` / `BENCH_MEASURE_SECS` — override 2.0 / 5.0.
//!
//! Add `--features bevy/trace_tracy` to stream the run to a Tracy capture
//! for per-system frame breakdowns.

use bevy::prelude::*;
use bevy::reflect::Reflect;
use bevy::camera::RenderTarget;
use bevy::render::render_resource::{
    Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
};
use bevy::ui::UiTargetCamera;
use bevy::window::{ExitCondition, PresentMode, WindowResolution};
use bevy::winit::{UpdateMode, WinitSettings};
use bevy_pf::prelude::*;

const NS: &str = r##"xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml""##;

#[derive(Reflect, Default)]
struct Row {
    name: String,
    kind: String,
    score: u32,
}

#[derive(Reflect, Default)]
struct BenchVm {
    rows: Vec<Row>,
}

fn bench_vm() -> Bindable {
    Bindable::new(BenchVm {
        rows: (0..20)
            .map(|i| Row {
                name: format!("item {i}"),
                kind: if i % 2 == 0 { "even" } else { "odd" }.to_string(),
                score: i * 7,
            })
            .collect(),
    })
}

fn needs_vm(name: &str) -> bool {
    matches!(
        name,
        "listbox_items" | "itemscontrol_items" | "datagrid" | "composite_app_shell"
    )
}

fn scene_names() -> Vec<&'static str> {
    vec![
        "empty",
        "bevy_ui_raw",
        "textblock",
        "label",
        "button",
        "togglebutton",
        "checkbox",
        "radiobutton",
        "textbox",
        "slider",
        "progressbar",
        "separator",
        "image",
        "border",
        "groupbox",
        "expander",
        "scrollviewer",
        "viewbox",
        "tooltip",
        "stackpanel",
        "grid",
        "wrappanel",
        "dockpanel",
        "canvas",
        "uniformgrid",
        "listbox",
        "listbox_items",
        "itemscontrol_items",
        "combobox",
        "combobox_open",
        "tabcontrol",
        "treeview",
        "menu",
        "menu_open",
        "contextmenu",
        "contextmenu_open",
        "datagrid",
        "shapes_basic",
        "shapes_path",
        "styles_triggers",
        "dynamicresource",
        "composite_app_shell",
    ]
}

/// The XAML for a scene, or `None` for the two code-built baselines.
fn scene_xaml(name: &str) -> Option<String> {
    let x = match name {
        "textblock" => format!(
            r##"<TextBlock {NS} Text="The quick brown fox jumps over the lazy dog" FontSize="16"/>"##
        ),
        "label" => format!(r##"<Label {NS} Content="A label"/>"##),
        "button" => format!(r##"<Button {NS} Content="Click me" Width="140" Height="36"/>"##),
        "togglebutton" => format!(r##"<ToggleButton {NS} Content="Toggle" Width="140"/>"##),
        "checkbox" => format!(r##"<CheckBox {NS} Content="Enable feature" IsChecked="True"/>"##),
        "radiobutton" => format!(
            r##"<StackPanel {NS}>
                 <RadioButton GroupName="g" Content="Option A" IsChecked="True"/>
                 <RadioButton GroupName="g" Content="Option B"/>
               </StackPanel>"##
        ),
        "textbox" => format!(r##"<TextBox {NS} Text="editable text" Width="220"/>"##),
        "slider" => {
            format!(r##"<Slider {NS} Minimum="0" Maximum="100" Value="42" Width="220"/>"##)
        }
        "progressbar" => format!(
            r##"<ProgressBar {NS} Minimum="0" Maximum="100" Value="66" Width="220" Height="16"/>"##
        ),
        "separator" => format!(
            r##"<StackPanel {NS} Width="200">
                 <TextBlock Text="above"/>
                 <Separator/>
                 <TextBlock Text="below"/>
               </StackPanel>"##
        ),
        "image" => format!(r##"<Image {NS} Source="ui/bench.png" Width="64" Height="64"/>"##),
        "border" => format!(
            r##"<Border {NS} Width="200" Height="120" Background="#FF3366AA" BorderBrush="#FF222222" BorderThickness="2" CornerRadius="8"/>"##
        ),
        "groupbox" => format!(
            r##"<GroupBox {NS} Header="Group"><TextBlock Text="grouped content"/></GroupBox>"##
        ),
        "expander" => format!(
            r##"<Expander {NS} Header="More" IsExpanded="True"><TextBlock Text="expanded content"/></Expander>"##
        ),
        "scrollviewer" => {
            let items: String = (0..20)
                .map(|i| format!(r##"<TextBlock Text="scrolling line {i}"/>"##))
                .collect();
            format!(
                r##"<ScrollViewer {NS} Width="220" Height="150"><StackPanel>{items}</StackPanel></ScrollViewer>"##
            )
        }
        "viewbox" => format!(
            r##"<Viewbox {NS} Width="300" Height="200"><TextBlock Text="Scaled text"/></Viewbox>"##
        ),
        "tooltip" => format!(
            r##"<Button {NS} Content="Hover me" ToolTip="A helpful tip" Width="140"/>"##
        ),
        "stackpanel" => {
            let items: String = (0..10)
                .map(|i| format!(r##"<TextBlock Text="stacked line {i}"/>"##))
                .collect();
            format!(r##"<StackPanel {NS} Width="220">{items}</StackPanel>"##)
        }
        "grid" => {
            let cells: String = (0..16)
                .map(|i| {
                    format!(
                        r##"<Border Grid.Row="{}" Grid.Column="{}" Background="#FF88{:02X}44" Margin="2"><TextBlock Text="r{}c{}"/></Border>"##,
                        i / 4,
                        i % 4,
                        60 + i * 10,
                        i / 4,
                        i % 4
                    )
                })
                .collect();
            format!(
                r##"<Grid {NS} RowDefinitions="Auto,Auto,Auto,Auto" ColumnDefinitions="*,*,*,*" Width="480">{cells}</Grid>"##
            )
        }
        "wrappanel" => {
            let items: String = (0..20)
                .map(|i| {
                    format!(
                        r##"<Border Width="46" Height="26" Margin="2" Background="#FF44{:02X}88"/>"##,
                        50 + i * 10
                    )
                })
                .collect();
            format!(r##"<WrapPanel {NS} Width="300">{items}</WrapPanel>"##)
        }
        "dockpanel" => format!(
            r##"<DockPanel {NS} Width="400" Height="300">
                 <Border DockPanel.Dock="Top" Height="40" Background="#FF334455"/>
                 <Border DockPanel.Dock="Left" Width="100" Background="#FF445566"/>
                 <Border Background="#FF556677"><TextBlock Text="fill"/></Border>
               </DockPanel>"##
        ),
        "canvas" => {
            let items: String = (0..6)
                .map(|i| {
                    format!(
                        r##"<Border Canvas.Left="{}" Canvas.Top="{}" Width="60" Height="40" Background="#FF{:02X}6688"/>"##,
                        20 + i * 50,
                        20 + (i % 3) * 60,
                        80 + i * 20
                    )
                })
                .collect();
            format!(r##"<Canvas {NS} Width="400" Height="250">{items}</Canvas>"##)
        }
        "uniformgrid" => {
            let items: String = (0..16)
                .map(|i| {
                    format!(r##"<Border Margin="2" Background="#FF66{:02X}66"/>"##, 40 + i * 12)
                })
                .collect();
            format!(
                r##"<UniformGrid {NS} Rows="4" Columns="4" Width="320" Height="320">{items}</UniformGrid>"##
            )
        }
        "listbox" => {
            let items: String = (0..20)
                .map(|i| format!(r##"<ListBoxItem Content="static item {i}"/>"##))
                .collect();
            format!(r##"<ListBox {NS} Width="220">{items}</ListBox>"##)
        }
        "listbox_items" => format!(
            r##"<ListBox {NS} Width="240" ItemsSource="{{Binding rows}}">
                 <ListBox.ItemTemplate>
                   <DataTemplate>
                     <StackPanel Orientation="Horizontal">
                       <TextBlock Text="{{Binding name}}" Margin="0,0,8,0"/>
                       <TextBlock Text="{{Binding score}}"/>
                     </StackPanel>
                   </DataTemplate>
                 </ListBox.ItemTemplate>
               </ListBox>"##
        ),
        "itemscontrol_items" => format!(
            r##"<ItemsControl {NS} Width="240" ItemsSource="{{Binding rows}}">
                 <ItemsControl.ItemTemplate>
                   <DataTemplate>
                     <TextBlock Text="{{Binding name}}"/>
                   </DataTemplate>
                 </ItemsControl.ItemTemplate>
               </ItemsControl>"##
        ),
        "combobox" | "combobox_open" => {
            let items: String = (0..8)
                .map(|i| format!(r##"<ComboBoxItem Content="choice {i}"/>"##))
                .collect();
            format!(r##"<ComboBox {NS} Width="180" SelectedIndex="0">{items}</ComboBox>"##)
        }
        "tabcontrol" => format!(
            r##"<TabControl {NS} Width="400" Height="260" SelectedIndex="0">
                 <TabItem Header="General"><TextBlock Text="general content"/></TabItem>
                 <TabItem Header="Advanced"><TextBlock Text="advanced content"/></TabItem>
                 <TabItem Header="About"><TextBlock Text="about content"/></TabItem>
               </TabControl>"##
        ),
        "treeview" => format!(
            r##"<TreeView {NS} Width="240">
                 <TreeViewItem Header="Root" IsExpanded="True">
                   <TreeViewItem Header="Child A" IsExpanded="True">
                     <TreeViewItem Header="Grandchild 1"/>
                     <TreeViewItem Header="Grandchild 2"/>
                   </TreeViewItem>
                   <TreeViewItem Header="Child B"/>
                   <TreeViewItem Header="Child C"/>
                 </TreeViewItem>
               </TreeView>"##
        ),
        "menu" | "menu_open" => format!(
            r##"<Menu {NS}>
                 <MenuItem Header="File" x:Name="File">
                   <MenuItem Header="New"/>
                   <MenuItem Header="Open Recent">
                     <MenuItem Header="a.txt"/>
                     <MenuItem Header="b.txt"/>
                   </MenuItem>
                   <Separator/>
                   <MenuItem Header="Exit"/>
                 </MenuItem>
                 <MenuItem Header="Edit">
                   <MenuItem Header="Copy"/>
                   <MenuItem Header="Paste"/>
                 </MenuItem>
                 <MenuItem Header="Help">
                   <MenuItem Header="About"/>
                 </MenuItem>
               </Menu>"##
        ),
        "contextmenu" | "contextmenu_open" => format!(
            r##"<Border {NS} Width="300" Height="200" Background="#FF556688">
                 <Border.ContextMenu>
                   <ContextMenu>
                     <MenuItem Header="Copy"/>
                     <MenuItem Header="Paste"/>
                     <MenuItem Header="Delete"/>
                   </ContextMenu>
                 </Border.ContextMenu>
               </Border>"##
        ),
        "datagrid" => format!(
            r##"<DataGrid {NS} Width="420" ItemsSource="{{Binding rows}}">
                 <DataGrid.Columns>
                   <DataGridTextColumn Header="Name" Binding="{{Binding name}}" Width="2*"/>
                   <DataGridTextColumn Header="Type" Binding="{{Binding kind}}" Width="*"/>
                   <DataGridTextColumn Header="Score" Binding="{{Binding score}}" Width="*"/>
                 </DataGrid.Columns>
               </DataGrid>"##
        ),
        "shapes_basic" => format!(
            r##"<Canvas {NS} Width="400" Height="300">
                 <Rectangle Canvas.Left="10" Canvas.Top="10" Width="120" Height="80" Fill="#FF3366AA" Stroke="#FF112233" StrokeThickness="2"/>
                 <Ellipse Canvas.Left="150" Canvas.Top="10" Width="100" Height="80" Fill="#FFAA6633"/>
                 <Line X1="10" Y1="120" X2="260" Y2="160" Stroke="#FF333333" StrokeThickness="3"/>
                 <Polygon Points="300,20 360,60 340,120 280,100" Fill="#FF66AA66" Stroke="#FF224422" StrokeThickness="2"/>
               </Canvas>"##
        ),
        "shapes_path" => format!(
            r##"<Canvas {NS} Width="400" Height="300">
                 <Path Canvas.Left="20" Canvas.Top="20" Width="220" Height="220" Stretch="Fill" Fill="#FF88AA33" Stroke="#FF224400" StrokeThickness="2"
                       Data="M 10,100 C 10,50 90,50 90,100 S 170,150 170,100 L 170,40 A 30,30 0 1 1 110,40 Z"/>
               </Canvas>"##
        ),
        "styles_triggers" => format!(
            r##"<StackPanel {NS}>
                 <StackPanel.Resources>
                   <Style x:Key="Hot" TargetType="Button">
                     <Setter Property="Background" Value="#FF4477AA"/>
                     <Style.Triggers>
                       <Trigger Property="IsMouseOver" Value="True">
                         <Setter Property="Background" Value="#FF66AACC"/>
                       </Trigger>
                     </Style.Triggers>
                   </Style>
                 </StackPanel.Resources>
                 <Button Style="{{StaticResource Hot}}" Content="Hover" Width="140"/>
               </StackPanel>"##
        ),
        "dynamicresource" => format!(
            r##"<StackPanel {NS}>
                 <StackPanel.Resources>
                   <SolidColorBrush x:Key="Accent" Color="#FF2288CC"/>
                 </StackPanel.Resources>
                 <Border Background="{{DynamicResource Accent}}" Width="200" Height="80"/>
               </StackPanel>"##
        ),
        "composite_app_shell" => format!(
            r##"<DockPanel {NS}>
                 <Menu DockPanel.Dock="Top">
                   <MenuItem Header="File">
                     <MenuItem Header="New"/>
                     <MenuItem Header="Open Recent">
                       <MenuItem Header="project_a"/>
                       <MenuItem Header="project_b"/>
                     </MenuItem>
                     <Separator/>
                     <MenuItem Header="Exit"/>
                   </MenuItem>
                   <MenuItem Header="Edit">
                     <MenuItem Header="Copy"/>
                     <MenuItem Header="Paste"/>
                   </MenuItem>
                 </Menu>
                 <Grid Margin="8" ColumnDefinitions="240, 8, *">
                   <TreeView ToolTip="Project explorer">
                     <TreeViewItem Header="bevy_pf" IsExpanded="True">
                       <TreeViewItem Header="crates" IsExpanded="True">
                         <TreeViewItem Header="bevy_pf"/>
                         <TreeViewItem Header="bevy_pf_xaml"/>
                       </TreeViewItem>
                       <TreeViewItem Header="README.md"/>
                     </TreeViewItem>
                   </TreeView>
                   <TabControl Grid.Column="2">
                     <TabItem Header="Files">
                       <DataGrid ItemsSource="{{Binding rows}}">
                         <DataGrid.Columns>
                           <DataGridTextColumn Header="Name" Binding="{{Binding name}}" Width="2*"/>
                           <DataGridTextColumn Header="Type" Binding="{{Binding kind}}" Width="*"/>
                           <DataGridTextColumn Header="Score" Binding="{{Binding score}}" Width="*"/>
                         </DataGrid.Columns>
                       </DataGrid>
                     </TabItem>
                     <TabItem Header="Preview">
                       <StackPanel>
                         <TextBlock Text="Second tab content" FontSize="18" Margin="0,0,0,8"/>
                         <ProgressBar Minimum="0" Maximum="100" Value="60"/>
                       </StackPanel>
                     </TabItem>
                   </TabControl>
                 </Grid>
               </DockPanel>"##
        ),
        _ => return None,
    };
    Some(x)
}

#[derive(Resource)]
struct BenchConfig {
    scene: String,
    warmup: f64,
    measure: f64,
    offscreen: bool,
}

#[derive(Resource)]
struct BenchCamera(Entity);

#[derive(Resource, Default)]
struct BenchState {
    frame: u64,
    post_done: bool,
    warmup_accum: f64,
    measure_accum: f64,
    deltas: Vec<f64>,
}

fn main() {
    if std::env::var("BENCH_LIST").is_ok() {
        for n in scene_names() {
            println!("{n}");
        }
        return;
    }
    if let Ok(dir) = std::env::var("BENCH_DUMP_DIR") {
        std::fs::create_dir_all(&dir).expect("create dump dir");
        let mut count = 0;
        for name in scene_names() {
            if let Some(xaml) = scene_xaml(name) {
                std::fs::write(format!("{dir}/{name}.xaml"), xaml).expect("write scene");
                count += 1;
            }
        }
        println!("dumped {count} XAML scenes to {dir}");
        return;
    }

    let scene = std::env::var("BENCH_SCENE").unwrap_or_else(|_| "empty".to_string());
    if scene != "empty" && scene != "bevy_ui_raw" && scene_xaml(&scene).is_none() {
        eprintln!("unknown scene `{scene}` — BENCH_LIST=1 prints valid names");
        std::process::exit(2);
    }
    let present = match std::env::var("BENCH_PRESENT").as_deref() {
        Ok("immediate") => PresentMode::Immediate,
        Ok("mailbox") => PresentMode::Mailbox,
        Ok("fifo") => PresentMode::Fifo,
        _ => PresentMode::AutoNoVsync,
    };
    let (win_w, win_h) = match std::env::var("BENCH_WINDOW").as_deref() {
        Ok("small") => (640, 360),
        _ => (1280, 720),
    };
    let warmup = std::env::var("BENCH_WARMUP_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2.0);
    let measure = std::env::var("BENCH_MEASURE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5.0);

    let offscreen = std::env::var("BENCH_OFFSCREEN").is_ok();
    let minimal = std::env::var("BENCH_MINIMAL").is_ok();

    let mut app = App::new();
    let window_plugin = if offscreen {
        // Headless: no window, no winit — the render graph draws every frame
        // into a texture at full speed. macOS enforces display sync on
        // windowed presents (even PresentMode::Immediate), so uncapped
        // throughput is only measurable offscreen.
        WindowPlugin {
            primary_window: None,
            exit_condition: ExitCondition::DontExit,
            close_when_requested: false,
            ..Default::default()
        }
    } else {
        WindowPlugin {
            primary_window: Some(Window {
                resolution: WindowResolution::new(win_w, win_h),
                present_mode: present,
                // Never occluded: macOS throttles presents for windows hidden
                // behind others, which caps any benchmark at compositor rates.
                window_level: bevy::window::WindowLevel::AlwaysOnTop,
                position: WindowPosition::At(IVec2::new(40, 40)),
                title: format!("bevy_pf bench: {scene}"),
                ..Default::default()
            }),
            ..Default::default()
        }
    };
    let mut plugins = DefaultPlugins.set(window_plugin);
    if offscreen {
        plugins = plugins.disable::<bevy::winit::WinitPlugin>();
    }
    if minimal {
        // A GUI-only app doesn't tick 3D/audio/animation machinery. This is
        // the plugin set a pure bevy_pf application would realistically ship.
        plugins = plugins
            .disable::<bevy::audio::AudioPlugin>()
            .disable::<bevy::animation::AnimationPlugin>();
    }
    #[cfg(not(target_arch = "wasm32"))] // configured out of bevy_render on wasm
    if std::env::var("BENCH_NOPIPE").is_ok() {
        plugins =
            plugins.disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>();
    }
    app.add_plugins(plugins);
    if std::env::var("BENCH_ST").is_ok() {
        // Sub-millisecond frames are dominated by async-executor dispatch
        // (~2us x ~400 mostly-empty systems); run schedules single-threaded.
        // Render-app schedules can only be swapped without a window (macOS
        // surface creation must stay on its expected thread).
        if offscreen {
            bevy_pf::perf::tune_schedules_for_gui_headless(&mut app);
        } else {
            bevy_pf::perf::tune_schedules_for_gui(&mut app);
        }
    }
    if offscreen {
        app.add_plugins(bevy::app::ScheduleRunnerPlugin::run_loop(
            std::time::Duration::ZERO,
        ));
    } else {
        if std::env::var("BENCH_REACTIVE").is_ok() {
            // Desktop-app scheduling: render only on input/UI events, like a
            // native GUI toolkit. The right mode for shipped GUI apps.
            app.insert_resource(WinitSettings::desktop_app());
        } else {
            app.insert_resource(WinitSettings {
                focused_mode: UpdateMode::Continuous,
                unfocused_mode: UpdateMode::Continuous,
            });
        }
    }
    app.add_plugins(PfUiPlugin)
        .insert_resource(BenchConfig {
            scene,
            warmup,
            measure,
            offscreen,
        })
        .init_resource::<BenchState>()
        .add_systems(Startup, setup)
        .add_systems(Update, (post_action, measure_frames).chain())
        .run();
}

fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    config: Res<BenchConfig>,
) {
    let camera = if config.offscreen {
        let size = Extent3d {
            width: 1280,
            height: 720,
            depth_or_array_layers: 1,
        };
        let mut image = Image {
            texture_descriptor: TextureDescriptor {
                label: Some("bench target"),
                size,
                dimension: TextureDimension::D2,
                format: TextureFormat::Bgra8UnormSrgb,
                mip_level_count: 1,
                sample_count: 1,
                usage: TextureUsages::TEXTURE_BINDING
                    | TextureUsages::COPY_DST
                    | TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            },
            ..Default::default()
        };
        image.resize(size);
        let target = images.add(image);
        let mut cam = commands.spawn((Camera2d, RenderTarget::Image(target.into())));
        if std::env::var("BENCH_MSAA").as_deref() == Ok("off") {
            cam.insert(bevy::render::view::Msaa::Off);
        }
        cam.id()
    } else {
        commands.spawn(Camera2d).id()
    };
    commands.insert_resource(BenchCamera(camera));
    match config.scene.as_str() {
        "empty" => {}
        "bevy_ui_raw" => {
            // The same button scene built with plain bevy_ui — quantifies
            // bevy_pf's runtime overhead against a hand-rolled equivalent.
            commands
                .spawn(Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..Default::default()
                })
                .with_children(|p| {
                    p.spawn((
                        Node {
                            width: Val::Px(140.0),
                            height: Val::Px(36.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..Default::default()
                        },
                        BackgroundColor(Color::srgb(0.87, 0.87, 0.87)),
                        Interaction::default(),
                    ))
                    .with_children(|b| {
                        b.spawn((
                            bevy::ui::widget::Text::new("Click me"),
                            bevy::text::TextFont {
                                font_size: bevy::text::FontSize::Px(13.0),
                                ..Default::default()
                            },
                            bevy::text::TextColor(Color::BLACK),
                        ));
                    });
                });
        }
        name => {
            let xaml = scene_xaml(name).expect("scene existence checked in main");
            let scene =
                XamlScene::parse(xaml).expect("bench scene XAML must be valid");
            if needs_vm(name) {
                commands.spawn_xaml_bound(scene, bench_vm());
            } else {
                commands.spawn_xaml(scene);
            }
        }
    }
}

/// One-shot post-instantiation action for the `*_open` popup variants, run on
/// the first Update frame (instantiation commands have been applied by then).
fn post_action(world: &mut World) {
    if world.resource::<BenchState>().post_done {
        return;
    }
    world.resource_mut::<BenchState>().post_done = true;

    // Offscreen: UI trees target the primary window by default; point every
    // UI root (the scene root and the popup overlay root) at the bench camera.
    if world.resource::<BenchConfig>().offscreen {
        let camera = world.resource::<BenchCamera>().0;
        let mut q = world.query_filtered::<Entity, (With<Node>, Without<ChildOf>)>();
        let roots: Vec<Entity> = q.iter(world).collect();
        for root in roots {
            world.entity_mut(root).insert(UiTargetCamera(camera));
        }
    }

    let scene = world.resource::<BenchConfig>().scene.clone();
    match scene.as_str() {
        "combobox_open" => {
            let mut q = world.query::<&bevy_pf::components::PfComboBox>();
            let popups: Vec<Entity> = q.iter(world).map(|c| c.popup).collect();
            for p in popups {
                if let Some(mut pop) = world.get_mut::<bevy_pf::PfPopup>(p) {
                    pop.open = true;
                }
            }
        }
        "menu_open" => {
            let mut q = world.query::<&XamlNames>();
            let file = q.iter(world).find_map(|n| n.get("File"));
            if let Some(file) = file {
                bevy_pf::instantiate::activate_menu_item(world, file);
            }
        }
        "contextmenu_open" => {
            let mut q = world.query::<(Entity, &bevy_pf::components::PfMenuPopup)>();
            let popups: Vec<Entity> = q.iter(world).map(|(e, _)| e).collect();
            for p in popups {
                if let Some(mut pop) = world.get_mut::<bevy_pf::PfPopup>(p) {
                    pop.open = true;
                }
            }
        }
        _ => {}
    }
}

fn measure_frames(
    time: Res<Time<Real>>,
    config: Res<BenchConfig>,
    mut state: ResMut<BenchState>,
    mut exit: MessageWriter<AppExit>,
) {
    state.frame += 1;
    // Let startup work (raster, layout, font atlas) settle out entirely.
    if state.frame <= 30 {
        return;
    }
    let dt = time.delta_secs_f64();
    if dt <= 0.0 {
        return;
    }
    if state.warmup_accum < config.warmup {
        state.warmup_accum += dt;
        return;
    }
    state.deltas.push(dt);
    state.measure_accum += dt;
    if state.measure_accum < config.measure {
        return;
    }

    let n = state.deltas.len();
    let total: f64 = state.deltas.iter().sum();
    let mut sorted = state.deltas.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = sorted[n / 2];
    let p99 = sorted[(((n as f64) * 0.99) as usize).min(n - 1)];
    let worst = *sorted.last().unwrap();
    println!(
        "BENCH_RESULT scene={} frames={} secs={:.3} mean_fps={:.1} p50_fps={:.1} p99low_fps={:.1} min_fps={:.1}",
        config.scene,
        n,
        total,
        n as f64 / total,
        1.0 / p50,
        1.0 / p99,
        1.0 / worst,
    );
    exit.write(AppExit::Success);
}
