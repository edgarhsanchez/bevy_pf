//! TabControl, TreeView, Menu/ContextMenu, and DataGrid.

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
fn tab_control_selection() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r#"<TabControl xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                       x:Name="T" SelectedIndex="1">
             <TabItem Header="General"><TextBlock Text="general content"/></TabItem>
             <TabItem Header="Advanced"><TextBlock Text="advanced content"/></TabItem>
             <TabItem Header="About"><TextBlock Text="about content"/></TabItem>
           </TabControl>"#,
    );
    let tab = named(&app, root, "T");
    let state = app
        .world()
        .get::<bevy_pf::components::PfTabControl>(tab)
        .unwrap()
        .clone();
    assert_eq!(state.headers.len(), 3);
    assert_eq!(state.contents.len(), 3);
    assert_eq!(state.selected, 1);

    // Only the selected tab's content is visible.
    fn display(app: &App, e: Entity) -> Display {
        app.world().get::<Node>(e).unwrap().display
    }
    assert_eq!(display(&app, state.contents[0]), Display::None);
    assert_ne!(display(&app, state.contents[1]), Display::None);
    assert_eq!(display(&app, state.contents[2]), Display::None);

    // Selected header is highlighted white.
    assert_eq!(
        app.world().get::<BackgroundColor>(state.headers[1]).unwrap().0,
        Color::WHITE
    );

    bevy_pf::instantiate::select_tab(app.world_mut(), tab, 2);
    let state = app
        .world()
        .get::<bevy_pf::components::PfTabControl>(tab)
        .unwrap()
        .clone();
    assert_eq!(state.selected, 2);
    assert_eq!(display(&app, state.contents[1]), Display::None);
    assert_ne!(display(&app, state.contents[2]), Display::None);
}

#[test]
fn tree_view_expansion_and_selection() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r#"<TreeView xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                     xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" x:Name="Tree">
             <TreeViewItem Header="Root" IsExpanded="True" x:Name="RootItem">
               <TreeViewItem Header="Child A" x:Name="ChildA">
                 <TreeViewItem Header="Grandchild"/>
               </TreeViewItem>
               <TreeViewItem Header="Child B"/>
             </TreeViewItem>
           </TreeView>"#,
    );
    let tree = named(&app, root, "Tree");
    let root_item = named(&app, root, "RootItem");
    let child_a = named(&app, root, "ChildA");

    let root_state = app
        .world()
        .get::<bevy_pf::components::PfTreeItem>(root_item)
        .unwrap()
        .clone();
    assert!(root_state.expanded);
    assert!(root_state.has_children);
    assert_ne!(
        app.world().get::<Node>(root_state.container).unwrap().display,
        Display::None
    );
    // Two children under the root item.
    assert_eq!(children_of(&app, root_state.container).len(), 2);

    // Child A starts collapsed.
    let a_state = app
        .world()
        .get::<bevy_pf::components::PfTreeItem>(child_a)
        .unwrap()
        .clone();
    assert!(!a_state.expanded);
    assert_eq!(
        app.world().get::<Node>(a_state.container).unwrap().display,
        Display::None
    );

    // Toggling expands + selects it.
    bevy_pf::instantiate::toggle_tree_item(app.world_mut(), tree, child_a);
    let a_state = app
        .world()
        .get::<bevy_pf::components::PfTreeItem>(child_a)
        .unwrap()
        .clone();
    assert!(a_state.expanded);
    assert_ne!(
        app.world().get::<Node>(a_state.container).unwrap().display,
        Display::None
    );
    assert_eq!(
        app.world()
            .get::<bevy_pf::components::PfTreeView>(tree)
            .unwrap()
            .selected,
        Some(child_a)
    );
    // Arrow glyph flipped.
    assert_eq!(app.world().get::<Text>(a_state.arrow).unwrap().0, "−");
}

#[test]
fn menu_submenus_open_and_close() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r#"<Menu xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                 xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" x:Name="M">
             <MenuItem Header="File" x:Name="File">
               <MenuItem Header="New" x:Name="New"/>
               <MenuItem Header="Open Recent" x:Name="Recent">
                 <MenuItem Header="a.txt"/>
               </MenuItem>
               <Separator/>
               <MenuItem Header="Exit" x:Name="Exit"/>
             </MenuItem>
             <MenuItem Header="Help" x:Name="Help">
               <MenuItem Header="About"/>
             </MenuItem>
           </Menu>"#,
    );
    let menu = named(&app, root, "M");
    let file = named(&app, root, "File");
    let help = named(&app, root, "Help");
    let exit = named(&app, root, "Exit");

    let file_state = app
        .world()
        .get::<bevy_pf::components::PfMenuItem>(file)
        .unwrap()
        .clone();
    let file_popup = file_state.submenu.expect("File has a submenu");
    // The submenu lives under the overlay with a logical link back.
    assert_eq!(
        app.world()
            .get::<bevy_pf::components::PfLogicalParent>(file_popup)
            .unwrap()
            .0,
        file
    );
    // 4 entries: New, Open Recent, separator, Exit.
    assert_eq!(children_of(&app, file_popup).len(), 4);

    // Activate "File" -> its popup opens.
    bevy_pf::instantiate::activate_menu_item(app.world_mut(), file);
    assert!(app.world().get::<bevy_pf::PfPopup>(file_popup).unwrap().open);

    // Activating "Help" closes File's popup, opens Help's.
    bevy_pf::instantiate::activate_menu_item(app.world_mut(), help);
    assert!(!app.world().get::<bevy_pf::PfPopup>(file_popup).unwrap().open);
    let help_popup = app
        .world()
        .get::<bevy_pf::components::PfMenuItem>(help)
        .unwrap()
        .submenu
        .unwrap();
    assert!(app.world().get::<bevy_pf::PfPopup>(help_popup).unwrap().open);

    // A leaf item closes everything in this menu.
    bevy_pf::instantiate::activate_menu_item(app.world_mut(), file);
    bevy_pf::instantiate::activate_menu_item(app.world_mut(), exit);
    assert!(!app.world().get::<bevy_pf::PfPopup>(file_popup).unwrap().open);
    assert!(!app.world().get::<bevy_pf::PfPopup>(help_popup).unwrap().open);
    let _ = menu;
}

#[test]
fn context_menu_attaches_and_opens() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r#"<Border xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                   xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                   x:Name="B" Width="100" Height="50">
             <Border.ContextMenu>
               <ContextMenu>
                 <MenuItem Header="Copy" x:Name="Copy"/>
                 <MenuItem Header="Paste"/>
               </ContextMenu>
             </Border.ContextMenu>
           </Border>"#,
    );
    let owner = named(&app, root, "B");
    // The context popup is a menu popup rooted at the owner.
    let mut found = None;
    let mut query = app
        .world_mut()
        .query::<(Entity, &bevy_pf::components::PfMenuPopup)>();
    for (e, m) in query.iter(app.world()) {
        if m.menu_root == owner {
            found = Some(e);
        }
    }
    let popup = found.expect("context menu popup exists");
    assert_eq!(children_of(&app, popup).len(), 2);
    assert!(!app.world().get::<bevy_pf::PfPopup>(popup).unwrap().open);

    // Leaf activation closes it after a manual open.
    app.world_mut()
        .get_mut::<bevy_pf::PfPopup>(popup)
        .unwrap()
        .open = true;
    let copy = named(&app, root, "Copy");
    bevy_pf::instantiate::activate_menu_item(app.world_mut(), copy);
    assert!(!app.world().get::<bevy_pf::PfPopup>(popup).unwrap().open);
}

#[derive(Reflect, Default)]
struct Row {
    name: String,
    score: u32,
}

#[derive(Reflect, Default)]
struct GridVm {
    rows: Vec<Row>,
}

#[test]
fn data_grid_columns_and_rows() {
    let mut app = test_app();
    let vm = Bindable::new(GridVm {
        rows: vec![
            Row { name: "Ada".into(), score: 10 },
            Row { name: "Bob".into(), score: 20 },
        ],
    });
    let root = spawn(
        &mut app,
        r#"<DataGrid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                     xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                     x:Name="G" ItemsSource="{Binding rows}">
             <DataGrid.Columns>
               <DataGridTextColumn Header="Name" Binding="{Binding name}" Width="2*"/>
               <DataGridTextColumn Header="Score" Binding="{Binding score}" Width="*"/>
             </DataGrid.Columns>
           </DataGrid>"#,
    );
    app.world_mut().entity_mut(root).insert(DataContext(vm.clone()));
    app.update();

    let grid = named(&app, root, "G");
    let state = app
        .world()
        .get::<bevy_pf::components::PfDataGrid>(grid)
        .unwrap()
        .clone();
    assert_eq!(state.columns.len(), 2);
    assert_eq!(state.columns[0].header, "Name");
    assert_eq!(state.columns[1].path, "score");

    // Header row + rows host under the grid.
    let kids = children_of(&app, grid);
    assert_eq!(kids.len(), 2);
    let header_cells = children_of(&app, kids[0]);
    assert_eq!(
        app.world().get::<Text>(header_cells[0]).unwrap().0,
        "Name"
    );

    // Generated rows with per-column cells.
    let rows = children_of(&app, state.rows_host);
    assert_eq!(rows.len(), 2);
    let row0_cells = children_of(&app, rows[0]);
    assert_eq!(app.world().get::<Text>(row0_cells[0]).unwrap().0, "Ada");
    assert_eq!(app.world().get::<Text>(row0_cells[1]).unwrap().0, "10");

    // Rows are selectable via the ListBox mechanism on the rows host.
    assert!(
        app.world()
            .get::<bevy_pf::components::PfListBox>(state.rows_host)
            .is_some()
    );
    assert!(
        app.world()
            .get::<bevy_pf::components::PfListBoxItem>(rows[0])
            .is_some()
    );

    // Model change rebuilds rows.
    vm.update(|m: &mut GridVm| m.rows.push(Row { name: "Cleo".into(), score: 30 }));
    app.update();
    let rows = children_of(&app, state.rows_host);
    assert_eq!(rows.len(), 3);
}

#[test]
fn checkable_menu_item_toggles_on_activation() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r#"<Menu xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                 xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <MenuItem Header="View">
               <MenuItem x:Name="Standard" IsCheckable="true" IsChecked="True" Header="Standard"/>
               <MenuItem x:Name="Plain" Header="Plain"/>
             </MenuItem>
           </Menu>"#,
    );
    let standard = named(&app, root, "Standard");
    let plain = named(&app, root, "Plain");

    // Initial IsChecked="True": Checked present, glyph visible.
    assert!(app.world().get::<bevy::ui::Checked>(standard).is_some());
    let glyph = app
        .world()
        .get::<bevy_pf::components::PfCheckableMenuItem>(standard)
        .unwrap()
        .glyph;
    assert_eq!(app.world().get::<Node>(glyph).unwrap().display, Display::Flex);

    // Leaf activation toggles off, then back on.
    bevy_pf::instantiate::activate_menu_item(app.world_mut(), standard);
    assert!(app.world().get::<bevy::ui::Checked>(standard).is_none());
    assert_eq!(app.world().get::<Node>(glyph).unwrap().display, Display::None);
    bevy_pf::instantiate::activate_menu_item(app.world_mut(), standard);
    assert!(app.world().get::<bevy::ui::Checked>(standard).is_some());

    // Non-checkable items are unaffected by activation.
    bevy_pf::instantiate::activate_menu_item(app.world_mut(), plain);
    assert!(app.world().get::<bevy::ui::Checked>(plain).is_none());
    assert!(
        app.world()
            .get::<bevy_pf::components::PfCheckableMenuItem>(plain)
            .is_none()
    );
}
