//! Built-in themes: parse cleanly, style controls, and re-theme live.

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy_pf::prelude::*;
use bevy_pf::{XamlEnv, instantiate_document_env};

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()));
    app.add_plugins(PfUiPlugin);
    app
}

fn srgb(hex: &str) -> Color {
    let h = hex.trim_start_matches('#');
    let (r, g, b) = (
        u8::from_str_radix(&h[0..2], 16).unwrap(),
        u8::from_str_radix(&h[2..4], 16).unwrap(),
        u8::from_str_radix(&h[4..6], 16).unwrap(),
    );
    Color::srgb_u8(r, g, b)
}

#[test]
fn every_builtin_theme_applies_cleanly() {
    for theme in bevy_pf::themes::THEMES {
        let mut app = test_app();
        let warnings =
            bevy_pf::themes::apply_theme(app.world_mut(), theme.slug).expect("theme applies");
        assert!(
            warnings.is_empty(),
            "theme `{}` warned: {warnings:?}",
            theme.slug
        );
    }
    assert_eq!(bevy_pf::themes::THEMES.len(), 12);
}

#[test]
fn themed_button_recolors_when_theme_changes() {
    let mut app = test_app();
    bevy_pf::themes::apply_theme(app.world_mut(), "nord").unwrap();

    let doc = bevy_pf_xaml::parse(
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <Button x:Name="B" Content="Themed" Width="120"/>
           </StackPanel>"#,
    )
    .unwrap();
    let world = app.world_mut();
    let root = world.spawn_empty().id();
    let result = instantiate_document_env(world, root, &doc, &XamlEnv::default()).unwrap();
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);

    let button = app.world().get::<XamlNames>(root).unwrap().get("B").unwrap();
    // Nord control face: nord2 #434C5E.
    assert_eq!(
        app.world().get::<BackgroundColor>(button).unwrap().0,
        srgb("#434C5E")
    );

    // Swap to Dracula: the same button re-colors through DynamicResource.
    bevy_pf::themes::apply_theme(app.world_mut(), "dracula").unwrap();
    app.update();
    assert_eq!(
        app.world().get::<BackgroundColor>(button).unwrap().0,
        srgb("#44475A")
    );
}

/// The code-drawn control chrome follows the theme.
///
/// Most controls are styled in markup and re-color through
/// `{DynamicResource}`. CheckBox boxes, slider tracks, combo faces and
/// dropdown rows are drawn in CODE from `PfControlTheme`, whose defaults are
/// the classic WPF light palette — so before this, applying a dark theme left
/// a white dropdown in a dark window, and the popup rows risked
/// black-on-black.
#[test]
fn a_dark_theme_darkens_the_code_drawn_chrome_too() {
    use bevy_pf::components::PfControlTheme;

    let luminance = |c: Color| {
        let s = c.to_srgba();
        0.2126 * s.red + 0.7152 * s.green + 0.0722 * s.blue
    };

    let mut app = test_app();
    let light = app.world().resource::<PfControlTheme>().clone();
    assert!(
        luminance(light.popup_face) > 0.7,
        "the default palette is WPF light"
    );

    bevy_pf::themes::apply_theme(app.world_mut(), "tokyo-night").unwrap();
    let dark = app.world().resource::<PfControlTheme>().clone();
    assert!(
        luminance(dark.popup_face) < 0.3,
        "a dark theme must darken the dropdown surface, got {:?}",
        dark.popup_face
    );
    assert!(
        luminance(dark.item_text) > 0.5,
        "and its rows must stay readable on it, got {:?}",
        dark.item_text
    );
    assert_ne!(dark.accent, light.accent, "the accent comes from the theme");

    // ...and back again, so this is a mapping rather than a one-way switch.
    bevy_pf::themes::apply_theme(app.world_mut(), "fluent-light").unwrap();
    let relit = app.world().resource::<PfControlTheme>().clone();
    assert!(
        luminance(relit.popup_face) > 0.7,
        "a light theme brings it back, got {:?}",
        relit.popup_face
    );
}

/// Every built-in theme keeps dropdown rows legible against their surface.
#[test]
fn every_theme_keeps_popup_rows_readable() {
    use bevy_pf::components::PfControlTheme;
    let luminance = |c: Color| {
        let s = c.to_srgba();
        0.2126 * s.red + 0.7152 * s.green + 0.0722 * s.blue
    };
    for theme in bevy_pf::themes::THEMES {
        let mut app = test_app();
        bevy_pf::themes::apply_theme(app.world_mut(), theme.slug).unwrap();
        let t = app.world().resource::<PfControlTheme>().clone();
        let contrast = (luminance(t.item_text) - luminance(t.popup_face)).abs();
        assert!(
            contrast > 0.25,
            "`{}`: dropdown rows are nearly invisible (contrast {contrast:.2})",
            theme.slug
        );
    }
}
