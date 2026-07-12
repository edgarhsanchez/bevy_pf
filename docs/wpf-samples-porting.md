# Porting microsoft/WPF-Samples

The official WPF samples (MIT) double as bevy_pf's conformance targets.
Ported examples keep the original XAML with documented, minimal deviations.

## Ported

| Sample | bevy_pf example | Deviations |
|---|---|---|
| Getting Started / WalkthroughFirstWPFApp (ExpenseIt) | `--example wpf_expense_it` | NavigationWindow->Frame, XmlDataProvider->reflected VM, high-contrast triggers + ImageBrush dropped |
| Getting Started / HelloWorld | `--example wpf_samples_gallery` | verbatim (Icon dropped) |
| Getting Started / SimpleLayout | `--example wpf_samples_gallery` | verbatim |
| Getting Started / ComplexLayout | `--example wpf_samples_gallery` | verbatim |
| Getting Started / DynamicLayout | `--example wpf_samples_gallery` | Click code-behind -> observer + bound ItemsControl |
| Getting Started / MultiPage | `--example wpf_samples_gallery` | verbatim — exercises inline `<Hyperlink>` runs in TextBlock |
| Data Binding / SimpleBinding | `--example wpf_samples_gallery` | `local:Person` resource -> DataContext |
| Data Binding / DirectionalBinding | `--example wpf_samples_gallery` | `local:NetIncome` -> DataContext; TargetUpdated handler -> observer |
| Data Binding / DataBindingToStringFormat | `--example wpf_samples_gallery` | MultiBinding half out of scope; `{0:c}` currency implemented |
| Data Binding / DataTrigger | `--example wpf_samples_gallery` | `local:Places` -> DataContext; DataType-implicit template -> explicit ItemTemplate; ListBoxItem implicit style -> keyed styles on template elements |
| Data Binding / CollectionBinding | `--example wpf_samples_gallery` | `local:People` -> DataContext; ToString() -> DisplayMemberPath; CollectionView currency + keyed ContentTemplate -> `current` VM field synced from ListBox selection, detail inlined |
| Data Binding / PropertyChangeNotification | `--example wpf_samples_gallery` | `local:BidCollection` -> DataContext; C# timer -> Bevy system mutating the observable; keyed ItemTemplate -> inline |
| Resources / DefiningResources | `--example wpf_samples_gallery` | verbatim (FontFamily attrs kept; Trebuchet resolves via family fallback) |
| Elements / VisibiltyChanges | `--example wpf_samples_gallery` | Click code-behind -> observers driving the property store's Visibility target |
| Graphics / ShapeElements (11 pages) | `--example wpf_samples_gallery` | original .xaml via `include_xaml!`; DrawingBrush graph paper -> solid brushes; MiterLimit's ScaleTransform copy at 1x (see examples/xaml/wpf_shapes/README.md) |

## Portable now (next in line)

- **Getting Started/MultiPage** — Two tiny Pages linked by Hyperlink NavigateUri with StartupUri=Page1.xaml — exercises the brand-new Frame/Page/Hyperlink navigation exactly; zero code-behind.
- **Getting Started/WalkthroughFirstWPFApp (ExpenseItIntro)** — Flagship walkthrough; portable after swapping XmlDataProvider for Rust-side data and dropping the nav-chrome dictionary (see expense_it.adaptation_notes).
- **Getting Started/HelloWorld** — Window+Grid+centered TextBlock only; ideal first smoke-test example.
- **Getting Started/SimpleLayout** — StackPanel + three Buttons with Margin/Width/HorizontalAlignment; pure supported layout.
- **Getting Started/DynamicLayout** — One Button with Click handler that adds text — Click maps to a Bevy observer; otherwise identical to SimpleLayout.
- **Getting Started/ComplexLayout** — Nested DockPanel/StackPanel with DockPanel.Dock attached props and 'px'-suffixed lengths (Height="30px") — good parser-conformance case.
- **Data Binding/SimpleBinding** — TwoWay TextBox binding with UpdateSourceTrigger=PropertyChanged to an object declared in resources, implicit TargetType styles, property-element Binding syntax; Person class becomes a Rust observable.
- **Data Binding/DirectionalBinding** — OneTime/OneWay/TwoWay + UpdateSourceTrigger matrix in one Grid — direct exercise of binding modes; TargetUpdated event handler is the only adaptation (drive the info text from the observable instead).
- **Data Binding/DataBindingToStringFomat** — ListView+GridView DisplayMemberBinding with StringFormat=Now {0:c}! — all supported; cut the second half (MultiBinding StringFormat).
- **Resources/MergedResources** — Window-level MergedDictionaries + DynamicResource Background + StaticResource Button Content; sys:Double/sys:String primitives need mapping (string resources); code-behind dictionary-swap buttons demo runtime resource updates.
- **Elements/HeightProperties** — Height/MinHeight/MaxHeight precedence with Canvas+Rectangle and ListBox SelectionChanged handlers — plain controls, property-set systems.
- **Sample Applications/CalculatorDemo** — Menu/MenuItem (IsCheckable), Grid keypad of Buttons, ToolTips — all supported controls; arithmetic lives in Click handlers, straightforward as Bevy systems.

## Blocked (feature gaps, in dependency order)

- **Styles & Templates/IntroToStylingAndTemplating** — blocked by ControlTemplate + Storyboard (EventTrigger MouseEnter animations)
- **Styles & Templates/EventTriggers** — blocked by Storyboard/BeginStoryboard
- **Styles & Templates/ContentControlStyle** — blocked by ControlTemplate
- **Styles & Templates/AlternatingAppearanceOfItems** — blocked by RelativeSource AlternationIndex + IValueConverter + CollectionViewSource
- **Resources/ApplicationResources** — blocked by ControlTemplate in Application.Resources
- **Data Binding/BindConversion** — blocked by IValueConverter (Converter= on bindings)
- **Data Binding/MultiBinding** — blocked by MultiBinding + IMultiValueConverter
- **Data Binding/CollectionViewSource** — blocked by CollectionViewSource SortDescriptions/GroupDescriptions
- **Data Binding/SortFilter** — blocked by ICollectionView sort/filter/currency driven from code-behind
- **Data Binding/Grouping** — blocked by PropertyGroupDescription + GroupStyle + XmlDataProvider
- **Data Binding/MasterDetail** — blocked by CollectionView currency: '/' paths (Divisions/Name) + IsSynchronizedWithCurrentItem across three levels
- **Data Binding/MasterDetailXml** — blocked by XmlDataProvider (external XML) + ElementName-chained DataContexts
- **Data Binding/XmlDataSource** — blocked by XmlDataProvider + XPath queries (*[@Stock='out'] ...)
- **Data Binding/UpdateSource** — blocked by UpdateSourceTrigger=Explicit + BindingExpression.UpdateSource() API
- **Data Binding/HierarchicalDataTemplate** — blocked by HierarchicalDataTemplate + DataType-implicit templates (TreeView/Menu)
- **Getting Started/ControlsAndLayout** — blocked by ListBoxItem ControlTemplate with template triggers in Scene.xaml
- **Getting Started/Concepts** — blocked by Mixed corpus: FlowDocument, Storyboards, converters throughout samps/
- **Windows/Wizard** — blocked by PageFunction<T> (x:TypeArguments) + modal NavigationWindow dialog + FocusManager ElementName
- **Windows/DialogBox** — blocked by Modal Window.ShowDialog + FocusManager.FocusedElement ElementName bindings
- **Windows/MessageBox** — blocked by MessageBox.Show API (XAML itself is a plain supported form)
- **Animation/PropertyAnimation** — blocked by Storyboard (in styles, templates, DataTriggers) — representative of all 11 Animation samples
- **Graphics/2DTransforms** — blocked by RenderTransform/MatrixTransform + Storyboard-animated transforms
- **Graphics/Brushes** — blocked by Storyboard brush animations + Command/ControlTemplate SampleViewer (static gradient pages could be cherry-picked)
- **Sample Applications/ExpenseIt/ExpenseItDemo** — blocked by ControlTemplate (EditBox custom control Themes/Generic.xaml), ICommand/x:Static, ValidationRules, RelativeSource
- **Sample Applications/DataBindingDemo** — blocked by CollectionViewSource sort/filter/group + IValueConverter + ControlTemplate

Gaps fixed by the batch-2 ports: inline `Hyperlink` runs inside `TextBlock`,
`Binding Mode=OneTime` (apply-once semantics), .NET numeric format specifiers
in `StringFormat` (`{0:c}` currency, `F`/`N`/`P`), and code-behind event
attributes (`Click=`, `Loaded=`, `TargetUpdated=`, ...) accepted silently so
verbatim markup instantiates clean. Template expansion now captures the declaring page's lexical resource scope
(`PfItemsSource.scopes` snapshot, applied by `instantiate_template`), so keyed
styles inside `DataTemplate`s resolve — this unblocked the DataTrigger port.

Biggest unlocks by sample count: `ControlTemplate`, `Storyboard`,
`IValueConverter`, `CollectionViewSource` (sort/filter/group), `RelativeSource`/`ElementName` bindings.
