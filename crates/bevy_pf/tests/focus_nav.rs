//! E2E: `PfFocusNav` — device-agnostic focus driving. Directional moves
//! over real layout geometry, scope containment, hidden-candidate
//! exclusion, synthetic activation, and scroll-into-view.

use bevy::asset::AssetPlugin;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy_pf::prelude::*;
use bevy_pf::{XamlEnv, instantiate_document_env};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

/// Real-layout headless app (the `layout_geometry.rs` harness): bevy_ui's
/// layout systems run, so ComputedNode carries actual pixel geometry —
/// directional navigation is meaningless without it.
fn layout_app() -> App {
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
            resolution: bevy::window::WindowResolution::new(1280, 800),
            ..Default::default()
        },
        bevy::window::PrimaryWindow,
    ));
    let mut camera = Camera::default();
    camera.computed.target_info = Some(bevy::camera::RenderTargetInfo {
        physical_size: UVec2::new(1280, 800),
        scale_factor: 1.0,
    });
    app.world_mut().spawn((Camera2d, camera));
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

fn nav(app: &mut App, msg: PfFocusNav) {
    app.world_mut().write_message(msg);
    app.update();
}

fn focused(app: &mut App) -> Option<Entity> {
    app.world_mut().resource::<InputFocus>().get()
}

const GRID: &str = r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                                   xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
      <StackPanel Orientation="Horizontal">
        <Button x:Name="TL" Width="100" Height="40" Content="tl"/>
        <Button x:Name="TR" Width="100" Height="40" Content="tr"/>
      </StackPanel>
      <StackPanel Orientation="Horizontal">
        <Button x:Name="BL" Width="100" Height="40" Content="bl"/>
        <Button x:Name="BR" Width="100" Height="40" Content="br"/>
      </StackPanel>
    </StackPanel>"##;

#[test]
fn first_move_lands_top_left_then_walks_the_grid() {
    let mut app = layout_app();
    let root = spawn(&mut app, GRID);
    app.update();
    app.update(); // layout settles

    nav(&mut app, PfFocusNav::Move(PfFocusDir::Down));
    assert_eq!(focused(&mut app), Some(named(&app, root, "TL")), "first move lands top-left");

    nav(&mut app, PfFocusNav::Move(PfFocusDir::Right));
    assert_eq!(focused(&mut app), Some(named(&app, root, "TR")));

    nav(&mut app, PfFocusNav::Move(PfFocusDir::Down));
    assert_eq!(focused(&mut app), Some(named(&app, root, "BR")));

    nav(&mut app, PfFocusNav::Move(PfFocusDir::Left));
    assert_eq!(focused(&mut app), Some(named(&app, root, "BL")));

    // No candidate below the bottom row: focus stays put, no wrap.
    nav(&mut app, PfFocusNav::Move(PfFocusDir::Down));
    assert_eq!(focused(&mut app), Some(named(&app, root, "BL")));
}

#[test]
fn scope_contains_navigation_and_hidden_candidates_are_skipped() {
    let mut app = layout_app();
    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                        Orientation="Horizontal">
          <StackPanel x:Name="PanelA">
            <Button x:Name="A1" Width="100" Height="40" Content="a1"/>
          </StackPanel>
          <StackPanel x:Name="PanelB">
            <Button x:Name="B1" Width="100" Height="40" Content="b1"/>
            <Button x:Name="B2" Width="100" Height="40" Content="b2" Visibility="Hidden"/>
            <Button x:Name="B3" Width="100" Height="40" Content="b3"/>
          </StackPanel>
        </StackPanel>"##,
    );
    app.update();
    app.update();

    let panel_b = named(&app, root, "PanelB");
    app.world_mut().resource_mut::<PfFocusScope>().0 = Some(panel_b);

    // First move lands inside the scope, not on the page's true top-left.
    nav(&mut app, PfFocusNav::Move(PfFocusDir::Down));
    assert_eq!(focused(&mut app), Some(named(&app, root, "B1")));

    // Down skips the hidden B2 and reaches B3.
    nav(&mut app, PfFocusNav::Move(PfFocusDir::Down));
    assert_eq!(focused(&mut app), Some(named(&app, root, "B3")));

    // Left would exit the scope toward A1: contained, focus stays.
    nav(&mut app, PfFocusNav::Move(PfFocusDir::Left));
    assert_eq!(focused(&mut app), Some(named(&app, root, "B3")));
}

#[test]
fn activate_runs_the_focused_buttons_command() {
    let mut app = layout_app();
    let hits = Arc::new(AtomicU32::new(0));

    #[derive(bevy::reflect::Reflect, Default)]
    struct Vm {}
    let vm = Bindable::new(Vm {});
    let h = hits.clone();
    vm.on_command("fire", move |_world, _param| {
        h.fetch_add(1, Ordering::SeqCst);
    });

    let doc = bevy_pf_xaml::parse(
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
          <Button x:Name="Go" Width="100" Height="40" Content="go" Command="fire"/>
        </StackPanel>"##,
    )
    .expect("parses");
    let world = app.world_mut();
    let root = world.spawn_empty().id();
    let env = XamlEnv::default();
    let result = instantiate_document_env(world, root, &doc, &env).expect("instantiates");
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    world.entity_mut(root).insert(DataContext(vm));
    app.update();
    app.update();

    nav(&mut app, PfFocusNav::Move(PfFocusDir::Down));
    assert_eq!(focused(&mut app), Some(named(&app, root, "Go")));

    nav(&mut app, PfFocusNav::Activate);
    app.update();
    assert_eq!(hits.load(Ordering::SeqCst), 1, "Activate delivered the click");
}

#[test]
fn focus_change_scrolls_the_focused_control_into_view() {
    let mut app = layout_app();
    let root = spawn(
        &mut app,
        r##"<ScrollViewer xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                          x:Name="SV" Height="90">
          <StackPanel>
            <Button x:Name="R0" Width="100" Height="40" Content="r0"/>
            <Button x:Name="R1" Width="100" Height="40" Content="r1"/>
            <Button x:Name="R2" Width="100" Height="40" Content="r2"/>
            <Button x:Name="R3" Width="100" Height="40" Content="r3"/>
          </StackPanel>
        </ScrollViewer>"##,
    );
    app.update();
    app.update();

    let viewer = named(&app, root, "SV");
    assert_eq!(
        app.world().get::<bevy::ui::ScrollPosition>(viewer).map(|p| p.y),
        Some(0.0),
        "starts unscrolled"
    );

    // Walk focus down to the below-the-fold row; the viewer follows.
    for _ in 0..4 {
        nav(&mut app, PfFocusNav::Move(PfFocusDir::Down));
    }
    assert_eq!(focused(&mut app), Some(named(&app, root, "R3")));
    app.update();
    let y = app
        .world()
        .get::<bevy::ui::ScrollPosition>(viewer)
        .map(|p| p.y)
        .unwrap();
    assert!(y > 0.0, "viewer scrolled to keep focus visible, got {y}");
}

#[derive(Reflect, Default)]
struct ShopVm {
    rows: Vec<ShopRowVm>,
}

#[derive(Reflect, Default)]
struct ShopRowVm {
    name: String,
    cost: String,
}

/// THE RING SURVIVES AN ITEMS REBUILD.
///
/// Every generated row is despawned and respawned when the bound
/// collection changes — there is no child reuse — so a focused control
/// inside one becomes a dangling entity. `focus_nav` then reads "nothing
/// focused" and lands top-left on the next move.
///
/// For a keyboard that is a nuisance. For a gamepad, whose only pointer
/// IS the ring, it means acting on a row throws the player to the top of
/// the dialog, and a row that can be used twice cannot be used twice.
/// Worse, the rebuild fires on ANY change to the collection, so a list
/// carrying live per-row state destroys the ring while the player sits
/// still.
#[test]
fn focus_survives_an_items_rebuild_and_stays_on_the_same_row() {
    let mut app = layout_app();
    let vm = Bindable::new(ShopVm {
        rows: vec![
            ShopRowVm { name: "shield".into(), cost: "6 TI".into() },
            ShopRowVm { name: "drive".into(), cost: "2 FE".into() },
            ShopRowVm { name: "core".into(), cost: "5 SI".into() },
        ],
    });
    let doc = bevy_pf_xaml::parse(
        r##"<ItemsControl xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                          x:Name="List" ItemsSource="{Binding rows}">
              <ItemsControl.ItemTemplate>
                <DataTemplate>
                  <StackPanel Orientation="Horizontal">
                    <TextBlock Text="{Binding name}"/>
                    <Button Content="{Binding cost}" Width="60" Height="20"/>
                  </StackPanel>
                </DataTemplate>
              </ItemsControl.ItemTemplate>
            </ItemsControl>"##,
    )
    .expect("parses");
    let world = app.world_mut();
    let root = world.spawn(DataContext(vm.clone())).id();
    instantiate_document_env(world, root, &doc, &XamlEnv::default()).expect("instantiates");
    app.update();

    // Focus the BUTTON in the middle row — index 1, second tab stop path.
    let list = named(&app, root, "List");
    let rows: Vec<Entity> = app.world().get::<Children>(list).unwrap().iter().collect();
    assert_eq!(rows.len(), 3, "one container per bound row");
    let middle_before = rows[1];
    let button_before = *app
        .world()
        .get::<Children>(middle_before)
        .and_then(|c| c.iter().next().and_then(|sp| app.world().get::<Children>(sp)))
        .expect("the row template built a panel of children")
        .iter()
        .collect::<Vec<_>>()
        .last()
        .expect("the row has a button");
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(button_before, bevy::input_focus::FocusCause::Navigated);
    app.update();
    assert_eq!(focused(&mut app), Some(button_before), "focus did not take");

    // Now change the collection the way crafting does: same length, new
    // per-row data. This despawns and respawns every row.
    vm.update(|m: &mut ShopVm| m.rows[1].cost = "1 FE".into());
    app.update();

    let rows_after: Vec<Entity> = app.world().get::<Children>(list).unwrap().iter().collect();
    assert_eq!(rows_after.len(), 3, "the rebuild kept the row count");
    assert_ne!(rows_after[1], middle_before, "the rows really were regenerated");

    let now = focused(&mut app).expect("the ring vanished after a rebuild");
    let middle_after = rows_after[1];
    let mut cursor = now;
    let mut inside_middle_row = false;
    loop {
        if cursor == middle_after {
            inside_middle_row = true;
            break;
        }
        match app.world().get::<ChildOf>(cursor) {
            Some(parent) => cursor = parent.parent(),
            None => break,
        }
    }
    assert!(
        inside_middle_row,
        "the ring left the row it was on — a pad cannot act on the same row twice"
    );
}

/// THE RING IS KEYBOARD FEEDBACK, NOT CLICK FEEDBACK.
///
/// `InputFocusVisible` is the contract: bevy's own `click_to_focus` sets
/// it false on a pointer press, Tab and `PfFocusNav::Move` set it true.
/// `focus_visuals` used to ignore it and outline every focus change, so
/// every mouse click on a button grew a cyan rectangle — WPF, and every
/// native toolkit, hides the focus visual for pointer-acquired focus.
#[test]
fn a_click_never_wears_the_focus_ring_and_navigation_always_does() {
    use bevy::input_focus::{FocusCause, InputFocus, InputFocusVisible};

    let mut app = layout_app();
    app.init_resource::<InputFocusVisible>();
    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
              <Button x:Name="A" Content="A" Width="80" Height="24"/>
              <Button x:Name="B" Content="B" Width="80" Height="24"/>
            </StackPanel>"##,
    );
    app.update();
    let a = named(&app, root, "A");
    let b = named(&app, root, "B");

    // Pad navigation: ring on.
    nav(&mut app, PfFocusNav::Move(PfFocusDir::Down));
    app.update();
    assert_eq!(focused(&mut app), Some(a));
    assert!(
        app.world().get::<bevy::ui::Outline>(a).is_some(),
        "navigation focus did not draw the ring"
    );

    // A pointer press, as bevy's click_to_focus + acquire_focus deliver
    // it: focus moves to the pressed control AND visibility goes false.
    app.world_mut().resource_mut::<InputFocusVisible>().0 = false;
    app.world_mut().resource_mut::<InputFocus>().set(b, FocusCause::Navigated);
    app.update();
    assert!(
        app.world().get::<bevy::ui::Outline>(b).is_none(),
        "a mouse click grew a focus rectangle"
    );
    assert!(
        app.world().get::<bevy::ui::Outline>(a).is_none(),
        "the old ring survived the click"
    );

    // Back on the pad: the ring returns.
    nav(&mut app, PfFocusNav::Move(PfFocusDir::Up));
    app.update();
    assert!(
        app.world().resource::<InputFocusVisible>().0,
        "a pad move did not make focus visible again"
    );
    let now = focused(&mut app).expect("something focused");
    assert!(
        app.world().get::<bevy::ui::Outline>(now).is_some(),
        "the ring did not come back for the pad"
    );
}
