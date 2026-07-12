//! ECS components attached to XAML-instantiated entities.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;

/// The XAML type name this entity was created from (`"Button"`, `"Grid"`, ...).
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct PfElementKind(pub String);

/// The element's `x:Name` / `Name`.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct PfName(pub String);

/// The element's `x:Uid` (WPF's localization/stable-identity directive).
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct PfUid(pub String);

/// The element's `AutomationProperties.AutomationId` (WPF's UI-automation id).
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct PfAutomationId(pub String);

/// On the root entity of an instantiated XAML scene: `x:Name` -> entity map.
#[derive(Component, Debug, Default)]
pub struct XamlNames(pub HashMap<String, Entity>);

impl XamlNames {
    /// Find a named element (e.g. to attach an observer to a button).
    pub fn get(&self, name: &str) -> Option<Entity> {
        self.0.get(name).copied()
    }
}

/// Attached-property values that a parent panel consumes
/// (`Grid.Row`, `Canvas.Left`, `DockPanel.Dock`, ...), stored as
/// `"Owner.Prop" -> raw string`.
#[derive(Component, Debug, Default, Clone)]
pub struct PfAttachedProps(pub HashMap<String, String>);

impl PfAttachedProps {
    pub fn get(&self, owner: &str, prop: &str) -> Option<&str> {
        self.0.get(&format!("{owner}.{prop}")).map(|s| s.as_str())
    }

    pub fn parse<T: std::str::FromStr>(&self, owner: &str, prop: &str) -> Option<T> {
        self.get(owner, prop).and_then(|s| s.trim().parse().ok())
    }
}

/// Interaction-state colors for a Button-like control; a system swaps the
/// background/border on `Interaction` changes.
#[derive(Component, Debug, Clone)]
pub struct ButtonVisual {
    pub normal_bg: Color,
    pub hover_bg: Color,
    pub pressed_bg: Color,
    pub normal_border: Color,
    pub hover_border: Color,
    pub pressed_border: Color,
}

impl Default for ButtonVisual {
    fn default() -> Self {
        // Classic Windows 10/11 button palette.
        Self {
            normal_bg: Color::srgb_u8(0xE1, 0xE1, 0xE1),
            hover_bg: Color::srgb_u8(0xE5, 0xF1, 0xFB),
            pressed_bg: Color::srgb_u8(0xCC, 0xE4, 0xF7),
            normal_border: Color::srgb_u8(0xAD, 0xAD, 0xAD),
            hover_border: Color::srgb_u8(0x00, 0x78, 0xD7),
            pressed_border: Color::srgb_u8(0x00, 0x54, 0x99),
        }
    }
}

/// The WPF accent color used by default control chrome.
pub const ACCENT: Color = Color::srgb(0.0, 0.47, 0.84); // #0078D7

/// Links a CheckBox/RadioButton root to its box and check-glyph entities so
/// visual-state systems can update them when `Checked` changes.
#[derive(Component, Debug, Clone)]
pub struct PfCheckVisual {
    pub box_node: Entity,
    pub glyph: Entity,
    /// CheckBox fills its box with the accent color when checked;
    /// RadioButton keeps a white circle and shows an accent dot.
    pub accent_fills_box: bool,
}

/// Marks a ToggleButton (a Button that latches `Checked`).
#[derive(Component, Debug, Default, Clone)]
pub struct PfToggleButton;

/// WPF `RadioButton.GroupName`. Radios with the same non-empty group name are
/// mutually exclusive; an empty name groups radios sharing the same parent.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct PfRadioGroup(pub String);

/// WPF `ProgressBar` state (`Minimum`/`Maximum`/`Value`).
#[derive(Component, Debug, Clone)]
pub struct PfProgress {
    pub min: f32,
    pub max: f32,
    pub value: f32,
    /// WPF `IsIndeterminate`: animated sweep instead of a value fill.
    pub indeterminate: bool,
}

impl PfProgress {
    pub fn fraction(&self) -> f32 {
        if self.max > self.min {
            ((self.value - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// WPF `Frame`: a navigable content host with a journal.
#[derive(Component, Debug, Clone)]
pub struct PfFrame {
    /// The entity whose children are the current page.
    pub content: Entity,
    /// Built-in back/forward chrome, when `NavigationUIVisibility` shows it.
    pub chrome: Option<PfFrameChrome>,
    /// Journal: routes behind the current page.
    pub back: Vec<String>,
    /// Journal: routes ahead of the current page (after `go_back`).
    pub forward: Vec<String>,
    /// The route currently shown.
    pub current: Option<String>,
    /// The current page's `Title`, if declared.
    pub current_title: Option<String>,
    /// `Source=` waiting for the page registry (resolved by a startup system).
    pub pending_source: Option<String>,
}

/// The built-in navigation chrome of a [`PfFrame`].
#[derive(Debug, Clone, Copy)]
pub struct PfFrameChrome {
    pub back_button: Entity,
    pub forward_button: Entity,
}

/// Links a ProgressBar root to its fill entity.
#[derive(Component, Debug, Clone)]
pub struct PfProgressVisual {
    pub fill: Entity,
}

/// Links a Slider root to its thumb entity (positioned by a system from
/// `SliderValue`/`SliderRange`).
#[derive(Component, Debug, Clone)]
pub struct PfSliderVisual {
    pub thumb: Entity,
}

/// Marks a ListBox root; tracks the selected item entity.
#[derive(Component, Debug, Default, Clone)]
pub struct PfListBox {
    pub selected: Option<Entity>,
}

/// Marks a selectable item container inside a ListBox.
#[derive(Component, Debug, Default, Clone)]
pub struct PfListBoxItem;

/// Links an Expander root to its collapsible content and arrow glyph.
#[derive(Component, Debug, Clone)]
pub struct PfExpander {
    pub content: Entity,
    pub arrow: Entity,
}

/// WPF `FrameworkElement.Tag`: an arbitrary user-data string slot.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct PfTag(pub String);

/// WPF `Viewbox`: scales its child to fit. The scale is applied post-layout
/// via `UiTransform` by a system (visual-only approximation).
#[derive(Component, Debug, Clone)]
pub struct PfViewbox {
    pub stretch: bevy_pf_xaml::value::Stretch,
}

/// The logical parent of an entity whose *entity-tree* parent differs (popup
/// content lives under the overlay root but inherits DataContext, resources,
/// and fonts from its logical owner, like WPF's logical tree).
#[derive(Component, Debug, Clone, Copy)]
pub struct PfLogicalParent(pub Entity);

/// WPF `ComboBox`: dropdown state and links to its generated parts.
#[derive(Component, Debug, Clone)]
pub struct PfComboBox {
    /// The dropdown content root (under the overlay layer).
    pub popup: Entity,
    /// The light-dismiss backdrop.
    pub backdrop: Entity,
    /// The text presenter showing the selection.
    pub text: Entity,
    pub selected: Option<usize>,
    pub open: bool,
}

/// A selectable entry inside a ComboBox dropdown.
#[derive(Component, Debug, Clone)]
pub struct PfComboItem {
    pub combo: Entity,
    pub index: usize,
}

/// WPF `TabControl`: tab strip + one content host per tab.
#[derive(Component, Debug, Clone)]
pub struct PfTabControl {
    pub headers: Vec<Entity>,
    pub contents: Vec<Entity>,
    pub selected: usize,
}

/// A clickable tab header.
#[derive(Component, Debug, Clone)]
pub struct PfTabHeader {
    pub tab_control: Entity,
    pub index: usize,
}

/// WPF `TreeView`: tracks the selected item's header entity.
#[derive(Component, Debug, Default, Clone)]
pub struct PfTreeView {
    pub selected: Option<Entity>,
}

/// A `TreeViewItem`: expander arrow + children container.
#[derive(Component, Debug, Clone)]
pub struct PfTreeItem {
    pub container: Entity,
    pub arrow: Entity,
    pub expanded: bool,
    pub has_children: bool,
}

/// A clickable tree item header row.
#[derive(Component, Debug, Clone)]
pub struct PfTreeHeader {
    pub tree: Entity,
    pub item: Entity,
}

/// A menu popup (top-level dropdown or nested submenu); closing a menu
/// closes every popup with this marker in the same menu tree.
#[derive(Component, Debug, Clone)]
pub struct PfMenuPopup {
    /// The Menu bar (or context-menu owner) this popup belongs to.
    pub menu_root: Entity,
}

/// A menu entry; leaf items close the menu on click, parents toggle their
/// submenu popup.
#[derive(Component, Debug, Clone)]
pub struct PfMenuItem {
    pub menu_root: Entity,
    pub submenu: Option<Entity>,
}

/// A `DataGrid` column definition (v1: text columns).
#[derive(Debug, Clone)]
pub struct PfGridColumn {
    pub header: String,
    pub path: String,
    pub width: bevy_pf_xaml::value::GridLength,
    /// `CellTemplate` (GridViewColumn) — expanded per cell with the row's
    /// scoped DataContext when present; otherwise `path` renders as text.
    pub template: Option<std::sync::Arc<bevy_pf_xaml::XamlNode>>,
}

/// WPF `DataGrid`: column definitions + the rows container (rows are
/// generated from `ItemsSource`).
#[derive(Component, Debug, Clone)]
pub struct PfDataGrid {
    pub columns: Vec<PfGridColumn>,
    pub rows_host: Entity,
}

/// WPF `Hyperlink`: clicking opens `NavigateUri` in the default browser.
#[derive(Component, Debug, Clone)]
pub struct PfHyperlink(pub String);

/// The XAML `<Popup>` placeholder element -> its overlay popup entity.
#[derive(Component, Debug, Clone)]
pub struct PfPopupSource {
    pub popup: Entity,
}

/// WPF `GridSplitter`: drags resize the two neighboring tracks of the
/// parent `Grid`.
#[derive(Component, Debug, Clone)]
pub struct PfGridSplitter {
    /// True = resizes columns (drag x), false = rows (drag y).
    pub columns: bool,
}

/// WPF `Calendar` month view.
#[derive(Component, Debug, Clone)]
pub struct PfCalendar {
    pub year: i32,
    pub month: u32,
    pub selected: Option<(i32, u32, u32)>,
    /// Grid hosting the day buttons (rebuilt on month change).
    pub days_host: Entity,
    /// The "July 2026" title text entity.
    pub title: Entity,
    /// DatePicker that owns this calendar, if any (selection reports back).
    pub owner_picker: Option<Entity>,
}

/// WPF `DatePicker`: display text + calendar dropdown on the popup layer.
#[derive(Component, Debug, Clone)]
pub struct PfDatePicker {
    pub calendar: Entity,
    pub popup: Entity,
    pub display: Entity,
    pub selected: Option<(i32, u32, u32)>,
}

/// Toolkit `ToggleSwitch`: pill track + sliding thumb, latching `Checked`.
#[derive(Component, Debug, Clone)]
pub struct PfToggleSwitch {
    pub track: Entity,
    pub thumb: Entity,
}

/// Toolkit `NumericUpDown` state and its readout entity.
#[derive(Component, Debug, Clone)]
pub struct PfNumericUpDown {
    pub value: f64,
    pub minimum: f64,
    pub maximum: f64,
    pub increment: f64,
    pub text: Entity,
}

/// Toolkit `RatingBar`: clickable pips, `value` of `maximum`.
#[derive(Component, Debug, Clone)]
pub struct PfRatingBar {
    pub value: u32,
    pub maximum: u32,
    pub pips: Vec<Entity>,
}

/// Watermark/placeholder overlay for an empty TextBox.
#[derive(Component, Debug, Clone)]
pub struct PfWatermark {
    pub overlay: Entity,
}

/// Toolkit `BusyIndicator`: dimming overlay shown while `busy`.
#[derive(Component, Debug, Clone)]
pub struct PfBusyIndicator {
    pub overlay: Entity,
    pub busy: bool,
}

/// Toolkit `RangeSlider`: two thumbs selecting an interval.
#[derive(Component, Debug, Clone)]
pub struct PfRangeSlider {
    pub lower: f32,
    pub upper: f32,
    pub minimum: f32,
    pub maximum: f32,
    pub thumb_lower: Entity,
    pub thumb_upper: Entity,
    pub fill: Entity,
}

/// Toolkit `TimePicker`: a display box with an hour/minute dropdown.
#[derive(Component, Debug, Clone)]
pub struct PfTimePicker {
    pub hour: Option<u32>,
    pub minute: Option<u32>,
    pub display: Entity,
    pub popup: Entity,
}

/// Toolkit `ColorPicker`: a swatch button with a palette dropdown.
#[derive(Component, Debug, Clone)]
pub struct PfColorPicker {
    pub selected: Color,
    pub swatch: Entity,
    pub hex_input: Entity,
    pub popup: Entity,
}

/// Marks the hex `EditableText` inside a [`PfColorPicker`] popup.
#[derive(Component, Debug, Clone)]
pub struct PfColorHexInput {
    pub owner: Entity,
}

/// Toolkit `AutoSuggestBox`: a TextBox with a filtered suggestion dropdown.
#[derive(Component, Debug, Clone)]
pub struct PfAutoSuggestBox {
    pub input: Entity,
    pub popup: Entity,
    pub items: Vec<String>,
}

/// Marks the `EditableText` inside a [`PfAutoSuggestBox`].
#[derive(Component, Debug, Clone)]
pub struct PfAutoSuggestInput {
    pub owner: Entity,
}

/// A control whose default chrome was replaced by a `ControlTemplate`.
/// Template-consumed properties (Background, BorderBrush/Thickness,
/// Padding, CornerRadius) stop painting the root — in WPF they reach the
/// visuals only through TemplateBinding inside the template.
#[derive(Component, Debug, Clone)]
pub struct PfTemplatedControl {
    /// The expanded template's visual root (despawn handle for template
    /// re-application / theme swaps).
    pub template_root: Entity,
}

/// A checkable `MenuItem` (`IsCheckable="True"`): activation toggles
/// `Checked` and the check glyph.
#[derive(Component, Debug, Clone)]
pub struct PfCheckableMenuItem {
    pub glyph: Entity,
}

/// WinUI-style `NavigationView`: a pane of items driving an embedded frame.
#[derive(Component, Debug, Clone)]
pub struct PfNavigationView {
    pub frame: Entity,
    pub items: Vec<Entity>,
    pub selected: Option<usize>,
}
