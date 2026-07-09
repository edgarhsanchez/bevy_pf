//! Style.Triggers end-to-end: activation, precedence-correct revert,
//! last-active-wins, DataTriggers, and MultiTriggers.

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy::reflect::Reflect;
use bevy::ui::Checked;
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
    let result = instantiate_document_env(world, root, &doc, &XamlEnv::default())
        .expect("instantiates");
    assert!(
        result.warnings.is_empty(),
        "expected clean instantiation: {:?}",
        result.warnings
    );
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

const RED: Color = Color::srgba(1.0, 0.0, 0.0, 1.0);

#[test]
fn hover_trigger_applies_and_reverts_to_style() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <StackPanel.Resources>
               <Style TargetType="Border">
                 <Setter Property="Background" Value="Green"/>
                 <Style.Triggers>
                   <Trigger Property="IsMouseOver" Value="True">
                     <Setter Property="Background" Value="Red"/>
                   </Trigger>
                 </Style.Triggers>
               </Style>
             </StackPanel.Resources>
             <Border x:Name="B"/>
           </StackPanel>"#,
    );
    app.update();
    let b = named(&app, root, "B");
    let green = Color::srgba_u8(0, 128, 0, 255);
    assert_eq!(bg(&app, b), green);
    // Triggers referencing IsMouseOver get an Interaction component.
    assert!(app.world().get::<Interaction>(b).is_some());

    // Hover on -> trigger applies at StyleTrigger tier.
    app.world_mut().entity_mut(b).insert(Interaction::Hovered);
    app.update();
    assert_eq!(bg(&app, b), Color::srgba_u8(255, 0, 0, 255));

    // Hover off -> reverts to the style setter's Green (not transparent).
    app.world_mut().entity_mut(b).insert(Interaction::None);
    app.update();
    assert_eq!(bg(&app, b), green);
}

#[test]
fn trigger_reverts_to_button_chrome_default() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <StackPanel.Resources>
               <Style TargetType="Button">
                 <Style.Triggers>
                   <Trigger Property="IsMouseOver" Value="True">
                     <Setter Property="Background" Value="Red"/>
                   </Trigger>
                 </Style.Triggers>
               </Style>
             </StackPanel.Resources>
             <Button x:Name="B" Content="hi"/>
           </StackPanel>"#,
    );
    app.update();
    let b = named(&app, root, "B");
    // Default chrome initially.
    let chrome = Color::srgba_u8(0xE1, 0xE1, 0xE1, 255);
    assert_eq!(bg(&app, b), chrome);

    app.world_mut().entity_mut(b).insert(Interaction::Hovered);
    app.update();
    assert_eq!(bg(&app, b), Color::srgba_u8(255, 0, 0, 255));

    // Revert lands on the seeded Default-tier chrome color.
    app.world_mut().entity_mut(b).insert(Interaction::None);
    app.update();
    assert_eq!(bg(&app, b), chrome);
}

#[test]
fn local_value_beats_trigger() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <StackPanel.Resources>
               <Style TargetType="Border">
                 <Style.Triggers>
                   <Trigger Property="IsMouseOver" Value="True">
                     <Setter Property="Background" Value="Red"/>
                   </Trigger>
                 </Style.Triggers>
               </Style>
             </StackPanel.Resources>
             <Border x:Name="B" Background="Blue"/>
           </StackPanel>"#,
    );
    app.update();
    let b = named(&app, root, "B");
    let blue = Color::srgba_u8(0, 0, 255, 255);
    assert_eq!(bg(&app, b), blue);

    // Even while the trigger is active, Local outranks StyleTrigger.
    app.world_mut().entity_mut(b).insert(Interaction::Hovered);
    app.update();
    assert_eq!(bg(&app, b), blue);
}

#[test]
fn ischecked_trigger() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <StackPanel.Resources>
               <Style TargetType="CheckBox">
                 <Style.Triggers>
                   <Trigger Property="IsChecked" Value="True">
                     <Setter Property="Background" Value="Red"/>
                   </Trigger>
                 </Style.Triggers>
               </Style>
             </StackPanel.Resources>
             <CheckBox x:Name="C" Content="on"/>
           </StackPanel>"#,
    );
    app.update();
    let c = named(&app, root, "C");
    app.world_mut().entity_mut(c).insert(Checked);
    app.update();
    assert_eq!(bg(&app, c), Color::srgba_u8(255, 0, 0, 255));
    app.world_mut().entity_mut(c).remove::<Checked>();
    app.update();
    // CheckBox has no background of its own -> reverts to unset/transparent.
    assert_eq!(bg(&app, c).alpha(), 0.0);
}

#[derive(Reflect, Default)]
struct Vm {
    status: String,
    count: u32,
}

#[test]
fn data_trigger_follows_view_model() {
    let mut app = test_app();
    let vm = Bindable::new(Vm {
        status: "ok".into(),
        ..Default::default()
    });
    let root = spawn(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <StackPanel.Resources>
               <Style TargetType="Border">
                 <Setter Property="Background" Value="Green"/>
                 <Style.Triggers>
                   <DataTrigger Binding="{Binding status}" Value="alarm">
                     <Setter Property="Background" Value="Red"/>
                   </DataTrigger>
                 </Style.Triggers>
               </Style>
             </StackPanel.Resources>
             <Border x:Name="B"/>
           </StackPanel>"#,
    );
    app.world_mut().entity_mut(root).insert(DataContext(vm.clone()));
    app.update();
    let b = named(&app, root, "B");
    assert_eq!(bg(&app, b), Color::srgba_u8(0, 128, 0, 255));

    vm.update(|m: &mut Vm| m.status = "alarm".into());
    app.update();
    assert_eq!(bg(&app, b), Color::srgba_u8(255, 0, 0, 255));

    vm.update(|m: &mut Vm| m.status = "ok".into());
    app.update();
    assert_eq!(bg(&app, b), Color::srgba_u8(0, 128, 0, 255));
}

#[test]
fn multi_trigger_requires_all_conditions() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <StackPanel.Resources>
               <Style TargetType="CheckBox">
                 <Style.Triggers>
                   <MultiTrigger>
                     <MultiTrigger.Conditions>
                       <Condition Property="IsMouseOver" Value="True"/>
                       <Condition Property="IsChecked" Value="True"/>
                     </MultiTrigger.Conditions>
                     <Setter Property="Background" Value="Red"/>
                   </MultiTrigger>
                 </Style.Triggers>
               </Style>
             </StackPanel.Resources>
             <CheckBox x:Name="C" Content="x"/>
           </StackPanel>"#,
    );
    app.update();
    let c = named(&app, root, "C");

    // One condition only: inactive.
    app.world_mut().entity_mut(c).insert(Interaction::Hovered);
    app.update();
    assert_ne!(bg(&app, c), RED);

    // Both: active.
    app.world_mut().entity_mut(c).insert(Checked);
    app.update();
    assert_eq!(bg(&app, c), Color::srgba_u8(255, 0, 0, 255));

    // Drop one: inactive again.
    app.world_mut().entity_mut(c).remove::<Checked>();
    app.update();
    assert_ne!(bg(&app, c), RED);
}

#[test]
fn last_declared_active_trigger_wins() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <StackPanel.Resources>
               <Style TargetType="CheckBox">
                 <Style.Triggers>
                   <Trigger Property="IsMouseOver" Value="True">
                     <Setter Property="Background" Value="Red"/>
                   </Trigger>
                   <Trigger Property="IsChecked" Value="True">
                     <Setter Property="Background" Value="Blue"/>
                   </Trigger>
                 </Style.Triggers>
               </Style>
             </StackPanel.Resources>
             <CheckBox x:Name="C" Content="x"/>
           </StackPanel>"#,
    );
    app.update();
    let c = named(&app, root, "C");

    // Both active: the later-declared (IsChecked) wins.
    app.world_mut()
        .entity_mut(c)
        .insert((Interaction::Hovered, Checked));
    app.update();
    assert_eq!(bg(&app, c), Color::srgba_u8(0, 0, 255, 255));

    // Drop the later one: the earlier active trigger's value shows.
    app.world_mut().entity_mut(c).remove::<Checked>();
    app.update();
    assert_eq!(bg(&app, c), Color::srgba_u8(255, 0, 0, 255));
}

#[test]
fn x_null_trigger_setter_clears() {
    let mut app = test_app();
    let root = spawn(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <StackPanel.Resources>
               <Style TargetType="Border">
                 <Setter Property="Background" Value="Green"/>
                 <Style.Triggers>
                   <Trigger Property="IsMouseOver" Value="True">
                     <Setter Property="Background" Value="{x:Null}"/>
                   </Trigger>
                 </Style.Triggers>
               </Style>
             </StackPanel.Resources>
             <Border x:Name="B"/>
           </StackPanel>"#,
    );
    app.update();
    let b = named(&app, root, "B");
    assert_eq!(bg(&app, b), Color::srgba_u8(0, 128, 0, 255));

    app.world_mut().entity_mut(b).insert(Interaction::Hovered);
    app.update();
    assert_eq!(bg(&app, b).alpha(), 0.0); // {x:Null} masks the style setter

    app.world_mut().entity_mut(b).insert(Interaction::None);
    app.update();
    assert_eq!(bg(&app, b), Color::srgba_u8(0, 128, 0, 255));
}
