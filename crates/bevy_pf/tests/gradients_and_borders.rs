//! Gradient and border-brush conformance: brush Opacity, gradient outlines,
//! and warnings for the attributes this brush model cannot express.

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy_pf::prelude::*;
use bevy_pf::{XamlEnv, instantiate_document_env};

fn app() -> App {
    let mut a = App::new();
    a.add_plugins((MinimalPlugins, AssetPlugin::default()));
    a.add_plugins(PfUiPlugin);
    a
}

fn spawn(app: &mut App, xaml: &str) -> (Entity, Vec<String>) {
    let doc = bevy_pf_xaml::parse(xaml).expect("parses");
    let world = app.world_mut();
    let root = world.spawn_empty().id();
    let r = instantiate_document_env(world, root, &doc, &XamlEnv::default()).expect("instantiates");
    (root, r.warnings)
}

fn named(app: &App, root: Entity, name: &str) -> Entity {
    app.world()
        .get::<XamlNames>(root)
        .unwrap()
        .get(name)
        .unwrap()
}

/// A gradient BorderBrush must actually paint an outline. It used to match
/// `Solid` only, so a gradient fell through and the border drew nothing.
#[test]
fn a_gradient_border_brush_paints_a_gradient_outline() {
    let mut app = app();
    let (root, warns) = spawn(
        &mut app,
        r##"<Border xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        x:Name="b" BorderThickness="2" CornerRadius="6">
  <Border.BorderBrush>
    <LinearGradientBrush StartPoint="0,0" EndPoint="0,1">
      <GradientStop Color="#FF8A50" Offset="0"/>
      <GradientStop Color="#00FFD4" Offset="1"/>
    </LinearGradientBrush>
  </Border.BorderBrush>
</Border>"##,
    );
    assert!(warns.is_empty(), "{warns:?}");
    let b = named(&app, root, "b");
    let g = app.world().get::<bevy::ui::BorderGradient>(b);
    assert!(
        g.is_some(),
        "a gradient BorderBrush painted no outline at all"
    );
    assert_eq!(g.unwrap().0.len(), 1);
}

/// Solid and gradient outlines are mutually exclusive — a solid must clear
/// any gradient, or the stale one paints over the colour replacing it.
#[test]
fn a_solid_border_brush_clears_a_previous_gradient() {
    let mut app = app();
    let (root, _) = spawn(
        &mut app,
        r##"<Border xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        x:Name="b" BorderThickness="2" BorderBrush="#FF0000"/>"##,
    );
    let b = named(&app, root, "b");
    assert!(app.world().get::<bevy::ui::BorderGradient>(b).is_none());
}

/// Brush Opacity multiplies the alpha it paints with — the standard WPF
/// scrim idiom. It used to parse and be dropped, rendering fully opaque.
#[test]
fn brush_opacity_reaches_the_alpha() {
    let mut app = app();
    let (root, _) = spawn(
        &mut app,
        r##"<Border xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" x:Name="b">
  <Border.Background>
    <SolidColorBrush Color="#000000" Opacity="0.4"/>
  </Border.Background>
</Border>"##,
    );
    let b = named(&app, root, "b");
    let bg = app.world().get::<BackgroundColor>(b).expect("background");
    let a = bg.0.alpha();
    assert!(
        (a - 0.4).abs() < 0.02,
        "brush Opacity was dropped: alpha {a}"
    );
}

/// What cannot be expressed must SAY SO. Silence is the bug this codebase
/// keeps re-learning.
#[test]
fn unsupported_gradient_attributes_warn_instead_of_vanishing() {
    let mut app = app();
    let (_, warns) = spawn(
        &mut app,
        r##"<Border xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" x:Name="b">
  <Border.Background>
    <RadialGradientBrush SpreadMethod="Reflect" GradientOrigin="0.25,0.25"
                         RadiusX="0.5" RadiusY="0.25">
      <GradientStop Color="White" Offset="0"/>
      <GradientStop Color="Navy" Offset="1"/>
    </RadialGradientBrush>
  </Border.Background>
</Border>"##,
    );
    let joined = warns.join("\n");
    for expect in ["SpreadMethod", "GradientOrigin", "RadiusX"] {
        assert!(
            joined.contains(expect),
            "no warning mentioned {expect}:\n{joined}"
        );
    }
}
