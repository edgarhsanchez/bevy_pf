# Porting microsoft/WPF-Samples

The official WPF samples (MIT) double as bevy_pf's conformance targets.
Ported examples keep the original XAML with documented, minimal deviations.

## Ported

| Sample | bevy_pf example | Deviations |
|---|---|---|
| Getting Started / WalkthroughFirstWPFApp (ExpenseIt) | `--example wpf_expense_it` | NavigationWindow->Frame, XmlDataProvider->reflected VM, high-contrast triggers + ImageBrush dropped |

## Portable now (next in line)

- **Getting Started/MultiPage** — Two tiny Pages linked by Hyperlink NavigateUri with StartupUri=Page1.xaml — exercises the brand-new Frame/Page/Hyperlink navigation exactly; zero code-behind.
- **Getting Started/WalkthroughFirstWPFApp (ExpenseItIntro)** — Flagship walkthrough; portable after swapping XmlDataProvider for Rust-side data and dropping the nav-chrome dictionary (see expense_it.adaptation_notes).
- **Getting Started/HelloWorld** — Window+Grid+centered TextBlock only; ideal first smoke-test example.
- **Getting Started/SimpleLayout** — StackPanel + three Buttons with Margin/Width/HorizontalAlignment; pure supported layout.
- **Getting Started/DynamicLayout** — One Button with Click handler that adds text — Click maps to a Bevy observer; otherwise identical to SimpleLayout.
- **Getting Started/ComplexLayout** — Nested DockPanel/StackPanel with DockPanel.Dock attached props and 'px'-suffixed lengths (Height="30px") — good parser-conformance case.
- **Data Binding/SimpleBinding** — TwoWay TextBox binding with UpdateSourceTrigger=PropertyChanged to an object declared in resources, implicit TargetType styles, property-element Binding syntax; Person class becomes a Rust observable.
- **Data Binding/DirectionalBinding** — OneTime/OneWay/TwoWay + UpdateSourceTrigger matrix in one Grid — direct exercise of binding modes; TargetUpdated event handler is the only adaptation (drive the info text from the observable instead).
- **Data Binding/DataTrigger** — Style DataTrigger + MultiDataTrigger over a custom collection with a DataTemplate — exactly the supported trigger surface; make the DataType-implicit template an explicit ItemTemplate.
- **Data Binding/CollectionBinding** — ListBox ItemsSource + keyed DataTemplate + detail ContentControl; adapt CollectionView currency (IsSynchronizedWithCurrentItem) by wiring the detail DataContext to the ListBox selection in a system.
- **Data Binding/PropertyChangeNotification** — ItemsControl + DataTemplate over Canvas with live-updating bid prices — INotifyPropertyChanged analog demo; the code-behind timer becomes a Bevy system mutating observables.
- **Data Binding/DataBindingToStringFomat** — ListView+GridView DisplayMemberBinding with StringFormat=Now {0:c}! — all supported; cut the second half (MultiBinding StringFormat).
- **Resources/DefiningResources** — Keyed SolidColorBrush + keyed styles referenced via StaticResource across TextBlock/Button/Ellipse — pure supported resource system.
- **Resources/MergedResources** — Window-level MergedDictionaries + DynamicResource Background + StaticResource Button Content; sys:Double/sys:String primitives need mapping (string resources); code-behind dictionary-swap buttons demo runtime resource updates.
- **Elements/VisibiltyChanges** — Visible/Hidden/Collapsed toggled by three buttons — small, tests the Visibility tri-state in layout.
- **Elements/HeightProperties** — Height/MinHeight/MaxHeight precedence with Canvas+Rectangle and ListBox SelectionChanged handlers — plain controls, property-set systems.
- **Sample Applications/CalculatorDemo** — Menu/MenuItem (IsCheckable), Grid keypad of Buttons, ToolTips — all supported controls; arithmetic lives in Click handlers, straightforward as Bevy systems.
- **Graphics/ShapeElements** — Per-topic Pages of Line/Ellipse/Polygon/Polyline/Path mini-language (M10,100 C...z) with LinearGradientBrush headers; only adaptation is the DrawingBrush graph-paper background style in App.xaml (swap for a solid brush).

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

Biggest unlocks by sample count: `ControlTemplate`, `Storyboard`,
`IValueConverter`, `CollectionViewSource` (sort/filter/group), `RelativeSource`/`ElementName` bindings.
