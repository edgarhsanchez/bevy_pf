//! `{OnPlatform}` and `{OnIdiom}` end to end.
//!
//! Both resolve ONCE — the platform and the idiom are fixed for a run — so
//! these tests set `PfDevice` before building and assert what got painted.
//! The interesting cases are the ones that fail quietly if they are wrong: a
//! misspelled arm name silently meaning "Default everywhere", and an arm that
//! matches nothing on a platform the document never mentioned.

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy_pf::prelude::*;
use bevy_pf::{XamlEnv, instantiate_document_env};

fn app_on(platform: DevicePlatform, idiom: DeviceIdiom) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()));
    app.add_plugins(PfUiPlugin);
    app.insert_resource(PfDevice { platform, idiom });
    app
}

fn desktop() -> App {
    app_on(DevicePlatform::Linux, DeviceIdiom::Desktop)
}

fn spawn(app: &mut App, xaml: &str) -> (Entity, Vec<String>) {
    let doc = bevy_pf_xaml::parse(xaml).expect("parses");
    let world = app.world_mut();
    let root = world.spawn_empty().id();
    let result = instantiate_document_env(world, root, &doc, &XamlEnv::default()).expect("builds");
    (root, result.warnings)
}

fn background(app: &App, root: Entity, name: &str) -> Option<String> {
    let entity = app.world().get::<XamlNames>(root).unwrap().get(name).unwrap();
    app.world()
        .get::<BackgroundColor>(entity)
        .map(|c| bevy_pf::instantiate::color_to_hex(c.0))
}

fn page(value: &str) -> String {
    format!(
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
     <Border x:Name="B" Background="{value}"/>
   </StackPanel>"##
    )
}

/// Build on `app`'s device and return (painted colour, warnings).
fn paint(mut app: App, value: &str) -> (Option<String>, Vec<String>) {
    let (root, warnings) = spawn(&mut app, &page(value));
    app.update();
    (background(&app, root, "B"), warnings)
}

// ---------------------------------------------------------------------
// OnPlatform
// ---------------------------------------------------------------------

#[test]
fn each_platform_takes_its_own_arm() {
    let markup =
        "{OnPlatform Android=#FF0000, iOS=#00FF00, WinUI=#0000FF, Linux=#FFFF00, Default=#101010}";
    let cases = [
        (DevicePlatform::Android, "#FF0000"),
        (DevicePlatform::IOS, "#00FF00"),
        (DevicePlatform::WinUI, "#0000FF"),
        (DevicePlatform::Linux, "#FFFF00"),
        // Named by no arm: falls back to Default.
        (DevicePlatform::MacOS, "#101010"),
    ];
    for (platform, expected) in cases {
        let (color, warnings) = paint(app_on(platform, DeviceIdiom::Desktop), markup);
        assert_eq!(warnings, Vec::<String>::new(), "{platform:?}");
        assert_eq!(color, Some(expected.into()), "{platform:?}");
    }
}

#[test]
fn maui_markup_ports_unchanged() {
    // A MAUI document naming MacCatalyst must work on a Mac, and the names
    // that only exist on .NET hosts must PARSE rather than being rejected —
    // otherwise a ported file fails on a line that is valid upstream.
    let markup = "{OnPlatform Android=#FF0000, iOS=#00FF00, MacCatalyst=#00FFFF, \
                  WinUI=#0000FF, Tizen=#FF00FF, UWP=#123456, GTK=#654321, WPF=#ABCDEF, \
                  Default=#101010}";
    let (color, warnings) = paint(app_on(DevicePlatform::MacOS, DeviceIdiom::Desktop), markup);
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(color, Some("#00FFFF".into()), "MacCatalyst must match a Mac");

    // The host-only names never match, so a Linux build takes Default.
    let (color, _) = paint(app_on(DevicePlatform::Linux, DeviceIdiom::Desktop), markup);
    assert_eq!(color, Some("#101010".into()));
}

#[test]
fn native_macos_wins_over_the_catalyst_alias() {
    // Both names describe a Mac here. A bevy app on a Mac is a native Mac
    // app, so the more specific name wins regardless of source order.
    for markup in [
        "{OnPlatform macOS=#FF0000, MacCatalyst=#0000FF}",
        "{OnPlatform MacCatalyst=#0000FF, macOS=#FF0000}",
    ] {
        let (color, warnings) = paint(app_on(DevicePlatform::MacOS, DeviceIdiom::Desktop), markup);
        assert_eq!(warnings, Vec::<String>::new(), "{markup}");
        assert_eq!(color, Some("#FF0000".into()), "{markup}");
    }
}

#[test]
fn a_platform_with_no_arm_and_no_default_clears_the_property() {
    let (color, warnings) = paint(
        app_on(DevicePlatform::Linux, DeviceIdiom::Desktop),
        "{OnPlatform Android=#FF0000}",
    );
    assert_eq!(warnings, Vec::<String>::new());
    assert_ne!(color, Some("#FF0000".into()), "the Android arm must not leak");
}

// ---------------------------------------------------------------------
// OnIdiom
// ---------------------------------------------------------------------

#[test]
fn each_idiom_takes_its_own_arm() {
    let markup = "{OnIdiom Phone=#FF0000, Tablet=#00FF00, Desktop=#0000FF, TV=#FFFF00, \
                  Watch=#FF00FF, Default=#101010}";
    for (idiom, expected) in [
        (DeviceIdiom::Phone, "#FF0000"),
        (DeviceIdiom::Tablet, "#00FF00"),
        (DeviceIdiom::Desktop, "#0000FF"),
        (DeviceIdiom::TV, "#FFFF00"),
        (DeviceIdiom::Watch, "#FF00FF"),
    ] {
        let (color, warnings) = paint(app_on(DevicePlatform::Linux, idiom), markup);
        assert_eq!(warnings, Vec::<String>::new(), "{idiom:?}");
        assert_eq!(color, Some(expected.into()), "{idiom:?}");
    }
}

#[test]
fn a_positional_argument_sets_default() {
    // Default is the ContentProperty on both extensions in MAUI.
    for markup in ["{OnPlatform #00FF00}", "{OnIdiom #00FF00}"] {
        let (color, warnings) = paint(desktop(), markup);
        assert_eq!(warnings, Vec::<String>::new(), "{markup}");
        assert_eq!(color, Some("#00FF00".into()), "{markup}");
    }
}

// ---------------------------------------------------------------------
// The failure that is otherwise silent.
// ---------------------------------------------------------------------

#[test]
fn a_misspelled_arm_is_reported_with_the_alternatives() {
    // Left unchecked, `Windows=` would simply never match and the document
    // would take Default on EVERY platform — working "fine" everywhere and
    // correct nowhere.
    let (_, warnings) = paint(desktop(), "{OnPlatform Windows=#FF0000, Default=#101010}");
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("no `Windows` argument") && w.contains("WinUI")),
        "expected the typo named and the alternatives offered, got {warnings:?}"
    );

    let (_, warnings) = paint(desktop(), "{OnIdiom Mobile=#FF0000, Default=#101010}");
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("no `Mobile` argument") && w.contains("Phone")),
        "got {warnings:?}"
    );
}

#[test]
fn an_extension_with_no_value_is_rejected() {
    for (markup, word) in [
        ("{OnPlatform}", "platform"),
        ("{OnIdiom}", "idiom"),
        ("{OnPlatform Android={x:Null}}", "platform"),
    ] {
        let (_, warnings) = paint(desktop(), markup);
        assert!(
            warnings
                .iter()
                .any(|w| w.contains(&format!("at least one {word} or Default"))),
            "{markup} should be rejected, got {warnings:?}"
        );
    }
}

// ---------------------------------------------------------------------
// Placements and arm kinds.
// ---------------------------------------------------------------------

#[test]
fn a_style_setter_can_select_by_platform() {
    let mut app = app_on(DevicePlatform::Android, DeviceIdiom::Phone);
    let (root, warnings) = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
     <StackPanel.Resources>
       <Style x:Key="S" TargetType="Border">
         <Setter Property="Background" Value="{OnPlatform Android=#FF0000, Default=#101010}"/>
       </Style>
     </StackPanel.Resources>
     <Border x:Name="B" Style="{StaticResource S}"/>
   </StackPanel>"##,
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(background(&app, root, "B"), Some("#FF0000".into()));
}

#[test]
fn arms_can_be_static_resources() {
    let mut app = app_on(DevicePlatform::IOS, DeviceIdiom::Phone);
    let (root, warnings) = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
     <StackPanel.Resources>
       <SolidColorBrush x:Key="Warm" Color="#FF0000"/>
       <SolidColorBrush x:Key="Cool" Color="#0000FF"/>
     </StackPanel.Resources>
     <Border x:Name="B"
             Background="{OnPlatform iOS={StaticResource Cool}, Default={StaticResource Warm}}"/>
   </StackPanel>"##,
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(background(&app, root, "B"), Some("#0000FF".into()));
}

#[test]
fn a_dynamic_resource_arm_stays_live() {
    // Picking the arm is static; what the picked KEY resolves to is not.
    // Resolving it eagerly would silently freeze it at the first theme.
    let mut app = app_on(DevicePlatform::Linux, DeviceIdiom::Desktop);
    bevy_pf::themes::apply_theme(app.world_mut(), "nord").unwrap();
    let (root, warnings) = spawn(
        &mut app,
        &page("{OnPlatform Linux={DynamicResource Pf.ControlBackground}, Default=#101010}"),
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());
    let nord = background(&app, root, "B");
    assert_ne!(nord, Some("#101010".into()), "the Linux arm was taken");

    bevy_pf::themes::apply_theme(app.world_mut(), "dracula").unwrap();
    app.update();
    assert_ne!(
        background(&app, root, "B"),
        nord,
        "a DynamicResource arm must follow the dictionary swap"
    );
}

#[test]
fn selectors_and_theme_bindings_compose_in_one_document() {
    // The two extensions resolve at different times — the selector once, the
    // theme binding on every flip — so a document using both must not have
    // one clobber the other.
    let mut app = app_on(DevicePlatform::Android, DeviceIdiom::Phone);
    set_user_app_theme(app.world_mut(), AppTheme::Light);
    let (root, warnings) = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
     <Border x:Name="ByPlatform" Background="{OnPlatform Android=#FF0000, Default=#101010}"/>
     <Border x:Name="ByTheme" Background="{AppThemeBinding Light=#00FF00, Dark=#0000FF}"/>
   </StackPanel>"##,
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(background(&app, root, "ByPlatform"), Some("#FF0000".into()));
    assert_eq!(background(&app, root, "ByTheme"), Some("#00FF00".into()));

    set_user_app_theme(app.world_mut(), AppTheme::Dark);
    app.update();
    assert_eq!(
        background(&app, root, "ByPlatform"),
        Some("#FF0000".into()),
        "a theme flip must not disturb a platform selection"
    );
    assert_eq!(background(&app, root, "ByTheme"), Some("#0000FF".into()));
}

// ---------------------------------------------------------------------
// Dialect vocabulary that maps onto concepts bevy_pf already has.
// ---------------------------------------------------------------------

#[test]
fn content_alignment_overrides_the_per_kind_default() {
    // A Button centres its content and a Border lead-aligns it. Saying so
    // explicitly must win over that default — this is the gap the
    // ControlTemplate plan carried as "deferred" for the whole of phase 2.
    let mut app = desktop();
    let (root, warnings) = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
     <Button x:Name="Default" Content="a"/>
     <Button x:Name="Lead" Content="b" HorizontalContentAlignment="Left"
             VerticalContentAlignment="Top"/>
   </StackPanel>"##,
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());

    let node = |name: &str| {
        let e = app.world().get::<XamlNames>(root).unwrap().get(name).unwrap();
        let n = app.world().get::<Node>(e).unwrap();
        (n.justify_items, n.align_items)
    };
    assert_eq!(
        node("Default"),
        (JustifyItems::Center, AlignItems::Center),
        "a Button still centres by default"
    );
    assert_eq!(
        node("Lead"),
        (JustifyItems::Start, AlignItems::Start),
        "explicit alignment must beat the per-kind default"
    );
}

#[test]
fn an_unknown_content_alignment_is_reported() {
    let mut app = desktop();
    let (_, warnings) = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
     <Button x:Name="B" Content="a" HorizontalContentAlignment="Middle"/>
   </StackPanel>"##,
    );
    assert!(
        warnings.iter().any(|w| w.contains("Middle")),
        "got {warnings:?}"
    );
}

#[test]
fn per_axis_spacing_reaches_the_gaps() {
    let mut app = desktop();
    let (root, warnings) = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
     <Grid x:Name="G" RowSpacing="7" ColumnSpacing="3"/>
     <WrapPanel x:Name="Across" Orientation="Horizontal" ItemSpacing="5"/>
     <WrapPanel x:Name="Down" Orientation="Vertical" ItemSpacing="5"/>
   </StackPanel>"##,
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());
    let gaps = |name: &str| {
        let e = app.world().get::<XamlNames>(root).unwrap().get(name).unwrap();
        let n = app.world().get::<Node>(e).unwrap();
        (n.row_gap, n.column_gap)
    };
    assert_eq!(gaps("G"), (Val::Px(7.0), Val::Px(3.0)));
    // ItemSpacing separates items ALONG the flow, so it swaps with orientation.
    assert_eq!(gaps("Across").1, Val::Px(5.0), "horizontal: column gap");
    assert_eq!(gaps("Down").0, Val::Px(5.0), "vertical: row gap");
}
