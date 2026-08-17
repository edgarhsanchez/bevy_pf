//! Element identity (x:Name, x:Uid, AutomationId) + the PfQuery SystemParam.

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

const SCENE: &str = r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                                   xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
      <TextBlock x:Name="Title" x:Uid="title.uid" Text="Hello"/>
      <Button x:Name="Go" AutomationProperties.AutomationId="GoButton"
              AutomationProperties.HelpText="ignored silently" Content="Run"/>
      <Button Content="Anonymous"/>
    </StackPanel>"#;

/// Run a one-shot system with world access and return its output.
fn run<T: 'static, S, M>(app: &mut App, system: S) -> T
where
    S: IntoSystem<(), T, M>,
{
    app.world_mut()
        .run_system_once(system)
        .expect("system runs")
}

use bevy::ecs::system::RunSystemOnce;

#[test]
fn ids_become_queryable_components() {
    let mut app = test_app();
    let root = spawn(&mut app, SCENE);

    let (title, go) = run(&mut app, move |ui: PfQuery| {
        (ui.by_name("Title"), ui.by_name("Go"))
    });
    let title = title.expect("Title found by x:Name");
    let go = go.expect("Go found by x:Name");

    // The same entities are reachable through every identity mechanism.
    assert_eq!(
        run(&mut app, move |ui: PfQuery| ui.by_uid("title.uid")),
        Some(title)
    );
    assert_eq!(
        run(&mut app, move |ui: PfQuery| ui.by_automation_id("GoButton")),
        Some(go)
    );
    assert_eq!(
        run(&mut app, move |ui: PfQuery| ui.named_in(root, "Go")),
        Some(go)
    );

    // ...and through raw component queries, no framework API needed.
    assert_eq!(app.world().get::<PfName>(go).unwrap().0, "Go");
    assert_eq!(app.world().get::<PfUid>(title).unwrap().0, "title.uid");
    assert_eq!(app.world().get::<PfAutomationId>(go).unwrap().0, "GoButton");

    // Kind query sees both buttons; scope_root walks back to the scene root.
    let (buttons, scope) = run(&mut app, move |ui: PfQuery| {
        (ui.by_kind("Button").len(), ui.scope_root(go))
    });
    assert_eq!(buttons, 2);
    assert_eq!(scope, Some(root));
    assert_eq!(
        run(&mut app, move |ui: PfQuery| ui
            .name_of(title)
            .map(str::to_string)),
        Some("Title".to_string())
    );
}

#[test]
fn systems_can_rewrite_found_elements() {
    let mut app = test_app();
    spawn(&mut app, SCENE);

    // A plain system: find by uid, rewrite the text.
    run(
        &mut app,
        |ui: PfQuery, mut texts: Query<&mut bevy::ui::widget::Text>| {
            let title = ui.by_uid("title.uid").unwrap();
            let text_entity = ui.first_text_in(title).unwrap();
            texts.get_mut(text_entity).unwrap().0 = "Rewritten".to_string();
        },
    );
    let title = run(&mut app, |ui: PfQuery| ui.by_name("Title").unwrap());
    let text_entity = run(&mut app, move |ui: PfQuery| {
        ui.first_text_in(title).unwrap()
    });
    assert_eq!(
        app.world()
            .get::<bevy::ui::widget::Text>(text_entity)
            .unwrap()
            .0,
        "Rewritten"
    );

    // set_local goes through the provider store: a Local-tier background that
    // masks (not destroys) whatever styles put beneath it.
    let go = run(&mut app, |ui: PfQuery| ui.by_name("Go").unwrap());
    bevy_pf::provider::set_local(
        app.world_mut(),
        go,
        bevy_pf::PropertyTarget::Background,
        bevy_pf::resources::PfValue::Brush(bevy_pf::xaml_ast::value::PfBrush::Solid(
            bevy_pf::xaml_ast::value::PfColor {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            },
        )),
    );
    assert_eq!(
        app.world().get::<BackgroundColor>(go).unwrap().0.to_srgba(),
        Color::srgb(1.0, 0.0, 0.0).to_srgba()
    );
    assert_eq!(
        app.world()
            .get::<bevy_pf::PfPropertyStore>(go)
            .unwrap()
            .effective_source(bevy_pf::PropertyTarget::Background),
        Some(bevy_pf::ValueSource::Local)
    );
}
