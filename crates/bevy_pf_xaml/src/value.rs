//! Framework-agnostic WPF value types and the string type converters XAML
//! requires ("Red" -> color, "1,2,3,4" -> thickness, "2*" -> grid length).
//!
//! These types deliberately avoid any Bevy dependency so they can be used at
//! compile time from proc macros; `bevy_pf` converts them into Bevy types.

use crate::error::{XamlError, XamlResult};
use std::str::FromStr;

/// Split a WPF list value on commas and/or whitespace, matching
/// `TokenizerHelper`: at most one comma between tokens; leading, consecutive,
/// or trailing commas are errors.
fn split_list<'a>(s: &'a str, target: &'static str) -> XamlResult<Vec<&'a str>> {
    let mut out = Vec::new();
    for segment in s.split(',') {
        let words: Vec<&str> = segment.split_whitespace().collect();
        if words.is_empty() {
            return Err(XamlError::convert(
                s,
                target,
                "empty token (leading, consecutive, or trailing separator)",
            ));
        }
        out.extend(words);
    }
    Ok(out)
}

/// Device-independent pixels per inch (WPF's fixed DPI basis).
const PX_PER_INCH: f32 = 96.0;

/// Parse a length with WPF `LengthConverter` semantics: optional
/// case-insensitive unit suffix `px`/`in`/`cm`/`pt`. A bare unit with no
/// number is `0.0` (a documented WPF quirk).
fn parse_unit_length(t: &str, orig: &str, target: &'static str) -> XamlResult<f32> {
    let lower = t.to_ascii_lowercase();
    let (num, factor) = if let Some(n) = lower.strip_suffix("px") {
        (n, 1.0)
    } else if let Some(n) = lower.strip_suffix("in") {
        (n, PX_PER_INCH)
    } else if let Some(n) = lower.strip_suffix("cm") {
        (n, PX_PER_INCH / 2.54)
    } else if let Some(n) = lower.strip_suffix("pt") {
        (n, PX_PER_INCH / 72.0)
    } else {
        (lower.as_str(), 1.0)
    };
    if num.is_empty() {
        return Ok(0.0);
    }
    num.parse::<f32>()
        .map(|v| v * factor)
        .map_err(|e| XamlError::convert(orig, target, e.to_string()))
}

fn parse_f32(s: &str, target: &'static str) -> XamlResult<f32> {
    let t = s.trim();
    if t.eq_ignore_ascii_case("auto") {
        return Ok(f32::NAN);
    }
    parse_unit_length(t, s, target)
}

/// Parse a WPF length with `LengthConverter` semantics: `Auto` -> NaN,
/// optional `px`/`in`/`cm`/`pt` unit suffix (the runtime's device-independent
/// pixel conversion). The engine's generic numeric attribute path uses this,
/// so `Height="30px"` and `FontSize="14pt"` convert everywhere.
pub fn parse_length(s: &str, target: &'static str) -> XamlResult<f32> {
    parse_f32(s, target)
}

// ---------------------------------------------------------------------------
// Thickness
// ---------------------------------------------------------------------------

/// WPF `Thickness` (Margin, Padding, BorderThickness).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Thickness {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Thickness {
    pub const ZERO: Self = Self::uniform(0.0);

    pub const fn uniform(v: f32) -> Self {
        Self {
            left: v,
            top: v,
            right: v,
            bottom: v,
        }
    }

    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }
}

impl FromStr for Thickness {
    type Err = XamlError;

    /// `"5"` | `"5,10"` (horizontal, vertical) | `"1,2,3,4"` (left, top, right, bottom).
    fn from_str(s: &str) -> XamlResult<Self> {
        let parts = split_list(s, "Thickness")?;
        let nums: Vec<f32> = parts
            .iter()
            .map(|p| parse_f32(p, "Thickness"))
            .collect::<XamlResult<_>>()?;
        match nums.as_slice() {
            [v] => Ok(Self::uniform(*v)),
            [h, v] => Ok(Self::new(*h, *v, *h, *v)),
            [l, t, r, b] => Ok(Self::new(*l, *t, *r, *b)),
            _ => Err(XamlError::convert(
                s,
                "Thickness",
                "expected 1, 2, or 4 numbers",
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// CornerRadius
// ---------------------------------------------------------------------------

/// WPF `CornerRadius` (top-left, top-right, bottom-right, bottom-left).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CornerRadius {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl CornerRadius {
    pub const fn uniform(v: f32) -> Self {
        Self {
            top_left: v,
            top_right: v,
            bottom_right: v,
            bottom_left: v,
        }
    }
}

impl FromStr for CornerRadius {
    type Err = XamlError;

    fn from_str(s: &str) -> XamlResult<Self> {
        // WPF's CornerRadiusConverter uses plain double parsing: no `Auto`
        // alias and no unit suffixes.
        let nums: Vec<f32> = split_list(s, "CornerRadius")?
            .iter()
            .map(|p| {
                p.trim()
                    .parse::<f32>()
                    .map_err(|e| XamlError::convert(s, "CornerRadius", e.to_string()))
            })
            .collect::<XamlResult<_>>()?;
        match nums.as_slice() {
            [v] => Ok(Self::uniform(*v)),
            [tl, tr, br, bl] => Ok(Self {
                top_left: *tl,
                top_right: *tr,
                bottom_right: *br,
                bottom_left: *bl,
            }),
            _ => Err(XamlError::convert(
                s,
                "CornerRadius",
                "expected 1 or 4 numbers",
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Color
// ---------------------------------------------------------------------------

/// An sRGB color with alpha, matching WPF `Color` semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PfColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl PfColor {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const TRANSPARENT: Self = Self::rgba(255, 255, 255, 0);
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const WHITE: Self = Self::rgb(255, 255, 255);

    /// Linear-space components in 0.0..=1.0 (sRGB values, not gamma-decoded).
    pub fn to_f32_srgba(self) -> [f32; 4] {
        [
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
            self.a as f32 / 255.0,
        ]
    }
}

impl FromStr for PfColor {
    type Err = XamlError;

    /// Named colors (`Red`), `#RGB`, `#ARGB`, `#RRGGBB`, `#AARRGGBB`,
    /// and `sc#` scRGB float syntax.
    fn from_str(s: &str) -> XamlResult<Self> {
        let t = s.trim();
        if let Some(hex) = t.strip_prefix('#') {
            return parse_hex_color(s, hex);
        }
        // WPF matches the `sc#` prefix case-sensitively (Knowncolors.cs:240).
        if let Some(sc) = t.strip_prefix("sc#") {
            return parse_sc_color(s, sc);
        }
        named_color(t).ok_or_else(|| XamlError::convert(s, "Color", "unknown color name"))
    }
}

fn parse_hex_color(orig: &str, hex: &str) -> XamlResult<PfColor> {
    let bad = || XamlError::convert(orig, "Color", "invalid hex color");
    let nib =
        |c: u8| -> XamlResult<u8> { (c as char).to_digit(16).map(|d| d as u8).ok_or_else(bad) };
    let b = hex.as_bytes();
    match b.len() {
        3 => {
            let (r, g, bl) = (nib(b[0])?, nib(b[1])?, nib(b[2])?);
            Ok(PfColor::rgb(r * 17, g * 17, bl * 17))
        }
        4 => {
            let (a, r, g, bl) = (nib(b[0])?, nib(b[1])?, nib(b[2])?, nib(b[3])?);
            Ok(PfColor::rgba(r * 17, g * 17, bl * 17, a * 17))
        }
        6 => {
            let byte = |i: usize| -> XamlResult<u8> { Ok(nib(b[i])? * 16 + nib(b[i + 1])?) };
            Ok(PfColor::rgb(byte(0)?, byte(2)?, byte(4)?))
        }
        8 => {
            let byte = |i: usize| -> XamlResult<u8> { Ok(nib(b[i])? * 16 + nib(b[i + 1])?) };
            Ok(PfColor::rgba(byte(2)?, byte(4)?, byte(6)?, byte(0)?))
        }
        _ => Err(bad()),
    }
}

fn parse_sc_color(orig: &str, sc: &str) -> XamlResult<PfColor> {
    // scRGB float components: "sc# a,r,g,b" or "sc# r,g,b" in 0.0..=1.0.
    // We approximate by treating them as sRGB floats (sufficient for UI use).
    let nums: Vec<f32> = split_list(sc, "Color")?
        .iter()
        .map(|p| parse_f32(p, "Color"))
        .collect::<XamlResult<_>>()?;
    let to_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    match nums.as_slice() {
        [r, g, b] => Ok(PfColor::rgb(to_u8(*r), to_u8(*g), to_u8(*b))),
        [a, r, g, b] => Ok(PfColor::rgba(to_u8(*r), to_u8(*g), to_u8(*b), to_u8(*a))),
        _ => Err(XamlError::convert(orig, "Color", "expected 3 or 4 floats")),
    }
}

/// The 141 named colors of `System.Windows.Media.Colors` (identical to the
/// CSS/X11 keyword palette, plus `Transparent`). Lookup is case-insensitive,
/// as in WPF.
pub fn named_color(name: &str) -> Option<PfColor> {
    let idx = NAMED_COLORS
        .binary_search_by(|(n, _)| {
            n.bytes()
                .map(|b| b.to_ascii_lowercase())
                .cmp(name.bytes().map(|b| b.to_ascii_lowercase()))
        })
        .ok()?;
    Some(NAMED_COLORS[idx].1)
}

const fn c(r: u8, g: u8, b: u8) -> PfColor {
    PfColor::rgb(r, g, b)
}

/// Sorted by lowercase name for binary search.
static NAMED_COLORS: &[(&str, PfColor)] = &[
    ("AliceBlue", c(0xF0, 0xF8, 0xFF)),
    ("AntiqueWhite", c(0xFA, 0xEB, 0xD7)),
    ("Aqua", c(0x00, 0xFF, 0xFF)),
    ("Aquamarine", c(0x7F, 0xFF, 0xD4)),
    ("Azure", c(0xF0, 0xFF, 0xFF)),
    ("Beige", c(0xF5, 0xF5, 0xDC)),
    ("Bisque", c(0xFF, 0xE4, 0xC4)),
    ("Black", c(0x00, 0x00, 0x00)),
    ("BlanchedAlmond", c(0xFF, 0xEB, 0xCD)),
    ("Blue", c(0x00, 0x00, 0xFF)),
    ("BlueViolet", c(0x8A, 0x2B, 0xE2)),
    ("Brown", c(0xA5, 0x2A, 0x2A)),
    ("BurlyWood", c(0xDE, 0xB8, 0x87)),
    ("CadetBlue", c(0x5F, 0x9E, 0xA0)),
    ("Chartreuse", c(0x7F, 0xFF, 0x00)),
    ("Chocolate", c(0xD2, 0x69, 0x1E)),
    ("Coral", c(0xFF, 0x7F, 0x50)),
    ("CornflowerBlue", c(0x64, 0x95, 0xED)),
    ("Cornsilk", c(0xFF, 0xF8, 0xDC)),
    ("Crimson", c(0xDC, 0x14, 0x3C)),
    ("Cyan", c(0x00, 0xFF, 0xFF)),
    ("DarkBlue", c(0x00, 0x00, 0x8B)),
    ("DarkCyan", c(0x00, 0x8B, 0x8B)),
    ("DarkGoldenrod", c(0xB8, 0x86, 0x0B)),
    ("DarkGray", c(0xA9, 0xA9, 0xA9)),
    ("DarkGreen", c(0x00, 0x64, 0x00)),
    ("DarkKhaki", c(0xBD, 0xB7, 0x6B)),
    ("DarkMagenta", c(0x8B, 0x00, 0x8B)),
    ("DarkOliveGreen", c(0x55, 0x6B, 0x2F)),
    ("DarkOrange", c(0xFF, 0x8C, 0x00)),
    ("DarkOrchid", c(0x99, 0x32, 0xCC)),
    ("DarkRed", c(0x8B, 0x00, 0x00)),
    ("DarkSalmon", c(0xE9, 0x96, 0x7A)),
    ("DarkSeaGreen", c(0x8F, 0xBC, 0x8F)),
    ("DarkSlateBlue", c(0x48, 0x3D, 0x8B)),
    ("DarkSlateGray", c(0x2F, 0x4F, 0x4F)),
    ("DarkTurquoise", c(0x00, 0xCE, 0xD1)),
    ("DarkViolet", c(0x94, 0x00, 0xD3)),
    ("DeepPink", c(0xFF, 0x14, 0x93)),
    ("DeepSkyBlue", c(0x00, 0xBF, 0xFF)),
    ("DimGray", c(0x69, 0x69, 0x69)),
    ("DodgerBlue", c(0x1E, 0x90, 0xFF)),
    ("Firebrick", c(0xB2, 0x22, 0x22)),
    ("FloralWhite", c(0xFF, 0xFA, 0xF0)),
    ("ForestGreen", c(0x22, 0x8B, 0x22)),
    ("Fuchsia", c(0xFF, 0x00, 0xFF)),
    ("Gainsboro", c(0xDC, 0xDC, 0xDC)),
    ("GhostWhite", c(0xF8, 0xF8, 0xFF)),
    ("Gold", c(0xFF, 0xD7, 0x00)),
    ("Goldenrod", c(0xDA, 0xA5, 0x20)),
    ("Gray", c(0x80, 0x80, 0x80)),
    ("Green", c(0x00, 0x80, 0x00)),
    ("GreenYellow", c(0xAD, 0xFF, 0x2F)),
    ("Honeydew", c(0xF0, 0xFF, 0xF0)),
    ("HotPink", c(0xFF, 0x69, 0xB4)),
    ("IndianRed", c(0xCD, 0x5C, 0x5C)),
    ("Indigo", c(0x4B, 0x00, 0x82)),
    ("Ivory", c(0xFF, 0xFF, 0xF0)),
    ("Khaki", c(0xF0, 0xE6, 0x8C)),
    ("Lavender", c(0xE6, 0xE6, 0xFA)),
    ("LavenderBlush", c(0xFF, 0xF0, 0xF5)),
    ("LawnGreen", c(0x7C, 0xFC, 0x00)),
    ("LemonChiffon", c(0xFF, 0xFA, 0xCD)),
    ("LightBlue", c(0xAD, 0xD8, 0xE6)),
    ("LightCoral", c(0xF0, 0x80, 0x80)),
    ("LightCyan", c(0xE0, 0xFF, 0xFF)),
    ("LightGoldenrodYellow", c(0xFA, 0xFA, 0xD2)),
    ("LightGray", c(0xD3, 0xD3, 0xD3)),
    ("LightGreen", c(0x90, 0xEE, 0x90)),
    ("LightPink", c(0xFF, 0xB6, 0xC1)),
    ("LightSalmon", c(0xFF, 0xA0, 0x7A)),
    ("LightSeaGreen", c(0x20, 0xB2, 0xAA)),
    ("LightSkyBlue", c(0x87, 0xCE, 0xFA)),
    ("LightSlateGray", c(0x77, 0x88, 0x99)),
    ("LightSteelBlue", c(0xB0, 0xC4, 0xDE)),
    ("LightYellow", c(0xFF, 0xFF, 0xE0)),
    ("Lime", c(0x00, 0xFF, 0x00)),
    ("LimeGreen", c(0x32, 0xCD, 0x32)),
    ("Linen", c(0xFA, 0xF0, 0xE6)),
    ("Magenta", c(0xFF, 0x00, 0xFF)),
    ("Maroon", c(0x80, 0x00, 0x00)),
    ("MediumAquamarine", c(0x66, 0xCD, 0xAA)),
    ("MediumBlue", c(0x00, 0x00, 0xCD)),
    ("MediumOrchid", c(0xBA, 0x55, 0xD3)),
    ("MediumPurple", c(0x93, 0x70, 0xDB)),
    ("MediumSeaGreen", c(0x3C, 0xB3, 0x71)),
    ("MediumSlateBlue", c(0x7B, 0x68, 0xEE)),
    ("MediumSpringGreen", c(0x00, 0xFA, 0x9A)),
    ("MediumTurquoise", c(0x48, 0xD1, 0xCC)),
    ("MediumVioletRed", c(0xC7, 0x15, 0x85)),
    ("MidnightBlue", c(0x19, 0x19, 0x70)),
    ("MintCream", c(0xF5, 0xFF, 0xFA)),
    ("MistyRose", c(0xFF, 0xE4, 0xE1)),
    ("Moccasin", c(0xFF, 0xE4, 0xB5)),
    ("NavajoWhite", c(0xFF, 0xDE, 0xAD)),
    ("Navy", c(0x00, 0x00, 0x80)),
    ("OldLace", c(0xFD, 0xF5, 0xE6)),
    ("Olive", c(0x80, 0x80, 0x00)),
    ("OliveDrab", c(0x6B, 0x8E, 0x23)),
    ("Orange", c(0xFF, 0xA5, 0x00)),
    ("OrangeRed", c(0xFF, 0x45, 0x00)),
    ("Orchid", c(0xDA, 0x70, 0xD6)),
    ("PaleGoldenrod", c(0xEE, 0xE8, 0xAA)),
    ("PaleGreen", c(0x98, 0xFB, 0x98)),
    ("PaleTurquoise", c(0xAF, 0xEE, 0xEE)),
    ("PaleVioletRed", c(0xDB, 0x70, 0x93)),
    ("PapayaWhip", c(0xFF, 0xEF, 0xD5)),
    ("PeachPuff", c(0xFF, 0xDA, 0xB9)),
    ("Peru", c(0xCD, 0x85, 0x3F)),
    ("Pink", c(0xFF, 0xC0, 0xCB)),
    ("Plum", c(0xDD, 0xA0, 0xDD)),
    ("PowderBlue", c(0xB0, 0xE0, 0xE6)),
    ("Purple", c(0x80, 0x00, 0x80)),
    ("Red", c(0xFF, 0x00, 0x00)),
    ("RosyBrown", c(0xBC, 0x8F, 0x8F)),
    ("RoyalBlue", c(0x41, 0x69, 0xE1)),
    ("SaddleBrown", c(0x8B, 0x45, 0x13)),
    ("Salmon", c(0xFA, 0x80, 0x72)),
    ("SandyBrown", c(0xF4, 0xA4, 0x60)),
    ("SeaGreen", c(0x2E, 0x8B, 0x57)),
    ("SeaShell", c(0xFF, 0xF5, 0xEE)),
    ("Sienna", c(0xA0, 0x52, 0x2D)),
    ("Silver", c(0xC0, 0xC0, 0xC0)),
    ("SkyBlue", c(0x87, 0xCE, 0xEB)),
    ("SlateBlue", c(0x6A, 0x5A, 0xCD)),
    ("SlateGray", c(0x70, 0x80, 0x90)),
    ("Snow", c(0xFF, 0xFA, 0xFA)),
    ("SpringGreen", c(0x00, 0xFF, 0x7F)),
    ("SteelBlue", c(0x46, 0x82, 0xB4)),
    ("Tan", c(0xD2, 0xB4, 0x8C)),
    ("Teal", c(0x00, 0x80, 0x80)),
    ("Thistle", c(0xD8, 0xBF, 0xD8)),
    ("Tomato", c(0xFF, 0x63, 0x47)),
    ("Transparent", PfColor::TRANSPARENT),
    ("Turquoise", c(0x40, 0xE0, 0xD0)),
    ("Violet", c(0xEE, 0x82, 0xEE)),
    ("Wheat", c(0xF5, 0xDE, 0xB3)),
    ("White", c(0xFF, 0xFF, 0xFF)),
    ("WhiteSmoke", c(0xF5, 0xF5, 0xF5)),
    ("Yellow", c(0xFF, 0xFF, 0x00)),
    ("YellowGreen", c(0x9A, 0xCD, 0x32)),
];

// ---------------------------------------------------------------------------
// Brush
// ---------------------------------------------------------------------------

/// A paint. Attribute syntax only produces solid brushes; gradient brushes are
/// built from property-element syntax by the runtime.
#[derive(Debug, Clone, PartialEq)]
pub enum PfBrush {
    Solid(PfColor),
    LinearGradient {
        start: Point,
        end: Point,
        stops: Vec<GradientStop>,
    },
    RadialGradient {
        center: Point,
        radius_x: f32,
        radius_y: f32,
        stops: Vec<GradientStop>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStop {
    pub offset: f32,
    pub color: PfColor,
}

impl FromStr for PfBrush {
    type Err = XamlError;

    fn from_str(s: &str) -> XamlResult<Self> {
        Ok(PfBrush::Solid(s.parse()?))
    }
}

// ---------------------------------------------------------------------------
// GridLength
// ---------------------------------------------------------------------------

/// WPF `GridLength`: `Auto`, `*`/`2.5*` (star), or an absolute pixel value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridLength {
    Auto,
    Star(f32),
    Pixel(f32),
}

impl FromStr for GridLength {
    type Err = XamlError;

    fn from_str(s: &str) -> XamlResult<Self> {
        let t = s.trim();
        if t.eq_ignore_ascii_case("auto") {
            return Ok(GridLength::Auto);
        }
        if let Some(factor) = t.strip_suffix('*') {
            let factor = factor.trim();
            let f = if factor.is_empty() {
                1.0
            } else {
                // Plain double parse: no Auto alias, no units.
                factor
                    .parse::<f32>()
                    .map_err(|e| XamlError::convert(s, "GridLength", e.to_string()))?
            };
            if f.is_nan() {
                return Err(XamlError::convert(
                    s,
                    "GridLength",
                    "star factor cannot be NaN",
                ));
            }
            return Ok(GridLength::Star(f));
        }
        // Pixel lengths accept unit suffixes but not Auto/NaN, and a bare
        // unit ("px") is an error here (unlike LengthConverter's 0.0 quirk).
        let lower = t.to_ascii_lowercase();
        if matches!(lower.as_str(), "nan" | "px" | "in" | "cm" | "pt") {
            return Err(XamlError::convert(s, "GridLength", "not a valid length"));
        }
        let v = parse_unit_length(t, s, "GridLength")?;
        if v.is_nan() {
            return Err(XamlError::convert(s, "GridLength", "length cannot be NaN"));
        }
        Ok(GridLength::Pixel(v))
    }
}

// ---------------------------------------------------------------------------
// Geometry primitives
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

impl FromStr for Point {
    type Err = XamlError;

    fn from_str(s: &str) -> XamlResult<Self> {
        let nums: Vec<f32> = split_list(s, "Point")?
            .iter()
            .map(|p| parse_f32(p, "Point"))
            .collect::<XamlResult<_>>()?;
        match nums.as_slice() {
            [x, y] => Ok(Point::new(*x, *y)),
            _ => Err(XamlError::convert(s, "Point", "expected 2 numbers")),
        }
    }
}

/// Parse a WPF `DoubleCollection` like `"2 4"` or `"1,2,3"` (StrokeDashArray).
pub fn parse_doubles(s: &str) -> XamlResult<Vec<f32>> {
    split_list(s, "DoubleCollection")?
        .iter()
        .map(|p| parse_f32(p, "DoubleCollection"))
        .collect()
}

/// Parse a WPF point list like `"0,0 10,20 30,15"` (Polyline/Polygon.Points).
pub fn parse_points(s: &str) -> XamlResult<Vec<Point>> {
    let nums: Vec<f32> = split_list(s, "PointCollection")?
        .iter()
        .map(|p| parse_f32(p, "PointCollection"))
        .collect::<XamlResult<_>>()?;
    if !nums.len().is_multiple_of(2) {
        return Err(XamlError::convert(
            s,
            "PointCollection",
            "expected an even number of coordinates",
        ));
    }
    Ok(nums.chunks(2).map(|c| Point::new(c[0], c[1])).collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl FromStr for Size {
    type Err = XamlError;

    fn from_str(s: &str) -> XamlResult<Self> {
        let nums: Vec<f32> = split_list(s, "Size")?
            .iter()
            .map(|p| parse_f32(p, "Size"))
            .collect::<XamlResult<_>>()?;
        match nums.as_slice() {
            [w, h] => Ok(Size {
                width: *w,
                height: *h,
            }),
            _ => Err(XamlError::convert(s, "Size", "expected 2 numbers")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl FromStr for Rect {
    type Err = XamlError;

    fn from_str(s: &str) -> XamlResult<Self> {
        let nums: Vec<f32> = split_list(s, "Rect")?
            .iter()
            .map(|p| parse_f32(p, "Rect"))
            .collect::<XamlResult<_>>()?;
        match nums.as_slice() {
            [x, y, w, h] => Ok(Rect {
                x: *x,
                y: *y,
                width: *w,
                height: *h,
            }),
            _ => Err(XamlError::convert(s, "Rect", "expected 4 numbers")),
        }
    }
}

// ---------------------------------------------------------------------------
// Transform
// ---------------------------------------------------------------------------

/// A decomposed 2D transform (WPF `TranslateTransform`/`ScaleTransform`/
/// `RotateTransform`/`TransformGroup`, composed approximately: translations
/// sum, scales multiply, angles add).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PfTransform {
    pub translate_x: f32,
    pub translate_y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotate_deg: f32,
}

impl Default for PfTransform {
    fn default() -> Self {
        Self {
            translate_x: 0.0,
            translate_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotate_deg: 0.0,
        }
    }
}

impl PfTransform {
    pub fn compose(&self, other: &PfTransform) -> PfTransform {
        PfTransform {
            translate_x: self.translate_x + other.translate_x,
            translate_y: self.translate_y + other.translate_y,
            scale_x: self.scale_x * other.scale_x,
            scale_y: self.scale_y * other.scale_y,
            rotate_deg: self.rotate_deg + other.rotate_deg,
        }
    }
}

// ---------------------------------------------------------------------------
// Duration
// ---------------------------------------------------------------------------

/// WPF `Duration`: a TimeSpan (`[d.]h:m:s[.f]`), `Automatic`, or `Forever`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PfDuration {
    Automatic,
    Forever,
    Time(std::time::Duration),
}

impl FromStr for PfDuration {
    type Err = XamlError;

    fn from_str(s: &str) -> XamlResult<Self> {
        let t = s.trim();
        if t.eq_ignore_ascii_case("automatic") {
            return Ok(PfDuration::Automatic);
        }
        if t.eq_ignore_ascii_case("forever") {
            return Ok(PfDuration::Forever);
        }
        parse_timespan(t)
            .map(PfDuration::Time)
            .ok_or_else(|| XamlError::convert(s, "Duration", "expected [d.]h:m:s[.f]"))
    }
}

/// Parse a .NET TimeSpan literal: `[d.]hh:mm[:ss[.fraction]]`.
fn parse_timespan(t: &str) -> Option<std::time::Duration> {
    let (days, rest) = match t.split_once('.') {
        // "1.02:03:04" — but "0:0:0.5" also contains '.': only treat the
        // prefix as days if it comes before the first ':'.
        Some((d, rest)) if !d.contains(':') && rest.contains(':') => (d.parse::<u64>().ok()?, rest),
        _ => (0, t),
    };
    let parts: Vec<&str> = rest.split(':').collect();
    let (h, m, s_frac): (u64, u64, &str) = match parts.as_slice() {
        [h, m] => (h.parse().ok()?, m.parse().ok()?, "0"),
        [h, m, s] => (h.parse().ok()?, m.parse().ok()?, s),
        _ => return None,
    };
    let secs_f: f64 = s_frac.parse().ok()?;
    if secs_f < 0.0 {
        return None;
    }
    let total = (days * 86_400 + h * 3_600 + m * 60) as f64 + secs_f;
    Some(std::time::Duration::from_secs_f64(total))
}

// ---------------------------------------------------------------------------
// Font types
// ---------------------------------------------------------------------------

/// WPF `FontWeight` (an OpenType weight, 1..=999).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FontWeight(pub u16);

impl FontWeight {
    pub const THIN: Self = Self(100);
    pub const EXTRA_LIGHT: Self = Self(200);
    pub const LIGHT: Self = Self(300);
    pub const NORMAL: Self = Self(400);
    pub const MEDIUM: Self = Self(500);
    pub const SEMI_BOLD: Self = Self(600);
    pub const BOLD: Self = Self(700);
    pub const EXTRA_BOLD: Self = Self(800);
    pub const BLACK: Self = Self(900);
}

impl Default for FontWeight {
    fn default() -> Self {
        Self::NORMAL
    }
}

impl FromStr for FontWeight {
    type Err = XamlError;

    fn from_str(s: &str) -> XamlResult<Self> {
        let t = s.trim();
        if let Ok(n) = t.parse::<u16>() {
            if (1..=999).contains(&n) {
                return Ok(FontWeight(n));
            }
            return Err(XamlError::convert(s, "FontWeight", "must be 1..=999"));
        }
        let w = match t.to_ascii_lowercase().as_str() {
            "thin" => 100,
            "extralight" | "ultralight" => 200,
            "light" => 300,
            "normal" | "regular" => 400,
            "medium" => 500,
            "demibold" | "semibold" => 600,
            "bold" => 700,
            "extrabold" | "ultrabold" => 800,
            "black" | "heavy" => 900,
            "extrablack" | "ultrablack" => 950,
            _ => return Err(XamlError::convert(s, "FontWeight", "unknown weight name")),
        };
        Ok(FontWeight(w))
    }
}

/// WPF `FontStyle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    Oblique,
}

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Implements case-insensitive `FromStr` for a fieldless enum, WPF-style.
macro_rules! xaml_enum {
    ($(#[$meta:meta])* $name:ident { $($(#[$vmeta:meta])* $variant:ident),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $($(#[$vmeta])* $variant),+
        }

        impl FromStr for $name {
            type Err = XamlError;

            fn from_str(s: &str) -> XamlResult<Self> {
                let t = s.trim();
                $(
                    if t.eq_ignore_ascii_case(stringify!($variant)) {
                        return Ok($name::$variant);
                    }
                )+
                Err(XamlError::convert(
                    s,
                    stringify!($name),
                    concat!("expected one of: ", $(stringify!($variant), " ",)+),
                ))
            }
        }
    };
}

xaml_enum!(PenLineCap {
    Flat,
    Square,
    Round,
    Triangle
});
xaml_enum!(PenLineJoin {
    Miter,
    Bevel,
    Round
});
xaml_enum!(HorizontalAlignment {
    Left,
    Center,
    Right,
    Stretch
});
xaml_enum!(VerticalAlignment {
    Top,
    Center,
    Bottom,
    Stretch
});
xaml_enum!(Orientation {
    Horizontal,
    Vertical
});
xaml_enum!(Visibility {
    Visible,
    Hidden,
    Collapsed
});
xaml_enum!(TextAlignment {
    Left,
    Center,
    Right,
    Justify
});
xaml_enum!(TextWrapping {
    NoWrap,
    Wrap,
    WrapWithOverflow
});
xaml_enum!(TextTrimming {
    None,
    CharacterEllipsis,
    WordEllipsis
});
xaml_enum!(Stretch {
    None,
    Fill,
    Uniform,
    UniformToFill
});
xaml_enum!(Dock {
    Left,
    Top,
    Right,
    Bottom
});
xaml_enum!(ScrollBarVisibility {
    Disabled,
    Auto,
    Hidden,
    Visible
});
xaml_enum!(FlowDirection {
    LeftToRight,
    RightToLeft
});
xaml_enum!(FontStyleKind {
    Normal,
    Italic,
    Oblique
});
xaml_enum!(BindingMode {
    OneWay,
    TwoWay,
    OneTime,
    OneWayToSource,
    Default
});
xaml_enum!(ClickMode {
    Release,
    Press,
    Hover
});
xaml_enum!(SelectionMode {
    Single,
    Multiple,
    Extended
});

// The enum comes out of `xaml_enum!`, which cannot attach `#[default]` to a
// variant, so the derive suggestion does not apply.
#[allow(clippy::derivable_impls)]
impl Default for HorizontalAlignment {
    fn default() -> Self {
        Self::Stretch
    }
}

// The enum comes out of `xaml_enum!`, which cannot attach `#[default]` to a
// variant, so the derive suggestion does not apply.
#[allow(clippy::derivable_impls)]
impl Default for VerticalAlignment {
    fn default() -> Self {
        Self::Stretch
    }
}

// The enum comes out of `xaml_enum!`, which cannot attach `#[default]` to a
// variant, so the derive suggestion does not apply.
#[allow(clippy::derivable_impls)]
impl Default for Orientation {
    fn default() -> Self {
        Self::Vertical
    }
}

// The enum comes out of `xaml_enum!`, which cannot attach `#[default]` to a
// variant, so the derive suggestion does not apply.
#[allow(clippy::derivable_impls)]
impl Default for Visibility {
    fn default() -> Self {
        Self::Visible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thickness_forms() {
        assert_eq!("5".parse::<Thickness>().unwrap(), Thickness::uniform(5.0));
        assert_eq!(
            "5,10".parse::<Thickness>().unwrap(),
            Thickness::new(5.0, 10.0, 5.0, 10.0)
        );
        assert_eq!(
            "1,2,3,4".parse::<Thickness>().unwrap(),
            Thickness::new(1.0, 2.0, 3.0, 4.0)
        );
        // WPF also allows space separators.
        assert_eq!(
            "1 2 3 4".parse::<Thickness>().unwrap(),
            Thickness::new(1.0, 2.0, 3.0, 4.0)
        );
        assert!("1,2,3".parse::<Thickness>().is_err());
    }

    #[test]
    fn corner_radius_forms() {
        assert_eq!(
            "8".parse::<CornerRadius>().unwrap(),
            CornerRadius::uniform(8.0)
        );
        let cr = "1,2,3,4".parse::<CornerRadius>().unwrap();
        assert_eq!(cr.top_left, 1.0);
        assert_eq!(cr.top_right, 2.0);
        assert_eq!(cr.bottom_right, 3.0);
        assert_eq!(cr.bottom_left, 4.0);
    }

    #[test]
    fn named_colors_parse() {
        assert_eq!("Red".parse::<PfColor>().unwrap(), PfColor::rgb(255, 0, 0));
        assert_eq!(
            "cornflowerblue".parse::<PfColor>().unwrap(),
            PfColor::rgb(0x64, 0x95, 0xED)
        );
        assert_eq!(
            "TRANSPARENT".parse::<PfColor>().unwrap(),
            PfColor::TRANSPARENT
        );
        assert!("NotAColor".parse::<PfColor>().is_err());
    }

    #[test]
    fn all_named_colors_sorted_for_binary_search() {
        for w in NAMED_COLORS.windows(2) {
            let a = w[0].0.to_ascii_lowercase();
            let b = w[1].0.to_ascii_lowercase();
            assert!(a < b, "colors out of order: {} >= {}", w[0].0, w[1].0);
        }
        assert_eq!(NAMED_COLORS.len(), 141);
    }

    #[test]
    fn hex_colors_parse() {
        assert_eq!(
            "#FF0000".parse::<PfColor>().unwrap(),
            PfColor::rgb(255, 0, 0)
        );
        assert_eq!(
            "#80FF0000".parse::<PfColor>().unwrap(),
            PfColor::rgba(255, 0, 0, 0x80)
        );
        assert_eq!("#F00".parse::<PfColor>().unwrap(), PfColor::rgb(255, 0, 0));
        assert_eq!(
            "#8F00".parse::<PfColor>().unwrap(),
            PfColor::rgba(255, 0, 0, 0x88)
        );
        assert!("#12345".parse::<PfColor>().is_err());
        assert!("#GGGGGG".parse::<PfColor>().is_err());
    }

    #[test]
    fn sc_colors_parse() {
        assert_eq!(
            "sc#1.0,1.0,0.0,0.0".parse::<PfColor>().unwrap(),
            PfColor::rgba(255, 0, 0, 255)
        );
    }

    #[test]
    fn grid_length_forms() {
        assert_eq!("Auto".parse::<GridLength>().unwrap(), GridLength::Auto);
        assert_eq!("auto".parse::<GridLength>().unwrap(), GridLength::Auto);
        assert_eq!("*".parse::<GridLength>().unwrap(), GridLength::Star(1.0));
        assert_eq!("2.5*".parse::<GridLength>().unwrap(), GridLength::Star(2.5));
        assert_eq!(
            "100".parse::<GridLength>().unwrap(),
            GridLength::Pixel(100.0)
        );
        assert!("abc".parse::<GridLength>().is_err());
    }

    #[test]
    fn points_parse() {
        assert_eq!("3,4".parse::<Point>().unwrap(), Point::new(3.0, 4.0));
        let pts = parse_points("0,0 10,20 30,15").unwrap();
        assert_eq!(pts.len(), 3);
        assert_eq!(pts[2], Point::new(30.0, 15.0));
        assert!(parse_points("1,2,3").is_err());
    }

    #[test]
    fn durations_parse() {
        assert_eq!(
            "0:0:1".parse::<PfDuration>().unwrap(),
            PfDuration::Time(std::time::Duration::from_secs(1))
        );
        assert_eq!(
            "0:0:0.25".parse::<PfDuration>().unwrap(),
            PfDuration::Time(std::time::Duration::from_millis(250))
        );
        assert_eq!(
            "1.02:03:04".parse::<PfDuration>().unwrap(),
            PfDuration::Time(std::time::Duration::from_secs(
                86_400 + 2 * 3_600 + 3 * 60 + 4
            ))
        );
        assert_eq!(
            "Forever".parse::<PfDuration>().unwrap(),
            PfDuration::Forever
        );
        assert_eq!(
            "Automatic".parse::<PfDuration>().unwrap(),
            PfDuration::Automatic
        );
        assert!("nope".parse::<PfDuration>().is_err());
    }

    #[test]
    fn font_weights_parse() {
        assert_eq!("Bold".parse::<FontWeight>().unwrap(), FontWeight::BOLD);
        assert_eq!("semibold".parse::<FontWeight>().unwrap(), FontWeight(600));
        assert_eq!("550".parse::<FontWeight>().unwrap(), FontWeight(550));
        assert!("1000".parse::<FontWeight>().is_err());
    }

    #[test]
    fn unit_suffixes_parse() {
        // LengthConverter.cs:187-212 — px/in/cm/pt, case-insensitive.
        assert_eq!(
            "96px".parse::<Thickness>().unwrap(),
            Thickness::uniform(96.0)
        );
        assert_eq!(
            "1in".parse::<Thickness>().unwrap(),
            Thickness::uniform(96.0)
        );
        assert_eq!(
            "2.54cm".parse::<Thickness>().unwrap(),
            Thickness::uniform(96.0)
        );
        assert_eq!(
            "72PT".parse::<Thickness>().unwrap(),
            Thickness::uniform(96.0)
        );
        assert_eq!(
            "1in,2,3,4".parse::<Thickness>().unwrap(),
            Thickness::new(96.0, 2.0, 3.0, 4.0)
        );
        assert_eq!(
            "1in".parse::<GridLength>().unwrap(),
            GridLength::Pixel(96.0)
        );
        // Bare unit: LengthConverter quirk -> 0.0 for lengths...
        assert_eq!("px".parse::<Thickness>().unwrap(), Thickness::uniform(0.0));
        // ...but an error for GridLength.
        assert!("px".parse::<GridLength>().is_err());
    }

    #[test]
    fn auto_is_fully_case_insensitive() {
        let t = "AUTO".parse::<Thickness>().unwrap();
        assert!(t.left.is_nan());
    }

    #[test]
    fn strict_list_separators() {
        // TokenizerHelper: consecutive/leading/trailing commas are errors.
        assert!("1,,2,3".parse::<Thickness>().is_err());
        assert!("1,2,3,4,".parse::<Thickness>().is_err());
        assert!(",1,2,3".parse::<Thickness>().is_err());
        // Mixed comma+space is still fine.
        assert_eq!(
            "1 , 2 3,4".parse::<Thickness>().unwrap(),
            Thickness::new(1.0, 2.0, 3.0, 4.0)
        );
    }

    #[test]
    fn grid_length_rejects_nan_and_auto_star() {
        assert!("NaN".parse::<GridLength>().is_err());
        assert!("Auto*".parse::<GridLength>().is_err());
        assert!("NaN*".parse::<GridLength>().is_err());
    }

    #[test]
    fn corner_radius_rejects_auto() {
        // CornerRadiusConverter uses plain double parsing.
        assert!("Auto".parse::<CornerRadius>().is_err());
        assert!("1,Auto,3,4".parse::<CornerRadius>().is_err());
    }

    #[test]
    fn sc_prefix_is_case_sensitive() {
        assert!("SC#1,0,0,0".parse::<PfColor>().is_err());
        assert!("Sc#1,0,0,0".parse::<PfColor>().is_err());
        assert!("sc#1,0,0,0".parse::<PfColor>().is_ok());
    }

    #[test]
    fn enums_parse_case_insensitively() {
        assert_eq!(
            "center".parse::<HorizontalAlignment>().unwrap(),
            HorizontalAlignment::Center
        );
        assert_eq!(
            "Collapsed".parse::<Visibility>().unwrap(),
            Visibility::Collapsed
        );
        assert_eq!(
            "horizontal".parse::<Orientation>().unwrap(),
            Orientation::Horizontal
        );
        assert_eq!(
            "TwoWay".parse::<BindingMode>().unwrap(),
            BindingMode::TwoWay
        );
        assert!("Diagonal".parse::<Orientation>().is_err());
    }
}
