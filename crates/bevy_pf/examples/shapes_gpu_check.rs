//! Visual check for the `vector_gpu` shape backend.
//!
//! Spawns one UI node per shape kind — plain and rounded rectangles, an
//! ellipse, a line, a polyline, an SVG path, a gradient fill, and a dashed
//! stroke — laid out by bevy_ui exactly as XAML shapes are, so the whole
//! chain is exercised: `ComputedNode` size -> engine geometry -> atlas slot
//! -> `ImageNode` sampling that slot.
//!
//!   cargo run -p bevy_pf --features vector_gpu --example shapes_gpu_check
//!   cargo run -p bevy_pf --features vector_gpu --example shapes_gpu_check -- --screenshot
//!
//! Run it after changing either backend: if a shape is missing, mispositioned,
//! flipped in Y, or clipped at its slot edge, it shows up immediately here.

use bevy::prelude::*;
use bevy_pf::shapes::{PfShape, ShapeGeometry};
use bevy_pf_xaml::geometry::{PathData, PathFigure, PathSegment};
use bevy_pf_xaml::value as v;

fn rgb(r: u8, g: u8, b: u8) -> v::PfBrush {
    v::PfBrush::Solid(v::PfColor { r, g, b, a: 255 })
}

fn main() {
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(0.04, 0.05, 0.07)))
        .add_plugins(DefaultPlugins)
        .add_plugins(bevy_pf::shapes_gpu::PfShapeGpuPlugin)
        .add_systems(Startup, setup);
    if std::env::args().any(|a| a == "--screenshot") {
        app.add_systems(Update, screenshot_and_exit);
    }
    if std::env::args().any(|a| a == "--stress") {
        app.add_systems(Update, stress_across_frames);
    }
    app.run();
}

fn specimens() -> Vec<(&'static str, PfShape)> {
    let mut out: Vec<(&'static str, PfShape)> = Vec::new();

    let mut rect = PfShape::new(ShapeGeometry::Rectangle {
        radius_x: 0.0,
        radius_y: 0.0,
    });
    rect.fill = Some(rgb(0x2F, 0xE9, 0xFF));
    out.push(("rect", rect));

    let mut rounded = PfShape::new(ShapeGeometry::Rectangle {
        radius_x: 18.0,
        radius_y: 18.0,
    });
    rounded.fill = Some(rgb(0xFF, 0x2F, 0x9D));
    rounded.stroke = Some(rgb(0xFF, 0xFF, 0xFF));
    rounded.stroke_thickness = 3.0;
    out.push(("rounded+stroke", rounded));

    let mut ellipse = PfShape::new(ShapeGeometry::Ellipse);
    ellipse.fill = Some(rgb(0x7A, 0xE5, 0x82));
    out.push(("ellipse", ellipse));

    // Gradient fill: WPF gradient coords are fractions of the shape box.
    let mut gradient = PfShape::new(ShapeGeometry::Rectangle {
        radius_x: 6.0,
        radius_y: 6.0,
    });
    gradient.fill = Some(v::PfBrush::LinearGradient {
        start: v::Point::new(0.0, 0.0),
        end: v::Point::new(1.0, 1.0),
        stops: vec![
            v::GradientStop {
                offset: 0.0,
                color: v::PfColor {
                    r: 0x2F,
                    g: 0xE9,
                    b: 0xFF,
                    a: 255,
                },
            },
            v::GradientStop {
                offset: 1.0,
                color: v::PfColor {
                    r: 0xFF,
                    g: 0x2F,
                    b: 0x9D,
                    a: 255,
                },
            },
        ],
    });
    out.push(("gradient", gradient));

    let mut line = PfShape::new(ShapeGeometry::Line {
        x1: 8.0,
        y1: 8.0,
        x2: 112.0,
        y2: 88.0,
    });
    line.stroke = Some(rgb(0xFF, 0xD8, 0x3A));
    line.stroke_thickness = 4.0;
    out.push(("line", line));

    let mut dashed = PfShape::new(ShapeGeometry::Line {
        x1: 8.0,
        y1: 48.0,
        x2: 112.0,
        y2: 48.0,
    });
    dashed.stroke = Some(rgb(0xFF, 0xFF, 0xFF));
    dashed.stroke_thickness = 4.0;
    dashed.stroke_dash_array = vec![2.0, 1.5];
    out.push(("dashed", dashed));

    let mut poly = PfShape::new(ShapeGeometry::Polyline {
        points: vec![
            v::Point::new(10.0, 80.0),
            v::Point::new(40.0, 16.0),
            v::Point::new(70.0, 80.0),
            v::Point::new(100.0, 16.0),
        ],
        closed: false,
    });
    poly.stroke = Some(rgb(0x2F, 0xE9, 0xFF));
    poly.stroke_thickness = 3.0;
    out.push(("polyline", poly));

    // A closed path with a cubic and an arc, to cover both curve conversions.
    let data = PathData {
        fill_rule: bevy_pf_xaml::geometry::FillRule::NonZero,
        figures: vec![PathFigure {
            start: v::Point::new(10.0, 80.0),
            segments: vec![
                PathSegment::Cubic(
                    v::Point::new(30.0, 10.0),
                    v::Point::new(70.0, 10.0),
                    v::Point::new(90.0, 80.0),
                ),
                PathSegment::Arc {
                    radii: v::Point::new(40.0, 30.0),
                    rotation: 0.0,
                    large_arc: false,
                    sweep: false,
                    to: v::Point::new(10.0, 80.0),
                },
            ],
            closed: true,
        }],
    };
    let mut path = PfShape::new(ShapeGeometry::Path(data));
    path.fill = Some(rgb(0xB0, 0x8C, 0xFF));
    path.stroke = Some(rgb(0xFF, 0xFF, 0xFF));
    path.stroke_thickness = 2.0;
    out.push(("path", path));

    // A SQUARE ellipse and a TINY one. Both matter and neither was covered:
    // `as_rounded_box` sends a square ellipse down the SDF fast path (radius
    // = half the side, i.e. a circle) while a non-square one stays
    // tessellated — and the only ellipse specimen here was oval, so the SDF
    // path was never rendered by this gallery at all. The game's status dot
    // is a 4x4 ellipse and came out square.
    let mut round = PfShape::new(ShapeGeometry::Ellipse);
    round.fill = Some(rgb(0x4F, 0xD8, 0xFF));
    out.push(("circle-sdf", round));

    let mut dot = PfShape::new(ShapeGeometry::Ellipse);
    dot.fill = Some(rgb(0x7F, 0xFF, 0xD4));
    out.push(("dot-tiny", dot));

    // A very low alpha fill, which is what the game's button interiors use
    // (0x02 over the scene) — fitted because bevy composites in linear
    // space. It vanished entirely on the GPU path.
    let mut faint = PfShape::new(ShapeGeometry::Rectangle {
        radius_x: 0.0,
        radius_y: 0.0,
    });
    faint.fill = Some(v::PfBrush::Solid(v::PfColor {
        r: 0x00,
        g: 0xFF,
        b: 0xD4,
        a: 0x40,
    }));
    faint.stroke = Some(rgb(0x00, 0xFF, 0xD4));
    faint.stroke_thickness = 1.0;
    out.push(("faint-fill", faint));

    // Stroke-only + Stretch=Fill: what the obsidian chrome's bevel passes
    // are (BevelMid/BevelBloom). Regression specimen — these vanished once
    // the GPU backend took over.
    let outline = PathData {
        fill_rule: bevy_pf_xaml::geometry::FillRule::NonZero,
        figures: vec![PathFigure {
            start: v::Point::new(14.0, 0.0),
            segments: vec![
                PathSegment::Line(v::Point::new(100.0, 0.0)),
                PathSegment::Line(v::Point::new(100.0, 86.0)),
                PathSegment::Line(v::Point::new(86.0, 100.0)),
                PathSegment::Line(v::Point::new(0.0, 100.0)),
                PathSegment::Line(v::Point::new(0.0, 14.0)),
            ],
            closed: true,
        }],
    };
    let mut bevel = PfShape::new(ShapeGeometry::Path(outline));
    bevel.stroke = Some(rgb(0x2F, 0xE9, 0xFF));
    bevel.stroke_thickness = 1.5;
    bevel.stretch = v::Stretch::Fill;
    out.push(("stroke+stretch", bevel));

    out
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            padding: UiRect::all(Val::Px(24.0)),
            column_gap: Val::Px(16.0),
            row_gap: Val::Px(16.0),
            flex_wrap: FlexWrap::Wrap,
            ..default()
        })
        .with_children(|parent| {
            // --stress runs the specimen set REPEATED, because slot
            // reclamation only misbehaves at scale: the game carries ~148
            // shapes and exhausted the atlas 1,445 times in 35 seconds,
            // while five shapes sweeping their size never even filled it.
            // A leak per despawn is invisible until there are enough
            // shapes to run the packer out before a rebuild reclaims.
            let repeats: usize = std::env::args()
                .position(|a| a == "--repeat")
                .and_then(|i| std::env::args().nth(i + 1))
                .and_then(|v| v.parse().ok())
                .unwrap_or(if std::env::args().any(|a| a == "--stress") {
                    20
                } else {
                    1
                });
            for (label, shape) in (0..repeats).flat_map(|_| specimens()) {
                parent
                    .spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(6.0),
                        ..default()
                    })
                    .with_children(|cell| {
                        let (w, h) = match label {
                            // Square, so the SDF path is taken.
                            "circle-sdf" => (96.0, 96.0),
                            // The game's status dot, to the pixel.
                            "dot-tiny" => (4.0, 4.0),
                            _ => (120.0, 96.0),
                        };
                        cell.spawn((
                            shape,
                            Node {
                                width: Val::Px(w),
                                height: Val::Px(h),
                                ..default()
                            },
                        ));
                        cell.spawn((
                            Text::new(label),
                            TextFont {
                                font_size: FontSize::Px(12.0),
                                ..default()
                            },
                            TextColor(Color::srgb(0.7, 0.75, 0.8)),
                        ));
                    });
            }
        });
}

fn screenshot_and_exit(
    mut frame: Local<u32>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    *frame += 1;
    if *frame == 60 {
        commands
            .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
            .observe(bevy::render::view::screenshot::save_to_disk(
                "shapes_gpu_check.png",
            ));
    }
    if *frame == 400 {
        exit.write(AppExit::Success);
    }
}

/// Exercise the backend ACROSS FRAMES, which is what a settled screenshot
/// cannot do.
///
/// Every regression this backend has shipped was a time-domain bug -- shapes
/// vanishing a few frames after being drawn, chrome blinking, layout shifting
/// on remount -- and every one of them survived a check that rendered one
/// frame and looked at it. Two things move here:
///
///   * RESIZE: slots are reserved on a 16px grain and mutated in place while
///     the shape still fits, so sweeping the size crosses grain boundaries and
///     forces genuine re-reservations.
///   * REMOUNT: despawning and respawning a shape must return its slot to a
///     working state, not leak it.
///
/// The assertion is the atlas REBUILD COUNT. A rebuild drops every
/// reservation, so shapes have no slot for a frame -- the blink. A couple
/// during warmup is fine; a count that climbs with frames means thrashing.
/// Frame time cannot see this: a thrashing atlas draws less and can measure
/// FASTER than a healthy one.
fn stress_across_frames(
    mut frame: Local<u32>,
    mut nodes: Query<&mut Node, With<PfShape>>,
    shapes: Query<Entity, With<PfShape>>,
    slotted: Query<Entity, With<bevy_pf::shapes_gpu::PfShapeGpu>>,
    rebuilds: Res<bevy_pf::shapes_gpu::PfAtlasRebuilds>,
    mut warmup_rebuilds: Local<Option<u32>>,
    mut late_rebuilds: Local<Option<u32>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    *frame += 1;
    const WARMUP: u32 = 60;
    const TOTAL: u32 = 600;

    // Baseline after the working set settles, so startup rebuilds are not
    // counted against the steady state.
    if *frame == WARMUP {
        *warmup_rebuilds = Some(rebuilds.0);
    }
    // A SECOND baseline, two thirds in. Capacities converge as shapes grow
    // into their largest size and then stay inside it, so the interesting
    // question is not "how many rebuilds in total" but "does it go quiet".
    // A count that keeps climbing after convergence is a leak; one that
    // stops is just the cost of growing into the working set.
    if *frame == TOTAL - 180 {
        *late_rebuilds = Some(rebuilds.0);
    }

    // Sweep every shape's size continuously.
    if *frame > WARMUP {
        let t = (*frame - WARMUP) as f32 / 30.0;
        let w = 120.0 + 60.0 * t.sin();
        let h = 96.0 + 40.0 * (t * 0.7).cos();
        for mut node in &mut nodes {
            node.width = Val::Px(w);
            node.height = Val::Px(h);
        }
    }

    // Remount one shape periodically: despawn it and let the next frames
    // re-register a fresh slot.
    if *frame > WARMUP && *frame % 120 == 0 {
        if let Some(entity) = shapes.iter().next() {
            commands.entity(entity).despawn();
        }
    }

    if *frame >= TOTAL {
        let baseline = warmup_rebuilds.unwrap_or(0);
        let steady = rebuilds.0.saturating_sub(baseline);
        let frames = TOTAL - WARMUP;
        println!(
            "stress: frames={frames} shapes={} slotted={} rebuilds_total={} rebuilds_steady={steady}",
            shapes.iter().count(),
            slotted.iter().count(),
            rebuilds.0,
        );
        // THE ASSERTION IS CONVERGENCE, NOT A TOTAL.
        //
        // Growing into the working set genuinely costs rebuilds: every shape
        // climbs through capacities before it settles into the largest size
        // it will hold, and while that is happening the packer can run out.
        // What must NOT happen is rebuilds continuing after the sizes have
        // settled — that is the leak this harness exists to catch, and it is
        // what "the borders vanished" looked like from the outside.
        //
        // So the budget scales with the working set (the old flat `frames /
        // 60` was written when this test had five shapes and passed a leak
        // that cost the game 1,445 exhaustions in 35 seconds), and the real
        // check is the LATE window below, which must be exactly zero.
        let budget = (frames / 60).max(shapes.iter().count() as u32 / 4);
        if steady > budget {
            println!(
                "stress: FAIL -- {steady} rebuilds in steady state exceeds budget {budget}; atlas is thrashing"
            );
            std::process::exit(1);
        }
        // THE REAL CHECK: once sizes have settled, the packer must be silent.
        let late = rebuilds
            .0
            .saturating_sub(late_rebuilds.unwrap_or(rebuilds.0));
        println!("stress: rebuilds in the last 180 frames = {late}");
        if late > 0 {
            println!(
                "stress: FAIL -- {late} rebuilds AFTER the working set settled; \
                 slots are leaking rather than being reused"
            );
            std::process::exit(1);
        }
        if slotted.iter().count() == 0 {
            println!("stress: FAIL -- no shape holds an atlas slot at exit");
            std::process::exit(1);
        }
        println!("stress: PASS");
        exit.write(AppExit::Success);
    }
}
