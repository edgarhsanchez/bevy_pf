# bevy_pf

A XAML / WPF-like UI framework for [Bevy](https://bevyengine.org). Write UI in
XAML — inline via `xaml!{}` or in `.xaml` files — and instantiate it as
`bevy_ui` entities.

```toml
bevy_pf = "0.1"
```

```rust,ignore
use bevy::prelude::*;
use bevy_pf::prelude::*;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, PfUiPlugin))
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.spawn_xaml(xaml!(
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                       Margin="24" Spacing="8">
             <TextBlock Text="Hello from XAML" FontSize="28" FontWeight="Bold"/>
             <Button x:Name="Go" Content="Click me" Width="140"/>
           </StackPanel>"#
    ));
}
```

## What is here

WPF's panels (`Grid` with star/auto/pixel tracks, `StackPanel`, `WrapPanel`,
`Canvas`, `DockPanel`, `Border`, `ScrollViewer`), 40+ controls from `Button`
to `DataGrid`, `TreeView`, `ComboBox`, `Menu`/`ContextMenu` and `DatePicker`,
plus ecosystem equivalents (`ToggleSwitch`, `NumericUpDown`, `RatingBar`,
`NavigationView`, toasts, dialogs, and vector-drawn icons that need no icon
font).

Around them: resources and styles (`StaticResource`, `DynamicResource`,
implicit styles, `BasedOn`, merged dictionaries), `ControlTemplate`
re-templating with `{TemplateBinding}` and triggers, data binding against any
`#[derive(Reflect)]` view-model, storyboard animation, and WPF's verbatim
dependency-property precedence tiers.

Element identity survives the trip: `x:Name` becomes a queryable ECS
component, so ordinary Bevy systems and observers work on XAML-declared UI.

## Conformance

WPF is treated as the specification. `docs/wpf-conformance-notes.md` in the
[repository](https://github.com/edgarhsanchez/bevy_pf) audits behavior against
the `dotnet/wpf` reference source and records every accepted deviation with
its reason — including hit testing, where a null `Background` on a panel is
transparent to input exactly as in WPF.

## Features

- `clipboard` *(default)* — system clipboard for text inputs. Android
  consumers should take `default-features = false`; the underlying crate has
  no Android backend and fails to compile there.
- `hot_reload` — watch `.xaml` assets and re-instantiate on change.
- `vector_gpu` — route shape rendering through
  [`bevy_pf_vector`](https://crates.io/crates/bevy_pf_vector) (tessellate
  once, draw instanced) with the CPU path kept as fallback.
- `native_shapes` — draw `<Rectangle>`/`<Ellipse>` with `bevy_ui`'s own node
  rendering. Measured at parity, so opt-in.

## License

MIT OR Apache-2.0, at your option.
