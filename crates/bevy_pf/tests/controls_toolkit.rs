//! Ecosystem "toolkit" controls: TimePicker, PackIcon, ColorPicker,
//! AutoSuggestBox, NavigationView, and ContentDialog.

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
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

/// Every `Text` value in the subtree under `root`, depth-first.
fn texts_in(app: &App, root: Entity) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        if let Some(t) = app.world().get::<Text>(e) {
            out.push(t.0.clone());
        }
        if let Some(children) = app.world().get::<Children>(e) {
            stack.extend(children.iter());
        }
    }
    out
}

#[test]
fn time_picker_parses_selected_time_and_sets() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r#"<TimePicker xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                       x:Name="T" SelectedTime="09:30"/>"#,
    );
    let picker = named(&app, root, "T");
    let state = app
        .world()
        .get::<bevy_pf::components::PfTimePicker>(picker)
        .unwrap()
        .clone();
    assert_eq!((state.hour, state.minute), (Some(9), Some(30)));
    assert!(texts_in(&app, root).contains(&"09:30".to_string()));

    // Programmatic set from any system.
    bevy_pf::instantiate::time_picker_set(app.world_mut(), picker, Some(14), Some(45));
    assert!(texts_in(&app, root).contains(&"14:45".to_string()));
}

#[test]
fn pack_icon_renders_known_kind_as_shape() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <PackIcon x:Name="I" Kind="Home" Width="20" Height="20"/>
             <SymbolIcon Symbol="Search" Width="20" Height="20"/>
             <FontIcon Glyph="+"/>
           </StackPanel>"#,
    );
    let icon = named(&app, root, "I");
    let children: Vec<Entity> = app
        .world()
        .get::<Children>(icon)
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    assert_eq!(children.len(), 1, "icon spawns one shape child");
    assert!(
        app.world()
            .get::<bevy_pf::shapes::PfShapeRendered>(children[0])
            .is_some(),
        "PackIcon child is a vector shape, not a font glyph"
    );
}

#[test]
fn pack_icon_unknown_kind_warns() {
    let mut app = test_app();
    let doc = bevy_pf_xaml::parse(
        r#"<PackIcon xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                     Kind="NoSuchGlyph"/>"#,
    )
    .unwrap();
    let world = app.world_mut();
    let root = world.spawn_empty().id();
    let result = instantiate_document_env(world, root, &doc, &XamlEnv::default()).unwrap();
    assert_eq!(result.warnings.len(), 1);
    assert!(result.warnings[0].contains("NoSuchGlyph"));
}

#[test]
fn color_picker_parses_and_sets_color() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<ColorPicker xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                        x:Name="C" SelectedColor="#FF8C00"/>"##,
    );
    let picker = named(&app, root, "C");
    let state = app
        .world()
        .get::<bevy_pf::components::PfColorPicker>(picker)
        .unwrap()
        .clone();
    assert_eq!(
        bevy_pf::instantiate::color_to_hex(state.selected),
        "#FF8C00"
    );
    // Swatch shows the color.
    let swatch_bg = app.world().get::<BackgroundColor>(state.swatch).unwrap().0;
    assert_eq!(bevy_pf::instantiate::color_to_hex(swatch_bg), "#FF8C00");

    // Programmatic set (what a palette click queues) updates swatch + hex.
    bevy_pf::instantiate::color_picker_set(
        app.world_mut(),
        picker,
        Color::srgb_u8(0x33, 0x99, 0x33),
        true,
    );
    let state = app
        .world()
        .get::<bevy_pf::components::PfColorPicker>(picker)
        .unwrap()
        .clone();
    assert_eq!(bevy_pf::instantiate::color_to_hex(state.selected), "#339933");
    let hex = app
        .world()
        .get::<bevy::text::EditableText>(state.hex_input)
        .unwrap()
        .editor()
        .text()
        .to_string();
    assert_eq!(hex, "#339933");
}

#[test]
fn auto_suggest_filters_as_you_type() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r#"<AutoSuggestBox xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                           xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                           x:Name="A" Suggestions="Amsterdam,Athens,Berlin,Bern"/>"#,
    );
    app.update();
    let boxx = named(&app, root, "A");
    let state = app
        .world()
        .get::<bevy_pf::components::PfAutoSuggestBox>(boxx)
        .unwrap()
        .clone();
    assert_eq!(state.items.len(), 4);

    // Type "be" -> Berlin + Bern.
    app.world_mut()
        .get_mut::<bevy::text::EditableText>(state.input)
        .unwrap()
        .editor
        .set_text("be");
    app.update();
    app.update(); // watch queues the rebuild; apply it
    let rows = texts_in(&app, state.popup);
    assert_eq!(rows, vec!["Bern".to_string(), "Berlin".to_string()],
        "prefix-filtered suggestions (depth-first order)");
    assert!(
        app.world()
            .get::<bevy_pf::overlay::PfPopup>(state.popup)
            .unwrap()
            .open
    );

    // Exact match -> dropdown closes.
    app.world_mut()
        .get_mut::<bevy::text::EditableText>(state.input)
        .unwrap()
        .editor
        .set_text("Berlin");
    app.update();
    app.update();
    assert!(
        !app.world()
            .get::<bevy_pf::overlay::PfPopup>(state.popup)
            .unwrap()
            .open
    );
}

#[test]
fn navigation_view_navigates_embedded_frame() {
    let mut app = test_app();
    app.register_page(
        "t-alpha",
        XamlScene::parse(
            r#"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Title="Alpha">
                 <TextBlock Text="alpha page body"/>
               </Page>"#,
        )
        .unwrap(),
    );
    app.register_page(
        "t-beta",
        XamlScene::parse(
            r#"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Title="Beta">
                 <TextBlock Text="beta page body"/>
               </Page>"#,
        )
        .unwrap(),
    );
    app.update(); // let the page registry build

    let root = spawn(
        &mut app,
        r#"<NavigationView xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                           xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                           x:Name="N">
             <NavigationViewItem Content="Alpha" Icon="Home" Tag="t-alpha"/>
             <NavigationViewItem Content="Beta" Icon="Settings" Tag="t-beta"/>
           </NavigationView>"#,
    );
    let view = named(&app, root, "N");
    let state = app
        .world()
        .get::<bevy_pf::components::PfNavigationView>(view)
        .unwrap()
        .clone();
    assert_eq!(state.items.len(), 2);
    // First item auto-selected and its page shown.
    assert_eq!(state.selected, Some(0));
    assert!(texts_in(&app, root).contains(&"alpha page body".to_string()));

    // Invoking the second item swaps the frame content.
    bevy_pf::instantiate::nav_view_invoke(app.world_mut(), view, 1, "t-beta");
    let texts = texts_in(&app, root);
    assert!(texts.contains(&"beta page body".to_string()));
    assert!(!texts.contains(&"alpha page body".to_string()));
}

#[test]
fn content_dialog_hosts_scene_and_reports_result() {
    let mut app = test_app();
    app.update();
    let dialog = bevy_pf::dialog::show_content(
        app.world_mut(),
        "About",
        &XamlScene::parse(
            r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation">
                 <TextBlock Text="hosted dialog body"/>
               </StackPanel>"#,
        )
        .unwrap(),
        &["OK", "Cancel"],
    );
    app.update();
    let texts = texts_in(&app, dialog);
    assert!(texts.contains(&"About".to_string()));
    assert!(texts.contains(&"hosted dialog body".to_string()));
    assert!(texts.contains(&"OK".to_string()));
    assert!(texts.contains(&"Cancel".to_string()));

    // Closing reports the button and despawns the dialog.
    bevy_pf::dialog::close_dialog(app.world_mut(), dialog, "OK");
    let results: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<bevy_pf::dialog::PfDialogResult>>()
        .drain()
        .collect();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].button, "OK");
    assert!(app.world().get_entity(dialog).is_err());
}
