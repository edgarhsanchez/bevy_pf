//! MAUI-dialect visual states: `<VisualState.Setters>`, groups attached
//! directly to an element, and the two dialects sharing one state machine.
//!
//! WPF drives a visual state with a storyboard; MAUI drives it with
//! setters. Both spell the outer elements identically, so nothing here
//! declares which dialect it is using — the SHAPE of the markup decides,
//! and these tests pin that the choice is unambiguous in every direction:
//! a state holding a `<Storyboard>` animates, a state holding
//! `<VisualState.Setters>` applies setters, and a state holding BOTH does
//! both without either path noticing the other.

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy_pf::prelude::*;
use bevy_pf::{XamlEnv, instantiate_document_env};

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()));
    app.add_plugins(PfUiPlugin);
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
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

fn bg(app: &App, e: Entity) -> Color {
    app.world().get::<BackgroundColor>(e).unwrap().0
}

/// The group is attached to the BUTTON, not to a ControlTemplate's visual
/// root. That is where MAUI puts it, and it used to be dropped with a
/// warning — parsed, stored nowhere, and silently inert.
const ELEMENT_ATTACHED: &str = r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
            xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
  <Button x:Name="btn" Background="#FF101010">
    <VisualStateManager.VisualStateGroups>
      <VisualStateGroup x:Name="CommonStates">
        <VisualState x:Name="Normal"/>
        <VisualState x:Name="PointerOver">
          <VisualState.Setters>
            <Setter Property="Background" Value="#FF3366CC"/>
          </VisualState.Setters>
        </VisualState>
      </VisualStateGroup>
    </VisualStateManager.VisualStateGroups>
  </Button>
</StackPanel>"##;

#[test]
fn a_group_attached_to_an_element_is_not_dropped() {
    let mut app = test_app();
    let root = spawn(&mut app, ELEMENT_ATTACHED);
    let btn = named(&app, root, "btn");
    assert!(
        app.world()
            .get::<bevy_pf::animation::PfVisualStates>(btn)
            .is_some(),
        "the group never reached the element — MAUI's own spelling is inert"
    );
}

#[test]
fn entering_a_state_applies_its_setters_and_leaving_reverts_them() {
    let mut app = test_app();
    let root = spawn(&mut app, ELEMENT_ATTACHED);
    let btn = named(&app, root, "btn");
    let base = bg(&app, btn);

    // Hover it. NOT `go_to_state` directly: CommonStates is a DRIVEN
    // group — `drive_visual_states` recomputes it from Interaction every
    // frame — so a hand-set state is overwritten before the trigger pass
    // ever sees it. Driving the interaction is both the realistic path and
    // the only one that holds.
    //
    // This also exercises the alias: the author wrote PointerOver, MAUI's
    // spelling, and the driver must enter THAT rather than WPF's MouseOver.
    app.world_mut().entity_mut(btn).insert(Interaction::Hovered);
    // Twice: the first frame enters the state, the trigger pass applies on
    // the next. The engine reaches the same place in two frames of hover.
    app.update();
    app.update();
    let hovered = bg(&app, btn);
    assert_ne!(hovered, base, "the state's setters never applied");

    // Leaving must put it back. Setters ride the trigger runtime, so this
    // is the revert that runtime already does rather than a second one.
    app.world_mut().entity_mut(btn).insert(Interaction::None);
    app.update();
    app.update();
    assert_eq!(
        bg(&app, btn),
        base,
        "leaving the state did not revert its setters"
    );
}

/// THE CASE THAT PROVES NO MODE FLAG IS NEEDED.
///
/// One state carries a storyboard AND setters. The two dialects are told
/// apart by node kind, not by a declaration, so a state holding both is
/// not ambiguous — it is simply both, and neither path may swallow the
/// other.
#[test]
fn a_state_may_carry_a_storyboard_and_setters_at_once() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
            xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
  <Button x:Name="btn" Background="#FF101010" Width="100">
    <VisualStateManager.VisualStateGroups>
      <VisualStateGroup x:Name="CommonStates">
        <VisualState x:Name="Normal"/>
        <VisualState x:Name="Pressed">
          <VisualState.Setters>
            <Setter Property="Background" Value="#FF3366CC"/>
          </VisualState.Setters>
          <Storyboard>
            <DoubleAnimation Storyboard.TargetProperty="Width"
                             From="100" To="140" Duration="0:0:0.1"/>
          </Storyboard>
        </VisualState>
      </VisualStateGroup>
    </VisualStateManager.VisualStateGroups>
  </Button>
</StackPanel>"##,
    );
    let btn = named(&app, root, "btn");
    let base = bg(&app, btn);

    app.world_mut().entity_mut(btn).insert(Interaction::Pressed);
    app.update();
    app.update();

    // The SETTER half.
    assert_ne!(
        bg(&app, btn),
        base,
        "the setters were swallowed by the storyboard"
    );
    // The STORYBOARD half: the state is live in the group, which is what
    // the animation runs off.
    let states = app
        .world()
        .get::<bevy_pf::animation::PfVisualStates>(btn)
        .unwrap();
    assert_eq!(
        states.current[0].as_deref(),
        Some("Pressed"),
        "the storyboard path lost the state the setters entered"
    );
}

/// A state nobody has entered must change nothing — the same rule the
/// trigger runtime already keeps for an inactive trigger.
#[test]
fn an_unentered_state_applies_nothing() {
    let mut app = test_app();
    let root = spawn(&mut app, ELEMENT_ATTACHED);
    let btn = named(&app, root, "btn");
    app.update();
    let bg_now = bg(&app, btn);
    // #FF101010 as authored, not the PointerOver blue.
    assert_eq!(bg_now, Color::srgba_u8(0x10, 0x10, 0x10, 0xFF));
}
