# WPF/XAML compatibility target (researched July 2026)

What bevy_pf aims to be compatible with, and in what order. Condensed from a
full survey of WPF (.NET 9/10), WinUI 3, Avalonia 11, and MAUI.

## Status of the dialects

- **WPF** is maintained (shipped with every .NET release, MIT on GitHub) but
  its XAML dialect is frozen — ideal compatibility target. .NET 9 added the
  Fluent theme + `ThemeMode`; **.NET 10 added `RowDefinitions="Auto,*"`
  shorthand** (bevy_pf supports this).
- **WinUI 3**: `{x:Bind}` compiled bindings (OneTime default, function
  bindings), `{ThemeResource}`, `AdaptiveTrigger` + VSM instead of property
  triggers, no DataTrigger/MultiBinding.
- **Avalonia 11**: CSS-like style selectors + pseudo-classes instead of
  triggers, compiled bindings by default (`x:DataType`), `using:` namespaces,
  Grid shorthand, `.axaml`.
- Consensus "modern" features worth adopting: compiled/typed bindings, Grid
  shorthand, style classes, light/dark theme dictionaries, deferred loading.

## Implementation order (dependency chain)

DP system + type converters → XAML loader → layout contract → binding engine →
styles/templates/triggers → controls (mostly templates over ~15 primitives) →
animation clock → geometry/brush rendering.

Two spots where naive implementations break compat: **property value
precedence** and **template namescopes**.

## Property value precedence (highest → lowest)

1. Coercion  2. Animations  3. **Local value** (attrs, bindings)
4. TemplatedParent template  5. Implicit style (Style property only)
6. Style triggers  7. Template triggers  8. **Style setters**
9. Theme style  10. **Inheritance**  11. Metadata default.

bevy_pf today: style setters → local attrs (correct relative order); triggers
and templates to come.

## Layout contract

- Two-pass Measure/Arrange; `DesiredSize`; alignment resolved at Arrange:
  non-Stretch elements take desired size within their slot. Default alignment
  is Stretch both axes.
- Grid: star math after fixed+auto; spans; out-of-range indices clamp;
  `SharedSizeGroup` (later). Children with no attached props overlap in (0,0).
- StackPanel: infinite measure along stacking axis, ignores main-axis
  alignment. Canvas: absolute, Left beats Right; desired size 0×0.
- DockPanel: docks in child order, `LastChildFill` default true.
- Visibility: `Hidden` reserves space, `Collapsed` doesn't.

## Inherited properties

`DataContext`, `FontFamily/Size/Style/Weight/Stretch`, `Foreground`,
`FlowDirection`; `IsEnabled` propagates by coercion.

## Binding engine (planned)

Source resolution: `Source` > `ElementName` > `RelativeSource` > inherited
DataContext. Paths: dots, indexers, attached in parens `(Grid.Row)`, `/`
current-item. Modes OneWay/TwoWay/OneTime/OneWayToSource + per-DP defaults
(TextBox.Text = TwoWay + LostFocus). `INotifyPropertyChanged` analog: a
change-tracked viewmodel type. StringFormat, converters, FallbackValue,
TargetNullValue. ItemsSource through a collection-view (sort/filter/group)
eventually.

## Controls inventory (~top 30, by priority)

1. Done: TextBlock, Border, Button, Label, Image, StackPanel, Grid, WrapPanel,
   Canvas, ScrollViewer(basic), Window/Page/UserControl.
2. Next: CheckBox, RadioButton (GroupName), ToggleButton, TextBox
   (via Bevy EditableText), Slider, ProgressBar, DockPanel(faithful),
   UniformGrid, Expander, GroupBox, Separator, ItemsControl.
3. Then: ListBox/ListBoxItem (Selector), ComboBox, TabControl/TabItem,
   ToolTip, Menu/MenuItem/ContextMenu, TreeView, ListView/GridView, DataGrid,
   Popup, GridSplitter, RepeatButton, PasswordBox, Calendar/DatePicker.

## Styling / templating roadmap

- Done: ResourceDictionary scoping, Static/DynamicResource (static resolution),
  implicit styles, BasedOn, Setters (incl. attached).
- Next: Style.Triggers (property triggers on Interaction/state), EventTrigger
  actions, MergedDictionaries, `x:Shared`.
- Later: ControlTemplate + TemplateBinding + PART_ contract, DataTemplate (+
  implicit by DataType), VisualStateManager, theme dictionaries (Fluent-like
  light/dark), DynamicResource live re-resolution.

## Animations (later)

Timeline model (Duration/BeginTime/AutoReverse/RepeatBehavior/easings),
Storyboard with TargetName/TargetProperty property paths, From/To/By +
keyframe animations. Map to a clock resource + per-entity animation
components.

## Vector graphics (later)

Shapes: Rectangle (RadiusX/Y), Ellipse, Line, Polyline/Polygon, Path with the
SVG-compatible mini-language (`M/L/H/V/C/S/Q/T/A/Z`, `F0/F1` fill rule).
Brushes beyond gradients: ImageBrush (TileMode), DrawingBrush, VisualBrush.
Strategy: lyon tessellation; geometry parser lives in `bevy_pf_xaml`.
