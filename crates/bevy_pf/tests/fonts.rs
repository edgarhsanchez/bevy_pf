//! Font resolution: the embedded UI family, weight/style faces, and WPF
//! family-name mapping.

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy::text::{FontSource, TextFont};
use bevy_pf::prelude::*;
use bevy_pf::{XamlEnv, instantiate_document_env};

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()));
    // Register the Font asset type so the built-in faces load (the full
    // TextPlugin isn't needed to test resolution).
    app.init_asset::<bevy::text::Font>();
    app.add_plugins(PfUiPlugin);
    app
}

fn text_font_of(app: &mut App, xaml_attrs: &str) -> TextFont {
    let doc = bevy_pf_xaml::parse(&format!(
        r#"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                      xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                      x:Name="T" Text="sample" {xaml_attrs}/>"#
    ))
    .unwrap();
    let world = app.world_mut();
    let root = world.spawn_empty().id();
    instantiate_document_env(world, root, &doc, &XamlEnv::default()).unwrap();
    let t = app
        .world()
        .get::<XamlNames>(root)
        .unwrap()
        .get("T")
        .unwrap();
    // The text lives on the TextBlock's text child (or the element itself).
    let target = if app.world().get::<TextFont>(t).is_some() {
        t
    } else {
        app.world()
            .get::<Children>(t)
            .and_then(|c| c.iter().find(|e| app.world().get::<TextFont>(*e).is_some()))
            .expect("text child with TextFont")
    };
    app.world().get::<TextFont>(target).unwrap().clone()
}

#[test]
fn builtin_faces_register_and_resolve() {
    let mut app = test_app();

    // Four embedded Fira Sans faces are alive in Assets<Font>.
    let faces = &app.world().resource::<bevy_pf::fonts::PfFonts>().faces;
    assert_eq!(faces.len(), 4);
    assert!(app.world().resource::<Assets<bevy::text::Font>>().len() >= 4);

    // Default: the embedded UI family, normal weight.
    let font = text_font_of(&mut app, "");
    assert!(
        matches!(&font.font, FontSource::Family(f) if f == "Fira Sans"),
        "default font should be the embedded family, got {:?}",
        font.font
    );

    // Weight and style flow into the TextFont for face selection.
    let font = text_font_of(&mut app, r#"FontWeight="Bold" FontStyle="Italic""#);
    assert_eq!(font.weight.0, 700);
    assert!(matches!(font.style, bevy::text::FontStyle::Italic));

    // Windows-only families map to sources that exist on every platform.
    let font = text_font_of(&mut app, r#"FontFamily="Segoe UI""#);
    assert!(matches!(&font.font, FontSource::Family(f) if f == "Fira Sans"));
    let font = text_font_of(&mut app, r#"FontFamily="Consolas""#);
    assert!(matches!(font.font, FontSource::Monospace));
    let font = text_font_of(&mut app, r#"FontFamily="Times New Roman""#);
    assert!(matches!(font.font, FontSource::Serif));

    // Unknown names pass through for natively-installed families.
    let font = text_font_of(&mut app, r#"FontFamily="My Custom Font""#);
    assert!(matches!(&font.font, FontSource::Family(f) if f == "My Custom Font"));
}
