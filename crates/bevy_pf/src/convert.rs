//! Conversions from `bevy_pf_xaml` value types into Bevy types.

use bevy::prelude::*;
use bevy::text::{FontSize, FontStyle as BevyFontStyle, FontWeight as BevyFontWeight};
use bevy::ui::{GridTrack, RepeatedGridTrack, Val};
use bevy_pf_xaml::value as v;

pub fn color(c: v::PfColor) -> Color {
    Color::srgba_u8(c.r, c.g, c.b, c.a)
}

pub fn thickness(t: v::Thickness) -> UiRect {
    UiRect {
        left: Val::Px(t.left),
        right: Val::Px(t.right),
        top: Val::Px(t.top),
        bottom: Val::Px(t.bottom),
    }
}

pub fn corner_radius(r: v::CornerRadius) -> BorderRadius {
    BorderRadius {
        top_left: Val::Px(r.top_left),
        top_right: Val::Px(r.top_right),
        bottom_right: Val::Px(r.bottom_right),
        bottom_left: Val::Px(r.bottom_left),
    }
}

pub fn grid_track(g: v::GridLength) -> RepeatedGridTrack {
    match g {
        v::GridLength::Auto => GridTrack::auto(),
        // WPF star tracks are strict proportions: content does not grow the
        // track past its share (it clips instead). CSS `fr` alone means
        // `minmax(auto, fr)`, which would grow — pin the minimum to 0.
        // (dotnet/wpf Grid.cs:2038-2065; see docs/wpf-conformance-notes.md L1.)
        v::GridLength::Star(f) => GridTrack::minmax(
            bevy::ui::MinTrackSizingFunction::Px(0.0),
            bevy::ui::MaxTrackSizingFunction::Fraction(f),
        ),
        v::GridLength::Pixel(p) => GridTrack::px(p),
    }
}

/// WPF `Width`/`Height` doubles: `NaN` means `Auto`.
pub fn dimension(value: f32) -> Val {
    if value.is_nan() {
        Val::Auto
    } else {
        Val::Px(value)
    }
}

pub fn font_weight(w: v::FontWeight) -> BevyFontWeight {
    BevyFontWeight(w.0)
}

pub fn font_style(s: v::FontStyleKind) -> BevyFontStyle {
    match s {
        v::FontStyleKind::Normal => BevyFontStyle::Normal,
        v::FontStyleKind::Italic => BevyFontStyle::Italic,
        // WPF Oblique has no angle; parley's default oblique angle is 14deg.
        v::FontStyleKind::Oblique => BevyFontStyle::Oblique(Some(14.0)),
    }
}

pub fn font_size(px: f32) -> FontSize {
    FontSize::Px(px)
}

pub fn text_justify(a: v::TextAlignment) -> bevy::text::Justify {
    use bevy::text::Justify;
    match a {
        v::TextAlignment::Left => Justify::Left,
        v::TextAlignment::Center => Justify::Center,
        v::TextAlignment::Right => Justify::Right,
        v::TextAlignment::Justify => Justify::Justified,
    }
}

pub fn line_break(w: v::TextWrapping) -> bevy::text::LineBreak {
    use bevy::text::LineBreak;
    match w {
        v::TextWrapping::NoWrap => LineBreak::NoWrap,
        v::TextWrapping::Wrap => LineBreak::WordBoundary,
        v::TextWrapping::WrapWithOverflow => LineBreak::WordOrCharacter,
    }
}

pub fn visibility(vis: v::Visibility) -> (Visibility, Option<Display>) {
    match vis {
        // WPF Visible: shown. Hidden: invisible but takes layout space.
        // Collapsed: takes no space — maps to Display::None.
        v::Visibility::Visible => (Visibility::Inherited, None),
        v::Visibility::Hidden => (Visibility::Hidden, None),
        v::Visibility::Collapsed => (Visibility::Hidden, Some(Display::None)),
    }
}

/// Convert a brush into UI background components. Returns the solid color (if
/// solid) or a gradient component value.
pub fn brush_to_background(
    brush: &v::PfBrush,
) -> Result<BackgroundColor, bevy::ui::BackgroundGradient> {
    use bevy::ui::{ColorStop, Gradient, LinearGradient, RadialGradient};
    match brush {
        v::PfBrush::Solid(c) => Ok(BackgroundColor(color(*c))),
        v::PfBrush::LinearGradient { start, end, stops } => {
            // WPF uses start/end points in a 0..1 relative box; bevy_ui linear
            // gradients use an angle. Derive the angle from the vector.
            let dx = end.x - start.x;
            let dy = end.y - start.y;
            // Angle 0 = bottom-to-top in bevy_ui (CSS convention: 0deg = up,
            // clockwise positive). WPF vector (dx, dy) with y-down.
            let angle = dx.atan2(-dy);
            let stops = stops
                .iter()
                .map(|s| ColorStop::new(color(s.color), Val::Percent(s.offset * 100.0)))
                .collect();
            Err(bevy::ui::BackgroundGradient(vec![Gradient::Linear(
                LinearGradient {
                    angle,
                    stops,
                    ..Default::default()
                },
            )]))
        }
        v::PfBrush::RadialGradient { stops, .. } => {
            let stops = stops
                .iter()
                .map(|s| ColorStop::new(color(s.color), Val::Percent(s.offset * 100.0)))
                .collect();
            Err(bevy::ui::BackgroundGradient(vec![Gradient::Radial(
                RadialGradient {
                    stops,
                    ..Default::default()
                },
            )]))
        }
    }
}

/// Map a WPF RenderTransform onto bevy_ui's post-layout `UiTransform`.
pub fn ui_transform(t: v::PfTransform) -> bevy::ui::UiTransform {
    bevy::ui::UiTransform {
        translation: bevy::ui::Val2::px(t.translate_x, t.translate_y),
        scale: Vec2::new(t.scale_x, t.scale_y),
        rotation: Rot2::degrees(t.rotate_deg),
    }
}

pub fn h_align_self(a: v::HorizontalAlignment) -> AlignSelf {
    match a {
        v::HorizontalAlignment::Left => AlignSelf::FlexStart,
        v::HorizontalAlignment::Center => AlignSelf::Center,
        v::HorizontalAlignment::Right => AlignSelf::FlexEnd,
        v::HorizontalAlignment::Stretch => AlignSelf::Stretch,
    }
}

pub fn v_align_self(a: v::VerticalAlignment) -> AlignSelf {
    match a {
        v::VerticalAlignment::Top => AlignSelf::FlexStart,
        v::VerticalAlignment::Center => AlignSelf::Center,
        v::VerticalAlignment::Bottom => AlignSelf::FlexEnd,
        v::VerticalAlignment::Stretch => AlignSelf::Stretch,
    }
}

pub fn h_justify_self(a: v::HorizontalAlignment) -> JustifySelf {
    match a {
        v::HorizontalAlignment::Left => JustifySelf::Start,
        v::HorizontalAlignment::Center => JustifySelf::Center,
        v::HorizontalAlignment::Right => JustifySelf::End,
        v::HorizontalAlignment::Stretch => JustifySelf::Stretch,
    }
}
