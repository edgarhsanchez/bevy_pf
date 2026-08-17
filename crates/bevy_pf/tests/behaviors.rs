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
    app.world()
        .get::<XamlNames>(root)
        .unwrap()
        .get(name)
        .unwrap()
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
    let color = app
        .world()
        .get::<bevy::text::TextColor>(label)
        .unwrap()
        .0
        .to_srgba();
    assert!(
        color.green > 0.9,
        "ChangePropertyAction recolored the label"
    );
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

#[test]
fn key_trigger_runs_actions_on_just_pressed() {
    let mut app = test_app();
    let hits = Arc::new(AtomicU32::new(0));
    let vm = Bindable::new(Vm);
    let h = hits.clone();
    vm.on_command("submit", move |_w, _p| {
        h.fetch_add(1, Ordering::SeqCst);
    });
    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <Border x:Name="Panel" Background="#FF101010" Padding="4">
               <Interaction.Triggers>
                 <KeyTrigger Key="Return">
                   <InvokeCommandAction Command="{Binding submit}"/>
                 </KeyTrigger>
               </Interaction.Triggers>
               <TextBlock Text="press enter"/>
             </Border>
           </StackPanel>"##,
    );
    app.world_mut().entity_mut(root).insert(DataContext(vm));
    app.init_resource::<ButtonInput<KeyCode>>();
    app.update();

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Enter);
    app.update();
    assert_eq!(hits.load(Ordering::SeqCst), 1, "Enter ran the command");

    // Held (not just-pressed) does not re-fire.
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .clear_just_pressed(KeyCode::Enter);
    app.update();
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[test]
fn set_focus_action_and_focus_scoped_key_trigger() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
              <TextBox x:Name="Input" Text="hi"/>
              <Button x:Name="Go" Content="Focus the box">
                <Interaction.Triggers>
                  <EventTrigger EventName="Click">
                    <SetFocusAction TargetName="Input"/>
                  </EventTrigger>
                </Interaction.Triggers>
              </Button>
              <Border x:Name="Scoped">
                <Interaction.Triggers>
                  <KeyTrigger Key="Return" ActiveOnFocus="True">
                    <InvokeCommandAction Command="fired"/>
                  </KeyTrigger>
                </Interaction.Triggers>
              </Border>
            </StackPanel>"##,
    );
    let fired = Arc::new(AtomicU32::new(0));
    let vm = Bindable::new(Vm);
    {
        let fired = fired.clone();
        vm.on_command("fired", move |_, _| {
            fired.fetch_add(1, Ordering::SeqCst);
        });
    }
    app.world_mut().entity_mut(root).insert(DataContext(vm));
    app.init_resource::<ButtonInput<KeyCode>>();
    app.update();

    let names = app.world().get::<XamlNames>(root).unwrap();
    let (input, go) = (names.get("Input").unwrap(), names.get("Go").unwrap());

    // SetFocusAction focuses the TextBox's inner editable.
    let actions = vec![bevy_pf::behaviors::PfAction::SetFocus {
        target_name: Some("Input".into()),
    }];
    bevy_pf::behaviors::run_actions(app.world_mut(), go, &actions);
    let focus = app
        .world()
        .resource::<bevy::input_focus::InputFocus>()
        .get()
        .expect("focus set");
    let mut cursor = focus;
    let mut inside = focus == input;
    while let Some(p) = app.world().get::<ChildOf>(cursor) {
        if p.parent() == input {
            inside = true;
            break;
        }
        cursor = p.parent();
    }
    assert!(inside, "focus landed in the TextBox subtree");

    // Focus-scoped KeyTrigger: Enter with focus elsewhere does nothing.
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Enter);
    app.update();
    assert_eq!(fired.load(Ordering::SeqCst), 0, "unfocused: no fire");
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(KeyCode::Enter);
    app.update();

    // Move focus into the scoped Border -> the trigger fires.
    let scoped = app
        .world()
        .get::<XamlNames>(root)
        .unwrap()
        .get("Scoped")
        .unwrap();
    app.world_mut()
        .insert_resource(bevy::input_focus::InputFocus::from_entity(scoped));
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Enter);
    app.update();
    assert_eq!(fired.load(Ordering::SeqCst), 1, "focused: fired");
}

/// `focus_control` resolves a TextBox to its editable child.
///
/// Setting `InputFocus` to the control entity itself does nothing — the control
/// is chrome, the buffer is a child — and that failure is silent, which is why
/// the resolver is public API rather than an internal detail.
#[test]
fn focus_control_resolves_a_textbox_to_its_editable_child() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <TextBox x:Name="Email" Width="200"/>
           </StackPanel>"##,
    );
    app.update();

    let control = app
        .world()
        .get::<XamlNames>(root)
        .unwrap()
        .get("Email")
        .unwrap();
    assert!(
        app.world()
            .get::<bevy::text::EditableText>(control)
            .is_none(),
        "the control itself is not the editable — that is the whole point"
    );

    let editable = find_editable_in(app.world(), control).expect("editable child");
    assert!(
        app.world()
            .get::<bevy::text::EditableText>(editable)
            .is_some()
    );

    focus_control(app.world_mut(), control);
    assert_eq!(
        app.world()
            .resource::<bevy::input_focus::InputFocus>()
            .get(),
        Some(editable),
        "focus must land on the editable child, not the control"
    );

    clear_focus(app.world_mut());
    assert_eq!(
        app.world()
            .resource::<bevy::input_focus::InputFocus>()
            .get(),
        None
    );
}
