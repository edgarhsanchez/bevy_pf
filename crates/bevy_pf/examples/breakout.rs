//! Breakout — the whole game surface is XAML.
//!
//! Menus (title, pause, game over, level clear), the HUD (`{Binding}` to a
//! reflected view-model), and every gameplay visual — paddle, ball, and all
//! 40 bricks — are XAML elements on a `Canvas` inside a `Viewbox`, so the
//! 960x600 logical playfield scales to any window. Bevy systems only push
//! numbers: ball physics writes `Canvas.Left/Top` offsets, scoring writes the
//! view-model, menu buttons are wired through `PfQuery` + observers.
//!
//! Run with: `cargo run -p bevy_pf --example breakout`
//!
//! Controls: mouse or ←/→ (A/D) to move, Space to launch, P to pause,
//! Esc for the menu.

use bevy::prelude::*;
use bevy::reflect::Reflect;
use bevy::window::PrimaryWindow;
use bevy_pf::prelude::*;

const W: f32 = 960.0;
const H: f32 = 600.0;
const PADDLE_W: f32 = 120.0;
const PADDLE_H: f32 = 16.0;
const PADDLE_Y: f32 = H - 44.0;
const BALL: f32 = 14.0;
const COLS: usize = 8;
const ROWS: usize = 5;
const BRICK_W: f32 = 104.0;
const BRICK_H: f32 = 24.0;
const BRICK_GAP: f32 = 8.0;
const BRICKS_LEFT: f32 = (W - (COLS as f32 * BRICK_W + (COLS as f32 - 1.0) * BRICK_GAP)) / 2.0;
const BRICKS_TOP: f32 = 84.0;
const SPONSOR_URL: &str = "https://github.com/sponsors/edgarhsanchez";

/// Row colors + points, top row is worth the most.
const ROW_STYLE: [(&str, u32); ROWS] = [
    ("#FFE05555", 50),
    ("#FFE09044", 40),
    ("#FFE0C044", 30),
    ("#FF55B055", 20),
    ("#FF5588CC", 10),
];

#[derive(Reflect, Default)]
struct Hud {
    score: u32,
    lives: u32,
    level: u32,
    best: u32,
}

#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Menu,
    Serve,
    Playing,
    Paused,
    GameOver,
    LevelClear,
}

struct Brick {
    entity: Entity,
    x: f32,
    y: f32,
    points: u32,
    alive: bool,
}

#[derive(Resource, Default)]
struct Game {
    wired: bool,
    ball: Option<Entity>,
    paddle: Option<Entity>,
    bricks: Vec<Brick>,
    ball_pos: Vec2,
    ball_vel: Vec2,
    paddle_x: f32,
    speed: f32,
}

#[derive(Resource)]
struct VmHandle(Bindable);

/// Button clicks land here; `run_actions` applies them with full world state.
#[derive(Resource, Default)]
struct PendingAction(Option<Action>);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    NewGame,
    Resume,
    NextLevel,
    ToMenu,
    Sponsor,
    Quit,
}

fn main() {
    #[allow(unused_mut)] // the wasm cfg block below mutates it
    let mut window = Window {
        title: "bevy_pf breakout — XAML all the way down".to_string(),
        ..Default::default()
    };
    #[cfg(target_arch = "wasm32")]
    {
        window.canvas = Some("#bevy-canvas".to_string());
        window.fit_canvas_to_parent = true;
    }
    App::new()
        .add_plugins({
            let plugins = DefaultPlugins.set(WindowPlugin {
                primary_window: Some(window),
                ..Default::default()
            });
            // No demo plays audio; skipping the plugin on the web avoids the
            // browser's AudioContext autoplay warning.
            #[cfg(target_arch = "wasm32")]
            let plugins = plugins.disable::<bevy::audio::AudioPlugin>();
            plugins
        })
        .add_plugins(PfUiPlugin)
        .insert_resource(Phase::Menu)
        .init_resource::<Game>()
        .init_resource::<PendingAction>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                wire_ui,
                run_actions,
                keyboard,
                paddle_input,
                tick_ball,
                sync_panels,
                apply_positions,
            )
                .chain(),
        )
        .run();
}

/// The complete game scene. The static shell is written out; the brick grid
/// is generated markup — still XAML through the same parser as everything.
fn scene_xaml() -> String {
    let mut bricks = String::new();
    for (row, &(color, _)) in ROW_STYLE.iter().enumerate() {
        for col in 0..COLS {
            let x = BRICKS_LEFT + col as f32 * (BRICK_W + BRICK_GAP);
            let y = BRICKS_TOP + row as f32 * (BRICK_H + BRICK_GAP);
            bricks.push_str(&format!(
                r##"<Border x:Name="Brick_{row}_{col}" Canvas.Left="{x}" Canvas.Top="{y}" Width="{BRICK_W}" Height="{BRICK_H}" Background="{color}" CornerRadius="4"/>"##
            ));
        }
    }
    let ball_x = W / 2.0 - BALL / 2.0;
    let paddle_x = W / 2.0 - PADDLE_W / 2.0;
    let quit_button = if cfg!(target_arch = "wasm32") {
        ""
    } else {
        r##"<Button x:Name="QuitBtn" Content="Quit" Width="240" Margin="0,6,0,0"/>"##
    };

    format!(
        r##"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                  xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                  Background="#FF0B0E14">

              <Viewbox>
                <Border Background="#FF141A26" BorderBrush="#FF2A3550" BorderThickness="2">
                  <Canvas Width="{W}" Height="{H}">
                    <StackPanel Canvas.Left="16" Canvas.Top="12" Orientation="Horizontal">
                      <TextBlock Text="SCORE " Foreground="#FF8FA3C8" FontSize="20" FontWeight="Bold"/>
                      <TextBlock Text="{{Binding score}}" Foreground="White" FontSize="20" FontWeight="Bold"/>
                      <TextBlock Text="   BEST " Foreground="#FF8FA3C8" FontSize="20" FontWeight="Bold"/>
                      <TextBlock Text="{{Binding best}}" Foreground="White" FontSize="20" FontWeight="Bold"/>
                    </StackPanel>
                    <StackPanel Canvas.Left="700" Canvas.Top="12" Orientation="Horizontal">
                      <TextBlock Text="LEVEL " Foreground="#FF8FA3C8" FontSize="20" FontWeight="Bold"/>
                      <TextBlock Text="{{Binding level}}" Foreground="White" FontSize="20" FontWeight="Bold"/>
                      <TextBlock Text="   LIVES " Foreground="#FF8FA3C8" FontSize="20" FontWeight="Bold"/>
                      <TextBlock Text="{{Binding lives}}" Foreground="White" FontSize="20" FontWeight="Bold"/>
                    </StackPanel>

                    {bricks}

                    <Border x:Name="Paddle" Canvas.Left="{paddle_x}" Canvas.Top="{PADDLE_Y}" Width="{PADDLE_W}" Height="{PADDLE_H}" CornerRadius="8" Background="#FF66AACC"/>
                    <Ellipse x:Name="Ball" Canvas.Left="{ball_x}" Canvas.Top="520" Width="{BALL}" Height="{BALL}" Fill="#FFF5F5F5"/>
                  </Canvas>
                </Border>
              </Viewbox>

              <Grid x:Name="MenuPanel" Background="#D00B0E14">
                <StackPanel HorizontalAlignment="Center" VerticalAlignment="Center">
                  <TextBlock Text="BREAKOUT" Foreground="White" FontSize="52" FontWeight="Bold" HorizontalAlignment="Center"/>
                  <TextBlock Text="menus, HUD, and every brick are XAML — running on Bevy" Foreground="#FF8FA3C8" FontSize="14" Margin="0,4,0,20" HorizontalAlignment="Center"/>
                  <Button x:Name="PlayBtn" Content="Play" Width="240"/>
                  <Button x:Name="SponsorBtn" Content="Sponsor this project" Width="240" Margin="0,6,0,0"/>
                  {quit_button}
                  <TextBlock Text="mouse or arrows to move - Space to launch - P pauses" Foreground="#FF6B7A99" FontSize="12" Margin="0,18,0,0" HorizontalAlignment="Center"/>
                </StackPanel>
              </Grid>

              <Grid x:Name="PausePanel" Background="#D00B0E14">
                <StackPanel HorizontalAlignment="Center" VerticalAlignment="Center">
                  <TextBlock Text="PAUSED" Foreground="White" FontSize="40" FontWeight="Bold" HorizontalAlignment="Center"/>
                  <Button x:Name="ResumeBtn" Content="Resume" Width="240" Margin="0,16,0,0"/>
                  <Button x:Name="PauseMenuBtn" Content="Back to Menu" Width="240" Margin="0,6,0,0"/>
                </StackPanel>
              </Grid>

              <Grid x:Name="GameOverPanel" Background="#D0140B0B">
                <StackPanel HorizontalAlignment="Center" VerticalAlignment="Center">
                  <TextBlock Text="GAME OVER" Foreground="#FFE05555" FontSize="44" FontWeight="Bold" HorizontalAlignment="Center"/>
                  <StackPanel Orientation="Horizontal" HorizontalAlignment="Center" Margin="0,8,0,0">
                    <TextBlock Text="score " Foreground="#FF8FA3C8" FontSize="20"/>
                    <TextBlock Text="{{Binding score}}" Foreground="White" FontSize="20" FontWeight="Bold"/>
                  </StackPanel>
                  <Button x:Name="RetryBtn" Content="Play Again" Width="240" Margin="0,16,0,0"/>
                  <Button x:Name="OverMenuBtn" Content="Back to Menu" Width="240" Margin="0,6,0,0"/>
                </StackPanel>
              </Grid>

              <Grid x:Name="ClearPanel" Background="#D00B140E">
                <StackPanel HorizontalAlignment="Center" VerticalAlignment="Center">
                  <TextBlock Text="LEVEL CLEAR!" Foreground="#FF55B055" FontSize="44" FontWeight="Bold" HorizontalAlignment="Center"/>
                  <Button x:Name="NextBtn" Content="Next Level" Width="240" Margin="0,16,0,0"/>
                  <Button x:Name="ClearMenuBtn" Content="Back to Menu" Width="240" Margin="0,6,0,0"/>
                </StackPanel>
              </Grid>
            </Grid>"##
    )
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let vm = Bindable::new(Hud {
        score: 0,
        lives: 3,
        level: 1,
        best: 0,
    });
    let scene = XamlScene::parse(scene_xaml()).expect("breakout scene must be valid XAML");
    commands.spawn_xaml_bound(scene, vm.clone());
    commands.insert_resource(VmHandle(vm));
}

/// One-shot after instantiation: cache entities, wire buttons to actions.
fn wire_ui(mut game: ResMut<Game>, ui: PfQuery, mut commands: Commands) {
    if game.wired {
        return;
    }
    let Some(ball) = ui.by_name("Ball") else {
        return; // scene not instantiated yet
    };
    game.ball = Some(ball);
    game.paddle = ui.by_name("Paddle");
    game.paddle_x = W / 2.0 - PADDLE_W / 2.0;
    game.ball_pos = Vec2::new(W / 2.0 - BALL / 2.0, 520.0);
    game.speed = 380.0;

    for (row, &(_, points)) in ROW_STYLE.iter().enumerate() {
        for col in 0..COLS {
            if let Some(entity) = ui.by_name(&format!("Brick_{row}_{col}")) {
                game.bricks.push(Brick {
                    entity,
                    x: BRICKS_LEFT + col as f32 * (BRICK_W + BRICK_GAP),
                    y: BRICKS_TOP + row as f32 * (BRICK_H + BRICK_GAP),
                    points,
                    alive: true,
                });
            }
        }
    }

    let buttons = [
        ("PlayBtn", Action::NewGame),
        ("SponsorBtn", Action::Sponsor),
        ("QuitBtn", Action::Quit),
        ("ResumeBtn", Action::Resume),
        ("PauseMenuBtn", Action::ToMenu),
        ("RetryBtn", Action::NewGame),
        ("OverMenuBtn", Action::ToMenu),
        ("NextBtn", Action::NextLevel),
        ("ClearMenuBtn", Action::ToMenu),
    ];
    for (name, action) in buttons {
        if let Some(button) = ui.by_name(name) {
            commands.entity(button).observe(
                move |_: On<Pointer<Click>>, mut pending: ResMut<PendingAction>| {
                    pending.0 = Some(action);
                },
            );
        }
    }
    game.wired = true;
}

fn run_actions(
    mut pending: ResMut<PendingAction>,
    mut phase: ResMut<Phase>,
    mut game: ResMut<Game>,
    vm: Res<VmHandle>,
    mut nodes: Query<&mut Node>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(action) = pending.0.take() else {
        return;
    };
    match action {
        Action::NewGame => {
            vm.0.update(|hud: &mut Hud| {
                hud.score = 0;
                hud.lives = 3;
                hud.level = 1;
            });
            game.speed = 380.0;
            reset_bricks(&mut game, &mut nodes);
            reset_serve(&mut game);
            *phase = Phase::Serve;
        }
        Action::NextLevel => {
            vm.0.update(|hud: &mut Hud| hud.level += 1);
            game.speed += 45.0;
            reset_bricks(&mut game, &mut nodes);
            reset_serve(&mut game);
            *phase = Phase::Serve;
        }
        Action::Resume => *phase = Phase::Playing,
        Action::ToMenu => *phase = Phase::Menu,
        Action::Sponsor => open_sponsor_page(),
        Action::Quit => {
            exit.write(AppExit::Success);
        }
    }
}

fn reset_bricks(game: &mut Game, nodes: &mut Query<&mut Node>) {
    for brick in &mut game.bricks {
        brick.alive = true;
        if let Ok(mut node) = nodes.get_mut(brick.entity) {
            node.display = Display::Flex;
        }
    }
}

fn reset_serve(game: &mut Game) {
    game.paddle_x = W / 2.0 - PADDLE_W / 2.0;
    game.ball_vel = Vec2::ZERO;
}

fn keyboard(keys: Res<ButtonInput<KeyCode>>, mut phase: ResMut<Phase>, mut game: ResMut<Game>) {
    match *phase {
        Phase::Serve => {
            if keys.just_pressed(KeyCode::Space) {
                // Launch upward with a slight random-ish tilt from paddle pos.
                let tilt = ((game.paddle_x / W) - 0.5) * 0.8;
                game.ball_vel = Vec2::new(tilt.sin(), -tilt.cos()).normalize() * game.speed;
                *phase = Phase::Playing;
            }
            if keys.just_pressed(KeyCode::Escape) {
                *phase = Phase::Menu;
            }
        }
        Phase::Playing => {
            if keys.just_pressed(KeyCode::KeyP) {
                *phase = Phase::Paused;
            }
            if keys.just_pressed(KeyCode::Escape) {
                *phase = Phase::Menu;
            }
        }
        Phase::Paused
            if (keys.just_pressed(KeyCode::KeyP) || keys.just_pressed(KeyCode::Space)) =>
        {
            *phase = Phase::Playing;
        }
        _ => {}
    }
}

fn paddle_input(
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cursor_moves: MessageReader<CursorMoved>,
    time: Res<Time>,
    phase: Res<Phase>,
    mut game: ResMut<Game>,
) {
    if !matches!(*phase, Phase::Serve | Phase::Playing) {
        cursor_moves.clear();
        return;
    }
    let mut dx = 0.0;
    if keys.pressed(KeyCode::ArrowLeft) || keys.pressed(KeyCode::KeyA) {
        dx -= 1.0;
    }
    if keys.pressed(KeyCode::ArrowRight) || keys.pressed(KeyCode::KeyD) {
        dx += 1.0;
    }
    if dx != 0.0 {
        game.paddle_x += dx * 620.0 * time.delta_secs();
    }
    // Mouse: map window x to playfield x (approximate under letterboxing).
    if let Some(moved) = cursor_moves.read().last()
        && let Ok(window) = windows.single()
    {
        let frac = moved.position.x / window.width().max(1.0);
        game.paddle_x = frac * W - PADDLE_W / 2.0;
    }
    game.paddle_x = game.paddle_x.clamp(0.0, W - PADDLE_W);
}

fn tick_ball(
    time: Res<Time>,
    mut phase: ResMut<Phase>,
    mut game: ResMut<Game>,
    vm: Res<VmHandle>,
    mut nodes: Query<&mut Node>,
) {
    if *phase != Phase::Playing {
        return;
    }
    let dt = time.delta_secs().min(1.0 / 30.0);
    let mut pos = game.ball_pos + game.ball_vel * dt;
    let mut vel = game.ball_vel;

    // Walls.
    if pos.x <= 0.0 {
        pos.x = 0.0;
        vel.x = vel.x.abs();
    }
    if pos.x >= W - BALL {
        pos.x = W - BALL;
        vel.x = -vel.x.abs();
    }
    if pos.y <= 0.0 {
        pos.y = 0.0;
        vel.y = vel.y.abs();
    }

    // Paddle: bounce angle follows where the ball strikes.
    let paddle_x = game.paddle_x;
    if vel.y > 0.0
        && pos.y + BALL >= PADDLE_Y
        && pos.y + BALL <= PADDLE_Y + PADDLE_H + 6.0
        && pos.x + BALL >= paddle_x
        && pos.x <= paddle_x + PADDLE_W
    {
        let hit = ((pos.x + BALL / 2.0) - (paddle_x + PADDLE_W / 2.0)) / (PADDLE_W / 2.0);
        let angle = hit.clamp(-1.0, 1.0) * 1.05; // up to ~60 degrees
        let speed = vel.length().max(game.speed);
        vel = Vec2::new(angle.sin(), -angle.cos()) * speed;
        pos.y = PADDLE_Y - BALL;
    }

    // Bricks: reflect off the axis of least penetration, kill the brick.
    let mut scored = 0;
    for brick in &mut game.bricks {
        if !brick.alive {
            continue;
        }
        let overlap_x = (pos.x + BALL).min(brick.x + BRICK_W) - pos.x.max(brick.x);
        let overlap_y = (pos.y + BALL).min(brick.y + BRICK_H) - pos.y.max(brick.y);
        if overlap_x <= 0.0 || overlap_y <= 0.0 {
            continue;
        }
        brick.alive = false;
        scored += brick.points;
        if let Ok(mut node) = nodes.get_mut(brick.entity) {
            node.display = Display::None;
        }
        if overlap_x < overlap_y {
            vel.x = -vel.x;
        } else {
            vel.y = -vel.y;
        }
        break; // one brick per frame keeps reflections sane
    }
    if scored > 0 {
        vm.0.update(|hud: &mut Hud| {
            hud.score += scored;
            hud.best = hud.best.max(hud.score);
        });
    }

    // Bottom: lose a life.
    if pos.y > H {
        let mut lives_left = 0;
        vm.0.update(|hud: &mut Hud| {
            hud.lives = hud.lives.saturating_sub(1);
            lives_left = hud.lives;
        });
        if lives_left == 0 {
            *phase = Phase::GameOver;
        } else {
            reset_serve(&mut game);
            *phase = Phase::Serve;
        }
        return;
    }

    game.ball_pos = pos;
    game.ball_vel = vel;

    if game.bricks.iter().all(|b| !b.alive) {
        *phase = Phase::LevelClear;
    }
}

/// Push logical positions into the XAML canvas offsets.
fn apply_positions(phase: Res<Phase>, mut game: ResMut<Game>, mut nodes: Query<&mut Node>) {
    if *phase == Phase::Serve {
        // Ball rides the paddle until launch.
        game.ball_pos = Vec2::new(
            game.paddle_x + PADDLE_W / 2.0 - BALL / 2.0,
            PADDLE_Y - BALL - 2.0,
        );
    }
    if let Some(paddle) = game.paddle
        && let Ok(mut node) = nodes.get_mut(paddle)
    {
        node.left = Val::Px(game.paddle_x);
    }
    if let Some(ball) = game.ball
        && let Ok(mut node) = nodes.get_mut(ball)
    {
        node.left = Val::Px(game.ball_pos.x);
        node.top = Val::Px(game.ball_pos.y);
    }
}

/// Show exactly the overlay panel the current phase calls for.
fn sync_panels(phase: Res<Phase>, ui: PfQuery, mut nodes: Query<&mut Node>) {
    if !phase.is_changed() {
        return;
    }
    let panels = [
        ("MenuPanel", *phase == Phase::Menu),
        ("PausePanel", *phase == Phase::Paused),
        ("GameOverPanel", *phase == Phase::GameOver),
        ("ClearPanel", *phase == Phase::LevelClear),
    ];
    for (name, visible) in panels {
        if let Some(panel) = ui.by_name(name)
            && let Ok(mut node) = nodes.get_mut(panel)
        {
            node.display = if visible {
                Display::Grid
            } else {
                Display::None
            };
        }
    }
}

fn open_sponsor_page() {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            let _ = window.open_with_url_and_target(SPONSOR_URL, "_blank");
        }
    }
    #[cfg(all(not(target_arch = "wasm32"), target_os = "macos"))]
    {
        let _ = std::process::Command::new("open").arg(SPONSOR_URL).spawn();
    }
    #[cfg(all(not(target_arch = "wasm32"), target_os = "linux"))]
    {
        let _ = std::process::Command::new("xdg-open")
            .arg(SPONSOR_URL)
            .spawn();
    }
    #[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", SPONSOR_URL])
            .spawn();
    }
}
