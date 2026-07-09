# XAML compatibility test corpus

Open-source XAML files used as integration tests to verify that `bevy_pf`
stays compatible with real-world XAML. Parsed by
`crates/bevy_pf_xaml/tests/corpus.rs`, and (as the framework grows)
instantiated in headless Bevy by `crates/bevy_pf/tests/`.

## Sources & licenses (all MIT)

| Directory | File | Source repository |
|---|---|---|
| `wpf/` | `hello.xaml` | [microsoft/WPF-Samples](https://github.com/microsoft/WPF-Samples) `Getting Started/HelloWorld/MainWindow.xaml` |
| `wpf/` | `dynamiclayout.xaml` | microsoft/WPF-Samples `Getting Started/DynamicLayout/MainWindow.xaml` |
| `wpf/` | `simplebinding.xaml` | microsoft/WPF-Samples `Data Binding/SimpleBinding/MainWindow.xaml` |
| `wpf/` | `datatemplate.xaml` | microsoft/WPF-Samples `Data Binding/DataTemplatingIntro/MainWindow.xaml` |
| `wpf/` | `styling.xaml` | microsoft/WPF-Samples `Styles & Templates/IntroToStylingAndTemplating/MainWindow.xaml` |
| `wpf/` | `expenseithome.xaml` | microsoft/WPF-Samples `Getting Started/WalkthroughFirstWPFApp/csharp/ExpenseItHome.xaml` |
| `wpf/` | `expenseit_styles.xaml` | microsoft/WPF-Samples `Getting Started/WalkthroughFirstWPFApp/csharp/Styles.xaml` |
| `wpf/` | `expenseit_demo_mainwindow.xaml` | microsoft/WPF-Samples `Sample Applications/ExpenseIt/ExpenseItDemo/MainWindow.xaml` |
| `avalonia/` | `simple_todo_mainwindow.axaml` | [AvaloniaUI/Avalonia.Samples](https://github.com/AvaloniaUI/Avalonia.Samples) `CompleteApps/SimpleToDoList/Views/MainWindow.axaml` |

microsoft/WPF-Samples: Copyright (c) Microsoft Corporation, MIT license.
AvaloniaUI/Avalonia.Samples: Copyright (c) AvaloniaUI, MIT license.

`wpf-upstream/` holds verbatim files harvested from Microsoft's WPF
reference implementation itself ([dotnet/wpf](https://github.com/dotnet/wpf),
MIT, Copyright (c) .NET Foundation and Contributors) — theme
ResourceDictionaries, project-template XAML, and parser-stress DRT files.
See `wpf-upstream/README.md` for the commit hash and per-file provenance.
These are parse-only (`crates/bevy_pf_xaml/tests/corpus.rs`); they are not
part of the `bevy_pf` instantiation corpus.

Files are verbatim copies. The corpus grows as more features land; the
graduated ladder (trivial layout → bindings → templates → styles/triggers →
full app pages) comes from the framework's compatibility research (see
`docs/wpf-xaml-spec.md`).
