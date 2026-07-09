# wpf-upstream corpus

Verbatim XAML files harvested from Microsoft's open-source WPF reference
implementation, [dotnet/wpf](https://github.com/dotnet/wpf), at commit
`07ae487dddc7a805ad1c98ed31cd09b298cb4c33`.

dotnet/wpf is the compatibility target for `bevy_pf`: these files are what
the real WPF XAML/BAML toolchain itself ships and tests, so our parser must
accept every one of them. They are parsed by
`crates/bevy_pf_xaml/tests/corpus.rs` (`parses_all_wpf_upstream_corpus_files`).
Files whose root element is `ResourceDictionary` are parse-only; they are
deliberately **not** part of the `bevy_pf` instantiation corpus test
(`crates/bevy_pf/tests/instantiate.rs` only reads `tests/corpus/wpf/`).

## License / attribution

All files are MIT licensed. Copyright (c) .NET Foundation and Contributors.
See <https://github.com/dotnet/wpf/blob/main/LICENSE.TXT>.

The two `fluent_*` files additionally carry their original in-file notice:
Copyright (C) Leszek Pomianowski and WPF UI Contributors (MIT), based on
Microsoft XAML for WinUI, Copyright (c) Microsoft Corporation — as shipped in
dotnet/wpf under `src/Microsoft.DotNet.Wpf/src/Themes/PresentationFramework.Fluent/`.

Files are byte-for-byte verbatim copies (including UTF-8 BOMs where present).

## Files (graduated by complexity)

| Corpus file | Original path in dotnet/wpf | Exercises |
|---|---|---|
| `template_app.xaml` | `packaging/Microsoft.Dotnet.Wpf.ProjectTemplates/content/WpfApplication-CSharp/net6.0/App.xaml` | `dotnet new wpf` template: `Application` root, `x:Class`, `StartupUri`, empty `Application.Resources` property element, whitespace-only element content. |
| `template_mainwindow.xaml` | `packaging/Microsoft.Dotnet.Wpf.ProjectTemplates/content/WpfApplication-CSharp/net6.0/MainWindow.xaml` | Template `Window`/`App` pair: designer namespaces (`d:`, `mc:`), `mc:Ignorable="d"`, empty `Grid`. |
| `template_customcontrol_generic.xaml` | `packaging/Microsoft.Dotnet.Wpf.ProjectTemplates/content/WpfCustomControlLibrary-CSharp/net6.0/Themes/Generic.xaml` | Minimal theme `ResourceDictionary`: implicit `Style` keyed by `{x:Type local:...}` (clr-namespace type), `Setter.Value` property element holding a `ControlTemplate` with `TemplateBinding`s. |
| `sample_resourcedictionary.xaml` | `src/Microsoft.DotNet.Wpf/tests/UnitTests/PresentationFramework.Tests/System/Windows/Resources/SampleResourceDictionary.xaml` | WPF's own unit-test dictionary: `Style` + `BasedOn`, `Style.Triggers` with `Trigger`/`MultiTrigger` (+`Conditions`)/`DataTrigger`, standalone `ControlTemplate` with `ControlTemplate.Triggers` and `Setter TargetName`, `DataTemplate`, `LinearGradientBrush`. |
| `presentationui_classic_theme.xaml` | `src/Microsoft.DotNet.Wpf/src/PresentationUI/Themes/Classic.xaml` | Complete (small) PresentationUI theme `ResourceDictionary`: `{ComponentResourceKey TypeInTargetAssembly={x:Type ...}, ResourceId=...}` as `x:Key`, `{StaticResource {x:Static SystemColors...Key}}` (markup extension nested in a resource key position), bare `xmlns:sys="System"` namespace. |
| `presentationui_findtoolbar.xaml` | `src/Microsoft.DotNet.Wpf/src/PresentationUI/MS/Internal/Documents/FindToolBar.xaml` | Real shipped WPF UI (DocumentViewer Find toolbar): `ToolBar` root with `x:Class`/`x:ClassModifier`/`x:Uid`/`xml:lang`, attached properties on the root (`ToolBarTray.IsLocked`, `KeyboardNavigation.*`, `FocusManager.*`), `ToolBar.Resources`, `GradientBrush.GradientStops` with explicit `GradientStopCollection`, `{DynamicResource {x:Static ...}}`, styles + control templates for menu/buttons. |
| `ribbon_aero_theme.xaml` | `src/Microsoft.DotNet.Wpf/src/System.Windows.Controls.Ribbon/Themes/Aero.NormalColor.xaml` | Complete theme `ResourceDictionary` (~50 KB) for `RibbonWindow`/`RibbonContextualTabGroup`: 16 `ControlTemplate`s, 42 trigger constructs, `ComponentResourceKey` styles, character-entity resource key (`x:Key="&#200;"`), `shell:WindowChrome`, converters, `MultiDataTrigger`. |
| `fluent_button_styles.xaml` | `src/Microsoft.DotNet.Wpf/src/Themes/PresentationFramework.Fluent/Styles/Button.xaml` | .NET 9+ Fluent theme fragment (standalone `ResourceDictionary`): typed resources (`<Thickness x:Key=...>`), `DefaultButtonStyle` for `ButtonBase` with full `ControlTemplate`, `DynamicResource`-heavy setters, `ControlTemplate.Triggers`. |
| `fluent_expander_styles.xaml` | `src/Microsoft.DotNet.Wpf/src/Themes/PresentationFramework.Fluent/Styles/Expander.xaml` | Storyboard/animation stress: `Trigger.EnterActions`/`ExitActions`, `BeginStoryboard` + `Storyboard`, `DoubleAnimation`/`ThicknessAnimation` with `Storyboard.TargetName`/`TargetProperty` paths (e.g. `(FrameworkElement.LayoutTransform).(RotateTransform.Angle)`), `KeySpline`/easing, `LayoutTransform`, `x:Shared`-style theme resources. |
