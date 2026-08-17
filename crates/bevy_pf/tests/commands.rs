//! `Command=` / `CommandParameter=` — the ICommand analog: named commands
//! registered on a `Bindable`, invoked from Button-family controls and
//! MenuItems, with a `PfCommandInvoked` message either way.

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy_pf::prelude::*;
use bevy_pf::{XamlEnv, instantiate_document_env};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

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

#[derive(bevy::reflect::Reflect, Default)]
struct Vm {
    selected: String,
}

#[test]
fn registered_command_runs_with_resolved_parameter() {
    let mut app = test_app();
    let hits = Arc::new(AtomicU32::new(0));
    let seen = Arc::new(std::sync::Mutex::new(None::<String>));

    let vm = Bindable::new(Vm {
        selected: "alpha".into(),
    });
    let (h, s) = (hits.clone(), seen.clone());
    vm.on_command("save", move |_world, param| {
        h.fetch_add(1, Ordering::SeqCst);
        *s.lock().unwrap() = param;
    });

    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <Button x:Name="Save" Content="Save"
                     Command="{Binding save}" CommandParameter="{Binding selected}"/>
             <Button x:Name="Lit" Content="Ping"
                     Command="save" CommandParameter="fixed"/>
           </StackPanel>"##,
    );
    app.world_mut().entity_mut(root).insert(DataContext(vm));
    app.update();

    // The buttons carry the command spec.
    let save = named(&app, root, "Save");
    let cmd = app
        .world()
        .get::<bevy_pf::components::PfCommand>(save)
        .unwrap()
        .clone();
    assert_eq!(cmd.name, "save");

    // Activation resolves the bound parameter against the DataContext.
    bevy_pf::binding::invoke_command(app.world_mut(), save, &cmd.name, cmd.parameter.as_ref());
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    assert_eq!(seen.lock().unwrap().as_deref(), Some("alpha"));

    // Literal name + literal parameter.
    let lit = named(&app, root, "Lit");
    let cmd = app
        .world()
        .get::<bevy_pf::components::PfCommand>(lit)
        .unwrap()
        .clone();
    bevy_pf::binding::invoke_command(app.world_mut(), lit, &cmd.name, cmd.parameter.as_ref());
    assert_eq!(hits.load(Ordering::SeqCst), 2);
    assert_eq!(seen.lock().unwrap().as_deref(), Some("fixed"));
}

#[test]
fn unregistered_command_still_writes_the_message() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <Button x:Name="B" Content="Fire" Command="do-thing" CommandParameter="42"/>
           </StackPanel>"##,
    );
    app.world_mut()
        .entity_mut(root)
        .insert(DataContext(Bindable::new(Vm::default())));
    let b = named(&app, root, "B");
    let cmd = app
        .world()
        .get::<bevy_pf::components::PfCommand>(b)
        .unwrap()
        .clone();
    bevy_pf::binding::invoke_command(app.world_mut(), b, &cmd.name, cmd.parameter.as_ref());

    let msgs: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<bevy_pf::binding::PfCommandInvoked>>()
        .drain()
        .collect();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].command, "do-thing");
    assert_eq!(msgs[0].parameter.as_deref(), Some("42"));
    assert_eq!(msgs[0].source, b);
}

#[test]
fn menu_item_carries_command() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<Menu xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                  xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <MenuItem Header="File">
               <MenuItem x:Name="SaveItem" Header="Save" Command="{Binding save}"/>
             </MenuItem>
           </Menu>"##,
    );
    let item = named(&app, root, "SaveItem");
    let cmd = app
        .world()
        .get::<bevy_pf::components::PfCommand>(item)
        .unwrap();
    assert_eq!(cmd.name, "save");
}
