//! WPF Shapes (`Rectangle`, `Ellipse`, `Line`, `Polyline`, `Polygon`, `Path`)
//! rendered with CPU vector rasterization (tiny-skia) into UI image nodes.
//!
//! bevy_ui has no arbitrary vector path support, so shapes rasterize at their
//! laid-out pixel size — crisp at any UI scale, re-rendered when the node is
//! resized.
//!
//! # The backend seam
//!
//! Shapes are retained data (`PfShape`); renderers are interchangeable
//! consumers of it. Every backend follows the same contract, defined by
//! [`PfShapeSystems`] and [`PfShapeClaim`]: in [`PfShapeSystems::Claim`]
//! (PostUpdate, after layout) a backend looks at unclaimed shapes, takes the
//! ones it can draw by inserting [`PfShapeClaim`], and turns them into
//! something bevy_ui composites — node styling, an `ImageNode`, an atlas
//! slot. Whatever nothing claims is rasterized here on the CPU in
//! [`PfShapeSystems::Rasterize`], making tiny-skia the fallback of last
//! resort rather than "the" renderer. Backends order themselves within
//! `Claim`, cheapest first (bevy_ui native styling claims before the GPU
//! atlas). A backend that abandons a shape must remove its claim so the
//! shape falls back the next frame.

use bevy::asset::RenderAssetUsages;
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::ComputedNode;
use bevy::ui::widget::{ImageNode, NodeImageMode};
use bevy_pf_xaml::geometry::{FillRule, PathData, PathSegment};
use bevy_pf_xaml::value as v;

/// The geometry of a WPF shape element.
#[derive(Debug, Clone)]
pub enum ShapeGeometry {
    /// Fills the layout rect; `radius_*` are the WPF `RadiusX/RadiusY`.
    Rectangle {
        radius_x: f32,
        radius_y: f32,
    },
    /// An ellipse inscribed in the layout rect.
    Ellipse,
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    },
    Polyline {
        points: Vec<v::Point>,
        closed: bool,
    },
    Path(PathData),
}

/// A WPF shape: geometry + paint. Rasterized by [`rasterize_shapes`].
#[derive(Component, Debug, Clone)]
pub struct PfShape {
    pub geometry: ShapeGeometry,
    pub fill: Option<v::PfBrush>,
    pub stroke: Option<v::PfBrush>,
    pub stroke_thickness: f32,
    pub stretch: v::Stretch,
    pub stroke_cap: v::PenLineCap,
    pub stroke_join: v::PenLineJoin,
    pub stroke_miter_limit: f32,
    /// Dash pattern in units of `stroke_thickness`, like WPF.
    pub stroke_dash_array: Vec<f32>,
    pub stroke_dash_offset: f32,
    /// Polygon/Polyline `FillRule=` (None -> WPF default EvenOdd).
    pub fill_rule: Option<FillRule>,
}

/// Claims a shape for a backend other than the CPU rasterizer, which skips
/// anything carrying it. See the module docs for the full backend contract.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PfShapeClaim {
    /// Which backend took the shape. Diagnostic, not dispatch — routing is
    /// the marker's presence plus system order inside [`PfShapeSystems::Claim`].
    pub backend: &'static str,
}

/// PostUpdate stages of the shape pipeline, both after `UiSystems::Layout`
/// so backends see final pixel sizes.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PfShapeSystems {
    /// Backends claim the shapes they can draw (see [`PfShapeClaim`]).
    Claim,
    /// The CPU rasterizer draws everything left unclaimed.
    Rasterize,
}

/// Style a shape with bevy_ui's OWN node rendering where the geometry allows
/// it, instead of rasterizing anything.
///
/// bevy_ui already draws rounded rectangles, borders and gradients on the GPU
/// with a signed-distance shader, correctly clipped and z-ordered because it
/// is part of the UI pass rather than a texture smuggled into it. A
/// `<Rectangle>` or `<Ellipse>` with solid paint IS that primitive, so the
/// fastest thing this backend can do for it is nothing at all: no tiny-skia
/// raster, no texture upload, no atlas, no extra camera or pass.
///
/// That covers most UI chrome. Anything it cannot express — arbitrary paths,
/// polylines, lines, dash patterns, elliptical corners, non-uniform ellipses
/// — falls through to the rasterizer untouched.
#[cfg(feature = "native_shapes")]
fn native_style(
    shape: &PfShape,
    px: UVec2,
) -> Option<(Option<Color>, Option<Color>, BorderRadius, f32)> {
    let size = Vec2::new(px.x as f32, px.y as f32);
    let solid = |brush: &v::PfBrush| match brush {
        v::PfBrush::Solid(c) => Some(Color::srgba_u8(c.r, c.g, c.b, c.a)),
        _ => None,
    };
    // A gradient or dashed stroke is not expressible as node styling.
    if !shape.stroke_dash_array.is_empty() {
        return None;
    }
    let fill = match &shape.fill {
        Some(brush) => Some(solid(brush)?),
        None => None,
    };
    let stroke = match &shape.stroke {
        Some(brush) => Some(solid(brush)?),
        None => None,
    };
    if fill.is_none() && stroke.is_none() {
        return None;
    }

    let radius = match &shape.geometry {
        ShapeGeometry::Rectangle { radius_x, radius_y } => {
            // bevy_ui takes one radius per corner, not an ellipse per corner.
            if (radius_x - radius_y).abs() > 0.01 {
                return None;
            }
            BorderRadius::all(Val::Px(*radius_x))
        }
        // An inscribed ellipse is a box rounded by half its extents.
        ShapeGeometry::Ellipse => BorderRadius::all(Val::Px(size.x.min(size.y) * 0.5)),
        _ => return None,
    };
    Some((fill, stroke, radius, shape.stroke_thickness))
}

/// Applies [`native_style`] and marks what it handled.
#[cfg(feature = "native_shapes")]
pub(crate) fn style_native_shapes(
    mut shapes: Query<
        (Entity, Ref<PfShape>, &ComputedNode, &mut bevy::ui::Node),
        Without<PfShapeClaim>,
    >,
    mut commands: Commands,
) {
    for (entity, shape, computed, mut node) in &mut shapes {
        let size = computed.size();
        let px = UVec2::new(size.x.round() as u32, size.y.round() as u32);
        if px.x == 0 || px.y == 0 {
            continue;
        }
        let Some((fill, stroke, radius, thickness)) = native_style(&shape, px) else {
            continue;
        };
        // border_radius is a Node FIELD in bevy 0.19, not a component.
        if node.border_radius != radius {
            node.border_radius = radius;
        }
        let mut e = commands.entity(entity);
        e.insert(PfShapeClaim {
            backend: "bevy_ui_native",
        });
        // Drop any texture a previous pass produced for this node.
        e.remove::<(ImageNode, PfShapeRendered)>();
        e.insert(BackgroundColor(fill.unwrap_or(Color::NONE)));
        if let Some(stroke) = stroke {
            e.insert(BorderColor::all(stroke));
        }
        // bevy_ui draws its border INSIDE the node; WPF straddles the edge.
        // Half a pixel of difference on HUD chrome. Mutate the existing Node
        // rather than insert one — it carries the layout.
        let border = if stroke.is_some() { thickness } else { 0.0 };
        if node.border != bevy::ui::UiRect::all(Val::Px(border)) {
            node.border = bevy::ui::UiRect::all(Val::Px(border));
        }
    }
}

/// Tracks the pixel size of the last rasterization to avoid redundant work.
#[derive(Component, Debug, Default, Clone, PartialEq)]
pub struct PfShapeRendered(pub UVec2);

impl PfShape {
    /// A shape with WPF default paint properties (no fill, no stroke).
    pub fn new(geometry: ShapeGeometry) -> Self {
        Self {
            geometry,
            fill: None,
            stroke: None,
            stroke_thickness: 1.0,
            stretch: v::Stretch::None,
            stroke_cap: v::PenLineCap::Flat,
            stroke_join: v::PenLineJoin::Miter,
            stroke_miter_limit: 10.0,
            stroke_dash_array: Vec::new(),
            stroke_dash_offset: 0.0,
            fill_rule: None,
        }
    }

    /// The natural (unstretched) size of the geometry, used as the node size
    /// for `Stretch=None` shapes, mirroring WPF's measure behavior.
    pub fn natural_size(&self) -> Option<Vec2> {
        let pad = if self.stroke.is_some() {
            self.stroke_thickness * 0.5
        } else {
            0.0
        };
        match &self.geometry {
            ShapeGeometry::Rectangle { .. } | ShapeGeometry::Ellipse => None,
            ShapeGeometry::Line { x1, y1, x2, y2 } => {
                Some(Vec2::new(x1.max(*x2) + pad, y1.max(*y2) + pad))
            }
            ShapeGeometry::Polyline { points, .. } => {
                let mut max = Vec2::ZERO;
                for p in points {
                    max.x = max.x.max(p.x);
                    max.y = max.y.max(p.y);
                }
                Some(max + pad)
            }
            ShapeGeometry::Path(data) => {
                let (_, max) = data.control_bounds()?;
                Some(Vec2::new(max.x + pad, max.y + pad))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Rasterization
// ---------------------------------------------------------------------------

fn to_skia_color(c: v::PfColor) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(c.r, c.g, c.b, c.a)
}

fn brush_to_shader<'a>(brush: &v::PfBrush, w: f32, h: f32) -> tiny_skia::Shader<'a> {
    use tiny_skia::{GradientStop, LinearGradient, Point as SkPoint, RadialGradient, SpreadMode};
    match brush {
        v::PfBrush::Solid(c) => tiny_skia::Shader::SolidColor(to_skia_color(*c)),
        v::PfBrush::LinearGradient { start, end, stops } => {
            let stops: Vec<GradientStop> = stops
                .iter()
                .map(|s| GradientStop::new(s.offset.clamp(0.0, 1.0), to_skia_color(s.color)))
                .collect();
            LinearGradient::new(
                SkPoint::from_xy(start.x * w, start.y * h),
                SkPoint::from_xy(end.x * w, end.y * h),
                stops,
                SpreadMode::Pad,
                tiny_skia::Transform::identity(),
            )
            .unwrap_or(tiny_skia::Shader::SolidColor(tiny_skia::Color::BLACK))
        }
        v::PfBrush::RadialGradient {
            center,
            radius_x,
            radius_y,
            stops,
        } => {
            let stops: Vec<GradientStop> = stops
                .iter()
                .map(|s| GradientStop::new(s.offset.clamp(0.0, 1.0), to_skia_color(s.color)))
                .collect();
            let radius = (radius_x * w).max(radius_y * h).max(1.0);
            RadialGradient::new(
                SkPoint::from_xy(center.x * w, center.y * h),
                0.0,
                SkPoint::from_xy(center.x * w, center.y * h),
                radius,
                stops,
                SpreadMode::Pad,
                tiny_skia::Transform::identity(),
            )
            .unwrap_or(tiny_skia::Shader::SolidColor(tiny_skia::Color::BLACK))
        }
    }
}

/// Convert an SVG-style endpoint arc into cubic bezier segments.
/// Implements the endpoint -> center parameterization from the SVG spec.
pub(crate) fn arc_to_cubics(
    from: v::Point,
    radii: v::Point,
    rotation_deg: f32,
    large_arc: bool,
    sweep: bool,
    to: v::Point,
) -> Vec<(v::Point, v::Point, v::Point)> {
    let (x1, y1) = (from.x, from.y);
    let (x2, y2) = (to.x, to.y);
    let mut rx = radii.x.abs();
    let mut ry = radii.y.abs();
    if rx < 1e-6 || ry < 1e-6 || (x1 == x2 && y1 == y2) {
        return vec![(from, to, to)];
    }
    let phi = rotation_deg.to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();

    // Step 1: compute (x1', y1')
    let dx2 = (x1 - x2) / 2.0;
    let dy2 = (y1 - y2) / 2.0;
    let x1p = cos_phi * dx2 + sin_phi * dy2;
    let y1p = -sin_phi * dx2 + cos_phi * dy2;

    // Correct out-of-range radii.
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }

    // Step 2: compute (cx', cy')
    let num = (rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p).max(0.0);
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut coef = if den.abs() < 1e-9 {
        0.0
    } else {
        (num / den).sqrt()
    };
    if large_arc == sweep {
        coef = -coef;
    }
    let cxp = coef * rx * y1p / ry;
    let cyp = -coef * ry * x1p / rx;

    // Step 3: center
    let cx = cos_phi * cxp - sin_phi * cyp + (x1 + x2) / 2.0;
    let cy = sin_phi * cxp + cos_phi * cyp + (y1 + y2) / 2.0;

    // Step 4: angles
    let angle = |ux: f32, uy: f32, vx: f32, vy: f32| -> f32 {
        let dot = ux * vx + uy * vy;
        let len = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        let mut a = (dot / len).clamp(-1.0, 1.0).acos();
        if ux * vy - uy * vx < 0.0 {
            a = -a;
        }
        a
    };
    let theta1 = angle(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut delta = angle(
        (x1p - cxp) / rx,
        (y1p - cyp) / ry,
        (-x1p - cxp) / rx,
        (-y1p - cyp) / ry,
    );
    if !sweep && delta > 0.0 {
        delta -= std::f32::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f32::consts::TAU;
    }

    // Split into <= 90 degree segments, each approximated by one cubic.
    let segments = (delta.abs() / std::f32::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let seg_delta = delta / segments as f32;
    // Control point distance for a unit-circle arc of angle seg_delta.
    let t = (seg_delta / 4.0).tan() * 4.0 / 3.0;

    let point_at = |theta: f32| -> v::Point {
        let (sin_t, cos_t) = theta.sin_cos();
        v::Point::new(
            cx + rx * cos_t * cos_phi - ry * sin_t * sin_phi,
            cy + rx * cos_t * sin_phi + ry * sin_t * cos_phi,
        )
    };
    let deriv_at = |theta: f32| -> v::Point {
        let (sin_t, cos_t) = theta.sin_cos();
        v::Point::new(
            -rx * sin_t * cos_phi - ry * cos_t * sin_phi,
            -rx * sin_t * sin_phi + ry * cos_t * cos_phi,
        )
    };

    let mut out = Vec::with_capacity(segments);
    let mut theta = theta1;
    let mut p0 = from;
    for _ in 0..segments {
        let theta2 = theta + seg_delta;
        let p3 = point_at(theta2);
        let d0 = deriv_at(theta);
        let d3 = deriv_at(theta2);
        let c1 = v::Point::new(p0.x + t * d0.x, p0.y + t * d0.y);
        let c2 = v::Point::new(p3.x - t * d3.x, p3.y - t * d3.y);
        out.push((c1, c2, p3));
        theta = theta2;
        p0 = p3;
    }
    out
}

fn path_data_to_skia(
    data: &PathData,
    transform: impl Fn(v::Point) -> v::Point,
) -> Option<tiny_skia::Path> {
    let mut pb = tiny_skia::PathBuilder::new();
    for fig in &data.figures {
        let s = transform(fig.start);
        pb.move_to(s.x, s.y);
        let mut current = fig.start;
        for seg in &fig.segments {
            match seg {
                PathSegment::Line(p) => {
                    let tp = transform(*p);
                    pb.line_to(tp.x, tp.y);
                    current = *p;
                }
                PathSegment::Cubic(c1, c2, to) => {
                    let (t1, t2, tt) = (transform(*c1), transform(*c2), transform(*to));
                    pb.cubic_to(t1.x, t1.y, t2.x, t2.y, tt.x, tt.y);
                    current = *to;
                }
                PathSegment::Quadratic(c, to) => {
                    let (tc, tt) = (transform(*c), transform(*to));
                    pb.quad_to(tc.x, tc.y, tt.x, tt.y);
                    current = *to;
                }
                PathSegment::Arc {
                    radii,
                    rotation,
                    large_arc,
                    sweep,
                    to,
                } => {
                    for (c1, c2, p3) in
                        arc_to_cubics(current, *radii, *rotation, *large_arc, *sweep, *to)
                    {
                        let (t1, t2, tt) = (transform(c1), transform(c2), transform(p3));
                        pb.cubic_to(t1.x, t1.y, t2.x, t2.y, tt.x, tt.y);
                    }
                    current = *to;
                }
            }
        }
        if fig.closed {
            pb.close();
        }
    }
    pb.finish()
}

/// Rasterize a shape at the given pixel size into straight-alpha RGBA8 data.
pub fn rasterize_shape(shape: &PfShape, width: u32, height: u32) -> Option<Vec<u8>> {
    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
    let (w, h) = (width as f32, height as f32);
    let st = shape.stroke_thickness;
    let inset = if shape.stroke.is_some() {
        st * 0.5
    } else {
        0.0
    };

    // Build the path in layout space.
    let (path, fill_rule) = match &shape.geometry {
        ShapeGeometry::Rectangle { radius_x, radius_y } => {
            let rect = tiny_skia::Rect::from_ltrb(
                inset,
                inset,
                (w - inset).max(inset + 0.1),
                (h - inset).max(inset + 0.1),
            )?;
            let mut pb = tiny_skia::PathBuilder::new();
            if *radius_x > 0.0 || *radius_y > 0.0 {
                // tiny-skia has no rounded-rect primitive with distinct rx/ry;
                // approximate with a path of arcs via cubics.
                let rx = radius_x.min(rect.width() / 2.0);
                let ry = radius_y.max(0.0).min(rect.height() / 2.0);
                let k = 0.5522848;
                let (l, t, r, b) = (rect.left(), rect.top(), rect.right(), rect.bottom());
                pb.move_to(l + rx, t);
                pb.line_to(r - rx, t);
                pb.cubic_to(r - rx + k * rx, t, r, t + ry - k * ry, r, t + ry);
                pb.line_to(r, b - ry);
                pb.cubic_to(r, b - ry + k * ry, r - rx + k * rx, b, r - rx, b);
                pb.line_to(l + rx, b);
                pb.cubic_to(l + rx - k * rx, b, l, b - ry + k * ry, l, b - ry);
                pb.line_to(l, t + ry);
                pb.cubic_to(l, t + ry - k * ry, l + rx - k * rx, t, l + rx, t);
                pb.close();
            } else {
                pb.push_rect(rect);
            }
            (pb.finish()?, tiny_skia::FillRule::Winding)
        }
        ShapeGeometry::Ellipse => {
            let rect = tiny_skia::Rect::from_ltrb(
                inset,
                inset,
                (w - inset).max(inset + 0.1),
                (h - inset).max(inset + 0.1),
            )?;
            (
                tiny_skia::PathBuilder::from_oval(rect)?,
                tiny_skia::FillRule::Winding,
            )
        }
        ShapeGeometry::Line { x1, y1, x2, y2 } => {
            let mut pb = tiny_skia::PathBuilder::new();
            pb.move_to(*x1, *y1);
            pb.line_to(*x2, *y2);
            (pb.finish()?, tiny_skia::FillRule::Winding)
        }
        ShapeGeometry::Polyline { points, closed } => {
            let mut pb = tiny_skia::PathBuilder::new();
            let mut iter = points.iter();
            let first = iter.next()?;
            pb.move_to(first.x, first.y);
            for p in iter {
                pb.line_to(p.x, p.y);
            }
            if *closed {
                pb.close();
            }
            let rule = match shape.fill_rule {
                Some(FillRule::NonZero) => tiny_skia::FillRule::Winding,
                _ => tiny_skia::FillRule::EvenOdd, // WPF default
            };
            (pb.finish()?, rule)
        }
        ShapeGeometry::Path(data) => {
            let rule = match data.fill_rule {
                FillRule::EvenOdd => tiny_skia::FillRule::EvenOdd,
                FillRule::NonZero => tiny_skia::FillRule::Winding,
            };
            (path_data_to_skia(data, |p| p)?, rule)
        }
    };

    // Stretch handling for coordinate geometries (rect/ellipse already fill).
    let path = match (&shape.geometry, shape.stretch) {
        (ShapeGeometry::Rectangle { .. } | ShapeGeometry::Ellipse, _) => path,
        (_, v::Stretch::None) => path,
        (_, stretch) => {
            let bounds = path.bounds();
            let (bw, bh) = (bounds.width().max(1e-3), bounds.height().max(1e-3));
            let avail_w = (w - st).max(1.0);
            let avail_h = (h - st).max(1.0);
            let (mut sx, mut sy) = (avail_w / bw, avail_h / bh);
            match stretch {
                v::Stretch::Uniform => {
                    let s = sx.min(sy);
                    sx = s;
                    sy = s;
                }
                v::Stretch::UniformToFill => {
                    let s = sx.max(sy);
                    sx = s;
                    sy = s;
                }
                _ => {}
            }
            let ts = tiny_skia::Transform::from_translate(-bounds.left(), -bounds.top())
                .post_scale(sx, sy)
                .post_translate(st * 0.5, st * 0.5);
            path.transform(ts)?
        }
    };

    if let Some(fill) = &shape.fill {
        let paint = tiny_skia::Paint {
            shader: brush_to_shader(fill, w, h),
            anti_alias: true,
            ..Default::default()
        };
        pixmap.fill_path(
            &path,
            &paint,
            fill_rule,
            tiny_skia::Transform::identity(),
            None,
        );
    }
    if let Some(stroke_brush) = &shape.stroke {
        let paint = tiny_skia::Paint {
            shader: brush_to_shader(stroke_brush, w, h),
            anti_alias: true,
            ..Default::default()
        };
        let width = st.max(0.01);
        let stroke = tiny_skia::Stroke {
            width,
            line_cap: match shape.stroke_cap {
                v::PenLineCap::Flat => tiny_skia::LineCap::Butt,
                v::PenLineCap::Square | v::PenLineCap::Triangle => tiny_skia::LineCap::Square,
                v::PenLineCap::Round => tiny_skia::LineCap::Round,
            },
            line_join: match shape.stroke_join {
                v::PenLineJoin::Miter => tiny_skia::LineJoin::Miter,
                v::PenLineJoin::Bevel => tiny_skia::LineJoin::Bevel,
                v::PenLineJoin::Round => tiny_skia::LineJoin::Round,
            },
            miter_limit: shape.stroke_miter_limit.max(1.0),
            // WPF dash units are multiples of the stroke thickness.
            dash: if shape.stroke_dash_array.is_empty() {
                None
            } else {
                let mut dashes: Vec<f32> = shape
                    .stroke_dash_array
                    .iter()
                    .map(|d| (d * width).max(0.01))
                    .collect();
                if !dashes.len().is_multiple_of(2) {
                    let copy = dashes.clone();
                    dashes.extend(copy); // odd counts repeat, like WPF
                }
                tiny_skia::StrokeDash::new(dashes, shape.stroke_dash_offset * width)
            },
        };
        pixmap.stroke_path(
            &path,
            &paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    // Premultiplied -> straight alpha RGBA8.
    let mut out = Vec::with_capacity((width * height * 4) as usize);
    for px in pixmap.pixels() {
        let c = px.demultiply();
        out.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    Some(out)
}

/// Rasterize shapes whose laid-out size changed, updating their UI image.
/// CPU rasterization backend. Public so a harness can register it directly
/// and compare it against the `vector_gpu` backend on the same workload.
pub fn rasterize_shapes(
    mut shapes: Query<
        (
            Entity,
            Ref<PfShape>,
            &ComputedNode,
            Option<&PfShapeRendered>,
        ),
        Without<PfShapeClaim>,
    >,
    images: Option<ResMut<Assets<Image>>>,
    mut commands: Commands,
) {
    let Some(mut images) = images else { return };
    for (entity, shape, computed, rendered) in &mut shapes {
        let size = computed.size();
        let px = UVec2::new(size.x.round() as u32, size.y.round() as u32);
        if px.x == 0 || px.y == 0 {
            continue;
        }
        // SIZE ALONE IS NOT THE CACHE KEY. `PfShapeRendered` records the
        // pixel size that was rasterized, and gating on that alone means a
        // brush change never reaches the texture: a hover that repaints a
        // border, a state setter that swaps a fill, a binding that writes a
        // new colour all leave the node exactly the same SIZE, so the
        // rasterizer skipped them and the shape kept its old paint until
        // something happened to resize it.
        //
        // That was survivable while this backend was the fallback of last
        // resort. It is not now: an over-committed atlas hands its overflow
        // here permanently, so the shapes most likely to be CPU-drawn are
        // ordinary chrome with ordinary hover states.
        if rendered.is_some_and(|r| r.0 == px) && !shape.is_changed() {
            continue;
        }
        let Some(data) = rasterize_shape(&shape, px.x, px.y) else {
            continue;
        };
        let image = Image::new(
            Extent3d {
                width: px.x,
                height: px.y,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
        );
        let handle = images.add(image);
        commands.entity(entity).insert((
            ImageNode::new(handle).with_mode(NodeImageMode::Stretch),
            PfShapeRendered(px),
        ));
    }
}

/// Collected shape-element attributes, mapped to a [`PfShape`] by
/// [`build_shape`].
#[derive(Debug, Clone, Default)]
pub struct ShapeParams {
    pub fill: Option<v::PfBrush>,
    /// Polygon/Polyline `FillRule=` (WPF default: EvenOdd).
    pub fill_rule: Option<FillRule>,
    pub stroke: Option<v::PfBrush>,
    pub stroke_thickness: Option<f32>,
    pub stretch: Option<v::Stretch>,
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub radius_x: f32,
    pub radius_y: f32,
    pub points: Option<Vec<v::Point>>,
    pub data: Option<PathData>,
    pub stroke_cap: Option<v::PenLineCap>,
    pub stroke_join: Option<v::PenLineJoin>,
    pub stroke_miter_limit: Option<f32>,
    pub stroke_dash_array: Option<Vec<f32>>,
    pub stroke_dash_offset: Option<f32>,
}

/// Map a XAML shape element name + collected attributes into a shape.
pub(crate) fn build_shape(name: &str, p: ShapeParams) -> Option<PfShape> {
    let (geometry, default_stretch) = match name {
        "Rectangle" => (
            ShapeGeometry::Rectangle {
                radius_x: p.radius_x,
                radius_y: p.radius_y,
            },
            v::Stretch::Fill,
        ),
        "Ellipse" => (ShapeGeometry::Ellipse, v::Stretch::Fill),
        "Line" => (
            ShapeGeometry::Line {
                x1: p.x1,
                y1: p.y1,
                x2: p.x2,
                y2: p.y2,
            },
            v::Stretch::None,
        ),
        "Polyline" => (
            ShapeGeometry::Polyline {
                points: p.points?,
                closed: false,
            },
            v::Stretch::None,
        ),
        "Polygon" => (
            ShapeGeometry::Polyline {
                points: p.points?,
                closed: true,
            },
            v::Stretch::None,
        ),
        "Path" => (ShapeGeometry::Path(p.data?), v::Stretch::None),
        _ => return None,
    };
    let mut shape = PfShape::new(geometry);
    shape.fill = p.fill;
    shape.stroke = p.stroke;
    shape.stroke_thickness = p.stroke_thickness.unwrap_or(1.0);
    shape.stretch = p.stretch.unwrap_or(default_stretch);
    if let Some(cap) = p.stroke_cap {
        shape.stroke_cap = cap;
    }
    if let Some(join) = p.stroke_join {
        shape.stroke_join = join;
    }
    if let Some(limit) = p.stroke_miter_limit {
        shape.stroke_miter_limit = limit;
    }
    if let Some(dashes) = p.stroke_dash_array {
        shape.stroke_dash_array = dashes;
    }
    if let Some(offset) = p.stroke_dash_offset {
        shape.stroke_dash_offset = offset;
    }
    shape.fill_rule = p.fill_rule;
    Some(shape)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red() -> v::PfBrush {
        v::PfBrush::Solid(v::PfColor::rgb(255, 0, 0))
    }

    fn shape(
        geometry: ShapeGeometry,
        fill: Option<v::PfBrush>,
        stroke: Option<v::PfBrush>,
        stroke_thickness: f32,
        stretch: v::Stretch,
    ) -> PfShape {
        let mut s = PfShape::new(geometry);
        s.fill = fill;
        s.stroke = stroke;
        s.stroke_thickness = stroke_thickness;
        s.stretch = stretch;
        s
    }

    fn pixel(data: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * w + x) * 4) as usize;
        [data[i], data[i + 1], data[i + 2], data[i + 3]]
    }

    #[test]
    fn rectangle_fills_bounds() {
        let shape = shape(
            ShapeGeometry::Rectangle {
                radius_x: 0.0,
                radius_y: 0.0,
            },
            Some(red()),
            None,
            0.0,
            v::Stretch::Fill,
        );
        let data = rasterize_shape(&shape, 20, 10).unwrap();
        assert_eq!(pixel(&data, 20, 10, 5), [255, 0, 0, 255]);
        assert_eq!(pixel(&data, 20, 0, 0), [255, 0, 0, 255]);
    }

    #[test]
    fn ellipse_leaves_corners_transparent() {
        let shape = shape(
            ShapeGeometry::Ellipse,
            Some(red()),
            None,
            0.0,
            v::Stretch::Fill,
        );
        let data = rasterize_shape(&shape, 40, 40).unwrap();
        assert_eq!(pixel(&data, 40, 20, 20), [255, 0, 0, 255]); // center
        assert_eq!(pixel(&data, 40, 1, 1)[3], 0); // corner transparent
    }

    #[test]
    fn polygon_fill_rule_evenodd_vs_nonzero() {
        // Five-point star drawn with crossing edges on a 40x40 grid.
        let star = vec![
            v::Point::new(20.0, 2.0),
            v::Point::new(31.0, 38.0),
            v::Point::new(2.0, 15.0),
            v::Point::new(38.0, 15.0),
            v::Point::new(9.0, 38.0),
        ];
        let geometry = ShapeGeometry::Polyline {
            points: star,
            closed: true,
        };

        // WPF default (EvenOdd): the pentagon core is a hole.
        let evenodd = shape(geometry.clone(), Some(red()), None, 0.0, v::Stretch::None);
        let data = rasterize_shape(&evenodd, 40, 40).unwrap();
        assert_eq!(
            pixel(&data, 40, 20, 18)[3],
            0,
            "star core hollow under EvenOdd"
        );

        // FillRule="NonZero": the core fills.
        let mut nonzero = shape(geometry, Some(red()), None, 0.0, v::Stretch::None);
        nonzero.fill_rule = Some(FillRule::NonZero);
        let data = rasterize_shape(&nonzero, 40, 40).unwrap();
        assert_eq!(
            pixel(&data, 40, 20, 18),
            [255, 0, 0, 255],
            "core filled under NonZero"
        );
    }

    #[test]
    fn triangle_path_renders() {
        let data_path = bevy_pf_xaml::geometry::parse_path_data("M 0,40 L 20,0 L 40,40 Z").unwrap();
        let shape = shape(
            ShapeGeometry::Path(data_path),
            Some(red()),
            None,
            0.0,
            v::Stretch::None,
        );
        let data = rasterize_shape(&shape, 40, 40).unwrap();
        assert_eq!(pixel(&data, 40, 20, 30), [255, 0, 0, 255]); // inside
        assert_eq!(pixel(&data, 40, 1, 1)[3], 0); // top-left outside
        assert_eq!(pixel(&data, 40, 39, 1)[3], 0); // top-right outside
    }

    #[test]
    fn line_strokes() {
        let shape = shape(
            ShapeGeometry::Line {
                x1: 0.0,
                y1: 0.0,
                x2: 20.0,
                y2: 20.0,
            },
            None,
            Some(red()),
            3.0,
            v::Stretch::None,
        );
        let data = rasterize_shape(&shape, 20, 20).unwrap();
        assert_eq!(pixel(&data, 20, 10, 10), [255, 0, 0, 255]); // on the line
        assert_eq!(pixel(&data, 20, 18, 2)[3], 0); // far off the line
    }

    #[test]
    fn arc_path_renders() {
        // Half-circle arc from WPF docs style data.
        let data_path =
            bevy_pf_xaml::geometry::parse_path_data("M 10,50 A 40,40 0 0 1 90,50 Z").unwrap();
        let shape = shape(
            ShapeGeometry::Path(data_path),
            Some(red()),
            None,
            0.0,
            v::Stretch::None,
        );
        let data = rasterize_shape(&shape, 100, 60).unwrap();
        // Top of the arc bulges above the chord midpoint.
        assert_eq!(pixel(&data, 100, 50, 20), [255, 0, 0, 255]);
        assert_eq!(pixel(&data, 100, 5, 5)[3], 0);
    }

    #[test]
    fn uniform_stretch_scales_path() {
        let data_path =
            bevy_pf_xaml::geometry::parse_path_data("M 0,0 L 10,0 L 10,10 L 0,10 Z").unwrap();
        let shape = shape(
            ShapeGeometry::Path(data_path),
            Some(red()),
            None,
            0.0,
            v::Stretch::Uniform,
        );
        // 10x10 square scaled uniformly into 100x50 -> 50x50 square.
        let data = rasterize_shape(&shape, 100, 50).unwrap();
        assert_eq!(pixel(&data, 100, 25, 25), [255, 0, 0, 255]);
        assert_eq!(pixel(&data, 100, 80, 25)[3], 0); // beyond scaled width
    }

    #[test]
    fn natural_size_from_geometry() {
        let shape = shape(
            ShapeGeometry::Line {
                x1: 0.0,
                y1: 0.0,
                x2: 30.0,
                y2: 10.0,
            },
            None,
            Some(red()),
            2.0,
            v::Stretch::None,
        );
        assert_eq!(shape.natural_size(), Some(Vec2::new(31.0, 11.0)));
    }
}
