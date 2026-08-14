//! Real-layout hit-testing: bevy_ui's layout runs headless, so these
//! assertions are made against actual pixel geometry rather than a model
//! of it. Every case here is a trap a hand-rolled hit test falls into.

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy_pf::prelude::*;
use bevy_pf::{PfHitFilter, PfHitTest};

const W: u32 = 1280;
const H: u32 = 800;

fn centre() -> Vec2 {
    Vec2::new(W as f32 / 2.0, H as f32 / 2.0)
}

/// Inside the 400x200 panel but clear of the label centred in it, so
/// the topmost painted node under this point is the panel itself.
fn on_panel_chrome() -> Vec2 {
    centre() + Vec2::new(150.0, 60.0)
}

fn layout_app(scale_factor: f32) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        bevy::window::WindowPlugin {
            primary_window: None,
            exit_condition: bevy::window::ExitCondition::DontExit,
            ..Default::default()
        },
        bevy::a11y::AccessibilityPlugin,
        bevy::input::InputPlugin,
        bevy::picking::DefaultPickingPlugins,
        bevy::text::TextPlugin,
        bevy::ui::UiPlugin,
    ));
    app.init_asset::<Image>();
    app.init_asset::<bevy::image::TextureAtlasLayout>();
    app.add_plugins(PfUiPlugin);
    app.world_mut().spawn((
        bevy::window::Window {
            resolution: bevy::window::WindowResolution::new(W, H)
                .with_scale_factor_override(scale_factor),
            ..Default::default()
        },
        bevy::window::PrimaryWindow,
    ));
    let mut camera = Camera::default();
    camera.computed.target_info = Some(bevy::camera::RenderTargetInfo {
        physical_size: UVec2::new(
            (W as f32 * scale_factor) as u32,
            (H as f32 * scale_factor) as u32,
        ),
        scale_factor,
    });
    app.world_mut().spawn((Camera2d, camera));
    app
}

fn mount(app: &mut App, xaml: &str) -> Entity {
    let doc = bevy_pf_xaml::parse(xaml).expect("parses");
    let world = app.world_mut();
    let root = world.spawn_empty().id();
    bevy_pf::instantiate_document_env(world, root, &doc, &bevy_pf::XamlEnv::default())
        .expect("instantiates");
    for _ in 0..5 {
        app.update();
    }
    root
}

/// Run a hit test through the real `SystemParam`, exactly as a consumer
/// would from a system.
fn hit(app: &mut App, point: Vec2, filter: PfHitFilter) -> Option<Entity> {
    let mut system =
        bevy::ecs::system::IntoSystem::into_system(move |hit: PfHitTest| hit.hit(point, filter));
    system.initialize(app.world_mut());
    system.run((), app.world_mut()).expect("hit test runs")
}

fn hit_in(app: &mut App, root: Entity, point: Vec2, filter: PfHitFilter) -> Option<Entity> {
    let mut system = bevy::ecs::system::IntoSystem::into_system(move |hit: PfHitTest| {
        hit.hit_in(root, point, filter)
    });
    system.initialize(app.world_mut());
    system.run((), app.world_mut()).expect("hit test runs")
}

/// A panel with a painted background, floating in a full-window document.
/// The wrapper covers the whole window; only the panel is drawn.
const PANEL: &str = r##"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
      <Border x:Name="Panel" Background="#FF102030" Width="400" Height="200"
              HorizontalAlignment="Center" VerticalAlignment="Center">
        <TextBlock x:Name="Label" Text="readout" Foreground="#FFFFFFFF"
                   HorizontalAlignment="Center" VerticalAlignment="Center"/>
      </Border>
    </Grid>"##;

/// The headline case: a press on the transparent part of a full-window
/// document is NOT a press on UI. `Visible` cannot tell the difference —
/// the root box covers everything — which is exactly why `Painted` exists.
#[test]
fn painted_filter_ignores_the_full_window_wrapper() {
    let mut app = layout_app(1.0);
    app.update();
    let root = mount(&mut app, PANEL);
    let panel = app
        .world()
        .get::<XamlNames>(root)
        .unwrap()
        .get("Panel")
        .unwrap();

    let chrome = on_panel_chrome();
    let corner = Vec2::new(20.0, 20.0);

    assert_eq!(
        hit(&mut app, chrome, PfHitFilter::Painted),
        Some(panel),
        "a press on the panel finds the panel"
    );
    assert_eq!(
        hit(&mut app, corner, PfHitFilter::Painted),
        None,
        "a press on the empty corner finds nothing — the wrapper paints nothing"
    );
    // The same corner press under `Visible` hits the wrapper, which is the
    // whole reason a naive walk reports "always inside".
    assert!(
        hit(&mut app, corner, PfHitFilter::Visible).is_some(),
        "Visible matches the full-window wrapper (documents the difference)"
    );
}

/// `Visibility::Hidden` leaves a node in layout with full geometry. A
/// hidden panel must not answer for the pixels it would have covered.
#[test]
fn hidden_panels_do_not_answer_for_their_geometry() {
    let mut app = layout_app(1.0);
    app.update();
    let root = mount(&mut app, PANEL);
    let panel = app
        .world()
        .get::<XamlNames>(root)
        .unwrap()
        .get("Panel")
        .unwrap();
    let chrome = on_panel_chrome();
    assert_eq!(hit(&mut app, chrome, PfHitFilter::Painted), Some(panel));

    *app.world_mut().get_mut::<Visibility>(panel).unwrap() = Visibility::Hidden;
    for _ in 0..3 {
        app.update();
    }
    // Layout still places it — that is the trap.
    let node = app.world().get::<bevy::ui::ComputedNode>(panel).unwrap();
    assert!(
        node.size().x > 0.0,
        "a hidden node keeps its geometry (precondition for this test)"
    );
    assert_eq!(
        hit(&mut app, chrome, PfHitFilter::Painted),
        None,
        "a hidden panel is not under the pointer"
    );
}

/// bevy resolves `Visibility::Visible` on a descendant as visible even
/// under a hidden ancestor, so the subtree cannot simply be pruned.
#[test]
fn visible_child_of_hidden_parent_still_hits() {
    let mut app = layout_app(1.0);
    app.update();
    let root = mount(&mut app, PANEL);
    let names = app.world().get::<XamlNames>(root).unwrap();
    let panel = names.get("Panel").unwrap();
    let label = names.get("Label").unwrap();

    *app.world_mut().get_mut::<Visibility>(panel).unwrap() = Visibility::Hidden;
    *app.world_mut().get_mut::<Visibility>(label).unwrap() = Visibility::Visible;
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(
        hit(&mut app, centre(), PfHitFilter::Painted),
        Some(label),
        "the forced-visible label survives its hidden parent, as bevy propagates it"
    );
}

/// `Pickable::IGNORE` is about events, not pixels: a passive readout is
/// still painted. The two filters must disagree here, and each must be
/// right for its own question.
#[test]
fn pickable_and_painted_are_different_questions() {
    let mut app = layout_app(1.0);
    app.update();
    let root = mount(&mut app, PANEL);
    let names = app.world().get::<XamlNames>(root).unwrap();
    let panel = names.get("Panel").unwrap();
    let label = names.get("Label").unwrap();
    for e in [root, panel, label] {
        app.world_mut()
            .entity_mut(e)
            .insert(bevy::picking::Pickable::IGNORE);
    }
    for _ in 0..3 {
        app.update();
    }
    let chrome = on_panel_chrome();
    assert_eq!(
        hit(&mut app, chrome, PfHitFilter::HitTestVisible),
        None,
        "an opted-out tree takes no clicks"
    );
    assert_eq!(
        hit(&mut app, chrome, PfHitFilter::Painted),
        Some(panel),
        "...but it is still visibly there"
    );
}

/// Node geometry is physical, pointers are logical. At scale factor 2 an
/// unconverted test is wrong by a factor of two — which lands a centre
/// press outside a 400x200 panel.
#[test]
fn hit_points_are_logical_not_physical() {
    let mut app = layout_app(2.0);
    app.update();
    let root = mount(&mut app, PANEL);
    let panel = app
        .world()
        .get::<XamlNames>(root)
        .unwrap()
        .get("Panel")
        .unwrap();
    let node = app.world().get::<bevy::ui::ComputedNode>(panel).unwrap();
    assert!(
        (node.inverse_scale_factor() - 0.5).abs() < 0.01,
        "scale factor 2 reached layout (precondition)"
    );
    assert!(
        node.size().x > 700.0,
        "panel is ~800 PHYSICAL px wide at 2x (precondition)"
    );

    assert_eq!(
        hit(&mut app, on_panel_chrome(), PfHitFilter::Painted),
        Some(panel),
        "a logical point inside the panel hits it at 2x"
    );
    // 400 logical px right of centre is outside the 400-wide panel, but
    // would still be inside it if the point were treated as physical.
    let outside = centre() + Vec2::new(400.0, 0.0);
    assert_eq!(
        hit(&mut app, outside, PfHitFilter::Painted),
        None,
        "a logical point beyond the panel misses, even though it is inside \
         the panel's PHYSICAL rect"
    );
}

/// Deeper and later-painted nodes win, so the returned entity is the one
/// the user would say they touched.
#[test]
fn topmost_node_wins() {
    let mut app = layout_app(1.0);
    app.update();
    let root = mount(
        &mut app,
        r##"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                  xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
              <Border x:Name="Under" Background="#FF102030" Width="400" Height="200"
                      HorizontalAlignment="Center" VerticalAlignment="Center"/>
              <Border x:Name="Over" Background="#FF804020" Width="100" Height="100"
                      HorizontalAlignment="Center" VerticalAlignment="Center"/>
            </Grid>"##,
    );
    let names = app.world().get::<XamlNames>(root).unwrap();
    let under = names.get("Under").unwrap();
    let over = names.get("Over").unwrap();
    let centre = centre();
    assert_eq!(
        hit(&mut app, centre, PfHitFilter::Painted),
        Some(over),
        "the later sibling paints on top and takes the hit"
    );
    // Just outside the small panel but inside the big one.
    let edge = centre + Vec2::new(120.0, 0.0);
    assert_eq!(hit(&mut app, edge, PfHitFilter::Painted), Some(under));
}

/// `hit_in` is scoped: a press on one document must not be reported as a
/// press on another.
#[test]
fn hit_in_is_scoped_to_its_subtree() {
    let mut app = layout_app(1.0);
    app.update();
    let left = mount(
        &mut app,
        r##"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                  xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
              <Border x:Name="L" Background="#FF102030" Width="200" Height="200"
                      HorizontalAlignment="Left" VerticalAlignment="Top"/>
            </Grid>"##,
    );
    let right = mount(
        &mut app,
        r##"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                  xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
              <Border x:Name="R" Background="#FF203040" Width="200" Height="200"
                      HorizontalAlignment="Right" VerticalAlignment="Top"/>
            </Grid>"##,
    );
    let l = app
        .world()
        .get::<XamlNames>(left)
        .unwrap()
        .get("L")
        .unwrap();
    let r = app
        .world()
        .get::<XamlNames>(right)
        .unwrap()
        .get("R")
        .unwrap();

    let on_left = Vec2::new(100.0, 100.0);
    let on_right = Vec2::new(W as f32 - 100.0, 100.0);
    assert_eq!(
        hit_in(&mut app, left, on_left, PfHitFilter::Painted),
        Some(l)
    );
    assert_eq!(hit_in(&mut app, left, on_right, PfHitFilter::Painted), None);
    assert_eq!(
        hit_in(&mut app, right, on_right, PfHitFilter::Painted),
        Some(r)
    );
    assert_eq!(hit_in(&mut app, right, on_left, PfHitFilter::Painted), None);
}

// ---------------------------------------------------------------------------
// WPF hit-testing conformance
// ---------------------------------------------------------------------------

fn pickable_of(app: &App, e: Entity) -> Option<bevy::picking::Pickable> {
    app.world().get::<bevy::picking::Pickable>(e).copied()
}

fn takes_clicks(app: &App, e: Entity) -> bool {
    // Absent `Pickable` is bevy's "blocks and hovers" default.
    pickable_of(app, e).is_none_or(|p| p.should_block_lower || p.is_hoverable)
}

/// WPF `UIElement.IsHitTestVisible="False"` excludes the element AND every
/// descendant — the hit-test walk never enters the subtree, so an inner
/// `IsHitTestVisible="True"` cannot claw its way back out.
#[test]
fn is_hit_test_visible_false_covers_the_whole_subtree() {
    let mut app = layout_app(1.0);
    app.update();
    let root = mount(
        &mut app,
        r##"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                  xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                  Background="#FF101010">
              <StackPanel x:Name="Passive" IsHitTestVisible="False" Background="#FF202020">
                <Button x:Name="Inner" Content="unreachable"/>
                <TextBlock x:Name="Deep" Text="also unreachable" IsHitTestVisible="True"/>
              </StackPanel>
            </Grid>"##,
    );
    let names = app.world().get::<XamlNames>(root).unwrap();
    let passive = names.get("Passive").unwrap();
    let inner = names.get("Inner").unwrap();
    let deep = names.get("Deep").unwrap();

    assert!(
        !takes_clicks(&app, passive),
        "the declaring element opts out"
    );
    assert!(
        !takes_clicks(&app, inner),
        "a Button inside a non-hit-test-visible panel takes no clicks"
    );
    assert!(
        !takes_clicks(&app, deep),
        "an inner IsHitTestVisible=True cannot re-expose itself (WPF never \
         descends into the subtree)"
    );

    // Flipping it back on restores the subtree.
    *app.world_mut()
        .get_mut::<bevy_pf::PfHitTestVisible>(passive)
        .unwrap() = bevy_pf::PfHitTestVisible(true);
    for _ in 0..3 {
        app.update();
    }
    assert!(
        takes_clicks(&app, passive),
        "restored on the declaring element"
    );
    assert!(takes_clicks(&app, inner), "restored through the subtree");
}

/// WPF's most-stubbed toe: a `Panel` renders nothing but its Background, so
/// a null Background means clicks pass straight through — and
/// `Background="Transparent"` is the idiom for "invisible but clickable".
/// bevy has no such rule, which is how a layout Grid wrapped around a screen
/// ends up eating every click aimed beneath it.
#[test]
fn a_panel_hit_tests_only_where_it_has_a_background() {
    let mut app = layout_app(1.0);
    app.update();
    let root = mount(
        &mut app,
        r##"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                  xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
              <StackPanel x:Name="Bare"/>
              <StackPanel x:Name="Clear" Background="Transparent"/>
              <StackPanel x:Name="Painted" Background="#FF203040"/>
              <Border x:Name="Chrome"/>
              <Button x:Name="Btn" Content="ok"/>
            </Grid>"##,
    );
    let names = app.world().get::<XamlNames>(root).unwrap();
    let get = |n: &str| names.get(n).unwrap();

    assert!(
        !takes_clicks(&app, root),
        "the document's own Grid root has no Background: clicks fall through \
         it to the scene behind"
    );
    assert!(
        !takes_clicks(&app, get("Bare")),
        "null Background: transparent to clicks"
    );
    assert!(
        takes_clicks(&app, get("Clear")),
        "Background=\"Transparent\" is a real brush — invisible but clickable"
    );
    assert!(
        takes_clicks(&app, get("Painted")),
        "a painted panel takes clicks"
    );
    assert!(
        takes_clicks(&app, get("Chrome")),
        "Border is exempt: WPF hits its border ring, a shape rectangular \
         picking cannot express, so it stays hit-testable"
    );
    assert!(
        takes_clicks(&app, get("Btn")),
        "controls are never governed by this rule"
    );
}

/// The rule follows the property, not the parse: a Background arriving from
/// a Style or a binding opts the panel back in, and clearing it opts back out.
#[test]
fn the_background_rule_follows_later_writes() {
    #[derive(bevy::reflect::Reflect, Default)]
    struct Vm {
        fill: String,
    }

    let mut app = layout_app(1.0);
    app.update();
    let vm = Bindable::new(Vm {
        fill: "#FF203040".into(),
    });
    let doc = bevy_pf_xaml::parse(
        r##"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                  xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
              <StackPanel x:Name="Bound" Background="{Binding fill}"/>
              <Grid.Resources>
                <Style x:Key="filled" TargetType="StackPanel">
                  <Setter Property="Background" Value="#FF304050"/>
                </Style>
              </Grid.Resources>
              <StackPanel x:Name="Styled" Style="{StaticResource filled}"/>
            </Grid>"##,
    )
    .expect("parses");
    let world = app.world_mut();
    let root = world.spawn(DataContext(vm.clone())).id();
    bevy_pf::instantiate_document_env(world, root, &doc, &bevy_pf::XamlEnv::default())
        .expect("instantiates");
    for _ in 0..5 {
        app.update();
    }
    let names = app.world().get::<XamlNames>(root).unwrap();
    let bound = names.get("Bound").unwrap();
    let styled = names.get("Styled").unwrap();
    assert!(takes_clicks(&app, bound), "a bound Background counts");
    assert!(
        takes_clicks(&app, styled),
        "a Style setter Background counts"
    );
}

/// The two WPF rules compose: lifting `IsHitTestVisible` must not hand a
/// null-Background Panel a pickability it never had.
#[test]
fn restoring_hit_test_visibility_respects_the_panel_rule() {
    let mut app = layout_app(1.0);
    app.update();
    let root = mount(
        &mut app,
        r##"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                  xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                  x:Name="Outer" IsHitTestVisible="False" Background="#FF101010">
              <StackPanel x:Name="Bare"/>
              <StackPanel x:Name="Painted" Background="#FF203040"/>
            </Grid>"##,
    );
    let names = app.world().get::<XamlNames>(root).unwrap();
    let bare = names.get("Bare").unwrap();
    let painted = names.get("Painted").unwrap();
    assert!(!takes_clicks(&app, bare));
    assert!(!takes_clicks(&app, painted));

    *app.world_mut()
        .get_mut::<bevy_pf::PfHitTestVisible>(root)
        .unwrap() = bevy_pf::PfHitTestVisible(true);
    for _ in 0..3 {
        app.update();
    }
    assert!(takes_clicks(&app, painted), "the painted panel comes back");
    assert!(
        !takes_clicks(&app, bare),
        "the null-Background panel stays transparent to clicks — it was opted \
         out for its own WPF reason, not by IsHitTestVisible"
    );
}
