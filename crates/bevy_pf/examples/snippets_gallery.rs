//! Control recipes: every card renders a live control NEXT TO the exact
//! XAML that produced it. The markup string is instantiated and displayed
//! from the same constant, so the code you read is the code that runs.
//!
//! Run with: `cargo run -p bevy_pf --example snippets_gallery`
//! On the web: site/snippets.html

use bevy::prelude::*;
use bevy::reflect::Reflect;
use bevy_pf::prelude::*;
use bevy_pf::{XamlEnv, instantiate_document_env};

#[derive(Reflect, Default, Clone)]
struct Profile {
    name: String,
    role: String,
}

#[derive(Reflect, Default)]
struct Vm {
    name: String,
    volume: f64,
    agreed: bool,
    pick: f64,
    count: f64,
    items: Vec<String>,
    profile: Profile,
}

struct Recipe {
    title: &'static str,
    blurb: &'static str,
    xaml: &'static str,
    rust: Option<&'static str>,
}

const RECIPES: &[Recipe] = &[
    Recipe {
        title: "Button",
        blurb: "Command= names a closure registered on the view-model; the count flows back through a normal binding.",
        xaml: r##"<StackPanel Spacing="6">
  <Button Content="Click me" Command="bump" Width="140"/>
  <TextBlock Text="{Binding count, StringFormat='{}clicked {0} times'}"/>
</StackPanel>"##,
        rust: Some(
            r##"let vm = Bindable::new(Vm::default());
vm.on_command("bump", {
    let vm = vm.clone();
    move |_world, _param| { vm.update(|m: &mut Vm| m.count += 1.0); }
});"##,
        ),
    },
    Recipe {
        title: "RepeatButton",
        blurb: "Fires Click repeatedly while held: first after Delay ms, then every Interval ms. Same command as above — hold it down.",
        xaml: r##"<StackPanel Spacing="6">
  <RepeatButton Content="Hold me" Delay="400" Interval="60"
                Command="bump" Width="140"/>
  <TextBlock Text="{Binding count, StringFormat='{}count: {0}'}"/>
</StackPanel>"##,
        rust: None,
    },
    Recipe {
        title: "Slider → ProgressBar",
        blurb: "Slider.Value binds TwoWay by default; the ProgressBar and readout watch the same field. Drag the slider.",
        xaml: r##"<StackPanel Spacing="8">
  <Slider Minimum="0" Maximum="100" Value="{Binding volume}" Width="220"/>
  <ProgressBar Minimum="0" Maximum="100" Value="{Binding volume}"
               Width="220" Height="12"/>
  <TextBlock Text="{Binding volume, StringFormat='{}volume: {0:F0}%'}"/>
</StackPanel>"##,
        rust: None,
    },
    Recipe {
        title: "TextBox",
        blurb: "Text edits write back into the view-model as you type; anything bound to the same path updates live.",
        xaml: r##"<StackPanel Spacing="6">
  <TextBox Text="{Binding name}" Width="220"/>
  <TextBlock Text="{Binding name, StringFormat='{}Hello, {0}!'}"/>
</StackPanel>"##,
        rust: None,
    },
    Recipe {
        title: "PasswordBox",
        blurb: "A TextBox that masks input with • as you type. The real password stays out of the visual tree.",
        xaml: r##"<PasswordBox Width="220"/>"##,
        rust: None,
    },
    Recipe {
        title: "CheckBox + ToggleButton",
        blurb: "IsChecked binds TwoWay; both controls and the readout share one boolean.",
        xaml: r##"<StackPanel Spacing="6">
  <CheckBox Content="I agree" IsChecked="{Binding agreed}"/>
  <ToggleButton Content="Same flag" IsChecked="{Binding agreed}"/>
  <TextBlock Text="{Binding agreed, StringFormat='{}agreed: {0}'}"/>
</StackPanel>"##,
        rust: None,
    },
    Recipe {
        title: "RadioButton",
        blurb: "GroupName scopes exclusivity — pick one of three.",
        xaml: r##"<StackPanel Spacing="4">
  <RadioButton GroupName="quality" Content="Low"/>
  <RadioButton GroupName="quality" Content="Medium" IsChecked="True"/>
  <RadioButton GroupName="quality" Content="High"/>
</StackPanel>"##,
        rust: None,
    },
    Recipe {
        title: "ComboBox",
        blurb: "SelectedIndex binds TwoWay — the readout follows your pick.",
        xaml: r##"<StackPanel Spacing="6">
  <ComboBox SelectedIndex="{Binding pick}" Width="180">
    <ComboBoxItem Content="Ada"/>
    <ComboBoxItem Content="Grace"/>
    <ComboBoxItem Content="Edsger"/>
  </ComboBox>
  <TextBlock Text="{Binding pick, StringFormat='{}selected index: {0}'}"/>
</StackPanel>"##,
        rust: None,
    },
    Recipe {
        title: "ListBox + ItemsSource",
        blurb: "Items generate from a Vec on the view-model; ItemsPanel swaps the layout to a WrapPanel and ItemContainerStyle restyles every generated item.",
        xaml: r##"<ListBox ItemsSource="{Binding items}" Width="230">
  <ListBox.ItemsPanel>
    <ItemsPanelTemplate>
      <WrapPanel/>
    </ItemsPanelTemplate>
  </ListBox.ItemsPanel>
  <ListBox.ItemContainerStyle>
    <Style TargetType="ListBoxItem">
      <Setter Property="Padding" Value="8,4"/>
      <Setter Property="Margin" Value="2"/>
    </Style>
  </ListBox.ItemContainerStyle>
</ListBox>"##,
        rust: Some(
            r##"let vm = Bindable::new(Vm {
    items: vec!["Ada".into(), "Grace".into(), "Edsger".into(),
                "Barbara".into(), "Tony".into()],
    ..Default::default()
});"##,
        ),
    },
    Recipe {
        title: "ContentControl + DataTemplate",
        blurb: "Content binds to a struct; the DataTemplate is selected by the value's type (DataType=), WPF's implicit-template rule.",
        xaml: r##"<StackPanel Spacing="6">
  <StackPanel.Resources>
    <DataTemplate DataType="{x:Type Profile}">
      <Border Background="#26303F" CornerRadius="6" Padding="10,6">
        <StackPanel>
          <TextBlock Text="{Binding name}" FontWeight="Bold"
                     Foreground="#EDF2FA"/>
          <TextBlock Text="{Binding role}" FontSize="11"
                     Foreground="#9FB4CE"/>
        </StackPanel>
      </Border>
    </DataTemplate>
  </StackPanel.Resources>
  <ContentControl Content="{Binding profile}"/>
</StackPanel>"##,
        rust: None,
    },
    Recipe {
        title: "Style + Trigger",
        blurb: "A keyed style with an IsMouseOver trigger — hover the button. Deactivation reverts structurally; no code.",
        xaml: r##"<StackPanel>
  <StackPanel.Resources>
    <Style x:Key="Hot" TargetType="Button">
      <Setter Property="Background" Value="#3C4658"/>
      <Setter Property="Foreground" Value="#EDF2FA"/>
      <Setter Property="Padding" Value="14,8"/>
      <Style.Triggers>
        <Trigger Property="IsMouseOver" Value="True">
          <Setter Property="Background" Value="#5B8DEF"/>
        </Trigger>
      </Style.Triggers>
    </Style>
  </StackPanel.Resources>
  <Button Content="Hover me" Style="{StaticResource Hot}" Width="140"/>
</StackPanel>"##,
        rust: None,
    },
    Recipe {
        title: "Storyboard",
        blurb: "Trigger.EnterActions / ExitActions fade the card in and out on hover — a real WPF storyboard, no code-behind.",
        xaml: r##"<StackPanel>
  <StackPanel.Resources>
    <Style x:Key="Fade" TargetType="Border">
      <Setter Property="Opacity" Value="0.35"/>
      <Style.Triggers>
        <Trigger Property="IsMouseOver" Value="True">
          <Trigger.EnterActions>
            <BeginStoryboard>
              <Storyboard>
                <DoubleAnimation Storyboard.TargetProperty="Opacity"
                                 To="1" Duration="0:0:0.25"/>
              </Storyboard>
            </BeginStoryboard>
          </Trigger.EnterActions>
          <Trigger.ExitActions>
            <BeginStoryboard>
              <Storyboard>
                <DoubleAnimation Storyboard.TargetProperty="Opacity"
                                 To="0.35" Duration="0:0:0.4"/>
              </Storyboard>
            </BeginStoryboard>
          </Trigger.ExitActions>
        </Trigger>
      </Style.Triggers>
    </Style>
  </StackPanel.Resources>
  <Border Style="{StaticResource Fade}" Background="#5B8DEF"
          CornerRadius="8" Padding="16" Width="200">
    <TextBlock Text="hover to fade in" Foreground="White"/>
  </Border>
</StackPanel>"##,
        rust: None,
    },
    Recipe {
        title: "Shapes",
        blurb: "Vector graphics with the WPF path mini-language — rendered by the same engine on native and wasm.",
        xaml: r##"<Canvas Width="240" Height="90">
  <Ellipse Canvas.Left="6" Canvas.Top="12" Width="64" Height="64"
           Fill="#5B8DEF"/>
  <Rectangle Canvas.Left="86" Canvas.Top="12" Width="64" Height="64"
             Fill="#35E0B8" RadiusX="10" RadiusY="10"/>
  <Path Canvas.Left="166" Canvas.Top="12" Width="64" Height="64"
        Stretch="Fill" Fill="#EF6A5B"
        Data="M 50,5 61,35 95,35 67,55 78,90 50,68 22,90 33,55 5,35 39,35 Z"/>
</Canvas>"##,
        rust: None,
    },
    Recipe {
        title: "Expander + ToolTip",
        blurb: "Disclosure and hover hints, straight from markup.",
        xaml: r##"<Expander Header="Details" IsExpanded="True" Width="230">
  <StackPanel Spacing="4">
    <TextBlock Text="Anything can live in here."/>
    <Button Content="With a tooltip" Width="140"
            ToolTip="Hovered long enough!"/>
  </StackPanel>
</Expander>"##,
        rust: None,
    },
    Recipe {
        title: "ScrollBar",
        blurb: "Proportional thumb (ViewportSize), draggable, line buttons repeat while held; Value binds two-way — drag it and watch the readout.",
        xaml: r##"<StackPanel Orientation="Horizontal" Spacing="10">
  <ScrollBar Minimum="0" Maximum="100" ViewportSize="25"
             SmallChange="5" Value="{Binding volume}" Height="120"/>
  <TextBlock Text="{Binding volume, StringFormat='{}at {0:F0}'}"
             VerticalAlignment="Center"/>
</StackPanel>"##,
        rust: None,
    },
];

fn main() {
    let mut app = App::new();
    app.add_plugins({
        let plugins = DefaultPlugins.set(WindowPlugin {
            primary_window: Some(primary_window()),
            ..Default::default()
        });
        #[cfg(target_arch = "wasm32")]
        let plugins = plugins.disable::<bevy::audio::AudioPlugin>();
        plugins
    })
    .add_plugins(PfUiPlugin)
    .add_systems(Startup, setup);
    // SNIPPETS_SHOT=<path.png> captures a frame ~2s in (native verification).
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(path) = std::env::var("SNIPPETS_SHOT") {
        app.add_systems(
            Update,
            move |mut commands: Commands, time: Res<Time>, mut done: Local<bool>| {
                if !*done && time.elapsed_secs() > 2.0 {
                    *done = true;
                    commands
                        .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
                        .observe(bevy::render::view::screenshot::save_to_disk(path.clone()));
                }
            },
        );
    }
    app.run();
}

fn primary_window() -> Window {
    #[allow(unused_mut)]
    let mut window = Window {
        title: "bevy_pf control recipes".to_string(),
        ..Default::default()
    };
    #[cfg(target_arch = "wasm32")]
    {
        window.canvas = Some("#bevy-canvas".to_string());
        window.fit_canvas_to_parent = true;
    }
    window
}

/// Card chrome: title, blurb, then [live demo | markup] side by side.
/// `Code`/`RustCode` panes are filled from the same constants the demo
/// pane instantiates.
const CARD: &str = r##"<Border xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        Background="#1D2531" BorderBrush="#33415A" BorderThickness="1"
        CornerRadius="10" Padding="16" Margin="0,0,0,14">
  <StackPanel>
    <TextBlock x:Name="Title" FontSize="17" FontWeight="Bold" Foreground="#EDF2FA"/>
    <TextBlock x:Name="Blurb" FontSize="12" Foreground="#9FB4CE"
               TextWrapping="Wrap" Margin="0,3,0,10"/>
    <Grid ColumnDefinitions="340, 16, *">
      <Border Background="#F2F5FA" CornerRadius="8" Padding="14"
              VerticalAlignment="Top">
        <StackPanel x:Name="Demo"/>
      </Border>
      <StackPanel Grid.Column="2">
        <Border Background="#141A24" CornerRadius="8" Padding="12">
          <TextBlock x:Name="Code" FontSize="11.5" Foreground="#B8C7DE"/>
        </Border>
        <Border x:Name="RustHolder" Background="#141A24" CornerRadius="8"
                Padding="12" Margin="0,8,0,0" Visibility="Collapsed">
          <TextBlock x:Name="RustCode" FontSize="11.5" Foreground="#A9D6B8"/>
        </Border>
      </StackPanel>
    </Grid>
  </StackPanel>
</Border>"##;

fn setup(world: &mut World) {
    world.spawn(Camera2d);

    let vm = Bindable::new(Vm {
        name: "Bevy".into(),
        volume: 40.0,
        pick: 1.0,
        items: vec![
            "Ada".into(),
            "Grace".into(),
            "Edsger".into(),
            "Barbara".into(),
            "Tony".into(),
        ],
        profile: Profile {
            name: "Grace Hopper".into(),
            role: "Rear Admiral, compiler pioneer".into(),
        },
        ..Default::default()
    });
    {
        let counter = vm.clone();
        vm.on_command("bump", move |_world, _param| {
            counter.update(|m: &mut Vm| m.count += 1.0);
        });
    }

    // Page scaffold.
    let shell = bevy_pf_xaml::parse(
        r##"<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                    xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                    Title="bevy_pf control recipes" Background="#101722">
              <ScrollViewer>
                <StackPanel x:Name="Cards" Margin="22" MaxWidth="1060"
                            HorizontalAlignment="Center">
                  <TextBlock Text="Control recipes" FontSize="26" FontWeight="Bold"
                             Foreground="#EDF2FA"/>
                  <TextBlock Text="Each card runs the exact markup shown beside it — the demo and the code come from the same string."
                             FontSize="13" Foreground="#9FB4CE" Margin="0,4,0,16"/>
                </StackPanel>
              </ScrollViewer>
            </Window>"##,
    )
    .expect("shell parses");
    let root = world.spawn_empty().id();
    let result =
        instantiate_document_env(world, root, &shell, &XamlEnv::default()).expect("shell spawns");
    for w in &result.warnings {
        warn!("shell: {w}");
    }
    let cards_host = world.get::<XamlNames>(root).unwrap().get("Cards").unwrap();

    let card_doc = bevy_pf_xaml::parse(CARD).expect("card chrome parses");
    for recipe in RECIPES {
        // Card chrome.
        let card_root = world.spawn_empty().id();
        let card = match instantiate_document_env(world, card_root, &card_doc, &XamlEnv::default())
        {
            Ok(r) => {
                for w in &r.warnings {
                    warn!("card `{}`: {w}", recipe.title);
                }
                r.root
            }
            Err(e) => {
                warn!("card `{}` failed: {e}", recipe.title);
                continue;
            }
        };
        world.entity_mut(cards_host).add_children(&[card]);

        let (title_e, blurb_e, code_e, rust_holder, rust_code, demo) = {
            let names = world.get::<XamlNames>(card_root).unwrap();
            (
                names.get("Title"),
                names.get("Blurb"),
                names.get("Code"),
                names.get("RustHolder"),
                names.get("RustCode"),
                names.get("Demo"),
            )
        };
        let set_text = |world: &mut World, e: Option<Entity>, value: &str| {
            if let Some(e) = e {
                set_first_text(world, e, value);
            }
        };
        set_text(world, title_e, recipe.title);
        set_text(world, blurb_e, recipe.blurb);
        set_text(world, code_e, recipe.xaml);
        if let Some(rust) = recipe.rust {
            set_text(world, rust_code, rust);
            if let Some(holder) = rust_holder {
                if let Some(mut node) = world.get_mut::<Node>(holder) {
                    // Border chrome is a single-cell grid.
                    node.display = Display::Grid;
                }
                // Collapsed set Visibility::Hidden too — clear both.
                world.entity_mut(holder).insert(Visibility::Inherited);
            }
        }

        // Live demo: instantiate the SAME string the code pane displays.
        let Some(demo_host) = demo else {
            continue;
        };
        let demo_xaml = format!(
            r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                            xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">{}</StackPanel>"##,
            recipe.xaml
        );
        match bevy_pf_xaml::parse(&demo_xaml) {
            Ok(doc) => {
                let wrapper = world.spawn(Node::default()).id();
                world.entity_mut(wrapper).insert(DataContext(vm.clone()));
                match instantiate_document_env(world, wrapper, &doc, &XamlEnv::default()) {
                    Ok(r) => {
                        for w in &r.warnings {
                            warn!("demo `{}`: {w}", recipe.title);
                        }
                        world.entity_mut(demo_host).add_children(&[r.root]);
                    }
                    Err(e) => warn!("demo `{}` failed: {e}", recipe.title),
                }
            }
            Err(e) => warn!("demo `{}` parse failed: {e}", recipe.title),
        }
    }
}

/// Set the string of the first Text in the subtree (the element itself for
/// a TextBlock, a child for controls that wrap their text).
fn set_first_text(world: &mut World, entity: Entity, value: &str) {
    fn walk(world: &mut World, e: Entity, value: &str) -> bool {
        if let Some(mut t) = world.get_mut::<bevy::ui::widget::Text>(e) {
            t.0 = value.to_string();
            return true;
        }
        let children: Vec<Entity> = world
            .get::<Children>(e)
            .map(|c| c.iter().collect())
            .unwrap_or_default();
        children.into_iter().any(|c| walk(world, c, value))
    }
    walk(world, entity, value);
}
