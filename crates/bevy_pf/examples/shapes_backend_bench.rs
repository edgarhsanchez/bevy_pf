//! Head-to-head benchmark: CPU (tiny-skia) vs GPU (`bevy_pf_vector`) shape
//! backends on the same workload, in the same app.
//!
//! The interesting case is NOT static shapes — the CPU backend caches those by
//! pixel size, so a shape that never changes costs nothing on either path.
//! It is shapes whose paint is data-bound, which is what `--animate` models:
//! the CPU backend re-rasterizes and allocates a fresh `Image` (a full texture
//! re-upload) on every change, while the GPU backend carries colour
//! per-instance and re-tessellates nothing.
//!
//!   cargo run --release -p bevy_pf --features vector_gpu \
//!       --example shapes_backend_bench -- --backend cpu --shapes 200 --animate
//!   cargo run --release -p bevy_pf --features vector_gpu \
//!       --example shapes_backend_bench -- --backend gpu --shapes 200 --animate
//!   cargo run --release -p bevy_pf --features vector_gpu \
//!       --example shapes_backend_bench -- --backend native --shapes 200 --animate
//!       [--warmup N] [--frames N]
//!
//! Reports CPU frame time percentiles over 600 frames after a 120-frame
//! warmup. Frame time, not FPS averages.

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::window::PresentMode;
use bevy_pf::shapes::{PfShape, ShapeGeometry};
use bevy_pf_xaml::value as v;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Backend {
    Cpu,
    Native,
    Gpu,
}

impl Backend {
    fn name(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Native => "native",
            Self::Gpu => "gpu",
        }
    }
}

#[derive(Resource)]
struct Config {
    shapes: usize,
    animate: bool,
    backend: Backend,
    size: Vec2,
    /// Points in the generated polygon; 0 uses a rounded rectangle.
    complex: usize,
    warmup: u32,
    frames: usize,
}

#[derive(Resource, Default)]
struct Samples {
    frames: u32,
    ms: Vec<f32>,
}

fn arg_value(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn main() {
    let backend = match arg_value("--backend").as_deref() {
        Some("cpu") => Backend::Cpu,
        Some("native") => Backend::Native,
        Some("gpu") | None => Backend::Gpu,
        Some(other) => panic!("unknown backend '{other}' (cpu|native|gpu)"),
    };
    let config = Config {
        shapes: arg_value("--shapes")
            .and_then(|s| s.parse().ok())
            .unwrap_or(200),
        animate: std::env::args().any(|a| a == "--animate"),
        backend,
        size: Vec2::new(
            arg_value("--w")
                .and_then(|s| s.parse().ok())
                .unwrap_or(56.0),
            arg_value("--h")
                .and_then(|s| s.parse().ok())
                .unwrap_or(40.0),
        ),
        complex: arg_value("--complex")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
        warmup: arg_value("--warmup")
            .and_then(|s| s.parse().ok())
            .unwrap_or(120),
        frames: arg_value("--frames")
            .and_then(|s| s.parse().ok())
            .unwrap_or(600),
    };
    println!(
        "bench: backend={} shapes={} animate={} size={}x{} complex={} frames={} warmup={}",
        config.backend.name(),
        config.shapes,
        config.animate,
        config.size.x,
        config.size.y,
        config.complex,
        config.frames,
        config.warmup
    );

    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(0.04, 0.05, 0.07)))
        .insert_resource(config)
        .init_resource::<Samples>()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                resolution: (1280u32, 720u32).into(),
                present_mode: PresentMode::Immediate,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        .add_systems(Startup, setup)
        .add_systems(Update, (animate_fills, sample_frames));

    app.configure_sets(
        PostUpdate,
        (
            bevy_pf::shapes::PfShapeSystems::Claim,
            bevy_pf::shapes::PfShapeSystems::Rasterize,
        )
            .chain()
            .after(bevy::ui::UiSystems::Layout),
    );
    app.add_systems(
        PostUpdate,
        bevy_pf::shapes::rasterize_shapes.in_set(bevy_pf::shapes::PfShapeSystems::Rasterize),
    );
    match backend {
        Backend::Cpu => {}
        Backend::Native => {
            app.add_systems(
                PostUpdate,
                bevy_pf::shapes::style_native_shapes.in_set(bevy_pf::shapes::PfShapeSystems::Claim),
            );
        }
        Backend::Gpu => {
            app.add_plugins(bevy_pf::shapes_gpu::PfShapeGpuPlugin);
        }
    }
    app.run();
}

fn setup(mut commands: Commands, config: Res<Config>) {
    commands.spawn(Camera2d);
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_wrap: FlexWrap::Wrap,
            ..default()
        })
        .with_children(|parent| {
            for i in 0..config.shapes {
                // A many-pointed star is the case tessellate-once is for: the
                // CPU backend re-flattens and scanline-fills it every time the
                // paint changes, the engine tessellates it once and re-instances.
                let mut shape = if config.complex > 0 {
                    let n = config.complex;
                    let (cx, cy) = (config.size.x * 0.5, config.size.y * 0.5);
                    let points = (0..n)
                        .map(|k| {
                            let a = k as f32 / n as f32 * std::f32::consts::TAU;
                            let r = if k % 2 == 0 { 0.48 } else { 0.22 };
                            v::Point::new(
                                cx + a.cos() * config.size.x * r,
                                cy + a.sin() * config.size.y * r,
                            )
                        })
                        .collect();
                    PfShape::new(ShapeGeometry::Polyline {
                        points,
                        closed: true,
                    })
                } else {
                    PfShape::new(ShapeGeometry::Rectangle {
                        radius_x: 4.0,
                        radius_y: 4.0,
                    })
                };
                shape.fill = Some(v::PfBrush::Solid(v::PfColor {
                    r: (i % 255) as u8,
                    g: 128,
                    b: 200,
                    a: 255,
                }));
                parent.spawn((
                    shape,
                    Node {
                        width: Val::Px(config.size.x),
                        height: Val::Px(config.size.y),
                        margin: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                ));
            }
        });
}

/// The data-bound case: every shape's fill changes every frame, exactly what a
/// `Fill="{Binding ...}"` does when its source animates.
fn animate_fills(config: Res<Config>, time: Res<Time>, mut shapes: Query<&mut PfShape>) {
    if !config.animate {
        return;
    }
    let t = time.elapsed_secs();
    for (i, mut shape) in shapes.iter_mut().enumerate() {
        let phase = t + i as f32 * 0.05;
        shape.fill = Some(v::PfBrush::Solid(v::PfColor {
            r: ((phase.sin() * 0.5 + 0.5) * 255.0) as u8,
            g: 128,
            b: ((phase.cos() * 0.5 + 0.5) * 255.0) as u8,
            a: 255,
        }));
    }
}

fn sample_frames(
    diagnostics: Res<DiagnosticsStore>,
    config: Res<Config>,
    mut samples: ResMut<Samples>,
    mut exit: MessageWriter<AppExit>,
) {
    samples.frames += 1;
    if samples.frames <= config.warmup {
        return; // warmup
    }
    if let Some(ms) = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.value())
    {
        samples.ms.push(ms as f32);
    }
    if samples.ms.len() < config.frames {
        return;
    }
    let mut sorted = samples.ms.clone();
    sorted.sort_by(f32::total_cmp);
    let pct = |p: f32| sorted[((sorted.len() - 1) as f32 * p) as usize];
    println!(
        "backend={} shapes={} animate={} size={}x{} complex={} frames={}  frame_ms p50={:.3} p95={:.3} p99={:.3}",
        config.backend.name(),
        config.shapes,
        config.animate,
        config.size.x,
        config.size.y,
        config.complex,
        config.frames,
        pct(0.50),
        pct(0.95),
        pct(0.99),
    );
    exit.write(AppExit::Success);
}
