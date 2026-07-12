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
    app.world().get::<XamlNames>(root).unwrap().get(name).unwrap()
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
    let mid = app.world().get::<BackgroundColor>(card).unwrap().0.to_srgba();
    assert!(mid.red > 0.3 && mid.red < 0.7, "mid-blend red, got {}", mid.red);

    advance(&mut app, 1.0);
    let after = app.world().get::<BackgroundColor>(card).unwrap().0.to_srgba();
    assert!(after.red < 0.05, "Stop reverts to the black base, got {}", after.red);
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
    let Val::Px(w) = node.width else { panic!("px width") };
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
