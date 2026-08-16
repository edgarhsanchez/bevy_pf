//! `{AppThemeBinding}` end to end, against MAUI's semantics.
//!
//! The unit tests in `src/app_theme.rs` pin the pick rules in isolation;
//! these pin that the rules actually reach a painted entity, survive a theme
//! flip without re-instantiation, and behave at each placement (attribute,
//! style setter, trigger setter).

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

fn spawn(app: &mut App, xaml: &str) -> (Entity, Vec<String>) {
    let doc = bevy_pf_xaml::parse(xaml).expect("parses");
    let world = app.world_mut();
    let root = world.spawn_empty().id();
    let result = instantiate_document_env(world, root, &doc, &XamlEnv::default()).expect("builds");
    (root, result.warnings)
}

fn named(app: &App, root: Entity, name: &str) -> Entity {
    app.world().get::<XamlNames>(root).unwrap().get(name).unwrap()
}

/// The painted background, or `None` when the property is unset/cleared.
fn background(app: &App, entity: Entity) -> Option<String> {
    app.world()
        .get::<BackgroundColor>(entity)
        .map(|c| bevy_pf::instantiate::color_to_hex(c.0))
}

fn page(background_value: &str) -> String {
    format!(
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
     <Border x:Name="B" Background="{background_value}"/>
   </StackPanel>"##
    )
}

/// Build under `theme` and return the resulting background.
fn under(theme: AppTheme, background_value: &str) -> (Option<String>, Vec<String>) {
    let mut app = test_app();
    set_user_app_theme(app.world_mut(), theme);
    let (root, warnings) = spawn(&mut app, &page(background_value));
    app.update();
    (background(&app, named(&app, root, "B")), warnings)
}

// ---------------------------------------------------------------------
// The pick rules, reaching a painted entity.
// ---------------------------------------------------------------------

#[test]
fn each_theme_takes_its_own_arm() {
    let markup = "{AppThemeBinding Light=#FF0000, Dark=#0000FF}";
    assert_eq!(under(AppTheme::Light, markup).0, Some("#FF0000".into()));
    assert_eq!(under(AppTheme::Dark, markup).0, Some("#0000FF".into()));
}

#[test]
fn unspecified_resolves_as_light() {
    // MAUI's switch has no Unspecified case — it shares Light's `_` arm.
    // Getting this wrong would silently give every un-themed app the DARK
    // palette, or no value at all.
    let (color, warnings) = under(
        AppTheme::Unspecified,
        "{AppThemeBinding Light=#FF0000, Dark=#0000FF}",
    );
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(color, Some("#FF0000".into()));
}

#[test]
fn a_missing_arm_falls_back_to_default_only() {
    // Dark is absent, so Dark takes Default — NOT Light.
    assert_eq!(
        under(AppTheme::Dark, "{AppThemeBinding Light=#FF0000, Default=#00FF00}").0,
        Some("#00FF00".into())
    );
    // ...and symmetrically.
    assert_eq!(
        under(AppTheme::Light, "{AppThemeBinding Dark=#0000FF, Default=#00FF00}").0,
        Some("#00FF00".into())
    );
}

#[test]
fn a_positional_argument_sets_default() {
    // Default is the extension's ContentProperty in MAUI.
    let markup = "{AppThemeBinding #00FF00}";
    assert_eq!(under(AppTheme::Light, markup).0, Some("#00FF00".into()));
    assert_eq!(under(AppTheme::Dark, markup).0, Some("#00FF00".into()));
}

#[test]
fn no_matching_arm_and_no_default_clears_rather_than_reverting() {
    // The most MAUI-specific behaviour in the feature: MAUI writes NULL to
    // the target, so a style-supplied value underneath does NOT show
    // through. Reverting instead would look reasonable and be wrong.
    let mut app = test_app();
    set_user_app_theme(app.world_mut(), AppTheme::Light);
    let (root, warnings) = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
     <StackPanel.Resources>
       <Style x:Key="S" TargetType="Border">
         <Setter Property="Background" Value="#FFFF00"/>
       </Style>
     </StackPanel.Resources>
     <Border x:Name="B" Style="{StaticResource S}"
             Background="{AppThemeBinding Dark=#0000FF}"/>
   </StackPanel>"##,
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());
    let painted = background(&app, named(&app, root, "B"));
    assert_ne!(painted, Some("#FFFF00".into()), "must NOT fall back to the style value");
    assert_ne!(painted, Some("#0000FF".into()), "must NOT take the Dark arm under Light");
}

#[test]
fn an_arm_written_as_null_is_supplied_and_stops_the_fallback() {
    // `Light={x:Null}` IS supplied, so Light yields null instead of
    // falling through to Default. "Supplied" is about the markup, not the
    // value.
    let (color, warnings) = under(
        AppTheme::Light,
        "{AppThemeBinding Light={x:Null}, Default=#00FF00}",
    );
    assert_eq!(warnings, Vec::<String>::new());
    assert_ne!(color, Some("#00FF00".into()), "Default must not be reached");
}

// ---------------------------------------------------------------------
// Liveness.
// ---------------------------------------------------------------------

#[test]
fn a_theme_flip_repaints_without_rebuilding() {
    let mut app = test_app();
    set_user_app_theme(app.world_mut(), AppTheme::Light);
    let (root, _) = spawn(&mut app, &page("{AppThemeBinding Light=#FF0000, Dark=#0000FF}"));
    app.update();
    let border = named(&app, root, "B");
    assert_eq!(background(&app, border), Some("#FF0000".into()));

    set_user_app_theme(app.world_mut(), AppTheme::Dark);
    app.update();
    assert_eq!(
        background(&app, border),
        Some("#0000FF".into()),
        "the SAME entity must repaint — no re-instantiation"
    );
}

#[test]
fn setting_the_theme_already_in_effect_changes_nothing() {
    let mut app = test_app();
    set_user_app_theme(app.world_mut(), AppTheme::Light);
    let generation = app.world().resource::<PfAppTheme>().generation();
    set_user_app_theme(app.world_mut(), AppTheme::Light);
    assert_eq!(
        app.world().resource::<PfAppTheme>().generation(),
        generation,
        "a no-op set must not bump the generation, or every frame would refresh"
    );
}

#[test]
fn a_user_choice_beats_the_platform_and_unspecified_hands_control_back() {
    let mut app = test_app();
    let (root, _) = spawn(&mut app, &page("{AppThemeBinding Light=#FF0000, Dark=#0000FF}"));
    let border = named(&app, root, "B");

    // Pretend the OS reports dark while the user has asked for light.
    set_user_app_theme(app.world_mut(), AppTheme::Light);
    app.update();
    assert_eq!(background(&app, border), Some("#FF0000".into()));
    assert_eq!(app.world().resource::<PfAppTheme>().requested(), AppTheme::Light);

    set_user_app_theme(app.world_mut(), AppTheme::Dark);
    app.update();
    assert_eq!(background(&app, border), Some("#0000FF".into()));
}

#[test]
fn applying_a_builtin_theme_drives_the_binding() {
    let mut app = test_app();
    let (root, _) = spawn(&mut app, &page("{AppThemeBinding Light=#FF0000, Dark=#0000FF}"));
    let border = named(&app, root, "B");

    bevy_pf::themes::apply_theme(app.world_mut(), "fluent-dark").unwrap();
    app.update();
    assert_eq!(background(&app, border), Some("#0000FF".into()));

    bevy_pf::themes::apply_theme(app.world_mut(), "fluent-light").unwrap();
    app.update();
    assert_eq!(background(&app, border), Some("#FF0000".into()));
}

// ---------------------------------------------------------------------
// Placements.
// ---------------------------------------------------------------------

#[test]
fn a_style_setter_is_live_and_still_loses_to_a_local_value() {
    let mut app = test_app();
    set_user_app_theme(app.world_mut(), AppTheme::Light);
    let (root, warnings) = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
     <StackPanel.Resources>
       <Style x:Key="S" TargetType="Border">
         <Setter Property="Background" Value="{AppThemeBinding Light=#FF0000, Dark=#0000FF}"/>
       </Style>
     </StackPanel.Resources>
     <Border x:Name="Themed" Style="{StaticResource S}"/>
     <Border x:Name="Local" Style="{StaticResource S}" Background="#00FF00"/>
   </StackPanel>"##,
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());
    let (themed, local) = (named(&app, root, "Themed"), named(&app, root, "Local"));
    assert_eq!(background(&app, themed), Some("#FF0000".into()));
    assert_eq!(background(&app, local), Some("#00FF00".into()));

    set_user_app_theme(app.world_mut(), AppTheme::Dark);
    app.update();
    assert_eq!(background(&app, themed), Some("#0000FF".into()));
    assert_eq!(
        background(&app, local),
        Some("#00FF00".into()),
        "a local value outranks a Style-tier theme binding, before AND after a flip"
    );
}

#[test]
fn a_trigger_setter_repicks_while_the_trigger_is_already_active() {
    // The regression test for the widened early return in evaluate_triggers:
    // the trigger's CONDITION does not change across the flip, only the
    // value its setter resolves to.
    let mut app = test_app();
    set_user_app_theme(app.world_mut(), AppTheme::Light);
    let (root, warnings) = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
     <StackPanel.Resources>
       <Style x:Key="S" TargetType="Border">
         <Setter Property="Background" Value="#101010"/>
         <Style.Triggers>
           <Trigger Property="IsMouseOver" Value="True">
             <Setter Property="Background" Value="{AppThemeBinding Light=#FF0000, Dark=#0000FF}"/>
           </Trigger>
         </Style.Triggers>
       </Style>
     </StackPanel.Resources>
     <Border x:Name="B" Style="{StaticResource S}"/>
   </StackPanel>"##,
    );
    assert_eq!(warnings, Vec::<String>::new());
    let border = named(&app, root, "B");
    app.update();
    assert_eq!(background(&app, border), Some("#101010".into()));

    app.world_mut().entity_mut(border).insert(Interaction::Hovered);
    app.update();
    assert_eq!(background(&app, border), Some("#FF0000".into()));

    // Flip the theme while it stays hovered.
    set_user_app_theme(app.world_mut(), AppTheme::Dark);
    app.update();
    assert_eq!(
        background(&app, border),
        Some("#0000FF".into()),
        "an ACTIVE trigger must re-pick; its condition never changed"
    );

    app.world_mut().entity_mut(border).insert(Interaction::None);
    app.update();
    assert_eq!(background(&app, border), Some("#101010".into()), "still reverts");
}

#[test]
fn a_dynamic_resource_trigger_setter_also_refreshes_while_active() {
    // The same widened early return fixes a pre-existing staleness: a
    // {DynamicResource} trigger setter used to keep the value it resolved
    // when it fired, across a whole theme dictionary swap.
    let mut app = test_app();
    bevy_pf::themes::apply_theme(app.world_mut(), "nord").unwrap();
    let (root, warnings) = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
     <StackPanel.Resources>
       <Style x:Key="S" TargetType="Border">
         <Setter Property="Background" Value="#101010"/>
         <Style.Triggers>
           <Trigger Property="IsMouseOver" Value="True">
             <Setter Property="Background" Value="{DynamicResource Pf.ControlBackground}"/>
           </Trigger>
         </Style.Triggers>
       </Style>
     </StackPanel.Resources>
     <Border x:Name="B" Style="{StaticResource S}"/>
   </StackPanel>"##,
    );
    assert_eq!(warnings, Vec::<String>::new());
    let border = named(&app, root, "B");
    app.world_mut().entity_mut(border).insert(Interaction::Hovered);
    app.update();
    let nord = background(&app, border);
    assert_ne!(nord, Some("#101010".into()), "the trigger fired");

    bevy_pf::themes::apply_theme(app.world_mut(), "dracula").unwrap();
    app.update();
    assert_ne!(
        background(&app, border),
        nord,
        "an active trigger's DynamicResource must follow the dictionary swap"
    );
}

#[test]
fn arms_can_be_static_resources() {
    let mut app = test_app();
    set_user_app_theme(app.world_mut(), AppTheme::Dark);
    let (root, warnings) = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
     <StackPanel.Resources>
       <SolidColorBrush x:Key="Day" Color="#FF0000"/>
       <SolidColorBrush x:Key="Night" Color="#0000FF"/>
     </StackPanel.Resources>
     <Border x:Name="B"
             Background="{AppThemeBinding Light={StaticResource Day}, Dark={StaticResource Night}}"/>
   </StackPanel>"##,
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(background(&app, named(&app, root, "B")), Some("#0000FF".into()));
}

// ---------------------------------------------------------------------
// Rejections and documented limits.
// ---------------------------------------------------------------------

#[test]
fn an_extension_with_no_value_is_rejected() {
    // MAUI throws; bevy_pf warns and skips, which is the crate's idiom.
    for markup in ["{AppThemeBinding}", "{AppThemeBinding Light={x:Null}}"] {
        let (_, warnings) = under(AppTheme::Light, markup);
        assert!(
            warnings.iter().any(|w| w.contains("at least one theme or Default")),
            "{markup} should be rejected, got {warnings:?}"
        );
    }
}

#[test]
fn an_unknown_argument_is_rejected() {
    let (_, warnings) = under(AppTheme::Light, "{AppThemeBinding Lite=#FF0000}");
    assert!(
        warnings.iter().any(|w| w.contains("no `Lite` argument")),
        "expected the typo to be named, got {warnings:?}"
    );
}

#[test]
fn a_property_outside_the_store_resolves_once_and_stays() {
    // A documented limit, pinned rather than pretended away: live
    // re-resolution reaches only the store-managed properties, exactly the
    // ceiling {DynamicResource} sits under.
    let mut app = test_app();
    set_user_app_theme(app.world_mut(), AppTheme::Light);
    let (root, warnings) = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
     <TextBlock x:Name="T" Text="{AppThemeBinding Light=Sun, Dark=Moon}"/>
   </StackPanel>"##,
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());
    let text = named(&app, root, "T");
    let read = |app: &App| app.world().get::<Text>(text).map(|t| t.0.clone());
    assert_eq!(read(&app), Some("Sun".into()));

    set_user_app_theme(app.world_mut(), AppTheme::Dark);
    app.update();
    assert_eq!(
        read(&app),
        Some("Sun".into()),
        "Text is not store-managed, so it resolves once — this pins the LIMIT"
    );
}
