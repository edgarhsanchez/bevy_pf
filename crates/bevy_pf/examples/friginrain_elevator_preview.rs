//! Design preview for the friginrain2 elevator screens.
//!
//! These four panels are rendered in-game onto quads in the world (Building
//! Shaper emits them tagged `frig_ui`), so the only faithful preview is
//! bevy_pf's own renderer — a WPF/XAML designer would not understand the
//! dialect, and VS Code shows only markup.
//!
//! The asset root is pointed at the GAME's assets so
//! `ResourceDictionary Source="ui/elevator/obsidian.xaml"` resolves through the
//! same path it will at runtime; this doubles as a check that the merged
//! dictionary actually loads.
//!
//! Each panel is shown at its true pixel size (the size of the offscreen image
//! it renders into in-game) and again at 3x, because at 1:1 these are small
//! physical devices — 152 x 232 px is a 0.30 x 0.45 m plate.
//!
//! Run with:
//!   cargo run -p bevy_pf --example friginrain_elevator_preview
//! Capture a frame and exit:
//!   ELEV_SHOT=out.png cargo run -p bevy_pf --example friginrain_elevator_preview

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy::reflect::Reflect;
use bevy_pf::prelude::*;

const GAME_ASSETS: &str = "D:/github/friginrain2/assets";

/// Mirrors the view-model the panels bind against; field names must match the
/// `{Binding ...}` paths in the .xaml files.
#[derive(Reflect, Default)]
struct ElevatorPanelVm {
    car_floor_label: String,
    car_state: String,
    status: String,
    target_label: String,
    travel_fraction: f64,
    up_opacity: f64,
    down_opacity: f64,
    safe_label: String,
    locked_caption: String,
    direction_label: String,
    floors: Vec<FloorRowVm>,
    locked_floors: Vec<FloorRowVm>,
}

#[derive(Reflect, Default)]
struct FloorRowVm {
    label: String,
    depth: i32,
}

fn main() {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(AssetPlugin {
                file_path: GAME_ASSETS.to_string(),
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "friginrain — elevator screens".into(),
                    resolution: bevy::window::WindowResolution::new(1280, 900),
                    ..default()
                }),
                ..default()
            }),
    )
    .add_plugins(PfUiPlugin)
    .add_systems(Startup, setup);

    if let Ok(path) = std::env::var("ELEV_SHOT") {
        app.add_systems(
            Update,
            move |mut commands: Commands, time: Res<Time>, mut done: Local<bool>| {
                // Two seconds is enough for the asset load + instantiate to
                // settle; screenshotting earlier catches an empty frame.
                if !*done && time.elapsed_secs() > 2.5 {
                    *done = true;
                    commands
                        .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
                        .observe(bevy::render::view::screenshot::save_to_disk(path.clone()));
                }
                if *done && time.elapsed_secs() > 4.0 {
                    std::process::exit(0);
                }
            },
        );
    }
    app.run();
}

fn vm() -> Bindable {
    Bindable::new(ElevatorPanelVm {
        car_floor_label: "3".into(),
        car_state: "DOORS OPEN".into(),
        status: "CLEAR B1 TO DESCEND".into(),
        target_label: "B2".into(),
        travel_fraction: 0.62,
        up_opacity: 1.0,
        down_opacity: 0.25,
        safe_label: "SAFE — DOORS CLOSED".into(),
        locked_caption: "LOCKED".into(),
        direction_label: "UP".into(),
        floors: vec![
            row("R", 0),
            row("6", 0),
            row("5", 0),
            row("4", 0),
            row("3", 0),
            row("2", 0),
            row("1", 0),
            row("G", 0),
            row("B1", 1),
        ],
        locked_floors: vec![row("B2", 2), row("B3", 3), row("B4", 4)],
    })
}

fn row(label: &str, depth: i32) -> FloorRowVm {
    FloorRowVm {
        label: label.into(),
        depth,
    }
}

fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    commands.spawn(Camera2d);

    let panels: [(&str, u32, u32); 4] = [
        ("ui/elevator/call_panel.xaml", 152, 232),
        ("ui/elevator/hall_indicator.xaml", 204, 92),
        ("ui/elevator/car_panel.xaml", 152, 360),
        ("ui/elevator/car_indicator.xaml", 184, 72),
    ];

    // Backdrop roughly the value of a lit interior wall, so the screens are
    // judged against what they will actually sit on rather than against black.
    let root = commands
        .spawn((
            Node {
                width: percent(100),
                height: percent(100),
                padding: UiRect::all(px(20)),
                column_gap: px(24),
                align_items: AlignItems::FlexStart,
                ..default()
            },
            BackgroundColor(Color::srgb(0.20, 0.20, 0.22)),
        ))
        .id();

    for (path, w, h) in panels {
        let name = path.rsplit('/').next().unwrap_or(path);
        let column = commands
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(10),
                    ..default()
                },
                ChildOf(root),
            ))
            .id();

        commands.spawn((
            Text::new(format!("{name}\n{w}x{h}")),
            TextFont::from_font_size(11.0),
            TextColor(Color::srgb(0.75, 0.78, 0.82)),
            ChildOf(column),
        ));

        // 1:1 only. Scaling the host node just makes a bigger box — font sizes
        // are fixed px, so a "3x" column would misrepresent the design. Upscale
        // the captured PNG instead if you want to inspect the type.
        spawn_panel(&mut commands, &assets, path, w, h, 1.0, column);
    }
}

fn spawn_panel(
    commands: &mut Commands,
    assets: &AssetServer,
    path: &str,
    w: u32,
    h: u32,
    scale: f32,
    parent: Entity,
) {
    let host = commands
        .spawn((
            Node {
                width: px(w as f32 * scale),
                height: px(h as f32 * scale),
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    commands.spawn((
        XamlView(assets.load(path.to_string())),
        DataContext(vm()),
        Node {
            width: percent(100),
            height: percent(100),
            ..default()
        },
        ChildOf(host),
    ));
}
