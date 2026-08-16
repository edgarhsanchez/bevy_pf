//! Built-in themes: popular palettes as swappable resource dictionaries.
//!
//! Every theme defines the same `Pf.*` brush keys plus implicit styles that
//! reference them through `{DynamicResource}`, so applying another theme
//! re-colors a live UI in place (the deferred-resource system refreshes every
//! reference on the app-resources revision bump).
//!
//! ```ignore
//! bevy_pf::themes::apply_theme(world, "catppuccin-mocha").unwrap();
//! ```

use bevy::prelude::*;

use crate::XamlEnv;
use crate::error::PfError;

/// A built-in theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeInfo {
    pub slug: &'static str,
    pub name: &'static str,
    pub dark: bool,
}

/// Every built-in theme, in menu order.
pub const THEMES: &[ThemeInfo] = &[
    ThemeInfo { slug: "fluent-light", name: "Fluent Light", dark: false },
    ThemeInfo { slug: "fluent-dark", name: "Fluent Dark", dark: true },
    ThemeInfo { slug: "material-light", name: "Material Light", dark: false },
    ThemeInfo { slug: "material-dark", name: "Material Dark", dark: true },
    ThemeInfo { slug: "nord", name: "Nord", dark: true },
    ThemeInfo { slug: "dracula", name: "Dracula", dark: true },
    ThemeInfo { slug: "catppuccin-latte", name: "Catppuccin Latte", dark: false },
    ThemeInfo { slug: "catppuccin-mocha", name: "Catppuccin Mocha", dark: true },
    ThemeInfo { slug: "solarized-light", name: "Solarized Light", dark: false },
    ThemeInfo { slug: "solarized-dark", name: "Solarized Dark", dark: true },
    ThemeInfo { slug: "gruvbox-dark", name: "Gruvbox Dark", dark: true },
    ThemeInfo { slug: "tokyo-night", name: "Tokyo Night", dark: true },
];

/// The XAML source of a built-in theme dictionary.
pub fn theme_source(slug: &str) -> Option<&'static str> {
    match slug {
        "fluent-light" => Some(include_str!("../assets/themes/fluent-light.xaml")),
        "fluent-dark" => Some(include_str!("../assets/themes/fluent-dark.xaml")),
        "material-light" => Some(include_str!("../assets/themes/material-light.xaml")),
        "material-dark" => Some(include_str!("../assets/themes/material-dark.xaml")),
        "nord" => Some(include_str!("../assets/themes/nord.xaml")),
        "dracula" => Some(include_str!("../assets/themes/dracula.xaml")),
        "catppuccin-latte" => Some(include_str!("../assets/themes/catppuccin-latte.xaml")),
        "catppuccin-mocha" => Some(include_str!("../assets/themes/catppuccin-mocha.xaml")),
        "solarized-light" => Some(include_str!("../assets/themes/solarized-light.xaml")),
        "solarized-dark" => Some(include_str!("../assets/themes/solarized-dark.xaml")),
        "gruvbox-dark" => Some(include_str!("../assets/themes/gruvbox-dark.xaml")),
        "tokyo-night" => Some(include_str!("../assets/themes/tokyo-night.xaml")),
        _ => None,
    }
}

/// Parse a built-in theme and merge it into the application resources.
/// Returns instantiation warnings (empty on the built-ins).
pub fn apply_theme(world: &mut World, slug: &str) -> Result<Vec<String>, PfError> {
    let source = theme_source(slug)
        .ok_or_else(|| PfError::instantiate(format!("unknown theme `{slug}`")))?;
    let doc = bevy_pf_xaml::parse(source)?;
    let warnings =
        crate::instantiate::set_application_resources(world, &doc, &XamlEnv::default());
    // Applying a theme is also a statement about light vs dark, so
    // `{AppThemeBinding}` follows the built-in themes without the host
    // having to say so twice. It lands on the USER theme, which means
    // picking a built-in theme stops following the OS until the host
    // resets it to `Unspecified`.
    derive_control_theme(world);
    if let Some(info) = THEMES.iter().find(|t| t.slug == slug) {
        let theme = if info.dark {
            crate::app_theme::AppTheme::Dark
        } else {
            crate::app_theme::AppTheme::Light
        };
        crate::app_theme::set_user_app_theme(world, theme);
    }
    Ok(warnings)
}

/// Derive the code-drawn control chrome from the theme dictionary.
///
/// Most controls are styled in markup and follow a theme through
/// `{DynamicResource}`. A handful are not: CheckBox and RadioButton boxes,
/// the Slider track and thumb, the ToggleSwitch rail, the ComboBox face and
/// its dropdown are drawn in code from [`PfControlTheme`], whose defaults
/// reproduce the classic WPF light palette.
///
/// Until now nothing connected the two, so applying a dark theme left that
/// chrome light — a white dropdown in a dark window — and the popup rows
/// risked black-on-black, which the type's own docs warned about. Each field
/// now reads the `Pf.*` brush that means the same thing, so a theme swap
/// carries the code-drawn parts with it.
fn derive_control_theme(world: &mut World) {
    use crate::components::PfControlTheme;
    use crate::resources::{PfValue, ResourceKey};

    let Some(app) = world.get_resource::<crate::dynamic::PfApplicationResources>() else {
        return;
    };
    let dict = app.dict.clone();
    let color = |key: &str| -> Option<Color> {
        match dict.get(&ResourceKey::Explicit(key.to_string()))? {
            PfValue::Brush(bevy_pf_xaml::value::PfBrush::Solid(c)) => {
                Some(Color::srgba_u8(c.r, c.g, c.b, c.a))
            }
            PfValue::Color(c) => Some(Color::srgba_u8(c.r, c.g, c.b, c.a)),
            _ => None,
        }
    };

    // A theme that defines none of these is not a bevy_pf theme; leave the
    // defaults rather than build a half-derived palette.
    let Some(accent) = color("Pf.AccentBrush") else {
        return;
    };
    let mut theme = PfControlTheme {
        accent,
        ..PfControlTheme::default()
    };
    if let Some(c) = color("Pf.AccentTextBrush") {
        theme.on_accent = c;
    }
    if let Some(c) = color("Pf.ControlBackground") {
        theme.control_face = c;
    }
    if let Some(c) = color("Pf.BorderBrush") {
        theme.control_border = c;
        theme.popup_border = c;
    }
    if let Some(c) = color("Pf.PanelBackground") {
        // The inactive rail behind a slider, and the dropdown surface.
        theme.track = c;
        theme.popup_face = c;
    }
    if let Some(c) = color("Pf.SelectionBrush") {
        theme.selection_fill = c;
    }
    if let Some(c) = color("Pf.ControlHoverBackground") {
        theme.hover_fill = c;
    }
    // Must contrast with popup_face above, which is why it is read from the
    // theme's own text brush rather than left at BLACK.
    if let Some(c) = color("Pf.TextBrush") {
        theme.item_text = c;
    }
    world.insert_resource(theme);
}
