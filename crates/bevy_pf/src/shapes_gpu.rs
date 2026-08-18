//! GPU shape backend: WPF shapes rendered by `bevy_pf_vector` instead of
//! CPU-rasterized with tiny-skia.
//!
//! # Why an atlas rather than a texture per shape
//!
//! The engine's whole thesis is tessellate-once-and-instance: geometry is
//! tessellated on first sight, keyed by content hash, and every later frame
//! costs one instance write. A render target per shape would mean a render
//! pass per shape and would throw that away — it would be *slower* than the
//! CPU path it replaces.
//!
//! So every shape draws into ONE shared atlas texture, in a single instanced
//! pass, through an offscreen camera on a dedicated render layer. Each UI node
//! keeps an `ImageNode`, but pointed at its slot in that atlas via
//! [`TextureAtlas`]. bevy_ui therefore keeps owning layout, compositing,
//! `Overflow::Clip` and z-order — the parts that are not worth reimplementing
//! and that a "draw the UI ourselves" approach would break.
//!
//! # What this actually buys
//!
//! Static shapes were already cheap: the CPU path caches by pixel size, so a
//! shape that never resizes never re-rasterized. The win is *dynamic* shapes.
//! A `Fill`/`Stroke` bound to a view model previously re-rasterized on the CPU
//! and allocated a fresh `Image` asset — a full texture re-upload — on every
//! change. Here colour is per-instance: a colour change re-tessellates
//! nothing, uploads no texture, and costs one 56-byte instance write.
//!
//! Shapes that cannot get an atlas slot (atlas full) fall back to the CPU
//! rasterizer, so this is never worse than not having it.

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, RenderTarget};
use bevy::image::{TextureAtlas, TextureAtlasLayout};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::ui::ComputedNode;
use bevy::ui::widget::{ImageNode, NodeImageMode};
use bevy_pf_vector::{
    Brush, DashPattern, FillRule as VFillRule, GradientStop, HudTransform, LineCap, LineJoin,
    PathCommand, PathStyle, StrokeStyle, VectorPrimitive, VectorShape,
};
use bevy_pf_xaml::geometry::{FillRule, PathData, PathSegment};
use bevy_pf_xaml::value as v;

use crate::shapes::{PfShape, PfShapeClaim, PfShapeRendered, ShapeGeometry, arc_to_cubics};

/// Render layer the shape atlas camera draws, kept clear of app content.
const SHAPE_LAYER: usize = 24;

/// Atlas edge length. 2048² holds a lot of HUD chrome; overflow falls back to
/// the CPU rasterizer rather than failing to draw.
const ATLAS_SIZE: u32 = 2048;

/// Gap between packed slots so a neighbour's antialiased fringe can never
/// bleed into this slot when bevy_ui samples it with filtering.
const SLOT_PADDING: u32 = 2;

/// Shared atlas, its layout asset, and the shelf allocator that packs it.
#[derive(Resource)]
pub struct PfShapeAtlas {
    pub image: Handle<Image>,
    pub layout: Handle<TextureAtlasLayout>,
    /// Next free x within the current shelf.
    cursor: UVec2,
    /// Height of the current shelf.
    shelf_height: u32,
    /// Regions returned by shapes that outgrew them, keyed by capacity.
    ///
    /// Shelf packing cannot reclaim an arbitrary hole, which is why released
    /// regions are only ever handed back for the SAME capacity. That is enough
    /// because capacities are quantized to a 16px grain, so a shape whose size
    /// oscillates cycles through a handful of distinct capacities and reuses
    /// them instead of consuming the atlas.
    free: std::collections::HashMap<UVec2, Vec<UVec2>>,
}

impl PfShapeAtlas {
    /// Reserve a `size` slot, returning its origin. Shelf packing: shapes are
    /// laid in rows, a new row starting when the current one runs out of
    /// width. Slots are never freed — a resized shape takes a new slot and
    /// the old one is abandoned — so the atlas is rebuilt wholesale when it
    /// fills rather than fragmenting.
    /// Reserve a region, returning where it starts and how big the region
    /// ACTUALLY is — which may be larger than asked for when an existing
    /// free region is recycled.
    ///
    /// BEST FIT, not exact fit. Exact-match reuse only helps a shape that
    /// returns to a size it held before; a shape sweeping through sizes
    /// leaves a trail of regions at capacities nobody asks for again, and
    /// the cursor marches on regardless. With 176 shapes sweeping together
    /// that still cost 226 rebuilds over 540 frames even after departing
    /// shapes started returning their slots. Accepting any free region big
    /// enough turns that trail back into supply.
    ///
    /// The caller must store the returned capacity, not the requested one,
    /// or the region comes back to the wrong bucket on release and the
    /// pool corrupts.
    fn allocate(&mut self, size: UVec2) -> Option<(UVec2, UVec2)> {
        if let Some(origins) = self.free.get_mut(&size)
            && let Some(origin) = origins.pop()
        {
            return Some((origin, size));
        }
        // Smallest region that still fits, so a 2048-wide leftover is not
        // spent on a 16px dot while a wide bar waits behind it.
        let best = self
            .free
            .iter()
            .filter(|(cap, origins)| !origins.is_empty() && cap.x >= size.x && cap.y >= size.y)
            .min_by_key(|(cap, _)| cap.x as u64 * cap.y as u64)
            .map(|(cap, _)| *cap);
        if let Some(cap) = best
            && let Some(origins) = self.free.get_mut(&cap)
            && let Some(origin) = origins.pop()
        {
            return Some((origin, cap));
        }
        let step = size + UVec2::splat(SLOT_PADDING);
        if step.x > ATLAS_SIZE || step.y > ATLAS_SIZE {
            return None;
        }
        if self.cursor.x + step.x > ATLAS_SIZE {
            self.cursor.x = 0;
            self.cursor.y += self.shelf_height;
            self.shelf_height = 0;
        }
        if self.cursor.y + step.y > ATLAS_SIZE {
            return None;
        }
        let origin = self.cursor;
        self.cursor.x += step.x;
        self.shelf_height = self.shelf_height.max(step.y);
        Some((origin, size))
    }

    /// Hand a region back for reuse at the same capacity.
    fn release(&mut self, capacity: UVec2, origin: UVec2) {
        self.free.entry(capacity).or_default().push(origin);
    }

    fn reset(&mut self) {
        self.cursor = UVec2::ZERO;
        self.shelf_height = 0;
        // The regions these point at no longer exist after a rebuild.
        self.free.clear();
    }
}

/// The atlas slot a shape currently occupies, plus the draw entity rendering
/// into it.
/// RECLAIMED ON REMOVAL, via the hook below.
///
/// Slots used to come back only through a wholesale atlas rebuild. That is
/// fine while shapes merely resize — the reservation is mutated in place —
/// but a shape that goes AWAY took its region with it: panels open and
/// close, templates re-expand, and every departed shape leaked a slot until
/// the packer ran out and dropped every reservation at once.
///
/// It hid at small scale. Five shapes sweeping their size never filled a
/// 2048² atlas, so the stress harness passed; the game carries ~148 and
/// exhausted it 1,445 times in 35 seconds, which is what erased the console
/// button chrome. The harness now runs the specimen set x20 and reproduces
/// it: 500 rebuilds against a budget of 9, with no shape holding a slot at
/// the end.
#[derive(Component, Debug, Clone)]
#[component(on_remove = release_slot)]
pub struct PfShapeGpu {
    /// Index into the atlas layout. Its rect is mutated in place when the
    /// shape resizes within its slot, so the `ImageNode` never gets rebuilt.
    index: usize,
    /// Top-left of the reserved region, in atlas pixels.
    origin: UVec2,
    /// Reserved region size, >= the drawn size (see [`slot_capacity`]).
    capacity: UVec2,
    /// Pixel size currently drawn.
    size: UVec2,
    /// The entity drawing into the slot.
    draw: Entity,
    /// Second pass for a shape that has BOTH fill and stroke on the SDF
    /// path, where each is its own instance.
    draw_stroke: Option<Entity>,
}

/// Hand a departing shape's atlas region back to the packer.
///
/// A component hook rather than a `RemovedComponents` system: despawns and
/// removals both land here, in the same command flush that did them, so a
/// slot can never outlive the shape that held it. The draw entities go too
/// — they are the instances rendering into that region, and leaving them
/// would keep painting a slot the packer has already re-let.
fn release_slot(
    mut world: bevy::ecs::world::DeferredWorld,
    ctx: bevy::ecs::lifecycle::HookContext,
) {
    let Some(gpu) = world.get::<PfShapeGpu>(ctx.entity).cloned() else {
        return;
    };
    if let Some(mut atlas) = world.get_resource_mut::<PfShapeAtlas>() {
        atlas.release(gpu.capacity, gpu.origin);
    }
    let mut commands = world.commands();
    commands.entity(gpu.draw).try_despawn();
    if let Some(stroke) = gpu.draw_stroke {
        commands.entity(stroke).try_despawn();
    }
}

/// Round a reservation up so small size changes reuse the same region.
/// Without this a shape whose width is data-bound — a progress bar, a meter —
/// would burn a fresh slot every frame and exhaust the atlas in seconds.
fn slot_capacity(px: UVec2) -> UVec2 {
    const GRAIN: u32 = 16;
    UVec2::new(px.x.div_ceil(GRAIN) * GRAIN, px.y.div_ceil(GRAIN) * GRAIN)
}

/// Set when a reservation fails; drives a wholesale atlas rebuild rather than
/// letting the packer fragment.
#[derive(Resource, Default)]
struct PfAtlasFull(bool);

/// How many wholesale atlas rebuilds have happened.
///
/// A rebuild drops every reservation, so each one costs a frame in which
/// shapes have no slot -- which is what "blinking" looked like in the game.
/// One or two during startup is normal as the working set settles; a counter
/// that keeps climbing means the atlas is thrashing and the content does not
/// fit. Exposed because frame-time percentiles CANNOT see this: a thrashing
/// atlas can measure faster than a healthy one while drawing less.
#[derive(Resource, Default)]
pub struct PfAtlasRebuilds(pub u32);

/// Marker for the atlas camera so its activity can be gated.
#[derive(Component)]
struct PfShapeAtlasCamera;

/// Frames the atlas camera stays active after the last shape edit.
///
/// CURRENTLY UNUSED — see `gate_atlas_camera`. Kept because the idea is
/// right and only the mechanism was wrong.
#[derive(Resource, Default)]
struct PfAtlasDirty(u8);

pub struct PfShapeGpuPlugin;

impl Plugin for PfShapeGpuPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<bevy_pf_vector::PfVectorPlugin>() {
            app.add_plugins(bevy_pf_vector::PfVectorPlugin);
        }
        // Claim what the atlas can render (see shapes.rs module docs for the
        // backend contract); unclaimed shapes fall through to the CPU
        // rasterizer. When bevy_ui native styling is also compiled in, it
        // claims first — a free bevy_ui node beats an atlas slot.
        let claim = (sync_gpu_shapes, rebuild_atlas_if_full, gate_atlas_camera)
            .chain()
            .in_set(crate::shapes::PfShapeSystems::Claim);
        #[cfg(feature = "native_shapes")]
        let claim = claim.after(crate::shapes::style_native_shapes);
        app.init_resource::<PfAtlasFull>()
            .init_resource::<PfAtlasRebuilds>()
            .init_resource::<PfAtlasDirty>()
            .add_systems(Startup, setup_atlas)
            .add_systems(PostUpdate, claim);
    }
}

fn setup_atlas(
    mut commands: Commands,
    images: Option<ResMut<Assets<Image>>>,
    layouts: Option<ResMut<Assets<TextureAtlasLayout>>>,
) {
    // Headless apps have no image/atlas asset collections. Without this the
    // system fails parameter validation and takes the process down, so simply
    // ADDING this plugin broke any test that did not have a renderer.
    let (Some(mut images), Some(mut layouts)) = (images, layouts) else {
        return;
    };
    let mut image = Image::new_fill(
        Extent3d {
            width: ATLAS_SIZE,
            height: ATLAS_SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    let image = images.add(image);
    let layout = layouts.add(TextureAtlasLayout::new_empty(UVec2::splat(ATLAS_SIZE)));

    // Offscreen camera: renders only the shape layer, into the atlas, with a
    // transparent clear. Msaa off — the engine antialiases analytically, and
    // the atlas is sampled 1:1 by bevy_ui anyway.
    let mut projection = OrthographicProjection::default_2d();
    projection.scaling_mode = bevy::camera::ScalingMode::Fixed {
        width: ATLAS_SIZE as f32,
        height: ATLAS_SIZE as f32,
    };
    commands.spawn((
        Camera2d,
        Camera {
            clear_color: ClearColorConfig::Custom(Color::NONE),
            // Behind every on-screen camera: this only fills a texture.
            order: -100,
            ..default()
        },
        // In bevy 0.19 the render target is its own component.
        RenderTarget::Image(image.clone().into()),
        Projection::Orthographic(projection),
        bevy::render::view::Msaa::Off,
        RenderLayers::layer(SHAPE_LAYER),
        PfShapeAtlasCamera,
        Name::new("PfShapeAtlasCamera"),
    ));

    commands.insert_resource(PfShapeAtlas {
        image,
        layout,
        cursor: UVec2::ZERO,
        shelf_height: 0,
        free: Default::default(),
    });
}

/// Atlas pixel rect -> world position for the atlas camera, whose world
/// origin is the atlas centre with +Y up.
fn slot_center_world(origin: UVec2, size: UVec2) -> Vec2 {
    let half = ATLAS_SIZE as f32 * 0.5;
    Vec2::new(
        origin.x as f32 + size.x as f32 * 0.5 - half,
        half - (origin.y as f32 + size.y as f32 * 0.5),
    )
}

/// Layout space (+Y down, origin top-left of the node) -> engine local space
/// (+Y up, origin at the shape's centre).
fn to_local(x: f32, y: f32, size: Vec2) -> Vec2 {
    Vec2::new(x - size.x * 0.5, size.y * 0.5 - y)
}

fn to_color(c: v::PfColor) -> LinearRgba {
    Color::srgba_u8(c.r, c.g, c.b, c.a).to_linear()
}

fn to_brush(brush: &v::PfBrush, size: Vec2) -> Brush {
    let stops = |stops: &Vec<v::GradientStop>| -> Vec<GradientStop> {
        stops
            .iter()
            .map(|s| GradientStop {
                offset: s.offset.clamp(0.0, 1.0),
                color: to_color(s.color),
            })
            .collect()
    };
    match brush {
        v::PfBrush::Solid(c) => Brush::Solid(to_color(*c)),
        // WPF gradient coordinates are fractions of the shape's box.
        v::PfBrush::LinearGradient {
            start,
            end,
            stops: s,
        } => Brush::Linear {
            start: to_local(start.x * size.x, start.y * size.y, size),
            end: to_local(end.x * size.x, end.y * size.y, size),
            stops: stops(s),
        },
        v::PfBrush::RadialGradient {
            center,
            radius_x,
            radius_y,
            stops: s,
        } => Brush::Radial {
            center: to_local(center.x * size.x, center.y * size.y, size),
            // The engine's radial is circular; match tiny-skia's collapse of
            // the WPF ellipse to its larger axis.
            radius: (radius_x * size.x).max(radius_y * size.y).max(1.0),
            stops: stops(s),
        },
    }
}

/// A rounded rectangle as a closed path, matching the CPU backend's arc
/// approximation exactly so the two look identical.
fn rounded_rect(l: f32, t: f32, r: f32, b: f32, rx: f32, ry: f32, size: Vec2) -> Vec<PathCommand> {
    let k = 0.5522848;
    let p = |x: f32, y: f32| to_local(x, y, size);
    vec![
        PathCommand::MoveTo(p(l + rx, t)),
        PathCommand::LineTo(p(r - rx, t)),
        PathCommand::CubicTo {
            ctrl1: p(r - rx + k * rx, t),
            ctrl2: p(r, t + ry - k * ry),
            to: p(r, t + ry),
        },
        PathCommand::LineTo(p(r, b - ry)),
        PathCommand::CubicTo {
            ctrl1: p(r, b - ry + k * ry),
            ctrl2: p(r - rx + k * rx, b),
            to: p(r - rx, b),
        },
        PathCommand::LineTo(p(l + rx, b)),
        PathCommand::CubicTo {
            ctrl1: p(l + rx - k * rx, b),
            ctrl2: p(l, b - ry + k * ry),
            to: p(l, b - ry),
        },
        PathCommand::LineTo(p(l, t + ry)),
        PathCommand::CubicTo {
            ctrl1: p(l, t + ry - k * ry),
            ctrl2: p(l + rx - k * rx, t),
            to: p(l + rx, t),
        },
        PathCommand::Close,
    ]
}

fn path_data_commands(data: &PathData, size: Vec2) -> Vec<PathCommand> {
    let p = |pt: v::Point| to_local(pt.x, pt.y, size);
    let mut out = Vec::new();
    for figure in &data.figures {
        out.push(PathCommand::MoveTo(p(figure.start)));
        let mut cursor = figure.start;
        for segment in &figure.segments {
            match segment {
                PathSegment::Line(to) => {
                    out.push(PathCommand::LineTo(p(*to)));
                    cursor = *to;
                }
                PathSegment::Cubic(c1, c2, to) => {
                    out.push(PathCommand::CubicTo {
                        ctrl1: p(*c1),
                        ctrl2: p(*c2),
                        to: p(*to),
                    });
                    cursor = *to;
                }
                PathSegment::Quadratic(c, to) => {
                    out.push(PathCommand::QuadTo {
                        ctrl: p(*c),
                        to: p(*to),
                    });
                    cursor = *to;
                }
                PathSegment::Arc {
                    radii,
                    rotation,
                    large_arc,
                    sweep,
                    to,
                } => {
                    // Reuse the CPU backend's endpoint->centre arc conversion
                    // so both backends draw the same curve.
                    for (c1, c2, end) in
                        arc_to_cubics(cursor, *radii, *rotation, *large_arc, *sweep, *to)
                    {
                        out.push(PathCommand::CubicTo {
                            ctrl1: p(c1),
                            ctrl2: p(c2),
                            to: p(end),
                        });
                    }
                    cursor = *to;
                }
            }
        }
        if figure.closed {
            out.push(PathCommand::Close);
        }
    }
    out
}

/// Axis-aligned bounds of a command list's control points — the same
/// conservative box the CPU backend stretches by.
fn commands_bounds(commands: &[PathCommand]) -> Option<(Vec2, Vec2)> {
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    let mut any = false;
    let mut visit = |p: Vec2| {
        min = min.min(p);
        max = max.max(p);
        any = true;
    };
    for command in commands {
        match command {
            PathCommand::MoveTo(p) | PathCommand::LineTo(p) => visit(*p),
            PathCommand::QuadTo { ctrl, to } => {
                visit(*ctrl);
                visit(*to);
            }
            PathCommand::CubicTo { ctrl1, ctrl2, to } => {
                visit(*ctrl1);
                visit(*ctrl2);
                visit(*to);
            }
            PathCommand::Close => {}
        }
    }
    any.then_some((min, max))
}

fn map_commands(commands: &mut [PathCommand], f: impl Fn(Vec2) -> Vec2) {
    for command in commands {
        match command {
            PathCommand::MoveTo(p) | PathCommand::LineTo(p) => *p = f(*p),
            PathCommand::QuadTo { ctrl, to } => {
                *ctrl = f(*ctrl);
                *to = f(*to);
            }
            PathCommand::CubicTo { ctrl1, ctrl2, to } => {
                *ctrl1 = f(*ctrl1);
                *ctrl2 = f(*ctrl2);
                *to = f(*to);
            }
            PathCommand::Close => {}
        }
    }
}

/// Build the engine geometry + style for a shape laid out at `px` pixels.
/// Mirrors `shapes::rasterize_shape` step for step so the backends agree.
pub fn shape_to_vector(shape: &PfShape, px: UVec2) -> Option<(Vec<PathCommand>, PathStyle)> {
    let size = Vec2::new(px.x as f32, px.y as f32);
    let (w, h) = (size.x, size.y);
    let st = shape.stroke_thickness;
    let inset = if shape.stroke.is_some() {
        st * 0.5
    } else {
        0.0
    };

    let (mut commands, rule) = match &shape.geometry {
        ShapeGeometry::Rectangle { radius_x, radius_y } => {
            let (l, t) = (inset, inset);
            let (r, b) = ((w - inset).max(inset + 0.1), (h - inset).max(inset + 0.1));
            let commands = if *radius_x > 0.0 || *radius_y > 0.0 {
                let rx = radius_x.min((r - l) / 2.0);
                let ry = radius_y.max(0.0).min((b - t) / 2.0);
                rounded_rect(l, t, r, b, rx, ry, size)
            } else {
                let p = |x: f32, y: f32| to_local(x, y, size);
                vec![
                    PathCommand::MoveTo(p(l, t)),
                    PathCommand::LineTo(p(r, t)),
                    PathCommand::LineTo(p(r, b)),
                    PathCommand::LineTo(p(l, b)),
                    PathCommand::Close,
                ]
            };
            (commands, VFillRule::NonZero)
        }
        ShapeGeometry::Ellipse => {
            let (l, t) = (inset, inset);
            let (r, b) = ((w - inset).max(inset + 0.1), (h - inset).max(inset + 0.1));
            // An oval is the rounded rect whose radii are half its extents.
            let (rx, ry) = ((r - l) * 0.5, (b - t) * 0.5);
            (rounded_rect(l, t, r, b, rx, ry, size), VFillRule::NonZero)
        }
        ShapeGeometry::Line { x1, y1, x2, y2 } => (
            vec![
                PathCommand::MoveTo(to_local(*x1, *y1, size)),
                PathCommand::LineTo(to_local(*x2, *y2, size)),
            ],
            VFillRule::NonZero,
        ),
        ShapeGeometry::Polyline { points, closed } => {
            let mut iter = points.iter();
            let first = iter.next()?;
            let mut commands = vec![PathCommand::MoveTo(to_local(first.x, first.y, size))];
            for p in iter {
                commands.push(PathCommand::LineTo(to_local(p.x, p.y, size)));
            }
            if *closed {
                commands.push(PathCommand::Close);
            }
            let rule = match shape.fill_rule {
                Some(FillRule::NonZero) => VFillRule::NonZero,
                _ => VFillRule::EvenOdd, // WPF default
            };
            (commands, rule)
        }
        ShapeGeometry::Path(data) => {
            let rule = match data.fill_rule {
                FillRule::EvenOdd => VFillRule::EvenOdd,
                FillRule::NonZero => VFillRule::NonZero,
            };
            (path_data_commands(data, size), rule)
        }
    };

    // Stretch, for coordinate geometries only (rect/ellipse already fill).
    let stretchable = !matches!(
        shape.geometry,
        ShapeGeometry::Rectangle { .. } | ShapeGeometry::Ellipse
    );
    if stretchable
        && shape.stretch != v::Stretch::None
        && let Some((min, max)) = commands_bounds(&commands)
    {
        let extent = (max - min).max(Vec2::splat(1e-3));
        let avail = Vec2::new((w - st).max(1.0), (h - st).max(1.0));
        let mut scale = avail / extent;
        match shape.stretch {
            v::Stretch::Uniform => scale = Vec2::splat(scale.x.min(scale.y)),
            v::Stretch::UniformToFill => scale = Vec2::splat(scale.x.max(scale.y)),
            _ => {}
        }
        // The CPU path works in layout space (+Y down) and lands the box at
        // (st/2, st/2). Here the geometry is already centred, so scale about
        // the box centre and re-centre — the same result, one step fewer.
        let centre = (min + max) * 0.5;
        let target = Vec2::new(0.0, 0.0);
        map_commands(&mut commands, |p| (p - centre) * scale + target);
    }

    let fill = shape.fill.as_ref().map(|b| to_brush(b, size));
    let stroke = shape.stroke.as_ref().map(|b| {
        let width = st.max(0.01);
        StrokeStyle {
            brush: to_brush(b, size),
            width,
            join: match shape.stroke_join {
                v::PenLineJoin::Miter => LineJoin::Miter,
                v::PenLineJoin::Bevel => LineJoin::Bevel,
                v::PenLineJoin::Round => LineJoin::Round,
            },
            cap: match shape.stroke_cap {
                v::PenLineCap::Flat => LineCap::Butt,
                v::PenLineCap::Square | v::PenLineCap::Triangle => LineCap::Square,
                v::PenLineCap::Round => LineCap::Round,
            },
            miter_limit: shape.stroke_miter_limit.max(1.0),
            dash: (!shape.stroke_dash_array.is_empty()).then(|| {
                // WPF dash units are multiples of the stroke thickness.
                let mut pattern: Vec<f32> = shape
                    .stroke_dash_array
                    .iter()
                    .map(|d| (d * width).max(0.01))
                    .collect();
                if pattern.len() % 2 != 0 {
                    let copy = pattern.clone();
                    pattern.extend(copy); // odd counts repeat, like WPF
                }
                DashPattern {
                    pattern,
                    offset: shape.stroke_dash_offset * width,
                }
            }),
        }
    });

    if fill.is_none() && stroke.is_none() {
        return None;
    }
    Some((
        commands,
        PathStyle {
            fill,
            stroke,
            fill_rule: rule,
        },
    ))
}

/// A rect/rounded-rect/ellipse reduces to a rounded box, which the engine
/// draws as an SDF primitive: one quad, and `size` lives in the instance.
///
/// This matters because it is what MOST UI chrome is. Sending those through
/// the tessellated path means every resize mints new geometry and
/// re-tessellates; as an SDF primitive a resize is one instance write. Only
/// genuinely arbitrary path data still needs tessellation, and that geometry
/// is fixed per screen — the case the tessellate-once cache is good at.
///
/// Returns (size, corner_radius) in the node's pixel space.
fn as_rounded_box(shape: &PfShape, px: UVec2) -> Option<(Vec2, f32)> {
    let size = Vec2::new(px.x as f32, px.y as f32);
    match &shape.geometry {
        ShapeGeometry::Rectangle { radius_x, radius_y } => {
            // The engine's SDF takes ONE radius; an elliptical corner would
            // have to stay on the tessellated path.
            if (radius_x - radius_y).abs() > 0.01 {
                return None;
            }
            Some((size, *radius_x))
        }
        // An ellipse inscribed in the box is the rounded box whose radius is
        // half the shortest side — exact only when the box is square, so a
        // non-square ellipse stays tessellated.
        ShapeGeometry::Ellipse => {
            if (size.x - size.y).abs() > 0.01 {
                return None;
            }
            Some((size, size.x * 0.5))
        }
        _ => None,
    }
}

/// Solid colour of a brush, if it is one. Gradients keep the tessellated
/// path, which already carries them per-instance.
fn solid(brush: &v::PfBrush) -> Option<Color> {
    match brush {
        v::PfBrush::Solid(c) => Some(Color::srgba_u8(c.r, c.g, c.b, c.a)),
        _ => None,
    }
}

/// Spawn the draw entities for a shape into its atlas slot: either SDF
/// primitives (fast path) or one tessellated `VectorShape`.
fn spawn_draws(
    commands: &mut Commands,
    shape: &PfShape,
    px: UVec2,
    origin: UVec2,
) -> (Entity, Option<Entity>) {
    let centre = slot_center_world(origin, px).extend(0.0);
    let transform = || HudTransform {
        translation: centre,
        ..default()
    };

    if let Some((size, radius)) = as_rounded_box(shape, px) {
        let fill = shape.fill.as_ref().and_then(solid);
        let stroke = shape.stroke.as_ref().and_then(solid);
        if fill.is_some() || stroke.is_some() {
            let st = shape.stroke_thickness;
            // Inset like the CPU backend: a stroke straddles the edge.
            let inset = if stroke.is_some() { st * 0.5 } else { 0.0 };
            let inner = (size - Vec2::splat(inset * 2.0)).max(Vec2::splat(0.1));
            let inner_radius = (radius - inset).max(0.0);

            let first = commands
                .spawn((
                    VectorPrimitive::Rect {
                        size: inner,
                        radius: inner_radius,
                        thickness: if fill.is_some() { 0.0 } else { st.max(0.01) },
                        color: fill.or(stroke).unwrap().to_linear(),
                    },
                    transform(),
                    RenderLayers::layer(SHAPE_LAYER),
                    Name::new("PfShapeSdf"),
                ))
                .id();
            // Both fill and stroke: the stroke is a second instance over it.
            let second = (fill.is_some() && stroke.is_some()).then(|| {
                commands
                    .spawn((
                        VectorPrimitive::Rect {
                            size: inner,
                            radius: inner_radius,
                            thickness: st.max(0.01),
                            color: stroke.unwrap().to_linear(),
                        },
                        HudTransform {
                            // Above the fill in the slot's local depth.
                            translation: centre + Vec3::new(0.0, 0.0, 1.0e-4),
                            ..default()
                        },
                        RenderLayers::layer(SHAPE_LAYER),
                        Name::new("PfShapeSdfStroke"),
                    ))
                    .id()
            });
            return (first, second);
        }
    }

    let (path, style) = shape_to_vector(shape, px)
        .unwrap_or_else(|| (Vec::new(), PathStyle::fill(LinearRgba::NONE)));
    let draw = commands
        .spawn((
            VectorShape {
                commands: path,
                style,
            },
            transform(),
            RenderLayers::layer(SHAPE_LAYER),
            Name::new("PfShapeDraw"),
        ))
        .id();
    (draw, None)
}

/// Give every laid-out shape an atlas slot and a `VectorShape` drawing into
/// it, and point its `ImageNode` at that slot.
#[allow(clippy::type_complexity)]
fn sync_gpu_shapes(
    mut shapes: Query<(
        Entity,
        Ref<PfShape>,
        &ComputedNode,
        Option<&mut PfShapeGpu>,
        Option<&PfShapeRendered>,
    )>,
    mut draws: Query<(&mut VectorShape, &mut HudTransform)>,
    atlas: Option<ResMut<PfShapeAtlas>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut full: ResMut<PfAtlasFull>,
    mut dirty: ResMut<PfAtlasDirty>,
    mut commands: Commands,
) {
    let Some(mut atlas) = atlas else { return };
    for (entity, shape, computed, gpu, cpu_rendered) in &mut shapes {
        let size = computed.size();
        let px = UVec2::new(size.x.round() as u32, size.y.round() as u32);
        if px.x == 0 || px.y == 0 {
            continue;
        }
        let resized = gpu.as_ref().is_none_or(|g| g.size != px);
        if !resized && !shape.is_changed() {
            continue;
        }
        let Some((_path, _style)) = shape_to_vector(&shape, px) else {
            continue;
        };
        let previous_draw: Vec<Entity> = gpu
            .as_ref()
            .map(|g| {
                [Some(g.draw), g.draw_stroke]
                    .into_iter()
                    .flatten()
                    .collect()
            })
            .unwrap_or_default();

        // Copied before the fast path moves `gpu`: if this shape ends up
        // re-reserving below, this is the region to hand back.
        let old_slot = gpu.as_ref().map(|g| (g.capacity, g.origin));

        // Fast path: the shape still fits its reservation, so the slot, the
        // layout index and the draw entity all stand. A colour-only edit
        // re-tessellates nothing (the engine hashes path content) and uploads
        // no texture — the case this backend exists for.
        if let Some(mut gpu) = gpu
            && px.cmple(gpu.capacity).all()
            && gpu.draw_stroke.is_none()
            && let Ok((mut vector, mut transform)) = draws.get_mut(gpu.draw)
        {
            let Some((path, style)) = shape_to_vector(&shape, px) else {
                continue;
            };
            vector.commands = path;
            vector.style = style;
            dirty.0 = 2;
            if gpu.size != px {
                gpu.size = px;
                transform.translation = slot_center_world(gpu.origin, px).extend(0.0);
                if let Some(mut layout) = layouts.get_mut(&atlas.layout)
                    && let Some(rect) = layout.textures.get_mut(gpu.index)
                {
                    *rect = URect::from_corners(gpu.origin, gpu.origin + px);
                }
            }
            continue;
        }

        // Falling through to here means the shape outgrew its reservation.
        // Give the old region back first, or it is leaked until the atlas
        // fills and rebuilds wholesale.
        if let Some((capacity, origin)) = old_slot {
            atlas.release(capacity, origin);
        }
        let wanted = slot_capacity(px);
        // `capacity` is what was actually reserved, which best-fit reuse may
        // widen; storing `wanted` here would release it to the wrong bucket.
        let Some((origin, capacity)) = atlas.allocate(wanted) else {
            // Out of room: rebuild the whole atlas next.
            full.0 = true;
            // AND HAND THE SHAPE BACK. The claim is what tells the CPU
            // rasterizer to keep its hands off (it queries
            // `Without<PfShapeClaim>`), so a shape that is claimed but has
            // no slot is drawn by NOBODY — it does not fall through, it
            // disappears. That is how the game lost every console-button
            // border while the labels stayed: the atlas was exhausted, the
            // claim stood, and the fallback this module's own header
            // promises ("never worse than not having it") never ran.
            //
            // Releasing the claim and the ImageNode puts the shape back in
            // the CPU path's query on the very next frame, so exhaustion
            // costs rasterization time instead of visible chrome.
            commands
                .entity(entity)
                .remove::<(PfShapeGpu, PfShapeClaim, ImageNode)>();
            continue;
        };
        let Some(mut layout) = layouts.get_mut(&atlas.layout) else {
            continue;
        };
        let index = layout.add_texture(URect::from_corners(origin, origin + px));

        // A grown shape abandons its old reservation; the rebuild reclaims it.
        for previous in previous_draw {
            commands.entity(previous).despawn();
        }
        let (draw, draw_stroke) = spawn_draws(&mut commands, &shape, px, origin);

        dirty.0 = 2;
        commands.entity(entity).insert((
            ImageNode::from_atlas_image(
                atlas.image.clone(),
                TextureAtlas {
                    layout: atlas.layout.clone(),
                    index,
                },
            )
            .with_mode(NodeImageMode::Stretch),
            PfShapeGpu {
                index,
                origin,
                capacity,
                size: px,
                draw,
                draw_stroke,
            },
            PfShapeClaim {
                backend: "vector_gpu",
            },
        ));
        // The CPU backend's cache marker is meaningless once this backend owns
        // the node; drop it so the two never fight over the `ImageNode`.
        if cpu_rendered.is_some() {
            commands.entity(entity).remove::<PfShapeRendered>();
        }
    }
}

/// Reclaim the atlas when it fills. Slots are never individually freed, so a
/// long session of resizes eventually exhausts the packer; this drops every
/// reservation at once and lets the next frame re-register each live shape.
/// Cheaper and simpler than tracking free lists, and rare enough not to be
/// worth more.
fn rebuild_atlas_if_full(
    mut full: ResMut<PfAtlasFull>,
    atlas: Option<ResMut<PfShapeAtlas>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    slots: Query<(Entity, &PfShapeGpu)>,
    mut rebuilds: ResMut<PfAtlasRebuilds>,
    mut commands: Commands,
) {
    if !full.0 {
        return;
    }
    full.0 = false;
    rebuilds.0 = rebuilds.0.saturating_add(1);
    let Some(mut atlas) = atlas else { return };
    atlas.reset();
    if let Some(mut layout) = layouts.get_mut(&atlas.layout) {
        layout.textures.clear();
    }
    for (entity, gpu) in &slots {
        commands.entity(gpu.draw).despawn();
        if let Some(stroke) = gpu.draw_stroke {
            commands.entity(stroke).despawn();
        }
        commands
            .entity(entity)
            .remove::<(PfShapeGpu, PfShapeClaim, ImageNode)>();
    }
}

/// The atlas camera runs every frame.
///
/// It used to be gated on a dirty counter — skip the pass and the 2048x2048
/// clear when no shape changed — which measured ~0.13 ms better on static UI.
/// That was WRONG: an inactive camera's render target does not reliably
/// retain its contents, so every shape vanished a few frames after it was
/// drawn. Caught in the game as "the borders around the access code fields
/// disappeared", reproduced in `shapes_gpu_check` as every specimen blank.
///
/// The saving is real but needs a mechanism that does not depend on the
/// target persisting: clear and redraw only the live slots, or double-buffer
/// the atlas. Until then, correctness.
fn gate_atlas_camera(
    mut dirty: ResMut<PfAtlasDirty>,
    mut cameras: Query<&mut Camera, With<PfShapeAtlasCamera>>,
) {
    dirty.0 = 0;
    for mut camera in &mut cameras {
        if !camera.is_active {
            camera.is_active = true;
        }
    }
}
