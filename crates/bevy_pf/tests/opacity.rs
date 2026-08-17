//! WPF `UIElement.Opacity`, approximated subtree-wide through the value
//! provider store (alpha-multiply with exact restore).

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy_pf::prelude::*;
use bevy_pf::provider::PropertyTarget;
use bevy_pf::{XamlEnv, instantiate_document_env};

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()));
    app.add_plugins(PfUiPlugin);
    app
}

fn spawn(app: &mut App, xaml: &str) -> Entity {
    let doc = bevy_pf_xaml::parse(xaml).expect("parses");
    let world = app.world_mut();
    let root = world.spawn_empty().id();
    let result =
        instantiate_document_env(world, root, &doc, &XamlEnv::default()).expect("instantiates");
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    root
}

fn named(app: &App, root: Entity, name: &str) -> Entity {
    app.world()
        .get::<XamlNames>(root)
        .unwrap()
        .get(name)
        .unwrap()
}

fn alpha_of_bg(app: &App, e: Entity) -> f32 {
    app.world().get::<BackgroundColor>(e).unwrap().0.alpha()
}

const PAGE: &str = r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
     <Border x:Name="Card" Background="#FF334455" Opacity="0.5" Padding="8">
       <TextBlock x:Name="Label" Text="ghost" Foreground="#FF112233"/>
     </Border>
   </StackPanel>"##;

#[test]
fn opacity_attribute_scales_subtree_alphas() {
    let mut app = test_app();
    let root = spawn(&mut app, PAGE);
    let card = named(&app, root, "Card");
    let label = named(&app, root, "Label");
    assert!((alpha_of_bg(&app, card) - 0.5).abs() < 0.01);
    let text_alpha = app
        .world()
        .get::<bevy::text::TextColor>(label)
        .unwrap()
        .0
        .alpha();
    assert!(
        (text_alpha - 0.5).abs() < 0.01,
        "text alpha scaled: {text_alpha}"
    );
}

#[test]
fn opacity_set_and_unset_are_exact() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <Border x:Name="Card" Background="#80334455" Padding="8">
               <TextBlock Text="body"/>
             </Border>
           </StackPanel>"##,
    );
    let card = named(&app, root, "Card");
    let base = alpha_of_bg(&app, card);

    bevy_pf::provider::set_local(
        app.world_mut(),
        card,
        PropertyTarget::Opacity,
        bevy_pf::resources::PfValue::Double(0.25),
    );
    assert!((alpha_of_bg(&app, card) - base * 0.25).abs() < 0.01);

    // Re-apply at a different value: multiplies from ORIGINALS, not the
    // already-scaled color.
    bevy_pf::provider::set_local(
        app.world_mut(),
        card,
        PropertyTarget::Opacity,
        bevy_pf::resources::PfValue::Double(0.5),
    );
    assert!((alpha_of_bg(&app, card) - base * 0.5).abs() < 0.01);

    // Clearing the tier unsets and restores exactly.
    app.world_mut()
        .get_mut::<bevy_pf::PfPropertyStore>(card)
        .unwrap()
        .clear(
            PropertyTarget::Opacity,
            bevy_pf::provider::ValueSource::Local,
        );
    bevy_pf::provider::apply_effective(app.world_mut(), card, PropertyTarget::Opacity);
    assert!(
        (alpha_of_bg(&app, card) - base).abs() < 0.005,
        "restored to {base}"
    );
}

#[test]
fn color_writes_under_active_opacity_stay_scaled() {
    let mut app = test_app();
    let root = spawn(&mut app, PAGE);
    let card = named(&app, root, "Card");

    // A style/trigger-tier write lands unscaled, then the hook re-scales.
    bevy_pf::provider::set_local(
        app.world_mut(),
        card,
        PropertyTarget::Background,
        bevy_pf::resources::PfValue::Color(bevy_pf::xaml_ast::value::PfColor::rgb(0, 255, 0)),
    );
    let bg = app.world().get::<BackgroundColor>(card).unwrap().0;
    assert!(
        (bg.alpha() - 0.5).abs() < 0.01,
        "fresh color re-scaled: {}",
        bg.alpha()
    );
    let srgba = bg.to_srgba();
    assert!(srgba.green > 0.9, "the new color itself applied");
}

#[test]
fn disabled_opacity_trigger_full_cycle() {
    // The Aero2 CheckBox idiom: IsEnabled=False -> Opacity=0.56.
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <StackPanel.Resources>
               <Style TargetType="Border">
                 <Style.Triggers>
                   <Trigger Property="IsEnabled" Value="False">
                     <Setter Property="Opacity" Value="0.56"/>
                   </Trigger>
                 </Style.Triggers>
               </Style>
             </StackPanel.Resources>
             <Border x:Name="Card" Background="#FF334455" Padding="6">
               <TextBlock Text="disabled look"/>
             </Border>
           </StackPanel>"##,
    );
    let card = named(&app, root, "Card");
    app.update();
    assert!(
        (alpha_of_bg(&app, card) - 1.0).abs() < 0.01,
        "enabled at rest"
    );

    app.world_mut()
        .entity_mut(card)
        .insert(bevy::ui::InteractionDisabled);
    app.update();
    assert!(
        (alpha_of_bg(&app, card) - 0.56).abs() < 0.01,
        "disabled dims"
    );

    app.world_mut()
        .entity_mut(card)
        .remove::<bevy::ui::InteractionDisabled>();
    app.update();
    assert!(
        (alpha_of_bg(&app, card) - 1.0).abs() < 0.01,
        "re-enabled restores"
    );
}
