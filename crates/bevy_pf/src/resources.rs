//! WPF-style resources: `ResourceDictionary` values, `Style` with setters,
//! and parsing of resource elements (`<SolidColorBrush x:Key="..."/>`,
//! `<Style TargetType="Button">`, `<x:Double x:Key="..">42</x:Double>`, ...).

use std::collections::HashMap;
use std::sync::Arc;

use bevy_pf_xaml::markup::MarkupValue;
use bevy_pf_xaml::value as v;
use bevy_pf_xaml::{XamlNode, XamlValue};

use crate::error::PfError;

/// Key of a resource in a dictionary: explicit `x:Key`, or the target type of
/// an implicit style (`{x:Type Button}`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResourceKey {
    Explicit(String),
    Type(String),
}

/// A value in a resource dictionary.
#[derive(Debug, Clone)]
pub enum PfValue {
    String(String),
    Double(f64),
    Bool(bool),
    Color(v::PfColor),
    Brush(v::PfBrush),
    Thickness(v::Thickness),
    CornerRadius(v::CornerRadius),
    Geometry(bevy_pf_xaml::geometry::PathData),
    Transform(v::PfTransform),
    Style(Arc<PfStyle>),
    /// An unexpanded `DataTemplate` subtree, instantiated per item.
    Template(Arc<XamlNode>),
}

/// A parsed WPF `Style`: target type plus setters and triggers (BasedOn is
/// already merged at parse time, since `{StaticResource}` requires
/// lexically-earlier definitions just like WPF; base triggers precede derived
/// ones, matching WPF's lookup-list build order).
#[derive(Debug, Clone, Default)]
pub struct PfStyle {
    pub target_type: Option<String>,
    pub setters: Vec<PfSetter>,
    pub triggers: Vec<PfTrigger>,
}

/// A `Style.Triggers` entry: all conditions must hold (one for `Trigger` /
/// `DataTrigger`, several for `MultiTrigger` / `MultiDataTrigger`).
#[derive(Debug, Clone)]
pub struct PfTrigger {
    pub conditions: Vec<PfTriggerCondition>,
    pub setters: Vec<PfSetter>,
}

#[derive(Debug, Clone)]
pub enum PfTriggerCondition {
    /// `<Trigger Property="IsMouseOver" Value="True">`
    Property { property: String, value: String },
    /// `<DataTrigger Binding="{Binding Path}" Value="...">`
    Data { path: String, value: String },
}

#[derive(Debug, Clone)]
pub struct PfSetter {
    /// `Some("DockPanel")` for attached setters like `Property="DockPanel.Dock"`.
    pub owner: Option<String>,
    pub property: String,
    pub value: PfSetterValue,
}

#[derive(Debug, Clone)]
pub enum PfSetterValue {
    /// A literal string, converted per-property at application time.
    Literal(String),
    /// `Value="{StaticResource key}"` — resolved at application time.
    Resource(ResourceKey),
    /// `Value="{DynamicResource key}"` — resolved at application time and
    /// re-resolved when dictionaries change.
    DynamicResource(ResourceKey),
    /// `Value="{x:Null}"` — clears the property (applied as a no-op for
    /// properties without null semantics).
    Null,
    /// A structured value from property-element syntax (e.g. a gradient brush).
    Value(PfValue),
}

pub type ResourceDictionary = HashMap<ResourceKey, PfValue>;

/// A stack of lexically-scoped resource dictionaries (innermost last), with
/// the application-level dictionary as the final fallback tier (mirroring
/// WPF's element tree -> `Application.Resources` lookup order).
#[derive(Debug, Default)]
pub struct ResourceScopes {
    stack: Vec<Arc<ResourceDictionary>>,
    /// Snapshot of the application resources for this instantiation.
    pub app: Option<Arc<ResourceDictionary>>,
}

impl ResourceScopes {
    pub fn push(&mut self, dict: Arc<ResourceDictionary>) {
        self.stack.push(dict);
    }

    pub fn pop(&mut self) {
        self.stack.pop();
    }

    pub fn lookup(&self, key: &ResourceKey) -> Option<&PfValue> {
        self.stack
            .iter()
            .rev()
            .find_map(|d| d.get(key))
            .or_else(|| self.app.as_ref().and_then(|d| d.get(key)))
    }

    pub fn lookup_str(&self, key: &str) -> Option<&PfValue> {
        self.lookup(&ResourceKey::Explicit(key.to_string()))
    }

    /// Look up the implicit style for a XAML type name.
    pub fn implicit_style(&self, type_name: &str) -> Option<Arc<PfStyle>> {
        match self.lookup(&ResourceKey::Type(type_name.to_string())) {
            Some(PfValue::Style(s)) => Some(s.clone()),
            _ => None,
        }
    }

    /// Temporarily remove the lexical stack (keeping the app tier), for
    /// contexts that must not see the consumer's scopes (merged dictionary
    /// files, WPF semantics). Pair with [`Self::restore_stack`].
    pub fn isolate_stack(&mut self) -> Vec<Arc<ResourceDictionary>> {
        std::mem::take(&mut self.stack)
    }

    pub fn restore_stack(&mut self, stack: Vec<Arc<ResourceDictionary>>) {
        self.stack = stack;
    }
}

/// Extract the resource key from a `{StaticResource ...}` positional argument,
/// handling both plain string keys and `{x:Type Button}` style keys.
pub fn static_resource_key(ext: &bevy_pf_xaml::MarkupExtension) -> Result<ResourceKey, PfError> {
    match ext.positional.first() {
        Some(MarkupValue::Str(s)) => Ok(ResourceKey::Explicit(s.clone())),
        Some(MarkupValue::Extension(inner))
            if inner.name == "x:Type" || inner.name == "Type" =>
        {
            let ty = inner
                .first_positional_str()
                .ok_or_else(|| PfError::resource("x:Type missing a type name"))?;
            // Strip any namespace prefix ("local:Foo" -> "Foo").
            let ty = ty.rsplit(':').next().unwrap_or(ty);
            Ok(ResourceKey::Type(ty.to_string()))
        }
        _ => Err(PfError::resource(format!(
            "unsupported {} key",
            ext.name
        ))),
    }
}

/// Parse the contents of a `Resources` property element into a dictionary.
/// `scopes` provides lexically-earlier resources for `BasedOn` / nested
/// `{StaticResource}` resolution.
pub fn parse_resource_dictionary<'a>(
    nodes: impl Iterator<Item = &'a XamlNode>,
    scopes: &ResourceScopes,
    warnings: &mut Vec<String>,
) -> ResourceDictionary {
    let mut dict = ResourceDictionary::new();
    parse_resource_entries_into(nodes, scopes, &mut dict, warnings);
    dict
}

/// Parse resource entries, merging into an existing dictionary (own entries
/// after merged dictionaries, so they win on key conflicts, like WPF).
pub fn parse_resource_entries_into<'a>(
    nodes: impl Iterator<Item = &'a XamlNode>,
    scopes: &ResourceScopes,
    dict: &mut ResourceDictionary,
    warnings: &mut Vec<String>,
) {
    for node in nodes {
        match parse_resource_entry(node, scopes, dict, warnings) {
            Ok(Some((key, value))) => {
                dict.insert(key, value);
            }
            Ok(None) => {}
            Err(e) => warnings.push(format!("{}: skipped resource `{}`: {e}", node.pos, node.name)),
        }
    }
}

fn parse_resource_entry(
    node: &XamlNode,
    scopes: &ResourceScopes,
    local: &ResourceDictionary,
    warnings: &mut Vec<String>,
) -> Result<Option<(ResourceKey, PfValue)>, PfError> {
    let value = match parse_resource_value(node, scopes, local, warnings)? {
        Some(v) => v,
        None => return Ok(None),
    };

    let key = if let Some(k) = &node.x_key {
        ResourceKey::Explicit(k.clone())
    } else if let PfValue::Style(style) = &value {
        match &style.target_type {
            Some(t) => ResourceKey::Type(t.clone()),
            None => return Err(PfError::resource("Style needs x:Key or TargetType")),
        }
    } else {
        return Err(PfError::resource("resource needs x:Key"));
    };
    Ok(Some((key, value)))
}

/// Parse a single resource element into a [`PfValue`].
pub fn parse_resource_value(
    node: &XamlNode,
    scopes: &ResourceScopes,
    local: &ResourceDictionary,
    warnings: &mut Vec<String>,
) -> Result<Option<PfValue>, PfError> {
    let text = node.text_content().unwrap_or_default();
    let value = match node.name.as_str() {
        // x: primitives
        "Double" => PfValue::Double(
            text.trim()
                .parse()
                .map_err(|e| PfError::resource(format!("bad x:Double: {e}")))?,
        ),
        "String" => PfValue::String(text),
        "Boolean" => PfValue::Bool(text.trim().eq_ignore_ascii_case("true")),
        "Int32" | "Int64" => PfValue::Double(
            text.trim()
                .parse::<i64>()
                .map_err(|e| PfError::resource(format!("bad integer: {e}")))? as f64,
        ),

        "SolidColorBrush" => {
            let color = match node.attribute("Color") {
                Some(XamlValue::Str(s)) => s.parse::<v::PfColor>()?,
                Some(XamlValue::Extension(ext)) => {
                    resolve_color_resource(ext, scopes, local)?
                }
                None => text.trim().parse::<v::PfColor>()?,
            };
            PfValue::Brush(v::PfBrush::Solid(color))
        }
        "Color" => PfValue::Color(text.trim().parse()?),
        "Thickness" => PfValue::Thickness(text.trim().parse()?),
        "CornerRadius" => PfValue::CornerRadius(text.trim().parse()?),
        "LinearGradientBrush" => PfValue::Brush(parse_linear_gradient(node, scopes, local)?),
        "RadialGradientBrush" => PfValue::Brush(parse_radial_gradient(node, scopes, local)?),
        "Style" => PfValue::Style(Arc::new(parse_style(node, scopes, local, warnings)?)),
        "DataTemplate" => {
            let mut roots = node.child_elements();
            let root = roots
                .next()
                .ok_or_else(|| PfError::resource("DataTemplate needs a root element"))?;
            if roots.next().is_some() {
                return Err(PfError::resource("DataTemplate must have one root element"));
            }
            PfValue::Template(Arc::new(root.clone()))
        }
        // `<Geometry x:Key="X">M 0,0 ...</Geometry>` — mini-language text.
        "Geometry" => PfValue::Geometry(bevy_pf_xaml::geometry::parse_path_data(
            text.trim(),
        )?),
        "PathGeometry" | "StreamGeometry" | "EllipseGeometry" | "RectangleGeometry"
        | "LineGeometry" | "GeometryGroup" => {
            PfValue::Geometry(parse_geometry_element(node, warnings)?)
        }
        "TranslateTransform" | "ScaleTransform" | "RotateTransform" | "TransformGroup" => {
            PfValue::Transform(parse_transform_element(node, warnings)?)
        }
        other => {
            warnings.push(format!(
                "{}: resource type `{other}` is not supported yet",
                node.pos
            ));
            return Ok(None);
        }
    };
    Ok(Some(value))
}

/// Resolve a `Color="{StaticResource ...}"` attribute.
fn resolve_color_resource(
    ext: &bevy_pf_xaml::MarkupExtension,
    scopes: &ResourceScopes,
    local: &ResourceDictionary,
) -> Result<v::PfColor, PfError> {
    if ext.name != "StaticResource" && ext.name != "DynamicResource" {
        return Err(PfError::resource(format!(
            "Color extension `{{{}}}` not supported",
            ext.name
        )));
    }
    let key = static_resource_key(ext)?;
    match local.get(&key).or_else(|| scopes.lookup(&key)) {
        Some(PfValue::Color(c)) => Ok(*c),
        Some(PfValue::Brush(v::PfBrush::Solid(c))) => Ok(*c),
        Some(PfValue::String(s)) => Ok(s.parse()?),
        Some(_) => Err(PfError::resource("resource is not a color")),
        None => Err(PfError::resource(format!(
            "color resource `{key:?}` not found"
        ))),
    }
}

fn attr_parse<T: std::str::FromStr<Err = bevy_pf_xaml::XamlError>>(
    node: &XamlNode,
    name: &str,
) -> Result<Option<T>, PfError> {
    match node.attribute(name) {
        Some(XamlValue::Str(s)) => Ok(Some(s.parse::<T>()?)),
        _ => Ok(None),
    }
}

fn parse_gradient_stops(
    node: &XamlNode,
    scopes: &ResourceScopes,
    local: &ResourceDictionary,
) -> Result<Vec<v::GradientStop>, PfError> {
    // Stops come either as direct content children (content property) or via
    // <LinearGradientBrush.GradientStops>.
    let mut stops = Vec::new();
    let from_property = node
        .property_element("GradientStops")
        .map(|p| p.elements().collect::<Vec<_>>());
    let elems: Vec<&XamlNode> = match &from_property {
        Some(elems) => elems.clone(),
        None => node.child_elements().collect(),
    };
    for stop in elems {
        // Tolerate an explicit <GradientStopCollection> wrapper.
        let items: Vec<&XamlNode> = if stop.name == "GradientStopCollection" {
            stop.child_elements().collect()
        } else {
            vec![stop]
        };
        for item in items {
            if item.name != "GradientStop" {
                continue;
            }
            let color: v::PfColor = match item.attribute("Color") {
                Some(XamlValue::Str(s)) => s.parse()?,
                Some(XamlValue::Extension(ext)) => {
                    resolve_color_resource(ext, scopes, local)?
                }
                None => return Err(PfError::resource("GradientStop needs Color")),
            };
            let offset: f64 = match item.attribute("Offset") {
                Some(XamlValue::Str(s)) => s.trim().parse().unwrap_or(0.0),
                _ => 0.0,
            };
            stops.push(v::GradientStop {
                color,
                offset: offset as f32,
            });
        }
    }
    if stops.is_empty() {
        return Err(PfError::resource("gradient brush has no stops"));
    }
    Ok(stops)
}

fn parse_linear_gradient(
    node: &XamlNode,
    scopes: &ResourceScopes,
    local: &ResourceDictionary,
) -> Result<v::PfBrush, PfError> {
    let start: v::Point = attr_parse(node, "StartPoint")?.unwrap_or(v::Point::new(0.0, 0.0));
    let end: v::Point = attr_parse(node, "EndPoint")?.unwrap_or(v::Point::new(1.0, 1.0));
    Ok(v::PfBrush::LinearGradient {
        start,
        end,
        stops: parse_gradient_stops(node, scopes, local)?,
    })
}

fn parse_radial_gradient(
    node: &XamlNode,
    scopes: &ResourceScopes,
    local: &ResourceDictionary,
) -> Result<v::PfBrush, PfError> {
    let center: v::Point = attr_parse(node, "Center")?.unwrap_or(v::Point::new(0.5, 0.5));
    let radius_x = match node.attribute("RadiusX") {
        Some(XamlValue::Str(s)) => s.trim().parse().unwrap_or(0.5),
        _ => 0.5,
    };
    let radius_y = match node.attribute("RadiusY") {
        Some(XamlValue::Str(s)) => s.trim().parse().unwrap_or(0.5),
        _ => 0.5,
    };
    Ok(v::PfBrush::RadialGradient {
        center,
        radius_x,
        radius_y,
        stops: parse_gradient_stops(node, scopes, local)?,
    })
}

/// Parse a geometry element (`PathGeometry`, `StreamGeometry`,
/// `EllipseGeometry`, `RectangleGeometry`, `LineGeometry`, `GeometryGroup`)
/// into path data.
pub fn parse_geometry_element(
    node: &XamlNode,
    warnings: &mut Vec<String>,
) -> Result<bevy_pf_xaml::geometry::PathData, PfError> {
    use bevy_pf_xaml::geometry::parse_path_data;
    let data = match node.name.as_str() {
        "StreamGeometry" => {
            let text = node.text_content().unwrap_or_default();
            parse_path_data(text.trim())?
        }
        "PathGeometry" => {
            if let Some(XamlValue::Str(figures)) = node.attribute("Figures") {
                parse_path_data(figures)?
            } else {
                parse_structured_path(node, warnings)?
            }
        }
        "LineGeometry" => {
            let start: v::Point =
                attr_parse(node, "StartPoint")?.unwrap_or(v::Point::new(0.0, 0.0));
            let end: v::Point =
                attr_parse(node, "EndPoint")?.unwrap_or(v::Point::new(0.0, 0.0));
            parse_path_data(&format!("M {},{} L {},{}", start.x, start.y, end.x, end.y))?
        }
        "RectangleGeometry" => {
            let rect: v::Rect = attr_parse(node, "Rect")?.unwrap_or_default();
            parse_path_data(&format!(
                "M {},{} L {},{} L {},{} L {},{} Z",
                rect.x,
                rect.y,
                rect.x + rect.width,
                rect.y,
                rect.x + rect.width,
                rect.y + rect.height,
                rect.x,
                rect.y + rect.height,
            ))?
        }
        "EllipseGeometry" => {
            let center: v::Point =
                attr_parse(node, "Center")?.unwrap_or(v::Point::new(0.0, 0.0));
            let rx = match node.attribute("RadiusX") {
                Some(XamlValue::Str(s)) => s.trim().parse().unwrap_or(0.0),
                _ => 0.0,
            };
            let ry = match node.attribute("RadiusY") {
                Some(XamlValue::Str(s)) => s.trim().parse().unwrap_or(0.0),
                _ => 0.0,
            };
            parse_path_data(&format!(
                "M {},{} A {rx},{ry} 0 1 0 {},{} A {rx},{ry} 0 1 0 {},{} Z",
                center.x - rx,
                center.y,
                center.x + rx,
                center.y,
                center.x - rx,
                center.y,
            ))?
        }
        "GeometryGroup" => {
            let mut merged = bevy_pf_xaml::geometry::PathData::default();
            if let Some(XamlValue::Str(rule)) = node.attribute("FillRule") {
                if rule.eq_ignore_ascii_case("nonzero") {
                    merged.fill_rule = bevy_pf_xaml::geometry::FillRule::NonZero;
                }
            }
            let children_pe = node
                .property_element("Children")
                .map(|p| p.elements().collect::<Vec<_>>());
            let children: Vec<&XamlNode> = match &children_pe {
                Some(elems) => elems.clone(),
                None => node.child_elements().collect(),
            };
            for child in children {
                match parse_geometry_element(child, warnings) {
                    Ok(sub) => merged.figures.extend(sub.figures),
                    Err(e) => warnings.push(format!("{}: {e}", child.pos)),
                }
            }
            merged
        }
        other => {
            return Err(PfError::resource(format!(
                "geometry `{other}` is not supported yet"
            )));
        }
    };
    Ok(data)
}

/// Parse a `<PathGeometry>` with structured `<PathFigure>` children.
fn parse_structured_path(
    node: &XamlNode,
    warnings: &mut Vec<String>,
) -> Result<bevy_pf_xaml::geometry::PathData, PfError> {
    use bevy_pf_xaml::geometry::{PathData, PathFigure, PathSegment};

    let mut data = PathData::default();
    if let Some(XamlValue::Str(rule)) = node.attribute("FillRule") {
        if rule.eq_ignore_ascii_case("nonzero") {
            data.fill_rule = bevy_pf_xaml::geometry::FillRule::NonZero;
        }
    }

    let figures_pe = node
        .property_element("Figures")
        .map(|p| p.elements().collect::<Vec<_>>());
    let figures: Vec<&XamlNode> = match &figures_pe {
        Some(elems) => elems.clone(),
        None => node.child_elements().collect(),
    };

    for fig in figures {
        if fig.name != "PathFigure" {
            warnings.push(format!(
                "{}: unexpected `{}` in PathGeometry",
                fig.pos, fig.name
            ));
            continue;
        }
        let start: v::Point = attr_parse(fig, "StartPoint")?.unwrap_or_default();
        let closed = matches!(
            fig.attribute("IsClosed"),
            Some(XamlValue::Str(s)) if s.eq_ignore_ascii_case("true")
        );
        let mut figure = PathFigure {
            start,
            segments: Vec::new(),
            closed,
        };

        let segs_pe = fig
            .property_element("Segments")
            .map(|p| p.elements().collect::<Vec<_>>());
        let segs: Vec<&XamlNode> = match &segs_pe {
            Some(elems) => elems.clone(),
            None => fig.child_elements().collect(),
        };

        for seg in segs {
            let point = |name: &str| -> Result<v::Point, PfError> {
                attr_parse::<v::Point>(seg, name)?
                    .ok_or_else(|| PfError::resource(format!("{} needs {name}", seg.name)))
            };
            let points = || -> Result<Vec<v::Point>, PfError> {
                match seg.attribute("Points") {
                    Some(XamlValue::Str(s)) => Ok(v::parse_points(s)?),
                    _ => Err(PfError::resource(format!("{} needs Points", seg.name))),
                }
            };
            match seg.name.as_str() {
                "LineSegment" => figure.segments.push(PathSegment::Line(point("Point")?)),
                "PolyLineSegment" => {
                    for p in points()? {
                        figure.segments.push(PathSegment::Line(p));
                    }
                }
                "BezierSegment" => figure.segments.push(PathSegment::Cubic(
                    point("Point1")?,
                    point("Point2")?,
                    point("Point3")?,
                )),
                "PolyBezierSegment" => {
                    for chunk in points()?.chunks_exact(3) {
                        figure
                            .segments
                            .push(PathSegment::Cubic(chunk[0], chunk[1], chunk[2]));
                    }
                }
                "QuadraticBezierSegment" => figure
                    .segments
                    .push(PathSegment::Quadratic(point("Point1")?, point("Point2")?)),
                "PolyQuadraticBezierSegment" => {
                    for chunk in points()?.chunks_exact(2) {
                        figure
                            .segments
                            .push(PathSegment::Quadratic(chunk[0], chunk[1]));
                    }
                }
                "ArcSegment" => {
                    let size: v::Size = attr_parse(seg, "Size")?.unwrap_or_default();
                    let rotation = match seg.attribute("RotationAngle") {
                        Some(XamlValue::Str(s)) => s.trim().parse().unwrap_or(0.0),
                        _ => 0.0,
                    };
                    let large_arc = matches!(
                        seg.attribute("IsLargeArc"),
                        Some(XamlValue::Str(s)) if s.eq_ignore_ascii_case("true")
                    );
                    // WPF SweepDirection: Clockwise == SVG sweep flag 1.
                    let sweep = matches!(
                        seg.attribute("SweepDirection"),
                        Some(XamlValue::Str(s)) if s.eq_ignore_ascii_case("clockwise")
                    );
                    figure.segments.push(PathSegment::Arc {
                        radii: v::Point::new(size.width, size.height),
                        rotation,
                        large_arc,
                        sweep,
                        to: point("Point")?,
                    });
                }
                other => warnings.push(format!(
                    "{}: path segment `{other}` is not supported yet",
                    seg.pos
                )),
            }
        }
        data.figures.push(figure);
    }

    if data.figures.is_empty() {
        return Err(PfError::resource("PathGeometry has no figures"));
    }
    Ok(data)
}

/// Parse a transform element into a decomposed [`v::PfTransform`].
pub fn parse_transform_element(
    node: &XamlNode,
    warnings: &mut Vec<String>,
) -> Result<v::PfTransform, PfError> {
    let attr_f32 = |name: &str, default: f32| -> f32 {
        match node.attribute(name) {
            Some(XamlValue::Str(s)) => s.trim().parse().unwrap_or(default),
            _ => default,
        }
    };
    let t = match node.name.as_str() {
        "TranslateTransform" => v::PfTransform {
            translate_x: attr_f32("X", 0.0),
            translate_y: attr_f32("Y", 0.0),
            ..Default::default()
        },
        "ScaleTransform" => v::PfTransform {
            scale_x: attr_f32("ScaleX", 1.0),
            scale_y: attr_f32("ScaleY", 1.0),
            ..Default::default()
        },
        "RotateTransform" => v::PfTransform {
            rotate_deg: attr_f32("Angle", 0.0),
            ..Default::default()
        },
        // bevy_ui's UiTransform has no skew; accepted as identity so themes
        // that skew decorative elements still load.
        "SkewTransform" | "MatrixTransform" => v::PfTransform::default(),
        "TransformGroup" => {
            let children_pe = node
                .property_element("Children")
                .map(|p| p.elements().collect::<Vec<_>>());
            let children: Vec<&XamlNode> = match &children_pe {
                Some(elems) => elems.clone(),
                None => node.child_elements().collect(),
            };
            let mut combined = v::PfTransform::default();
            for child in children {
                match parse_transform_element(child, warnings) {
                    Ok(t) => combined = combined.compose(&t),
                    Err(e) => warnings.push(format!("{}: {e}", child.pos)),
                }
            }
            combined
        }
        other => {
            return Err(PfError::resource(format!(
                "transform `{other}` is not supported yet"
            )));
        }
    };
    Ok(t)
}

/// Parse a `<Style>` element: `TargetType`, `BasedOn`, and `<Setter>`s.
pub fn parse_style(
    node: &XamlNode,
    scopes: &ResourceScopes,
    local: &ResourceDictionary,
    warnings: &mut Vec<String>,
) -> Result<PfStyle, PfError> {
    let mut style = PfStyle::default();

    // TargetType="Button" or TargetType="{x:Type Button}"
    match node.attribute("TargetType") {
        Some(XamlValue::Str(s)) => {
            let t = s.rsplit(':').next().unwrap_or(s);
            style.target_type = Some(t.to_string());
        }
        Some(XamlValue::Extension(ext)) if ext.name == "x:Type" || ext.name == "Type" => {
            if let Some(t) = ext.first_positional_str() {
                let t = t.rsplit(':').next().unwrap_or(t);
                style.target_type = Some(t.to_string());
            }
        }
        _ => {}
    }

    // BasedOn="{StaticResource ...}" — merge base setters first.
    if let Some(XamlValue::Extension(ext)) = node.attribute("BasedOn") {
        if ext.name == "StaticResource" {
            let key = static_resource_key(ext)?;
            let base = local
                .get(&key)
                .or_else(|| scopes.lookup(&key));
            match base {
                Some(PfValue::Style(base)) => {
                    style.setters.extend(base.setters.iter().cloned());
                    style.triggers.extend(base.triggers.iter().cloned());
                    if style.target_type.is_none() {
                        style.target_type = base.target_type.clone();
                    }
                }
                _ => warnings.push(format!(
                    "{}: BasedOn resource not found (must be defined before use)",
                    node.pos
                )),
            }
        }
    }

    // Setters: direct children (content property) or <Style.Setters>.
    let from_property = node
        .property_element("Setters")
        .map(|p| p.elements().collect::<Vec<_>>());
    let setter_elems: Vec<&XamlNode> = match &from_property {
        Some(elems) => elems.clone(),
        None => node.child_elements().collect(),
    };

    for setter in setter_elems {
        match setter.name.as_str() {
            "Setter" => match parse_setter(setter, scopes, local, warnings) {
                Ok(s) => style.setters.push(s),
                Err(e) => warnings.push(format!("{}: skipped setter: {e}", setter.pos)),
            },
            "EventSetter" => warnings.push(format!(
                "{}: EventSetter is not supported yet",
                setter.pos
            )),
            other => warnings.push(format!(
                "{}: `{other}` in Style is not supported yet",
                setter.pos
            )),
        }
    }

    // Style.Triggers.
    if let Some(triggers_pe) = node.property_element("Triggers") {
        for trigger in triggers_pe.elements() {
            match parse_trigger(trigger, scopes, local, warnings) {
                Ok(Some(t)) => style.triggers.push(t),
                Ok(None) => {}
                Err(e) => {
                    warnings.push(format!("{}: skipped trigger: {e}", trigger.pos))
                }
            }
        }
    }

    Ok(style)
}

/// Parse one `Style.Triggers` entry. `Ok(None)` = recognized-but-unsupported
/// (warned).
fn parse_trigger(
    node: &XamlNode,
    scopes: &ResourceScopes,
    local: &ResourceDictionary,
    warnings: &mut Vec<String>,
) -> Result<Option<PfTrigger>, PfError> {
    let attr_str = |el: &XamlNode, name: &str| -> Option<String> {
        match el.attribute(name) {
            Some(XamlValue::Str(s)) => Some(s.clone()),
            _ => None,
        }
    };
    let binding_path = |el: &XamlNode| -> Option<String> {
        match el.attribute("Binding") {
            Some(XamlValue::Extension(ext)) if ext.name == "Binding" => {
                let spec = crate::binding::parse_binding_extension(ext);
                (spec.unsupported.is_none() && spec.element_name.is_none())
                    .then_some(spec.path)
            }
            _ => None,
        }
    };
    // `Value` may be an attribute or a property element whose content is
    // text (or a single element with text content, e.g. an enum value).
    let trigger_value = |el: &XamlNode| -> Option<String> {
        if let Some(v) = attr_str(el, "Value") {
            return Some(v);
        }
        let pe = el.property_element("Value")?;
        if let Some(inner) = pe.single_element() {
            return inner.text_content();
        }
        pe.values.iter().find_map(|c| c.as_text()).map(String::from)
    };

    let mut conditions = Vec::new();
    match node.name.as_str() {
        "Trigger" => {
            let property = attr_str(node, "Property")
                .ok_or_else(|| PfError::resource("Trigger needs Property"))?;
            let value = trigger_value(node)
                .ok_or_else(|| PfError::resource("Trigger needs Value"))?;
            conditions.push(PfTriggerCondition::Property { property, value });
        }
        "DataTrigger" => {
            let path = binding_path(node).ok_or_else(|| {
                PfError::resource("DataTrigger needs a plain-path Binding")
            })?;
            let value = trigger_value(node)
                .ok_or_else(|| PfError::resource("DataTrigger needs Value"))?;
            conditions.push(PfTriggerCondition::Data { path, value });
        }
        "MultiTrigger" | "MultiDataTrigger" => {
            let Some(conds) = node.property_element("Conditions") else {
                return Err(PfError::resource("MultiTrigger needs Conditions"));
            };
            for cond in conds.elements() {
                if cond.name != "Condition" {
                    continue;
                }
                let value = trigger_value(cond)
                    .ok_or_else(|| PfError::resource("Condition needs Value"))?;
                if let Some(property) = attr_str(cond, "Property") {
                    conditions.push(PfTriggerCondition::Property { property, value });
                } else if let Some(path) = binding_path(cond) {
                    conditions.push(PfTriggerCondition::Data { path, value });
                } else {
                    return Err(PfError::resource(
                        "Condition needs Property or a plain-path Binding",
                    ));
                }
            }
            if conditions.is_empty() {
                return Err(PfError::resource("MultiTrigger has no conditions"));
            }
        }
        "EventTrigger" => {
            warnings.push(format!(
                "{}: EventTrigger (storyboards) is not supported yet",
                node.pos
            ));
            return Ok(None);
        }
        other => {
            warnings.push(format!(
                "{}: trigger `{other}` is not supported yet",
                node.pos
            ));
            return Ok(None);
        }
    }

    if node.property_element("EnterActions").is_some()
        || node.property_element("ExitActions").is_some()
    {
        warnings.push(format!(
            "{}: Trigger Enter/ExitActions (storyboards) are not supported yet",
            node.pos
        ));
    }

    // Setters: direct children or <Trigger.Setters>.
    let from_property = node
        .property_element("Setters")
        .map(|p| p.elements().collect::<Vec<_>>());
    let setter_elems: Vec<&XamlNode> = match &from_property {
        Some(elems) => elems.clone(),
        None => node.child_elements().collect(),
    };
    let mut setters = Vec::new();
    for setter in setter_elems {
        if setter.name != "Setter" {
            continue;
        }
        if setter.attribute("TargetName").is_some() {
            warnings.push(format!(
                "{}: trigger setter TargetName needs templates; skipped",
                setter.pos
            ));
            continue;
        }
        match parse_setter(setter, scopes, local, warnings) {
            Ok(s) => setters.push(s),
            Err(e) => warnings.push(format!("{}: skipped trigger setter: {e}", setter.pos)),
        }
    }

    Ok(Some(PfTrigger {
        conditions,
        setters,
    }))
}

fn parse_setter(
    node: &XamlNode,
    scopes: &ResourceScopes,
    local: &ResourceDictionary,
    warnings: &mut Vec<String>,
) -> Result<PfSetter, PfError> {
    let prop = match node.attribute("Property") {
        Some(XamlValue::Str(s)) => s.clone(),
        _ => return Err(PfError::resource("Setter needs Property")),
    };
    let (owner, property) = match prop.split_once('.') {
        // `Control.Background`-style self-qualified setters (common in
        // base styles shared across control types) are not attached.
        Some((o @ ("Control" | "FrameworkElement" | "UIElement" | "TextElement"), p)) => {
            let _ = o;
            (None, p.to_string())
        }
        Some((o, p)) => (Some(o.to_string()), p.to_string()),
        None => (None, prop),
    };

    let value = if let Some(value) = node.attribute("Value") {
        match value {
            XamlValue::Str(s) => PfSetterValue::Literal(s.clone()),
            XamlValue::Extension(ext) if ext.name == "StaticResource" => {
                PfSetterValue::Resource(static_resource_key(ext)?)
            }
            XamlValue::Extension(ext) if ext.name == "DynamicResource" => {
                PfSetterValue::DynamicResource(static_resource_key(ext)?)
            }
            XamlValue::Extension(ext) if ext.name == "x:Null" || ext.name == "Null" => {
                PfSetterValue::Null
            }
            XamlValue::Extension(ext) => {
                return Err(PfError::resource(format!(
                    "setter value extension `{}` not supported yet",
                    ext.name
                )));
            }
        }
    } else if let Some(pe) = node.property_element("Value") {
        let el = pe
            .single_element()
            .ok_or_else(|| PfError::resource("Setter.Value must hold one element"))?;
        match parse_resource_value(el, scopes, local, warnings)? {
            Some(v) => PfSetterValue::Value(v),
            None => {
                return Err(PfError::resource(format!(
                    "unsupported Setter.Value element `{}`",
                    el.name
                )));
            }
        }
    } else {
        return Err(PfError::resource("Setter needs Value"));
    };

    Ok(PfSetter {
        owner,
        property,
        value,
    })
}
