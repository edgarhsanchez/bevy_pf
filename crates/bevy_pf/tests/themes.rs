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
