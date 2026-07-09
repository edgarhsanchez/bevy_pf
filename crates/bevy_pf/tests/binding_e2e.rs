//! End-to-end data binding: DataContext + {Binding} + change propagation,
//! running the real systems in a headless app.

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy::reflect::Reflect;
use bevy::ui::Checked;
use bevy::ui::widget::Text;
use bevy_pf::prelude::*;

#[derive(Reflect, Default)]
struct GameVm {
    score: u32,
    ready: bool,
    status: String,
    progress: f32,
}

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()));
    app.add_plugins(PfUiPlugin);
    app
}

fn spawn_bound_scene(app: &mut App, xaml: &'static str, vm: Bindable) -> Entity {
    let world = app.world_mut();
    let scene = bevy_pf::XamlScene::parse(xaml).expect("valid XAML");
    let root = world.spawn(DataContext(vm)).id();
    let doc = scene.document();
    bevy_pf::instantiate_document(world, root, &doc).expect("instantiates");
    // instantiate replaces components; re-add the context.
    root
}

#[test]
fn one_way_text_binding_with_format() {
    let mut app = test_app();
    let vm = Bindable::new(GameVm::default());
    let root = spawn_bound_scene(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation">
             <TextBlock x:Name="Score" Text="{Binding score, StringFormat=Score: {0} pts}"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"/>
           </StackPanel>"#,
        vm.clone(),
    );
    app.world_mut().entity_mut(root).insert(DataContext(vm.clone()));
    app.update();

    let names = app.world().get::<XamlNames>(root).unwrap();
    let tb = names.get("Score").unwrap();
    assert_eq!(app.world().get::<Text>(tb).unwrap().0, "Score: 0 pts");

    vm.update(|m: &mut GameVm| m.score = 42);
    app.update();
    assert_eq!(app.world().get::<Text>(tb).unwrap().0, "Score: 42 pts");
}

#[test]
fn ischecked_binding_both_directions() {
    let mut app = test_app();
    let vm = Bindable::new(GameVm::default());
    let root = spawn_bound_scene(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <CheckBox x:Name="Ready" Content="Ready" IsChecked="{Binding ready}"/>
           </StackPanel>"#,
        vm.clone(),
    );
    app.world_mut().entity_mut(root).insert(DataContext(vm.clone()));
    app.update();

    let cb = app.world().get::<XamlNames>(root).unwrap().get("Ready").unwrap();
    assert!(app.world().get::<Checked>(cb).is_none());

    // Source -> target.
    vm.update(|m: &mut GameVm| m.ready = true);
    app.update();
    assert!(app.world().get::<Checked>(cb).is_some());

    // Target -> source (simulates a click toggling the state off).
    app.world_mut().entity_mut(cb).remove::<Checked>();
    app.update();
    assert_eq!(
        vm.read_path("ready"),
        Some(bevy_pf::BoundValue::Bool(false))
    );
}

#[test]
fn textbox_binding_initial_and_write_back() {
    let mut app = test_app();
    let vm = Bindable::new(GameVm {
        status: "hello".into(),
        ..Default::default()
    });
    let root = spawn_bound_scene(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <TextBox x:Name="Status" Text="{Binding status}"/>
           </StackPanel>"#,
        vm.clone(),
    );
    app.world_mut().entity_mut(root).insert(DataContext(vm.clone()));
    app.update();

    let tb = app.world().get::<XamlNames>(root).unwrap().get("Status").unwrap();
    let input = app.world().get::<Children>(tb).unwrap().iter().next().unwrap();
    assert_eq!(
        app.world()
            .get::<bevy::text::EditableText>(input)
            .unwrap()
            .editor()
            .text(),
        "hello"
    );

    // Simulate typing: mutate the editable text, expect write-back.
    app.world_mut()
        .get_mut::<bevy::text::EditableText>(input)
        .unwrap()
        .editor
        .set_text("typed");
    app.update();
    assert_eq!(
        vm.read_path("status"),
        Some(bevy_pf::BoundValue::Str("typed".into()))
    );
}

#[test]
fn progress_value_binding() {
    let mut app = test_app();
    let vm = Bindable::new(GameVm::default());
    let root = spawn_bound_scene(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <ProgressBar x:Name="P" Minimum="0" Maximum="100" Value="{Binding progress}"/>
           </StackPanel>"#,
        vm.clone(),
    );
    app.world_mut().entity_mut(root).insert(DataContext(vm.clone()));
    app.update();

    let bar = app.world().get::<XamlNames>(root).unwrap().get("P").unwrap();
    vm.update(|m: &mut GameVm| m.progress = 55.0);
    app.update();
    let progress = app
        .world()
        .get::<bevy_pf::components::PfProgress>(bar)
        .unwrap();
    assert_eq!(progress.value, 55.0);
}

#[test]
fn content_binding_creates_text_child() {
    let mut app = test_app();
    let vm = Bindable::new(GameVm {
        status: "Click".into(),
        ..Default::default()
    });
    let root = spawn_bound_scene(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <Button x:Name="B" Content="{Binding status}"/>
           </StackPanel>"#,
        vm.clone(),
    );
    app.world_mut().entity_mut(root).insert(DataContext(vm.clone()));
    app.update();

    let button = app.world().get::<XamlNames>(root).unwrap().get("B").unwrap();
    let text_child = app
        .world()
        .get::<Children>(button)
        .unwrap()
        .iter()
        .next()
        .unwrap();
    assert_eq!(app.world().get::<Text>(text_child).unwrap().0, "Click");
}

#[test]
fn element_name_binding_slider_to_textblock() {
    let mut app = test_app();
    let vm = Bindable::new(GameVm::default());
    let root = spawn_bound_scene(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <!-- label BEFORE the slider: forward reference must resolve -->
             <TextBlock x:Name="Label"
                        Text="{Binding Value, ElementName=Speed, StringFormat='{}{0} km/h'}"/>
             <Slider x:Name="Speed" Minimum="0" Maximum="200" Value="60"/>
           </StackPanel>"#,
        vm.clone(),
    );
    app.world_mut().entity_mut(root).insert(DataContext(vm));
    app.update();

    let names = app.world().get::<XamlNames>(root).unwrap();
    let label = names.get("Label").unwrap();
    let slider = names.get("Speed").unwrap();
    assert_eq!(app.world().get::<Text>(label).unwrap().0, "60 km/h");

    // Move the slider; the label follows on the next frame.
    app.world_mut()
        .entity_mut(slider)
        .insert(bevy::ui_widgets::SliderValue(125.0));
    app.update();
    assert_eq!(app.world().get::<Text>(label).unwrap().0, "125 km/h");
}

#[test]
fn element_name_binding_checkbox_to_text() {
    let mut app = test_app();
    let vm = Bindable::new(GameVm::default());
    let root = spawn_bound_scene(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <CheckBox x:Name="Toggle" Content="On?" IsChecked="True"/>
             <TextBlock x:Name="State" Text="{Binding IsChecked, ElementName=Toggle}"/>
           </StackPanel>"#,
        vm.clone(),
    );
    app.world_mut().entity_mut(root).insert(DataContext(vm));
    app.update();

    let names = app.world().get::<XamlNames>(root).unwrap();
    let state = names.get("State").unwrap();
    let toggle = names.get("Toggle").unwrap();
    assert_eq!(app.world().get::<Text>(state).unwrap().0, "True");

    app.world_mut().entity_mut(toggle).remove::<bevy::ui::Checked>();
    app.update();
    assert_eq!(app.world().get::<Text>(state).unwrap().0, "False");
}
