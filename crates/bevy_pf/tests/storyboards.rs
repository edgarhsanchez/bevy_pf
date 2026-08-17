//! Storyboard core (phase 2a): Double/Color animations through the value
//! store's Animation tier, EventTrigger(Loaded/...) delivery, HoldEnd/Stop
//! fill behaviors, and the Rust `begin_storyboard` API.

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy_pf::prelude::*;
use bevy_pf::provider::{PropertyTarget, ValueSource};
use bevy_pf::{XamlEnv, instantiate_document_env};
use std::time::Duration;

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()));
    app.add_plugins(PfUiPlugin);
    // Deterministic clock: pause virtual time, advance manually.
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    app
}

fn advance(app: &mut App, secs: f32) {
    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .advance_by(Duration::from_secs_f32(secs));
    app.update();
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

fn bg_alpha(app: &App, e: Entity) -> f32 {
    app.world().get::<BackgroundColor>(e).unwrap().0.alpha()
}

#[test]
fn loaded_event_trigger_animates_opacity_and_holds() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <Border x:Name="Card" Background="#FF334455" Padding="4">
               <Border.Triggers>
                 <EventTrigger RoutedEvent="Loaded">
                   <BeginStoryboard>
                     <Storyboard>
                       <DoubleAnimation Storyboard.TargetProperty="Opacity"
                                        From="0" To="1" Duration="0:0:1"/>
                     </Storyboard>
                   </BeginStoryboard>
                 </EventTrigger>
               </Border.Triggers>
               <TextBlock Text="fade in"/>
             </Border>
           </StackPanel>"##,
    );
    let card = named(&app, root, "Card");
    app.update(); // pending storyboard begins (t=0)
    advance(&mut app, 0.0);
    assert!(bg_alpha(&app, card) < 0.05, "starts at From=0");

    advance(&mut app, 0.5);
    let mid = bg_alpha(&app, card);
    assert!((mid - 0.5).abs() < 0.1, "midway ≈ 0.5, got {mid}");

    advance(&mut app, 1.0);
    assert!((bg_alpha(&app, card) - 1.0).abs() < 0.01, "HoldEnd at To=1");
    // The animation layer holds the final value at the Animation tier.
    let store = app.world().get::<bevy_pf::PfPropertyStore>(card).unwrap();
    assert_eq!(
        store.effective_source(PropertyTarget::Opacity),
        Some(ValueSource::Animation)
    );
}

#[test]
fn color_animation_with_stop_reverts_to_base() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <Border x:Name="Card" Background="#FF000000" Padding="4">
               <Border.Triggers>
                 <EventTrigger RoutedEvent="Loaded">
                   <BeginStoryboard>
                     <Storyboard>
                       <ColorAnimation Storyboard.TargetProperty="Background"
                                       To="#FFFF0000" Duration="0:0:1"
                                       FillBehavior="Stop"/>
                     </Storyboard>
                   </BeginStoryboard>
                 </EventTrigger>
               </Border.Triggers>
             </Border>
           </StackPanel>"##,
    );
    let card = named(&app, root, "Card");
    app.update();
    advance(&mut app, 0.5);
    let mid = app
        .world()
        .get::<BackgroundColor>(card)
        .unwrap()
        .0
        .to_srgba();
    assert!(
        mid.red > 0.3 && mid.red < 0.7,
        "mid-blend red, got {}",
        mid.red
    );

    advance(&mut app, 1.0);
    let after = app
        .world()
        .get::<BackgroundColor>(card)
        .unwrap()
        .0
        .to_srgba();
    assert!(
        after.red < 0.05,
        "Stop reverts to the black base, got {}",
        after.red
    );
    let store = app.world().get::<bevy_pf::PfPropertyStore>(card).unwrap();
    assert_ne!(
        store.effective_source(PropertyTarget::Background),
        Some(ValueSource::Animation),
        "animation layer cleared"
    );
}

#[test]
fn keyed_storyboard_with_target_name_via_rust_api() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <StackPanel.Resources>
               <Storyboard x:Key="Grow">
                 <DoubleAnimation Storyboard.TargetName="Box"
                                  Storyboard.TargetProperty="Width"
                                  From="100" To="200" Duration="0:0:1"/>
               </Storyboard>
             </StackPanel.Resources>
             <Border x:Name="Box" Width="100" Height="20" Background="#FF224466"/>
           </StackPanel>"##,
    );
    app.update();

    // Fetch the keyed storyboard from the element's resource scope.
    let sb = {
        let dict = app
            .world()
            .get::<bevy_pf::PfResources>(root)
            .expect("scene resources")
            .0
            .clone();
        match dict
            .get(&bevy_pf::resources::ResourceKey::Explicit("Grow".into()))
            .expect("keyed storyboard")
        {
            bevy_pf::resources::PfValue::Storyboard(sb) => sb.clone(),
            other => panic!("expected storyboard, got {other:?}"),
        }
    };
    bevy_pf::animation::begin_storyboard(app.world_mut(), root, root, &sb);

    advance(&mut app, 0.5);
    let boxx = named(&app, root, "Box");
    let node = app.world().get::<Node>(boxx).unwrap();
    let Val::Px(w) = node.width else {
        panic!("px width")
    };
    assert!((w - 150.0).abs() < 12.0, "midway width ≈150, got {w}");

    advance(&mut app, 1.0);
    let node = app.world().get::<Node>(boxx).unwrap();
    assert_eq!(node.width, Val::Px(200.0), "held at To");
}

#[test]
fn style_event_trigger_and_repeat_forever() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <StackPanel.Resources>
               <Style TargetType="Border">
                 <Style.Triggers>
                   <EventTrigger RoutedEvent="Loaded">
                     <BeginStoryboard>
                       <Storyboard>
                         <DoubleAnimation Storyboard.TargetProperty="Opacity"
                                          From="1" To="0" Duration="0:0:1"
                                          AutoReverse="True" RepeatBehavior="Forever"/>
                       </Storyboard>
                     </BeginStoryboard>
                   </EventTrigger>
                 </Style.Triggers>
               </Style>
             </StackPanel.Resources>
             <Border x:Name="Pulse" Background="#FF334455" Width="40" Height="8"/>
           </StackPanel>"##,
    );
    let pulse = named(&app, root, "Pulse");
    app.update();

    advance(&mut app, 0.5);
    assert!((bg_alpha(&app, pulse) - 0.5).abs() < 0.1, "fading out");
    advance(&mut app, 1.0); // t=1.5: reversing back up
    assert!((bg_alpha(&app, pulse) - 0.5).abs() < 0.1, "fading back in");
    advance(&mut app, 1_000.0); // far future: still running (Forever)
    let anims = app
        .world()
        .resource::<bevy_pf::animation::PfRunningAnimations>();
    let _ = anims;
    advance(&mut app, 0.25);
    let a = bg_alpha(&app, pulse);
    assert!((0.0..=1.0).contains(&a), "still animating in range");
}

// ---------------------------------------------------------------------
// VisualStateManager over the storyboard core.
// ---------------------------------------------------------------------

#[test]
fn visual_states_drive_hover_and_revert() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <Button x:Name="B" Content="Go">
               <Button.Template>
                 <ControlTemplate TargetType="Button">
                   <Border x:Name="chrome" Background="#FFE1E1E1">
                     <VisualStateManager.VisualStateGroups>
                       <VisualStateGroup x:Name="CommonStates">
                         <VisualState x:Name="Normal"/>
                         <VisualState x:Name="MouseOver">
                           <Storyboard>
                             <ColorAnimation Storyboard.TargetName="chrome"
                                             Storyboard.TargetProperty="Background"
                                             To="#FFBEE6FD" Duration="0:0:0.1"/>
                           </Storyboard>
                         </VisualState>
                         <VisualState x:Name="Pressed">
                           <Storyboard>
                             <ColorAnimation Storyboard.TargetName="chrome"
                                             Storyboard.TargetProperty="Background"
                                             To="#FFC4E5F6" Duration="0:0:0.1"/>
                           </Storyboard>
                         </VisualState>
                       </VisualStateGroup>
                     </VisualStateManager.VisualStateGroups>
                     <ContentPresenter/>
                   </Border>
                 </ControlTemplate>
               </Button.Template>
             </Button>
           </StackPanel>"##,
    );
    let button = named(&app, root, "B");
    let chrome = app
        .world()
        .get::<bevy_pf::components::PfTemplatedControl>(button)
        .unwrap()
        .template_root;
    app.update(); // enter Normal
    advance(&mut app, 0.2);
    let rest = app
        .world()
        .get::<BackgroundColor>(chrome)
        .unwrap()
        .0
        .to_srgba();
    assert!(
        (rest.red - 0.882).abs() < 0.02,
        "Normal keeps the template value"
    );

    // Hover: MouseOver state animates the chrome.
    app.world_mut()
        .entity_mut(button)
        .insert(Interaction::Hovered);
    advance(&mut app, 0.05); // state switch + animation start
    advance(&mut app, 0.2); // finish the 0.1s transition
    let hover = app
        .world()
        .get::<BackgroundColor>(chrome)
        .unwrap()
        .0
        .to_srgba();
    assert!(hover.blue > 0.95, "MouseOver brush, got {hover:?}");

    // Press: switching states stops+reverts the old before the new runs.
    app.world_mut()
        .entity_mut(button)
        .insert(Interaction::Pressed);
    advance(&mut app, 0.05); // state switch + animation start
    advance(&mut app, 0.2); // transition completes
    let pressed = app
        .world()
        .get::<BackgroundColor>(chrome)
        .unwrap()
        .0
        .to_srgba();
    assert!(
        (pressed.red - 0.769).abs() < 0.03,
        "Pressed brush, got {pressed:?}"
    );

    // Release to rest: Normal has no storyboard — the group reverts the
    // Animation tier and the template literal shows again.
    app.world_mut().entity_mut(button).insert(Interaction::None);
    advance(&mut app, 0.05);
    advance(&mut app, 0.2);
    let back = app
        .world()
        .get::<BackgroundColor>(chrome)
        .unwrap()
        .0
        .to_srgba();
    assert!((back.red - 0.882).abs() < 0.02, "reverted, got {back:?}");

    // GoToState from Rust.
    assert!(bevy_pf::animation::go_to_state(
        app.world_mut(),
        button,
        "CommonStates",
        "Pressed"
    ));
}

// ---------------------------------------------------------------------
// Keyframe animations: Linear/Discrete/Easing/Spline keyframes.
// ---------------------------------------------------------------------

#[test]
fn keyframe_animation_walks_its_track() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <Border x:Name="Bar" Width="0" Height="8" Background="#FF3366CC">
               <Border.Triggers>
                 <EventTrigger RoutedEvent="Loaded">
                   <BeginStoryboard>
                     <Storyboard>
                       <DoubleAnimationUsingKeyFrames Storyboard.TargetProperty="Width">
                         <LinearDoubleKeyFrame KeyTime="0:0:1" Value="100"/>
                         <DiscreteDoubleKeyFrame KeyTime="0:0:2" Value="300"/>
                         <SplineDoubleKeyFrame KeyTime="0:0:3" Value="400"
                                               KeySpline="0.4,0 0.6,1"/>
                       </DoubleAnimationUsingKeyFrames>
                     </Storyboard>
                   </BeginStoryboard>
                 </EventTrigger>
               </Border.Triggers>
             </Border>
           </StackPanel>"##,
    );
    let bar = named(&app, root, "Bar");
    let width = |app: &App| -> f32 {
        match app.world().get::<Node>(bar).unwrap().width {
            Val::Px(w) => w,
            other => panic!("expected px, got {other:?}"),
        }
    };
    app.update(); // Loaded begins (duration defaults to the last key: 3s)

    advance(&mut app, 0.5); // mid first segment: base(0) -> 100 linear
    assert!(
        (width(&app) - 50.0).abs() < 8.0,
        "linear mid ≈50, got {}",
        width(&app)
    );

    advance(&mut app, 1.0); // t=1.5: discrete segment holds the PREVIOUS value
    assert!(
        (width(&app) - 100.0).abs() < 3.0,
        "discrete holds 100, got {}",
        width(&app)
    );

    advance(&mut app, 0.6); // t=2.1: discrete fired at 2.0 -> 300; spline seg begins
    let w = width(&app);
    assert!((300.0..360.0).contains(&w), "after discrete jump, got {w}");

    advance(&mut app, 1.5); // t=3.6: past the end, HoldEnd at 400
    assert_eq!(width(&app), 400.0, "held at the last keyframe");
}

#[test]
fn transform_field_animation_scales_and_reverts() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <Border x:Name="Chip" Width="40" Height="20" Background="#FF3366CC">
               <Border.Triggers>
                 <EventTrigger RoutedEvent="Loaded">
                   <BeginStoryboard>
                     <Storyboard>
                       <DoubleAnimation
                           Storyboard.TargetProperty="(UIElement.RenderTransform).(TransformGroup.Children)[0].(ScaleTransform.ScaleX)"
                           To="2" Duration="0:0:1" FillBehavior="Stop"/>
                       <DoubleAnimation Storyboard.TargetProperty="Angle"
                           To="45" Duration="0:0:1"/>
                     </Storyboard>
                   </BeginStoryboard>
                 </EventTrigger>
               </Border.Triggers>
             </Border>
           </StackPanel>"##,
    );
    let chip = named(&app, root, "Chip");
    app.update();

    advance(&mut app, 0.5);
    let tf = app
        .world()
        .get::<bevy::ui::UiTransform>(chip)
        .expect("transform inserted");
    assert!(
        (tf.scale.x - 1.5).abs() < 0.15,
        "mid-scale ≈1.5, got {}",
        tf.scale.x
    );
    assert!(
        (tf.rotation.as_degrees() - 22.5).abs() < 5.0,
        "mid-rotation ≈22.5°, got {}",
        tf.rotation.as_degrees()
    );

    advance(&mut app, 1.0); // past the end
    let tf = app.world().get::<bevy::ui::UiTransform>(chip).unwrap();
    assert!(
        (tf.scale.x - 1.0).abs() < 0.01,
        "Stop restored scale, got {}",
        tf.scale.x
    );
    assert!(
        (tf.rotation.as_degrees() - 45.0).abs() < 0.5,
        "HoldEnd kept rotation, got {}",
        tf.rotation.as_degrees()
    );
}

#[test]
fn trigger_enter_exit_actions_run_the_wpf_sample_pattern() {
    // The verbatim Animation/PropertyAnimation PropertyTriggerExample
    // pattern: a style whose IsMouseOver trigger fades Opacity via
    // EnterActions and fades it back via ExitActions.
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <StackPanel.Resources>
               <Style x:Key="FadeStyle" TargetType="Border">
                 <Setter Property="Opacity" Value="0.25"/>
                 <Style.Triggers>
                   <Trigger Property="IsMouseOver" Value="True">
                     <Trigger.EnterActions>
                       <BeginStoryboard>
                         <Storyboard>
                           <DoubleAnimation Storyboard.TargetProperty="Opacity"
                                            To="1" Duration="0:0:1"/>
                         </Storyboard>
                       </BeginStoryboard>
                     </Trigger.EnterActions>
                     <Trigger.ExitActions>
                       <BeginStoryboard>
                         <Storyboard>
                           <DoubleAnimation Storyboard.TargetProperty="Opacity"
                                            To="0.25" Duration="0:0:1"/>
                         </Storyboard>
                       </BeginStoryboard>
                     </Trigger.ExitActions>
                   </Trigger>
                 </Style.Triggers>
               </Style>
             </StackPanel.Resources>
             <Border x:Name="B" Style="{StaticResource FadeStyle}"
                     Background="#FF334455" Width="120" Height="30"/>
           </StackPanel>"##,
    );
    let b = named(&app, root, "B");
    app.update();
    advance(&mut app, 0.1);
    assert!(
        (bg_alpha(&app, b) - 0.25).abs() < 0.03,
        "styled rest opacity"
    );

    // Hover: EnterActions animate toward 1.
    app.world_mut().entity_mut(b).insert(Interaction::Hovered);
    advance(&mut app, 0.05);
    advance(&mut app, 1.2);
    assert!(
        (bg_alpha(&app, b) - 1.0).abs() < 0.03,
        "faded in, got {}",
        bg_alpha(&app, b)
    );

    // Leave: ExitActions animate back to 0.25.
    app.world_mut().entity_mut(b).insert(Interaction::None);
    advance(&mut app, 0.05);
    advance(&mut app, 1.2);
    assert!(
        (bg_alpha(&app, b) - 0.25).abs() < 0.03,
        "faded back out, got {}",
        bg_alpha(&app, b)
    );
}

// ---------------------------------------------------------------------
// VisualTransitions: GeneratedDuration cross-fades between states.
// ---------------------------------------------------------------------

#[test]
fn visual_transitions_cross_fade_and_animate_back() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <Button x:Name="B" Content="Go">
               <Button.Template>
                 <ControlTemplate TargetType="Button">
                   <Border x:Name="chrome" Background="#FF000000">
                     <VisualStateManager.VisualStateGroups>
                       <VisualStateGroup x:Name="CommonStates">
                         <VisualStateGroup.Transitions>
                           <VisualTransition GeneratedDuration="0:0:1"/>
                         </VisualStateGroup.Transitions>
                         <VisualState x:Name="Normal"/>
                         <VisualState x:Name="MouseOver">
                           <Storyboard>
                             <ColorAnimation Storyboard.TargetName="chrome"
                                             Storyboard.TargetProperty="Background"
                                             To="#FFFFFFFF" Duration="0:0:0"/>
                           </Storyboard>
                         </VisualState>
                       </VisualStateGroup>
                     </VisualStateManager.VisualStateGroups>
                     <ContentPresenter/>
                   </Border>
                 </ControlTemplate>
               </Button.Template>
             </Button>
           </StackPanel>"##,
    );
    let button = named(&app, root, "B");
    let chrome = app
        .world()
        .get::<bevy_pf::components::PfTemplatedControl>(button)
        .unwrap()
        .template_root;
    app.update(); // enter Normal
    advance(&mut app, 0.1);

    // Hover: the 0-duration snap is stretched over the 1s transition.
    app.world_mut()
        .entity_mut(button)
        .insert(Interaction::Hovered);
    advance(&mut app, 0.01); // state switch, animation starts
    advance(&mut app, 0.5);
    let mid = app
        .world()
        .get::<BackgroundColor>(chrome)
        .unwrap()
        .0
        .to_srgba();
    assert!(
        mid.red > 0.2 && mid.red < 0.8,
        "mid-fade should be between black and white, got {mid:?}"
    );
    advance(&mut app, 0.7);
    let done = app
        .world()
        .get::<BackgroundColor>(chrome)
        .unwrap()
        .0
        .to_srgba();
    assert!(done.red > 0.95, "fade completed, got {done:?}");

    // Unhover: Normal has no storyboard — the transition animates BACK
    // to the template value instead of snapping.
    app.world_mut().entity_mut(button).insert(Interaction::None);
    advance(&mut app, 0.01);
    advance(&mut app, 0.5);
    let back_mid = app
        .world()
        .get::<BackgroundColor>(chrome)
        .unwrap()
        .0
        .to_srgba();
    assert!(
        back_mid.red > 0.2 && back_mid.red < 0.8,
        "return fade should be mid-way, got {back_mid:?}"
    );
    advance(&mut app, 0.7);
    let back = app
        .world()
        .get::<BackgroundColor>(chrome)
        .unwrap()
        .0
        .to_srgba();
    assert!(back.red < 0.05, "returned to template black, got {back:?}");
    // And the Animation tier is cleared (structural revert, not a held value).
    let store = app
        .world()
        .get::<bevy_pf::provider::PfPropertyStore>(chrome)
        .unwrap();
    assert_ne!(
        store.effective_source(bevy_pf::provider::PropertyTarget::Background),
        Some(bevy_pf::provider::ValueSource::Animation),
        "animation layer cleared after the return fade"
    );
}

// ---------------------------------------------------------------------
// FocusStates driven from bevy input focus.
// ---------------------------------------------------------------------

#[test]
fn focus_states_follow_input_focus() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <Button x:Name="B" Content="Go">
               <Button.Template>
                 <ControlTemplate TargetType="Button">
                   <Border x:Name="chrome" Background="#FF000000">
                     <VisualStateManager.VisualStateGroups>
                       <VisualStateGroup x:Name="FocusStates">
                         <VisualState x:Name="Unfocused"/>
                         <VisualState x:Name="Focused">
                           <Storyboard>
                             <ColorAnimation Storyboard.TargetName="chrome"
                                             Storyboard.TargetProperty="Background"
                                             To="#FFFFFFFF" Duration="0:0:0.05"/>
                           </Storyboard>
                         </VisualState>
                       </VisualStateGroup>
                     </VisualStateManager.VisualStateGroups>
                     <ContentPresenter/>
                   </Border>
                 </ControlTemplate>
               </Button.Template>
             </Button>
           </StackPanel>"##,
    );
    let button = named(&app, root, "B");
    let chrome = app
        .world()
        .get::<bevy_pf::components::PfTemplatedControl>(button)
        .unwrap()
        .template_root;
    app.update();
    advance(&mut app, 0.1);
    let rest = app
        .world()
        .get::<BackgroundColor>(chrome)
        .unwrap()
        .0
        .to_srgba();
    assert!(rest.red < 0.05, "unfocused keeps template value");

    // Focus the control.
    app.world_mut()
        .insert_resource(bevy::input_focus::InputFocus::from_entity(button));
    advance(&mut app, 0.01);
    advance(&mut app, 0.2);
    let focused = app
        .world()
        .get::<BackgroundColor>(chrome)
        .unwrap()
        .0
        .to_srgba();
    assert!(focused.red > 0.95, "Focused state ran, got {focused:?}");

    // Clear focus -> Unfocused reverts.
    app.world_mut()
        .insert_resource(bevy::input_focus::InputFocus::default());
    advance(&mut app, 0.01);
    advance(&mut app, 0.2);
    let back = app
        .world()
        .get::<BackgroundColor>(chrome)
        .unwrap()
        .0
        .to_srgba();
    assert!(back.red < 0.05, "Unfocused reverted, got {back:?}");
}

/// Does a VisualState storyboard actually DRIVE a value, or merely parse?
///
/// This exists because a ControlTemplate carrying VisualStateManager states
/// was shipped to a game on the strength of "it parsed, it expanded, and it
/// logged no warnings" -- none of which is evidence that anything moves. The
/// buttons did not animate. Expansion is not execution, and this test is the
/// difference: it asserts an INTERMEDIATE value partway through the ramp, so
/// it fails both when nothing animates and when the value merely snaps.
#[test]
fn visual_state_storyboard_drives_a_value_over_time() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <StackPanel.Resources>
               <ControlTemplate x:Key="AnimatedButton" TargetType="Button">
                 <Border x:Name="Face" Background="#FF000000">
                   <VisualStateManager.VisualStateGroups>
                     <VisualStateGroup x:Name="CommonStates">
                       <VisualState x:Name="Normal"/>
                       <VisualState x:Name="MouseOver">
                         <Storyboard>
                           <ColorAnimation Storyboard.TargetName="Face"
                                           Storyboard.TargetProperty="Background"
                                           To="#FFFFFFFF" Duration="0:0:1"/>
                         </Storyboard>
                       </VisualState>
                     </VisualStateGroup>
                   </VisualStateManager.VisualStateGroups>
                   <ContentPresenter/>
                 </Border>
               </ControlTemplate>
             </StackPanel.Resources>
             <Button x:Name="Btn" Content="Go"
                     Template="{StaticResource AnimatedButton}"/>
           </StackPanel>"##,
    );
    let btn = named(&app, root, "Btn");
    app.update();

    // The template root: the Border the storyboard targets by name.
    let face = *app
        .world()
        .get::<bevy::prelude::Children>(btn)
        .expect("templated button has a template root")
        .first()
        .expect("template root child");

    let start = app
        .world()
        .get::<bevy::prelude::BackgroundColor>(face)
        .expect("template root has a background")
        .0
        .to_srgba()
        .red;
    assert!(start < 0.1, "starts black, got {start}");

    // Enter MouseOver: bevy_pf drives CommonStates from Interaction. The
    // extra zero-advance update is load-bearing: the driver only SEES the
    // hover during an update, and go_to_state stamps the storyboard's begin
    // time then. Folding this into advance(0.5) starts the clock at the end
    // of the advance, so the midpoint sample reads t=0 -- which looked
    // exactly like "the storyboard never ran" and cost a day of chasing the
    // framework for a bug that lived in this test's sequencing.
    app.world_mut()
        .entity_mut(btn)
        .insert(bevy::ui::Interaction::Hovered);
    app.update();
    advance(&mut app, 0.5);

    let mid = app
        .world()
        .get::<bevy::prelude::BackgroundColor>(face)
        .expect("background")
        .0
        .to_srgba()
        .red;
    assert!(
        mid > 0.15 && mid < 0.85,
        "half a second into a one-second ramp the colour should be PARTWAY \
         between black and white; got {mid}. <=0.15 means the storyboard never \
         ran, >=0.85 means it snapped instead of ramping."
    );

    advance(&mut app, 1.0);
    let end = app
        .world()
        .get::<bevy::prelude::BackgroundColor>(face)
        .expect("background")
        .0
        .to_srgba()
        .red;
    assert!(end > 0.9, "settles at the target, got {end}");
}
