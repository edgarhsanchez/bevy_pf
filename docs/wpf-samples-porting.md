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
| Styles & Templates / IntroToStylingAndTemplating | `--example wpf_samples_gallery` | photo ListBox re-template (IsItemsHost) + MouseEnter Storyboards deferred; keeps implicit/BasedOn styles and the implicit-Button ControlTemplate with hover/press triggers |
| Sample Applications / CalculatorDemo | `--example wpf_samples_gallery` | `local:MyTextBox` -> bordered TextBlocks; Click= -> observers; full arithmetic/memory/paper-tape state machine as a resource; gap fixed: MenuItem `IsCheckable` |
| Resources / MergedResources | `--example wpf_samples_gallery` | `sys:Double`/`sys:String` primitives verbatim; Source= files inlined; dictionary #3 file round-trip -> `merge_application_resources` (DynamicResource Background re-resolves live) |
| Elements / HeightProperties | `--example wpf_samples_gallery` | SelectionChanged -> a ListBox-selection system; ClipToBounds -> Canvas Overflow clip |

## Portable now (next in line)

The portable-now queue is empty — every sample that needed no new engine
features (plus the ones unlocked by the template-scope fix and MenuItem
IsCheckable) is ported. Everything left needs a feature from the blocked
list below.

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
