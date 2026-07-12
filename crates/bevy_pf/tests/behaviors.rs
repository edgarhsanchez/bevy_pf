//! Interactivity behaviors: <Interaction.Triggers> EventTriggers running
//! InvokeCommand / ControlStoryboard / ChangeProperty / GoToState actions.

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
    app.world().get::<XamlNames>(root).unwrap().get(name).unwrap()
}

#[derive(bevy::reflect::Reflect, Default)]
struct Vm;

#[test]
fn loaded_behaviors_run_commands_and_change_properties() {
    let mut app = test_app();
    let hits = Arc::new(AtomicU32::new(0));
    let vm = Bindable::new(Vm);
    let h = hits.clone();
    vm.on_command("boot", move |_w, param| {
        assert_eq!(param.as_deref(), Some("ready"));
        h.fetch_add(1, Ordering::SeqCst);
    });

    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <Border x:Name="Card" Background="#FF101010" Padding="4">
               <Interaction.Triggers>
                 <EventTrigger EventName="Loaded">
                   <InvokeCommandAction Command="{Binding boot}" CommandParameter="ready"/>
                   <ChangePropertyAction TargetName="Label" PropertyName="Foreground" Value="#FF00FF00"/>
                 </EventTrigger>
               </Interaction.Triggers>
               <TextBlock x:Name="Label" Text="status"/>
             </Border>
           </StackPanel>"##,
    );
    app.world_mut().entity_mut(root).insert(DataContext(vm));
    app.update(); // pending Loaded actions fire

    assert_eq!(hits.load(Ordering::SeqCst), 1, "command invoked on Loaded");
    let label = named(&app, root, "Label");
    let color = app.world().get::<bevy::text::TextColor>(label).unwrap().0.to_srgba();
    assert!(color.green > 0.9, "ChangePropertyAction recolored the label");
}

#[test]
fn behavior_storyboard_action_plays_a_keyed_storyboard() {
    let mut app = test_app();
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <StackPanel.Resources>
               <Storyboard x:Key="Widen">
                 <DoubleAnimation Storyboard.TargetName="Bar"
                                  Storyboard.TargetProperty="Width"
                                  From="10" To="90" Duration="0:0:1"/>
               </Storyboard>
             </StackPanel.Resources>
             <Border x:Name="Bar" Width="10" Height="6" Background="#FF3366CC">
               <Interaction.Triggers>
                 <EventTrigger EventName="Loaded">
                   <ControlStoryboardAction Storyboard="{StaticResource Widen}"/>
                 </EventTrigger>
               </Interaction.Triggers>
             </Border>
           </StackPanel>"##,
    );
    app.update(); // Loaded action begins the storyboard
    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .advance_by(std::time::Duration::from_secs_f32(2.0));
    app.update();
    let bar = named(&app, root, "Bar");
    assert_eq!(
        app.world().get::<Node>(bar).unwrap().width,
        Val::Px(90.0),
        "storyboard action animated the width to its end"
    );
}
