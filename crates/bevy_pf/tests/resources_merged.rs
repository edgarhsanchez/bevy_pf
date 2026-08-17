//! MergedDictionaries, application resources, and deferred DynamicResource.

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy_pf::prelude::*;
use bevy_pf::{
    XamlEnv, instantiate_document_env, merge_application_resources, set_application_resources,
    set_application_resources_dict,
};

fn fixtures() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/merged")
}

fn fs_env() -> XamlEnv {
    XamlEnv::from_fs_root(fixtures())
}

fn spawn_env(world: &mut World, xaml: &str, env: &XamlEnv) -> (Entity, Vec<String>) {
    let doc = bevy_pf_xaml::parse(xaml).expect("parses");
    let root = world.spawn_empty().id();
    let result = instantiate_document_env(world, root, &doc, env).expect("instantiates");
    (root, result.warnings)
}

fn bg(world: &World, names: &XamlNames, name: &str) -> Color {
    world
        .get::<BackgroundColor>(names.get(name).unwrap())
        .unwrap()
        .0
}

#[test]
fn merged_dictionaries_resolve_across_files() {
    let mut world = World::new();
    let source = std::fs::read_to_string(fixtures().join("main.xaml")).unwrap();
    let (root, warnings) = spawn_env(&mut world, &source, &fs_env());
    assert!(
        warnings.is_empty(),
        "expected clean instantiation, got: {warnings:?}"
    );

    let names = world.get::<XamlNames>(root).unwrap().clone_map();
    // Two levels deep: main -> themes/theme.xaml -> ../colors.xaml.
    assert_eq!(
        bg(&world, &names, "FromColors"),
        Color::srgba_u8(0, 128, 128, 255)
    ); // Teal
    assert_eq!(
        bg(&world, &names, "FromTheme"),
        Color::srgba_u8(255, 215, 0, 255)
    ); // Gold
    // Own entry beats merged: theme.xaml overrides colors.xaml's Red with Green.
    assert_eq!(
        bg(&world, &names, "Overridden"),
        Color::srgba_u8(0, 128, 0, 255)
    );
    // Element-local entry.
    assert_eq!(
        bg(&world, &names, "Local"),
        Color::srgba_u8(128, 0, 128, 255)
    ); // Purple
    // StaticResource inside a merged file (brush referencing a color).
    assert_eq!(
        bg(&world, &names, "Accent"),
        Color::srgba_u8(0x00, 0x78, 0xD7, 255)
    );
    // x:Double through the chain.
    let sized = names.get("Sized").unwrap();
    assert_eq!(
        world.get::<bevy::text::TextFont>(sized).unwrap().font_size,
        bevy::text::FontSize::Px(21.0)
    );
}

#[test]
fn merged_dictionary_cycles_warn_but_terminate() {
    let mut world = World::new();
    let xaml = r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                              xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
         <StackPanel.Resources>
           <ResourceDictionary>
             <ResourceDictionary.MergedDictionaries>
               <ResourceDictionary Source="cyclic_a.xaml"/>
             </ResourceDictionary.MergedDictionaries>
           </ResourceDictionary>
         </StackPanel.Resources>
         <Border x:Name="A" Background="{StaticResource Brush.A}"/>
         <Border x:Name="B" Background="{StaticResource Brush.B}"/>
       </StackPanel>"#;
    let (root, warnings) = spawn_env(&mut world, xaml, &fs_env());
    assert!(
        warnings.iter().any(|w| w.contains("cycle")),
        "expected a cycle warning, got: {warnings:?}"
    );
    // Both dictionaries still contributed their entries once.
    let names = world.get::<XamlNames>(root).unwrap().clone_map();
    assert_eq!(bg(&world, &names, "A"), Color::srgba_u8(255, 0, 0, 255));
    assert_eq!(bg(&world, &names, "B"), Color::srgba_u8(0, 0, 255, 255));
}

#[test]
fn source_without_loader_warns() {
    let mut world = World::new();
    let xaml = r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation">
         <StackPanel.Resources>
           <ResourceDictionary>
             <ResourceDictionary.MergedDictionaries>
               <ResourceDictionary Source="nope.xaml"/>
             </ResourceDictionary.MergedDictionaries>
           </ResourceDictionary>
         </StackPanel.Resources>
       </StackPanel>"#;
    let (_, warnings) = spawn_env(&mut world, xaml, &XamlEnv::default());
    assert!(warnings.iter().any(|w| w.contains("loading environment")));
}

#[test]
fn application_resources_are_the_fallback_tier() {
    let mut world = World::new();
    let app_rd = bevy_pf_xaml::parse(
        r#"<ResourceDictionary xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                               xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <SolidColorBrush x:Key="App.Brush" Color="Crimson"/>
             <SolidColorBrush x:Key="Shadowed" Color="Black"/>
           </ResourceDictionary>"#,
    )
    .unwrap();
    let warnings = set_application_resources(&mut world, &app_rd, &XamlEnv::default());
    assert!(warnings.is_empty(), "{warnings:?}");

    let xaml = r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                              xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
         <StackPanel.Resources>
           <SolidColorBrush x:Key="Shadowed" Color="White"/>
         </StackPanel.Resources>
         <Border x:Name="FromApp" Background="{StaticResource App.Brush}"/>
         <Border x:Name="Shadowed" Background="{StaticResource Shadowed}"/>
       </StackPanel>"#;
    let (root, warnings) = spawn_env(&mut world, xaml, &XamlEnv::default());
    assert!(warnings.is_empty(), "{warnings:?}");
    let names = world.get::<XamlNames>(root).unwrap().clone_map();
    assert_eq!(
        bg(&world, &names, "FromApp"),
        Color::srgba_u8(0xDC, 0x14, 0x3C, 255)
    );
    // Element scope shadows the app tier.
    assert_eq!(
        bg(&world, &names, "Shadowed"),
        Color::srgba_u8(255, 255, 255, 255)
    );
}

#[test]
fn application_element_root_feeds_global_resources() {
    let mut world = World::new();
    let app_doc = bevy_pf_xaml::parse(
        r#"<Application xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <Application.Resources>
               <SolidColorBrush x:Key="Global" Color="Navy"/>
             </Application.Resources>
           </Application>"#,
    )
    .unwrap();
    let root = world.spawn_empty().id();
    instantiate_document_env(&mut world, root, &app_doc, &XamlEnv::default()).unwrap();

    // A separately spawned scene sees the Application's resources.
    let xaml = r#"<Border xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                          x:Name="B" Background="{StaticResource Global}"/>"#;
    let (root2, warnings) = spawn_env(&mut world, xaml, &XamlEnv::default());
    assert!(warnings.is_empty(), "{warnings:?}");
    let names = world.get::<XamlNames>(root2).unwrap().clone_map();
    assert_eq!(bg(&world, &names, "B"), Color::srgba_u8(0, 0, 0x80, 255));
}

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()));
    app.add_plugins(PfUiPlugin);
    app
}

#[test]
fn dynamic_resource_reresolves_on_theme_swap() {
    let mut app = test_app();
    let world = app.world_mut();

    let light = bevy_pf_xaml::parse(
        r#"<ResourceDictionary xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                               xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <SolidColorBrush x:Key="Window.Bg" Color="White"/>
           </ResourceDictionary>"#,
    )
    .unwrap();
    set_application_resources(world, &light, &XamlEnv::default());

    let xaml = r#"<Border xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                          x:Name="B" Background="{DynamicResource Window.Bg}"/>"#;
    let (root, warnings) = spawn_env(world, xaml, &XamlEnv::default());
    assert!(warnings.is_empty(), "{warnings:?}");
    app.update();

    let names = app.world().get::<XamlNames>(root).unwrap().clone_map();
    assert_eq!(
        bg(app.world(), &names, "B"),
        Color::srgba_u8(255, 255, 255, 255)
    );

    // Theme swap: replace the app dictionary; the brush re-resolves.
    let mut dark = bevy_pf::resources::ResourceDictionary::new();
    dark.insert(
        bevy_pf::resources::ResourceKey::Explicit("Window.Bg".into()),
        bevy_pf::resources::PfValue::Brush(bevy_pf_xaml::value::PfBrush::Solid(
            bevy_pf_xaml::value::PfColor::rgb(0x1E, 0x1E, 0x1E),
        )),
    );
    set_application_resources_dict(app.world_mut(), dark);
    app.update();
    assert_eq!(
        bg(app.world(), &names, "B"),
        Color::srgba_u8(0x1E, 0x1E, 0x1E, 255)
    );
}

#[test]
fn dynamic_resource_defers_until_key_appears() {
    let mut app = test_app();
    let world = app.world_mut();

    // Key does not exist yet: no warning, no color applied.
    let xaml = r#"<Border xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                          x:Name="B" Background="{DynamicResource Late.Key}"/>"#;
    let (root, warnings) = spawn_env(world, xaml, &XamlEnv::default());
    assert!(
        !warnings.iter().any(|w| w.contains("Late.Key")),
        "late DynamicResource keys must not warn: {warnings:?}"
    );
    app.update();
    let names = app.world().get::<XamlNames>(root).unwrap().clone_map();
    // Node requires BackgroundColor (default transparent); the deferred key
    // must not have painted anything yet.
    assert_eq!(
        app.world()
            .get::<BackgroundColor>(names.get("B").unwrap())
            .unwrap()
            .0
            .alpha(),
        0.0
    );

    // The key appears later (e.g. a theme merged after styles) -> applies.
    let mut late = bevy_pf::resources::ResourceDictionary::new();
    late.insert(
        bevy_pf::resources::ResourceKey::Explicit("Late.Key".into()),
        bevy_pf::resources::PfValue::Brush(bevy_pf_xaml::value::PfBrush::Solid(
            bevy_pf_xaml::value::PfColor::rgb(255, 165, 0),
        )),
    );
    merge_application_resources(app.world_mut(), late);
    app.update();
    assert_eq!(
        bg(app.world(), &names, "B"),
        Color::srgba_u8(255, 165, 0, 255)
    );
}

#[test]
fn fluent_theme_fragments_ingest_without_errors() {
    // The .NET 9 Fluent button styles (harvested, MIT) must ingest as app
    // resources with warnings only — the staged Fluent acceptance gate.
    let mut world = World::new();
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/corpus/wpf-upstream/fluent_button_styles.xaml");
    let source = std::fs::read_to_string(path).unwrap();
    let doc = bevy_pf_xaml::parse(&source).unwrap();
    let _warnings = set_application_resources(&mut world, &doc, &XamlEnv::default());
    let app = world.resource::<bevy_pf::PfApplicationResources>();
    assert!(
        !app.dict.is_empty(),
        "Fluent button styles should contribute at least some resources"
    );
}

/// Helper: clone the name map out of the borrow.
trait CloneMap {
    fn clone_map(&self) -> XamlNames;
}
impl CloneMap for XamlNames {
    fn clone_map(&self) -> XamlNames {
        XamlNames(self.0.clone())
    }
}

// ---------------------------------------------------------------------------
// Regression tests from the adversarial review
// ---------------------------------------------------------------------------

#[test]
fn sibling_and_diamond_merges_are_not_cycles() {
    // Two sibling panels merging the same file is legal WPF, not a cycle.
    let mut world = World::new();
    let xaml = r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                              xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
         <StackPanel>
           <StackPanel.Resources>
             <ResourceDictionary>
               <ResourceDictionary.MergedDictionaries>
                 <ResourceDictionary Source="colors.xaml"/>
               </ResourceDictionary.MergedDictionaries>
             </ResourceDictionary>
           </StackPanel.Resources>
           <Border x:Name="First" Background="{StaticResource Brush.FromColors}"/>
         </StackPanel>
         <StackPanel>
           <StackPanel.Resources>
             <ResourceDictionary>
               <ResourceDictionary.MergedDictionaries>
                 <ResourceDictionary Source="colors.xaml"/>
               </ResourceDictionary.MergedDictionaries>
             </ResourceDictionary>
           </StackPanel.Resources>
           <Border x:Name="Second" Background="{StaticResource Brush.FromColors}"/>
         </StackPanel>
       </StackPanel>"#;
    let (root, warnings) = spawn_env(&mut world, xaml, &fs_env());
    assert!(
        !warnings.iter().any(|w| w.contains("cycle")),
        "sibling merges must not report cycles: {warnings:?}"
    );
    let names = world.get::<XamlNames>(root).unwrap().clone_map();
    let teal = Color::srgba_u8(0, 128, 128, 255);
    assert_eq!(bg(&world, &names, "First"), teal);
    assert_eq!(bg(&world, &names, "Second"), teal);
}

#[test]
fn merged_file_does_not_see_consumer_scopes() {
    // WPF: a merged dictionary resolves its own StaticResources against
    // itself (and the app tier), never the consuming element's scopes.
    let dir = tempfile_dir();
    std::fs::write(
        dir.join("leaky.xaml"),
        r#"<ResourceDictionary xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                               xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <SolidColorBrush x:Key="Uses.Outer" Color="{StaticResource OuterColor}"/>
           </ResourceDictionary>"#,
    )
    .unwrap();

    let mut world = World::new();
    let xaml = r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                              xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
         <StackPanel.Resources>
           <Color x:Key="OuterColor">#FF123456</Color>
         </StackPanel.Resources>
         <StackPanel>
           <StackPanel.Resources>
             <ResourceDictionary>
               <ResourceDictionary.MergedDictionaries>
                 <ResourceDictionary Source="leaky.xaml"/>
               </ResourceDictionary.MergedDictionaries>
             </ResourceDictionary>
           </StackPanel.Resources>
         </StackPanel>
       </StackPanel>"#;
    let (_, warnings) = spawn_env(&mut world, xaml, &XamlEnv::from_fs_root(&dir));
    // The merged file must NOT resolve OuterColor from the consumer's scope.
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("OuterColor") && w.contains("not found")),
        "expected the merged file's brush to fail isolated resolution: {warnings:?}"
    );
}

#[test]
fn local_value_survives_theme_swap_over_style_dynamic_setter() {
    let mut app = test_app();
    let world = app.world_mut();

    let mut theme = bevy_pf::resources::ResourceDictionary::new();
    theme.insert(
        bevy_pf::resources::ResourceKey::Explicit("Bg".into()),
        bevy_pf::resources::PfValue::Brush(bevy_pf_xaml::value::PfBrush::Solid(
            bevy_pf_xaml::value::PfColor::rgb(1, 2, 3),
        )),
    );
    set_application_resources_dict(world, theme);

    let xaml = r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                              xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
         <StackPanel.Resources>
           <Style TargetType="Border">
             <Setter Property="Background" Value="{DynamicResource Bg}"/>
           </Style>
         </StackPanel.Resources>
         <!-- local Red must win over the style's dynamic setter, forever -->
         <Border x:Name="B" Background="Red"/>
       </StackPanel>"#;
    let (root, warnings) = spawn_env(world, xaml, &XamlEnv::default());
    assert!(warnings.is_empty(), "{warnings:?}");
    let names = app.world().get::<XamlNames>(root).unwrap().clone_map();
    let red = Color::srgba_u8(255, 0, 0, 255);
    assert_eq!(bg(app.world(), &names, "B"), red);

    // First update (revision already bumped) and a later swap: local wins.
    app.update();
    assert_eq!(bg(app.world(), &names, "B"), red);
    let mut swapped = bevy_pf::resources::ResourceDictionary::new();
    swapped.insert(
        bevy_pf::resources::ResourceKey::Explicit("Bg".into()),
        bevy_pf::resources::PfValue::Brush(bevy_pf_xaml::value::PfBrush::Solid(
            bevy_pf_xaml::value::PfColor::rgb(9, 9, 9),
        )),
    );
    set_application_resources_dict(app.world_mut(), swapped);
    app.update();
    assert_eq!(bg(app.world(), &names, "B"), red);
}

#[test]
fn x_null_setter_clears_based_on_background() {
    let mut world = World::new();
    let xaml = r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                              xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
         <StackPanel.Resources>
           <Style x:Key="Base" TargetType="Border">
             <Setter Property="Background" Value="Red"/>
           </Style>
           <Style x:Key="Clear" TargetType="Border" BasedOn="{StaticResource Base}">
             <Setter Property="Background" Value="{x:Null}"/>
           </Style>
         </StackPanel.Resources>
         <Border x:Name="B" Style="{StaticResource Clear}"/>
       </StackPanel>"#;
    let (root, warnings) = spawn_env(&mut world, xaml, &XamlEnv::default());
    assert!(warnings.is_empty(), "{warnings:?}");
    let names = world.get::<XamlNames>(root).unwrap().clone_map();
    // The base's Red is cleared by the derived {x:Null}.
    assert_eq!(bg(&world, &names, "B").alpha(), 0.0);
}

#[test]
fn dynamic_foreground_reaches_content_control_text() {
    let mut app = test_app();
    let world = app.world_mut();

    let mut theme = bevy_pf::resources::ResourceDictionary::new();
    theme.insert(
        bevy_pf::resources::ResourceKey::Explicit("Fg".into()),
        bevy_pf::resources::PfValue::Brush(bevy_pf_xaml::value::PfBrush::Solid(
            bevy_pf_xaml::value::PfColor::rgb(10, 20, 30),
        )),
    );
    set_application_resources_dict(world, theme);

    let xaml = r#"<Button xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                          x:Name="B" Content="Save" Foreground="{DynamicResource Fg}"/>"#;
    let (root, warnings) = spawn_env(world, xaml, &XamlEnv::default());
    assert!(warnings.is_empty(), "{warnings:?}");
    app.update();

    let names = app.world().get::<XamlNames>(root).unwrap().clone_map();
    let button = names.get("B").unwrap();
    let text_child = app
        .world()
        .get::<Children>(button)
        .unwrap()
        .iter()
        .next()
        .unwrap();
    assert_eq!(
        app.world()
            .get::<bevy::text::TextColor>(text_child)
            .unwrap()
            .0,
        Color::srgba_u8(10, 20, 30, 255)
    );

    // Swap: the generated text child updates even though the reference
    // lives on the Button.
    let mut swapped = bevy_pf::resources::ResourceDictionary::new();
    swapped.insert(
        bevy_pf::resources::ResourceKey::Explicit("Fg".into()),
        bevy_pf::resources::PfValue::Brush(bevy_pf_xaml::value::PfBrush::Solid(
            bevy_pf_xaml::value::PfColor::rgb(200, 100, 50),
        )),
    );
    set_application_resources_dict(app.world_mut(), swapped);
    app.update();
    assert_eq!(
        app.world()
            .get::<bevy::text::TextColor>(text_child)
            .unwrap()
            .0,
        Color::srgba_u8(200, 100, 50, 255)
    );
}

fn tempfile_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("bevy_pf_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
