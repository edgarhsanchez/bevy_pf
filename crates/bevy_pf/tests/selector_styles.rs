//! Avalonia selector styling, applied to a real element tree.
//!
//! Phase 1 covers the NON-ACTIVATED bucket: selectors made of types, names
//! and structure, whose membership is fixed once the tree exists. They write
//! at the same tier as a WPF style, in attach order — Avalonia's "last
//! attached wins", which is what `PfPropertyStore` already does.

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

fn background(app: &App, root: Entity, name: &str) -> Option<String> {
    let e = app.world().get::<XamlNames>(root).unwrap().get(name).unwrap();
    app.world()
        .get::<BackgroundColor>(e)
        .map(|c| bevy_pf::instantiate::color_to_hex(c.0))
}

/// A page whose root carries a `Styles` collection.
fn styled_page(styles: &str, body: &str) -> String {
    format!(
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
  <StackPanel.Styles>
{styles}
  </StackPanel.Styles>
{body}
</StackPanel>"##
    )
}

#[test]
fn a_type_selector_styles_every_element_of_that_type() {
    let mut app = test_app();
    let (root, warnings) = spawn(
        &mut app,
        &styled_page(
            r##"<Style Selector="Border"><Setter Property="Background" Value="#FF0000"/></Style>"##,
            r##"<Border x:Name="A"/><Border x:Name="B"/><TextBlock x:Name="T" Text="x"/>"##,
        ),
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(background(&app, root, "A"), Some("#FF0000".into()));
    assert_eq!(background(&app, root, "B"), Some("#FF0000".into()));
    assert_ne!(
        background(&app, root, "T"),
        Some("#FF0000".into()),
        "a TextBlock is not a Border"
    );
}

#[test]
fn a_name_selector_styles_exactly_one_element() {
    let mut app = test_app();
    let (root, warnings) = spawn(
        &mut app,
        &styled_page(
            r##"<Style Selector="Border#Target"><Setter Property="Background" Value="#00FF00"/></Style>"##,
            r##"<Border x:Name="Target"/><Border x:Name="Other"/>"##,
        ),
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(background(&app, root, "Target"), Some("#00FF00".into()));
    assert_ne!(background(&app, root, "Other"), Some("#00FF00".into()));
}

#[test]
fn a_child_combinator_does_not_reach_a_grandchild() {
    // `UniformGrid > Button` is the corpus's most-used structural selector,
    // and the whole point of `>` is that it stops at one level.
    let mut app = test_app();
    let (root, warnings) = spawn(
        &mut app,
        &styled_page(
            r##"<Style Selector="StackPanel > Border"><Setter Property="Background" Value="#0000FF"/></Style>"##,
            r##"<Border x:Name="Direct"/>
               <Grid><Border x:Name="Grandchild"/></Grid>"##,
        ),
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(background(&app, root, "Direct"), Some("#0000FF".into()));
    assert_ne!(
        background(&app, root, "Grandchild"),
        Some("#0000FF".into()),
        "a child combinator must not reach through the Grid"
    );
}

#[test]
fn a_descendant_combinator_does_reach_a_grandchild() {
    let mut app = test_app();
    let (root, warnings) = spawn(
        &mut app,
        &styled_page(
            r##"<Style Selector="StackPanel Border"><Setter Property="Background" Value="#0000FF"/></Style>"##,
            r##"<Grid><Border x:Name="Deep"/></Grid>"##,
        ),
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(background(&app, root, "Deep"), Some("#0000FF".into()));
}

#[test]
fn comma_alternation_styles_both_types() {
    let mut app = test_app();
    let (root, warnings) = spawn(
        &mut app,
        &styled_page(
            r##"<Style Selector="Border, Grid"><Setter Property="Background" Value="#ABCDEF"/></Style>"##,
            r##"<Border x:Name="B"/><Grid x:Name="G"/><TextBlock x:Name="T" Text="x"/>"##,
        ),
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(background(&app, root, "B"), Some("#ABCDEF".into()));
    assert_eq!(background(&app, root, "G"), Some("#ABCDEF".into()));
    assert_ne!(background(&app, root, "T"), Some("#ABCDEF".into()));
}

#[test]
fn the_last_matching_style_wins() {
    // Avalonia computes NO specificity: inside a bucket it is simply the
    // last style attached that wins. A CSS engine would let the more
    // specific `Border#B` beat a later bare `Border`; Avalonia does not.
    let mut app = test_app();
    let (root, warnings) = spawn(
        &mut app,
        &styled_page(
            r##"<Style Selector="Border#B"><Setter Property="Background" Value="#111111"/></Style>
               <Style Selector="Border"><Setter Property="Background" Value="#222222"/></Style>"##,
            r##"<Border x:Name="B"/>"##,
        ),
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(
        background(&app, root, "B"),
        Some("#222222".into()),
        "later attachment wins over a more 'specific' earlier selector"
    );
}

#[test]
fn a_local_attribute_still_beats_a_selector_style() {
    let mut app = test_app();
    let (root, warnings) = spawn(
        &mut app,
        &styled_page(
            r##"<Style Selector="Border"><Setter Property="Background" Value="#111111"/></Style>"##,
            r##"<Border x:Name="B" Background="#00FF00"/>"##,
        ),
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(
        background(&app, root, "B"),
        Some("#00FF00".into()),
        "a selector style writes at the Style tier, below a local value"
    );
}

#[test]
fn styles_apply_only_inside_the_subtree_that_declares_them() {
    let mut app = test_app();
    let (root, warnings) = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
      <Grid>
        <Grid.Styles>
          <Style Selector="Border"><Setter Property="Background" Value="#FF0000"/></Style>
        </Grid.Styles>
        <Border x:Name="Inside"/>
      </Grid>
      <Border x:Name="Outside"/>
    </StackPanel>"##,
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(background(&app, root, "Inside"), Some("#FF0000".into()));
    assert_ne!(
        background(&app, root, "Outside"),
        Some("#FF0000".into()),
        "a Styles collection reaches its own subtree, not its siblings"
    );
}

#[test]
fn a_malformed_selector_is_reported_rather_than_matching_nothing() {
    let mut app = test_app();
    let (_, warnings) = spawn(
        &mut app,
        &styled_page(
            r##"<Style Selector="Border >"><Setter Property="Background" Value="#FF0000"/></Style>"##,
            r##"<Border x:Name="B"/>"##,
        ),
    );
    assert!(
        warnings.iter().any(|w| w.contains("selector")),
        "a broken selector must say so, got {warnings:?}"
    );
}

#[test]
fn a_style_without_a_selector_is_reported() {
    let mut app = test_app();
    let (_, warnings) = spawn(
        &mut app,
        &styled_page(
            r##"<Style><Setter Property="Background" Value="#FF0000"/></Style>"##,
            r##"<Border x:Name="B"/>"##,
        ),
    );
    assert!(
        warnings.iter().any(|w| w.contains("needs a Selector")),
        "got {warnings:?}"
    );
}

// ---------------------------------------------------------------------
// Phase 2: the ACTIVATED bucket — classes and pseudo-classes.
//
// These compile to ordinary triggers, because bevy_pf's condition set
// already covers every Avalonia pseudo-class. They write at a higher tier
// than a plain selector style, which is Avalonia's ordering.
// ---------------------------------------------------------------------

#[test]
fn a_class_selector_styles_only_elements_carrying_the_class() {
    let mut app = test_app();
    let (root, warnings) = spawn(
        &mut app,
        &styled_page(
            r##"<Style Selector="Border.danger"><Setter Property="Background" Value="#FF0000"/></Style>"##,
            r##"<Border x:Name="Yes" Classes="danger"/><Border x:Name="No"/>"##,
        ),
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(background(&app, root, "Yes"), Some("#FF0000".into()));
    assert_ne!(background(&app, root, "No"), Some("#FF0000".into()));
}

#[test]
fn one_of_several_classes_is_enough() {
    let mut app = test_app();
    let (root, warnings) = spawn(
        &mut app,
        &styled_page(
            r##"<Style Selector=".accent"><Setter Property="Background" Value="#00FF00"/></Style>"##,
            r##"<Border x:Name="B" Classes="big accent rounded"/>"##,
        ),
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(background(&app, root, "B"), Some("#00FF00".into()));
}

#[test]
fn an_activated_style_outranks_a_plain_one() {
    // Avalonia puts activated styles in a higher bucket than non-activated
    // ones, regardless of declaration order — so the plain `Border` rule
    // declared LAST still loses to the class rule declared first.
    let mut app = test_app();
    let (root, warnings) = spawn(
        &mut app,
        &styled_page(
            r##"<Style Selector="Border.danger"><Setter Property="Background" Value="#FF0000"/></Style>
               <Style Selector="Border"><Setter Property="Background" Value="#222222"/></Style>"##,
            r##"<Border x:Name="B" Classes="danger"/>"##,
        ),
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(
        background(&app, root, "B"),
        Some("#FF0000".into()),
        "the activated bucket wins over the plain one"
    );
}

#[test]
fn a_pseudo_class_follows_the_interaction_state() {
    // :pointerover is bevy_pf's existing IsMouseOver condition, so this
    // rides the trigger runtime that already existed — and reverts
    // structurally when the pointer leaves.
    let mut app = test_app();
    let (root, warnings) = spawn(
        &mut app,
        &styled_page(
            r##"<Style Selector="Border"><Setter Property="Background" Value="#111111"/></Style>
               <Style Selector="Border:pointerover"><Setter Property="Background" Value="#FF0000"/></Style>"##,
            r##"<Border x:Name="B"/>"##,
        ),
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());
    let border = app.world().get::<XamlNames>(root).unwrap().get("B").unwrap();
    assert_eq!(background(&app, root, "B"), Some("#111111".into()), "at rest");

    app.world_mut().entity_mut(border).insert(Interaction::Hovered);
    app.update();
    assert_eq!(background(&app, root, "B"), Some("#FF0000".into()), "hovered");

    app.world_mut().entity_mut(border).insert(Interaction::None);
    app.update();
    assert_eq!(
        background(&app, root, "B"),
        Some("#111111".into()),
        "reverts structurally to the plain style"
    );
}

#[test]
fn a_class_and_a_pseudo_class_must_both_hold() {
    let mut app = test_app();
    let (root, warnings) = spawn(
        &mut app,
        &styled_page(
            r##"<Style Selector="Border.danger:pointerover"><Setter Property="Background" Value="#FF0000"/></Style>"##,
            r##"<Border x:Name="Plain"/><Border x:Name="Danger" Classes="danger"/>"##,
        ),
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());
    let plain = app.world().get::<XamlNames>(root).unwrap().get("Plain").unwrap();
    let danger = app.world().get::<XamlNames>(root).unwrap().get("Danger").unwrap();

    // Hovering the one WITHOUT the class must not style it.
    app.world_mut().entity_mut(plain).insert(Interaction::Hovered);
    app.world_mut().entity_mut(danger).insert(Interaction::Hovered);
    app.update();
    assert_ne!(background(&app, root, "Plain"), Some("#FF0000".into()));
    assert_eq!(background(&app, root, "Danger"), Some("#FF0000".into()));
}

#[test]
fn adding_a_class_at_runtime_restyles_the_element() {
    // The reason classes are an ACTIVATED selector at all: membership can
    // change after the tree is built, and the style has to follow.
    let mut app = test_app();
    let (root, warnings) = spawn(
        &mut app,
        &styled_page(
            r##"<Style Selector="Border.danger"><Setter Property="Background" Value="#FF0000"/></Style>"##,
            r##"<Border x:Name="B"/>"##,
        ),
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());
    assert_ne!(background(&app, root, "B"), Some("#FF0000".into()));

    let border = app.world().get::<XamlNames>(root).unwrap().get("B").unwrap();
    app.world_mut()
        .entity_mut(border)
        .insert(bevy_pf::components::PfClasses(vec!["danger".into()]));
    app.update();
    assert_eq!(
        background(&app, root, "B"),
        Some("#FF0000".into()),
        "the style follows the class"
    );

    app.world_mut()
        .entity_mut(border)
        .insert(bevy_pf::components::PfClasses(vec![]));
    app.update();
    assert_ne!(
        background(&app, root, "B"),
        Some("#FF0000".into()),
        "and reverts when the class goes away"
    );
}

#[test]
fn a_class_selector_still_loses_to_a_local_value() {
    let mut app = test_app();
    let (root, warnings) = spawn(
        &mut app,
        &styled_page(
            r##"<Style Selector="Border.danger"><Setter Property="Background" Value="#FF0000"/></Style>"##,
            r##"<Border x:Name="B" Classes="danger" Background="#00FF00"/>"##,
        ),
    );
    app.update();
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(
        background(&app, root, "B"),
        Some("#00FF00".into()),
        "a local attribute is above every style tier"
    );
}
