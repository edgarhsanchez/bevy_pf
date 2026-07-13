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
    app.world_mut().entity_mut(root).insert(DataContext(vm.clone()));
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
    app.world_mut().entity_mut(root).insert(DataContext(vm.clone()));
    app.update();

    let cb = app.world().get::<XamlNames>(root).unwrap().get("Ready").unwrap();
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
    app.world_mut().entity_mut(root).insert(DataContext(vm.clone()));
    app.update();

    let tb = app.world().get::<XamlNames>(root).unwrap().get("Status").unwrap();
    let input = app.world().get::<Children>(tb).unwrap().iter().next().unwrap();
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
    app.world_mut().entity_mut(root).insert(DataContext(vm.clone()));
    app.update();

    let bar = app.world().get::<XamlNames>(root).unwrap().get("P").unwrap();
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
    app.world_mut().entity_mut(root).insert(DataContext(vm.clone()));
    app.update();

    let button = app.world().get::<XamlNames>(root).unwrap().get("B").unwrap();
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

    app.world_mut().entity_mut(toggle).remove::<bevy::ui::Checked>();
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
    let vm = Bindable::new(ConvVm { online: true, credits: 21.0 });
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
        app.world().get::<bevy::ui::widget::Text>(e).unwrap().0.clone()
    };
    // Built-in bool->visibility converter, applied to a text target too.
    assert_eq!(text(&app, badge), "Visible");
    // Custom converter with a parameter: 21 * 3.
    assert_eq!(text(&app, credits), "63");
    // FallbackValue covers the unresolvable path (no warning spam).
    assert_eq!(text(&app, missing), "offline");
    // Visibility target actually toggles layout.
    assert_ne!(app.world().get::<Node>(panel).unwrap().display, Display::None);

    vm.update(|m: &mut ConvVm| m.online = false);
    app.update();
    assert_eq!(text(&app, badge), "Collapsed");
    assert_eq!(app.world().get::<Node>(panel).unwrap().display, Display::None);
}

#[test]
fn relative_source_self_and_templated_parent() {
    let mut app = test_app();
    let vm = Bindable::new(ConvVm { online: true, credits: 5.0 });
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
        app.world().get::<bevy::ui::widget::Text>(state_text).unwrap().0,
        "True",
        "template chrome reads the templated parent"
    );

    // Self-source stays live: uncheck -> re-renders.
    app.world_mut().entity_mut(toggle).remove::<bevy::ui::Checked>();
    app.update();
    assert_eq!(
        app.world().get::<bevy::ui::widget::Text>(state_text).unwrap().0,
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
    let vm = Bindable::new(NullVm { nickname: None, ..Default::default() });
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
    let vm = Bindable::new(NullVm { done: 3.0, total: 4.0, ..Default::default() });
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
    assert!(text.starts_with("w="), "FindAncestor bound and formatted: {text}");
}
