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
    app.world_mut()
        .entity_mut(root)
        .insert(DataContext(vm.clone()));
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
    app.world_mut()
        .entity_mut(root)
        .insert(DataContext(vm.clone()));
    app.update();

    let cb = app
        .world()
        .get::<XamlNames>(root)
        .unwrap()
        .get("Ready")
        .unwrap();
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
    app.world_mut()
        .entity_mut(root)
        .insert(DataContext(vm.clone()));
    app.update();

    let tb = app
        .world()
        .get::<XamlNames>(root)
        .unwrap()
        .get("Status")
        .unwrap();
    let input = app
        .world()
        .get::<Children>(tb)
        .unwrap()
        .iter()
        .next()
        .unwrap();
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
    app.world_mut()
        .entity_mut(root)
        .insert(DataContext(vm.clone()));
    app.update();

    let bar = app
        .world()
        .get::<XamlNames>(root)
        .unwrap()
        .get("P")
        .unwrap();
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
    app.world_mut()
        .entity_mut(root)
        .insert(DataContext(vm.clone()));
    app.update();

    let button = app
        .world()
        .get::<XamlNames>(root)
        .unwrap()
        .get("B")
        .unwrap();
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

    app.world_mut()
        .entity_mut(toggle)
        .remove::<bevy::ui::Checked>();
    app.update();
    assert_eq!(app.world().get::<Text>(state).unwrap().0, "False");
}

#[derive(Reflect, Default)]
struct ConvVm {
    online: bool,
    credits: f64,
}

#[test]
fn value_converters_and_fallback() {
    struct Doubler;
    impl bevy_pf::binding::PfValueConverter for Doubler {
        fn convert(
            &self,
            value: &bevy_pf::BoundValue,
            parameter: Option<&str>,
        ) -> Option<bevy_pf::BoundValue> {
            let factor: f64 = parameter.and_then(|p| p.parse().ok()).unwrap_or(2.0);
            match value {
                bevy_pf::BoundValue::Num(n) => Some(bevy_pf::BoundValue::Num(n * factor)),
                _ => None,
            }
        }
    }

    let mut app = test_app();
    app.register_converter("Doubler", Doubler);
    let vm = Bindable::new(ConvVm {
        online: true,
        credits: 21.0,
    });
    let root = spawn_bound_scene(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <TextBlock x:Name="Badge" Text="{Binding online, Converter={StaticResource BooleanToVisibilityConverter}}"/>
             <Border x:Name="Panel" Background="#FF223344" Width="40" Height="10"
                     Visibility="{Binding online, Converter={StaticResource BooleanToVisibilityConverter}}"/>
             <TextBlock x:Name="Credits" Text="{Binding credits, Converter={StaticResource Doubler}, ConverterParameter=3}"/>
             <TextBlock x:Name="Missing" Text="{Binding no_such_field, FallbackValue=offline}"/>
           </StackPanel>"##,
        vm.clone(),
    );
    app.update();

    let (badge, credits, missing, panel) = {
        let names = app.world().get::<XamlNames>(root).unwrap();
        (
            names.get("Badge").unwrap(),
            names.get("Credits").unwrap(),
            names.get("Missing").unwrap(),
            names.get("Panel").unwrap(),
        )
    };
    let text = |app: &App, e: Entity| -> String {
        app.world()
            .get::<bevy::ui::widget::Text>(e)
            .unwrap()
            .0
            .clone()
    };
    // Built-in bool->visibility converter, applied to a text target too.
    assert_eq!(text(&app, badge), "Visible");
    // Custom converter with a parameter: 21 * 3.
    assert_eq!(text(&app, credits), "63");
    // FallbackValue covers the unresolvable path (no warning spam).
    assert_eq!(text(&app, missing), "offline");
    // Visibility target actually toggles layout.
    assert_ne!(
        app.world().get::<Node>(panel).unwrap().display,
        Display::None
    );

    vm.update(|m: &mut ConvVm| m.online = false);
    app.update();
    assert_eq!(text(&app, badge), "Collapsed");
    assert_eq!(
        app.world().get::<Node>(panel).unwrap().display,
        Display::None
    );
}

#[test]
fn relative_source_self_and_templated_parent() {
    let mut app = test_app();
    let vm = Bindable::new(ConvVm {
        online: true,
        credits: 5.0,
    });
    let root = spawn_bound_scene(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <!-- Self: a toggle reporting its own checked state. -->
             <CheckBox x:Name="Chk" Content="on?" IsChecked="True"/>
             <TextBlock x:Name="SelfRead"
                        Text="{Binding IsChecked, RelativeSource={RelativeSource Self}}"/>
             <!-- TemplatedParent: template chrome reading the control. -->
             <ToggleButton x:Name="T" Content="pin" IsChecked="True">
               <ToggleButton.Template>
                 <ControlTemplate TargetType="ToggleButton">
                   <Border Background="#FF223344" Padding="4">
                     <TextBlock x:Name="state"
                                Text="{Binding IsChecked, RelativeSource={RelativeSource TemplatedParent}}"/>
                   </Border>
                 </ControlTemplate>
               </ToggleButton.Template>
             </ToggleButton>
           </StackPanel>"##,
        vm,
    );
    app.update();

    let toggle = {
        let names = app.world().get::<XamlNames>(root).unwrap();
        names.get("T").unwrap()
    };
    // Find the templated text (template-internal names are per-expansion).
    fn find_text(app: &App, e: Entity) -> Option<Entity> {
        if app.world().get::<bevy::ui::widget::Text>(e).is_some() {
            return Some(e);
        }
        let children: Vec<Entity> = app
            .world()
            .get::<Children>(e)
            .map(|c| c.iter().collect())
            .unwrap_or_default();
        children.into_iter().find_map(|c| find_text(app, c))
    }
    let chrome_root = app
        .world()
        .get::<bevy_pf::components::PfTemplatedControl>(toggle)
        .unwrap()
        .template_root;
    let state_text = find_text(&app, chrome_root).unwrap();
    assert_eq!(
        app.world()
            .get::<bevy::ui::widget::Text>(state_text)
            .unwrap()
            .0,
        "True",
        "template chrome reads the templated parent"
    );

    // Self-source stays live: uncheck -> re-renders.
    app.world_mut()
        .entity_mut(toggle)
        .remove::<bevy::ui::Checked>();
    app.update();
    assert_eq!(
        app.world()
            .get::<bevy::ui::widget::Text>(state_text)
            .unwrap()
            .0,
        "False"
    );
}

#[derive(Reflect, Default)]
struct SelVm {
    pick: f64,
}

#[test]
fn selected_index_binds_two_way_on_listbox() {
    let mut app = test_app();
    let vm = Bindable::new(SelVm { pick: 1.0 });
    let root = spawn_bound_scene(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <ListBox x:Name="L" SelectedIndex="{Binding pick}">
               <ListBoxItem><TextBlock Text="alpha"/></ListBoxItem>
               <ListBoxItem><TextBlock Text="beta"/></ListBoxItem>
               <ListBoxItem><TextBlock Text="gamma"/></ListBoxItem>
             </ListBox>
           </StackPanel>"##,
        vm.clone(),
    );
    app.update();

    // To-target: vm.pick=1 selects the second item.
    let (list_entity, items) = {
        let names = app.world().get::<XamlNames>(root).unwrap();
        let l = names.get("L").unwrap();
        let children: Vec<Entity> = app.world().get::<Children>(l).unwrap().iter().collect();
        (l, children)
    };
    let selected = app
        .world()
        .get::<bevy_pf::components::PfListBox>(list_entity)
        .unwrap()
        .selected;
    assert_eq!(selected, Some(items[1]), "initial binding selected index 1");

    // VM change re-targets the selection.
    vm.update(|m: &mut SelVm| m.pick = 2.0);
    app.update();
    let selected = app
        .world()
        .get::<bevy_pf::components::PfListBox>(list_entity)
        .unwrap()
        .selected;
    assert_eq!(selected, Some(items[2]));

    // Write-back: user selection (set directly, as the click handler does)
    // flows into the VM.
    app.world_mut()
        .get_mut::<bevy_pf::components::PfListBox>(list_entity)
        .unwrap()
        .selected = Some(items[0]);
    app.update();
    assert_eq!(
        vm.read_path("pick").and_then(|v| v.as_f64()),
        Some(0.0),
        "selection wrote back"
    );
}

#[derive(Reflect, Default)]
struct NullVm {
    nickname: Option<String>,
    done: f64,
    total: f64,
}

#[test]
fn target_null_value_substitutes_for_none() {
    let mut app = test_app();
    let vm = Bindable::new(NullVm {
        nickname: None,
        ..Default::default()
    });
    let root = spawn_bound_scene(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <TextBlock x:Name="T" Text="{Binding nickname, TargetNullValue='(no nickname)'}"/>
           </StackPanel>"##,
        vm.clone(),
    );
    app.update();
    let names = app.world().get::<XamlNames>(root).unwrap();
    let t = names.get("T").unwrap();
    let text_of = |app: &App, e| app.world().get::<Text>(e).map(|t| t.0.clone());
    assert_eq!(text_of(&app, t).as_deref(), Some("(no nickname)"));

    // Some(value) unwraps to the inner value.
    vm.update(|m: &mut NullVm| m.nickname = Some("Ace".into()));
    app.update();
    assert_eq!(text_of(&app, t).as_deref(), Some("Ace"));

    // Back to None -> the null substitute again.
    vm.update(|m: &mut NullVm| m.nickname = None);
    app.update();
    assert_eq!(text_of(&app, t).as_deref(), Some("(no nickname)"));
}

#[test]
fn multi_binding_string_format_and_converter() {
    let mut app = test_app();
    struct Ratio;
    impl PfMultiValueConverter for Ratio {
        fn convert(
            &self,
            values: &[bevy_pf::BoundValue],
            _parameter: Option<&str>,
        ) -> Option<bevy_pf::BoundValue> {
            let a = values.first()?.as_f64()?;
            let b = values.get(1)?.as_f64()?;
            Some(bevy_pf::BoundValue::Num(if b == 0.0 { 0.0 } else { a / b }))
        }
    }
    app.register_multi_converter("Ratio", Ratio);
    let vm = Bindable::new(NullVm {
        done: 3.0,
        total: 4.0,
        ..Default::default()
    });
    let root = spawn_bound_scene(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <TextBlock x:Name="F">
               <TextBlock.Text>
                 <MultiBinding StringFormat="{}{0} of {1} done">
                   <Binding Path="done"/>
                   <Binding Path="total"/>
                 </MultiBinding>
               </TextBlock.Text>
             </TextBlock>
             <TextBlock x:Name="C">
               <TextBlock.Text>
                 <MultiBinding Converter="{StaticResource Ratio}" StringFormat="{}{0:P0}">
                   <Binding Path="done"/>
                   <Binding Path="total"/>
                 </MultiBinding>
               </TextBlock.Text>
             </TextBlock>
           </StackPanel>"##,
        vm.clone(),
    );
    app.update();
    let names = app.world().get::<XamlNames>(root).unwrap();
    let (f, c) = (names.get("F").unwrap(), names.get("C").unwrap());
    let text_of = |app: &App, e| app.world().get::<Text>(e).map(|t| t.0.clone());
    assert_eq!(text_of(&app, f).as_deref(), Some("3 of 4 done"));

    // Both members update live.
    vm.update(|m: &mut NullVm| m.done = 2.0);
    vm.update(|m: &mut NullVm| m.total = 8.0);
    app.update();
    assert_eq!(text_of(&app, f).as_deref(), Some("2 of 8 done"));
    // Converter result flows through the single-value StringFormat path?
    // No: multi bindings format positionally; the converter output here is
    // {0}. 2/8 = 25%.
    let c_text = text_of(&app, c).unwrap();
    assert!(c_text.contains("25"), "ratio converter applied: {c_text}");
}

#[test]
fn find_ancestor_binds_to_enclosing_element() {
    let mut app = test_app();
    let vm = Bindable::new(NullVm::default());
    let root = spawn_bound_scene(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <Border Width="240" Padding="4">
               <StackPanel>
                 <TextBlock x:Name="W"
                            Text="{Binding ActualWidth, RelativeSource={RelativeSource Mode=FindAncestor, AncestorType={x:Type Border}}, StringFormat='{}w={0:F0}'}"/>
               </StackPanel>
             </Border>
           </StackPanel>"##,
        vm,
    );
    app.update();
    app.update(); // element sources re-evaluate after layout state settles

    let names = app.world().get::<XamlNames>(root).unwrap();
    let w = names.get("W").unwrap();
    // Headless: no real layout pass; the binding resolved to the Border and
    // the element read yielded its configured width through the store.
    let text = app.world().get::<Text>(w).map(|t| t.0.clone()).unwrap();
    assert!(
        text.starts_with("w="),
        "FindAncestor bound and formatted: {text}"
    );
}

#[test]
fn two_way_element_bindings_write_back_to_the_source_element() {
    let mut app = test_app();
    let vm = Bindable::new(NullVm::default());
    let root = spawn_bound_scene(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <CheckBox x:Name="A" Content="mirror"
                       IsChecked="{Binding IsChecked, ElementName=B, Mode=TwoWay}"/>
             <CheckBox x:Name="B" Content="source"/>
             <ToggleButton x:Name="T" Content="templated">
               <ToggleButton.Template>
                 <ControlTemplate TargetType="ToggleButton">
                   <Border Padding="2">
                     <CheckBox x:Name="part"
                               IsChecked="{Binding IsChecked, RelativeSource={RelativeSource TemplatedParent}, Mode=TwoWay}"/>
                   </Border>
                 </ControlTemplate>
               </ToggleButton.Template>
             </ToggleButton>
           </StackPanel>"##,
        vm,
    );
    app.update();
    let names = app.world().get::<XamlNames>(root).unwrap();
    let (a, b, t) = (
        names.get("A").unwrap(),
        names.get("B").unwrap(),
        names.get("T").unwrap(),
    );

    // Source -> target (existing element read): checking B mirrors into A.
    app.world_mut().entity_mut(b).insert(Checked);
    app.update();
    assert!(app.world().get::<Checked>(a).is_some(), "B -> A mirrored");

    // Target -> source (new write-back): unchecking A writes through to B.
    app.world_mut().entity_mut(a).remove::<Checked>();
    app.update();
    assert!(
        app.world().get::<Checked>(b).is_none(),
        "A -> B wrote back through the element binding"
    );

    // TemplatedParent TwoWay: toggling the template part checks the parent.
    let part = {
        let parts = app
            .world()
            .get::<bevy_pf::components::PfTemplateParts>(t)
            .expect("template parts");
        parts.get("part").expect("inner checkbox")
    };
    app.world_mut().entity_mut(part).insert(Checked);
    app.update();
    assert!(
        app.world().get::<Checked>(t).is_some(),
        "template part checked the templated parent"
    );
}

#[derive(Reflect, Default)]
struct PaintVm {
    fill: String,
    stroke: String,
}

/// `BorderBrush="{Binding ...}"` — the counterpart of the `Background` binding.
///
/// Motivating case: a list where each row's stroke encodes that row's status, so
/// one `DataTemplate` renders a ruined item in oxide and a finished one in
/// green. Without this the row template needs a `DataTrigger` per status, which
/// does not scale past a handful and cannot express a colour computed at
/// runtime.
#[test]
fn border_brush_binds_and_updates() {
    let mut app = test_app();
    let vm = Bindable::new(PaintVm {
        fill: "#FF101820".into(),
        stroke: "#FFB4451A".into(),
    });
    let root = spawn_bound_scene(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <Border x:Name="Plate" Width="40" Height="20" BorderThickness="2"
                     Background="{Binding fill}" BorderBrush="{Binding stroke}"/>
           </StackPanel>"##,
        vm.clone(),
    );
    app.update();

    let plate = app
        .world()
        .get::<XamlNames>(root)
        .unwrap()
        .get("Plate")
        .unwrap();
    let border =
        |app: &App| -> Color { app.world().get::<bevy::ui::BorderColor>(plate).unwrap().top };
    let background = |app: &App| -> Color {
        app.world()
            .get::<bevy::ui::BackgroundColor>(plate)
            .unwrap()
            .0
    };

    assert_eq!(border(&app), Color::srgba_u8(0xB4, 0x45, 0x1A, 0xFF));
    assert_eq!(background(&app), Color::srgba_u8(0x10, 0x18, 0x20, 0xFF));

    // A status change repaints the stroke, which is the whole point.
    vm.update(|m: &mut PaintVm| m.stroke = "#FF2BF59B".into());
    app.update();
    assert_eq!(border(&app), Color::srgba_u8(0x2B, 0xF5, 0x9B, 0xFF));

    // All four sides, like WPF: BorderThickness varies per side, BorderBrush
    // does not.
    let sides = app.world().get::<bevy::ui::BorderColor>(plate).unwrap();
    assert_eq!(sides.top, sides.bottom);
    assert_eq!(sides.left, sides.right);
    assert_eq!(sides.top, sides.left);
}

/// An unparseable colour leaves the existing stroke alone rather than falling
/// back to a default: a transient bad value in a view-model should not flash the
/// UI to black.
#[test]
fn a_malformed_border_brush_leaves_the_previous_colour() {
    let mut app = test_app();
    let vm = Bindable::new(PaintVm {
        fill: "#FF000000".into(),
        stroke: "#FFB4451A".into(),
    });
    let root = spawn_bound_scene(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <Border x:Name="Plate" Width="40" Height="20" BorderThickness="2"
                     BorderBrush="{Binding stroke}"/>
           </StackPanel>"##,
        vm.clone(),
    );
    app.update();
    let plate = app
        .world()
        .get::<XamlNames>(root)
        .unwrap()
        .get("Plate")
        .unwrap();
    assert_eq!(
        app.world().get::<bevy::ui::BorderColor>(plate).unwrap().top,
        Color::srgba_u8(0xB4, 0x45, 0x1A, 0xFF)
    );

    vm.update(|m: &mut PaintVm| m.stroke = "not a colour".into());
    app.update();
    assert_eq!(
        app.world().get::<bevy::ui::BorderColor>(plate).unwrap().top,
        Color::srgba_u8(0xB4, 0x45, 0x1A, 0xFF),
        "a bad value must not flash the stroke to a default"
    );
}

/// `Stroke` / `Fill` bind on shapes — the counterpart of `BorderBrush` /
/// `Background` for chamfered plates and code cells, which are `Path`s rather
/// than `Border`s because a Border cannot express a cut corner.
///
/// The subtle half is repainting: shapes rasterize to an image cached on their
/// pixel size, so a colour change at the SAME size has to invalidate that cache
/// or the old colour silently persists.
#[test]
fn shape_stroke_and_fill_bind_and_repaint_at_the_same_size() {
    let mut app = test_app();
    let vm = Bindable::new(PaintVm {
        fill: "#FF101820".into(),
        stroke: "#FFB4451A".into(),
    });
    let root = spawn_bound_scene(
        &mut app,
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <Path x:Name="Cell" Width="40" Height="40" Stretch="Fill"
                   StrokeThickness="1"
                   Fill="{Binding fill}" Stroke="{Binding stroke}"
                   Data="M 0,0 L 32,0 L 40,8 L 40,40 L 0,40 Z"/>
           </StackPanel>"##,
        vm.clone(),
    );
    app.update();

    let cell = app
        .world()
        .get::<XamlNames>(root)
        .unwrap()
        .get("Cell")
        .unwrap();
    fn solid(brush: Option<&bevy_pf_xaml::value::PfBrush>) -> (u8, u8, u8) {
        match brush {
            Some(bevy_pf_xaml::value::PfBrush::Solid(c)) => (c.r, c.g, c.b),
            other => panic!("expected a solid brush, got {other:?}"),
        }
    }
    let paint = |app: &App| -> ((u8, u8, u8), (u8, u8, u8)) {
        let shape = app.world().get::<bevy_pf::shapes::PfShape>(cell).unwrap();
        (solid(shape.stroke.as_ref()), solid(shape.fill.as_ref()))
    };

    let (stroke, fill) = paint(&app);
    assert_eq!(stroke, (0xB4, 0x45, 0x1A), "oxide stroke bound");
    assert_eq!(fill, (0x10, 0x18, 0x20), "dark fill bound");

    // A status change repaints. The shape keeps its size, so this is exactly
    // the case a size-keyed raster cache would miss.
    vm.update(|m: &mut PaintVm| m.stroke = "#FF2BF59B".into());
    app.update();
    let (stroke, _) = paint(&app);
    assert_eq!(
        stroke,
        (0x2B, 0xF5, 0x9B),
        "a same-size colour change must actually repaint"
    );
}

#[derive(Reflect, Default)]
struct ProfileVm {
    user: UserVm,
    title: String,
}

#[derive(Reflect, Default)]
struct UserVm {
    name: String,
    stats: StatsVm,
}

#[derive(Reflect, Default)]
struct StatsVm {
    wins: u32,
}

#[test]
fn datacontext_attribute_rescopes_descendants() {
    let mut app = test_app();
    let vm = Bindable::new(ProfileVm {
        user: UserVm {
            name: "ada".into(),
            ..Default::default()
        },
        title: "profile".into(),
    });
    let root = spawn_bound_scene(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <TextBlock x:Name="Title" Text="{Binding title}"/>
             <StackPanel DataContext="{Binding user}">
               <TextBlock x:Name="Name" Text="{Binding name}"/>
             </StackPanel>
           </StackPanel>"#,
        vm.clone(),
    );
    app.world_mut()
        .entity_mut(root)
        .insert(DataContext(vm.clone()));
    app.update();

    let names = app.world().get::<XamlNames>(root).unwrap();
    let (title, name) = (names.get("Title").unwrap(), names.get("Name").unwrap());
    // Sibling outside the scope still sees the root context.
    assert_eq!(app.world().get::<Text>(title).unwrap().0, "profile");
    // Descendant of the scoped panel resolves against `user`.
    assert_eq!(app.world().get::<Text>(name).unwrap().0, "ada");

    // Change propagation flows through the scoped path.
    vm.update(|m: &mut ProfileVm| m.user.name = "grace".into());
    app.update();
    assert_eq!(app.world().get::<Text>(name).unwrap().0, "grace");
}

#[test]
fn datacontext_scopes_nest_and_apply_on_the_same_element() {
    let mut app = test_app();
    let vm = Bindable::new(ProfileVm {
        user: UserVm {
            stats: StatsVm { wins: 7 },
            ..Default::default()
        },
        ..Default::default()
    });
    let root = spawn_bound_scene(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <StackPanel DataContext="{Binding user}">
               <StackPanel DataContext="{Binding stats}">
                 <TextBlock x:Name="Wins" Text="{Binding wins}"/>
               </StackPanel>
               <TextBlock x:Name="SelfScoped" DataContext="{Binding stats}"
                          Text="{Binding wins}"/>
             </StackPanel>
           </StackPanel>"#,
        vm.clone(),
    );
    app.world_mut()
        .entity_mut(root)
        .insert(DataContext(vm.clone()));
    app.update();

    let names = app.world().get::<XamlNames>(root).unwrap();
    let wins = names.get("Wins").unwrap();
    let self_scoped = names.get("SelfScoped").unwrap();
    // Two nested scopes compose: root -> user -> stats.
    assert_eq!(app.world().get::<Text>(wins).unwrap().0, "7");
    // A scope on the binding's own element applies to that element too.
    assert_eq!(app.world().get::<Text>(self_scoped).unwrap().0, "7");
}

#[derive(Reflect, Default)]
struct StoreVm {
    gap: String,
    fade: f64,
}

#[test]
fn store_managed_properties_bind_through_the_precedence_store() {
    let mut app = test_app();
    let vm = Bindable::new(StoreVm {
        gap: "4".into(),
        fade: 0.5,
    });
    let root = spawn_bound_scene(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <Border x:Name="Box" Margin="{Binding gap}" Opacity="{Binding fade}"/>
           </StackPanel>"#,
        vm.clone(),
    );
    app.world_mut()
        .entity_mut(root)
        .insert(DataContext(vm.clone()));
    app.update();

    let node = app
        .world()
        .get::<XamlNames>(root)
        .unwrap()
        .get("Box")
        .unwrap();
    let margin = app.world().get::<Node>(node).unwrap().margin;
    assert_eq!(margin, UiRect::all(Val::Px(4.0)));
    let opacity = app
        .world()
        .get::<bevy_pf::provider::PfOpacity>(node)
        .unwrap();
    assert_eq!(opacity.value, 0.5);

    vm.update(|m: &mut StoreVm| {
        m.gap = "12,2,12,2".into();
        m.fade = 1.0;
    });
    app.update();
    let margin = app.world().get::<Node>(node).unwrap().margin;
    assert_eq!(
        margin,
        UiRect {
            left: Val::Px(12.0),
            top: Val::Px(2.0),
            right: Val::Px(12.0),
            bottom: Val::Px(2.0),
        }
    );
    let opacity = app
        .world()
        .get::<bevy_pf::provider::PfOpacity>(node)
        .unwrap();
    assert_eq!(opacity.value, 1.0);
}

#[derive(Reflect, Default, bevy_pf::Bindable)]
struct NotifyVm {
    score: u32,
    status: String,
}

#[test]
fn named_updates_invalidate_only_overlapping_bindings() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // A pass-through converter that counts how often the `score` binding
    // re-applies — the observable for selective invalidation.
    struct Counting(Arc<AtomicUsize>);
    impl bevy_pf::binding::PfValueConverter for Counting {
        fn convert(
            &self,
            value: &bevy_pf::BoundValue,
            _parameter: Option<&str>,
        ) -> Option<bevy_pf::BoundValue> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Some(value.clone())
        }
    }

    let mut app = test_app();
    let count = Arc::new(AtomicUsize::new(0));
    app.register_converter("Counting", Counting(count.clone()));

    let vm = Bindable::new(NotifyVm::default());
    let root = spawn_bound_scene(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <TextBlock x:Name="Score" Text="{Binding score, Converter=Counting}"/>
             <TextBlock x:Name="Status" Text="{Binding status}"/>
           </StackPanel>"#,
        vm.clone(),
    );
    app.world_mut()
        .entity_mut(root)
        .insert(DataContext(vm.clone()));
    app.update();
    let initial = count.load(Ordering::SeqCst);
    assert!(initial >= 1);

    // A named update to `status` must not re-apply the `score` binding.
    vm.update_named::<NotifyVm>("status", |m| {
        m.status = "ready".into();
        true
    });
    app.update();
    let names = app.world().get::<XamlNames>(root).unwrap();
    let status = names.get("Status").unwrap();
    assert_eq!(app.world().get::<Text>(status).unwrap().0, "ready");
    assert_eq!(count.load(Ordering::SeqCst), initial);

    // An unnamed (whole-model) update still re-applies everything.
    vm.update(|m: &mut NotifyVm| m.score = 3);
    app.update();
    assert!(count.load(Ordering::SeqCst) > initial);
}

#[test]
fn derived_setters_notify_selectively_and_skip_equal_values() {
    let mut app = test_app();
    let vm = Bindable::new(NotifyVm::default());
    let root = spawn_bound_scene(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <TextBlock x:Name="Score" Text="{Binding score}"/>
           </StackPanel>"#,
        vm.clone(),
    );
    app.world_mut()
        .entity_mut(root)
        .insert(DataContext(vm.clone()));
    app.update();

    // Generated setter: writes, notifies, propagates.
    assert!(vm.set_score(42));
    app.update();
    let score = app
        .world()
        .get::<XamlNames>(root)
        .unwrap()
        .get("Score")
        .unwrap();
    assert_eq!(app.world().get::<Text>(score).unwrap().0, "42");

    // Equal value: no write, no version bump.
    let v = vm.version();
    assert!(!vm.set_score(42));
    assert_eq!(vm.version(), v);

    assert!(vm.set_status("live".into()));
    assert_eq!(
        vm.read_path("status"),
        Some(bevy_pf::BoundValue::Str("live".into()))
    );
}

/// A bound `IsEnabled` has to work in BOTH directions.
///
/// The regression this pins: `IsEnabled` was applied once while the tree
/// was built and only ever INSERTED `InteractionDisabled`. A control bound
/// to a live value therefore went dead the first time the value was false
/// and stayed dead forever — the exact shape of a Fire button that is
/// disabled while out of ammunition and never comes back when the rack
/// refills.
#[test]
fn isenabled_binding_disables_and_re_enables() {
    let mut app = test_app();
    let vm = Bindable::new(GameVm::default()); // ready: false
    let root = spawn_bound_scene(
        &mut app,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <Button x:Name="Fire" Content="FIRE" IsEnabled="{Binding ready}"/>
           </StackPanel>"#,
        vm.clone(),
    );
    app.world_mut()
        .entity_mut(root)
        .insert(DataContext(vm.clone()));
    app.update();

    let btn = app
        .world()
        .get::<XamlNames>(root)
        .unwrap()
        .get("Fire")
        .unwrap();

    // ready = false -> disabled.
    assert!(
        app.world()
            .get::<bevy::ui::InteractionDisabled>(btn)
            .is_some(),
        "a false IsEnabled must disable the control"
    );

    // ready = true -> the marker must come back OFF. This is the half
    // that did not exist.
    vm.update(|m: &mut GameVm| m.ready = true);
    app.update();
    assert!(
        app.world()
            .get::<bevy::ui::InteractionDisabled>(btn)
            .is_none(),
        "a true IsEnabled must re-enable the control"
    );

    // And back again, so the binding is not one-shot in either direction.
    vm.update(|m: &mut GameVm| m.ready = false);
    app.update();
    assert!(
        app.world()
            .get::<bevy::ui::InteractionDisabled>(btn)
            .is_some(),
        "IsEnabled must keep tracking the value, not latch"
    );
}
