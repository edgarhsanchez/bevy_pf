//! ComboBox, popup layer, and ItemsSource + DataTemplate generation.

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy::reflect::Reflect;
use bevy::ui::widget::Text;
use bevy_pf::prelude::*;
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
    let result = instantiate_document_env(world, root, &doc, &XamlEnv::default())
        .expect("instantiates");
    assert!(
        result.warnings.is_empty(),
        "expected clean instantiation: {:?}",
        result.warnings
    );
    root
}

fn named(app: &App, root: Entity, name: &str) -> Entity {
    app.world().get::<XamlNames>(root).unwrap().get(name).unwrap()
}

fn children_of(app: &App, e: Entity) -> Vec<Entity> {
    app.world()
        .get::<Children>(e)
        .map(|c| c.iter().collect())
        .unwrap_or_default()
}

#[test]
fn combo_box_static_items_and_selection() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <ComboBox x:Name="C" SelectedIndex="1">
               <ComboBoxItem>Alpha</ComboBoxItem>
               <ComboBoxItem>Beta</ComboBoxItem>
               <ComboBoxItem>Gamma</ComboBoxItem>
             </ComboBox>
           </StackPanel>"#,
    );
    let combo = named(&app, root, "C");
    let state = app
        .world()
        .get::<bevy_pf::components::PfComboBox>(combo)
        .unwrap()
        .clone();

    // Dropdown lives under the overlay root, not the combo.
    let popup_parent = app.world().get::<ChildOf>(state.popup).unwrap().parent();
    assert!(
        app.world()
            .get::<bevy_pf::overlay::PfOverlayRoot>(popup_parent)
            .is_some()
    );
    // ...but inherits logically from the combo.
    assert_eq!(
        app.world()
            .get::<bevy_pf::components::PfLogicalParent>(state.popup)
            .unwrap()
            .0,
        combo
    );
    assert_eq!(children_of(&app, state.popup).len(), 3);

    // Initial SelectedIndex resolved to the item's text.
    assert_eq!(app.world().get::<Text>(state.text).unwrap().0, "Beta");
    assert_eq!(state.selected, Some(1));

    // Selecting programmatically (what the item click observer runs).
    bevy_pf::instantiate::select_combo_index(app.world_mut(), combo, 2);
    let state = app
        .world()
        .get::<bevy_pf::components::PfComboBox>(combo)
        .unwrap();
    assert_eq!(app.world().get::<Text>(state.text).unwrap().0, "Gamma");
    assert!(!state.open);
}

#[test]
fn combo_open_state_reaches_popup_display() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r#"<ComboBox xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                     xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" x:Name="C">
             <ComboBoxItem>One</ComboBoxItem>
           </ComboBox>"#,
    );
    let combo = named(&app, root, "C");
    let popup = app
        .world()
        .get::<bevy_pf::components::PfComboBox>(combo)
        .unwrap()
        .popup;
    app.update();
    assert_eq!(app.world().get::<Node>(popup).unwrap().display, Display::None);

    app.world_mut()
        .get_mut::<bevy_pf::components::PfComboBox>(combo)
        .unwrap()
        .open = true;
    app.update();
    assert!(app.world().get::<bevy_pf::overlay::PfPopup>(popup).unwrap().open);
    assert_ne!(app.world().get::<Node>(popup).unwrap().display, Display::None);
}

#[derive(Reflect, Default)]
struct Player {
    name: String,
    score: u32,
}

#[derive(Reflect, Default)]
struct Roster {
    title: String,
    players: Vec<Player>,
    names: Vec<String>,
    pick: f64,
}

#[test]
fn items_source_plain_strings() {
    let mut app = test_app();
    let vm = Bindable::new(Roster {
        names: vec!["Ada".into(), "Bob".into()],
        ..Default::default()
    });
    let root = spawn(
        &mut app,
        r#"<ListBox xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                    xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                    x:Name="L" ItemsSource="{Binding names}"/>"#,
    );
    app.world_mut().entity_mut(root).insert(DataContext(vm.clone()));
    app.update();

    let list = named(&app, root, "L");
    let items = children_of(&app, list);
    assert_eq!(items.len(), 2);
    let texts: Vec<String> = items
        .iter()
        .map(|&i| {
            let inner = children_of(&app, i)[0];
            app.world().get::<Text>(inner).unwrap().0.clone()
        })
        .collect();
    assert_eq!(texts, ["Ada", "Bob"]);
    // Wrappers are selectable list items.
    assert!(
        app.world()
            .get::<bevy_pf::components::PfListBoxItem>(items[0])
            .is_some()
    );

    // Model change rebuilds.
    vm.update(|m: &mut Roster| m.names.push("Cleo".into()));
    app.update();
    let items = children_of(&app, list);
    assert_eq!(items.len(), 3);
}

#[test]
fn items_source_with_data_template() {
    let mut app = test_app();
    let vm = Bindable::new(Roster {
        players: vec![
            Player { name: "Ada".into(), score: 10 },
            Player { name: "Bob".into(), score: 20 },
        ],
        ..Default::default()
    });
    let root = spawn(
        &mut app,
        r#"<ItemsControl xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                         xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                         x:Name="I" ItemsSource="{Binding players}">
             <ItemsControl.ItemTemplate>
               <DataTemplate>
                 <StackPanel Orientation="Horizontal" Spacing="4">
                   <TextBlock Text="{Binding name}"/>
                   <TextBlock Text="{Binding score, StringFormat='{}{0} pts'}"/>
                 </StackPanel>
               </DataTemplate>
             </ItemsControl.ItemTemplate>
           </ItemsControl>"#,
    );
    app.world_mut().entity_mut(root).insert(DataContext(vm.clone()));
    app.update();
    app.update(); // template bindings resolve on the frame after generation

    let host = named(&app, root, "I");
    let items = children_of(&app, host);
    assert_eq!(items.len(), 2);

    // Each item's template expanded with its own scoped DataContext.
    let panel0 = children_of(&app, items[0])[0];
    let texts0 = children_of(&app, panel0);
    assert_eq!(app.world().get::<Text>(texts0[0]).unwrap().0, "Ada");
    assert_eq!(app.world().get::<Text>(texts0[1]).unwrap().0, "10 pts");
    let panel1 = children_of(&app, items[1])[0];
    let texts1 = children_of(&app, panel1);
    assert_eq!(app.world().get::<Text>(texts1[0]).unwrap().0, "Bob");

    // Item-level model change propagates through the scoped context.
    vm.update(|m: &mut Roster| m.players[0].score = 99);
    app.update();
    // Rebuild regenerated entities; re-resolve.
    let items = children_of(&app, host);
    let panel0 = children_of(&app, items[0])[0];
    let texts0 = children_of(&app, panel0);
    app.update();
    assert_eq!(app.world().get::<Text>(texts0[1]).unwrap().0, "99 pts");
}

#[test]
fn items_source_display_member_path() {
    let mut app = test_app();
    let vm = Bindable::new(Roster {
        players: vec![
            Player { name: "Ada".into(), score: 1 },
            Player { name: "Bob".into(), score: 2 },
        ],
        ..Default::default()
    });
    let root = spawn(
        &mut app,
        r#"<ListBox xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                    xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                    x:Name="L" ItemsSource="{Binding players}" DisplayMemberPath="name"/>"#,
    );
    app.world_mut().entity_mut(root).insert(DataContext(vm));
    app.update();

    let list = named(&app, root, "L");
    let items = children_of(&app, list);
    let first = children_of(&app, items[0])[0];
    assert_eq!(app.world().get::<Text>(first).unwrap().0, "Ada");
}

#[test]
fn combo_box_items_source() {
    let mut app = test_app();
    let vm = Bindable::new(Roster {
        names: vec!["Red".into(), "Green".into(), "Blue".into()],
        ..Default::default()
    });
    let root = spawn(
        &mut app,
        r#"<ComboBox xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                     xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                     x:Name="C" ItemsSource="{Binding names}"/>"#,
    );
    app.world_mut().entity_mut(root).insert(DataContext(vm));
    app.update();

    let combo = named(&app, root, "C");
    let state = app
        .world()
        .get::<bevy_pf::components::PfComboBox>(combo)
        .unwrap()
        .clone();
    assert_eq!(children_of(&app, state.popup).len(), 3);

    bevy_pf::instantiate::select_combo_index(app.world_mut(), combo, 1);
    let state = app
        .world()
        .get::<bevy_pf::components::PfComboBox>(combo)
        .unwrap();
    assert_eq!(app.world().get::<Text>(state.text).unwrap().0, "Green");
}

#[test]
fn tooltip_property_attaches() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r#"<Button xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                   xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                   x:Name="B" Content="Save" ToolTip="Saves the document"/>"#,
    );
    let b = named(&app, root, "B");
    assert_eq!(
        app.world().get::<bevy_pf::PfToolTip>(b).unwrap().0,
        "Saves the document"
    );
}

#[test]
fn item_write_back_through_scoped_context() {
    // TwoWay through a scoped item context writes into the list element.
    let vm = Bindable::new(Roster {
        players: vec![Player { name: "Ada".into(), score: 1 }],
        ..Default::default()
    });
    let item = vm.at("players[0]");
    assert_eq!(
        item.read_path("name"),
        Some(bevy_pf::BoundValue::Str("Ada".into()))
    );
    assert!(item.write_path("name", &bevy_pf::BoundValue::Str("Grace".into())));
    assert_eq!(
        vm.read_path("players[0].name"),
        Some(bevy_pf::BoundValue::Str("Grace".into()))
    );
    assert_eq!(vm.list_len("players"), Some(1));
}

#[test]
fn items_panel_redirects_generation_and_container_style_applies() {
    let mut app = test_app();
    let vm = Bindable::new(Roster {
        names: vec!["Ada".into(), "Bob".into(), "Cleo".into()],
        ..Default::default()
    });
    let root = spawn(
        &mut app,
        r##"<ListBox xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                     xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                     x:Name="L" ItemsSource="{Binding names}">
              <ListBox.ItemsPanel>
                <ItemsPanelTemplate>
                  <StackPanel Orientation="Horizontal"/>
                </ItemsPanelTemplate>
              </ListBox.ItemsPanel>
              <ListBox.ItemContainerStyle>
                <Style TargetType="ListBoxItem">
                  <Setter Property="Padding" Value="10"/>
                </Style>
              </ListBox.ItemContainerStyle>
            </ListBox>"##,
    );
    app.world_mut().entity_mut(root).insert(DataContext(vm.clone()));
    app.update();

    let list = named(&app, root, "L");
    let panel = app
        .world()
        .get::<bevy_pf::components::PfItemsPanel>(list)
        .expect("ItemsPanel recorded")
        .panel;

    // The panel is the list's only child and took the template's layout.
    assert_eq!(children_of(&app, list), vec![panel]);
    assert_eq!(
        app.world().get::<Node>(panel).unwrap().flex_direction,
        FlexDirection::Row,
        "Orientation=Horizontal from the ItemsPanelTemplate"
    );

    // Generated containers land inside the panel.
    let items = children_of(&app, panel);
    assert_eq!(items.len(), 3);
    let texts: Vec<String> = items
        .iter()
        .map(|&i| {
            let inner = children_of(&app, i)[0];
            app.world().get::<Text>(inner).unwrap().0.clone()
        })
        .collect();
    assert_eq!(texts, ["Ada", "Bob", "Cleo"]);

    // ItemContainerStyle setters reached each generated container.
    for &item in &items {
        assert_eq!(
            app.world().get::<Node>(item).unwrap().padding,
            UiRect::all(Val::Px(10.0)),
            "Padding=10 from ItemContainerStyle"
        );
    }

    // Model change regenerates into the panel, not the list root.
    vm.update(|m: &mut Roster| m.names.push("Dee".into()));
    app.update();
    assert_eq!(children_of(&app, panel).len(), 4);
    assert_eq!(children_of(&app, list), vec![panel]);
}

#[test]
fn selected_index_two_way_through_items_panel() {
    let mut app = test_app();
    let vm = Bindable::new(Roster {
        names: vec!["Ada".into(), "Bob".into(), "Cleo".into()],
        pick: 1.0,
        ..Default::default()
    });
    let root = spawn(
        &mut app,
        r##"<ListBox xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                     xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                     x:Name="L" ItemsSource="{Binding names}"
                     SelectedIndex="{Binding pick}">
              <ListBox.ItemsPanel>
                <ItemsPanelTemplate>
                  <UniformGrid Columns="2"/>
                </ItemsPanelTemplate>
              </ListBox.ItemsPanel>
            </ListBox>"##,
    );
    app.world_mut().entity_mut(root).insert(DataContext(vm.clone()));
    app.update();
    app.update(); // settle: generation, then selection apply

    let list = named(&app, root, "L");
    let panel = app
        .world()
        .get::<bevy_pf::components::PfItemsPanel>(list)
        .unwrap()
        .panel;
    let items = children_of(&app, panel);
    assert_eq!(items.len(), 3);

    // To-target: pick=1 selects the second generated container.
    let selected = app
        .world()
        .get::<bevy_pf::components::PfListBox>(list)
        .unwrap()
        .selected;
    assert_eq!(selected, Some(items[1]), "initial selection through panel");

    // Write-back: a user selection (what the item click observer sets)
    // computes its index against the panel's children.
    app.world_mut()
        .get_mut::<bevy_pf::components::PfListBox>(list)
        .unwrap()
        .selected = Some(items[2]);
    app.update();
    assert_eq!(
        vm.read_path("pick").and_then(|v| v.as_f64()),
        Some(2.0),
        "selection index wrote back through the panel"
    );
}
