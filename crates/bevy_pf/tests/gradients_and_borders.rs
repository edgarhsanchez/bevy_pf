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

#[derive(Reflect, Default)]
struct EdgeVm {
    edge: String,
}

/// A BOUND BorderBrush MUST SURVIVE THE BUTTON'S OWN CHROME.
///
/// A Button repaints its border from `ButtonVisual::normal_border` on
/// every `Changed<Interaction>` — which includes the frame it spawns,
/// because that is when `Interaction` is first added. Setting only
/// `BorderColor` from the binding was therefore painted over by the
/// style's colour before it was ever seen, and any later hover undid it
/// again.
///
/// The symptom that found this: a transparent-when-unavailable row button
/// drew a visible empty box on every row of a list.
#[test]
fn a_bound_border_brush_survives_the_buttons_chrome() {
    let mut a = app();
    let vm = Bindable::new(EdgeVm { edge: "#00000000".into() });
    let doc = bevy_pf_xaml::parse(
        r##"<Button xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                    xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                    x:Name="B" Content="X" BorderThickness="1"
                    BorderBrush="{Binding edge}"/>"##,
    )
    .expect("parses");
    let world = a.world_mut();
    let root = world.spawn(DataContext(vm.clone())).id();
    instantiate_document_env(world, root, &doc, &XamlEnv::default()).expect("instantiates");
    a.update();

    let b = named(&a, root, "B");
    let transparent = a.world().get::<bevy::ui::BorderColor>(b).expect("a border").top;
    assert_eq!(
        transparent.alpha(),
        0.0,
        "the bound transparent border was repainted by the button's chrome"
    );

    // And a later change still lands, rather than being reverted by the
    // next interaction repaint.
    vm.update(|m: &mut EdgeVm| m.edge = "#FFFFB454".into());
    a.update();
    let lit = a.world().get::<bevy::ui::BorderColor>(b).expect("a border").top;
    assert!(lit.alpha() > 0.9, "the bound border did not light up");
}
