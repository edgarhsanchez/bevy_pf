//! Real-layout geometry tests: bevy_ui's layout systems run headless so
//! ComputedNode carries actual pixel geometry — regressions like "the
//! dialog button hangs outside the dialog" become assertable.

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy_pf::prelude::*;

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
    // Font atlases live in image/atlas assets; headless needs them
    // registered by hand (no ImagePlugin without a renderer).
    app.init_asset::<Image>();
    app.init_asset::<bevy::image::TextureAtlasLayout>();
    app.add_plugins(PfUiPlugin);
    // A primary window entity gives layout its viewport (no real OS window).
    app.world_mut().spawn((
        bevy::window::Window {
            resolution: bevy::window::WindowResolution::new(1280, 800),
            ..Default::default()
        },
        bevy::window::PrimaryWindow,
    ));
    // No renderer runs headless, so the camera never learns its target
    // size; stamp it by hand and percent-sized UI roots resolve normally.
    let mut camera = Camera::default();
    camera.computed.target_info = Some(bevy::camera::RenderTargetInfo {
        physical_size: UVec2::new(1280, 800),
        scale_factor: 1.0,
    });
    app.world_mut().spawn((Camera2d, camera));
    app
}

fn rect_of(world: &World, e: Entity) -> (Vec2, Vec2) {
    let node = world.get::<bevy::ui::ComputedNode>(e).expect("computed");
    let tf = world
        .get::<bevy::ui::UiGlobalTransform>(e)
        .expect("ui transform");
    let scale = node.inverse_scale_factor();
    let size = node.size() * scale;
    let center: Vec2 = tf.translation * scale;
    (center - size / 2.0, center + size / 2.0)
}

#[test]
fn message_dialog_keeps_buttons_inside_panel() {
    let mut app = layout_app();
    app.update();

    // The Calculator About dialog, verbatim call.
    let dialog = bevy_pf::dialog::show_message(
        app.world_mut(),
        "About Calculator",
        "The WPF-Samples CalculatorDemo, running on bevy_pf. Menus, tooltips, \
         a Grid keypad, and a checkable View menu — arithmetic lives in Bevy \
         observers.",
        &["OK"],
    );
    // Let layout + text measurement settle.
    for _ in 0..5 {
        app.update();
    }

    let names = app.world().get::<XamlNames>(dialog).unwrap();
    let button = names.get("PfDlgBtn0").expect("OK button");
    // The dialog panel is the Border: the root Grid's only child.
    let panel = app
        .world()
        .get::<Children>(dialog)
        .unwrap()
        .iter()
        .next()
        .expect("dialog panel");

    let (root_min, root_max) = rect_of(app.world(), dialog);
    eprintln!("DIAG dialog root rect: {root_min:?}..{root_max:?}");
    // Walk panel -> stack -> children, printing geometry + key style facts.
    fn diag(world: &World, e: Entity, depth: usize) {
        let node = world.get::<bevy::ui::ComputedNode>(e);
        let style = world.get::<Node>(e);
        let (min, max) = match (node, world.get::<bevy::ui::UiGlobalTransform>(e)) {
            (Some(n), Some(t)) => {
                let s = n.size() * n.inverse_scale_factor();
                let c: Vec2 = t.translation * n.inverse_scale_factor();
                (c - s / 2.0, c + s / 2.0)
            }
            _ => (Vec2::ZERO, Vec2::ZERO),
        };
        let kind = world
            .get::<bevy_pf::PfElementKind>(e)
            .map(|k| k.0.clone())
            .unwrap_or_default();
        if let Some(st) = style {
            eprintln!(
                "DIAG {}{kind} x {:.0}..{:.0} y {:.0}..{:.0} | align_self {:?} justify_self {:?} margin.l {:?} w {:?}",
                "  ".repeat(depth),
                min.x,
                max.x,
                min.y,
                max.y,
                st.align_self,
                st.justify_self,
                st.margin.left,
                st.width
            );
        }
        if depth < 4
            && let Some(children) = world.get::<Children>(e)
        {
            let kids: Vec<Entity> = children.iter().collect();
            for c in kids {
                diag(world, c, depth + 1);
            }
        }
    }
    diag(app.world(), dialog, 0);
    let (panel_min, panel_max) = rect_of(app.world(), panel);
    let (btn_min, btn_max) = rect_of(app.world(), button);
    assert!(
        panel_max.x - panel_min.x > 0.0,
        "layout ran (panel has size)"
    );

    // The WPF contract: children lay out inside the padded dialog panel.
    assert!(
        btn_min.x >= panel_min.x - 0.5
            && btn_max.x <= panel_max.x + 0.5
            && btn_min.y >= panel_min.y - 0.5
            && btn_max.y <= panel_max.y + 0.5,
        "OK button {btn_min:?}..{btn_max:?} escapes the dialog panel {panel_min:?}..{panel_max:?}"
    );

    // And the panel respects its MaxWidth=480.
    assert!(
        panel_max.x - panel_min.x <= 480.5,
        "panel width {} exceeds MaxWidth 480",
        panel_max.x - panel_min.x
    );
}

#[derive(bevy::reflect::Reflect, Default)]
struct DeckRow {
    name: String,
    count: String,
}

#[derive(bevy::reflect::Reflect, Default)]
struct DeckVm {
    rows: Vec<DeckRow>,
    deck_visible: bool,
}

/// WPF contract: a ScrollViewer with vertical-only scrolling stretches its
/// content to the viewport width — a generated item's `*` Grid track must
/// span the full viewport, pushing a trailing right-aligned element flush
/// right. Regression shape: the Play Hub operative deck, where a bound
/// `Visibility` on the ScrollViewer rewrote its `display` to Flex (a flex
/// ROW), so group headers rendered content-sized with the count glued to
/// the title and cards never filled the module.
#[test]
fn scroll_viewer_stretches_generated_rows_to_viewport_width() {
    let mut app = layout_app();
    app.update();

    let vm = Bindable::new(DeckVm {
        rows: vec![
            DeckRow {
                name: "BLACKSITE".into(),
                count: "01".into(),
            },
            DeckRow {
                name: "DELTA".into(),
                count: "02".into(),
            },
        ],
        deck_visible: true,
    });
    let doc = bevy_pf_xaml::parse(
        r##"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                  xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                  Width="400" Height="120">
              <ScrollViewer x:Name="SV" Visibility="{Binding deck_visible}">
                <ItemsControl x:Name="List" ItemsSource="{Binding rows}">
                  <ItemsControl.ItemTemplate>
                    <DataTemplate>
                      <Grid HorizontalAlignment="Stretch" ColumnDefinitions="*,Auto">
                        <TextBlock Grid.Column="0" Text="{Binding name}"/>
                        <TextBlock Grid.Column="1" Text="{Binding count}"/>
                      </Grid>
                    </DataTemplate>
                  </ItemsControl.ItemTemplate>
                </ItemsControl>
              </ScrollViewer>
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
    let viewer = names.get("SV").unwrap();
    let list = names.get("List").unwrap();

    let assert_stretched = |app: &App, label: &str| {
        let (sv_min, sv_max) = rect_of(app.world(), viewer);
        let sv_width = sv_max.x - sv_min.x;
        assert!(
            (sv_width - 400.0).abs() < 0.5,
            "{label}: viewport width is definite"
        );
        assert_eq!(
            app.world().get::<Node>(viewer).unwrap().display,
            Display::Grid,
            "{label}: a visible ScrollViewer keeps its Grid display"
        );
        let wrappers: Vec<Entity> = app.world().get::<Children>(list).unwrap().iter().collect();
        assert_eq!(wrappers.len(), 2, "{label}: two generated rows");
        for &wrapper in &wrappers {
            // The template's Grid root and its trailing Auto column.
            let grid = app
                .world()
                .get::<Children>(wrapper)
                .unwrap()
                .iter()
                .next()
                .unwrap();
            let (gmin, gmax) = rect_of(app.world(), grid);
            assert!(
                gmax.x - gmin.x >= sv_width - 0.5,
                "{label}: template grid ({:.0}px) must span the {sv_width:.0}px viewport",
                gmax.x - gmin.x
            );
            let cols: Vec<Entity> = app.world().get::<Children>(grid).unwrap().iter().collect();
            let (_, count_max) = rect_of(app.world(), cols[1]);
            assert!(
                count_max.x >= sv_max.x - 1.0,
                "{label}: trailing Auto column ({:.0}) must sit flush right ({:.0})",
                count_max.x,
                sv_max.x
            );
        }
    };
    assert_stretched(&app, "initial");

    // Collapse and restore: the ScrollViewer must come back as a Grid, not
    // Display::DEFAULT (Flex).
    vm.update(|m: &mut DeckVm| m.deck_visible = false);
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(
        app.world().get::<Node>(viewer).unwrap().display,
        Display::None,
        "collapsed ScrollViewer leaves layout"
    );
    vm.update(|m: &mut DeckVm| m.deck_visible = true);
    for _ in 0..3 {
        app.update();
    }
    assert_stretched(&app, "after collapse/restore");
}

// Minimal probes for the Border content-box bug: which sizing mode makes a
// Border's child take the OUTER width instead of the padded content box?
#[test]
fn border_child_respects_padding_probes() {
    let mut app = layout_app();
    app.update();
    let doc = bevy_pf_xaml::parse(
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Width="600">
              <Border x:Name="Fixed" Width="480" Padding="16" BorderThickness="1">
                <StackPanel x:Name="FixedChild"><TextBlock Text="content"/></StackPanel>
              </Border>
              <Border x:Name="Maxed" MinWidth="320" MaxWidth="480" Padding="16" BorderThickness="1"
                      HorizontalAlignment="Center">
                <StackPanel x:Name="MaxedChild">
                  <TextBlock TextWrapping="Wrap"
                             Text="a long body of wrapped text that wants to be much wider than the four hundred and eighty pixel maximum width of this bordered panel"/>
                </StackPanel>
              </Border>
              <Grid Height="200">
                <Border x:Name="InGrid" MinWidth="320" MaxWidth="480" Padding="16" BorderThickness="1"
                        HorizontalAlignment="Center" VerticalAlignment="Center">
                  <StackPanel x:Name="InGridChild">
                    <TextBlock TextWrapping="Wrap"
                               Text="a long body of wrapped text that wants to be much wider than the four hundred and eighty pixel maximum width of this bordered panel"/>
                  </StackPanel>
                </Border>
              </Grid>
            </StackPanel>"##,
    )
    .expect("parses");
    let world = app.world_mut();
    let root = world.spawn_empty().id();
    bevy_pf::instantiate_document_env(world, root, &doc, &bevy_pf::XamlEnv::default())
        .expect("instantiates");
    for _ in 0..5 {
        app.update();
    }
    let names = app.world().get::<XamlNames>(root).unwrap();
    let probes = [
        (
            "Fixed",
            names.get("Fixed").unwrap(),
            names.get("FixedChild").unwrap(),
        ),
        (
            "Maxed",
            names.get("Maxed").unwrap(),
            names.get("MaxedChild").unwrap(),
        ),
        (
            "InGrid",
            names.get("InGrid").unwrap(),
            names.get("InGridChild").unwrap(),
        ),
    ];
    for (label, border, child) in probes {
        let (bmin, bmax) = rect_of(app.world(), border);
        let (cmin, cmax) = rect_of(app.world(), child);
        eprintln!(
            "PROBE {label}: border {:.0}..{:.0} ({:.0} wide), child {:.0}..{:.0} ({:.0} wide)",
            bmin.x,
            bmax.x,
            bmax.x - bmin.x,
            cmin.x,
            cmax.x,
            cmax.x - cmin.x
        );
        // KNOWN ENGINE BUG (taffy 0.10 grid sizing): a GRID item with
        // Min/MaxWidth + Padding hands its child the outer width instead
        // of the padded content box. Flex parents are correct — dialogs
        // center via flex as the workaround. When a taffy fix lands, the
        // InGrid probe below starts passing: promote it to an assert.
        if label == "InGrid" {
            let overflow = (cmax.x - cmin.x) - ((bmax.x - bmin.x) - 34.0);
            eprintln!("PROBE InGrid known-bug overflow: {overflow:.0}px (0 = fixed upstream)");
            continue;
        }
        assert!(
            (cmax.x - cmin.x) <= (bmax.x - bmin.x) - 33.0,
            "{label}: child must fit the padded content box"
        );
    }
}
