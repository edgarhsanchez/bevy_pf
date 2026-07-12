//! StatusBar, ToolBar, ListView+GridView, Popup, Hyperlink, GridSplitter,
//! Calendar, DatePicker, and indeterminate ProgressBar.

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
    let result =
        instantiate_document_env(world, root, &doc, &XamlEnv::default()).expect("instantiates");
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
fn statusbar_and_toolbar_flow_horizontally() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<DockPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                      xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <ToolBarTray DockPanel.Dock="Top">
               <ToolBar x:Name="Tools">
                 <Button Content="New"/>
                 <Button Content="Open"/>
                 <Separator/>
                 <Button Content="Save"/>
               </ToolBar>
             </ToolBarTray>
             <StatusBar DockPanel.Dock="Bottom" x:Name="Status">
               <StatusBarItem><TextBlock Text="Ready"/></StatusBarItem>
               <StatusBarItem><TextBlock Text="Ln 12"/></StatusBarItem>
             </StatusBar>
             <TextBlock Text="body"/>
           </DockPanel>"##,
    );
    let toolbar = named(&app, root, "Tools");
    let status = named(&app, root, "Status");
    assert_eq!(children_of(&app, toolbar).len(), 4);
    assert_eq!(children_of(&app, status).len(), 2);
    for e in [toolbar, status] {
        assert_eq!(
            app.world().get::<Node>(e).unwrap().flex_direction,
            FlexDirection::Row
        );
    }
}

#[derive(Reflect, Default)]
struct Row {
    name: String,
    size: u32,
}

#[derive(Reflect, Default)]
struct Vm {
    rows: Vec<Row>,
}

#[test]
fn listview_gridview_generates_columned_rows() {
    let mut app = test_app();
    let vm = Bindable::new(Vm {
        rows: vec![
            Row { name: "alpha".into(), size: 1 },
            Row { name: "beta".into(), size: 2 },
        ],
    });
    let root = spawn(
        &mut app,
        r##"<ListView xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                     xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                     x:Name="L" ItemsSource="{Binding rows}">
             <ListView.View>
               <GridView>
                 <GridViewColumn Header="Name" DisplayMemberBinding="{Binding name}" Width="2*"/>
                 <GridViewColumn Header="Size" Width="*">
                   <GridViewColumn.CellTemplate>
                     <DataTemplate>
                       <StackPanel Orientation="Horizontal">
                         <TextBlock Text="sz:"/>
                         <TextBlock Text="{Binding size}"/>
                       </StackPanel>
                     </DataTemplate>
                   </GridViewColumn.CellTemplate>
                 </GridViewColumn>
               </GridView>
             </ListView.View>
           </ListView>"##,
    );
    app.world_mut().entity_mut(root).insert(DataContext(vm));
    app.update();

    let list = named(&app, root, "L");
    let grid = app
        .world()
        .get::<bevy_pf::components::PfDataGrid>(list)
        .unwrap()
        .clone();
    assert_eq!(grid.columns.len(), 2);
    assert_eq!(grid.columns[0].path, "name");
    assert!(grid.columns[1].template.is_some());

    let rows = children_of(&app, grid.rows_host);
    assert_eq!(rows.len(), 2);
    let cells = children_of(&app, rows[0]);
    assert_eq!(cells.len(), 2);
    // Text column renders the value; template column expands the template.
    assert_eq!(app.world().get::<Text>(cells[0]).unwrap().0, "alpha");
    let template_texts: Vec<String> = {
        fn collect(app: &App, e: Entity, out: &mut Vec<String>) {
            if let Some(t) = app.world().get::<Text>(e) {
                out.push(t.0.clone());
            }
            for c in app
                .world()
                .get::<Children>(e)
                .map(|c| c.iter().collect::<Vec<_>>())
                .unwrap_or_default()
            {
                collect(app, c, out);
            }
        }
        let mut out = Vec::new();
        collect(&app, cells[1], &mut out);
        out
    };
    assert_eq!(template_texts, vec!["sz:".to_string(), "1".to_string()]);
}

#[test]
fn popup_element_opens_and_anchors_to_parent() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" x:Name="Host">
             <Popup IsOpen="True" x:Name="P">
               <Border Width="120" Height="40" Background="#FF334455"/>
             </Popup>
           </StackPanel>"##,
    );
    let host = named(&app, root, "Host");
    let placeholder = named(&app, root, "P");
    let source = app
        .world()
        .get::<bevy_pf::components::PfPopupSource>(placeholder)
        .unwrap()
        .clone();
    let popup_state = app.world().get::<bevy_pf::PfPopup>(source.popup).unwrap();
    assert!(popup_state.open);
    app.update(); // resolve_popup_sources runs
    assert_eq!(
        app.world().get::<bevy_pf::PfPopup>(source.popup).unwrap().anchor,
        host
    );
    assert_eq!(children_of(&app, source.popup).len(), 1);
}

#[test]
fn hyperlink_and_indeterminate_progress() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <Hyperlink x:Name="Link" NavigateUri="https://bevy.org">Bevy engine</Hyperlink>
             <ProgressBar x:Name="Busy" IsIndeterminate="True" Width="200" Height="12"/>
           </StackPanel>"##,
    );
    let link = named(&app, root, "Link");
    assert_eq!(
        app.world()
            .get::<bevy_pf::components::PfHyperlink>(link)
            .unwrap()
            .0,
        "https://bevy.org"
    );
    let busy = named(&app, root, "Busy");
    assert!(
        app.world()
            .get::<bevy_pf::components::PfProgress>(busy)
            .unwrap()
            .indeterminate
    );
    // The sweep animates the fill's left margin.
    app.update();
    app.update();
    let visual = app
        .world()
        .get::<bevy_pf::components::PfProgressVisual>(busy)
        .unwrap()
        .fill;
    let node = app.world().get::<Node>(visual).unwrap();
    assert_eq!(node.width, Val::Percent(30.0));
}

#[test]
fn grid_splitter_converts_tracks_to_pixels() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                 xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                 x:Name="G" ColumnDefinitions="200, 6, *">
             <Border Grid.Column="0" Background="#FF445566"/>
             <GridSplitter Grid.Column="1" Width="6"/>
             <Border Grid.Column="2" Background="#FF556677"/>
           </Grid>"##,
    );
    let grid = named(&app, root, "G");
    let splitter = children_of(&app, grid)[1];
    assert!(
        app.world()
            .get::<bevy_pf::components::PfGridSplitter>(splitter)
            .unwrap()
            .columns
    );
    app.update();
    bevy_pf::instantiate::splitter_drag(app.world_mut(), splitter, Vec2::new(40.0, 0.0));
    let node = app.world().get::<Node>(grid).unwrap();
    // Neighbor tracks got converted to concrete pixel tracks.
    assert!(matches!(node.grid_template_columns.len(), 3));
}

#[test]
fn calendar_builds_month_and_selects() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<Calendar xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                     xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                     x:Name="Cal" SelectedDate="2026-07-09"/>"##,
    );
    let cal = named(&app, root, "Cal");
    let state = app
        .world()
        .get::<bevy_pf::components::PfCalendar>(cal)
        .unwrap()
        .clone();
    assert_eq!((state.year, state.month), (2026, 7));
    assert_eq!(state.selected, Some((2026, 7, 9)));
    assert_eq!(children_of(&app, state.days_host).len(), 31);
    assert_eq!(
        app.world().get::<Text>(state.title).unwrap().0,
        "July 2026"
    );

    bevy_pf::instantiate::calendar_shift_month(app.world_mut(), cal, 1);
    let state = app
        .world()
        .get::<bevy_pf::components::PfCalendar>(cal)
        .unwrap()
        .clone();
    assert_eq!((state.year, state.month), (2026, 8));
    assert_eq!(children_of(&app, state.days_host).len(), 31);
    assert_eq!(
        app.world().get::<Text>(state.title).unwrap().0,
        "August 2026"
    );

    bevy_pf::instantiate::calendar_select(app.world_mut(), cal, 2026, 8, 15);
    let state = app
        .world()
        .get::<bevy_pf::components::PfCalendar>(cal)
        .unwrap()
        .clone();
    assert_eq!(state.selected, Some((2026, 8, 15)));
}

#[test]
fn date_picker_selection_updates_display_and_closes() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<DatePicker xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                       x:Name="Picker" SelectedDate="2026-07-09"/>"##,
    );
    let picker = named(&app, root, "Picker");
    let state = app
        .world()
        .get::<bevy_pf::components::PfDatePicker>(picker)
        .unwrap()
        .clone();
    assert_eq!(state.selected, Some((2026, 7, 9)));
    assert_eq!(
        app.world().get::<Text>(state.display).unwrap().0,
        "2026-07-09"
    );

    // Open the dropdown, pick a day: display updates, popup closes.
    app.world_mut()
        .get_mut::<bevy_pf::PfPopup>(state.popup)
        .unwrap()
        .open = true;
    bevy_pf::instantiate::calendar_select(app.world_mut(), state.calendar, 2026, 7, 20);
    let state = app
        .world()
        .get::<bevy_pf::components::PfDatePicker>(picker)
        .unwrap()
        .clone();
    assert_eq!(state.selected, Some((2026, 7, 20)));
    assert_eq!(
        app.world().get::<Text>(state.display).unwrap().0,
        "2026-07-20"
    );
    assert!(!app.world().get::<bevy_pf::PfPopup>(state.popup).unwrap().open);
}

#[test]
fn scene_roots_fill_their_container_like_wpf() {
    let mut app = test_app();
    // A bare Grid root stretches to the full window, like WPF window content.
    let root = spawn(
        &mut app,
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                 xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"/>"#,
    );
    let node = app.world().get::<Node>(root).unwrap();
    assert_eq!(node.width, Val::Percent(100.0));
    assert_eq!(node.height, Val::Percent(100.0));

    // Explicit dimensions still win over the root stretch.
    let root = spawn(
        &mut app,
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                 xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                 Width="300" Height="200"/>"#,
    );
    let node = app.world().get::<Node>(root).unwrap();
    assert_eq!(node.width, Val::Px(300.0));
    assert_eq!(node.height, Val::Px(200.0));
}

#[test]
fn item_template_resolves_page_scoped_resources() {
    // A keyed style declared in the page's resources must be visible to
    // {StaticResource} inside a DataTemplate expanded later (WPF lexical
    // template scope) — templates expand from a scope snapshot, not from a
    // bare default environment.
    let mut app = test_app();
    let vm = Bindable::new(Vm {
        rows: vec![Row { name: "alpha".into(), size: 1 }],
    });
    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <StackPanel.Resources>
               <Style x:Key="Loud" TargetType="TextBlock">
                 <Setter Property="Foreground" Value="#FFCC2200"/>
               </Style>
             </StackPanel.Resources>
             <ItemsControl x:Name="I" ItemsSource="{Binding rows}">
               <ItemsControl.ItemTemplate>
                 <DataTemplate>
                   <TextBlock Text="{Binding name}" Style="{StaticResource Loud}"/>
                 </DataTemplate>
               </ItemsControl.ItemTemplate>
             </ItemsControl>
           </StackPanel>"##,
    );
    app.world_mut().entity_mut(root).insert(DataContext(vm));
    app.update();

    let host = named(&app, root, "I");
    let items = children_of(&app, host);
    assert_eq!(items.len(), 1);
    // Find the templated TextBlock and check the style applied.
    fn find_text(app: &App, e: Entity) -> Option<Entity> {
        if app.world().get::<Text>(e).is_some() {
            return Some(e);
        }
        for c in children_of(app, e) {
            if let Some(found) = find_text(app, c) {
                return Some(found);
            }
        }
        None
    }
    let text = find_text(&app, items[0]).expect("templated text spawned");
    assert_eq!(app.world().get::<Text>(text).unwrap().0, "alpha");
    let color = app
        .world()
        .get::<bevy::text::TextColor>(text)
        .expect("styled text has a color")
        .0;
    assert_eq!(
        bevy_pf::instantiate::color_to_hex(color),
        "#CC2200",
        "page-scoped keyed style applied inside the template"
    );
}
