//! ControlTemplate phase 1: representation and parsing (no expansion yet).
//! All three delivery channels parse into `PfValue::ControlTemplate`, the
//! DataTemplate/ControlTemplate kinds reject each other, `{x:Type}` keys
//! meet `{StaticResource {x:Type ...}}` lookups, and `{x:Null}` trigger
//! values survive parsing.

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy_pf::prelude::*;
use bevy_pf::resources::{
    PfSetterValue, PfTriggerValue, PfValue, ResourceDictionary, ResourceScopes,
    parse_resource_value,
};
use bevy_pf::{XamlEnv, instantiate_document_env};

fn parse_value(xaml: &str) -> (Option<PfValue>, Vec<String>) {
    let doc = bevy_pf_xaml::parse(xaml).expect("parses");
    let mut warnings = Vec::new();
    let value = parse_resource_value(
        &doc.root,
        &ResourceScopes::default(),
        &ResourceDictionary::new(),
        &mut warnings,
    )
    .expect("no hard error");
    (value, warnings)
}

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()));
    app.add_plugins(PfUiPlugin);
    app
}

fn spawn_collect_warnings(app: &mut App, xaml: &str) -> (Entity, Vec<String>) {
    let doc = bevy_pf_xaml::parse(xaml).expect("parses");
    let world = app.world_mut();
    let root = world.spawn_empty().id();
    let result =
        instantiate_document_env(world, root, &doc, &XamlEnv::default()).expect("instantiates");
    (root, result.warnings)
}

#[test]
fn keyed_control_template_parses_with_triggers_and_target_name() {
    let (value, warnings) = parse_value(
        r##"<ControlTemplate xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                             xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                             x:Key="T" TargetType="Button">
              <Border x:Name="border" Background="#FFDDDDDD">
                <ContentPresenter/>
              </Border>
              <ControlTemplate.Triggers>
                <Trigger Property="IsMouseOver" Value="True">
                  <Setter Property="Background" Value="#FFBEE6FD"/>
                </Trigger>
                <Trigger Property="IsPressed" Value="True">
                  <Setter TargetName="border" Property="Background" Value="#FFC4E5F6"/>
                </Trigger>
              </ControlTemplate.Triggers>
            </ControlTemplate>"##,
    );
    assert_eq!(warnings, Vec::<String>::new());
    let Some(PfValue::ControlTemplate(t)) = value else {
        panic!("expected ControlTemplate, got {value:?}");
    };
    assert_eq!(t.target_type.as_deref(), Some("Button"));
    assert_eq!(t.root.name, "Border");
    assert_eq!(t.triggers.len(), 2);
    assert_eq!(t.triggers[0].setters[0].target_name, None);
    assert_eq!(
        t.triggers[1].setters[0].target_name.as_deref(),
        Some("border")
    );
}

#[test]
fn style_setter_template_carries_control_template() {
    for property in ["Template", "Control.Template"] {
        let (value, warnings) = parse_value(&format!(
            r##"<Style xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                       TargetType="Button">
                  <Setter Property="{property}">
                    <Setter.Value>
                      <ControlTemplate TargetType="Button">
                        <Border><ContentPresenter/></Border>
                      </ControlTemplate>
                    </Setter.Value>
                  </Setter>
                </Style>"##
        ));
        assert_eq!(warnings, Vec::<String>::new(), "{property}");
        let Some(PfValue::Style(style)) = value else {
            panic!("expected Style");
        };
        assert_eq!(style.setters.len(), 1);
        let setter = &style.setters[0];
        assert_eq!(setter.owner, None, "{property} normalizes to unqualified");
        assert_eq!(setter.property, "Template");
        assert!(
            matches!(setter.value, PfSetterValue::Value(PfValue::ControlTemplate(_))),
            "{property}: got {:?}",
            setter.value
        );
    }
}

#[test]
fn template_kinds_reject_each_other() {
    let mut app = test_app();
    let (_, warnings) = spawn_collect_warnings(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <StackPanel.Resources>
               <ControlTemplate x:Key="CT" TargetType="Button">
                 <Border/>
               </ControlTemplate>
               <DataTemplate x:Key="DT">
                 <TextBlock Text="row"/>
               </DataTemplate>
             </StackPanel.Resources>
             <ItemsControl ItemTemplate="{StaticResource CT}"/>
             <Button Template="{StaticResource DT}" Content="x"/>
           </StackPanel>"##,
    );
    assert_eq!(warnings.len(), 2, "one rejection each: {warnings:?}");
    assert!(warnings.iter().any(|w| w.contains("got a ControlTemplate")));
    assert!(warnings.iter().any(|w| w.contains("got a DataTemplate")));
}

#[test]
fn templated_button_stores_and_renders_default_chrome_for_now() {
    // Phase 1 regression guard: a style-delivered template is recorded and
    // ignored; the Button renders its default chrome with zero warnings.
    let mut app = test_app();
    let (root, warnings) = spawn_collect_warnings(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <StackPanel.Resources>
               <Style TargetType="Button">
                 <Setter Property="Template">
                   <Setter.Value>
                     <ControlTemplate TargetType="Button">
                       <Border><ContentPresenter/></Border>
                     </ControlTemplate>
                   </Setter.Value>
                 </Setter>
               </Style>
             </StackPanel.Resources>
             <Button x:Name="B" Content="hello"/>
           </StackPanel>"##,
    );
    assert_eq!(warnings, Vec::<String>::new());
    let button = app.world().get::<XamlNames>(root).unwrap().get("B").unwrap();
    assert!(
        app.world().get::<bevy_pf::ButtonVisual>(button).is_some(),
        "default chrome still present until expansion lands"
    );
}

#[test]
fn x_type_keys_meet_static_resource_type_lookups() {
    let mut app = test_app();
    let (root, warnings) = spawn_collect_warnings(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <StackPanel.Resources>
               <Style x:Key="{x:Type TextBoxBase}" TargetType="TextBox">
                 <Setter Property="Foreground" Value="#FF112233"/>
               </Style>
               <Style x:Key="Derived" TargetType="TextBox"
                      BasedOn="{StaticResource {x:Type TextBoxBase}}">
                 <Setter Property="FontSize" Value="15"/>
               </Style>
             </StackPanel.Resources>
             <TextBox x:Name="T" Style="{StaticResource Derived}"/>
           </StackPanel>"##,
    );
    assert_eq!(warnings, Vec::<String>::new(), "key spaces meet: {warnings:?}");
    // The base style's Foreground reached the TextBox through BasedOn.
    let root_names = app.world().get::<XamlNames>(root).unwrap();
    let tb = root_names.get("T").unwrap();
    let input = app
        .world()
        .get::<Children>(tb)
        .and_then(|c| c.iter().next())
        .unwrap();
    let color = app.world().get::<bevy::text::TextColor>(input).unwrap().0;
    assert_eq!(
        bevy_pf::instantiate::color_to_hex(color),
        "#112233",
        "BasedOn {{x:Type}} base style applied"
    );
}

#[test]
fn x_null_trigger_value_parses_and_skips_at_attach() {
    // Parse level: the trigger survives with a Null condition value.
    let (value, warnings) = parse_value(
        r##"<Style xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                   xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                   TargetType="CheckBox">
              <Style.Triggers>
                <Trigger Property="IsChecked" Value="{x:Null}">
                  <Setter Property="Foreground" Value="Red"/>
                </Trigger>
              </Style.Triggers>
            </Style>"##,
    );
    assert_eq!(warnings, Vec::<String>::new());
    let Some(PfValue::Style(style)) = value else {
        panic!("expected Style");
    };
    assert_eq!(style.triggers.len(), 1, "trigger retained at parse");
    let bevy_pf::resources::PfTriggerCondition::Property { value, .. } =
        &style.triggers[0].conditions[0]
    else {
        panic!("expected property condition");
    };
    assert_eq!(*value, PfTriggerValue::Null);

    // Attach level: warn-and-skip, no panic.
    let mut app = test_app();
    let (_, warnings) = spawn_collect_warnings(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <StackPanel.Resources>
               <Style TargetType="CheckBox">
                 <Style.Triggers>
                   <Trigger Property="IsChecked" Value="{x:Null}">
                     <Setter Property="Foreground" Value="Red"/>
                   </Trigger>
                 </Style.Triggers>
               </Style>
             </StackPanel.Resources>
             <CheckBox Content="tri-state"/>
           </StackPanel>"##,
    );
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("three-state"), "{warnings:?}");
}

#[test]
fn style_trigger_target_name_still_skips_with_warning() {
    let (value, warnings) = parse_value(
        r##"<Style xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                   xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                   TargetType="Button">
              <Style.Triggers>
                <Trigger Property="IsMouseOver" Value="True">
                  <Setter TargetName="border" Property="Background" Value="Red"/>
                </Trigger>
              </Style.Triggers>
            </Style>"##,
    );
    assert!(matches!(value, Some(PfValue::Style(_))));
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("only valid in ControlTemplate"));
}
