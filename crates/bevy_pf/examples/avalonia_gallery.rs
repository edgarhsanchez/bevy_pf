//! Real Avalonia XAML, rendered by bevy_pf.
//!
//!     cargo run -p bevy_pf --example avalonia_gallery
//!
//! The left column is a live selector-styling demo written in Avalonia's own
//! dialect — its namespace, a `Styles` collection with selectors, `Classes`,
//! `Spacing`, and pseudo-classes. Hover and press the buttons: that is
//! `:pointerover` and `:pressed`, reverting structurally when you let go.
//!
//! The right column is a file from the vendored AvaloniaUI/Avalonia.Samples
//! suite, loaded BYTE FOR BYTE with no edits.
//!
//! What this cannot show: the sample files bind to view models written in
//! C#, so `{Binding SomeProperty}` renders empty — there is no object to
//! bind to. Structure, styling and literal content are what you see.

use bevy::prelude::*;
use bevy::reflect::Reflect;
use bevy_pf::prelude::*;

/// The sample binds `{Binding SelectedFiles}` to a view model written in C#.
/// There is no C# here — but the binding does not care what language the
/// object came from, only that the path resolves. A Rust struct of the same
/// SHAPE makes the file render with real rows, which is the honest way to
/// demo it: the XAML is still untouched.
#[derive(Reflect, Default)]
#[allow(non_snake_case)]
struct CustomInteractionViewModel {
    SelectedFiles: Vec<String>,
}

/// A sample that loads with NO warnings, so what renders is what the file says.
const SAMPLE_PATH: &str =
    "ViewInteraction/MvvmDialogSample/Views/CustomInteractionView.axaml";
const SAMPLE: &str = include_str!(
    "avalonia_samples/ViewInteraction/MvvmDialogSample/Views/CustomInteractionView.axaml"
);

fn primary_window() -> Window {
    #[allow(unused_mut)]
    let mut window = Window {
        title: "bevy_pf — Avalonia XAML".to_string(),
        ..Default::default()
    };
    #[cfg(target_arch = "wasm32")]
    {
        window.canvas = Some("#bevy-canvas".to_string());
        window.fit_canvas_to_parent = true;
    }
    window
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(primary_window()),
            ..Default::default()
        }))
        .add_plugins(PfUiPlugin)
        .add_systems(Startup, setup)
        .run();
}

/// The shell: Avalonia's namespace, a `Styles` collection of selectors, and
/// a named panel the real sample file is instantiated into.
fn scene() -> String {
    r##"<Border xmlns="https://github.com/avaloniaui"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        Background="#0A1218" Padding="16">
  <Border.Styles>
    <!-- A bare type selector. No activator, so it sits at the style tier. -->
    <Style Selector="Border.card">
      <Setter Property="Background" Value="#12202B"/>
      <Setter Property="Padding" Value="14"/>
      <Setter Property="CornerRadius" Value="6"/>
      <Setter Property="BorderBrush" Value="#1E3646"/>
      <Setter Property="BorderThickness" Value="1"/>
    </Style>

    <!-- A class selector IS activated, so it outranks the plain rule above
         regardless of which was declared first. -->
    <Style Selector="Border.accent">
      <Setter Property="Background" Value="#0E3A4A"/>
      <Setter Property="BorderBrush" Value="#00A6C4"/>
    </Style>

    <!-- Pseudo-classes compile to the trigger runtime that already existed
         for WPF, so they revert structurally. -->
    <Style Selector="Button">
      <Setter Property="Background" Value="#243447"/>
      <Setter Property="Foreground" Value="#E8F4F8"/>
      <Setter Property="Padding" Value="16,9"/>
      <Setter Property="BorderBrush" Value="#31506A"/>
      <Setter Property="BorderThickness" Value="1"/>
    </Style>
    <Style Selector="Button:pointerover">
      <Setter Property="Background" Value="#00A6C4"/>
      <Setter Property="Foreground" Value="#06121A"/>
    </Style>
    <Style Selector="Button:pressed">
      <Setter Property="Background" Value="#00FFD4"/>
    </Style>
    <Style Selector="Button.danger:pointerover">
      <Setter Property="Background" Value="#C4453A"/>
      <Setter Property="Foreground" Value="#FFFFFF"/>
    </Style>
  </Border.Styles>

  <StackPanel Orientation="Horizontal" Spacing="16">

    <StackPanel Spacing="10" Width="460">
      <TextBlock Text="Avalonia selectors, live"
                 FontSize="22" FontWeight="Bold" Foreground="#E8F4F8"/>
      <TextBlock Text="Written in Avalonia's dialect: its xmlns, a Styles collection, Classes and pseudo-classes."
                 TextWrapping="Wrap" FontSize="12" Foreground="#7E97AB"/>

      <Border Classes="card">
        <TextBlock Text="Border.card  —  plain type+class selector"
                   Foreground="#C7D6E2" FontSize="13"/>
      </Border>

      <Border Classes="card accent">
        <TextBlock Text="Border.card.accent  —  the activated bucket wins"
                   Foreground="#C7D6E2" FontSize="13"/>
      </Border>

      <TextBlock Text="Hover and press these:" Foreground="#7E97AB" FontSize="12" Margin="0,6,0,0"/>
      <StackPanel Orientation="Horizontal" Spacing="8">
        <Button Content="Hover me"/>
        <Button Content="Press me"/>
        <Button Content="Danger" Classes="danger"/>
      </StackPanel>

      <TextBlock Text="&#8226; :pointerover and :pressed are bevy_pf's existing IsMouseOver / IsPressed conditions&#10;&#8226; the class rule beats the plain one no matter the declaration order&#10;&#8226; releasing reverts structurally, not to a remembered value"
                 TextWrapping="Wrap" FontSize="11" Foreground="#5E7386" Margin="0,8,0,0"/>
    </StackPanel>

    <StackPanel Spacing="10" Width="520">
      <TextBlock Text="A real sample, unedited"
                 FontSize="22" FontWeight="Bold" Foreground="#E8F4F8"/>
      <TextBlock Text="PATHGOESHERE"
                 TextWrapping="Wrap" FontSize="11" Foreground="#7E97AB"/>
      <Border x:Name="SampleHost" Classes="card"/>
    </StackPanel>

  </StackPanel>
</Border>"##
        .replace("PATHGOESHERE", SAMPLE_PATH)
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    // The dark theme now also drives the CODE-DRAWN control chrome (list
    // rows, combo faces, check boxes), which used to stay WPF-light.
    commands.queue(|world: &mut World| {
        bevy_pf::themes::apply_theme(world, "tokyo-night").expect("built-in theme applies");
    });

    let scene = XamlScene::parse(scene()).expect("gallery scene parses");
    let root = commands.spawn_xaml(scene);

    // Instantiate the untouched sample into the named host, so the file on
    // disk is what renders rather than a transcription of it.
    commands.queue(move |world: &mut World| {
        let Some(host) = world
            .get::<XamlNames>(root)
            .and_then(|names| names.get("SampleHost"))
        else {
            error!("gallery: SampleHost not found");
            return;
        };
        let doc = match bevy_pf_xaml::parse(SAMPLE) {
            Ok(doc) => doc,
            Err(e) => {
                error!("gallery: the sample did not parse: {e}");
                return;
            }
        };
        // Give the sample a data context of the shape its bindings expect.
        let vm = Bindable::new(CustomInteractionViewModel {
            SelectedFiles: vec![
                "notes/roadmap.md".into(),
                "src/main.rs".into(),
                "assets/logo.png".into(),
                "Cargo.toml".into(),
            ],
        });
        world.entity_mut(host).insert(DataContext(vm));

        match bevy_pf::instantiate_document_env(world, host, &doc, &bevy_pf::XamlEnv::default()) {
            Ok(result) => {
                for w in &result.warnings {
                    warn!("gallery sample: {w}");
                }
                info!("gallery: {SAMPLE_PATH} loaded with {} warnings", result.warnings.len());
            }
            Err(e) => error!("gallery: {e}"),
        }
    });
}
