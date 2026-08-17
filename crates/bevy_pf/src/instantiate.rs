//! Instantiate a parsed [`XamlDocument`] into a `bevy_ui` entity tree.
//!
//! Layout mapping notes:
//! - `Grid` maps to CSS grid (taffy). All children get explicit placements so
//!   that, like WPF, un-annotated children overlap in cell (0,0) instead of
//!   auto-flowing.
//! - Single-content elements (`Window`, `Border`, `Button`, ...) are modeled
//!   as single-cell grids so WPF alignment semantics (default `Stretch`) fall
//!   out of `justify_self`/`align_self`.
//! - `StackPanel` maps to flex; per WPF semantics, main-axis alignment is
//!   ignored and cross-axis alignment maps to `align_self`.
//! - `DockPanel` is currently approximated as a vertical flex with
//!   `LastChildFill`; a faithful custom layout is planned.

use bevy::prelude::*;
use bevy::ui::{BorderColor, GridPlacement, GridTrack, Overflow, RepeatedGridTrack, Val};
use bevy_pf_xaml::markup::MarkupExtension;
use bevy_pf_xaml::value as v;
use bevy_pf_xaml::{XamlChild, XamlDocument, XamlNode, XamlValue};

use crate::components::*;
use crate::convert;
use crate::error::PfError;
use crate::resources::*;

/// Result of instantiating a XAML document.
pub struct InstantiateResult {
    pub root: Entity,
    pub warnings: Vec<String>,
}

/// Resolves a `(assembly, normalized path)` to XAML source text — how
/// `ResourceDictionary Source=` references load their files.
pub type XamlSourceLoader =
    std::sync::Arc<dyn Fn(Option<&str>, &str) -> Option<String> + Send + Sync>;

/// The loading environment of a XAML document: where it lives (for resolving
/// relative `Source=` uris) and how referenced files are read.
#[derive(Default, Clone)]
pub struct XamlEnv {
    /// Directory of the root document, `/`-separated, relative to the loader
    /// root. Empty = the loader root itself.
    pub base_dir: String,
    pub loader: Option<XamlSourceLoader>,
}

impl XamlEnv {
    /// An environment whose `Source=` references read from the filesystem
    /// under `root` (useful for tests and non-asset usage).
    pub fn from_fs_root(root: impl Into<std::path::PathBuf>) -> Self {
        let root: std::path::PathBuf = root.into();
        Self {
            base_dir: String::new(),
            loader: Some(std::sync::Arc::new(move |assembly, path| {
                if assembly.is_some() {
                    return None; // assembly routing not supported for plain fs
                }
                std::fs::read_to_string(root.join(path)).ok()
            })),
        }
    }
}

/// Instantiate `doc` into the world, reusing `root` as the root entity.
pub fn instantiate_document(
    world: &mut World,
    root: Entity,
    doc: &XamlDocument,
) -> Result<InstantiateResult, PfError> {
    instantiate_document_env(world, root, doc, &XamlEnv::default())
}

/// Ingest a document's resources into the application dictionary
/// (`Application.Resources` semantics): accepts a `ResourceDictionary` root
/// or any root with a `Resources` property element. Returns warnings.
pub fn set_application_resources(
    world: &mut World,
    doc: &XamlDocument,
    env: &XamlEnv,
) -> Vec<String> {
    let mut ctx = Ctx::new(world, env);
    let mut dict = ResourceDictionary::new();
    if doc.root.name == "ResourceDictionary" {
        ctx.ingest_rd_element(&doc.root, &mut dict);
    } else if let Some(res) = doc.root.property_element("Resources") {
        let res = res.clone();
        dict = ctx.build_resources_dict(&res);
    } else {
        ctx.warnings.push(format!(
            "`{}` root has no resources to ingest",
            doc.root.name
        ));
    }
    let warnings = std::mem::take(&mut ctx.warnings);
    crate::dynamic::merge_application_resources(world, dict);
    warnings
}

/// Instantiate with an explicit loading environment (merged dictionaries,
/// relative `Source=` resolution).
pub fn instantiate_document_env(
    world: &mut World,
    root: Entity,
    doc: &XamlDocument,
    env: &XamlEnv,
) -> Result<InstantiateResult, PfError> {
    let mut ctx = Ctx::new(world, env);
    ctx.spawn_element(&doc.root, ParentKind::None, Some(root))?;

    let names: bevy::platform::collections::HashMap<String, Entity> =
        ctx.names.iter().cloned().collect();
    let mut warnings = std::mem::take(&mut ctx.warnings);

    // Resolve ElementName + FindAncestor binding sources now that the tree
    // is complete (forward references included).
    let binding_entities = std::mem::take(&mut ctx.binding_entities);
    crate::binding::resolve_deferred_sources(world, &binding_entities, &names, &mut warnings);

    world.entity_mut(root).insert((
        XamlNames(names),
        bevy::input_focus::tab_navigation::TabGroup::default(),
    ));
    Ok(InstantiateResult { root, warnings })
}

/// What kind of layout container the parent is; decides how alignment and
/// attached properties apply to a child.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParentKind {
    None,
    /// CSS-grid parent (real `Grid`, or single-cell content hosts).
    Grid,
    FlexColumn,
    FlexRow,
    Canvas,
    /// DockPanel: the wrapper axis isn't known until docking, so alignment
    /// is stashed and applied by `build_dock_chain`.
    Dock,
}

/// Element kinds with dedicated behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ElemKind {
    Root, // Window / Page / UserControl
    Grid,
    StackPanel,
    WrapPanel,
    DockPanel,
    Canvas,
    UniformGrid,
    Border,
    ScrollViewer,
    TextBlock,
    Label,
    Button,
    ToggleButton,
    CheckBox,
    RadioButton,
    TextBox,
    Slider,
    ProgressBar,
    Separator,
    ListBox,
    ListBoxItem,
    ItemsControl,
    ComboBox,
    TabControl,
    TreeView,
    Menu,
    DataGrid,
    GroupBox,
    Expander,
    Viewbox,
    Frame,
    ToggleSwitch,
    NumericUpDown,
    RatingBar,
    Badge,
    BusyIndicator,
    RangeSlider,
    Image,
    Shape,
    StatusBar,
    StatusBarItem,
    ToolBar,
    ToolBarTray,
    Hyperlink,
    PopupElement,
    GridSplitter,
    Calendar,
    DatePicker,
    TimePicker,
    PackIcon,
    ColorPicker,
    ScrollBar,
    /// WPF `Track` (ScrollBar/Slider part): hosts the Thumb + page buttons.
    Track,
    /// WPF `Thumb`: the draggable grip.
    Thumb,
    AutoSuggestBox,
    NavigationView,
    ContentPresenter,
    Unknown,
}

impl ElemKind {
    fn from_name(name: &str) -> Self {
        match name {
            "Window" | "Page" | "UserControl" | "ContentControl" | "Application" => Self::Root,
            "HeaderedContentControl" => Self::GroupBox,
            "Grid" => Self::Grid,
            "StackPanel" | "VirtualizingStackPanel" => Self::StackPanel,
            "WrapPanel" => Self::WrapPanel,
            "DockPanel" => Self::DockPanel,
            "Canvas" => Self::Canvas,
            "UniformGrid" => Self::UniformGrid,
            "Border" => Self::Border,
            "ScrollViewer" => Self::ScrollViewer,
            "TextBlock" => Self::TextBlock,
            "Label" => Self::Label,
            "Button" | "RepeatButton" => Self::Button,
            "ToggleButton" => Self::ToggleButton,
            "CheckBox" => Self::CheckBox,
            "RadioButton" => Self::RadioButton,
            "TextBox" | "PasswordBox" => Self::TextBox,
            "Slider" => Self::Slider,
            "ProgressBar" => Self::ProgressBar,
            "Separator" => Self::Separator,
            "ListBox" | "ListView" => Self::ListBox,
            "ScrollBar" => Self::ScrollBar,
            "Track" => Self::Track,
            "Thumb" => Self::Thumb,
            "ComboBox" => Self::ComboBox,
            "TabControl" => Self::TabControl,
            "TreeView" => Self::TreeView,
            "Menu" | "ContextMenu" => Self::Menu,
            "DataGrid" => Self::DataGrid,
            "ListBoxItem" | "ComboBoxItem" | "ListViewItem" => Self::ListBoxItem,
            "ItemsControl" => Self::ItemsControl,
            "GroupBox" => Self::GroupBox,
            "Expander" => Self::Expander,
            "Viewbox" => Self::Viewbox,
            "Frame" => Self::Frame,
            "ToggleSwitch" => Self::ToggleSwitch,
            "NumericUpDown" | "IntegerUpDown" | "DoubleUpDown" => Self::NumericUpDown,
            "RatingBar" | "Rate" | "Rating" => Self::RatingBar,
            "Badge" | "Badged" => Self::Badge,
            "BusyIndicator" => Self::BusyIndicator,
            "RangeSlider" => Self::RangeSlider,
            // Toolkit presets that reuse Border's box model (chrome added in
            // insert_defaults by element name).
            "Card" | "Chip" | "Tag" => Self::Border,
            "Image" => Self::Image,
            "Rectangle" | "Ellipse" | "Line" | "Polyline" | "Polygon" | "Path" => Self::Shape,
            "StatusBar" => Self::StatusBar,
            "StatusBarItem" => Self::StatusBarItem,
            "ToolBar" => Self::ToolBar,
            "ToolBarTray" => Self::ToolBarTray,
            "Hyperlink" => Self::Hyperlink,
            "Popup" => Self::PopupElement,
            "GridSplitter" => Self::GridSplitter,
            "Calendar" => Self::Calendar,
            "DatePicker" => Self::DatePicker,
            "TimePicker" => Self::TimePicker,
            "PackIcon" | "FontIcon" | "SymbolIcon" => Self::PackIcon,
            "ColorPicker" => Self::ColorPicker,
            "AutoSuggestBox" => Self::AutoSuggestBox,
            "NavigationView" | "HamburgerMenu" => Self::NavigationView,
            "ContentPresenter" => Self::ContentPresenter,
            _ => Self::Unknown,
        }
    }

    /// The layout context this element provides for its children.
    fn as_parent(self, orientation: v::Orientation) -> ParentKind {
        match self {
            Self::Grid
            | Self::Root
            | Self::Border
            | Self::Button
            | Self::ToggleButton
            | Self::Label
            | Self::ScrollViewer
            | Self::ListBoxItem
            | Self::UniformGrid
            | Self::ContentPresenter => ParentKind::Grid,
            Self::StackPanel | Self::WrapPanel | Self::DockPanel => match orientation {
                v::Orientation::Vertical => ParentKind::FlexColumn,
                v::Orientation::Horizontal => ParentKind::FlexRow,
            },
            Self::Canvas => ParentKind::Canvas,
            Self::CheckBox | Self::RadioButton | Self::TextBox => ParentKind::FlexRow,
            Self::TextBlock
            | Self::Image
            | Self::Unknown
            | Self::Slider
            | Self::ScrollBar
            | Self::Track
            | Self::Thumb
            | Self::ProgressBar
            | Self::Separator
            | Self::ListBox
            | Self::ItemsControl
            | Self::ComboBox
            | Self::Shape
            | Self::GroupBox
            | Self::Expander
            | Self::Viewbox
            | Self::TabControl
            | Self::TreeView
            | Self::Menu
            | Self::DataGrid
            | Self::Hyperlink
            | Self::PopupElement
            | Self::GridSplitter
            | Self::Calendar
            | Self::DatePicker
            | Self::Frame
            | Self::ToggleSwitch
            | Self::NumericUpDown
            | Self::RatingBar
            | Self::Badge
            | Self::BusyIndicator
            | Self::RangeSlider
            | Self::TimePicker
            | Self::PackIcon
            | Self::ColorPicker
            | Self::AutoSuggestBox => ParentKind::FlexColumn,
            Self::StatusBar
            | Self::StatusBarItem
            | Self::ToolBar
            | Self::ToolBarTray
            | Self::NavigationView => ParentKind::FlexRow,
        }
    }
}

/// Control-specific attribute values collected during property application and
/// consumed when the element's content is built.
#[derive(Debug, Clone, Default)]
struct Pending {
    is_checked: Option<bool>,
    group_name: Option<String>,
    minimum: Option<f32>,
    maximum: Option<f32>,
    value: Option<f32>,
    max_length: Option<usize>,
    accepts_return: bool,
    rows: Option<u16>,
    columns: Option<u16>,
    selected_index: Option<usize>,
    shape: crate::shapes::ShapeParams,
    /// `{Binding}`s collected during property application: (property, spec).
    bindings: Vec<(String, crate::binding::BindingSpec)>,
    /// Entity of the generated content text child (Content="{Binding ...}").
    content_text: Option<Entity>,
    /// Entity of a TextBox's inner EditableText input.
    text_input: Option<Entity>,
    /// `ItemTemplate` for items controls (resource or inline DataTemplate).
    item_template: Option<std::sync::Arc<XamlNode>>,
    /// `Command=` name (literal or `{Binding path}` — the path IS the name).
    command: Option<String>,
    /// `CommandParameter=`, resolved at activation time.
    command_parameter: Option<crate::binding::PfCommandParameter>,
    /// `Template` (ControlTemplate) with the resource scope captured where
    /// it was applied; expansion lands in a later phase.
    control_template: Option<(
        std::sync::Arc<crate::resources::PfControlTemplate>,
        std::sync::Arc<ResourceScopes>,
    )>,
    /// `HorizontalContentAlignment` / `VerticalContentAlignment`: where the
    /// CONTENT sits inside the control, as distinct from where the control
    /// sits inside its parent. Collected here and consumed by
    /// `project_content`, which otherwise applies a per-kind default.
    h_content_align: Option<JustifyItems>,
    v_content_align: Option<AlignItems>,
    /// `DisplayMemberPath` for items controls.
    display_member: Option<String>,
    /// `IsIndeterminate` for ProgressBar.
    is_indeterminate: bool,
    /// A `Foreground` set on THIS element (not inherited). WPF's
    /// ProgressBar fills with its Foreground brush; the inherited value
    /// must not repaint every bar under a colored container, so the fill
    /// only follows an explicit per-element Foreground.
    own_foreground: Option<v::PfColor>,
    /// Code-behind pointer events: (event name, handler name) pairs wired
    /// as observers dispatching through `PfEventHandlers` after spawn.
    event_handlers: Vec<(String, String)>,
    /// `ItemContainerStyle` for items controls (inline or resource).
    item_container_style: Option<std::sync::Arc<crate::resources::PfStyle>>,
    /// `ContentTemplate` for ContentControl (wins over DataType selection).
    content_template: Option<std::sync::Arc<XamlNode>>,
    /// The element is a literal `<ContentControl>`: a bound Content becomes
    /// a `PfContentSource` (template expansion) instead of a text binding.
    is_content_control: bool,
}

/// Font and text properties that flow down the tree (WPF property value
/// inheritance for `TextElement.*`), plus per-element text state consumed
/// when the element's text components are built.
#[derive(Debug, Clone)]
struct Inherited {
    font_size: f32,
    foreground: v::PfColor,
    font_family: Option<String>,
    font_weight: v::FontWeight,
    font_style: v::FontStyleKind,
    text_alignment: Option<v::TextAlignment>,
    text_wrapping: v::TextWrapping,
}

impl Default for Inherited {
    fn default() -> Self {
        Self {
            // WPF defaults: 12 device-independent px, black text.
            font_size: 12.0,
            foreground: v::PfColor::BLACK,
            font_family: None,
            font_weight: v::FontWeight::NORMAL,
            font_style: v::FontStyleKind::Normal,
            text_alignment: None,
            text_wrapping: v::TextWrapping::NoWrap,
        }
    }
}

/// A property value after markup-extension resolution.
enum Resolved<'a> {
    Str(&'a str),
    Value(PfValue),
    Null,
}

impl<'a> Resolved<'a> {
    fn to_brush(&self) -> Result<v::PfBrush, PfError> {
        match self {
            Resolved::Str(s) => Ok(s.parse()?),
            Resolved::Value(PfValue::Brush(b)) => Ok(b.clone()),
            Resolved::Value(PfValue::Color(c)) => Ok(v::PfBrush::Solid(*c)),
            Resolved::Value(PfValue::String(s)) => Ok(s.parse()?),
            _ => Err(PfError::instantiate("expected a brush value")),
        }
    }

    fn to_f32(&self) -> Result<f32, PfError> {
        match self {
            // WPF LengthConverter semantics: Auto/NaN, px/in/cm/pt units.
            Resolved::Str(s) => Ok(v::parse_length(s, "length")?),
            Resolved::Value(PfValue::Double(d)) => Ok(*d as f32),
            Resolved::Value(PfValue::String(s)) => s
                .trim()
                .parse()
                .map_err(|e| PfError::instantiate(format!("bad number `{s}`: {e}"))),
            _ => Err(PfError::instantiate("expected a number")),
        }
    }

    fn to_thickness(&self) -> Result<v::Thickness, PfError> {
        match self {
            Resolved::Str(s) => Ok(s.parse()?),
            Resolved::Value(PfValue::Thickness(t)) => Ok(*t),
            Resolved::Value(PfValue::Double(d)) => Ok(v::Thickness::uniform(*d as f32)),
            Resolved::Value(PfValue::String(s)) => Ok(s.parse()?),
            _ => Err(PfError::instantiate("expected a thickness")),
        }
    }

    fn to_corner_radius(&self) -> Result<v::CornerRadius, PfError> {
        match self {
            Resolved::Str(s) => Ok(s.parse()?),
            Resolved::Value(PfValue::CornerRadius(c)) => Ok(*c),
            Resolved::Value(PfValue::Double(d)) => Ok(v::CornerRadius::uniform(*d as f32)),
            _ => Err(PfError::instantiate("expected a corner radius")),
        }
    }

    fn to_text(&self) -> Result<String, PfError> {
        match self {
            Resolved::Str(s) => Ok((*s).to_string()),
            Resolved::Value(PfValue::String(s)) => Ok(s.clone()),
            Resolved::Value(PfValue::Double(d)) => Ok(format!("{d}")),
            Resolved::Value(PfValue::Bool(b)) => Ok(format!("{b}")),
            Resolved::Null => Ok(String::new()),
            _ => Err(PfError::instantiate("expected a string")),
        }
    }

    fn to_bool(&self) -> Result<bool, PfError> {
        match self {
            Resolved::Str(s) => Ok(s.trim().eq_ignore_ascii_case("true")),
            Resolved::Value(PfValue::Bool(b)) => Ok(*b),
            Resolved::Value(PfValue::String(s)) => Ok(s.trim().eq_ignore_ascii_case("true")),
            Resolved::Null => Ok(false),
            _ => Err(PfError::instantiate("expected a boolean")),
        }
    }

    fn parse_enum<T>(&self) -> Result<T, PfError>
    where
        T: std::str::FromStr<Err = bevy_pf_xaml::XamlError>,
    {
        match self {
            Resolved::Str(s) => Ok(s.parse()?),
            Resolved::Value(PfValue::String(s)) => Ok(s.parse()?),
            _ => Err(PfError::instantiate("expected an enum value")),
        }
    }
}

/// One in-flight ControlTemplate expansion (see `Ctx::template_ctx`).
struct TemplateCtx {
    /// The control this template is being applied to.
    templated_parent: Entity,
    /// Per-expansion namescope: template-internal x:Names land here, never
    /// in the page registry (each expansion re-uses the same names).
    names: bevy::platform::collections::HashMap<String, Entity>,
    /// The first ContentPresenter spawned during the expansion.
    content_host: Option<Entity>,
}

struct Ctx<'w> {
    world: &'w mut World,
    scopes: ResourceScopes,
    names: Vec<(String, Entity)>,
    inherited: Inherited,
    pending: Pending,
    /// Entities that received `PfBindings` (for ElementName resolution).
    binding_entities: Vec<Entity>,
    env: XamlEnv,
    /// The value-provider tier the current property writes land at
    /// (`Local` for attributes, `Style` while applying style setters).
    tier: crate::provider::ValueSource,
    /// Active ControlTemplate expansions (top = innermost). While non-empty,
    /// x:Name registrations divert into the top scope, and a
    /// ContentPresenter records itself as the content host.
    template_ctx: Vec<TemplateCtx>,
    /// Directory stack for resolving relative `Source=` uris while loading
    /// nested merged dictionaries.
    base_stack: Vec<String>,
    /// Cycle guard: paths on the *active* merge chain only (insert/remove
    /// paired around ingestion) — sibling and diamond re-merges of the same
    /// file are legal WPF and must not trip it.
    merge_path: std::collections::HashSet<String>,
    /// Memoized merged dictionaries by resolved path, so diamonds load once.
    merged_cache: std::collections::HashMap<String, std::sync::Arc<ResourceDictionary>>,
    /// `Styles` collections in scope, in ATTACH order: application
    /// first, then each ancestor's, then the element's own. Order is the
    /// whole precedence story inside a bucket — last attached wins — so this
    /// must never be reordered.
    sheets: Vec<std::sync::Arc<crate::resources::PfStyle>>,
    /// The element chain being built, root first, for selector matching.
    element_stack: Vec<crate::selector::ElementInfo>,
    /// Position of the element about to be spawned among its siblings, for
    /// `:nth-child`. Set by the child loop; (0, 1) for a lone root.
    child_pos: (usize, usize),
    warnings: Vec<String>,
}

impl<'w> Ctx<'w> {
    fn new(world: &'w mut World, env: &XamlEnv) -> Self {
        let app = world
            .get_resource::<crate::dynamic::PfApplicationResources>()
            .map(|r| r.dict.clone());
        let mut scopes = ResourceScopes::default();
        scopes.app = app;
        Ctx {
            world,
            scopes,
            names: Vec::new(),
            inherited: Inherited::default(),
            pending: Pending::default(),
            binding_entities: Vec::new(),
            env: env.clone(),
            tier: crate::provider::ValueSource::Local,
            template_ctx: Vec::new(),
            base_stack: vec![env.base_dir.clone()],
            merge_path: std::collections::HashSet::new(),
            merged_cache: std::collections::HashMap::new(),
            sheets: Vec::new(),
            element_stack: Vec::new(),
            child_pos: (0, 1),
            warnings: Vec::new(),
        }
    }

    fn warn(&mut self, msg: impl Into<String>) {
        self.warnings.push(msg.into());
    }

    // -----------------------------------------------------------------
    // Resource dictionaries (incl. merged dictionaries / Source=)
    // -----------------------------------------------------------------

    /// Build the dictionary for a `Resources` property element, handling an
    /// optional explicit `<ResourceDictionary>` wrapper with
    /// `MergedDictionaries` / `Source=`.
    fn build_resources_dict(
        &mut self,
        pe: &bevy_pf_xaml::XamlPropertyElement,
    ) -> ResourceDictionary {
        let elements: Vec<XamlNode> = pe.elements().cloned().collect();
        let mut dict = ResourceDictionary::new();
        if elements.len() == 1 && elements[0].name == "ResourceDictionary" {
            self.ingest_rd_element(&elements[0], &mut dict);
        } else {
            parse_resource_entries_into(
                elements.iter(),
                &self.scopes,
                &mut dict,
                &mut self.warnings,
            );
        }
        dict
    }

    /// Ingest a `<ResourceDictionary>` element. WPF order: merged
    /// dictionaries are the fallback tier (later beats earlier); content
    /// loaded via `Source=` counts as this dictionary's own content (beats
    /// merged); literal own entries win over everything.
    fn ingest_rd_element(&mut self, node: &XamlNode, dict: &mut ResourceDictionary) {
        if let Some(md) = node.property_element("MergedDictionaries") {
            let entries: Vec<XamlNode> = md.elements().cloned().collect();
            for rd in &entries {
                if rd.name != "ResourceDictionary" {
                    self.warn(format!(
                        "{}: unexpected `{}` in MergedDictionaries",
                        rd.pos, rd.name
                    ));
                    continue;
                }
                if let Some(XamlValue::Str(src)) = rd.attribute("Source") {
                    let src = src.clone();
                    if let Some(loaded) = self.load_merged_source(&src) {
                        dict.extend(loaded.iter().map(|(k, v)| (k.clone(), v.clone())));
                    }
                } else {
                    // Inline nested dictionary.
                    self.ingest_rd_element(rd, dict);
                }
            }
        }
        if let Some(XamlValue::Str(src)) = node.attribute("Source") {
            let src = src.clone();
            if let Some(loaded) = self.load_merged_source(&src) {
                dict.extend(loaded.iter().map(|(k, v)| (k.clone(), v.clone())));
            }
        }
        let own: Vec<XamlNode> = node.child_elements().cloned().collect();
        parse_resource_entries_into(own.iter(), &self.scopes, dict, &mut self.warnings);
    }

    /// Load and ingest a `Source=`-referenced dictionary file (memoized per
    /// resolved path; only genuine reference cycles are rejected).
    fn load_merged_source(&mut self, src: &str) -> Option<std::sync::Arc<ResourceDictionary>> {
        let uri = match bevy_pf_xaml::uri::PfUri::parse(src) {
            Ok(u) => u,
            Err(e) => {
                self.warn(format!("bad ResourceDictionary Source `{src}`: {e}"));
                return None;
            }
        };
        let base = self.base_stack.last().cloned().unwrap_or_default();
        let resolved = uri.resolve(&base);
        let visit_key = format!("{}::{resolved}", uri.assembly.as_deref().unwrap_or(""));
        if let Some(cached) = self.merged_cache.get(&visit_key) {
            return Some(cached.clone());
        }
        if self.merge_path.contains(&visit_key) {
            self.warn(format!(
                "merged dictionary cycle detected at `{src}`; skipping"
            ));
            return None;
        }
        let Some(loader) = self.env.loader.clone() else {
            self.warn(format!(
                "ResourceDictionary Source `{src}` needs a loading environment \
                 (spawn via XamlView assets or instantiate_document_env)"
            ));
            return None;
        };
        let Some(source) = loader(uri.assembly.as_deref(), &resolved) else {
            self.warn(format!(
                "merged dictionary `{src}` (resolved `{resolved}`) not found"
            ));
            return None;
        };
        let doc = match bevy_pf_xaml::parse(&source) {
            Ok(d) => d,
            Err(e) => {
                self.warn(format!("merged dictionary `{src}` failed to parse: {e}"));
                return None;
            }
        };
        if doc.root.name != "ResourceDictionary" {
            self.warn(format!(
                "merged dictionary `{src}` root is `{}`, expected ResourceDictionary",
                doc.root.name
            ));
            return None;
        }
        let sub_base = resolved
            .rsplit_once('/')
            .map(|(d, _)| d.to_string())
            .unwrap_or_default();
        self.merge_path.insert(visit_key.clone());
        self.base_stack.push(sub_base);
        // WPF: a merged dictionary's own StaticResources resolve against the
        // file itself (and the app tier), never the consuming element's
        // lexical scopes — isolate the stack while ingesting.
        let outer_scopes = self.scopes.isolate_stack();
        let mut dict = ResourceDictionary::new();
        self.ingest_rd_element(&doc.root, &mut dict);
        self.scopes.restore_stack(outer_scopes);
        self.base_stack.pop();
        self.merge_path.remove(&visit_key);
        let dict = std::sync::Arc::new(dict);
        self.merged_cache.insert(visit_key, dict.clone());
        Some(dict)
    }

    fn node_mut(&mut self, entity: Entity) -> Mut<'_, Node> {
        self.world
            .get_mut::<Node>(entity)
            .expect("pf entities always have Node")
    }

    // -----------------------------------------------------------------
    // Element spawning
    // -----------------------------------------------------------------

    fn spawn_element(
        &mut self,
        node: &XamlNode,
        parent_kind: ParentKind,
        reuse: Option<Entity>,
    ) -> Result<Entity, PfError> {
        let kind = ElemKind::from_name(&node.name);
        if kind == ElemKind::Unknown && !self.is_registered_element(&node.name) {
            self.warn(format!(
                "{}: element `{}` is not supported yet; treating as a plain container",
                node.pos, node.name
            ));
        }

        let saved_inherited = self.inherited.clone();
        let saved_pending = std::mem::take(&mut self.pending);

        // The element chain the selector matcher walks. Pushed before any
        // style is applied and popped with the other scoped state below.
        let (child_index, sibling_count) = std::mem::replace(&mut self.child_pos, (0, 1));
        self.element_stack.push(crate::selector::ElementInfo {
            type_name: node.name.clone(),
            name: node.x_name.clone(),
            classes: crate::selector::classes_of(node),
            child_index,
            sibling_count,
        });
        // `<X.Styles>` applies to this element and its subtree, so it joins
        // the sheet stack for exactly that span.
        let sheets_before = self.sheets.len();
        if let Some(styles) = node.property_element("Styles") {
            let styles = styles.clone();
            self.collect_sheets(&styles);
        }

        // Spawn the entity with per-kind defaults.
        let entity = match reuse {
            Some(e) => e,
            None => self.world.spawn_empty().id(),
        };

        // Lexically scoped resources (kept on the entity for runtime
        // DynamicResource lookups; Application roots feed the global tier).
        let mut pushed_scope = false;
        if let Some(res) = node.property_element("Resources") {
            let res = res.clone();
            let dict = std::sync::Arc::new(self.build_resources_dict(&res));
            self.world
                .entity_mut(entity)
                .insert(crate::dynamic::PfResources(dict.clone()));
            if node.name == "Application" {
                crate::dynamic::merge_application_resources(self.world, (*dict).clone());
                self.scopes.app = self
                    .world
                    .get_resource::<crate::dynamic::PfApplicationResources>()
                    .map(|r| r.dict.clone());
            }
            self.scopes.push(dict);
            pushed_scope = true;
        }

        self.insert_defaults(entity, kind, node);
        self.apply_toolkit_presets(entity, kind, node);

        if kind == ElemKind::Thumb {
            self.world.entity_mut(entity).insert(Interaction::default());
        }
        // RepeatButton is a Button plus the repeat-while-held component.
        if node.name == "RepeatButton" {
            self.world
                .entity_mut(entity)
                .insert(crate::components::PfRepeatButton::default());
        }
        self.pending.is_content_control = node.name == "ContentControl";

        // WPF: interactive controls are tab stops (text inputs get their
        // TabIndex on the inner editable instead). `IsTabStop="False"` opts a
        // control out of the Tab ring AND out of click-to-focus -- both key
        // off TabIndex -- which is how a screen keeps its buttons from
        // stealing keyboard focus from its text fields.
        let is_tab_stop = !matches!(
            node.attribute("IsTabStop"),
            Some(XamlValue::Str(v)) if v.eq_ignore_ascii_case("false")
        );
        if is_tab_stop
            && matches!(
                kind,
                ElemKind::Button
                    | ElemKind::ToggleButton
                    | ElemKind::CheckBox
                    | ElemKind::RadioButton
                    | ElemKind::Slider
                    | ElemKind::ScrollBar
                    | ElemKind::ComboBox
                    | ElemKind::ListBox
                    | ElemKind::ToggleSwitch
                    | ElemKind::Hyperlink
            )
        {
            self.world
                .entity_mut(entity)
                .insert(bevy::input_focus::tab_navigation::TabIndex(0));
        }

        // Inside a ControlTemplate expansion, stamp the templated parent
        // BEFORE properties/bindings apply (RelativeSource TemplatedParent
        // resolves at attach time; PART_ lookups and TemplateBinding too).
        if let Some(tc) = self.template_ctx.last() {
            let parent = tc.templated_parent;
            self.world
                .entity_mut(entity)
                .insert(crate::components::PfTemplatedParent(parent));
        }

        // WPF arranges a scene's root content with the full window constraint:
        // any root element fills its container. Explicit Width/Height (applied
        // below) still override, exactly like fixed-size WPF content.
        if parent_kind == ParentKind::None
            && kind != ElemKind::Root
            && let Some(mut n) = self.world.get_mut::<Node>(entity)
        {
            n.width = Val::Percent(100.0);
            n.height = Val::Percent(100.0);
        }

        // Effective style: explicit `Style` attribute wins over implicit
        // (by-type) style, exactly like WPF.
        let style = self.effective_style(node);
        if let Some(style) = style {
            for setter in style.setters.clone() {
                self.apply_setter(entity, kind, parent_kind, &setter);
            }
            if !style.triggers.is_empty() {
                self.attach_triggers(entity, &style.triggers, None);
            }
            if !style.event_triggers.is_empty() {
                let event_triggers = style.event_triggers.clone();
                self.attach_event_triggers(entity, &event_triggers);
            }
        }

        // Selector styles, after the WPF style and before local
        // attributes, so the precedence order matches both dialects: style
        // tier below, local attribute above.
        self.apply_selector_styles(entity, kind, parent_kind);

        // Local attribute values (highest precedence).
        for attr in &node.attributes {
            let (owner, name, value) = (attr.owner.clone(), attr.name.clone(), attr.value.clone());
            self.apply_xaml_value(entity, kind, parent_kind, owner.as_deref(), &name, &value);
        }

        // Property-element values other than the structural ones.
        for pe in &node.property_elements {
            if matches!(
                pe.name.as_str(),
                // `Styles` is consumed above, onto the selector sheet stack.
                "Resources" | "RowDefinitions" | "ColumnDefinitions" | "Styles"
            ) {
                continue;
            }
            self.apply_property_element(entity, kind, parent_kind, pe);
        }

        // Grid track definitions (needs to happen before children).
        if kind == ElemKind::Grid {
            self.configure_grid_tracks(entity, node);
        }

        // Panel.ZIndex / Canvas.ZIndex attached properties.
        if let Some(attached) = self.world.get::<PfAttachedProps>(entity) {
            let z = attached
                .parse::<i32>("Panel", "ZIndex")
                .or_else(|| attached.parse::<i32>("Canvas", "ZIndex"))
                .or_else(|| attached.parse::<i32>("Grid", "ZIndex"));
            if let Some(z) = z {
                self.world.entity_mut(entity).insert(bevy::ui::ZIndex(z));
            }
        }

        // Content.
        self.spawn_content(entity, kind, node)?;

        // Resolve recorded {Binding}s to typed targets.
        let bindings = std::mem::take(&mut self.pending.bindings);
        for (property, spec) in bindings {
            self.attach_binding(entity, kind, &property, spec);
        }

        // An Opacity attribute applied before content spawned covers the
        // full subtree now.
        crate::provider::reapply_opacity(self.world, entity);

        // Command= source: activation runs the DataContext's registered
        // command and writes PfCommandInvoked.
        if let Some(name) = self.pending.command.take() {
            let parameter = self.pending.command_parameter.take();
            self.world
                .entity_mut(entity)
                .insert(crate::components::PfCommand { name, parameter });
            let source = entity;
            self.world.entity_mut(entity).observe(
                move |_click: On<Pointer<Click>>, mut commands: Commands| {
                    commands.queue(move |world: &mut World| {
                        let Some(cmd) = world.get::<crate::components::PfCommand>(source).cloned()
                        else {
                            return;
                        };
                        crate::binding::invoke_command(
                            world,
                            source,
                            &cmd.name,
                            cmd.parameter.as_ref(),
                        );
                    });
                },
            );
        }

        // Code-behind pointer events → named-handler observers.
        for (event, handler) in std::mem::take(&mut self.pending.event_handlers) {
            crate::events::attach_event_handler(self.world, entity, &event, handler);
        }

        // Custom elements (app.register_element): the builder runs last,
        // with common properties, bindings, and children already in place.
        if kind == ElemKind::Unknown {
            let builder = self
                .world
                .get_resource::<PfElements>()
                .and_then(|r| r.0.get(&node.name).cloned());
            if let Some(builder) = builder {
                builder(self.world, entity, node);
            }
        }

        // Register x:Name. Inside a ControlTemplate expansion, names are
        // per-expansion (never the page registry, and no PfName: a global
        // by-name query must not surface template internals).
        if let Some(name) = &node.x_name {
            if let Some(tc) = self.template_ctx.last_mut() {
                tc.names.insert(name.clone(), entity);
            } else {
                self.names.push((name.clone(), entity));
                self.world.entity_mut(entity).insert(PfName(name.clone()));
            }
        }

        // x:Uid: a stable identity, queryable like x:Name but never scoped.
        if let Some(uid) = &node.x_uid {
            self.world
                .entity_mut(entity)
                .insert(crate::components::PfUid(uid.clone()));
        }

        self.inherited = saved_inherited;
        self.pending = saved_pending;
        self.element_stack.pop();
        self.sheets.truncate(sheets_before);
        if pushed_scope {
            self.scopes.pop();
        }
        Ok(entity)
    }

    /// Gather `<Style Selector=...>` entries from a `Styles` collection onto
    /// the sheet stack, in document order.
    fn collect_sheets(&mut self, styles: &crate::xaml_ast::XamlPropertyElement) {
        for child in styles.elements() {
            match child.name.as_str() {
                "Style" => {
                    let mut warnings = Vec::new();
                    let scopes = self.scopes.clone();
                    let empty = ResourceDictionary::new();
                    match crate::resources::parse_style(child, &scopes, &empty, &mut warnings) {
                        Ok(style) if style.selector.is_some() => {
                            self.sheets.push(std::sync::Arc::new(style))
                        }
                        // A <Style> in a Styles collection without a Selector
                        // matches nothing — say so rather than
                        // silently keeping it.
                        Ok(_) => self.warn(format!(
                            "{}: a <Style> in a Styles collection needs a Selector",
                            child.pos
                        )),
                        Err(e) => self.warn(format!("{}: {e}", child.pos)),
                    }
                    self.warnings.extend(warnings);
                }
                // StyleInclude / FluentTheme and friends land in later phases.
                other => self.warn(format!(
                    "{}: `{other}` in a Styles collection is not supported yet",
                    child.pos
                )),
            }
        }
    }

    /// Apply every selector style whose selector matches this element.
    ///
    /// Phase 1 handles the NON-ACTIVATED bucket only — selectors built from
    /// types, names and structure, whose membership cannot change once the
    /// tree exists. They write at the same `Style` tier as a WPF style, in
    /// attach order, which is exactly "last attached wins".
    /// Activated selectors (classes, pseudo-classes, property tests) need the
    /// trigger runtime and land in the next phase.
    fn apply_selector_styles(&mut self, entity: Entity, kind: ElemKind, parent_kind: ParentKind) {
        if self.sheets.is_empty() {
            return;
        }
        let stack = std::mem::take(&mut self.element_stack);
        let matched: Vec<std::sync::Arc<crate::resources::PfStyle>> = self
            .sheets
            .iter()
            .filter(|style| {
                style.selector.as_ref().is_some_and(|sel| {
                    !sel.has_activator() && crate::selector::matches(sel, &stack)
                })
            })
            .cloned()
            .collect();
        self.element_stack = stack;

        let saved_tier = self.tier;
        self.tier = crate::provider::ValueSource::Style;
        for style in matched {
            for setter in &style.setters {
                self.apply_setter_inner(entity, kind, parent_kind, setter);
            }
        }
        self.tier = saved_tier;
        self.attach_activated_styles(entity, kind, parent_kind);
    }

    /// Attach the ACTIVATED bucket: selectors carrying a class, a pseudo-class
    /// or a property test, whose membership can change while the app runs.
    ///
    /// These become ordinary triggers. bevy_pf's condition set already covers
    /// every pseudo-class — `:pointerover` is `IsMouseOver`,
    /// `:disabled` is `IsEnabled=false`, and so on — so no new runtime is
    /// needed: `evaluate_triggers` already clears the tier and re-applies
    /// every active contributor in declaration order, which IS the dialect's
    /// reevaluation rule. They write at `ImplicitReference`, a tier that was
    /// declared and never used, so no existing writer can be disturbed.
    fn attach_activated_styles(&mut self, entity: Entity, kind: ElemKind, parent_kind: ParentKind) {
        let _ = (kind, parent_kind);
        if self.sheets.is_empty() {
            return;
        }
        let stack = std::mem::take(&mut self.element_stack);
        let candidates: Vec<std::sync::Arc<crate::resources::PfStyle>> = self
            .sheets
            .iter()
            .filter(|style| {
                style.selector.as_ref().is_some_and(|sel| {
                    sel.has_activator() && crate::selector::matches_static(sel, &stack)
                })
            })
            .cloned()
            .collect();
        self.element_stack = stack;
        if candidates.is_empty() {
            return;
        }

        let mut resolved = Vec::new();
        for style in candidates {
            let Some(sel) = style.selector.as_ref() else {
                continue;
            };
            let mut classes = Vec::new();
            sel.classes(&mut classes);
            let conditions: Vec<crate::triggers::ResolvedCondition> = classes
                .iter()
                .map(|c| crate::selector::pseudo_class_condition(c))
                .collect();
            let mut setters = Vec::new();
            for setter in &style.setters {
                if setter.owner.is_some() {
                    continue;
                }
                let Some(target) = crate::provider::property_target_for(&setter.property) else {
                    self.warn(format!(
                        "selector setter `{}` is not dynamically writable yet; skipped",
                        setter.property
                    ));
                    continue;
                };
                let value = match &setter.value {
                    crate::resources::PfSetterValue::Literal(text) => {
                        match self.literal_to_pf_value(target, text) {
                            Ok(v) => crate::triggers::TriggerValue::Static(Some(v)),
                            Err(e) => {
                                self.warn(format!("selector setter `{}`: {e}", setter.property));
                                continue;
                            }
                        }
                    }
                    crate::resources::PfSetterValue::Resource(key) => {
                        match self.scopes.lookup(key).cloned() {
                            Some(v) => crate::triggers::TriggerValue::Static(Some(v)),
                            None => {
                                self.warn(format!(
                                    "selector setter resource for `{}` not found",
                                    setter.property
                                ));
                                continue;
                            }
                        }
                    }
                    crate::resources::PfSetterValue::DynamicResource(key) => {
                        crate::triggers::TriggerValue::Dynamic(key.clone())
                    }
                    crate::resources::PfSetterValue::Null => {
                        crate::triggers::TriggerValue::Static(None)
                    }
                    crate::resources::PfSetterValue::Value(v) => {
                        crate::triggers::TriggerValue::Static(Some(v.clone()))
                    }
                    other => {
                        self.warn(format!(
                            "selector setter `{}`: {other:?} is not supported yet",
                            setter.property
                        ));
                        continue;
                    }
                };
                if matches!(
                    target,
                    crate::provider::PropertyTarget::Foreground
                        | crate::provider::PropertyTarget::FontSize
                ) {
                    self.ensure_inherited_seed(entity, target);
                }
                setters.push(crate::triggers::ResolvedTriggerSetter {
                    target,
                    value,
                    dest: None,
                    tier: crate::provider::ValueSource::ImplicitReference,
                });
            }
            if !setters.is_empty() {
                resolved.push(crate::triggers::ResolvedTrigger {
                    conditions,
                    setters,
                    enter_storyboards: Vec::new(),
                    exit_storyboards: Vec::new(),
                });
            }
        }
        if resolved.is_empty() {
            return;
        }
        // Merge, never replace: a WPF Style.Triggers block may already be
        // here, and the two live at different tiers without colliding.
        let mut e = self.world.entity_mut(entity);
        if let Some(mut existing) = e.get_mut::<crate::triggers::PfTriggers>() {
            let total = existing.triggers.len() + resolved.len();
            existing.triggers.extend(resolved);
            existing.active.resize(total, false);
        } else {
            let active = vec![false; resolved.len()];
            e.insert(crate::triggers::PfTriggers {
                triggers: resolved,
                active,
            });
        }
        // Pseudo-class conditions read Interaction, which only exists on
        // elements that opted into picking. Without it :pointerover and
        // :pressed would silently never fire.
        if !self.world.entity(entity).contains::<Interaction>() {
            self.world.entity_mut(entity).insert(Interaction::default());
        }
    }

    fn insert_defaults(&mut self, entity: Entity, kind: ElemKind, node: &XamlNode) {
        let single_cell = || Node {
            display: Display::Grid,
            grid_template_rows: vec![GridTrack::fr(1.0)],
            grid_template_columns: vec![GridTrack::fr(1.0)],
            ..Default::default()
        };

        let ui_node = match kind {
            ElemKind::Root => Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..single_cell()
            },
            ElemKind::Grid => Node {
                display: Display::Grid,
                ..Default::default()
            },
            ElemKind::StackPanel | ElemKind::DockPanel => Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Stretch,
                ..Default::default()
            },
            ElemKind::WrapPanel => Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                // WPF: each child's slot spans the full line extent (children
                // stretch to the line), and lines pack at natural size
                // (WrapPanel.cs:342-346; conformance note L7).
                align_items: AlignItems::Stretch,
                align_content: AlignContent::FlexStart,
                ..Default::default()
            },
            ElemKind::Canvas => Node::default(),
            ElemKind::Border => single_cell(),
            ElemKind::ScrollViewer => Node {
                overflow: Overflow::scroll_y(),
                ..single_cell()
            },
            ElemKind::Button | ElemKind::ToggleButton => Node {
                padding: UiRect::axes(Val::Px(12.0), Val::Px(4.0)),
                border: UiRect::all(Val::Px(1.0)),
                justify_items: JustifyItems::Center,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..single_cell()
            },
            ElemKind::Label => Node {
                padding: UiRect::all(Val::Px(5.0)),
                align_items: AlignItems::Center,
                ..single_cell()
            },
            ElemKind::CheckBox | ElemKind::RadioButton => Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                ..Default::default()
            },
            ElemKind::TextBox => Node {
                display: Display::Flex,
                align_items: AlignItems::Center,
                padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                border: UiRect::all(Val::Px(1.0)),
                min_width: Val::Px(80.0),
                min_height: Val::Px(24.0),
                ..Default::default()
            },
            ElemKind::ComboBox => Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                border: UiRect::all(Val::Px(1.0)),
                min_width: Val::Px(120.0),
                min_height: Val::Px(26.0),
                ..Default::default()
            },
            ElemKind::TabControl => Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                ..Default::default()
            },
            ElemKind::TreeView => Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                border: UiRect::all(Val::Px(1.0)),
                padding: UiRect::all(Val::Px(4.0)),
                ..Default::default()
            },
            ElemKind::Menu => Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                padding: UiRect::all(Val::Px(2.0)),
                ..Default::default()
            },
            ElemKind::DataGrid => Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                border: UiRect::all(Val::Px(1.0)),
                ..Default::default()
            },
            ElemKind::StatusBar => Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                width: Val::Percent(100.0),
                padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                column_gap: Val::Px(10.0),
                ..Default::default()
            },
            ElemKind::StatusBarItem => Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                padding: UiRect::axes(Val::Px(4.0), Val::Px(1.0)),
                ..Default::default()
            },
            ElemKind::ToolBar => Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                padding: UiRect::axes(Val::Px(4.0), Val::Px(2.0)),
                column_gap: Val::Px(4.0),
                border: UiRect::bottom(Val::Px(1.0)),
                ..Default::default()
            },
            ElemKind::ToolBarTray => Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                width: Val::Percent(100.0),
                ..Default::default()
            },
            ElemKind::Hyperlink => Node {
                display: Display::Flex,
                align_items: AlignItems::Center,
                ..Default::default()
            },
            ElemKind::Frame => Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                ..Default::default()
            },
            ElemKind::ToggleSwitch | ElemKind::NumericUpDown | ElemKind::RatingBar => Node {
                display: Display::Flex,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                ..Default::default()
            },
            ElemKind::Badge | ElemKind::BusyIndicator => single_cell(),
            ElemKind::RangeSlider => Node {
                height: Val::Px(20.0),
                min_width: Val::Px(120.0),
                ..Default::default()
            },
            // The <Popup> placeholder never lays out; its content lives on
            // the overlay layer.
            ElemKind::PopupElement => Node {
                display: Display::None,
                ..Default::default()
            },
            ElemKind::GridSplitter => Node {
                display: Display::Flex,
                min_width: Val::Px(4.0),
                min_height: Val::Px(4.0),
                ..Default::default()
            },
            ElemKind::Calendar => Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Val::Px(252.0),
                padding: UiRect::all(Val::Px(8.0)),
                border: UiRect::all(Val::Px(1.0)),
                row_gap: Val::Px(4.0),
                ..Default::default()
            },
            ElemKind::DatePicker => Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                min_width: Val::Px(140.0),
                min_height: Val::Px(26.0),
                padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                border: UiRect::all(Val::Px(1.0)),
                column_gap: Val::Px(6.0),
                ..Default::default()
            },
            ElemKind::TimePicker => Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                min_width: Val::Px(110.0),
                min_height: Val::Px(26.0),
                padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                border: UiRect::all(Val::Px(1.0)),
                column_gap: Val::Px(6.0),
                ..Default::default()
            },
            ElemKind::ColorPicker => Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                min_height: Val::Px(26.0),
                padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                border: UiRect::all(Val::Px(1.0)),
                column_gap: Val::Px(6.0),
                ..Default::default()
            },
            ElemKind::AutoSuggestBox => Node {
                display: Display::Flex,
                align_items: AlignItems::Center,
                padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                border: UiRect::all(Val::Px(1.0)),
                min_width: Val::Px(120.0),
                min_height: Val::Px(24.0),
                ..Default::default()
            },
            ElemKind::PackIcon => Node {
                width: Val::Px(16.0),
                height: Val::Px(16.0),
                flex_shrink: 0.0,
                ..Default::default()
            },
            ElemKind::NavigationView => Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Stretch,
                flex_grow: 1.0,
                ..Default::default()
            },
            ElemKind::ScrollBar => Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Val::Px(16.0),
                min_height: Val::Px(60.0),
                ..Default::default()
            },
            ElemKind::Track => Node {
                flex_grow: 1.0,
                position_type: PositionType::Relative,
                ..Default::default()
            },
            ElemKind::Thumb => Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(20.0),
                ..Default::default()
            },
            ElemKind::Slider => Node {
                display: Display::Flex,
                align_items: AlignItems::Center,
                height: Val::Px(20.0),
                min_width: Val::Px(60.0),
                ..Default::default()
            },
            ElemKind::ProgressBar => Node {
                display: Display::Flex,
                height: Val::Px(15.0),
                border: UiRect::all(Val::Px(1.0)),
                ..Default::default()
            },
            ElemKind::Separator => Node {
                height: Val::Px(1.0),
                margin: UiRect::axes(Val::Px(0.0), Val::Px(4.0)),
                ..Default::default()
            },
            ElemKind::ListBox => Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                border: UiRect::all(Val::Px(1.0)),
                padding: UiRect::all(Val::Px(1.0)),
                ..Default::default()
            },
            ElemKind::ListBoxItem => Node {
                padding: UiRect::axes(Val::Px(4.0), Val::Px(2.0)),
                ..single_cell()
            },
            ElemKind::ItemsControl => Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                ..Default::default()
            },
            // HeaderedContentControl shares GroupBox structure minus the
            // border chrome.
            ElemKind::GroupBox if node.name == "HeaderedContentControl" => Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                ..Default::default()
            },
            ElemKind::GroupBox => Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                padding: UiRect::all(Val::Px(10.0)),
                row_gap: Val::Px(6.0),
                ..Default::default()
            },
            ElemKind::Expander => Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                ..Default::default()
            },
            // Viewbox: the child keeps its natural size and is centered; a
            // post-layout system applies the scale (visual-only — WPF's
            // scale participates in measure, an accepted deviation for now).
            ElemKind::Viewbox => Node {
                display: Display::Flex,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                overflow: bevy::ui::Overflow::clip(),
                ..Default::default()
            },
            ElemKind::UniformGrid => Node {
                display: Display::Grid,
                ..Default::default()
            },
            ElemKind::ContentPresenter => single_cell(),
            ElemKind::TextBlock | ElemKind::Image | ElemKind::Shape | ElemKind::Unknown => {
                Node::default()
            }
        };

        let control_theme = self.control_theme();
        let mut e = self.world.entity_mut(entity);
        e.insert((ui_node, PfElementKind(node.name.clone())));

        // WPF: a `Panel` renders nothing but its Background, so with the
        // default null Background a click passes straight through it. bevy
        // has no such rule — every node blocks picks — which is why a bare
        // layout Grid wrapped around a screen silently eats every click
        // aimed at what is behind it. Panels therefore START opted out and
        // are opted back IN by `provider::apply_value` the moment any
        // Background brush is assigned, `Transparent` included: that is
        // precisely what makes `Background="Transparent"` mean "invisible
        // but clickable" here as it does in WPF.
        if crate::hit_test::background_governs_hit_testing(&node.name) {
            e.insert(bevy::picking::Pickable::IGNORE);
        }

        match kind {
            ElemKind::Root if node.name == "Window" => {
                let mut store = crate::provider::PfPropertyStore::default();
                store.set(
                    crate::provider::PropertyTarget::Background,
                    crate::provider::ValueSource::Default,
                    Some(PfValue::Color(v::PfColor::WHITE)),
                );
                e.insert((BackgroundColor(Color::WHITE), store));
            }
            ElemKind::Button | ElemKind::ToggleButton => {
                let visual = ButtonVisual::default();
                // Seed the Default tier so trigger revert restores chrome.
                let mut store = crate::provider::PfPropertyStore::default();
                store.set(
                    crate::provider::PropertyTarget::Background,
                    crate::provider::ValueSource::Default,
                    Some(PfValue::Color(v::PfColor::rgb(0xE1, 0xE1, 0xE1))),
                );
                store.set(
                    crate::provider::PropertyTarget::BorderBrush,
                    crate::provider::ValueSource::Default,
                    Some(PfValue::Color(v::PfColor::rgb(0xAD, 0xAD, 0xAD))),
                );
                e.insert((
                    bevy::ui::widget::Button,
                    Interaction::default(),
                    BackgroundColor(visual.normal_bg),
                    BorderColor::all(visual.normal_border),
                    visual,
                    store,
                ));
                if kind == ElemKind::ToggleButton {
                    e.insert((PfToggleButton, bevy::ui_widgets::Checkbox));
                }
            }
            ElemKind::TextBox => {
                e.insert((
                    BackgroundColor(Color::WHITE),
                    BorderColor::all(Color::srgb_u8(0x7A, 0x7A, 0x7A)),
                    Interaction::default(),
                ));
            }
            ElemKind::ComboBox => {
                e.insert((
                    BackgroundColor(control_theme.control_face),
                    BorderColor::all(control_theme.control_border),
                    Interaction::default(),
                ));
            }
            ElemKind::ProgressBar => {
                e.insert((
                    BackgroundColor(Color::srgb_u8(0xE6, 0xE6, 0xE6)),
                    BorderColor::all(Color::srgb_u8(0xBC, 0xBC, 0xBC)),
                ));
            }
            ElemKind::Separator => {
                e.insert(BackgroundColor(Color::srgb_u8(0xD0, 0xD0, 0xD0)));
            }
            ElemKind::ListBox => {
                e.insert((
                    BackgroundColor(Color::WHITE),
                    BorderColor::all(Color::srgb_u8(0xAD, 0xAD, 0xAD)),
                    PfListBox::default(),
                ));
            }
            ElemKind::TreeView => {
                e.insert((
                    BackgroundColor(Color::WHITE),
                    BorderColor::all(Color::srgb_u8(0xAD, 0xAD, 0xAD)),
                    crate::components::PfTreeView::default(),
                ));
            }
            ElemKind::Menu => {
                e.insert(BackgroundColor(Color::srgb_u8(0xF0, 0xF0, 0xF0)));
            }
            ElemKind::DataGrid => {
                e.insert((
                    BackgroundColor(Color::WHITE),
                    BorderColor::all(Color::srgb_u8(0xAD, 0xAD, 0xAD)),
                ));
            }
            ElemKind::GroupBox => {
                e.insert(BorderColor::all(Color::srgb_u8(0xD5, 0xD5, 0xD5)));
            }
            ElemKind::StatusBar => {
                e.insert(BackgroundColor(Color::srgb_u8(0xF0, 0xF0, 0xF0)));
            }
            ElemKind::ToolBar => {
                e.insert((
                    BackgroundColor(Color::srgb_u8(0xF5, 0xF5, 0xF5)),
                    BorderColor::all(Color::srgb_u8(0xD5, 0xD5, 0xD5)),
                ));
            }
            ElemKind::ToolBarTray => {
                e.insert(BackgroundColor(Color::srgb_u8(0xEB, 0xEB, 0xEB)));
            }
            ElemKind::GridSplitter => {
                e.insert((
                    BackgroundColor(Color::srgb_u8(0xC8, 0xC8, 0xC8)),
                    Interaction::default(),
                ));
            }
            ElemKind::Calendar
            | ElemKind::DatePicker
            | ElemKind::TimePicker
            | ElemKind::ColorPicker
            | ElemKind::AutoSuggestBox => {
                e.insert((
                    BackgroundColor(Color::WHITE),
                    BorderColor::all(Color::srgb_u8(0xAD, 0xAD, 0xAD)),
                ));
            }
            _ => {}
        }
    }

    fn effective_style(&mut self, node: &XamlNode) -> Option<std::sync::Arc<PfStyle>> {
        match node.attribute("Style") {
            Some(XamlValue::Extension(ext))
                if ext.name == "StaticResource" || ext.name == "DynamicResource" =>
            {
                match static_resource_key(ext) {
                    Ok(key) => match self.scopes.lookup(&key) {
                        Some(PfValue::Style(s)) => Some(s.clone()),
                        _ => {
                            self.warn(format!(
                                "{}: style resource not found for `{}`",
                                node.pos, node.name
                            ));
                            None
                        }
                    },
                    Err(e) => {
                        self.warn(format!("{}: {e}", node.pos));
                        None
                    }
                }
            }
            Some(_) => None,
            None => self.scopes.implicit_style(&node.name),
        }
    }

    fn apply_setter(
        &mut self,
        entity: Entity,
        kind: ElemKind,
        parent_kind: ParentKind,
        setter: &PfSetter,
    ) {
        let saved_tier = self.tier;
        self.tier = crate::provider::ValueSource::Style;
        self.apply_setter_inner(entity, kind, parent_kind, setter);
        self.tier = saved_tier;
    }

    fn apply_setter_inner(
        &mut self,
        entity: Entity,
        kind: ElemKind,
        parent_kind: ParentKind,
        setter: &PfSetter,
    ) {
        let result = match &setter.value {
            PfSetterValue::Literal(s) => self.apply_property(
                entity,
                kind,
                parent_kind,
                setter.owner.as_deref(),
                &setter.property,
                &Resolved::Str(s),
            ),
            PfSetterValue::Resource(key) => match self.scopes.lookup(key) {
                Some(value) => {
                    let value = value.clone();
                    self.apply_property(
                        entity,
                        kind,
                        parent_kind,
                        setter.owner.as_deref(),
                        &setter.property,
                        &Resolved::Value(value),
                    )
                }
                None => Err(PfError::resource(format!(
                    "setter resource for `{}` not found",
                    setter.property
                ))),
            },
            PfSetterValue::DynamicResource(key) if setter.owner.is_none() => self
                .apply_dynamic_reference(
                    entity,
                    kind,
                    parent_kind,
                    &setter.property.clone(),
                    key.clone(),
                ),
            PfSetterValue::DynamicResource(_) => Err(PfError::resource(
                "DynamicResource setters on attached properties are not supported yet",
            )),
            PfSetterValue::AppTheme(raw) if setter.owner.is_none() => {
                let property = setter.property.clone();
                self.apply_app_theme_reference(entity, kind, parent_kind, &property, raw)
            }
            PfSetterValue::AppTheme(_) => Err(PfError::resource(
                "AppThemeBinding setters on attached properties are not supported yet",
            )),
            // Static for the life of the process: pick now and apply as if
            // the picked arm had been written directly.
            PfSetterValue::Selector(selector, arms) => {
                let device = crate::device::device(self.world);
                let property = setter.property.clone();
                match arms.pick(&device, *selector).cloned() {
                    Some(crate::resources::RawArm::Text(s)) => self.apply_property(
                        entity,
                        kind,
                        parent_kind,
                        setter.owner.as_deref(),
                        &property,
                        &Resolved::Str(&s),
                    ),
                    Some(crate::resources::RawArm::Static(k)) => {
                        match self.scopes.lookup(&k).cloned() {
                            Some(v) => self.apply_property(
                                entity,
                                kind,
                                parent_kind,
                                setter.owner.as_deref(),
                                &property,
                                &Resolved::Value(v),
                            ),
                            None => Err(PfError::resource(format!(
                                "{} resource `{k:?}` not found",
                                selector.extension_name()
                            ))),
                        }
                    }
                    Some(crate::resources::RawArm::Dynamic(k)) if setter.owner.is_none() => {
                        self.apply_dynamic_reference(entity, kind, parent_kind, &property, k)
                    }
                    Some(crate::resources::RawArm::Dynamic(_)) => Err(PfError::resource(
                        "DynamicResource setters on attached properties are not supported yet",
                    )),
                    Some(crate::resources::RawArm::Null) | None => {
                        if let Some(target) = crate::provider::property_target_for(&property) {
                            crate::provider::store_and_apply(
                                self.world, entity, target, self.tier, None,
                            );
                        }
                        Ok(())
                    }
                }
            }
            // {x:Null}: masks lower tiers in the store; the effective value
            // becomes "unset" and components clear accordingly.
            PfSetterValue::Null => {
                if let Some(target) = crate::provider::property_target_for(&setter.property) {
                    crate::provider::store_and_apply(self.world, entity, target, self.tier, None);
                }
                Ok(())
            }
            PfSetterValue::Value(value) => self.apply_property(
                entity,
                kind,
                parent_kind,
                setter.owner.as_deref(),
                &setter.property,
                &Resolved::Value(value.clone()),
            ),
            // A plain Style setter has no templated parent to read from; WPF
            // and MAUI both reject this, so say where it IS valid rather than
            // leaving a property mysteriously unset.
            PfSetterValue::TemplatedParent(src) => Err(PfError::resource(format!(
                "`{{TemplateBinding {src}}}` in a Setter.Value is only valid inside \
                 ControlTemplate.Triggers, which has a templated parent to read"
            ))),
        };
        if let Err(e) = result {
            self.warn(format!("setter `{}`: {e}", setter.property));
        }
    }

    fn apply_xaml_value(
        &mut self,
        entity: Entity,
        kind: ElemKind,
        parent_kind: ParentKind,
        owner: Option<&str>,
        name: &str,
        value: &XamlValue,
    ) {
        // Accessibility metadata: AutomationId becomes a queryable component;
        // the other AutomationProperties.* are accepted without warnings.
        if owner == Some("AutomationProperties") {
            if name == "AutomationId"
                && let XamlValue::Str(s) = value
            {
                self.world
                    .entity_mut(entity)
                    .insert(crate::components::PfAutomationId(s.clone()));
            }
            return;
        }

        let result = match value {
            XamlValue::Str(s) => {
                self.apply_property(entity, kind, parent_kind, owner, name, &Resolved::Str(s))
            }
            XamlValue::Extension(ext)
                if matches!(name, "Command" | "CommandParameter")
                    && matches!(ext.name.as_str(), "Binding" | "x:Bind" | "CompiledBinding") =>
            {
                let spec = crate::binding::parse_binding_extension(ext);
                if name == "Command" {
                    // WPF resolves the command object through the binding;
                    // here the path names the registered command.
                    self.pending.command = Some(spec.path);
                } else {
                    self.pending.command_parameter =
                        Some(crate::binding::PfCommandParameter::Path(spec.path));
                }
                Ok(())
            }
            XamlValue::Extension(ext)
                if matches!(ext.name.as_str(), "Binding" | "x:Bind" | "CompiledBinding") =>
            {
                self.record_binding(name, ext);
                Ok(())
            }
            XamlValue::Extension(ext) if ext.name == "DynamicResource" && owner.is_none() => {
                match static_resource_key(ext) {
                    Ok(key) => self.apply_dynamic_reference(entity, kind, parent_kind, name, key),
                    Err(e) => Err(e),
                }
            }
            // `{x:Null}` on a store-managed property MASKS the tiers below,
            // exactly as the setter form has always done. Left to
            // `resolve_extension` it reaches the per-property converters
            // instead, and `Background="{x:Null}"` — valid in both WPF and
            // MAUI — warns "expected a brush value" and paints nothing.
            XamlValue::Extension(ext)
                if (ext.name == "x:Null" || ext.name == "Null")
                    && owner.is_none()
                    && crate::provider::property_target_for(name).is_some() =>
            {
                let target = crate::provider::property_target_for(name).expect("just checked");
                crate::provider::store_and_apply(self.world, entity, target, self.tier, None);
                Ok(())
            }
            // Handled here rather than left to `resolve_extension` so that a
            // {DynamicResource} arm stays LIVE: picking the arm is static,
            // but what the picked key resolves to is not.
            XamlValue::Extension(ext)
                if (ext.name == "OnPlatform" || ext.name == "OnIdiom") && owner.is_none() =>
            {
                let selector = if ext.name == "OnPlatform" {
                    crate::device::SelectorKind::Platform
                } else {
                    crate::device::SelectorKind::Idiom
                };
                match crate::resources::parse_selector_arms(ext, selector) {
                    Ok(arms) => {
                        let device = crate::device::device(self.world);
                        match arms.pick(&device, selector).cloned() {
                            Some(crate::resources::RawArm::Dynamic(key)) => {
                                self.apply_dynamic_reference(entity, kind, parent_kind, name, key)
                            }
                            Some(crate::resources::RawArm::Text(s)) => self.apply_property(
                                entity,
                                kind,
                                parent_kind,
                                None,
                                name,
                                &Resolved::Str(&s),
                            ),
                            Some(crate::resources::RawArm::Static(k)) => {
                                match self.scopes.lookup(&k).cloned() {
                                    Some(v) => self.apply_property(
                                        entity,
                                        kind,
                                        parent_kind,
                                        None,
                                        name,
                                        &Resolved::Value(v),
                                    ),
                                    None => Err(PfError::resource(format!(
                                        "{} resource `{k:?}` not found",
                                        selector.extension_name()
                                    ))),
                                }
                            }
                            // Nothing matched and there is no Default. On a
                            // store-managed property that MASKS the lower
                            // tiers, which is the crate's {x:Null}; elsewhere
                            // there is no entry to mask, so leave it alone.
                            Some(crate::resources::RawArm::Null) | None => {
                                if let Some(target) = crate::provider::property_target_for(name) {
                                    crate::provider::store_and_apply(
                                        self.world, entity, target, self.tier, None,
                                    );
                                }
                                Ok(())
                            }
                        }
                    }
                    Err(e) => Err(e),
                }
            }
            XamlValue::Extension(ext) if ext.name == "AppThemeBinding" && owner.is_none() => {
                match crate::resources::parse_app_theme_arms(ext) {
                    Ok(raw) => {
                        self.apply_app_theme_reference(entity, kind, parent_kind, name, &raw)
                    }
                    Err(e) => Err(e),
                }
            }
            XamlValue::Extension(ext) if ext.name == "TemplateBinding" => {
                self.apply_template_binding(entity, name, ext);
                Ok(())
            }
            XamlValue::Extension(ext) => match self.resolve_extension(ext) {
                Ok(Some(resolved)) => {
                    self.apply_property(entity, kind, parent_kind, owner, name, &resolved)
                }
                Ok(None) => Ok(()), // unsupported extension, already warned
                Err(e) => Err(e),
            },
        };
        if let Err(e) = result {
            self.warn(format!("property `{name}`: {e}"));
        }
    }

    /// Write a store-managed property at the current tier and apply the
    /// effective value.
    fn store_apply(
        &mut self,
        entity: Entity,
        target: crate::provider::PropertyTarget,
        value: PfValue,
    ) {
        crate::provider::store_and_apply(self.world, entity, target, self.tier, Some(value));
    }

    /// Record in the store without applying (text properties: components are
    /// created later from the inherited state; the store copy is the revert
    /// base for triggers and dynamic refresh).
    fn store_set_only(
        &mut self,
        entity: Entity,
        target: crate::provider::PropertyTarget,
        value: PfValue,
    ) {
        let tier = self.tier;
        let mut e = self.world.entity_mut(entity);
        if let Some(mut store) = e.get_mut::<crate::provider::PfPropertyStore>() {
            store.set(target, tier, Some(value));
        } else {
            let mut store = crate::provider::PfPropertyStore::default();
            store.set(target, tier, Some(value));
            e.insert(store);
        }
    }

    /// Seed the Inherited tier for text properties so trigger/dynamic revert
    /// has a base to fall back to.
    fn ensure_inherited_seed(&mut self, entity: Entity, target: crate::provider::PropertyTarget) {
        use crate::provider::{PfPropertyStore, PropertyTarget, ValueSource};
        let value = match target {
            PropertyTarget::Foreground => {
                PfValue::Brush(v::PfBrush::Solid(self.inherited.foreground))
            }
            PropertyTarget::FontSize => PfValue::Double(self.inherited.font_size as f64),
            _ => return,
        };
        let mut e = self.world.entity_mut(entity);
        if let Some(mut store) = e.get_mut::<PfPropertyStore>() {
            if store.effective(target).is_none() {
                store.set(target, ValueSource::Inherited, Some(value));
            }
        } else {
            let mut store = PfPropertyStore::default();
            store.set(target, ValueSource::Inherited, Some(value));
            e.insert(store);
        }
    }

    /// Convert a trigger-setter literal into a typed value for its target.
    fn literal_to_pf_value(
        &self,
        target: crate::provider::PropertyTarget,
        s: &str,
    ) -> Result<PfValue, PfError> {
        use crate::provider::PropertyTarget as T;
        Ok(match target {
            T::Background | T::BorderBrush | T::Foreground | T::Fill | T::Stroke => {
                PfValue::Brush(s.parse()?)
            }
            T::BorderThickness | T::Margin | T::Padding => PfValue::Thickness(s.parse()?),
            T::CornerRadius => PfValue::CornerRadius(s.parse()?),
            T::Width | T::Height | T::FontSize | T::Opacity => {
                PfValue::Double(Resolved::Str(s).to_f32()? as f64)
            }
            T::Visibility => PfValue::String(s.to_string()),
        })
    }

    /// Resolve a style's triggers against the current scopes and attach the
    /// runtime component.
    fn attach_triggers(
        &mut self,
        entity: Entity,
        triggers: &[crate::resources::PfTrigger],
        template_parts: Option<&crate::components::PfTemplateParts>,
    ) {
        use crate::provider::PropertyTarget;
        use crate::triggers::{
            PfTriggers, ResolvedCondition, ResolvedTrigger, ResolvedTriggerSetter, TriggerValue,
        };

        let mut resolved: Vec<ResolvedTrigger> = Vec::new();
        let mut needs_interaction = false;

        'triggers: for trigger in triggers {
            let mut conditions = Vec::new();
            for cond in &trigger.conditions {
                match cond {
                    crate::resources::PfTriggerCondition::Property { property, value } => {
                        let crate::resources::PfTriggerValue::Text(value) = value else {
                            self.warn(format!(
                                "trigger condition `{property}={{x:Null}}` (three-state) is not supported; trigger skipped"
                            ));
                            continue 'triggers;
                        };
                        let expected = value.trim().eq_ignore_ascii_case("true");
                        // Class-qualified condition properties from theme
                        // markup ("ToggleButton.IsChecked") match by their
                        // unqualified name, like WPF's owner resolution.
                        let unqualified = property.rsplit('.').next().unwrap_or(property.as_str());
                        let condition = match unqualified {
                            "IsMouseOver" => {
                                needs_interaction = true;
                                ResolvedCondition::MouseOver(expected)
                            }
                            "IsPressed" => {
                                needs_interaction = true;
                                ResolvedCondition::Pressed(expected)
                            }
                            // IsExpanded state rides the Checked component.
                            "IsChecked" | "IsExpanded" => ResolvedCondition::Checked(expected),
                            "IsEnabled" => ResolvedCondition::Enabled(expected),
                            // Focus lives in the InputFocus resource rather
                            // than a component, so unlike IsMouseOver this
                            // needs no interaction opt-in.
                            "IsFocused" | "IsKeyboardFocusWithin" => {
                                ResolvedCondition::Focused(expected)
                            }
                            "IsSelected" => ResolvedCondition::Selected(expected),
                            _ => {
                                self.warn(format!(
                                    "trigger condition `{property}` is not supported yet; trigger skipped"
                                ));
                                continue 'triggers;
                            }
                        };
                        conditions.push(condition);
                    }
                    crate::resources::PfTriggerCondition::Data { path, value } => {
                        let crate::resources::PfTriggerValue::Text(value) = value else {
                            self.warn(
                                "DataTrigger Value={x:Null} is not supported; trigger skipped"
                                    .to_string(),
                            );
                            continue 'triggers;
                        };
                        conditions.push(ResolvedCondition::Data {
                            path: path.clone(),
                            expected: value.clone(),
                        });
                    }
                    crate::resources::PfTriggerCondition::VisualState { group, state } => {
                        // Synthesised while parsing `<VisualState.Setters>`;
                        // it carries no user-facing value to validate.
                        conditions.push(ResolvedCondition::VisualState {
                            group: group.clone(),
                            state: state.clone(),
                        });
                    }
                }
            }

            let mut setters = Vec::new();
            for setter in &trigger.setters {
                if setter.owner.is_some() {
                    self.warn(format!(
                        "trigger setter on attached `{}` is not supported yet",
                        setter.property
                    ));
                    continue;
                }
                let Some(target) = crate::provider::property_target_for(&setter.property) else {
                    self.warn(format!(
                        "trigger setter `{}` is not dynamically writable yet; skipped",
                        setter.property
                    ));
                    continue;
                };
                let value = match &setter.value {
                    PfSetterValue::Literal(text) => match self.literal_to_pf_value(target, text) {
                        Ok(v) => TriggerValue::Static(Some(v)),
                        Err(e) => {
                            self.warn(format!("trigger setter `{}`: {e}", setter.property));
                            continue;
                        }
                    },
                    PfSetterValue::Resource(key) => match self.scopes.lookup(key).cloned() {
                        Some(v) => TriggerValue::Static(Some(v)),
                        None => {
                            self.warn(format!(
                                "trigger setter resource for `{}` not found",
                                setter.property
                            ));
                            continue;
                        }
                    },
                    PfSetterValue::DynamicResource(key) => TriggerValue::Dynamic(key.clone()),
                    PfSetterValue::Null => TriggerValue::Static(None),
                    PfSetterValue::Value(v) => TriggerValue::Static(Some(v.clone())),
                    // Picked once: neither the platform nor the idiom moves.
                    PfSetterValue::Selector(selector, arms) => {
                        let device = crate::device::device(self.world);
                        match arms.pick(&device, *selector) {
                            Some(crate::resources::RawArm::Text(s)) => {
                                match self.literal_to_pf_value(target, s) {
                                    Ok(v) => TriggerValue::Static(Some(v)),
                                    Err(e) => {
                                        self.warn(format!(
                                            "trigger setter `{}`: {e}",
                                            setter.property
                                        ));
                                        continue;
                                    }
                                }
                            }
                            Some(crate::resources::RawArm::Static(k)) => {
                                match self.scopes.lookup(k).cloned() {
                                    Some(v) => TriggerValue::Static(Some(v)),
                                    None => {
                                        self.warn(format!(
                                            "trigger setter `{}`: resource `{k:?}` not found",
                                            setter.property
                                        ));
                                        continue;
                                    }
                                }
                            }
                            Some(crate::resources::RawArm::Dynamic(k)) => {
                                TriggerValue::Dynamic(k.clone())
                            }
                            Some(crate::resources::RawArm::Null) | None => {
                                TriggerValue::Static(None)
                            }
                        }
                    }
                    // Converted now, picked at activation — and re-picked on a
                    // theme change even while the trigger stays active.
                    PfSetterValue::AppTheme(raw) => {
                        use crate::app_theme::{ThemeArm, ThemeArms};
                        use crate::resources::RawArm;
                        let mut arms = ThemeArms::default();
                        let mut failed = None;
                        for (raw_arm, slot) in [
                            (&raw.light, &mut arms.light),
                            (&raw.dark, &mut arms.dark),
                            (&raw.default, &mut arms.default),
                        ] {
                            let Some(raw_arm) = raw_arm else { continue };
                            let converted = match raw_arm {
                                RawArm::Text(s) => match self.literal_to_pf_value(target, s) {
                                    Ok(v) => Ok(ThemeArm::Value(Some(v))),
                                    Err(e) => Err(e.to_string()),
                                },
                                RawArm::Null => Ok(ThemeArm::Value(None)),
                                RawArm::Static(k) => match self.scopes.lookup(k).cloned() {
                                    Some(v) => Ok(ThemeArm::Value(Some(v))),
                                    None => Err(format!("resource `{k:?}` not found")),
                                },
                                RawArm::Dynamic(k) => Ok(ThemeArm::Dynamic(k.clone())),
                            };
                            match converted {
                                Ok(v) => *slot = Some(v),
                                Err(e) => {
                                    failed = Some(e);
                                    break;
                                }
                            }
                        }
                        if let Some(e) = failed {
                            self.warn(format!("trigger setter `{}`: {e}", setter.property));
                            continue;
                        }
                        TriggerValue::AppTheme(arms)
                    }
                    // Deferred to activation rather than read here: at this
                    // point the templated parent is still being built and its
                    // own Background/BorderBrush may not be stored yet.
                    PfSetterValue::TemplatedParent(src_property) => {
                        if template_parts.is_none() {
                            self.warn(format!(
                                "trigger setter `{}` reads the templated parent outside a \
                                 ControlTemplate; skipped",
                                setter.property
                            ));
                            continue;
                        }
                        match crate::provider::property_target_for(src_property) {
                            Some(src) => TriggerValue::TemplatedParent(src),
                            None => {
                                self.warn(format!(
                                    "trigger setter `{}` reads `{src_property}` off the templated \
                                     parent, which is outside the store-managed property set; skipped",
                                    setter.property
                                ));
                                continue;
                            }
                        }
                    }
                };
                // Tier + destination: style triggers own tier 7 on the
                // root; ControlTemplate triggers own tier 6 on the root and
                // tier 10 on TargetName-resolved template children.
                // A visual state's setters take the VisualState tier
                // wherever they sit, template or not: what makes them
                // different from a Style trigger is that they must beat a
                // local value, and that is true of a templated control as
                // much as a bare one.
                let from_visual_state = trigger
                    .conditions
                    .iter()
                    .any(|c| matches!(c, crate::resources::PfTriggerCondition::VisualState { .. }));
                let (dest, tier) = match (template_parts, &setter.target_name) {
                    (None, Some(name)) => {
                        self.warn(format!(
                            "trigger setter TargetName `{name}` outside a ControlTemplate; skipped"
                        ));
                        continue;
                    }
                    (None, None) if from_visual_state => {
                        (None, crate::provider::ValueSource::VisualState)
                    }
                    (None, None) => (None, crate::provider::ValueSource::StyleTrigger),
                    (Some(_), None) => (None, crate::provider::ValueSource::TemplateTrigger),
                    (Some(parts), Some(name)) => match parts.get(name) {
                        Some(dest) => (
                            Some(dest),
                            crate::provider::ValueSource::ParentTemplateTrigger,
                        ),
                        None => {
                            self.warn(format!(
                                "trigger setter TargetName `{name}` does not match a template x:Name; skipped"
                            ));
                            continue;
                        }
                    },
                };
                if matches!(
                    target,
                    PropertyTarget::Foreground | PropertyTarget::FontSize
                ) {
                    self.ensure_inherited_seed(dest.unwrap_or(entity), target);
                }
                setters.push(ResolvedTriggerSetter {
                    target,
                    value,
                    dest,
                    tier,
                });
            }
            // A trigger can carry only Enter/ExitActions (no setters).
            if !setters.is_empty()
                || !trigger.enter_storyboards.is_empty()
                || !trigger.exit_storyboards.is_empty()
            {
                resolved.push(ResolvedTrigger {
                    conditions,
                    setters,
                    enter_storyboards: trigger.enter_storyboards.clone(),
                    exit_storyboards: trigger.exit_storyboards.clone(),
                });
            }
        }

        if resolved.is_empty() {
            return;
        }
        let mut e = self.world.entity_mut(entity);
        if needs_interaction && e.get::<Interaction>().is_none() {
            e.insert(Interaction::default());
        }
        // MERGE into an existing block (a control can carry Style.Triggers
        // AND ControlTemplate.Triggers): insert would silently clobber.
        // Cross-block ordering is irrelevant — their setters occupy
        // disjoint tiers, and the store's max-by-tier decides.
        if let Some(mut existing) = e.get_mut::<PfTriggers>() {
            existing
                .active
                .extend(std::iter::repeat_n(false, resolved.len()));
            existing.triggers.extend(resolved);
        } else {
            let count = resolved.len();
            e.insert(PfTriggers {
                triggers: resolved,
                active: vec![false; count],
            });
        }
    }

    /// Parse and wire `<Interaction.Triggers>` behavior triggers.
    fn apply_behavior_triggers(&mut self, entity: Entity, pe: &bevy_pf_xaml::XamlPropertyElement) {
        use crate::behaviors::PfAction;
        for trigger in pe.elements() {
            // KeyTrigger shares the action grammar; its dispatch differs.
            let active_on_focus = matches!(
                trigger.attribute("ActiveOnFocus"),
                Some(XamlValue::Str(v)) if v.eq_ignore_ascii_case("true")
            );
            let key = if trigger.name == "KeyTrigger" {
                let name = match trigger.attribute("Key") {
                    Some(XamlValue::Str(k)) => k.clone(),
                    _ => {
                        self.warn(format!("{}: KeyTrigger needs Key=", trigger.pos));
                        continue;
                    }
                };
                match crate::behaviors::key_from_name(&name) {
                    Some(kc) => Some(kc),
                    None => {
                        self.warn(format!(
                            "{}: KeyTrigger Key `{name}` is not mapped yet",
                            trigger.pos
                        ));
                        continue;
                    }
                }
            } else {
                None
            };
            if trigger.name != "EventTrigger" && key.is_none() {
                self.warn(format!(
                    "{}: behavior trigger `{}` is not supported yet",
                    trigger.pos, trigger.name
                ));
                continue;
            }
            let event = if key.is_some() {
                String::new()
            } else {
                match trigger.attribute("EventName") {
                    Some(XamlValue::Str(e)) => e.clone(),
                    _ => {
                        self.warn(format!(
                            "{}: behavior EventTrigger needs EventName",
                            trigger.pos
                        ));
                        continue;
                    }
                }
            };

            let mut actions = Vec::new();
            for action in trigger.child_elements() {
                let attr = |name: &str| -> Option<String> {
                    match action.attribute(name) {
                        Some(XamlValue::Str(v)) => Some(v.clone()),
                        _ => None,
                    }
                };
                match action.name.as_str() {
                    "InvokeCommandAction" => {
                        let name = match action.attribute("Command") {
                            Some(XamlValue::Str(c)) => Some(c.clone()),
                            Some(XamlValue::Extension(ext))
                                if matches!(
                                    ext.name.as_str(),
                                    "Binding" | "x:Bind" | "CompiledBinding"
                                ) =>
                            {
                                Some(crate::binding::parse_binding_extension(ext).path)
                            }
                            _ => None,
                        };
                        let Some(name) = name else {
                            self.warn(format!("{}: InvokeCommandAction needs Command", action.pos));
                            continue;
                        };
                        let parameter = match action.attribute("CommandParameter") {
                            Some(XamlValue::Str(p)) => {
                                Some(crate::binding::PfCommandParameter::Literal(p.clone()))
                            }
                            Some(XamlValue::Extension(ext))
                                if matches!(
                                    ext.name.as_str(),
                                    "Binding" | "x:Bind" | "CompiledBinding"
                                ) =>
                            {
                                Some(crate::binding::PfCommandParameter::Path(
                                    crate::binding::parse_binding_extension(ext).path,
                                ))
                            }
                            _ => None,
                        };
                        actions.push(PfAction::InvokeCommand { name, parameter });
                    }
                    "ControlStoryboardAction" => {
                        let sb = match action.attribute("Storyboard") {
                            Some(XamlValue::Extension(ext)) => static_resource_key(ext)
                                .ok()
                                .and_then(|key| self.scopes.lookup(&key).cloned())
                                .and_then(|v| match v {
                                    PfValue::Storyboard(sb) => Some(sb),
                                    _ => None,
                                }),
                            _ => None,
                        };
                        match sb {
                            Some(storyboard) => {
                                actions.push(PfAction::ControlStoryboard { storyboard })
                            }
                            None => self.warn(format!(
                                "{}: ControlStoryboardAction Storyboard resource not found",
                                action.pos
                            )),
                        }
                    }
                    "GoToStateAction" => match attr("StateName") {
                        Some(state) => actions.push(PfAction::GoToState { state }),
                        None => {
                            self.warn(format!("{}: GoToStateAction needs StateName", action.pos))
                        }
                    },
                    "ChangePropertyAction" => {
                        let (Some(property), Some(value)) = (attr("PropertyName"), attr("Value"))
                        else {
                            self.warn(format!(
                                "{}: ChangePropertyAction needs PropertyName and Value",
                                action.pos
                            ));
                            continue;
                        };
                        actions.push(PfAction::ChangeProperty {
                            target_name: attr("TargetName"),
                            property,
                            value,
                        });
                    }
                    "LaunchUriOrFileAction" => match attr("Path") {
                        Some(path) => actions.push(PfAction::LaunchUri { path }),
                        None => {
                            self.warn(format!("{}: LaunchUriOrFileAction needs Path", action.pos))
                        }
                    },
                    "SetFocusAction" => {
                        actions.push(PfAction::SetFocus {
                            target_name: attr("TargetName"),
                        });
                    }
                    "PlaySoundAction" => match attr("Source") {
                        Some(path) => actions.push(PfAction::PlaySound {
                            path,
                            volume: attr("Volume")
                                .and_then(|v| v.trim().parse().ok())
                                .unwrap_or(1.0),
                        }),
                        None => self.warn(format!("{}: PlaySoundAction needs Source", action.pos)),
                    },
                    other => self.warn(format!(
                        "{}: behavior action `{other}` is not supported yet",
                        action.pos
                    )),
                }
            }
            if actions.is_empty() {
                continue;
            }

            if let Some(key) = key {
                let mut e = self.world.entity_mut(entity);
                if let Some(mut triggers) = e.get_mut::<crate::behaviors::PfKeyTriggers>() {
                    triggers.0.push((key, active_on_focus, actions));
                } else {
                    e.insert(crate::behaviors::PfKeyTriggers(vec![(
                        key,
                        active_on_focus,
                        actions,
                    )]));
                }
                continue;
            }

            match event.as_str() {
                "Loaded" => {
                    let mut e = self.world.entity_mut(entity);
                    if let Some(mut pending) = e.get_mut::<crate::behaviors::PfPendingActions>() {
                        pending.0.push(actions);
                    } else {
                        e.insert(crate::behaviors::PfPendingActions(vec![actions]));
                    }
                }
                "Click" => {
                    self.world.entity_mut(entity).observe(
                        move |click: On<Pointer<Click>>, mut commands: Commands| {
                            let actions = actions.clone();
                            let host = click.entity;
                            commands.queue(move |world: &mut World| {
                                crate::behaviors::run_actions(world, host, &actions);
                            });
                        },
                    );
                }
                "MouseEnter" => {
                    self.world.entity_mut(entity).observe(
                        move |over: On<Pointer<Over>>, mut commands: Commands| {
                            let actions = actions.clone();
                            let host = over.entity;
                            commands.queue(move |world: &mut World| {
                                crate::behaviors::run_actions(world, host, &actions);
                            });
                        },
                    );
                }
                "MouseLeave" => {
                    self.world.entity_mut(entity).observe(
                        move |out: On<Pointer<Out>>, mut commands: Commands| {
                            let actions = actions.clone();
                            let host = out.entity;
                            commands.queue(move |world: &mut World| {
                                crate::behaviors::run_actions(world, host, &actions);
                            });
                        },
                    );
                }
                other => self.warn(format!("behavior EventName `{other}` is not supported yet")),
            }
        }
    }

    /// Wire `EventTrigger` storyboard launchers: `Loaded` defers one frame
    /// (the namescope component must exist); pointer events start on the
    /// corresponding picking observers.
    fn attach_event_triggers(
        &mut self,
        entity: Entity,
        launchers: &[crate::animation::PfEventTrigger],
    ) {
        for launcher in launchers {
            let sb = launcher.storyboard.clone();
            match launcher.event.as_str() {
                "Loaded" => {
                    let mut e = self.world.entity_mut(entity);
                    if let Some(mut pending) = e.get_mut::<crate::animation::PfPendingStoryboards>()
                    {
                        pending.0.push(sb);
                    } else {
                        e.insert(crate::animation::PfPendingStoryboards(vec![sb]));
                    }
                }
                "MouseEnter" => {
                    self.world.entity_mut(entity).observe(
                        move |over: On<Pointer<Over>>, mut commands: Commands| {
                            let sb = sb.clone();
                            let host = over.entity;
                            commands.queue(move |world: &mut World| {
                                crate::animation::begin_storyboard(world, host, host, &sb);
                            });
                        },
                    );
                }
                "MouseLeave" => {
                    self.world.entity_mut(entity).observe(
                        move |out: On<Pointer<Out>>, mut commands: Commands| {
                            let sb = sb.clone();
                            let host = out.entity;
                            commands.queue(move |world: &mut World| {
                                crate::animation::begin_storyboard(world, host, host, &sb);
                            });
                        },
                    );
                }
                "Click" => {
                    self.world.entity_mut(entity).observe(
                        move |click: On<Pointer<Click>>, mut commands: Commands| {
                            let sb = sb.clone();
                            let host = click.entity;
                            commands.queue(move |world: &mut World| {
                                crate::animation::begin_storyboard(world, host, host, &sb);
                            });
                        },
                    );
                }
                other => {
                    self.warn(format!("EventTrigger `{other}` is not supported yet"));
                }
            }
        }
    }

    /// Apply a `{DynamicResource key}` reference: eager initial value if
    /// resolvable, plus a recorded entry so later dictionary changes (or a
    /// key that only appears after a theme merge) re-apply it at the tier it
    /// was written from — the store keeps precedence structural.
    fn apply_dynamic_reference(
        &mut self,
        entity: Entity,
        kind: ElemKind,
        parent_kind: ParentKind,
        name: &str,
        key: crate::resources::ResourceKey,
    ) -> Result<(), PfError> {
        let target = crate::provider::property_target_for(name);
        if let Some(target) = target {
            if matches!(
                target,
                crate::provider::PropertyTarget::Foreground
                    | crate::provider::PropertyTarget::FontSize
            ) {
                self.ensure_inherited_seed(entity, target);
            }
            let tier = self.tier;
            let entry = crate::dynamic::DynEntry {
                key: key.clone(),
                target,
                tier,
            };
            let mut e = self.world.entity_mut(entity);
            if let Some(mut dynamics) = e.get_mut::<crate::dynamic::PfDynamicResources>() {
                // One entry per (target, tier).
                dynamics
                    .0
                    .retain(|d| !(d.target == target && d.tier == tier));
                dynamics.0.push(entry);
            } else {
                e.insert(crate::dynamic::PfDynamicResources(vec![entry]));
            }
        }
        match self.scopes.lookup(&key).cloned() {
            Some(value) => self.apply_property(
                entity,
                kind,
                parent_kind,
                None,
                name,
                &Resolved::Value(value),
            ),
            // Deferrable: silently wait for the key to appear (WPF allows
            // forward/late DynamicResource references).
            None if target.is_some() => Ok(()),
            None => Err(PfError::resource(format!(
                "resource `{key:?}` not found (and `{name}` is not dynamically updatable)"
            ))),
        }
    }

    /// `{AppThemeBinding Light=a, Dark=b}`: apply the arm for the current
    /// theme and, on a store-managed property, record it so the refresh pass
    /// re-picks when the theme changes.
    ///
    /// A property outside the store is applied ONCE, exactly as a
    /// `{DynamicResource}` on such a property is — the ceiling is the store,
    /// not this extension.
    fn apply_app_theme_reference(
        &mut self,
        entity: Entity,
        kind: ElemKind,
        parent_kind: ParentKind,
        name: &str,
        raw: &crate::resources::RawThemeArms,
    ) -> Result<(), PfError> {
        use crate::app_theme::{ThemeArm, ThemeArms};
        use crate::resources::RawArm;

        let theme = crate::app_theme::app_theme(self.world);
        let Some(target) = crate::provider::property_target_for(name) else {
            // One-shot: resolve the picked arm and hand it to the normal
            // property path. An absent arm clears nothing here, because
            // there is no store entry to mask.
            return match raw.pick(theme) {
                Some(RawArm::Text(s)) => {
                    self.apply_property(entity, kind, parent_kind, None, name, &Resolved::Str(s))
                }
                Some(RawArm::Static(k)) | Some(RawArm::Dynamic(k)) => {
                    match self.scopes.lookup(k).cloned() {
                        Some(v) => self.apply_property(
                            entity,
                            kind,
                            parent_kind,
                            None,
                            name,
                            &Resolved::Value(v),
                        ),
                        None => Err(PfError::resource(format!(
                            "AppThemeBinding resource `{k:?}` for `{name}` not found"
                        ))),
                    }
                }
                Some(RawArm::Null) | None => {
                    self.apply_property(entity, kind, parent_kind, None, name, &Resolved::Null)
                }
            };
        };

        if matches!(
            target,
            crate::provider::PropertyTarget::Foreground | crate::provider::PropertyTarget::FontSize
        ) {
            self.ensure_inherited_seed(entity, target);
        }

        // Convert every SUPPLIED arm up front, so a bad value is reported at
        // instantiation rather than the first time that theme is selected.
        let convert = |ctx: &Self, arm: &RawArm| -> Result<ThemeArm, PfError> {
            match arm {
                RawArm::Text(s) => Ok(ThemeArm::Value(Some(ctx.literal_to_pf_value(target, s)?))),
                RawArm::Null => Ok(ThemeArm::Value(None)),
                RawArm::Static(k) => match ctx.scopes.lookup(k).cloned() {
                    Some(v) => Ok(ThemeArm::Value(Some(v))),
                    None => Err(PfError::resource(format!(
                        "AppThemeBinding resource `{k:?}` for `{name}` not found"
                    ))),
                },
                RawArm::Dynamic(k) => Ok(ThemeArm::Dynamic(k.clone())),
            }
        };
        let mut arms = ThemeArms::default();
        for (raw_arm, slot) in [
            (&raw.light, &mut arms.light),
            (&raw.dark, &mut arms.dark),
            (&raw.default, &mut arms.default),
        ] {
            if let Some(raw_arm) = raw_arm {
                *slot = Some(convert(self, raw_arm)?);
            }
        }

        let tier = self.tier;
        let entry = crate::app_theme::AppThemeEntry {
            arms: arms.clone(),
            target,
            tier,
        };
        {
            let mut e = self.world.entity_mut(entity);
            if let Some(mut refs) = e.get_mut::<crate::app_theme::PfAppThemeRefs>() {
                // One entry per (target, tier), so a hot-reload rebuild does
                // not stack duplicates.
                refs.0.retain(|r| !(r.target == target && r.tier == tier));
                refs.0.push(entry);
            } else {
                e.insert(crate::app_theme::PfAppThemeRefs(vec![entry]));
            }
        }

        // The initial pick. `None` means no arm matched and there is no
        // Default: MAUI writes null, which here masks the lower tiers.
        let value = match arms.pick(theme) {
            Some(ThemeArm::Value(v)) => v.clone(),
            Some(ThemeArm::Dynamic(k)) => match self.scopes.lookup(k).cloned() {
                Some(v) => Some(v),
                // Late-resource rule, as {DynamicResource}: wait for the key.
                None => return Ok(()),
            },
            None => None,
        };
        crate::provider::store_and_apply(self.world, entity, target, tier, value);
        Ok(())
    }

    /// Record a `{Binding}` for later target resolution.
    fn record_binding(&mut self, property: &str, ext: &MarkupExtension) {
        let spec = crate::binding::parse_binding_extension(ext);
        if let Some(arg) = &spec.unsupported {
            self.warn(format!(
                "binding on `{property}`: `{arg}` is not supported yet; binding skipped"
            ));
            return;
        }
        self.pending.bindings.push((property.to_string(), spec));
    }

    /// Resolve a markup extension to a value. `Ok(None)` means "skip quietly"
    /// (a warning has been recorded). Always returns owned data, so the
    /// borrow on `self` ends with the call.
    fn resolve_extension(
        &mut self,
        ext: &MarkupExtension,
    ) -> Result<Option<Resolved<'static>>, PfError> {
        match ext.name.as_str() {
            "StaticResource" | "DynamicResource" => {
                let key = static_resource_key(ext)?;
                match self.scopes.lookup(&key) {
                    Some(value) => Ok(Some(Resolved::Value(value.clone()))),
                    None => Err(PfError::resource(format!("resource `{key:?}` not found"))),
                }
            }
            "x:Null" | "Null" => Ok(Some(Resolved::Null)),
            // Enum members: {x:Static Visibility.Collapsed} -> "Collapsed",
            // which the per-property enum converters understand.
            "x:Static" | "Static" => match ext.first_positional_str() {
                Some(member) => {
                    let value = member.rsplit('.').next().unwrap_or(member).to_string();
                    Ok(Some(Resolved::Value(PfValue::String(value))))
                }
                None => Err(PfError::instantiate("x:Static needs a member path")),
            },
            // Bindings are recorded by `apply_xaml_value`/content handling;
            // reaching here means the caller just needs "no literal value".
            "Binding" | "x:Bind" | "CompiledBinding" => Ok(None),
            // Reached by the paths that resolve an attribute to a VALUE
            // rather than routing it through the property store — Text= is
            // the common one. Those cannot forward live, so the templated
            // parent is read once here, which is right for the properties
            // that actually get bound this way.
            "TemplateBinding" => {
                let Some(tc) = self.template_ctx.last() else {
                    self.warn(
                        "{TemplateBinding} outside a ControlTemplate; property skipped".to_string(),
                    );
                    return Ok(None);
                };
                let parent = tc.templated_parent;
                let Some(src) = ext.first_positional_str() else {
                    self.warn("TemplateBinding needs a source property".to_string());
                    return Ok(None);
                };
                // Tag: WPF's per-instance scratch property, and the standard
                // way to pass one small piece of data into a template.
                if src == "Tag" {
                    return Ok(self
                        .world
                        .get::<crate::components::PfTag>(parent)
                        .map(|t| Resolved::Value(PfValue::String(t.0.clone()))));
                }
                // Anything the store manages can be read at its current value.
                if let Some(target) = crate::provider::property_target_for(src)
                    && let Some(value) = self
                        .world
                        .get::<crate::provider::PfPropertyStore>(parent)
                        .and_then(|store| store.effective(target))
                        .and_then(|(_, v)| v.clone())
                {
                    return Ok(Some(Resolved::Value(value)));
                }
                self.warn(format!(
                    "TemplateBinding `{src}` is not readable here; property skipped"
                ));
                Ok(None)
            }
            // Neither the platform nor the idiom changes while the app runs,
            // so these resolve here and need no recorded reference.
            "OnPlatform" | "OnIdiom" => {
                let kind = if ext.name == "OnPlatform" {
                    crate::device::SelectorKind::Platform
                } else {
                    crate::device::SelectorKind::Idiom
                };
                let arms = crate::resources::parse_selector_arms(ext, kind)?;
                let device = crate::device::device(self.world);
                match arms.pick(&device, kind) {
                    Some(crate::resources::RawArm::Text(s)) => {
                        Ok(Some(Resolved::Value(PfValue::String(s.clone()))))
                    }
                    Some(crate::resources::RawArm::Static(k))
                    | Some(crate::resources::RawArm::Dynamic(k)) => match self.scopes.lookup(k) {
                        Some(value) => Ok(Some(Resolved::Value(value.clone()))),
                        None => Err(PfError::resource(format!(
                            "{} resource `{k:?}` not found",
                            kind.extension_name()
                        ))),
                    },
                    Some(crate::resources::RawArm::Null) | None => Ok(Some(Resolved::Null)),
                }
            }
            // Reached for attached properties and other one-shot sites: pick
            // once at the current theme. The live path is
            // `apply_app_theme_reference`, which the attribute handler takes.
            "AppThemeBinding" => {
                let raw = crate::resources::parse_app_theme_arms(ext)?;
                match raw.pick(crate::app_theme::app_theme(self.world)) {
                    Some(crate::resources::RawArm::Text(s)) => {
                        Ok(Some(Resolved::Value(PfValue::String(s.clone()))))
                    }
                    Some(crate::resources::RawArm::Static(k))
                    | Some(crate::resources::RawArm::Dynamic(k)) => match self.scopes.lookup(k) {
                        Some(value) => Ok(Some(Resolved::Value(value.clone()))),
                        None => Err(PfError::resource(format!(
                            "AppThemeBinding resource `{k:?}` not found"
                        ))),
                    },
                    Some(crate::resources::RawArm::Null) | None => Ok(Some(Resolved::Null)),
                }
            }
            other => {
                self.warn(format!(
                    "markup extension `{{{other}}}` is not supported yet; property skipped"
                ));
                Ok(None)
            }
        }
    }

    /// `<X.ItemsPanel><ItemsPanelTemplate><panel/>` — spawn the panel as the
    /// real items host; generated containers land inside it.
    fn apply_items_panel(
        &mut self,
        entity: Entity,
        kind: ElemKind,
        pe: &bevy_pf_xaml::XamlPropertyElement,
    ) {
        if !matches!(kind, ElemKind::ListBox | ElemKind::ItemsControl) {
            self.warn(format!(
                "{}: ItemsPanel is only supported on ListBox/ListView/ItemsControl",
                pe.pos
            ));
            return;
        }
        let template = pe.elements().next();
        let panel_node = match template {
            Some(t) if t.name == "ItemsPanelTemplate" => t.child_elements().next(),
            // Tolerate the panel written directly, without the template shell.
            other => other,
        };
        let Some(panel_node) = panel_node else {
            self.warn(format!("{}: ItemsPanel has no panel element", pe.pos));
            return;
        };
        match self.spawn_element(panel_node, ParentKind::FlexColumn, None) {
            Ok(panel) => {
                if let Some(mut n) = self.world.get_mut::<Node>(panel) {
                    n.flex_grow = 1.0;
                }
                self.add_children(entity, &[panel]);
                self.world
                    .entity_mut(entity)
                    .insert(crate::components::PfItemsPanel { panel });
                self.world
                    .entity_mut(panel)
                    .insert(crate::components::PfGeneratedItemsHost { owner: entity });
            }
            Err(e) => self.warn(format!("{}: ItemsPanel: {e}", pe.pos)),
        }
    }

    fn is_registered_element(&self, name: &str) -> bool {
        self.world
            .get_resource::<PfElements>()
            .is_some_and(|r| r.0.contains_key(name))
    }

    fn apply_property_element(
        &mut self,
        entity: Entity,
        kind: ElemKind,
        parent_kind: ParentKind,
        pe: &bevy_pf_xaml::XamlPropertyElement,
    ) {
        // Attached property elements of another owner (rare) and content-like
        // property elements are handled by kind-specific code; everything else
        // is a structured value (brush, style, ...) applied as a property.
        if pe.name == "VisualStateGroups" {
            // Inside a ControlTemplate these were already consumed into
            // PfControlTemplate.state_groups at parse time. Attached
            // DIRECTLY to an element they belong to that element, which is
            // where MAUI puts them — the two dialects differ by POSITION,
            // not by syntax, so the same parser serves both.
            if self.template_ctx.is_empty() {
                let dict = crate::resources::ResourceDictionary::default();
                match crate::resources::parse_visual_state_groups(
                    pe,
                    &self.scopes,
                    &dict,
                    &mut self.warnings,
                ) {
                    Ok((groups, triggers)) if !groups.is_empty() => {
                        let count = groups.len();
                        self.world
                            .entity_mut(entity)
                            .insert(crate::animation::PfVisualStates {
                                groups,
                                current: vec![None; count],
                                touched: vec![Vec::new(); count],
                            });
                        // `<VisualState.Setters>` arrives as triggers keyed
                        // on the state; there is no template here, so they
                        // target the element itself.
                        if !triggers.is_empty() {
                            self.attach_triggers(entity, &triggers, None);
                        }
                    }
                    Ok(_) => {}
                    Err(e) => self.warn(format!("{}: VisualStateGroups: {e}", pe.pos)),
                }
            }
            return;
        }
        if pe.name == "Triggers" && pe.owner == "Interaction" {
            self.apply_behavior_triggers(entity, pe);
            return;
        }
        if kind == ElemKind::Track
            && matches!(
                pe.name.as_str(),
                "Thumb" | "DecreaseRepeatButton" | "IncreaseRepeatButton"
            )
        {
            // <Track.Thumb>/<Track.*RepeatButton>: spawn the wrapped element
            // as a track child (page buttons colored NONE by default).
            if let Some(child_node) = pe.elements().next() {
                match self.spawn_element(child_node, ParentKind::Grid, None) {
                    Ok(child) => self.add_children(entity, &[child]),
                    Err(e) => self.warn(format!("{}: Track.{}: {e}", pe.pos, pe.name)),
                }
            }
            return;
        }
        if pe.name == "ItemsPanel" {
            self.apply_items_panel(entity, kind, pe);
            return;
        }
        // `<X.Prop><MultiBinding StringFormat=...><Binding Path=a/>...` —
        // recorded like an attribute binding; combined at apply time.
        if let Some(mb) = pe.elements().next().filter(|n| n.name == "MultiBinding") {
            let attr_str = |name: &str| match mb.attribute(name) {
                Some(XamlValue::Str(v)) => Some(v.clone()),
                _ => None,
            };
            let converter = match mb.attribute("Converter") {
                Some(XamlValue::Extension(ext)) => {
                    ext.first_positional_str().map(|k| k.to_string())
                }
                Some(XamlValue::Str(v)) => Some(v.clone()),
                None => None,
            };
            let multi_paths: Vec<String> = mb
                .child_elements()
                .filter(|n| n.name == "Binding")
                .filter_map(|n| match n.attribute("Path") {
                    Some(XamlValue::Str(v)) => Some(v.clone()),
                    _ => None,
                })
                .collect();
            if multi_paths.is_empty() {
                self.warn(format!(
                    "{}: MultiBinding needs child <Binding Path=.../> elements",
                    pe.pos
                ));
                return;
            }
            if converter.is_none() && attr_str("StringFormat").is_none() {
                self.warn(format!(
                    "{}: MultiBinding needs a Converter or StringFormat",
                    pe.pos
                ));
                return;
            }
            self.pending.bindings.push((
                pe.name.clone(),
                crate::binding::BindingSpec {
                    path: String::new(),
                    element_name: None,
                    mode: v::BindingMode::OneWay,
                    string_format: attr_str("StringFormat"),
                    converter,
                    converter_parameter: attr_str("ConverterParameter"),
                    fallback: attr_str("FallbackValue"),
                    target_null: None,
                    multi_paths,
                    relative: None,
                    unsupported: None,
                },
            ));
            return;
        }
        if pe.name == "ItemContainerStyle" {
            match pe.elements().next() {
                Some(style_node) if style_node.name == "Style" => {
                    let dict = ResourceDictionary::new();
                    match crate::resources::parse_style(
                        style_node,
                        &self.scopes,
                        &dict,
                        &mut self.warnings,
                    ) {
                        Ok(style) => {
                            self.pending.item_container_style = Some(std::sync::Arc::new(style));
                        }
                        Err(e) => self.warn(format!("{}: ItemContainerStyle: {e}", pe.pos)),
                    }
                }
                _ => self.warn(format!(
                    "{}: ItemContainerStyle expects an inline <Style>",
                    pe.pos
                )),
            }
            return;
        }
        if pe.name == "Triggers" {
            // Element-scope triggers: WPF only allows EventTriggers here.
            let mut launchers = Vec::new();
            for trigger in pe.elements() {
                if trigger.name == "EventTrigger" {
                    let dict = ResourceDictionary::new();
                    let mut warnings = std::mem::take(&mut self.warnings);
                    if let Some(et) = crate::resources::parse_event_trigger(
                        trigger,
                        &self.scopes,
                        &dict,
                        &mut warnings,
                    ) {
                        launchers.push(et);
                    }
                    self.warnings = warnings;
                } else {
                    self.warn(format!(
                        "{}: only EventTriggers are valid in element Triggers",
                        trigger.pos
                    ));
                }
            }
            if !launchers.is_empty() {
                self.attach_event_triggers(entity, &launchers);
            }
            return;
        }
        if matches!(
            pe.name.as_str(),
            "Content" | "Child" | "Children" | "Header" | "Columns" | "View"
        ) {
            return; // consumed by spawn_content
        }
        if pe.name == "ContextMenu" {
            let menu_node = pe.single_element().cloned();
            match menu_node {
                Some(menu_node) if menu_node.name == "ContextMenu" => {
                    if let Err(e) = self.attach_context_menu(entity, &menu_node) {
                        self.warn(format!("{}: ContextMenu: {e}", pe.pos));
                    }
                }
                _ => self.warn(format!(
                    "{}: ContextMenu property expects a <ContextMenu> element",
                    pe.pos
                )),
            }
            return;
        }
        // Blend behaviors (xmlns:i Interaction.Triggers/Behaviors).
        if matches!(pe.owner.as_str(), "Interaction" | "Interactivity") {
            self.warn(format!(
                "{}: Blend behaviors (`{}.{}`) are not supported yet",
                pe.pos, pe.owner, pe.name
            ));
            return;
        }
        let Some(el) = pe.single_element() else {
            // e.g. <TextBox.Text><Binding .../></TextBox.Text> with text? No
            // elements at all means text content: apply as a literal.
            if let Some(text) = pe.values.iter().find_map(XamlChild::as_text) {
                let text = text.to_string();
                self.apply_xaml_value(
                    entity,
                    kind,
                    parent_kind,
                    None,
                    &pe.name.clone(),
                    &XamlValue::Str(text),
                );
                return;
            }
            self.warn(format!(
                "{}: property element `{}.{}` with multiple values is not supported here",
                pe.pos, pe.owner, pe.name
            ));
            return;
        };

        if el.name == "Binding" {
            // Element syntax: <TextBox.Text><Binding Path="..."/></TextBox.Text>
            let mut ext = MarkupExtension {
                name: "Binding".to_string(),
                positional: Vec::new(),
                named: Vec::new(),
            };
            for attr in &el.attributes {
                if let XamlValue::Str(s) = &attr.value {
                    ext.named.push((
                        attr.name.clone(),
                        bevy_pf_xaml::markup::MarkupValue::Str(s.clone()),
                    ));
                } else {
                    // e.g. Source="{StaticResource ...}" — unsupported source.
                    ext.named.push((
                        attr.name.clone(),
                        bevy_pf_xaml::markup::MarkupValue::Str(String::new()),
                    ));
                }
            }
            let name = pe.name.clone();
            self.record_binding(&name, &ext);
            return;
        }

        let parsed = {
            let dict = ResourceDictionary::new();
            parse_resource_value(el, &self.scopes, &dict, &mut self.warnings)
        };
        match parsed {
            Ok(Some(value)) => {
                let name = pe.name.clone();
                if let Err(e) = self.apply_property(
                    entity,
                    kind,
                    parent_kind,
                    None,
                    &name,
                    &Resolved::Value(value),
                ) {
                    self.warn(format!("{}: property `{}`: {e}", pe.pos, pe.name));
                }
            }
            Ok(None) => {}
            Err(e) => self.warn(format!("{}: property `{}`: {e}", pe.pos, pe.name)),
        }
    }

    // -----------------------------------------------------------------
    // Property application
    // -----------------------------------------------------------------

    fn apply_property(
        &mut self,
        entity: Entity,
        kind: ElemKind,
        parent_kind: ParentKind,
        owner: Option<&str>,
        name: &str,
        value: &Resolved,
    ) -> Result<(), PfError> {
        // Attached properties (Grid.Row="1", DockPanel.Dock="Top", ...) are
        // recorded on the child and consumed by the parent afterwards.
        if let Some(owner) = owner {
            let raw = value.to_text()?;
            let mut e = self.world.entity_mut(entity);
            if let Some(mut attached) = e.get_mut::<PfAttachedProps>() {
                attached.0.insert(format!("{owner}.{name}"), raw);
            } else {
                let mut map = bevy::platform::collections::HashMap::default();
                map.insert(format!("{owner}.{name}"), raw);
                e.insert(PfAttachedProps(map));
            }
            return Ok(());
        }

        match name {
            "Width" => {
                let px = value.to_f32()?;
                self.store_apply(
                    entity,
                    crate::provider::PropertyTarget::Width,
                    PfValue::Double(px as f64),
                );
            }
            "Height" => {
                let px = value.to_f32()?;
                self.store_apply(
                    entity,
                    crate::provider::PropertyTarget::Height,
                    PfValue::Double(px as f64),
                );
            }
            "Opacity" => {
                let v = value.to_f32()?;
                self.store_apply(
                    entity,
                    crate::provider::PropertyTarget::Opacity,
                    PfValue::Double(f64::from(v.clamp(0.0, 1.0))),
                );
            }
            "MinWidth" => {
                let px = value.to_f32()?;
                self.node_mut(entity).min_width = convert::dimension(px);
            }
            "MinHeight" => {
                let px = value.to_f32()?;
                self.node_mut(entity).min_height = convert::dimension(px);
            }
            "MaxWidth" => {
                let px = value.to_f32()?;
                self.node_mut(entity).max_width = convert::dimension(px);
            }
            "MaxHeight" => {
                let px = value.to_f32()?;
                self.node_mut(entity).max_height = convert::dimension(px);
            }
            "Margin" => {
                let t = value.to_thickness()?;
                self.store_apply(
                    entity,
                    crate::provider::PropertyTarget::Margin,
                    PfValue::Thickness(t),
                );
            }
            "Padding" => {
                let t = value.to_thickness()?;
                self.store_apply(
                    entity,
                    crate::provider::PropertyTarget::Padding,
                    PfValue::Thickness(t),
                );
            }
            "HorizontalAlignment" => {
                let a: v::HorizontalAlignment = value.parse_enum()?;
                self.apply_h_alignment(entity, parent_kind, a);
            }
            "VerticalAlignment" => {
                let a: v::VerticalAlignment = value.parse_enum()?;
                self.apply_v_alignment(entity, parent_kind, a);
            }
            "Visibility" => {
                let vis: v::Visibility = value.parse_enum()?;
                self.store_apply(
                    entity,
                    crate::provider::PropertyTarget::Visibility,
                    PfValue::String(format!("{vis:?}")),
                );
            }
            "Background" => {
                let brush = value.to_brush()?;
                self.store_apply(
                    entity,
                    crate::provider::PropertyTarget::Background,
                    PfValue::Brush(brush),
                );
            }
            "BorderBrush" => {
                let brush = value.to_brush()?;
                if !matches!(brush, v::PfBrush::Solid(_)) {
                    self.warn("gradient BorderBrush is not supported yet".to_string());
                } else {
                    self.store_apply(
                        entity,
                        crate::provider::PropertyTarget::BorderBrush,
                        PfValue::Brush(brush),
                    );
                }
            }
            "BorderThickness" => {
                let t = value.to_thickness()?;
                self.store_apply(
                    entity,
                    crate::provider::PropertyTarget::BorderThickness,
                    PfValue::Thickness(t),
                );
            }
            "CornerRadius" => {
                let r = value.to_corner_radius()?;
                self.store_apply(
                    entity,
                    crate::provider::PropertyTarget::CornerRadius,
                    PfValue::CornerRadius(r),
                );
            }
            "Orientation" => {
                let o: v::Orientation = value.parse_enum()?;
                let dir = match (kind, o) {
                    (_, v::Orientation::Vertical) => FlexDirection::Column,
                    (_, v::Orientation::Horizontal) => FlexDirection::Row,
                };
                self.node_mut(entity).flex_direction = dir;
            }
            "Spacing" => {
                // WinUI StackPanel.Spacing convenience.
                let px = value.to_f32()?;
                let mut node = self.node_mut(entity);
                node.row_gap = Val::Px(px);
                node.column_gap = Val::Px(px);
            }
            // Per-axis spacing. `ItemSpacing` is WrapPanel's name
            // for the gap between items along the flow direction, and its
            // cross-axis partner is LineSpacing; both map to the same gaps.
            "RowSpacing" | "LineSpacing" => {
                let px = value.to_f32()?;
                self.node_mut(entity).row_gap = Val::Px(px);
            }
            "ColumnSpacing" => {
                let px = value.to_f32()?;
                self.node_mut(entity).column_gap = Val::Px(px);
            }
            "ItemSpacing" => {
                // The gap BETWEEN items, which runs along the flow: it is the
                // column gap on a horizontal panel and the row gap on a
                // vertical one. Read from the flex direction that
                // `Orientation` has already written, so this depends on
                // Orientation appearing first — which is how it is always
                // written, and the horizontal default is right regardless.
                let px = value.to_f32()?;
                let mut n = self.node_mut(entity);
                match n.flex_direction {
                    FlexDirection::Column | FlexDirection::ColumnReverse => n.row_gap = Val::Px(px),
                    _ => n.column_gap = Val::Px(px),
                }
            }
            "RowDefinitions" if kind == ElemKind::Grid => {
                // .NET 10 shorthand: RowDefinitions="Auto,*,2*"
                let tracks = parse_track_list(&value.to_text()?)?;
                self.node_mut(entity).grid_template_rows = tracks;
            }
            "ColumnDefinitions" if kind == ElemKind::Grid => {
                let tracks = parse_track_list(&value.to_text()?)?;
                self.node_mut(entity).grid_template_columns = tracks;
            }

            // Text / font properties: inherited downward, consumed when text
            // components are built.
            "FontSize" => {
                let px = value.to_f32()?;
                self.inherited.font_size = px;
                self.store_set_only(
                    entity,
                    crate::provider::PropertyTarget::FontSize,
                    PfValue::Double(px as f64),
                );
            }
            "Foreground" => {
                let brush = value.to_brush()?;
                if let v::PfBrush::Solid(c) = brush {
                    self.inherited.foreground = c;
                    self.pending.own_foreground = Some(c);
                    self.store_set_only(
                        entity,
                        crate::provider::PropertyTarget::Foreground,
                        PfValue::Brush(v::PfBrush::Solid(c)),
                    );
                }
            }
            "FontFamily" => {
                let fam = value.to_text()?;
                // WPF allows fallback lists: take the first family.
                let first = fam.split(',').next().unwrap_or(&fam).trim().to_string();
                self.inherited.font_family = Some(first);
            }
            "FontWeight" => {
                self.inherited.font_weight = match value {
                    Resolved::Str(s) => s.parse()?,
                    Resolved::Value(PfValue::Double(d)) => v::FontWeight(*d as u16),
                    Resolved::Value(PfValue::String(s)) => s.parse()?,
                    _ => return Err(PfError::instantiate("expected a font weight")),
                }
            }
            "FontStyle" => self.inherited.font_style = value.parse_enum()?,
            "TextAlignment" => self.inherited.text_alignment = Some(value.parse_enum()?),
            "TextWrapping" => self.inherited.text_wrapping = value.parse_enum()?,

            // Control state, consumed by spawn_content.
            "IsChecked" | "IsExpanded" => self.pending.is_checked = Some(value.to_bool()?),
            "GroupName" => self.pending.group_name = Some(value.to_text()?),
            "Minimum" => self.pending.minimum = Some(value.to_f32()?),
            "Maximum" => self.pending.maximum = Some(value.to_f32()?),
            "Value"
                if matches!(
                    kind,
                    ElemKind::Slider | ElemKind::ProgressBar | ElemKind::ScrollBar
                ) =>
            {
                self.pending.value = Some(value.to_f32()?)
            }
            "IsIndeterminate" if kind == ElemKind::ProgressBar => {
                self.pending.is_indeterminate = value.to_bool()?
            }
            // RepeatButton timing, in milliseconds like WPF.
            "Delay" | "Interval" if kind == ElemKind::Button => {
                let ms = value.to_f32()?;
                if let Some(mut rb) = self
                    .world
                    .get_mut::<crate::components::PfRepeatButton>(entity)
                {
                    if name == "Delay" {
                        rb.delay = ms;
                    } else {
                        rb.interval = ms;
                    }
                }
            }
            // Consumed directly by the control builders.
            "NavigateUri" | "Content" if kind == ElemKind::Hyperlink => {}
            "IsOpen" | "Placement" | "PlacementTarget" | "StaysOpen"
                if kind == ElemKind::PopupElement => {}
            "SelectedDate" | "DisplayDate" | "DisplayMode" | "FirstDayOfWeek"
                if matches!(kind, ElemKind::Calendar | ElemKind::DatePicker) => {}
            "ResizeDirection" | "ResizeBehavior" if kind == ElemKind::GridSplitter => {}
            "Source" | "NavigationUIVisibility" | "JournalOwnership" if kind == ElemKind::Frame => {
            }
            // Code-behind pointer events: the attribute value is a handler
            // name resolved against `PfEventHandlers` (app.on_ui_event) at
            // fire time; unregistered names stay silent, so verbatim WPF
            // markup still instantiates clean without any Rust code-behind.
            "Click" | "MouseEnter" | "MouseLeave" | "MouseDown" | "MouseUp" | "MouseMove"
            | "MouseDoubleClick" | "DragStart" | "Drag" | "DragEnd" | "Drop" => {
                let handler = value.to_text()?;
                self.pending
                    .event_handlers
                    .push((name.to_string(), handler));
            }
            // Code-behind event handlers with no pointer analog yet. There
            // is no C# to call; these are accepted without noise.
            "Loaded" | "Unloaded" | "Initialized" | "TargetUpdated" | "SelectionChanged"
            | "TextChanged" | "Checked" | "Unchecked" | "ValueChanged" | "Navigating"
            | "Navigated" | "KeyDown" | "KeyUp" | "GotFocus" | "LostFocus" | "SizeChanged"
            | "DataContextChanged" | "Closed" | "Opened" | "ContextMenuOpening" => {}
            // WPF DataGrid knobs that don't apply here: columns are never
            // auto-generated, headers always show, sizing is fixed. Accepted
            // silently so verbatim WPF markup instantiates clean.
            "IsOn" if kind == ElemKind::ToggleSwitch => {}
            "Value" | "Increment" | "FormatString" if kind == ElemKind::NumericUpDown => {}
            "Watermark" | "PlaceholderText" if kind == ElemKind::TextBox => {}
            "Password" | "PasswordChar" if kind == ElemKind::TextBox => {}
            // Consumed at build time (TabIndex insertion checks it directly).
            "IsTabStop" => {}
            // Compiled-binding metadata. `x:DataType` names the
            // type a `{Binding}` is checked against and `x:CompileBindings`
            // switches that checking on; both are consumed by a XAML
            // COMPILER and mean nothing at runtime, so warning about them
            // would report a non-problem on nearly every such document.
            // Bindings here resolve dynamically and need no declared type.
            // (`DataTemplate DataType=` is a different path — it is a resource
            // type, keyed in parse_resource_value, and is honoured there.)
            "DataType" | "CompileBindings" => {}
            // Style classes. Already read into the element stack for
            // selector matching; kept on the entity so a class can change at
            // runtime and re-match.
            "Classes" => {
                let classes: Vec<String> = value
                    .to_text()?
                    .split_whitespace()
                    .map(str::to_string)
                    .collect();
                self.world
                    .entity_mut(entity)
                    .insert(crate::components::PfClasses(classes));
            }
            // Where the CONTENT sits inside the control, not where the
            // control sits inside its parent. Consumed by project_content,
            // which otherwise picks a per-kind default.
            //
            // Applied at build time rather than through the property store,
            // so a Style setter reaches it but a TRIGGER cannot flip it —
            // that needs alignment PropertyTargets, still deferred.
            "HorizontalContentAlignment" => {
                let text = value.to_text()?;
                self.pending.h_content_align = match text.trim() {
                    "Left" | "Start" => Some(JustifyItems::Start),
                    "Center" => Some(JustifyItems::Center),
                    "Right" | "End" => Some(JustifyItems::End),
                    "Stretch" => Some(JustifyItems::Stretch),
                    other => {
                        self.warn(format!(
                            "HorizontalContentAlignment `{other}` is not a value"
                        ));
                        None
                    }
                };
            }
            "VerticalContentAlignment" => {
                let text = value.to_text()?;
                self.pending.v_content_align = match text.trim() {
                    "Top" | "Start" => Some(AlignItems::Start),
                    "Center" => Some(AlignItems::Center),
                    "Bottom" | "End" => Some(AlignItems::End),
                    "Stretch" => Some(AlignItems::Stretch),
                    other => {
                        self.warn(format!("VerticalContentAlignment `{other}` is not a value"));
                        None
                    }
                };
            }
            // The theme variant, which is this crate's app theme:
            // Light / Dark, and `Default` meaning "follow the system".
            "RequestedThemeVariant" => {
                let variant = value.to_text()?;
                let theme = match variant.trim() {
                    "Light" => Some(crate::app_theme::AppTheme::Light),
                    "Dark" => Some(crate::app_theme::AppTheme::Dark),
                    "Default" => Some(crate::app_theme::AppTheme::Unspecified),
                    _ => None,
                };
                match theme {
                    Some(theme) => crate::app_theme::set_user_app_theme(self.world, theme),
                    None => self.warn(format!(
                        "RequestedThemeVariant `{variant}` is not one of Light, Dark or Default"
                    )),
                }
            }
            "Value" if kind == ElemKind::RatingBar => {}
            "LowerValue" | "UpperValue" | "MinRange" if kind == ElemKind::RangeSlider => {}
            // Consumed by spawn_scroll_bar (Orientation hits the generic arm).
            "ViewportSize" | "SmallChange" | "LargeChange" if kind == ElemKind::ScrollBar => {}
            "Badge" | "BadgePlacementMode" if kind == ElemKind::Badge => {}
            "IsBusy" | "BusyContent" if kind == ElemKind::BusyIndicator => {}
            "Command" => {
                self.pending.command = Some(value.to_text()?);
            }
            "CommandParameter" => {
                self.pending.command_parameter = Some(crate::binding::PfCommandParameter::Literal(
                    value.to_text()?,
                ));
            }
            "SelectedTime" if kind == ElemKind::TimePicker => {}
            "ContentSource"
            | "RecognizesAccessKey"
            | "ContentTemplate"
            | "ContentTemplateSelector"
            | "ContentStringFormat"
                if kind == ElemKind::ContentPresenter =>
            {
                self.warn(format!(
                    "ContentPresenter {name}= is not supported yet; ignored"
                ));
            }
            "Kind" | "Symbol" | "Glyph" if kind == ElemKind::PackIcon => {}
            "SelectedColor" if kind == ElemKind::ColorPicker => {}
            "Suggestions" | "QueryIcon" | "PlaceholderText" if kind == ElemKind::AutoSuggestBox => {
            }
            "PaneDisplayMode"
            | "IsPaneOpen"
            | "OpenPaneLength"
            | "IsBackButtonVisible"
            | "IsSettingsVisible"
                if kind == ElemKind::NavigationView => {}
            "AutoGenerateColumns"
            | "HeadersVisibility"
            | "CanUserResizeColumns"
            | "CanUserResizeRows"
            | "CanUserSortColumns"
            | "CanUserAddRows"
            | "CanUserDeleteRows"
            | "CanUserReorderColumns"
            | "IsReadOnly"
            | "ColumnHeaderStyle"
            | "GridLinesVisibility"
            | "SelectionMode"
            | "SelectionUnit"
                if kind == ElemKind::DataGrid => {}
            "MaxLength" => self.pending.max_length = Some(value.to_f32()? as usize),
            "AcceptsReturn" => self.pending.accepts_return = value.to_bool()?,
            "Rows" if kind == ElemKind::UniformGrid => {
                self.pending.rows = Some(value.to_f32()? as u16)
            }
            "Columns" if kind == ElemKind::UniformGrid => {
                self.pending.columns = Some(value.to_f32()? as u16)
            }
            "DisplayMemberPath" => {
                self.pending.display_member = Some(value.to_text()?);
            }
            // Which member of an item `SelectedValue` binds. Empty (the WPF
            // default) means the item itself.
            "SelectedValuePath" => {
                let path = value.to_text()?;
                self.world
                    .entity_mut(entity)
                    .insert(crate::components::PfSelectedValuePath(path));
            }
            "ItemContainerStyle" => match value {
                Resolved::Value(PfValue::Style(s)) => {
                    self.pending.item_container_style = Some(s.clone());
                }
                _ => {
                    return Err(PfError::instantiate(
                        "ItemContainerStyle expects a Style resource",
                    ));
                }
            },
            "ContentTemplate" => match value {
                Resolved::Value(PfValue::Template(t)) => {
                    self.pending.content_template = Some(t.clone());
                }
                _ => {
                    return Err(PfError::instantiate(
                        "ContentTemplate expects a DataTemplate (inline or resource)",
                    ));
                }
            },
            "ItemTemplate" => match value {
                Resolved::Value(PfValue::Template(t)) => {
                    self.pending.item_template = Some(t.clone());
                }
                Resolved::Value(PfValue::ControlTemplate(_)) => {
                    return Err(PfError::instantiate(
                        "ItemTemplate expects a DataTemplate, got a ControlTemplate",
                    ));
                }
                _ => {
                    return Err(PfError::instantiate(
                        "ItemTemplate expects a DataTemplate (inline or resource)",
                    ));
                }
            },
            "Template" => match value {
                Resolved::Value(PfValue::ControlTemplate(t)) => {
                    // Scope captured at application time (consumer-scope
                    // approximation, same as DataTemplate expansion).
                    let scopes = std::sync::Arc::new(self.scopes.clone());
                    self.pending.control_template = Some((t.clone(), scopes));
                    // Recorded, not applied: expansion happens in
                    // `expand_control_template` once the element's kind is
                    // known (only template-capable kinds accept one), and no
                    // borrow may be held across that `&mut self` call.
                }
                Resolved::Value(PfValue::Template(_)) => {
                    return Err(PfError::instantiate(
                        "Template expects a ControlTemplate, got a DataTemplate",
                    ));
                }
                _ => {
                    return Err(PfError::instantiate(
                        "Template expects a ControlTemplate (inline or resource)",
                    ));
                }
            },
            "ToolTip" => {
                let tip = value.to_text()?;
                self.world
                    .entity_mut(entity)
                    .insert(crate::overlay::PfToolTip(tip));
            }
            "SelectedIndex" => {
                let idx = value.to_f32()?;
                if idx >= 0.0 {
                    self.pending.selected_index = Some(idx as usize);
                }
            }
            "IsEnabled" => {
                if !value.to_bool()? {
                    self.world
                        .entity_mut(entity)
                        .insert(bevy::ui::InteractionDisabled);
                }
            }
            // WPF `UIElement.IsHitTestVisible`. The subtree reach is applied
            // by `hit_test::propagate_hit_test_visibility`, which also lets
            // the flag be flipped at runtime and lets generated children
            // (ItemsControl rows) inherit it.
            "IsHitTestVisible" => {
                let visible = value.to_bool()?;
                self.world
                    .entity_mut(entity)
                    .insert(crate::hit_test::PfHitTestVisible(visible));
            }
            "Tag" => {
                let tag = value.to_text()?;
                self.world.entity_mut(entity).insert(PfTag(tag));
            }
            "RenderTransform" => match value {
                Resolved::Value(PfValue::Transform(t)) => {
                    self.world
                        .entity_mut(entity)
                        .insert(convert::ui_transform(*t));
                }
                _ => {
                    return Err(PfError::instantiate(
                        "RenderTransform expects a transform element or resource",
                    ));
                }
            },

            // Shape properties.
            "Fill" if kind == ElemKind::Shape => {
                let brush = value.to_brush()?;
                self.pending.shape.fill = Some(brush.clone());
                // Seed the store at the declaring tier so trigger writes to
                // Fill revert to this brush (not to unset) on deactivation.
                let tier = self.tier;
                let mut e = self.world.entity_mut(entity);
                if let Some(mut store) = e.get_mut::<crate::provider::PfPropertyStore>() {
                    store.set(
                        crate::provider::PropertyTarget::Fill,
                        tier,
                        Some(PfValue::Brush(brush)),
                    );
                } else {
                    let mut store = crate::provider::PfPropertyStore::default();
                    store.set(
                        crate::provider::PropertyTarget::Fill,
                        tier,
                        Some(PfValue::Brush(brush)),
                    );
                    e.insert(store);
                }
            }
            "Stroke" if kind == ElemKind::Shape => {
                let brush = value.to_brush()?;
                self.pending.shape.stroke = Some(brush.clone());
                // Seeded at the declaring tier for the same reason Fill is:
                // so a trigger writing Stroke reverts to THIS brush rather
                // than to unset when it deactivates.
                let tier = self.tier;
                let mut e = self.world.entity_mut(entity);
                if let Some(mut store) = e.get_mut::<crate::provider::PfPropertyStore>() {
                    store.set(
                        crate::provider::PropertyTarget::Stroke,
                        tier,
                        Some(PfValue::Brush(brush)),
                    );
                } else {
                    let mut store = crate::provider::PfPropertyStore::default();
                    store.set(
                        crate::provider::PropertyTarget::Stroke,
                        tier,
                        Some(PfValue::Brush(brush)),
                    );
                    e.insert(store);
                }
            }
            "StrokeThickness" => self.pending.shape.stroke_thickness = Some(value.to_f32()?),
            // Consumed by the Image builder (Source loads the asset, Slice
            // selects nine-patch mode).
            "Source" | "Slice" | "Stretch" if kind == ElemKind::Image => {}
            "Stretch" if matches!(kind, ElemKind::Shape | ElemKind::Viewbox) => {
                self.pending.shape.stretch = Some(value.parse_enum()?)
            }
            "X1" => self.pending.shape.x1 = value.to_f32()?,
            "Y1" => self.pending.shape.y1 = value.to_f32()?,
            "X2" => self.pending.shape.x2 = value.to_f32()?,
            "Y2" => self.pending.shape.y2 = value.to_f32()?,
            "RadiusX" if kind == ElemKind::Shape => self.pending.shape.radius_x = value.to_f32()?,
            "RadiusY" if kind == ElemKind::Shape => self.pending.shape.radius_y = value.to_f32()?,
            "Points" if kind == ElemKind::Shape => {
                self.pending.shape.points = Some(v::parse_points(&value.to_text()?)?)
            }
            "FillRule" if kind == ElemKind::Shape => {
                let text = value.to_text()?;
                self.pending.shape.fill_rule =
                    Some(match text.trim().to_ascii_lowercase().as_str() {
                        "nonzero" => bevy_pf_xaml::geometry::FillRule::NonZero,
                        "evenodd" => bevy_pf_xaml::geometry::FillRule::EvenOdd,
                        other => {
                            return Err(PfError::instantiate(format!(
                                "unknown FillRule `{other}`"
                            )));
                        }
                    });
            }
            "Data" if kind == ElemKind::Shape => {
                self.pending.shape.data = Some(match value {
                    // Element syntax / resource: already-parsed geometry.
                    Resolved::Value(PfValue::Geometry(g)) => g.clone(),
                    other => bevy_pf_xaml::geometry::parse_path_data(&other.to_text()?)?,
                })
            }
            "StrokeStartLineCap" | "StrokeDashCap" => {
                // tiny-skia has a single cap; start cap wins over end cap.
                self.pending.shape.stroke_cap = Some(value.parse_enum()?)
            }
            "StrokeEndLineCap" => {
                if self.pending.shape.stroke_cap.is_none() {
                    self.pending.shape.stroke_cap = Some(value.parse_enum()?);
                }
            }
            "StrokeLineJoin" => self.pending.shape.stroke_join = Some(value.parse_enum()?),
            "StrokeMiterLimit" => self.pending.shape.stroke_miter_limit = Some(value.to_f32()?),
            "StrokeDashArray" => {
                self.pending.shape.stroke_dash_array = Some(v::parse_doubles(&value.to_text()?)?)
            }
            "StrokeDashOffset" => self.pending.shape.stroke_dash_offset = Some(value.to_f32()?),

            // Root-only.
            "Title" if kind == ElemKind::Root => {
                let title = value.to_text()?;
                let mut q = self
                    .world
                    .query_filtered::<&mut bevy::window::Window, With<bevy::window::PrimaryWindow>>(
                    );
                if let Ok(mut window) = q.single_mut(self.world) {
                    window.title = title;
                }
            }

            // Consumed elsewhere ("Style" by effective_style, the rest by
            // spawn_content).
            "Style" | "Text" | "Content" | "Source" | "Header" => {}

            // Recognized but deliberately ignored (design-time / app-level).
            "ShowGridLines"
            | "SizeToContent"
            | "WindowStartupLocation"
            | "Icon"
            | "ResizeMode"
            | "WindowStyle"
            | "WindowState"
            | "IsDefault"
            | "IsCancel"
            | "SnapsToDevicePixels"
            | "UseLayoutRounding"
            | "Focusable"
            | "TabIndex"
            | "ClipToBounds"
            | "LastChildFill"
            | "IsReadOnly"
            | "IsIndeterminate"
            | "SmallChange"
            | "LargeChange"
            | "TickPlacement"
            | "TickFrequency"
            | "IsSnapToTickEnabled"
            | "SelectionMode"
            | "AcceptsTab"
            | "IsThreeState"
            | "CharacterCasing"
            | "PasswordChar"
            | "RenderTransformOrigin"
            | "LayoutTransform"
            | "FocusVisualStyle"
            | "StartupUri"
            | "OverridesDefaultStyle"
            | "Cursor" => {}

            // Unknown/custom elements own their attribute surface: the
            // registered builder reads them off the node, and unregistered
            // ones already warned at the element level.
            _ if kind == ElemKind::Unknown => {}
            other => {
                self.warn(format!(
                    "property `{other}` on `{kind:?}` is not supported yet"
                ));
            }
        }
        Ok(())
    }

    fn apply_h_alignment(
        &mut self,
        entity: Entity,
        parent_kind: ParentKind,
        a: v::HorizontalAlignment,
    ) {
        match parent_kind {
            ParentKind::Grid => {
                self.node_mut(entity).justify_self = convert::h_justify_self(a);
            }
            ParentKind::FlexColumn => {
                self.node_mut(entity).align_self = convert::h_align_self(a);
            }
            ParentKind::Dock => self.stash_dock_alignment(entity, "HAlign", format!("{a:?}")),
            // The document root sits in the Root wrapper's single grid
            // cell. WPF: a non-Stretch alignment sizes the element to
            // content on that axis instead of filling the window (the
            // forced window constraint is the Stretch default).
            ParentKind::None => {
                if a != v::HorizontalAlignment::Stretch {
                    let mut node = self.node_mut(entity);
                    if node.width == Val::Percent(100.0) {
                        node.width = Val::Auto;
                    }
                    node.justify_self = convert::h_justify_self(a);
                }
            }
            // Main-axis alignment in a horizontal stack: WPF ignores it.
            ParentKind::FlexRow | ParentKind::Canvas => {}
        }
    }

    fn apply_v_alignment(
        &mut self,
        entity: Entity,
        parent_kind: ParentKind,
        a: v::VerticalAlignment,
    ) {
        match parent_kind {
            ParentKind::Grid => {
                self.node_mut(entity).align_self = convert::v_align_self(a);
            }
            ParentKind::FlexRow => {
                self.node_mut(entity).align_self = convert::v_align_self(a);
            }
            ParentKind::Dock => self.stash_dock_alignment(entity, "VAlign", format!("{a:?}")),
            // Root grid cell: same non-Stretch -> size-to-content rule as
            // the horizontal case above.
            ParentKind::None => {
                if a != v::VerticalAlignment::Stretch {
                    let mut node = self.node_mut(entity);
                    if node.height == Val::Percent(100.0) {
                        node.height = Val::Auto;
                    }
                    node.align_self = convert::v_align_self(a);
                }
            }
            ParentKind::FlexColumn | ParentKind::Canvas => {}
        }
    }

    /// Record alignment on a DockPanel child for later resolution (the
    /// wrapper axis depends on the child's Dock value).
    fn stash_dock_alignment(&mut self, entity: Entity, key: &str, value: String) {
        let mut e = self.world.entity_mut(entity);
        if let Some(mut attached) = e.get_mut::<PfAttachedProps>() {
            attached.0.insert(format!("Pf.{key}"), value);
        } else {
            let mut map = bevy::platform::collections::HashMap::default();
            map.insert(format!("Pf.{key}"), value);
            e.insert(PfAttachedProps(map));
        }
    }

    // -----------------------------------------------------------------
    // Grid specifics
    // -----------------------------------------------------------------

    fn configure_grid_tracks(&mut self, entity: Entity, node: &XamlNode) {
        let rows = self.tracks_from_defs(node, "RowDefinitions", "RowDefinition", "Height");
        let cols = self.tracks_from_defs(node, "ColumnDefinitions", "ColumnDefinition", "Width");
        let mut ui_node = self.node_mut(entity);
        if let Some(rows) = rows {
            ui_node.grid_template_rows = rows;
        } else if ui_node.grid_template_rows.is_empty() {
            ui_node.grid_template_rows = vec![GridTrack::fr(1.0)];
        }
        if let Some(cols) = cols {
            ui_node.grid_template_columns = cols;
        } else if ui_node.grid_template_columns.is_empty() {
            ui_node.grid_template_columns = vec![GridTrack::fr(1.0)];
        }
    }

    fn tracks_from_defs(
        &mut self,
        node: &XamlNode,
        collection: &str,
        item: &str,
        length_attr: &str,
    ) -> Option<Vec<RepeatedGridTrack>> {
        let pe = node.property_element(collection)?;
        let mut tracks = Vec::new();
        for def in pe.elements() {
            if def.name != item {
                self.warn(format!(
                    "{}: unexpected `{}` in {collection}",
                    def.pos, def.name
                ));
                continue;
            }
            let length = match def.attribute(length_attr) {
                Some(XamlValue::Str(s)) => match s.parse::<v::GridLength>() {
                    Ok(l) => l,
                    Err(e) => {
                        self.warn(format!("{}: {e}", def.pos));
                        v::GridLength::Star(1.0)
                    }
                },
                _ => v::GridLength::Star(1.0), // WPF default is "*"
            };
            tracks.push(convert::grid_track(length));
        }
        if tracks.is_empty() {
            tracks.push(GridTrack::fr(1.0));
        }
        Some(tracks)
    }

    /// Read a grid child's attached placement (with WPF-style clamping).
    fn place_grid_child(&mut self, child: Entity, row_count: usize, col_count: usize) {
        let attached = self
            .world
            .get::<PfAttachedProps>(child)
            .cloned()
            .unwrap_or_default();
        let row: i64 = attached.parse("Grid", "Row").unwrap_or(0);
        let col: i64 = attached.parse("Grid", "Column").unwrap_or(0);
        let row_span: u16 = attached.parse("Grid", "RowSpan").unwrap_or(1).max(1);
        let col_span: u16 = attached.parse("Grid", "ColumnSpan").unwrap_or(1).max(1);
        let row = row.clamp(0, row_count.saturating_sub(1) as i64) as i16;
        let col = col.clamp(0, col_count.saturating_sub(1) as i64) as i16;
        // WPF clamps spans to the grid edge (Grid.cs:962-966); without this,
        // CSS grid would create phantom implicit tracks.
        let row_span = row_span.min((row_count as i64 - row as i64).max(1) as u16);
        let col_span = col_span.min((col_count as i64 - col as i64).max(1) as u16);
        let mut node = self.node_mut(child);
        node.grid_row = GridPlacement::start_span(row + 1, row_span);
        node.grid_column = GridPlacement::start_span(col + 1, col_span);
    }

    fn place_canvas_child(&mut self, child: Entity) {
        let attached = self
            .world
            .get::<PfAttachedProps>(child)
            .cloned()
            .unwrap_or_default();
        let mut node = self.node_mut(child);
        node.position_type = PositionType::Absolute;
        if let Some(left) = attached.parse::<f32>("Canvas", "Left") {
            node.left = Val::Px(left);
        }
        if let Some(top) = attached.parse::<f32>("Canvas", "Top") {
            node.top = Val::Px(top);
        }
        // WPF: Left beats Right, Top beats Bottom.
        if attached.get("Canvas", "Left").is_none()
            && let Some(right) = attached.parse::<f32>("Canvas", "Right")
        {
            node.right = Val::Px(right);
        }
        if attached.get("Canvas", "Top").is_none()
            && let Some(bottom) = attached.parse::<f32>("Canvas", "Bottom")
        {
            node.bottom = Val::Px(bottom);
        }
    }

    // -----------------------------------------------------------------
    // Content
    // -----------------------------------------------------------------

    fn spawn_content(
        &mut self,
        entity: Entity,
        kind: ElemKind,
        node: &XamlNode,
    ) -> Result<(), PfError> {
        // A ControlTemplate replaces the default chrome of template-capable
        // controls. Move the payload out of pending first: no borrow may be
        // held across the &mut self expansion.
        if self.pending.control_template.is_some() {
            match kind {
                ElemKind::Button
                | ElemKind::ToggleButton
                | ElemKind::CheckBox
                | ElemKind::RadioButton
                | ElemKind::Label
                | ElemKind::Expander
                | ElemKind::ComboBox
                | ElemKind::ScrollBar
                | ElemKind::TextBox => {
                    let (template, scopes) =
                        self.pending.control_template.take().expect("checked above");
                    return self.expand_control_template(entity, kind, node, template, scopes);
                }
                other => {
                    self.pending.control_template = None;
                    self.warn(format!(
                        "Template on `{other:?}` is not supported yet; template ignored"
                    ));
                }
            }
        }
        match kind {
            // Track content arrives via its property elements
            // (<Track.Thumb>, page buttons); Thumb is a leaf grip.
            ElemKind::Track | ElemKind::Thumb => {}
            ElemKind::TextBlock => {
                // WPF inlines with anchors: flow text runs and Hyperlinks as
                // real children instead of flattening the link away.
                let has_inline_links = node.attribute("Text").is_none()
                    && node
                        .children
                        .iter()
                        .any(|c| matches!(c, XamlChild::Element(el) if el.name == "Hyperlink"));
                if has_inline_links {
                    if let Some(mut n) = self.world.get_mut::<Node>(entity) {
                        n.display = Display::Flex;
                        n.flex_wrap = FlexWrap::Wrap;
                        n.align_items = AlignItems::Center;
                    }
                    let children: Vec<XamlChild> = node.children.clone();
                    let mut spawned = Vec::new();
                    for child in &children {
                        match child {
                            XamlChild::Text(t) => {
                                let t = t.trim();
                                if !t.is_empty() {
                                    spawned.push(self.spawn_text_child(t.to_string()));
                                }
                            }
                            XamlChild::Element(el) if el.name == "Hyperlink" => {
                                spawned.push(self.spawn_element(el, ParentKind::FlexRow, None)?);
                            }
                            XamlChild::Element(el) => {
                                let flat = collect_inline_text(el);
                                if !flat.trim().is_empty() {
                                    spawned.push(self.spawn_text_child(flat));
                                }
                            }
                        }
                    }
                    self.add_children(entity, &spawned);
                    return Ok(());
                }
                let text = match node.attribute("Text") {
                    Some(value) => {
                        let value = value.clone();
                        self.resolve_text_attr(&value)
                    }
                    None => Some(collect_inline_text(node)),
                };
                let has_text_binding = self.pending.bindings.iter().any(|(p, _)| p == "Text");
                if let Some(text) = text {
                    self.insert_text_components(entity, text);
                } else if has_text_binding {
                    // Bound text: start empty; the binding system fills it in.
                    self.insert_text_components(entity, String::new());
                }
            }
            ElemKind::Image => {
                if let Some(XamlValue::Str(path)) = node.attribute("Source") {
                    let path = path.clone();
                    if let Some(assets) = self.world.get_resource::<bevy::asset::AssetServer>() {
                        let image: Handle<Image> = assets.load(path);
                        let mut image = bevy::ui::widget::ImageNode::new(image);
                        // Nine-slice: `Slice="24"` / `Slice="8,4,8,4"` (WPF
                        // Thickness order l,t,r,b) stretches the center and
                        // edges while corners keep their pixel size — the
                        // A nine-slice border mapped onto bevy's texture slicer.
                        if let Some(XamlValue::Str(slice)) = node.attribute("Slice") {
                            match slice.parse::<v::Thickness>() {
                                Ok(t) => {
                                    image.image_mode = bevy::ui::widget::NodeImageMode::Sliced(
                                        bevy::sprite::TextureSlicer {
                                            border: bevy::sprite::BorderRect {
                                                min_inset: Vec2::new(t.left, t.top),
                                                max_inset: Vec2::new(t.right, t.bottom),
                                            },
                                            ..Default::default()
                                        },
                                    );
                                }
                                Err(e) => {
                                    self.warn(format!("{}: bad Slice thickness: {e}", node.pos))
                                }
                            }
                        }
                        self.world.entity_mut(entity).insert(image);
                    } else {
                        self.warn("Image.Source ignored: no AssetServer".to_string());
                    }
                }
            }
            ElemKind::ContentPresenter => {
                if let Some(tc) = self.template_ctx.last_mut() {
                    if tc.content_host.is_none() {
                        tc.content_host = Some(entity);
                    } else {
                        self.warn(
                            "a ControlTemplate projects Content into one ContentPresenter; \
                             extra presenter left empty"
                                .to_string(),
                        );
                    }
                    // Content is projected later, in the templated parent's
                    // frame (its Pending is unreachable here).
                } else {
                    self.warn(format!(
                        "{}: ContentPresenter outside a ControlTemplate is a plain container",
                        node.pos
                    ));
                    let children = self.spawn_child_elements(node, ParentKind::Grid)?;
                    self.attach_children(entity, &children, ParentKind::Grid);
                }
            }
            ElemKind::Button
            | ElemKind::ToggleButton
            | ElemKind::Label
            | ElemKind::Root
            | ElemKind::ListBoxItem => {
                self.spawn_content_control_children(entity, kind, node)?;
                if kind == ElemKind::ToggleButton {
                    self.finish_toggle_button(entity);
                }
            }
            ElemKind::CheckBox | ElemKind::RadioButton => {
                self.spawn_toggle_control(entity, kind, node)?;
            }
            ElemKind::TextBox => {
                let pending = self.pending.clone();
                let is_password = node.name == "PasswordBox";
                let mask = match node.attribute("PasswordChar") {
                    Some(XamlValue::Str(c)) => c.chars().next().unwrap_or('\u{2022}'),
                    _ => '\u{2022}',
                };
                let mut text = match node.attribute("Text") {
                    Some(value) => {
                        let value = value.clone();
                        self.resolve_text_attr(&value).unwrap_or_default()
                    }
                    None => String::new(),
                };
                let mut password = String::new();
                if is_password {
                    if let Some(XamlValue::Str(p)) = node.attribute("Password") {
                        password = p.clone();
                    } else {
                        password = std::mem::take(&mut text);
                    }
                    // Only mask characters ever render.
                    text = std::iter::repeat_n(mask, password.chars().count()).collect();
                }
                let mut editable = bevy::text::EditableText::new(&text);
                editable.allow_newlines = pending.accepts_return;
                editable.max_characters = pending.max_length;
                let inherited = self.inherited.clone();
                let input = self
                    .world
                    .spawn((
                        Node {
                            flex_grow: 1.0,
                            ..Default::default()
                        },
                        editable,
                        bevy::text::TextColor(convert::color(inherited.foreground)),
                    ))
                    .id();
                // `IsTabStop="False"`: editable and clickable, but out of the
                // Tab ring.
                let tab_stop = !matches!(
                    node.attribute("IsTabStop"),
                    Some(XamlValue::Str(v)) if v.eq_ignore_ascii_case("false")
                );
                if tab_stop {
                    self.world
                        .entity_mut(input)
                        .insert(bevy::input_focus::tab_navigation::TabIndex(0));
                }
                self.apply_text_font(input);
                // Click-to-focus for the WHOLE control, not just the
                // editable child. bevy routes a press through a bubbling
                // `AcquireFocus`: each hop with a `TabIndex` claims it, and an
                // unclaimed event reaches the window and CLEARS focus. The
                // editable child claims its own presses, but a press on the
                // control's padding or border started a walk that found
                // nothing — so clicking a TextBox's edge silently unfocused
                // it. Intercepting the walk at the control redirects it to the
                // editable; racing the press with a direct focus write loses,
                // because the window fallback applies at command flush, after
                // any observer ran.
                let editable_child = input;
                self.world.entity_mut(entity).observe(
                    move |mut acquire: On<bevy::input_focus::AcquireFocus>,
                          mut focus: ResMut<bevy::input_focus::InputFocus>| {
                        acquire.propagate(false);
                        if focus.get() != Some(editable_child) {
                            focus.set(editable_child, bevy::input_focus::FocusCause::Navigated);
                        }
                    },
                );
                if is_password {
                    self.world
                        .entity_mut(input)
                        .insert(crate::components::PfPasswordInput { owner: entity });
                    self.world
                        .entity_mut(entity)
                        .insert(crate::components::PfPasswordBox {
                            input,
                            password,
                            mask,
                        });
                    // WPF: Password is not bindable (security); Text bindings
                    // would fight the masking.
                    self.pending.text_input = None;
                } else {
                    self.pending.text_input = Some(input);
                }
                self.add_children(entity, &[input]);
                // Toolkit watermark/placeholder: grey overlay while empty.
                let watermark = node
                    .attribute("Watermark")
                    .or_else(|| node.attribute("PlaceholderText"));
                if let Some(XamlValue::Str(w)) = watermark {
                    let w = w.clone();
                    let label = self.spawn_text_child(w);
                    self.world.entity_mut(label).insert((
                        bevy::text::TextColor(Color::srgb_u8(0x9A, 0x9A, 0x9A)),
                        bevy::picking::Pickable::IGNORE,
                    ));
                    let overlay = self
                        .world
                        .spawn((
                            Node {
                                // Cover the control and centre like the real
                                // text does, instead of the old hardcoded
                                // left:4/top:2 that ignored the control's
                                // padding (a tall padded field drew its
                                // placeholder pinned top-left while typed
                                // text sat centre-left). Padding is copied
                                // from the control every frame by
                                // `toolkit_control_sync`, so style-applied
                                // padding stays honoured too.
                                position_type: PositionType::Absolute,
                                left: Val::Px(0.0),
                                right: Val::Px(0.0),
                                top: Val::Px(0.0),
                                bottom: Val::Px(0.0),
                                align_items: AlignItems::Center,
                                display: if text.is_empty() {
                                    Display::Flex
                                } else {
                                    Display::None
                                },
                                ..Default::default()
                            },
                            BackgroundColor(Color::NONE),
                            bevy::picking::Pickable::IGNORE,
                        ))
                        .id();
                    self.world.entity_mut(overlay).add_children(&[label]);
                    self.add_children(entity, &[overlay]);
                    self.world
                        .entity_mut(entity)
                        .insert(crate::components::PfWatermark { overlay });
                }
            }
            ElemKind::Slider => {
                let pending = self.pending.clone();
                self.spawn_slider(entity, &pending);
            }
            ElemKind::ProgressBar => {
                let pending = self.pending.clone();
                let min = pending.minimum.unwrap_or(0.0);
                let max = pending.maximum.unwrap_or(100.0);
                let value = pending.value.unwrap_or(min);
                let progress = PfProgress {
                    min,
                    max,
                    value,
                    indeterminate: self.pending.is_indeterminate,
                };
                // WPF fills the bar with the control's Foreground brush; the
                // stock green stands in only when none was set on the element.
                let fill_color = pending
                    .own_foreground
                    .map(crate::convert::color)
                    .unwrap_or(Color::srgb_u8(0x06, 0xB0, 0x25));
                let fill = self
                    .world
                    .spawn((
                        Node {
                            width: Val::Percent(progress.fraction() * 100.0),
                            ..Default::default()
                        },
                        BackgroundColor(fill_color),
                    ))
                    .id();
                self.world
                    .entity_mut(entity)
                    .insert((progress, PfProgressVisual { fill }));
                self.add_children(entity, &[fill]);
            }
            ElemKind::Separator => {}
            ElemKind::Shape => {
                let shape = crate::shapes::build_shape(&node.name, self.pending.shape.clone());
                match shape {
                    Some(shape) => {
                        // Stretch=None shapes get their natural geometry size
                        // unless an explicit size was set.
                        if shape.stretch == v::Stretch::None
                            && let Some(size) = shape.natural_size()
                        {
                            let mut ui_node = self.node_mut(entity);
                            if ui_node.width == Val::Auto {
                                ui_node.width = Val::Px(size.x);
                            }
                            if ui_node.height == Val::Auto {
                                ui_node.height = Val::Px(size.y);
                            }
                        }
                        let mut shape = shape;
                        // The store may already hold Fill/Stroke: a
                        // {TemplateBinding} resolves before the shape is
                        // built, and writing into a component that does not
                        // exist yet loses the value silently.
                        if let Some(store) =
                            self.world.get::<crate::provider::PfPropertyStore>(entity)
                        {
                            for (target, slot) in [
                                (crate::provider::PropertyTarget::Fill, 0),
                                (crate::provider::PropertyTarget::Stroke, 1),
                            ] {
                                if let Some((_, Some(PfValue::Brush(brush)))) =
                                    store.effective(target)
                                {
                                    let brush = brush.clone();
                                    if slot == 0 {
                                        shape.fill = Some(brush);
                                    } else {
                                        shape.stroke = Some(brush);
                                    }
                                }
                            }
                        }
                        self.world
                            .entity_mut(entity)
                            .insert((shape, crate::shapes::PfShapeRendered::default()));
                    }
                    None => self.warn(format!(
                        "{}: shape `{}` is missing its geometry (Points/Data)",
                        node.pos, node.name
                    )),
                }
            }
            ElemKind::UniformGrid => {
                let pending = self.pending.clone();
                let children = self.spawn_child_elements(node, ParentKind::Grid)?;
                let n = children.len().max(1) as u16;
                let cols = pending
                    .columns
                    .unwrap_or_else(|| (n as f32).sqrt().ceil() as u16)
                    .max(1);
                let rows = pending.rows.unwrap_or_else(|| n.div_ceil(cols)).max(1);
                {
                    let mut ui_node = self.node_mut(entity);
                    ui_node.grid_template_columns = vec![RepeatedGridTrack::fr(cols, 1.0)];
                    ui_node.grid_template_rows = vec![RepeatedGridTrack::fr(rows, 1.0)];
                }
                // Children auto-flow row-major into equal cells (WPF order).
                self.add_children(entity, &children);
            }
            ElemKind::ListBox | ElemKind::ItemsControl => {
                // ListView with a GridView view is column mode (details view).
                let grid_view = if node.name == "ListView" {
                    node.property_element("View")
                        .and_then(|v| v.elements().find(|e| e.name == "GridView"))
                        .cloned()
                } else {
                    None
                };
                if let Some(gv) = grid_view {
                    self.spawn_list_view(entity, &gv)?;
                } else {
                    let pending = self.pending.clone();
                    let children = self.spawn_child_elements(node, ParentKind::FlexColumn)?;
                    if kind == ElemKind::ItemsControl {
                        self.add_children(entity, &children);
                    } else {
                        self.finish_list_box(entity, node, &children, &pending);
                    }
                }
            }
            ElemKind::GroupBox => {
                let header = self.spawn_header(node, true)?;
                let children = self.spawn_child_elements(node, ParentKind::FlexColumn)?;
                if let Some(h) = header {
                    self.add_children(entity, &[h]);
                }
                self.add_children(entity, &children);
            }
            ElemKind::Expander => {
                self.spawn_expander(entity, node)?;
            }
            ElemKind::ScrollBar => {
                let pending = self.pending.clone();
                self.spawn_scroll_bar(entity, node, &pending)?;
            }
            ElemKind::ComboBox => {
                let pending = self.pending.clone();
                self.spawn_combo_box(entity, node, &pending)?;
            }
            ElemKind::TabControl => {
                let pending = self.pending.clone();
                self.spawn_tab_control(entity, node, &pending)?;
            }
            ElemKind::TreeView => {
                self.spawn_tree_view(entity, node)?;
            }
            ElemKind::Menu => {
                self.spawn_menu(entity, node)?;
            }
            ElemKind::DataGrid => {
                self.spawn_data_grid(entity, node)?;
            }
            ElemKind::StatusBar
            | ElemKind::StatusBarItem
            | ElemKind::ToolBar
            | ElemKind::ToolBarTray => {
                let children = self.spawn_child_elements(node, ParentKind::FlexRow)?;
                self.add_children(entity, &children);
            }
            ElemKind::Hyperlink => {
                self.spawn_hyperlink(entity, node)?;
            }
            ElemKind::Frame => {
                self.spawn_frame(entity, node);
            }
            ElemKind::ToggleSwitch => self.spawn_toggle_switch(entity, node),
            ElemKind::NumericUpDown => self.spawn_numeric_up_down(entity, node),
            ElemKind::RatingBar => self.spawn_rating_bar(entity, node),
            ElemKind::Badge => self.spawn_badge(entity, node)?,
            ElemKind::BusyIndicator => self.spawn_busy_indicator(entity, node)?,
            ElemKind::RangeSlider => self.spawn_range_slider(entity, node),
            ElemKind::TimePicker => self.spawn_time_picker(entity, node)?,
            ElemKind::PackIcon => self.spawn_pack_icon(entity, node),
            ElemKind::ColorPicker => self.spawn_color_picker(entity, node),
            ElemKind::AutoSuggestBox => self.spawn_auto_suggest_box(entity, node)?,
            ElemKind::NavigationView => self.spawn_navigation_view(entity, node),
            ElemKind::PopupElement => {
                self.spawn_popup_element(entity, node)?;
            }
            ElemKind::GridSplitter => {
                self.spawn_grid_splitter(entity, node);
            }
            ElemKind::Calendar => {
                self.spawn_calendar(entity, node, None)?;
            }
            ElemKind::DatePicker => {
                self.spawn_date_picker(entity, node)?;
            }
            ElemKind::Viewbox => {
                let stretch = self.pending.shape.stretch.unwrap_or(v::Stretch::Uniform);
                let children = self.spawn_child_elements(node, ParentKind::FlexColumn)?;
                if children.len() > 1 {
                    self.warn(format!(
                        "{}: Viewbox supports a single child; extra children stacked",
                        node.pos
                    ));
                }
                self.world
                    .entity_mut(entity)
                    .insert(crate::components::PfViewbox { stretch });
                self.add_children(entity, &children);
            }
            ElemKind::Grid => {
                let children = self.spawn_child_elements(node, ParentKind::Grid)?;
                let (rows, cols) = {
                    let n = self.node_mut(entity);
                    (
                        n.grid_template_rows.len().max(1),
                        n.grid_template_columns.len().max(1),
                    )
                };
                for &child in &children {
                    self.place_grid_child(child, rows, cols);
                }
                self.add_children(entity, &children);
            }
            ElemKind::Canvas => {
                let children = self.spawn_child_elements(node, ParentKind::Canvas)?;
                for &child in &children {
                    self.place_canvas_child(child);
                }
                self.add_children(entity, &children);
            }
            ElemKind::DockPanel => {
                let children = self.spawn_child_elements(node, ParentKind::Dock)?;
                let last_child_fill = !matches!(
                    node.attribute("LastChildFill"),
                    Some(XamlValue::Str(s)) if s.eq_ignore_ascii_case("false")
                );
                // Faithful WPF DockPanel via nested flex wrappers: each docked
                // child gets a wrapper in its dock direction, with the rest of
                // the panel nested inside.
                {
                    let mut ui_node = self.node_mut(entity);
                    ui_node.display = Display::Grid;
                    ui_node.grid_template_rows = vec![GridTrack::fr(1.0)];
                    ui_node.grid_template_columns = vec![GridTrack::fr(1.0)];
                }
                if let Some(chain) = self.build_dock_chain(&children, last_child_fill) {
                    self.add_children(entity, &[chain]);
                }
            }
            ElemKind::StackPanel | ElemKind::WrapPanel | ElemKind::Unknown => {
                let orientation = self.orientation_of(node);
                let parent = kind.as_parent(orientation);
                let children = self.spawn_child_elements(node, parent)?;
                if matches!(kind, ElemKind::StackPanel | ElemKind::WrapPanel) {
                    // WPF stacking panels arrange children at exactly their
                    // desired size — they overflow, never shrink
                    // (Stack.cs:559-570; conformance note L6).
                    for &child in &children {
                        self.node_mut(child).flex_shrink = 0.0;
                    }
                }
                self.add_children(entity, &children);
            }
            ElemKind::Border | ElemKind::ScrollViewer => {
                let children = self.spawn_child_elements(node, ParentKind::Grid)?;
                if children.len() > 1 {
                    self.warn(format!(
                        "{}: `{}` supports a single child; extra children stacked",
                        node.pos, node.name
                    ));
                }
                self.attach_children(entity, &children, ParentKind::Grid);
            }
        }
        Ok(())
    }

    fn orientation_of(&self, node: &XamlNode) -> v::Orientation {
        match node.attribute("Orientation") {
            Some(XamlValue::Str(s)) => s.parse().unwrap_or_default(),
            _ => match ElemKind::from_name(&node.name) {
                ElemKind::WrapPanel => v::Orientation::Horizontal,
                _ => v::Orientation::Vertical,
            },
        }
    }

    fn resolve_text_attr(&mut self, value: &XamlValue) -> Option<String> {
        match value {
            XamlValue::Str(s) => Some(s.clone()),
            XamlValue::Extension(ext) => match self.resolve_extension(ext) {
                Ok(Some(resolved)) => resolved.to_text().ok(),
                Ok(None) => None,
                Err(e) => {
                    self.warn(format!("{e}"));
                    None
                }
            },
        }
    }

    fn spawn_child_elements(
        &mut self,
        node: &XamlNode,
        parent: ParentKind,
    ) -> Result<Vec<Entity>, PfError> {
        let mut children = Vec::new();
        // Elements from either direct content or a Content/Child/Children
        // property element.
        let content_pe = node
            .property_elements
            .iter()
            .find(|pe| matches!(pe.name.as_str(), "Content" | "Child" | "Children"));
        let values: Vec<&XamlChild> = match content_pe {
            Some(pe) => pe.values.iter().collect(),
            None => node.children.iter().collect(),
        };
        for child in values {
            match child {
                XamlChild::Element(el) => {
                    children.push(self.spawn_element(el, parent, None)?);
                }
                // Whitespace-only runs between elements are only significant
                // for inline text collections (handled by
                // `collect_inline_text`), not for panel children.
                XamlChild::Text(t) if t.trim().is_empty() => {}
                XamlChild::Text(t) => {
                    let t = t.clone();
                    children.push(self.spawn_text_child(t));
                }
            }
        }
        Ok(children)
    }

    fn attach_children(&mut self, entity: Entity, children: &[Entity], parent: ParentKind) {
        if parent == ParentKind::Grid {
            for &child in children {
                self.place_grid_child(child, 1, 1);
            }
        }
        self.add_children(entity, children);
    }

    fn add_children(&mut self, entity: Entity, children: &[Entity]) {
        if !children.is_empty() {
            self.world.entity_mut(entity).add_children(children);
        }
    }

    /// Spawn a bare text node (implicit TextBlock) for string content.
    fn spawn_text_child(&mut self, text: String) -> Entity {
        let entity = self
            .world
            .spawn((Node::default(), PfElementKind("TextBlock".to_string())))
            .id();
        self.insert_text_components(entity, text);
        entity
    }

    fn insert_text_components(&mut self, entity: Entity, text: String) {
        let inherited = self.inherited.clone();
        let mut layout = bevy::text::TextLayout {
            linebreak: convert::line_break(inherited.text_wrapping),
            ..Default::default()
        };
        if let Some(a) = inherited.text_alignment {
            layout.justify = convert::text_justify(a);
        }
        self.apply_text_font(entity);
        self.world.entity_mut(entity).insert((
            bevy::ui::widget::Text::new(text),
            bevy::text::TextColor(convert::color(inherited.foreground)),
            layout,
        ));
    }

    /// Insert a `TextFont` built from the inherited font properties.
    fn apply_text_font(&mut self, entity: Entity) {
        use bevy::text::{FontSource, TextFont};
        let inherited = &self.inherited;
        let _ = FontSource::Handle(Default::default()); // keep the import used
        let font = TextFont {
            font_size: convert::font_size(inherited.font_size),
            weight: convert::font_weight(inherited.font_weight),
            style: convert::font_style(inherited.font_style),
            // No explicit family -> the embedded UI family, so weight/style
            // resolve on every platform (the web has no system fonts).
            font: match &inherited.font_family {
                Some(family) => crate::fonts::resolve_family(family),
                None => crate::fonts::default_font(),
            },
            ..Default::default()
        };
        self.world.entity_mut(entity).insert(font);
        // A caret and selection highlight only render when the `EditableText`
        // also carries a `TextCursorStyle` — bevy_ui_render's
        // `extract_text_cursor` takes it as a REQUIRED query member, so an input
        // without it is excluded from cursor/selection extraction entirely.
        // Colour it to the foreground so it shows on dark themes (the default
        // SLATE_700 is near-invisible there).
        if self.world.get::<bevy::text::EditableText>(entity).is_some() {
            let cursor_color = convert::color(self.inherited.foreground);
            self.world.entity_mut(entity).insert((
                bevy::text::TextCursorStyle {
                    color: cursor_color,
                    ..Default::default()
                },
                // Blink modulates alpha every frame; it needs the authored
                // colour kept aside or the first write would erode it.
                crate::caret::PfCaretBase(cursor_color),
            ));
        }
    }

    // -----------------------------------------------------------------
    // Control builders
    // -----------------------------------------------------------------

    /// ContentControl semantics: `Content` attribute, child elements, or text.
    /// `{TemplateBinding Prop}` on a template child: apply the templated
    /// parent's current effective value at ParentTemplate tier and register
    /// a liveness link so later parent writes forward automatically.
    fn apply_template_binding(&mut self, entity: Entity, dst_name: &str, ext: &MarkupExtension) {
        let Some(tc) = self.template_ctx.last() else {
            self.warn("{TemplateBinding} outside a ControlTemplate; property skipped".to_string());
            return;
        };
        let parent = tc.templated_parent;
        let Some(src_name) = ext.first_positional_str() else {
            self.warn("TemplateBinding needs a source property".to_string());
            return;
        };
        // `Tag` is WPF's per-instance scratch property, and binding it into a
        // template is the standard way to pass one small piece of data in —
        // a keyboard hint, a badge count — without giving the template a
        // second control to carry it. It is not store-managed (it lives as
        // PfTag), so it cannot forward LIVE; it is read once here, which is
        // all a Tag ever needs since nothing writes it after construction.
        if src_name == "Tag" {
            let tag = self
                .world
                .get::<crate::components::PfTag>(parent)
                .map(|t| t.0.clone());
            match tag {
                Some(tag) => {
                    let kind = self
                        .world
                        .get::<crate::components::PfElementKind>(entity)
                        .map(|k| ElemKind::from_name(&k.0))
                        .unwrap_or(ElemKind::Unknown);
                    if let Err(e) = self.apply_property(
                        entity,
                        kind,
                        ParentKind::Grid,
                        None,
                        dst_name,
                        &Resolved::Str(&tag),
                    ) {
                        self.warn(format!("TemplateBinding Tag -> {dst_name}: {e}"));
                    }
                }
                // No Tag on the control: leave the destination alone rather
                // than writing an empty string over the template's own value.
                None => {}
            }
            return;
        }
        let src = crate::provider::property_target_for(src_name);
        let dst = crate::provider::property_target_for(dst_name);
        let (Some(src), Some(dst)) = (src, dst) else {
            self.warn(format!(
                "TemplateBinding {src_name} -> {dst_name} is outside the \
                 store-managed property set; skipped"
            ));
            return;
        };

        // Liveness link on the parent.
        {
            let mut e = self.world.entity_mut(parent);
            if let Some(mut deps) = e.get_mut::<crate::provider::PfTemplateBindingDependents>() {
                deps.0.push((src, entity, dst));
            } else {
                e.insert(crate::provider::PfTemplateBindingDependents(vec![(
                    src, entity, dst,
                )]));
            }
        }

        // Initial value: the parent's current effective entry, if any.
        let value = self
            .world
            .get::<crate::provider::PfPropertyStore>(parent)
            .and_then(|s| s.effective(src))
            .map(|(_, v)| v.clone());
        if let Some(stored) = value {
            crate::provider::store_and_apply(
                self.world,
                entity,
                dst,
                crate::provider::ValueSource::ParentTemplate,
                stored,
            );
        }
    }

    /// Expand a `ControlTemplate` over a control: strip the default chrome,
    /// spawn the template subtree at ParentTemplate tier under a fresh
    /// per-expansion namescope, then project the control's Content into the
    /// template's ContentPresenter (back in the parent frame, so projected
    /// content gets Local-tier attrs and the page namescope).
    fn expand_control_template(
        &mut self,
        entity: Entity,
        kind: ElemKind,
        node: &XamlNode,
        template: std::sync::Arc<crate::resources::PfControlTemplate>,
        scopes: std::sync::Arc<ResourceScopes>,
    ) -> Result<(), PfError> {
        self.strip_default_chrome(entity);

        // Template literals land at ParentTemplate (9): below Local, above
        // Style — template triggers (tier 10) override them and revert to
        // them on deactivation.
        let saved_tier = self.tier;
        self.tier = crate::provider::ValueSource::ParentTemplate;
        self.template_ctx.push(TemplateCtx {
            templated_parent: entity,
            names: Default::default(),
            content_host: None,
        });
        // The template resolves resources against its declaring scope (app
        // tier backfilled), not the instantiation site's.
        let mut template_scopes = (*scopes).clone();
        if template_scopes.app.is_none() {
            template_scopes.app = self.scopes.app.clone();
        }
        let saved_scopes = std::mem::replace(&mut self.scopes, template_scopes);

        let parent_kind = kind.as_parent(self.orientation_of(node));
        let spawned = self.spawn_element(&template.root, parent_kind, None);

        self.scopes = saved_scopes;
        let tc = self.template_ctx.pop().expect("pushed above");
        self.tier = saved_tier;
        let root_child = spawned?;

        self.add_children(entity, &[root_child]);
        self.world
            .entity_mut(entity)
            .insert(crate::components::PfTemplatedControl {
                template_root: root_child,
            });

        // Project Content. Without a presenter, content simply does not
        // render (WPF semantics).
        if let Some(host) = tc.content_host {
            self.project_content(entity, kind, node, host)?;
        }

        // Persist the per-expansion namescope (WPF GetTemplateChild).
        let parts = crate::components::PfTemplateParts(tc.names);
        self.on_template_applied(entity, kind, node, &parts);
        if !template.state_groups.is_empty() {
            let count = template.state_groups.len();
            self.world
                .entity_mut(entity)
                .insert(crate::animation::PfVisualStates {
                    groups: template.state_groups.clone(),
                    current: vec![None; count],
                    touched: vec![Vec::new(); count],
                });
        }
        if !template.triggers.is_empty() {
            self.attach_triggers(entity, &template.triggers, Some(&parts));
        }
        self.world.entity_mut(entity).insert(parts);

        // Per-kind behavior the default chrome used to set up.
        match kind {
            ElemKind::ToggleButton => self.finish_toggle_button(entity),
            ElemKind::CheckBox | ElemKind::RadioButton => self.toggle_behavior(entity, kind),
            _ => {}
        }
        Ok(())
    }

    /// Per-kind wiring after a template expansion (the PART_ contract).
    /// Runs in the parent frame with the parent's live `pending`, before
    /// the binding attach — so `pending.text_input` set here preserves the
    /// TwoWay `Text` -> EditableText contract.
    fn on_template_applied(
        &mut self,
        entity: Entity,
        kind: ElemKind,
        node: &XamlNode,
        parts: &crate::components::PfTemplateParts,
    ) {
        let _ = entity;
        #[allow(clippy::single_match_else, clippy::match_like_matches_macro)]
        #[allow(clippy::single_match)] // the PART_ table grows per control kind
        match kind {
            ElemKind::ScrollBar => {
                // The template provides Track/Thumb (PART_Track, or the
                // first Track/Thumb descendants); the engine wires the value
                // runtime onto them: SliderValue carrier, thumb drag, and
                // repeat line buttons.
                let track = parts
                    .get("PART_Track")
                    .or_else(|| find_kind_entity(self.world, entity, "Track"));
                let Some(track) = track else {
                    self.warn(
                        "templated ScrollBar has no Track part; it renders inert".to_string(),
                    );
                    return;
                };
                let Some(thumb) = find_kind_entity(self.world, track, "Thumb") else {
                    self.warn(
                        "templated ScrollBar Track has no Thumb; it renders inert".to_string(),
                    );
                    return;
                };
                let attr_f32 = |name: &str| -> Option<f32> {
                    match node.attribute(name) {
                        Some(XamlValue::Str(v)) => v.trim().parse().ok(),
                        _ => None,
                    }
                };
                let horizontal = matches!(
                    node.attribute("Orientation"),
                    Some(XamlValue::Str(o)) if o.eq_ignore_ascii_case("horizontal")
                );
                let minimum = self.pending.minimum.unwrap_or(0.0);
                let maximum = self.pending.maximum.unwrap_or(100.0);
                let value = self
                    .pending
                    .value
                    .unwrap_or(minimum)
                    .clamp(minimum, maximum);
                self.world.entity_mut(entity).insert((
                    bevy::ui_widgets::SliderValue(value),
                    bevy::ui_widgets::SliderRange::new(minimum, maximum),
                    crate::components::PfScrollBar {
                        horizontal,
                        viewport: attr_f32("ViewportSize").unwrap_or(10.0),
                        small_change: attr_f32("SmallChange").unwrap_or(1.0),
                        track,
                        thumb,
                    },
                ));
                let bar = entity;
                self.world.entity_mut(thumb).observe(
                    move |drag: On<Pointer<Drag>>, mut commands: Commands| {
                        let delta = drag.delta;
                        commands.queue(move |world: &mut World| {
                            scroll_bar_drag(world, bar, delta);
                        });
                    },
                );
                // Template RepeatButtons outside the Track are line buttons:
                // first nudges down, last nudges up (WPF template order).
                let repeats = collect_kind_entities(self.world, entity, "RepeatButton");
                let line: Vec<Entity> = repeats
                    .into_iter()
                    .filter(|&r| !is_descendant_of(self.world, r, track))
                    .collect();
                for (i, button) in line.iter().enumerate() {
                    let dir = if i == 0 { -1.0f32 } else { 1.0 };
                    let button = *button;
                    self.world.entity_mut(button).observe(
                        move |_click: On<Pointer<Click>>, mut commands: Commands| {
                            commands.queue(move |world: &mut World| {
                                scroll_bar_nudge(world, bar, dir);
                            });
                        },
                    );
                    if self
                        .world
                        .get::<crate::components::PfRepeatButton>(button)
                        .is_none()
                    {
                        self.world
                            .entity_mut(button)
                            .insert(crate::components::PfRepeatButton {
                                delay: 300.0,
                                interval: 80.0,
                                ..Default::default()
                            });
                    }
                }
            }
            ElemKind::ComboBox => {
                // The template replaces the face; the popup/backdrop/items
                // runtime is engine infrastructure, built here. Selection
                // text lands in PART_SelectionBoxText (or the template's
                // first Text descendant, or a hidden fallback).
                let text = parts
                    .get("PART_SelectionBoxText")
                    .or_else(|| find_text_entity(self.world, entity))
                    .unwrap_or_else(|| {
                        let hidden = self
                            .world
                            .spawn((
                                Node {
                                    display: Display::None,
                                    ..Default::default()
                                },
                                bevy::ui::widget::Text::new(""),
                            ))
                            .id();
                        self.add_children(entity, &[hidden]);
                        hidden
                    });
                let pending = self.pending.clone();
                if let Err(e) = self.build_combo_runtime(entity, node, &pending, text) {
                    self.warn(format!("templated ComboBox runtime failed: {e}"));
                }
            }
            ElemKind::Expander => {
                // The template's ToggleButton part drives Checked through
                // its TwoWay TemplatedParent binding; seed the initial
                // IsExpanded and make the root a trigger host.
                if self.pending.is_checked == Some(true) {
                    self.world.entity_mut(entity).insert(bevy::ui::Checked);
                }
            }
            ElemKind::TextBox => {
                let Some(host) = parts.get("PART_ContentHost") else {
                    self.warn(
                        "templated TextBox has no PART_ContentHost; \
                         it renders without an editing surface"
                            .to_string(),
                    );
                    return;
                };
                let text = match node.attribute("Text") {
                    Some(value) => {
                        let value = value.clone();
                        self.resolve_text_attr(&value).unwrap_or_default()
                    }
                    None => String::new(),
                };
                let mut editable = bevy::text::EditableText::new(&text);
                editable.allow_newlines = self.pending.accepts_return;
                editable.max_characters = self.pending.max_length;
                let input = self
                    .world
                    .spawn((
                        Node {
                            flex_grow: 1.0,
                            ..Default::default()
                        },
                        editable,
                        bevy::input_focus::tab_navigation::TabIndex(0),
                        bevy::text::TextColor(convert::color(self.inherited.foreground)),
                    ))
                    .id();
                self.apply_text_font(input);
                self.pending.text_input = Some(input);
                self.add_children(host, &[input]);
            }
            // Button family and Label define no parts (WPF PART_ table).
            _ => {}
        }
    }

    /// Remove the built-in chrome from a control whose template replaces it.
    /// Kept on the root: Node, PfElementKind, Button marker + Interaction,
    /// PfToggleButton, names, store writes, binding contracts.
    fn strip_default_chrome(&mut self, entity: Entity) {
        use crate::provider::{PropertyTarget, ValueSource};
        // The Default-tier chrome seeds exist to restore default chrome on
        // trigger revert — exactly wrong once chrome is templated.
        if let Some(mut store) = self
            .world
            .get_mut::<crate::provider::PfPropertyStore>(entity)
        {
            store.clear(PropertyTarget::Background, ValueSource::Default);
            store.clear(PropertyTarget::BorderBrush, ValueSource::Default);
        }
        let mut e = self.world.entity_mut(entity);
        e.remove::<BackgroundColor>();
        e.remove::<BorderColor>();
        e.remove::<ButtonVisual>();
        // Padding/border reach the visuals only via TemplateBinding in WPF,
        // and bevy_ui border width takes layout space even without a color.
        // min_width/min_height stay: control sizing defaults, not chrome.
        if let Some(mut n) = self.world.get_mut::<Node>(entity) {
            n.padding = UiRect::ZERO;
            n.border = UiRect::ZERO;
            // Default (= stretch for grid children): the template root fills
            // the control instead of centering like default Button content.
            n.justify_items = JustifyItems::Default;
            n.align_items = AlignItems::Default;
        }
    }

    /// Project the control's Content under the template's ContentPresenter.
    /// Runs in the parent frame (template ctx popped): projected content
    /// gets Local-tier attrs, the enclosing namescope, and a live
    /// `pending.content_text` for the Content binding attach.
    fn project_content(
        &mut self,
        entity: Entity,
        kind: ElemKind,
        node: &XamlNode,
        host: Entity,
    ) -> Result<(), PfError> {
        let _ = entity;
        // Content alignment: what the control ASKED for, else the per-kind
        // default (Button centers, the rest lead-align) that stands in for
        // WPF's per-control HorizontalContentAlignment defaults.
        {
            let (h, v) = (self.pending.h_content_align, self.pending.v_content_align);
            let mut n = self.node_mut(host);
            let centered = matches!(kind, ElemKind::Button | ElemKind::ToggleButton);
            n.justify_items = h.unwrap_or(if centered {
                JustifyItems::Center
            } else {
                JustifyItems::Start
            });
            n.align_items = v.unwrap_or(AlignItems::Center);
        }
        let has_content_binding = self.pending.bindings.iter().any(|(p, _)| p == "Content");
        if has_content_binding {
            let child = self.spawn_text_child(String::new());
            self.pending.content_text = Some(child);
            self.add_children(host, &[child]);
        } else if let Some(value) = node.attribute("Content").cloned() {
            if let Some(text) = self.resolve_text_attr(&value) {
                let child = self.spawn_text_child(text);
                self.add_children(host, &[child]);
            }
        } else {
            let children = self.spawn_child_elements(node, ParentKind::Grid)?;
            self.add_children(host, &children);
        }
        Ok(())
    }

    fn spawn_content_control_children(
        &mut self,
        entity: Entity,
        kind: ElemKind,
        node: &XamlNode,
    ) -> Result<(), PfError> {
        let orientation = self.orientation_of(node);
        let parent = kind.as_parent(orientation);
        // Explicit content alignment beats the per-kind default that the
        // element's Node was spawned with (Button centres, most lead-align).
        // Templated controls take the same override in `project_content`.
        if self.pending.h_content_align.is_some() || self.pending.v_content_align.is_some() {
            let (h, v) = (self.pending.h_content_align, self.pending.v_content_align);
            let mut n = self.node_mut(entity);
            if let Some(h) = h {
                n.justify_items = h;
            }
            if let Some(v) = v {
                n.align_items = v;
            }
        }
        let has_content_binding = self.pending.bindings.iter().any(|(p, _)| p == "Content");
        if has_content_binding {
            // Bound content: a text child that the binding system updates.
            let child = self.spawn_text_child(String::new());
            self.pending.content_text = Some(child);
            self.attach_children(entity, &[child], parent);
        } else if let Some(value) = node.attribute("Content").cloned() {
            if let Some(text) = self.resolve_text_attr(&value) {
                let child = self.spawn_text_child(text);
                self.attach_children(entity, &[child], parent);
            }
        } else {
            let children = self.spawn_child_elements(node, parent)?;
            self.attach_children(entity, &children, parent);
        }
        Ok(())
    }

    fn finish_toggle_button(&mut self, entity: Entity) {
        if self.pending.is_checked.unwrap_or(false) {
            self.world.entity_mut(entity).insert(bevy::ui::Checked);
        }
        self.world
            .entity_mut(entity)
            .observe(bevy::ui_widgets::checkbox_self_update);
    }

    /// Build a CheckBox or RadioButton: [box + glyph] followed by content.
    fn spawn_toggle_control(
        &mut self,
        entity: Entity,
        kind: ElemKind,
        node: &XamlNode,
    ) -> Result<(), PfError> {
        let pending = self.pending.clone();
        let checked = pending.is_checked.unwrap_or(false);
        let is_radio = kind == ElemKind::RadioButton;

        let theme = self.control_theme();
        let box_border = theme.control_border;
        let (box_bg, glyph_vis) = match (is_radio, checked) {
            (false, true) => (theme.accent, Visibility::Inherited),
            (false, false) => (theme.control_face, Visibility::Hidden),
            (true, true) => (theme.control_face, Visibility::Inherited),
            (true, false) => (theme.control_face, Visibility::Hidden),
        };

        // Inner glyph: on-accent square for CheckBox, accent dot for RadioButton.
        let glyph = self
            .world
            .spawn((
                Node {
                    width: Val::Px(8.0),
                    height: Val::Px(8.0),
                    border_radius: if is_radio {
                        BorderRadius::MAX
                    } else {
                        BorderRadius::ZERO
                    },
                    ..Default::default()
                },
                BackgroundColor(if is_radio {
                    theme.accent
                } else {
                    theme.on_accent
                }),
                glyph_vis,
            ))
            .id();
        let box_node = self
            .world
            .spawn((
                Node {
                    width: Val::Px(16.0),
                    height: Val::Px(16.0),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: if is_radio {
                        BorderRadius::MAX
                    } else {
                        BorderRadius::all(Val::Px(2.0))
                    },
                    display: Display::Flex,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    flex_shrink: 0.0,
                    ..Default::default()
                },
                BackgroundColor(box_bg),
                BorderColor::all(box_border),
            ))
            .id();
        self.add_children(box_node, &[glyph]);
        self.add_children(entity, &[box_node]);

        // Content after the box.
        if let Some(value) = node.attribute("Content").cloned() {
            if let Some(text) = self.resolve_text_attr(&value) {
                let child = self.spawn_text_child(text);
                self.add_children(entity, &[child]);
            }
        } else {
            let children = self.spawn_child_elements(node, ParentKind::FlexRow)?;
            self.add_children(entity, &children);
        }

        self.world.entity_mut(entity).insert(PfCheckVisual {
            box_node,
            glyph,
            accent_fills_box: !is_radio,
        });
        self.toggle_behavior(entity, kind);
        Ok(())
    }

    /// The host's control chrome palette ([`crate::components::PfControlTheme`]),
    /// falling back to the stock WPF look.
    fn control_theme(&self) -> crate::components::PfControlTheme {
        self.world
            .get_resource::<crate::components::PfControlTheme>()
            .cloned()
            .unwrap_or_default()
    }

    /// The behavioral half of CheckBox/RadioButton — always runs, templated
    /// or not: `Interaction` (trigger conditions and click observers bind to
    /// the root and are inserted nowhere else), the `Checked` seed, and the
    /// group/self-update click wiring.
    fn toggle_behavior(&mut self, entity: Entity, kind: ElemKind) {
        let pending = self.pending.clone();
        let checked = pending.is_checked.unwrap_or(false);
        let is_radio = kind == ElemKind::RadioButton;

        let mut e = self.world.entity_mut(entity);
        e.insert(Interaction::default());
        if checked {
            e.insert(bevy::ui::Checked);
        }

        if is_radio {
            let group = pending.group_name.clone().unwrap_or_default();
            self.world.entity_mut(entity).insert(PfRadioGroup(group));
            let radio = entity;
            self.world.entity_mut(entity).observe(
                move |_click: On<Pointer<Click>>,
                      radios: Query<(Entity, &PfRadioGroup, Option<&ChildOf>)>,
                      mut commands: Commands| {
                    let Ok((_, group, parent)) = radios.get(radio) else {
                        return;
                    };
                    let my_parent = parent.map(|p| p.parent());
                    for (other, other_group, other_parent) in &radios {
                        if other == radio {
                            continue;
                        }
                        let same_group = if group.0.is_empty() {
                            other_group.0.is_empty()
                                && other_parent.map(|p| p.parent()) == my_parent
                        } else {
                            other_group.0 == group.0
                        };
                        if same_group {
                            commands.entity(other).remove::<bevy::ui::Checked>();
                        }
                    }
                    commands.entity(radio).insert(bevy::ui::Checked);
                },
            );
        } else {
            self.world
                .entity_mut(entity)
                .insert(bevy::ui_widgets::Checkbox);
            self.world
                .entity_mut(entity)
                .observe(bevy::ui_widgets::checkbox_self_update);
        }
    }

    fn spawn_slider(&mut self, entity: Entity, pending: &Pending) {
        use bevy::ui_widgets::{Slider, SliderRange, SliderThumb, SliderValue};

        let min = pending.minimum.unwrap_or(0.0);
        // WPF Slider default Maximum is 10.
        let max = pending.maximum.unwrap_or(10.0).max(min);
        let value = pending.value.unwrap_or(min).clamp(min, max);
        let range = SliderRange::new(min, max);
        let fraction = range.thumb_position(value);

        let theme = self.control_theme();
        const THUMB: f32 = 16.0;
        let track = self
            .world
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    // Extend past the travel wrapper so the track spans the
                    // full slider width (wrapper is inset by the thumb width).
                    right: Val::Px(-THUMB),
                    top: Val::Px(8.0),
                    height: Val::Px(4.0),
                    border_radius: BorderRadius::all(Val::Px(2.0)),
                    ..Default::default()
                },
                BackgroundColor(theme.track),
            ))
            .id();
        let thumb = self
            .world
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(fraction * 100.0),
                    top: Val::Px(2.0),
                    width: Val::Px(THUMB),
                    height: Val::Px(THUMB),
                    border_radius: BorderRadius::MAX,
                    ..Default::default()
                },
                BackgroundColor(theme.accent),
                SliderThumb,
            ))
            .id();
        // Travel wrapper: full width minus one thumb width, so percent
        // positioning of the thumb matches the widget's travel math.
        let travel = self
            .world
            .spawn(Node {
                flex_grow: 1.0,
                height: Val::Percent(100.0),
                margin: UiRect {
                    right: Val::Px(THUMB),
                    ..Default::default()
                },
                ..Default::default()
            })
            .id();
        self.add_children(travel, &[track, thumb]);
        self.add_children(entity, &[travel]);

        self.world.entity_mut(entity).insert((
            Slider::default(),
            SliderValue(value),
            range,
            PfSliderVisual { thumb },
        ));
        self.world
            .entity_mut(entity)
            .observe(bevy::ui_widgets::slider_self_update);
    }

    fn finish_list_box(
        &mut self,
        entity: Entity,
        _node: &XamlNode,
        children: &[Entity],
        pending: &Pending,
    ) {
        let mut items = Vec::with_capacity(children.len());
        for &child in children {
            let already_item = self
                .world
                .get::<PfElementKind>(child)
                .is_some_and(|k| ElemKind::from_name(&k.0) == ElemKind::ListBoxItem);
            let item = if already_item {
                child
            } else {
                let wrapper = self
                    .world
                    .spawn((
                        Node {
                            padding: UiRect::axes(Val::Px(4.0), Val::Px(2.0)),
                            ..Default::default()
                        },
                        PfElementKind("ListBoxItem".to_string()),
                    ))
                    .id();
                self.add_children(wrapper, &[child]);
                wrapper
            };
            self.world.entity_mut(item).insert((
                PfListBoxItem,
                Interaction::default(),
                BackgroundColor(Color::NONE),
            ));
            let list = entity;
            let this = item;
            self.world.entity_mut(item).observe(
                move |_click: On<Pointer<Click>>, mut lists: Query<&mut PfListBox>| {
                    if let Ok(mut list_state) = lists.get_mut(list) {
                        list_state.selected = Some(this);
                    }
                },
            );
            items.push(item);
        }
        self.add_children(entity, &items);

        if let Some(idx) = pending.selected_index
            && let Some(&item) = items.get(idx)
        {
            let selection_fill = self.control_theme().selection_fill;
            if let Some(mut list_state) = self.world.get_mut::<PfListBox>(entity) {
                list_state.selected = Some(item);
            }
            if let Some(mut bg) = self.world.get_mut::<BackgroundColor>(item) {
                bg.0 = selection_fill;
            }
        }
    }

    /// Spawn a `Header` (attribute string, binding, or property element).
    /// `bold` renders string headers semi-bold (GroupBox style).
    fn spawn_header(&mut self, node: &XamlNode, bold: bool) -> Result<Option<Entity>, PfError> {
        if let Some(value) = node.attribute("Header").cloned() {
            if let Some(text) = self.resolve_text_attr(&value) {
                let saved = self.inherited.font_weight;
                if bold {
                    self.inherited.font_weight = v::FontWeight::SEMI_BOLD;
                }
                let e = self.spawn_text_child(text);
                self.inherited.font_weight = saved;
                return Ok(Some(e));
            }
        } else if let Some(pe) = node.property_element("Header") {
            let pe = pe.clone();
            if let Some(el) = pe.single_element() {
                return Ok(Some(self.spawn_element(
                    el,
                    ParentKind::FlexColumn,
                    None,
                )?));
            }
            if let Some(t) = pe.values.iter().find_map(XamlChild::as_text) {
                return Ok(Some(self.spawn_text_child(t.to_string())));
            }
        }
        Ok(None)
    }

    /// Build an Expander: clickable header row + collapsible content.
    /// Build a ScrollBar: line RepeatButtons at both ends, a track, and a
    /// proportional thumb. Value rides `SliderValue`/`SliderRange`.
    fn spawn_scroll_bar(
        &mut self,
        entity: Entity,
        node: &XamlNode,
        pending: &Pending,
    ) -> Result<(), PfError> {
        use crate::components::{PfRepeatButton, PfScrollBar};

        let attr_f32 = |name: &str| -> Option<f32> {
            match node.attribute(name) {
                Some(XamlValue::Str(v)) => v.trim().parse().ok(),
                _ => None,
            }
        };
        let horizontal = matches!(
            node.attribute("Orientation"),
            Some(XamlValue::Str(o)) if o.eq_ignore_ascii_case("horizontal")
        );
        let minimum = pending.minimum.unwrap_or(0.0);
        let maximum = pending.maximum.unwrap_or(100.0);
        let value = pending.value.unwrap_or(minimum).clamp(minimum, maximum);
        let viewport = attr_f32("ViewportSize").unwrap_or(10.0);
        let small_change = attr_f32("SmallChange").unwrap_or(1.0);

        if horizontal && let Some(mut n) = self.world.get_mut::<Node>(entity) {
            {
                n.flex_direction = FlexDirection::Row;
                n.width = Val::Auto;
                n.min_width = Val::Px(60.0);
                n.min_height = Val::Px(0.0);
                n.height = Val::Px(16.0);
            }
        }
        self.world
            .entity_mut(entity)
            .insert(BackgroundColor(Color::srgb_u8(0xF0, 0xF0, 0xF0)));

        let line_button = |world: &mut World| {
            world
                .spawn((
                    Node {
                        width: Val::Px(16.0),
                        height: Val::Px(16.0),
                        flex_shrink: 0.0,
                        ..Default::default()
                    },
                    Interaction::default(),
                    PfRepeatButton {
                        delay: 300.0,
                        interval: 80.0,
                        ..Default::default()
                    },
                    BackgroundColor(Color::srgb_u8(0xDA, 0xDA, 0xDA)),
                ))
                .id()
        };
        let dec = line_button(self.world);
        let inc = line_button(self.world);
        let thumb = self
            .world
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(20.0),
                    ..Default::default()
                },
                Interaction::default(),
                BackgroundColor(Color::srgb_u8(0xC0, 0xC0, 0xC0)),
            ))
            .id();
        let track = self
            .world
            .spawn((
                Node {
                    flex_grow: 1.0,
                    position_type: PositionType::Relative,
                    ..Default::default()
                },
                Interaction::default(),
                BackgroundColor(Color::NONE),
            ))
            .id();
        self.world.entity_mut(track).add_children(&[thumb]);
        self.add_children(entity, &[dec, track, inc]);

        self.world.entity_mut(entity).insert((
            bevy::ui_widgets::SliderValue(value),
            bevy::ui_widgets::SliderRange::new(minimum, maximum),
            PfScrollBar {
                horizontal,
                viewport,
                small_change,
                track,
                thumb,
            },
        ));

        // Line buttons: nudge by SmallChange per (repeat) click.
        let bar = entity;
        for (button, dir) in [(dec, -1.0f32), (inc, 1.0f32)] {
            self.world.entity_mut(button).observe(
                move |_click: On<Pointer<Click>>, mut commands: Commands| {
                    commands.queue(move |world: &mut World| {
                        scroll_bar_nudge(world, bar, dir);
                    });
                },
            );
        }
        // Thumb drag: capture semantics via bevy's drag events.
        self.world.entity_mut(thumb).observe(
            move |drag: On<Pointer<Drag>>, mut commands: Commands| {
                let delta = drag.delta;
                commands.queue(move |world: &mut World| {
                    scroll_bar_drag(world, bar, delta);
                });
            },
        );
        Ok(())
    }

    fn spawn_expander(&mut self, entity: Entity, node: &XamlNode) -> Result<(), PfError> {
        use bevy::ui::Checked;
        let expanded = self.pending.is_checked.unwrap_or(false);

        let arrow = self.spawn_text_child(if expanded { "−" } else { "+" }.to_string());
        let header_row = self
            .world
            .spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(6.0),
                    padding: UiRect::axes(Val::Px(4.0), Val::Px(4.0)),
                    ..Default::default()
                },
                Interaction::default(),
                PfElementKind("Expander.Header".to_string()),
            ))
            .id();
        self.add_children(header_row, &[arrow]);
        if let Some(h) = self.spawn_header(node, false)? {
            self.add_children(header_row, &[h]);
        }

        let children = self.spawn_child_elements(node, ParentKind::FlexColumn)?;
        let content = self
            .world
            .spawn((
                Node {
                    display: if expanded {
                        Display::Flex
                    } else {
                        Display::None
                    },
                    flex_direction: FlexDirection::Column,
                    padding: UiRect {
                        left: Val::Px(18.0),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                PfElementKind("Expander.Content".to_string()),
            ))
            .id();
        self.add_children(content, &children);
        self.add_children(entity, &[header_row, content]);

        let mut e = self.world.entity_mut(entity);
        e.insert(PfExpander { content, arrow });
        if expanded {
            e.insert(Checked);
        }

        let root = entity;
        self.world.entity_mut(header_row).observe(
            move |_click: On<Pointer<Click>>,
                  state: Query<Has<Checked>>,
                  mut commands: Commands| {
                if let Ok(checked) = state.get(root) {
                    if checked {
                        commands.entity(root).remove::<Checked>();
                    } else {
                        commands.entity(root).insert(Checked);
                    }
                }
            },
        );
        Ok(())
    }

    /// Build a ComboBox: chrome (text presenter + arrow) plus a popup under
    /// the overlay layer holding the dropdown items.
    fn spawn_combo_box(
        &mut self,
        entity: Entity,
        node: &XamlNode,
        pending: &Pending,
    ) -> Result<(), PfError> {
        // Chrome: selection text + dropdown arrow.
        let text = self
            .world
            .spawn((
                Node {
                    flex_grow: 1.0,
                    ..Default::default()
                },
                bevy::ui::widget::Text::new(""),
                bevy::text::TextColor(convert::color(self.inherited.foreground)),
            ))
            .id();
        self.apply_text_font(text);
        let arrow_shape = crate::shapes::build_shape(
            "Polygon",
            crate::shapes::ShapeParams {
                points: Some(vec![
                    v::Point::new(0.0, 0.0),
                    v::Point::new(8.0, 0.0),
                    v::Point::new(4.0, 5.0),
                ]),
                fill: Some(v::PfBrush::Solid(v::PfColor::rgb(0x60, 0x60, 0x60))),
                ..Default::default()
            },
        )
        .expect("triangle");
        let arrow = self
            .world
            .spawn((
                Node {
                    width: Val::Px(8.0),
                    height: Val::Px(5.0),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
                arrow_shape,
                crate::shapes::PfShapeRendered::default(),
            ))
            .id();
        self.add_children(entity, &[text, arrow]);
        self.build_combo_runtime(entity, node, pending, text)
    }

    /// The non-visual half of a ComboBox: overlay popup + backdrop, item
    /// wrappers, selection/toggle observers, and the `PfComboBox` state.
    /// Shared by the default chrome and templated ComboBoxes (whose
    /// templates replace only the face).
    fn build_combo_runtime(
        &mut self,
        entity: Entity,
        node: &XamlNode,
        pending: &Pending,
        text: Entity,
    ) -> Result<(), PfError> {
        use crate::components::{PfComboBox, PfComboItem};
        use crate::overlay::{PfPlacement, PfPopup, ensure_overlay_root, spawn_backdrop};

        // Popup + backdrop under the overlay root.
        let theme = self.control_theme();
        let overlay = ensure_overlay_root(self.world);
        let popup = self
            .world
            .spawn((
                PfPopup {
                    anchor: entity,
                    placement: PfPlacement::Bottom,
                    open: false,
                    match_anchor_width: true,
                },
                crate::components::PfLogicalParent(entity),
                Node {
                    position_type: PositionType::Absolute,
                    display: Display::None,
                    flex_direction: FlexDirection::Column,
                    border: UiRect::all(Val::Px(1.0)),
                    padding: UiRect::all(Val::Px(1.0)),
                    max_height: Val::Px(240.0),
                    overflow: bevy::ui::Overflow::scroll_y(),
                    ..Default::default()
                },
                BackgroundColor(theme.popup_face),
                BorderColor::all(theme.popup_border),
                bevy::ui::GlobalZIndex(i32::MAX - 900),
            ))
            .id();
        let backdrop = spawn_backdrop(self.world, popup);
        self.world
            .entity_mut(overlay)
            .add_children(&[backdrop, popup]);

        // Static children become dropdown items (ItemsSource replaces them).
        let children = self.spawn_child_elements(node, ParentKind::FlexColumn)?;
        let mut items = Vec::with_capacity(children.len());
        for (index, &child) in children.iter().enumerate() {
            let wrapper = self
                .world
                .spawn((
                    Node {
                        padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                        ..Default::default()
                    },
                    PfComboItem {
                        combo: entity,
                        index,
                    },
                    Interaction::default(),
                    BackgroundColor(Color::NONE),
                ))
                .id();
            self.add_children(wrapper, &[child]);
            let combo = entity;
            self.world.entity_mut(wrapper).observe(
                move |_click: On<Pointer<Click>>, mut commands: Commands| {
                    commands.queue(move |world: &mut World| {
                        select_combo_index(world, combo, index);
                    });
                },
            );
            items.push(wrapper);
        }
        self.add_children(popup, &items);

        self.world.entity_mut(entity).insert(PfComboBox {
            popup,
            backdrop,
            text,
            selected: None,
            open: false,
        });

        // Toggle on click; light-dismiss via the backdrop.
        let combo = entity;
        self.world.entity_mut(entity).observe(
            move |_click: On<Pointer<Click>>, mut combos: Query<&mut PfComboBox>| {
                if let Ok(mut c) = combos.get_mut(combo) {
                    c.open = !c.open;
                }
            },
        );
        self.world.entity_mut(backdrop).observe(
            move |_click: On<Pointer<Click>>, mut combos: Query<&mut PfComboBox>| {
                if let Ok(mut c) = combos.get_mut(combo) {
                    c.open = false;
                }
            },
        );

        // Initial selection.
        if let Some(index) = pending.selected_index {
            select_combo_index(self.world, entity, index);
        }
        Ok(())
    }

    /// Build a TabControl: a strip of clickable headers over a shared
    /// content host; one content container per TabItem, toggled by selection.
    fn spawn_tab_control(
        &mut self,
        entity: Entity,
        node: &XamlNode,
        pending: &Pending,
    ) -> Result<(), PfError> {
        use crate::components::{PfTabControl, PfTabHeader};

        let strip = self
            .world
            .spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(2.0),
                    ..Default::default()
                },
                PfElementKind("TabControl.Strip".to_string()),
            ))
            .id();
        let host = self
            .world
            .spawn((
                Node {
                    display: Display::Grid,
                    grid_template_rows: vec![GridTrack::fr(1.0)],
                    grid_template_columns: vec![GridTrack::fr(1.0)],
                    border: UiRect::all(Val::Px(1.0)),
                    padding: UiRect::all(Val::Px(8.0)),
                    flex_grow: 1.0,
                    ..Default::default()
                },
                BackgroundColor(Color::WHITE),
                BorderColor::all(Color::srgb_u8(0xAD, 0xAD, 0xAD)),
                PfElementKind("TabControl.Host".to_string()),
            ))
            .id();

        let tab_items: Vec<XamlNode> = node
            .child_elements()
            .filter(|c| c.name == "TabItem")
            .cloned()
            .collect();

        let mut headers = Vec::new();
        let mut contents = Vec::new();
        for (index, item) in tab_items.iter().enumerate() {
            // Header: a clickable pill in the strip.
            let header = self
                .world
                .spawn((
                    Node {
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(5.0)),
                        border: UiRect {
                            left: Val::Px(1.0),
                            right: Val::Px(1.0),
                            top: Val::Px(1.0),
                            bottom: Val::Px(0.0),
                        },
                        ..Default::default()
                    },
                    BackgroundColor(Color::srgb_u8(0xF0, 0xF0, 0xF0)),
                    BorderColor::all(Color::srgb_u8(0xAD, 0xAD, 0xAD)),
                    Interaction::default(),
                    PfTabHeader {
                        tab_control: entity,
                        index,
                    },
                ))
                .id();
            if let Some(h) = self.spawn_header(item, false)? {
                self.add_children(header, &[h]);
            } else if let Some(text) = item.text_content() {
                let t = self.spawn_text_child(text);
                self.add_children(header, &[t]);
            }
            let tab = entity;
            self.world.entity_mut(header).observe(
                move |_click: On<Pointer<Click>>, mut commands: Commands| {
                    commands.queue(move |world: &mut World| {
                        select_tab(world, tab, index);
                    });
                },
            );
            self.add_children(strip, &[header]);
            headers.push(header);

            // Content container (single-cell grid, hidden unless selected).
            let content = self
                .world
                .spawn((
                    Node {
                        display: Display::None,
                        grid_template_rows: vec![GridTrack::fr(1.0)],
                        grid_template_columns: vec![GridTrack::fr(1.0)],
                        ..Default::default()
                    },
                    PfElementKind("TabItem.Content".to_string()),
                ))
                .id();
            let children = self.spawn_child_elements(item, ParentKind::Grid)?;
            self.attach_children(content, &children, ParentKind::Grid);
            self.add_children(host, &[content]);
            contents.push(content);
        }

        self.add_children(entity, &[strip, host]);
        self.world.entity_mut(entity).insert(PfTabControl {
            headers,
            contents,
            selected: usize::MAX, // force the initial select_tab to apply
        });
        let initial = pending.selected_index.unwrap_or(0);
        select_tab(self.world, entity, initial);
        Ok(())
    }

    /// Build a TreeView from nested TreeViewItems.
    fn spawn_tree_view(&mut self, entity: Entity, node: &XamlNode) -> Result<(), PfError> {
        let items: Vec<XamlNode> = node.child_elements().cloned().collect();
        let mut roots = Vec::new();
        for item in &items {
            if item.name != "TreeViewItem" {
                self.warn(format!(
                    "{}: `{}` inside TreeView is not supported yet",
                    item.pos, item.name
                ));
                continue;
            }
            roots.push(self.build_tree_item(entity, item)?);
        }
        self.add_children(entity, &roots);
        Ok(())
    }

    fn build_tree_item(&mut self, tree: Entity, node: &XamlNode) -> Result<Entity, PfError> {
        use crate::components::{PfTreeHeader, PfTreeItem};

        let saved_pending = std::mem::take(&mut self.pending);
        // Per-item attributes (IsExpanded, Header) live on the item node.
        for attr in &node.attributes {
            if attr.name == "IsExpanded"
                && let XamlValue::Str(v) = &attr.value
            {
                self.pending.is_checked = Some(v.trim().eq_ignore_ascii_case("true"));
            }
        }
        let expanded = self.pending.is_checked.unwrap_or(false);
        self.pending = saved_pending;

        let child_items: Vec<XamlNode> = node
            .child_elements()
            .filter(|c| c.name == "TreeViewItem")
            .cloned()
            .collect();
        let has_children = !child_items.is_empty();

        let item = self
            .world
            .spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    ..Default::default()
                },
                PfElementKind("TreeViewItem".to_string()),
            ))
            .id();

        // Header row: arrow + header content.
        let arrow = self.spawn_text_child(
            if !has_children {
                " "
            } else if expanded {
                "−"
            } else {
                "+"
            }
            .to_string(),
        );
        let header_row = self
            .world
            .spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(6.0),
                    padding: UiRect::axes(Val::Px(4.0), Val::Px(2.0)),
                    ..Default::default()
                },
                BackgroundColor(Color::NONE),
                Interaction::default(),
                PfTreeHeader { tree, item },
            ))
            .id();
        self.add_children(header_row, &[arrow]);
        if let Some(h) = self.spawn_header(node, false)? {
            self.add_children(header_row, &[h]);
        }

        // Children container, indented.
        let container = self
            .world
            .spawn((
                Node {
                    display: if expanded {
                        Display::Flex
                    } else {
                        Display::None
                    },
                    flex_direction: FlexDirection::Column,
                    margin: UiRect {
                        left: Val::Px(18.0),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                PfElementKind("TreeViewItem.Children".to_string()),
            ))
            .id();
        let mut nested = Vec::new();
        for child in &child_items {
            nested.push(self.build_tree_item(tree, child)?);
        }
        self.add_children(container, &nested);
        self.add_children(item, &[header_row, container]);

        self.world.entity_mut(item).insert(PfTreeItem {
            container,
            arrow,
            expanded,
            has_children,
        });

        // Click: select; toggle expansion when the item has children.
        let this = item;
        self.world.entity_mut(header_row).observe(
            move |_click: On<Pointer<Click>>, mut commands: Commands| {
                commands.queue(move |world: &mut World| {
                    toggle_tree_item(world, tree, this);
                });
            },
        );
        if let Some(name) = &node.x_name {
            self.names.push((name.clone(), item));
            self.world.entity_mut(item).insert(PfName(name.clone()));
        }
        Ok(item)
    }

    /// Build a Menu bar (or ContextMenu content) from MenuItems.
    fn spawn_menu(&mut self, entity: Entity, node: &XamlNode) -> Result<(), PfError> {
        let items: Vec<XamlNode> = node.child_elements().cloned().collect();
        let mut top = Vec::new();
        for item in &items {
            match item.name.as_str() {
                "MenuItem" => top.push(self.build_menu_item(entity, item, 0)?),
                "Separator" => {
                    let sep = self
                        .world
                        .spawn((
                            Node {
                                width: Val::Px(1.0),
                                margin: UiRect::axes(Val::Px(4.0), Val::Px(2.0)),
                                ..Default::default()
                            },
                            BackgroundColor(Color::srgb_u8(0xC0, 0xC0, 0xC0)),
                        ))
                        .id();
                    top.push(sep);
                }
                other => self.warn(format!(
                    "{}: `{other}` inside Menu is not supported yet",
                    item.pos
                )),
            }
        }
        self.add_children(entity, &top);
        Ok(())
    }

    fn build_menu_item(
        &mut self,
        menu_root: Entity,
        node: &XamlNode,
        depth: usize,
    ) -> Result<Entity, PfError> {
        use crate::components::{PfMenuItem, PfMenuPopup};
        use crate::overlay::{PfPlacement, PfPopup, ensure_overlay_root};

        let item = self
            .world
            .spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(8.0),
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)),
                    ..Default::default()
                },
                BackgroundColor(Color::NONE),
                Interaction::default(),
                PfElementKind("MenuItem".to_string()),
            ))
            .id();
        // IsCheckable: a leading check glyph toggled on activation.
        let checkable = matches!(node.attribute("IsCheckable"),
            Some(XamlValue::Str(v)) if v.eq_ignore_ascii_case("true"));
        if checkable {
            let checked = matches!(node.attribute("IsChecked"),
                Some(XamlValue::Str(v)) if v.eq_ignore_ascii_case("true"));
            let glyph = self.spawn_text_child("\u{2022}".to_string());
            {
                let mut n = self.node_mut(glyph);
                n.width = Val::Px(12.0);
                n.display = if checked {
                    Display::Flex
                } else {
                    Display::None
                };
            }
            self.add_children(item, &[glyph]);
            self.world
                .entity_mut(item)
                .insert(crate::components::PfCheckableMenuItem { glyph });
            if checked {
                self.world.entity_mut(item).insert(bevy::ui::Checked);
            }
        }
        if let Some(h) = self.spawn_header(node, false)? {
            self.add_children(item, &[h]);
        }

        let sub_nodes: Vec<XamlNode> = node
            .child_elements()
            .filter(|c| c.name == "MenuItem" || c.name == "Separator")
            .cloned()
            .collect();

        let submenu = if sub_nodes.is_empty() {
            None
        } else {
            // Submenu indicator for nested levels.
            if depth > 0 {
                let mark = self.spawn_text_child(">".to_string());
                self.add_children(item, &[mark]);
            }
            let overlay = ensure_overlay_root(self.world);
            let popup = self
                .world
                .spawn((
                    PfPopup {
                        anchor: item,
                        placement: if depth == 0 {
                            PfPlacement::Bottom
                        } else {
                            PfPlacement::Right
                        },
                        open: false,
                        match_anchor_width: false,
                    },
                    PfMenuPopup { menu_root },
                    crate::components::PfLogicalParent(item),
                    Node {
                        position_type: PositionType::Absolute,
                        display: Display::None,
                        flex_direction: FlexDirection::Column,
                        border: UiRect::all(Val::Px(1.0)),
                        padding: UiRect::all(Val::Px(2.0)),
                        min_width: Val::Px(140.0),
                        ..Default::default()
                    },
                    BackgroundColor(Color::WHITE),
                    BorderColor::all(Color::srgb_u8(0xAD, 0xAD, 0xAD)),
                    bevy::ui::GlobalZIndex(i32::MAX - 900),
                ))
                .id();
            let mut children = Vec::new();
            for sub in &sub_nodes {
                match sub.name.as_str() {
                    "MenuItem" => children.push(self.build_menu_item(menu_root, sub, depth + 1)?),
                    _ => {
                        let sep = self
                            .world
                            .spawn((
                                Node {
                                    height: Val::Px(1.0),
                                    margin: UiRect::axes(Val::Px(2.0), Val::Px(3.0)),
                                    ..Default::default()
                                },
                                BackgroundColor(Color::srgb_u8(0xD0, 0xD0, 0xD0)),
                            ))
                            .id();
                        children.push(sep);
                    }
                }
            }
            self.add_children(popup, &children);
            self.world.entity_mut(overlay).add_children(&[popup]);
            Some(popup)
        };

        // MenuItem Command= (menu items are built outside spawn_element).
        let command = match node.attribute("Command") {
            Some(XamlValue::Str(c)) => Some(c.clone()),
            Some(XamlValue::Extension(ext))
                if matches!(ext.name.as_str(), "Binding" | "x:Bind" | "CompiledBinding") =>
            {
                Some(crate::binding::parse_binding_extension(ext).path)
            }
            _ => None,
        };
        if let Some(name) = command {
            let parameter = match node.attribute("CommandParameter") {
                Some(XamlValue::Str(p)) => {
                    Some(crate::binding::PfCommandParameter::Literal(p.clone()))
                }
                Some(XamlValue::Extension(ext))
                    if matches!(ext.name.as_str(), "Binding" | "x:Bind" | "CompiledBinding") =>
                {
                    Some(crate::binding::PfCommandParameter::Path(
                        crate::binding::parse_binding_extension(ext).path,
                    ))
                }
                _ => None,
            };
            self.world
                .entity_mut(item)
                .insert(crate::components::PfCommand { name, parameter });
            let source = item;
            self.world.entity_mut(item).observe(
                move |_click: On<Pointer<Click>>, mut commands: Commands| {
                    commands.queue(move |world: &mut World| {
                        let Some(cmd) = world.get::<crate::components::PfCommand>(source).cloned()
                        else {
                            return;
                        };
                        crate::binding::invoke_command(
                            world,
                            source,
                            &cmd.name,
                            cmd.parameter.as_ref(),
                        );
                    });
                },
            );
        }

        self.world
            .entity_mut(item)
            .insert(PfMenuItem { menu_root, submenu });
        let this = item;
        self.world.entity_mut(item).observe(
            move |_click: On<Pointer<Click>>, mut commands: Commands| {
                commands.queue(move |world: &mut World| {
                    activate_menu_item(world, this);
                });
            },
        );
        if let Some(name) = &node.x_name {
            self.names.push((name.clone(), item));
            self.world.entity_mut(item).insert(PfName(name.clone()));
        }
        Ok(item)
    }

    /// Attach a right-click ContextMenu to an element: a menu popup opened
    /// on secondary click, light-dismissed via a backdrop.
    fn attach_context_menu(&mut self, entity: Entity, menu_node: &XamlNode) -> Result<(), PfError> {
        use crate::components::PfMenuPopup;
        use crate::overlay::{PfPlacement, PfPopup, ensure_overlay_root, spawn_backdrop};

        let overlay = ensure_overlay_root(self.world);
        let popup = self
            .world
            .spawn((
                PfPopup {
                    anchor: entity,
                    placement: PfPlacement::Bottom,
                    open: false,
                    match_anchor_width: false,
                },
                PfMenuPopup { menu_root: entity },
                crate::components::PfLogicalParent(entity),
                Node {
                    position_type: PositionType::Absolute,
                    display: Display::None,
                    flex_direction: FlexDirection::Column,
                    border: UiRect::all(Val::Px(1.0)),
                    padding: UiRect::all(Val::Px(2.0)),
                    min_width: Val::Px(140.0),
                    ..Default::default()
                },
                BackgroundColor(Color::WHITE),
                BorderColor::all(Color::srgb_u8(0xAD, 0xAD, 0xAD)),
                bevy::ui::GlobalZIndex(i32::MAX - 900),
            ))
            .id();
        let backdrop = spawn_backdrop(self.world, popup);
        self.world
            .entity_mut(overlay)
            .add_children(&[backdrop, popup]);

        let items: Vec<XamlNode> = menu_node.child_elements().cloned().collect();
        let mut children = Vec::new();
        for item in &items {
            if item.name == "MenuItem" {
                children.push(self.build_menu_item(entity, item, 1)?);
            }
        }
        self.add_children(popup, &children);

        // Open on right-click, AT the cursor (position_popups then keeps the
        // menu inside the window).
        let menu_popup = popup;
        self.world.entity_mut(entity).observe(
            move |click: On<Pointer<Click>>,
                  mut popups: Query<&mut PfPopup>,
                  mut commands: Commands| {
                if click.button == bevy::picking::pointer::PointerButton::Secondary
                    && let Ok(mut p) = popups.get_mut(menu_popup)
                {
                    p.open = true;
                    commands
                        .entity(menu_popup)
                        .insert(crate::overlay::PfPointerAnchor(
                            click.pointer_location.position,
                        ));
                }
            },
        );
        let owner = entity;
        self.world.entity_mut(backdrop).observe(
            move |_click: On<Pointer<Click>>, mut commands: Commands| {
                commands.queue(move |world: &mut World| {
                    close_menu_popups(world, owner);
                });
            },
        );
        Ok(())
    }

    /// Build a DataGrid: header row + a rows host filled by ItemsSource.
    fn spawn_data_grid(&mut self, entity: Entity, node: &XamlNode) -> Result<(), PfError> {
        use crate::components::{PfDataGrid, PfGridColumn};

        // Parse <DataGrid.Columns>.
        let mut columns: Vec<PfGridColumn> = Vec::new();
        if let Some(cols) = node.property_element("Columns") {
            for col in cols.elements() {
                if col.name != "DataGridTextColumn" {
                    self.warn(format!(
                        "{}: `{}` is not supported yet (text columns only)",
                        col.pos, col.name
                    ));
                    continue;
                }
                let header = match col.attribute("Header") {
                    Some(XamlValue::Str(s)) => s.clone(),
                    _ => String::new(),
                };
                let path = match col.attribute("Binding") {
                    Some(XamlValue::Extension(ext)) if ext.name == "Binding" => {
                        crate::binding::parse_binding_extension(ext).path
                    }
                    _ => {
                        self.warn(format!(
                            "{}: DataGridTextColumn needs Binding=\"{{Binding path}}\"",
                            col.pos
                        ));
                        continue;
                    }
                };
                let width = match col.attribute("Width") {
                    Some(XamlValue::Str(s)) => s
                        .parse::<v::GridLength>()
                        .unwrap_or(v::GridLength::Star(1.0)),
                    _ => v::GridLength::Star(1.0),
                };
                columns.push(PfGridColumn {
                    header,
                    path,
                    width,
                    template: None,
                });
            }
        }
        if columns.is_empty() {
            self.warn(format!(
                "{}: DataGrid needs <DataGrid.Columns> with DataGridTextColumn entries",
                node.pos
            ));
        }

        let template: Vec<RepeatedGridTrack> = columns
            .iter()
            .map(|c| match c.width {
                // Auto can't align across independent row grids; treat as star.
                v::GridLength::Auto => GridTrack::fr(1.0),
                other => convert::grid_track(other),
            })
            .collect();

        // Header row.
        let header_row = self
            .world
            .spawn((
                Node {
                    display: Display::Grid,
                    grid_template_columns: template.clone(),
                    grid_template_rows: vec![GridTrack::auto()],
                    border: UiRect {
                        bottom: Val::Px(1.0),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                BackgroundColor(Color::srgb_u8(0xF0, 0xF0, 0xF0)),
                BorderColor::all(Color::srgb_u8(0xAD, 0xAD, 0xAD)),
                PfElementKind("DataGrid.Header".to_string()),
            ))
            .id();
        let saved_weight = self.inherited.font_weight;
        self.inherited.font_weight = v::FontWeight::SEMI_BOLD;
        for (i, col) in columns.iter().enumerate() {
            let cell = self.spawn_text_child(col.header.clone());
            {
                let mut n = self.node_mut(cell);
                n.grid_column = GridPlacement::start_span(i as i16 + 1, 1);
                n.grid_row = GridPlacement::start_span(1, 1);
                n.padding = UiRect::axes(Val::Px(8.0), Val::Px(4.0));
            }
            self.add_children(header_row, &[cell]);
        }
        self.inherited.font_weight = saved_weight;

        // Rows host (selection managed like a ListBox).
        let rows_host = self
            .world
            .spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    ..Default::default()
                },
                PfListBox::default(),
                PfElementKind("DataGrid.Rows".to_string()),
            ))
            .id();
        self.add_children(entity, &[header_row, rows_host]);
        self.world
            .entity_mut(entity)
            .insert(PfDataGrid { columns, rows_host });
        Ok(())
    }

    /// Shared tail for DataGrid / column-mode ListView: header row + rows
    /// host, with per-column grid tracks.
    fn build_column_grid(&mut self, entity: Entity, columns: Vec<crate::components::PfGridColumn>) {
        use crate::components::PfDataGrid;

        let template: Vec<RepeatedGridTrack> = columns
            .iter()
            .map(|c| match c.width {
                v::GridLength::Auto => GridTrack::fr(1.0),
                other => convert::grid_track(other),
            })
            .collect();

        let header_row = self
            .world
            .spawn((
                Node {
                    display: Display::Grid,
                    grid_template_columns: template,
                    grid_template_rows: vec![GridTrack::auto()],
                    border: UiRect {
                        bottom: Val::Px(1.0),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                BackgroundColor(Color::srgb_u8(0xF0, 0xF0, 0xF0)),
                BorderColor::all(Color::srgb_u8(0xAD, 0xAD, 0xAD)),
                PfElementKind("GridView.Header".to_string()),
            ))
            .id();
        let saved_weight = self.inherited.font_weight;
        self.inherited.font_weight = v::FontWeight::SEMI_BOLD;
        for (i, col) in columns.iter().enumerate() {
            let cell = self.spawn_text_child(col.header.clone());
            {
                let mut n = self.node_mut(cell);
                n.grid_column = GridPlacement::start_span(i as i16 + 1, 1);
                n.grid_row = GridPlacement::start_span(1, 1);
                n.padding = UiRect::axes(Val::Px(8.0), Val::Px(4.0));
            }
            self.add_children(header_row, &[cell]);
        }
        self.inherited.font_weight = saved_weight;

        let rows_host = self
            .world
            .spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    ..Default::default()
                },
                PfListBox::default(),
                PfElementKind("GridView.Rows".to_string()),
            ))
            .id();
        self.add_children(entity, &[header_row, rows_host]);
        self.world
            .entity_mut(entity)
            .insert(PfDataGrid { columns, rows_host });
    }

    /// WPF `ListView` + `GridView`: details view with `GridViewColumn`s
    /// (`DisplayMemberBinding` text cells or `CellTemplate` templates).
    fn spawn_list_view(&mut self, entity: Entity, grid_view: &XamlNode) -> Result<(), PfError> {
        use crate::components::PfGridColumn;

        let mut columns: Vec<PfGridColumn> = Vec::new();
        for col in grid_view.children.iter().filter_map(|c| c.as_element()) {
            if col.name != "GridViewColumn" {
                self.warn(format!(
                    "{}: `{}` inside GridView is not supported yet",
                    col.pos, col.name
                ));
                continue;
            }
            let header = match col.attribute("Header") {
                Some(XamlValue::Str(s)) => s.clone(),
                _ => String::new(),
            };
            let path = match col.attribute("DisplayMemberBinding") {
                Some(XamlValue::Extension(ext)) if ext.name == "Binding" => {
                    crate::binding::parse_binding_extension(ext).path
                }
                _ => String::new(),
            };
            let template = col
                .property_element("CellTemplate")
                .and_then(|pe| pe.elements().find(|e| e.name == "DataTemplate"))
                .map(|dt| std::sync::Arc::new(dt.clone()));
            if path.is_empty() && template.is_none() {
                self.warn(format!(
                    "{}: GridViewColumn needs DisplayMemberBinding or CellTemplate",
                    col.pos
                ));
                continue;
            }
            let width = match col.attribute("Width") {
                Some(XamlValue::Str(s)) => s
                    .parse::<v::GridLength>()
                    .unwrap_or(v::GridLength::Star(1.0)),
                _ => v::GridLength::Star(1.0),
            };
            columns.push(PfGridColumn {
                header,
                path,
                width,
                template,
            });
        }
        if columns.is_empty() {
            self.warn("ListView GridView needs GridViewColumn entries".to_string());
        }
        self.build_column_grid(entity, columns);
        Ok(())
    }

    /// WPF `Hyperlink`: accent-colored text that opens `NavigateUri`.
    fn spawn_hyperlink(&mut self, entity: Entity, node: &XamlNode) -> Result<(), PfError> {
        use crate::components::PfHyperlink;

        let mut text = String::new();
        for child in &node.children {
            if let Some(t) = child.as_text() {
                text.push_str(t);
            }
        }
        if text.is_empty()
            && let Some(XamlValue::Str(s)) = node.attribute("Content")
        {
            text = s.clone();
        }
        let uri = match node.attribute("NavigateUri") {
            Some(XamlValue::Str(s)) => s.clone(),
            _ => String::new(),
        };

        let saved = self.inherited.foreground;
        self.inherited.foreground = v::PfColor::rgb(0x00, 0x66, 0xCC);
        let label = self.spawn_text_child(text);
        self.inherited.foreground = saved;
        self.add_children(entity, &[label]);

        self.world
            .entity_mut(entity)
            .insert((PfHyperlink(uri), Interaction::default()));
        self.world.entity_mut(entity).observe(
            move |click: On<Pointer<Click>>, links: Query<&PfHyperlink>, mut commands: Commands| {
                let link_entity = click.entity;
                if let Ok(link) = links.get(link_entity)
                    && !link.0.is_empty()
                {
                    let uri = link.0.clone();
                    commands.queue(move |world: &mut World| {
                        crate::navigation::follow_hyperlink(world, link_entity, &uri);
                    });
                }
            },
        );
        Ok(())
    }

    /// WPF `Frame`: optional back/forward chrome over a content host that
    /// pages instantiate into. `Source=` is resolved by a plugin system once
    /// the page registry is populated.
    fn spawn_frame(&mut self, entity: Entity, node: &XamlNode) {
        use crate::components::{PfFrame, PfFrameChrome};

        let source = match node.attribute("Source") {
            Some(XamlValue::Str(s)) => Some(s.clone()),
            _ => None,
        };
        let show_chrome = !matches!(
            node.attribute("NavigationUIVisibility"),
            Some(XamlValue::Str(s)) if s.eq_ignore_ascii_case("hidden")
        );

        let chrome = if show_chrome {
            let mut make_button = |glyph: &str| {
                let label = self
                    .world
                    .spawn((
                        Node::default(),
                        bevy::ui::widget::Text::new(glyph),
                        bevy::text::TextFont {
                            font_size: bevy::text::FontSize::Px(13.0),
                            font: crate::fonts::default_font(),
                            ..Default::default()
                        },
                        bevy::text::TextColor(Color::srgb_u8(0xAF, 0xAF, 0xAF)),
                    ))
                    .id();
                let button = self
                    .world
                    .spawn((
                        Node {
                            width: Val::Px(30.0),
                            height: Val::Px(24.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            margin: UiRect::right(Val::Px(4.0)),
                            ..Default::default()
                        },
                        BackgroundColor(Color::srgb_u8(0xF4, 0xF4, 0xF4)),
                        Interaction::default(),
                    ))
                    .id();
                self.world.entity_mut(button).add_children(&[label]);
                button
            };
            let back_button = make_button("<");
            let forward_button = make_button(">");
            let frame = entity;
            self.world.entity_mut(back_button).observe(
                move |_: On<Pointer<Click>>, mut commands: Commands| {
                    commands.queue(move |world: &mut World| {
                        crate::navigation::go_back(world, frame);
                    });
                },
            );
            self.world.entity_mut(forward_button).observe(
                move |_: On<Pointer<Click>>, mut commands: Commands| {
                    commands.queue(move |world: &mut World| {
                        crate::navigation::go_forward(world, frame);
                    });
                },
            );
            let bar = self
                .world
                .spawn((
                    Node {
                        display: Display::Flex,
                        flex_direction: FlexDirection::Row,
                        padding: UiRect::all(Val::Px(4.0)),
                        ..Default::default()
                    },
                    BackgroundColor(Color::NONE),
                ))
                .id();
            self.world
                .entity_mut(bar)
                .add_children(&[back_button, forward_button]);
            self.add_children(entity, &[bar]);
            Some(PfFrameChrome {
                back_button,
                forward_button,
            })
        } else {
            None
        };

        let content = self
            .world
            .spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0,
                    ..Default::default()
                },
                BackgroundColor(Color::NONE),
            ))
            .id();
        self.add_children(entity, &[content]);

        self.world.entity_mut(entity).insert(PfFrame {
            content,
            chrome,
            back: Vec::new(),
            forward: Vec::new(),
            current: None,
            current_title: None,
            pending_source: source,
        });
    }

    /// Toolkit presets that ride on Border's box model: `Card` and `Chip`.
    fn apply_toolkit_presets(&mut self, entity: Entity, kind: ElemKind, node: &XamlNode) {
        if kind != ElemKind::Border {
            return;
        }
        match node.name.as_str() {
            "Card" => {
                self.world.entity_mut(entity).insert((
                    BackgroundColor(Color::WHITE),
                    bevy::ui::BoxShadow(vec![bevy::ui::ShadowStyle {
                        color: Color::srgba(0.0, 0.0, 0.0, 0.18),
                        x_offset: Val::Px(0.0),
                        y_offset: Val::Px(2.0),
                        spread_radius: Val::Px(0.0),
                        blur_radius: Val::Px(8.0),
                    }]),
                ));
                if let Some(mut n) = self.world.get_mut::<Node>(entity) {
                    n.border_radius = bevy::ui::BorderRadius::all(Val::Px(8.0));
                    if n.padding == UiRect::DEFAULT {
                        n.padding = UiRect::all(Val::Px(14.0));
                    }
                }
            }
            "Chip" | "Tag" => {
                self.world
                    .entity_mut(entity)
                    .insert(BackgroundColor(Color::srgb_u8(0xE8, 0xEC, 0xF4)));
                if let Some(mut n) = self.world.get_mut::<Node>(entity) {
                    n.border_radius = bevy::ui::BorderRadius::all(Val::Px(999.0));
                    n.padding = UiRect::axes(Val::Px(10.0), Val::Px(3.0));
                    n.align_items = AlignItems::Center;
                }
            }
            _ => {}
        }
    }

    /// Toolkit `ToggleSwitch`: pill track, sliding thumb, latching Checked.
    fn spawn_toggle_switch(&mut self, entity: Entity, node: &XamlNode) {
        use crate::components::PfToggleSwitch;
        use bevy::ui::Checked;
        let on = matches!(
            node.attribute("IsOn").or_else(|| node.attribute("IsChecked")),
            Some(XamlValue::Str(s)) if s.eq_ignore_ascii_case("true")
        );
        let thumb = self
            .world
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(16.0),
                    height: Val::Px(16.0),
                    top: Val::Px(2.0),
                    left: Val::Px(if on { 22.0 } else { 2.0 }),
                    border_radius: bevy::ui::BorderRadius::all(Val::Px(999.0)),
                    ..Default::default()
                },
                BackgroundColor(Color::WHITE),
            ))
            .id();
        let theme = self.control_theme();
        let track = self
            .world
            .spawn((
                Node {
                    width: Val::Px(40.0),
                    height: Val::Px(20.0),
                    border_radius: bevy::ui::BorderRadius::all(Val::Px(999.0)),
                    ..Default::default()
                },
                BackgroundColor(if on { theme.accent } else { theme.track }),
            ))
            .id();
        self.world.entity_mut(track).add_children(&[thumb]);
        self.add_children(entity, &[track]);
        if let Some(text) = node.attribute("Content").and_then(|v| match v {
            XamlValue::Str(s) => Some(s.clone()),
            _ => None,
        }) {
            let label = self.spawn_text_child(text);
            self.add_children(entity, &[label]);
        }
        let mut e = self.world.entity_mut(entity);
        e.insert((PfToggleSwitch { track, thumb }, Interaction::default()));
        if on {
            e.insert(Checked);
        }
        self.world.entity_mut(entity).observe(
            |click: On<Pointer<Click>>, mut commands: Commands, checked: Query<Has<Checked>>| {
                let e = click.entity;
                if let Ok(is_on) = checked.get(e) {
                    if is_on {
                        commands.entity(e).remove::<Checked>();
                    } else {
                        commands.entity(e).insert(Checked);
                    }
                }
            },
        );
    }

    /// Toolkit `NumericUpDown`: [-] readout [+] with Min/Max/Increment.
    fn spawn_numeric_up_down(&mut self, entity: Entity, node: &XamlNode) {
        use crate::components::PfNumericUpDown;
        let get = |name: &str, default: f64| -> f64 {
            match node.attribute(name) {
                Some(XamlValue::Str(s)) => s.trim().parse().unwrap_or(default),
                _ => default,
            }
        };
        let value = get("Value", 0.0);
        let (minimum, maximum, increment) = (
            get("Minimum", f64::MIN),
            get("Maximum", f64::MAX),
            get("Increment", 1.0),
        );
        fn spinner_button(ctx: &mut Ctx, glyph: &str) -> Entity {
            let this = ctx;
            let label = this.spawn_text_child(glyph.to_string());
            let b = this
                .world
                .spawn((
                    Node {
                        width: Val::Px(22.0),
                        height: Val::Px(22.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..Default::default()
                    },
                    BackgroundColor(Color::srgb_u8(0xE6, 0xE6, 0xE6)),
                    Interaction::default(),
                ))
                .id();
            this.world.entity_mut(b).add_children(&[label]);
            b
        }
        let minus = spinner_button(self, "-");
        let text = self.spawn_text_child(format!("{value}"));
        let plus = spinner_button(self, "+");
        self.add_children(entity, &[minus, text, plus]);
        self.world.entity_mut(entity).insert(PfNumericUpDown {
            value,
            minimum,
            maximum,
            increment,
            text,
        });
        let owner = entity;
        for (button, dir) in [(minus, -1.0f64), (plus, 1.0f64)] {
            self.world.entity_mut(button).observe(
                move |_: On<Pointer<Click>>, mut nums: Query<&mut PfNumericUpDown>| {
                    if let Ok(mut n) = nums.get_mut(owner) {
                        n.value = (n.value + dir * n.increment).clamp(n.minimum, n.maximum);
                    }
                },
            );
        }
    }

    /// Toolkit `RatingBar`: clickable pips.
    fn spawn_rating_bar(&mut self, entity: Entity, node: &XamlNode) {
        use crate::components::PfRatingBar;
        let get_u32 = |name: &str, default: u32| match node.attribute(name) {
            Some(XamlValue::Str(s)) => s.trim().parse().unwrap_or(default),
            _ => default,
        };
        let maximum = get_u32("Maximum", 5).max(1);
        let value = get_u32("Value", 0).min(maximum);
        let mut pips = Vec::new();
        for i in 0..maximum {
            let filled = i < value;
            let pip = self
                .world
                .spawn((
                    Node {
                        width: Val::Px(16.0),
                        height: Val::Px(16.0),
                        border_radius: bevy::ui::BorderRadius::all(Val::Px(999.0)),
                        ..Default::default()
                    },
                    BackgroundColor(if filled {
                        Color::srgb_u8(0xF2, 0xB0, 0x24)
                    } else {
                        Color::srgb_u8(0xD6, 0xD6, 0xD6)
                    }),
                    Interaction::default(),
                ))
                .id();
            let owner = entity;
            self.world.entity_mut(pip).observe(
                move |_: On<Pointer<Click>>, mut bars: Query<&mut PfRatingBar>| {
                    if let Ok(mut bar) = bars.get_mut(owner) {
                        bar.value = i + 1;
                    }
                },
            );
            pips.push(pip);
        }
        self.add_children(entity, &pips);
        self.world.entity_mut(entity).insert(PfRatingBar {
            value,
            maximum,
            pips,
        });
    }

    /// Toolkit `Badge`/`Badged`: content with a count bubble top-right.
    fn spawn_badge(&mut self, entity: Entity, node: &XamlNode) -> Result<(), PfError> {
        let children = self.spawn_child_elements(node, ParentKind::Grid)?;
        self.add_children(entity, &children);
        let badge_text = match node.attribute("Badge") {
            Some(XamlValue::Str(s)) => s.clone(),
            _ => String::new(),
        };
        if !badge_text.is_empty() {
            let label = self.spawn_text_child(badge_text);
            if let Some(mut t) = self.world.get_mut::<bevy::text::TextFont>(label) {
                t.font_size = bevy::text::FontSize::Px(10.0);
            }
            self.world
                .entity_mut(label)
                .insert(bevy::text::TextColor(Color::WHITE));
            let bubble = self
                .world
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(-8.0),
                        right: Val::Px(-8.0),
                        min_width: Val::Px(16.0),
                        height: Val::Px(16.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        padding: UiRect::axes(Val::Px(4.0), Val::Px(0.0)),
                        border_radius: bevy::ui::BorderRadius::all(Val::Px(999.0)),
                        ..Default::default()
                    },
                    BackgroundColor(Color::srgb_u8(0xE0, 0x3E, 0x3E)),
                ))
                .id();
            self.world.entity_mut(bubble).add_children(&[label]);
            self.add_children(entity, &[bubble]);
        }
        Ok(())
    }

    /// Toolkit `BusyIndicator`: content + dimming overlay while IsBusy.
    fn spawn_busy_indicator(&mut self, entity: Entity, node: &XamlNode) -> Result<(), PfError> {
        use crate::components::PfBusyIndicator;
        let children = self.spawn_child_elements(node, ParentKind::Grid)?;
        self.add_children(entity, &children);
        let busy = matches!(
            node.attribute("IsBusy"),
            Some(XamlValue::Str(s)) if s.eq_ignore_ascii_case("true")
        );
        let busy_text = match node.attribute("BusyContent") {
            Some(XamlValue::Str(s)) => s.clone(),
            _ => "Working...".to_string(),
        };
        let label = self.spawn_text_child(busy_text);
        let overlay = self
            .world
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    display: if busy { Display::Flex } else { Display::None },
                    ..Default::default()
                },
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.75)),
            ))
            .id();
        self.world.entity_mut(overlay).add_children(&[label]);
        self.add_children(entity, &[overlay]);
        self.world
            .entity_mut(entity)
            .insert(PfBusyIndicator { overlay, busy });
        Ok(())
    }

    /// Toolkit `RangeSlider`: track, highlighted interval, two drag thumbs.
    fn spawn_range_slider(&mut self, entity: Entity, node: &XamlNode) {
        use crate::components::PfRangeSlider;
        let get = |name: &str, default: f32| -> f32 {
            match node.attribute(name) {
                Some(XamlValue::Str(s)) => s.trim().parse().unwrap_or(default),
                _ => default,
            }
        };
        let minimum = get("Minimum", 0.0);
        let maximum = get("Maximum", 10.0).max(minimum);
        // MinRange: the smallest gap the two thumbs may be dragged to, capped at
        // the full span so a too-large value can never invert the interval.
        let min_range = get("MinRange", 0.0).clamp(0.0, maximum - minimum);
        let lower = get("LowerValue", minimum).clamp(minimum, maximum);
        let upper = get("UpperValue", maximum).clamp(lower, maximum);
        let span = (maximum - minimum).max(f32::EPSILON);
        const THUMB: f32 = 16.0;
        let track = self
            .world
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    right: Val::Px(0.0),
                    top: Val::Px(8.0),
                    height: Val::Px(4.0),
                    border_radius: BorderRadius::all(Val::Px(2.0)),
                    ..Default::default()
                },
                BackgroundColor(Color::srgb_u8(0xC4, 0xC4, 0xC4)),
            ))
            .id();
        let fill = self
            .world
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent((lower - minimum) / span * 100.0),
                    right: Val::Percent((maximum - upper) / span * 100.0),
                    top: Val::Px(8.0),
                    height: Val::Px(4.0),
                    ..Default::default()
                },
                BackgroundColor(ACCENT),
            ))
            .id();
        fn thumb_at(ctx: &mut Ctx, fraction: f32) -> Entity {
            ctx.world
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(fraction * 100.0),
                        top: Val::Px(2.0),
                        width: Val::Px(THUMB),
                        height: Val::Px(THUMB),
                        margin: UiRect::left(Val::Px(-THUMB / 2.0)),
                        border_radius: BorderRadius::MAX,
                        ..Default::default()
                    },
                    BackgroundColor(ACCENT),
                    Interaction::default(),
                ))
                .id()
        }
        let thumb_lower = thumb_at(self, (lower - minimum) / span);
        let thumb_upper = thumb_at(self, (upper - minimum) / span);
        self.add_children(entity, &[track, fill, thumb_lower, thumb_upper]);
        self.world.entity_mut(entity).insert(PfRangeSlider {
            lower,
            upper,
            minimum,
            maximum,
            min_range,
            thumb_lower,
            thumb_upper,
            fill,
        });
        // Dragging a thumb maps pointer x within the slider to a value.
        let owner = entity;
        for (thumb, is_lower) in [(thumb_lower, true), (thumb_upper, false)] {
            self.world.entity_mut(thumb).observe(
                move |drag: On<Pointer<Drag>>,
                      mut sliders: Query<&mut PfRangeSlider>,
                      computed: Query<(&bevy::ui::ComputedNode, &bevy::ui::UiGlobalTransform)>| {
                    let Ok(mut rs) = sliders.get_mut(owner) else { return };
                    let Ok((node, transform)) = computed.get(owner) else { return };
                    let width = node.size().x * node.inverse_scale_factor();
                    if width <= 0.0 {
                        return;
                    }
                    let origin_x = transform.translation.x * node.inverse_scale_factor()
                        - width / 2.0;
                    let frac = ((drag.pointer_location.position.x - origin_x) / width)
                        .clamp(0.0, 1.0);
                    let value = rs.minimum + frac * (rs.maximum - rs.minimum);
                    // Keep the two thumbs at least `min_range` apart so the
                    // interval never collapses (WPF MinRange semantics).
                    let gap = rs.min_range.clamp(0.0, rs.maximum - rs.minimum);
                    if is_lower {
                        rs.lower = value.min(rs.upper - gap).max(rs.minimum);
                    } else {
                        rs.upper = value.max(rs.lower + gap).min(rs.maximum);
                    }
                },
            );
        }
    }

    /// Toolkit `TimePicker`: a display box with an hour/minute dropdown
    /// (MaterialDesignInXaml / HandyControl equivalents).
    fn spawn_time_picker(&mut self, entity: Entity, node: &XamlNode) -> Result<(), PfError> {
        use crate::components::PfTimePicker;
        use crate::overlay::{PfPlacement, PfPopup, ensure_overlay_root, spawn_backdrop};

        // SelectedTime="HH:MM".
        let mut hour = None;
        let mut minute = None;
        if let Some(XamlValue::Str(t)) = node.attribute("SelectedTime") {
            let mut it = t.split(':');
            hour = it
                .next()
                .and_then(|h| h.trim().parse::<u32>().ok())
                .filter(|h| *h < 24);
            minute = it
                .next()
                .and_then(|m| m.trim().parse::<u32>().ok())
                .filter(|m| *m < 60);
        }

        let display = self.spawn_text_child("Select time".to_string());
        {
            let mut n = self.node_mut(display);
            n.flex_grow = 1.0;
        }
        let arrow = self.spawn_text_child("\u{25BE}".to_string());
        self.add_children(entity, &[display, arrow]);

        let overlay = ensure_overlay_root(self.world);
        let popup = self
            .world
            .spawn((
                PfPopup {
                    anchor: entity,
                    placement: PfPlacement::Bottom,
                    open: false,
                    match_anchor_width: false,
                },
                crate::components::PfLogicalParent(entity),
                Node {
                    position_type: PositionType::Absolute,
                    display: Display::None,
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(4.0),
                    padding: UiRect::all(Val::Px(6.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..Default::default()
                },
                BackgroundColor(Color::WHITE),
                BorderColor::all(Color::srgb_u8(0xAD, 0xAD, 0xAD)),
                bevy::ui::GlobalZIndex(i32::MAX - 900),
            ))
            .id();
        let backdrop = spawn_backdrop(self.world, popup);
        self.world
            .entity_mut(overlay)
            .add_children(&[backdrop, popup]);

        let hours: Vec<u32> = (0..24).collect();
        let minutes: Vec<u32> = (0..60).step_by(5).collect();
        let hour_col = self.spawn_time_column(entity, &hours, true);
        let minute_col = self.spawn_time_column(entity, &minutes, false);
        self.add_children(popup, &[hour_col, minute_col]);

        self.world.entity_mut(entity).insert((
            PfTimePicker {
                hour,
                minute,
                display,
                popup,
            },
            Interaction::default(),
        ));
        if hour.is_some() || minute.is_some() {
            time_picker_set(self.world, entity, hour, minute);
        }

        let popup_entity = popup;
        self.world.entity_mut(entity).observe(
            move |_click: On<Pointer<Click>>, mut popups: Query<&mut PfPopup>| {
                if let Ok(mut p) = popups.get_mut(popup_entity) {
                    p.open = !p.open;
                }
            },
        );
        self.world.entity_mut(backdrop).observe(
            move |_click: On<Pointer<Click>>, mut popups: Query<&mut PfPopup>| {
                if let Ok(mut p) = popups.get_mut(popup_entity) {
                    p.open = false;
                }
            },
        );
        Ok(())
    }

    /// One scrollable column of clickable time values.
    fn spawn_time_column(&mut self, picker: Entity, values: &[u32], is_hour: bool) -> Entity {
        let column = self
            .world
            .spawn(Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                max_height: Val::Px(190.0),
                overflow: bevy::ui::Overflow::scroll_y(),
                ..Default::default()
            })
            .id();
        for &value in values {
            let label = self.spawn_text_child(format!("{value:02}"));
            let cell = self
                .world
                .spawn((
                    Node {
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(2.0)),
                        justify_content: JustifyContent::Center,
                        ..Default::default()
                    },
                    BackgroundColor(Color::NONE),
                    Interaction::default(),
                ))
                .id();
            self.add_children(cell, &[label]);
            self.world.entity_mut(cell).observe(
                move |_click: On<Pointer<Click>>, mut commands: Commands| {
                    commands.queue(move |world: &mut World| {
                        if is_hour {
                            time_picker_set(world, picker, Some(value), None);
                        } else {
                            time_picker_set(world, picker, None, Some(value));
                        }
                    });
                },
            );
            self.add_children(column, &[cell]);
        }
        column
    }

    /// Toolkit `PackIcon` / `SymbolIcon` / `FontIcon`: vector icons rendered
    /// by the shape engine (see [`crate::icons`]) — no icon font, identical
    /// output on native and wasm.
    fn spawn_pack_icon(&mut self, entity: Entity, node: &XamlNode) {
        // FontIcon: a literal glyph from the current font.
        if let Some(XamlValue::Str(glyph)) = node.attribute("Glyph") {
            let glyph = glyph.clone();
            let label = self.spawn_text_child(glyph);
            self.add_children(entity, &[label]);
            return;
        }
        let kind = match node.attribute("Kind").or_else(|| node.attribute("Symbol")) {
            Some(XamlValue::Str(k)) => k.clone(),
            _ => {
                self.warn(format!(
                    "{}: {} needs Kind= or Symbol=",
                    node.pos, node.name
                ));
                return;
            }
        };
        let Some(data) = crate::icons::icon_path(&kind) else {
            self.warn(format!("{}: unknown icon kind '{kind}'", node.pos));
            let label = self.spawn_text_child("?".to_string());
            self.add_children(entity, &[label]);
            return;
        };
        let geometry =
            bevy_pf_xaml::geometry::parse_path_data(data).expect("icon path data is pre-validated");
        let shape = crate::shapes::build_shape(
            "Path",
            crate::shapes::ShapeParams {
                data: Some(geometry),
                fill: Some(v::PfBrush::Solid(self.inherited.foreground)),
                stretch: Some(v::Stretch::Uniform),
                ..Default::default()
            },
        )
        .expect("icon shape");
        let inner = self
            .world
            .spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..Default::default()
                },
                shape,
                crate::shapes::PfShapeRendered::default(),
            ))
            .id();
        self.add_children(entity, &[inner]);
    }

    /// Toolkit `ColorPicker`: a swatch button opening a palette grid plus a
    /// hex entry. (HSV canvas: deferred; the palette + hex path covers the
    /// ecosystem's common usage.)
    fn spawn_color_picker(&mut self, entity: Entity, node: &XamlNode) {
        use crate::components::{PfColorHexInput, PfColorPicker};
        use crate::overlay::{PfPlacement, PfPopup, ensure_overlay_root, spawn_backdrop};

        let selected = match node.attribute("SelectedColor") {
            Some(XamlValue::Str(t)) => t
                .parse::<v::PfColor>()
                .map(convert::color)
                .unwrap_or(Color::srgb_u8(0x33, 0x66, 0xCC)),
            _ => Color::srgb_u8(0x33, 0x66, 0xCC),
        };

        let swatch = self
            .world
            .spawn((
                Node {
                    width: Val::Px(18.0),
                    height: Val::Px(18.0),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
                BackgroundColor(selected),
            ))
            .id();
        let arrow = self.spawn_text_child("\u{25BE}".to_string());
        self.add_children(entity, &[swatch, arrow]);

        let overlay = ensure_overlay_root(self.world);
        let popup = self
            .world
            .spawn((
                PfPopup {
                    anchor: entity,
                    placement: PfPlacement::Bottom,
                    open: false,
                    match_anchor_width: false,
                },
                crate::components::PfLogicalParent(entity),
                Node {
                    position_type: PositionType::Absolute,
                    display: Display::None,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    padding: UiRect::all(Val::Px(8.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..Default::default()
                },
                BackgroundColor(Color::WHITE),
                BorderColor::all(Color::srgb_u8(0xAD, 0xAD, 0xAD)),
                bevy::ui::GlobalZIndex(i32::MAX - 900),
            ))
            .id();
        let backdrop = spawn_backdrop(self.world, popup);
        self.world
            .entity_mut(overlay)
            .add_children(&[backdrop, popup]);

        // 20-color palette (5x4): a practical designer set.
        const PALETTE: [(u8, u8, u8); 20] = [
            (0x00, 0x00, 0x00),
            (0x44, 0x44, 0x44),
            (0x88, 0x88, 0x88),
            (0xCC, 0xCC, 0xCC),
            (0xFF, 0xFF, 0xFF),
            (0xE8, 0x1B, 0x23),
            (0xFF, 0x8C, 0x00),
            (0xFF, 0xD4, 0x00),
            (0x33, 0x99, 0x33),
            (0x00, 0x7F, 0x5F),
            (0x00, 0xB7, 0xC3),
            (0x33, 0x66, 0xCC),
            (0x00, 0x3D, 0x99),
            (0x68, 0x21, 0x7A),
            (0xC2, 0x39, 0x8A),
            (0x8B, 0x45, 0x13),
            (0xF4, 0xA0, 0x8C),
            (0xB5, 0xE6, 0x1D),
            (0x9B, 0xB7, 0xD4),
            (0x1A, 0x1A, 0x2E),
        ];
        let grid = self
            .world
            .spawn(Node {
                display: Display::Grid,
                grid_template_columns: vec![RepeatedGridTrack::px(5, 22.0)],
                row_gap: Val::Px(4.0),
                column_gap: Val::Px(4.0),
                ..Default::default()
            })
            .id();
        for (r, g, b) in PALETTE {
            let cell = self
                .world
                .spawn((
                    Node {
                        width: Val::Px(22.0),
                        height: Val::Px(22.0),
                        border_radius: BorderRadius::all(Val::Px(3.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..Default::default()
                    },
                    BackgroundColor(Color::srgb_u8(r, g, b)),
                    BorderColor::all(Color::srgb_u8(0xD0, 0xD0, 0xD0)),
                    Interaction::default(),
                ))
                .id();
            let picker = entity;
            self.world.entity_mut(cell).observe(
                move |_click: On<Pointer<Click>>, mut commands: Commands| {
                    commands.queue(move |world: &mut World| {
                        color_picker_set(world, picker, Color::srgb_u8(r, g, b), true);
                    });
                },
            );
            self.add_children(grid, &[cell]);
        }

        // Hex entry row.
        let hex = color_to_hex(selected);
        let hex_input = self
            .world
            .spawn((
                Node {
                    flex_grow: 1.0,
                    ..Default::default()
                },
                bevy::text::EditableText::new(&hex),
                bevy::text::TextColor(convert::color(self.inherited.foreground)),
                PfColorHexInput { owner: entity },
            ))
            .id();
        self.apply_text_font(hex_input);
        let hex_row = self
            .world
            .spawn((
                Node {
                    display: Display::Flex,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    min_height: Val::Px(22.0),
                    ..Default::default()
                },
                BackgroundColor(Color::WHITE),
                BorderColor::all(Color::srgb_u8(0xAD, 0xAD, 0xAD)),
            ))
            .id();
        self.add_children(hex_row, &[hex_input]);
        self.add_children(popup, &[grid, hex_row]);

        self.world.entity_mut(entity).insert((
            PfColorPicker {
                selected,
                swatch,
                hex_input,
                popup,
            },
            Interaction::default(),
        ));

        let popup_entity = popup;
        self.world.entity_mut(entity).observe(
            move |_click: On<Pointer<Click>>, mut popups: Query<&mut PfPopup>| {
                if let Ok(mut p) = popups.get_mut(popup_entity) {
                    p.open = !p.open;
                }
            },
        );
        self.world.entity_mut(backdrop).observe(
            move |_click: On<Pointer<Click>>, mut popups: Query<&mut PfPopup>| {
                if let Ok(mut p) = popups.get_mut(popup_entity) {
                    p.open = false;
                }
            },
        );
    }

    /// WinUI `AutoSuggestBox`: an editable text box with a prefix-filtered
    /// suggestion dropdown. Suggestions come from a comma-separated
    /// `Suggestions=` attribute and/or inline string children.
    fn spawn_auto_suggest_box(&mut self, entity: Entity, node: &XamlNode) -> Result<(), PfError> {
        use crate::components::{PfAutoSuggestBox, PfAutoSuggestInput};
        use crate::overlay::{PfPlacement, PfPopup, ensure_overlay_root, spawn_backdrop};

        let text = match node.attribute("Text") {
            Some(value) => {
                let value = value.clone();
                self.resolve_text_attr(&value).unwrap_or_default()
            }
            None => String::new(),
        };
        let mut editable = bevy::text::EditableText::new(&text);
        editable.allow_newlines = false;
        let input = self
            .world
            .spawn((
                Node {
                    flex_grow: 1.0,
                    ..Default::default()
                },
                editable,
                bevy::input_focus::tab_navigation::TabIndex(0),
                bevy::text::TextColor(convert::color(self.inherited.foreground)),
                PfAutoSuggestInput { owner: entity },
            ))
            .id();
        self.apply_text_font(input);
        self.add_children(entity, &[input]);

        let mut items: Vec<String> = Vec::new();
        if let Some(XamlValue::Str(list)) = node.attribute("Suggestions") {
            items.extend(
                list.split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty()),
            );
        }
        for child in &node.children {
            if let Some(el) = child.as_element()
                && let Some(t) = el.text_content()
            {
                let t = t.trim().to_string();
                if !t.is_empty() {
                    items.push(t);
                }
            }
        }

        let overlay = ensure_overlay_root(self.world);
        let popup = self
            .world
            .spawn((
                PfPopup {
                    anchor: entity,
                    placement: PfPlacement::Bottom,
                    open: false,
                    match_anchor_width: true,
                },
                crate::components::PfLogicalParent(entity),
                Node {
                    position_type: PositionType::Absolute,
                    display: Display::None,
                    flex_direction: FlexDirection::Column,
                    border: UiRect::all(Val::Px(1.0)),
                    padding: UiRect::all(Val::Px(1.0)),
                    max_height: Val::Px(200.0),
                    overflow: bevy::ui::Overflow::scroll_y(),
                    ..Default::default()
                },
                BackgroundColor(Color::WHITE),
                BorderColor::all(Color::srgb_u8(0xAD, 0xAD, 0xAD)),
                bevy::ui::GlobalZIndex(i32::MAX - 900),
            ))
            .id();
        let backdrop = spawn_backdrop(self.world, popup);
        self.world
            .entity_mut(overlay)
            .add_children(&[backdrop, popup]);

        self.world.entity_mut(entity).insert((
            PfAutoSuggestBox {
                input,
                popup,
                items,
            },
            Interaction::default(),
        ));

        let popup_entity = popup;
        self.world.entity_mut(backdrop).observe(
            move |_click: On<Pointer<Click>>, mut popups: Query<&mut PfPopup>| {
                if let Ok(mut p) = popups.get_mut(popup_entity) {
                    p.open = false;
                }
            },
        );
        Ok(())
    }

    /// WinUI `NavigationView` (v1: `PaneDisplayMode=Left`): a pane of
    /// icon+label items driving an embedded chrome-less [`Frame`].
    /// Items navigate to their `Tag=` route (see `register_page`).
    fn spawn_navigation_view(&mut self, entity: Entity, node: &XamlNode) {
        use crate::components::{PfFrame, PfNavigationView};

        let pane = self
            .world
            .spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    width: Val::Px(170.0),
                    flex_shrink: 0.0,
                    padding: UiRect::all(Val::Px(6.0)),
                    row_gap: Val::Px(2.0),
                    ..Default::default()
                },
                BackgroundColor(Color::srgb_u8(0xF2, 0xF2, 0xF2)),
                PfElementKind("NavigationView.Pane".to_string()),
            ))
            .id();

        let content = self
            .world
            .spawn(Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                ..Default::default()
            })
            .id();
        let frame = self
            .world
            .spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0,
                    padding: UiRect::all(Val::Px(10.0)),
                    ..Default::default()
                },
                PfElementKind("Frame".to_string()),
            ))
            .id();
        self.add_children(frame, &[content]);
        self.world.entity_mut(frame).insert(PfFrame {
            content,
            chrome: None,
            back: Vec::new(),
            forward: Vec::new(),
            current: None,
            current_title: None,
            pending_source: None,
        });
        self.add_children(entity, &[pane, frame]);

        let mut items = Vec::new();
        let mut first_tag: Option<String> = None;
        for child in &node.children {
            let Some(el) = child.as_element() else {
                continue;
            };
            if el.name != "NavigationViewItem" {
                self.warn(format!(
                    "{}: NavigationView only takes NavigationViewItem children, got {}",
                    el.pos, el.name
                ));
                continue;
            }
            let label = match el.attribute("Content") {
                Some(XamlValue::Str(t)) => t.clone(),
                _ => String::new(),
            };
            let tag = match el.attribute("Tag") {
                Some(XamlValue::Str(t)) => t.clone(),
                _ => label.clone(),
            };

            let row = self
                .world
                .spawn((
                    Node {
                        display: Display::Flex,
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(8.0),
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..Default::default()
                    },
                    BackgroundColor(Color::NONE),
                    Interaction::default(),
                ))
                .id();
            if let Some(XamlValue::Str(kind)) = el.attribute("Icon")
                && let Some(data) = crate::icons::icon_path(kind)
            {
                let geometry = bevy_pf_xaml::geometry::parse_path_data(data)
                    .expect("icon path data is pre-validated");
                let shape = crate::shapes::build_shape(
                    "Path",
                    crate::shapes::ShapeParams {
                        data: Some(geometry),
                        fill: Some(v::PfBrush::Solid(v::PfColor::rgb(0x44, 0x44, 0x44))),
                        stretch: Some(v::Stretch::Uniform),
                        ..Default::default()
                    },
                )
                .expect("icon shape");
                let icon = self
                    .world
                    .spawn((
                        Node {
                            width: Val::Px(16.0),
                            height: Val::Px(16.0),
                            flex_shrink: 0.0,
                            ..Default::default()
                        },
                        shape,
                        crate::shapes::PfShapeRendered::default(),
                    ))
                    .id();
                self.add_children(row, &[icon]);
            }
            let text = self.spawn_text_child(label);
            self.add_children(row, &[text]);

            let index = items.len();
            let view = entity;
            let route = tag.clone();
            self.world.entity_mut(row).observe(
                move |_click: On<Pointer<Click>>, mut commands: Commands| {
                    let route = route.clone();
                    commands.queue(move |world: &mut World| {
                        nav_view_invoke(world, view, index, &route);
                    });
                },
            );
            items.push(row);
            if first_tag.is_none() {
                first_tag = Some(tag);
            }
        }
        self.add_children(pane, &items);
        self.world.entity_mut(entity).insert(PfNavigationView {
            frame,
            items,
            selected: None,
        });
        if let Some(tag) = first_tag {
            nav_view_invoke(self.world, entity, 0, &tag);
        }
    }

    /// The raw WPF `<Popup>` element: content lives on the overlay layer,
    /// anchored to the popup's XAML parent (resolved by a plugin system once
    /// the tree is assembled). `IsOpen` sets the initial state.
    fn spawn_popup_element(&mut self, entity: Entity, node: &XamlNode) -> Result<(), PfError> {
        use crate::components::PfPopupSource;
        use crate::overlay::{PfPlacement, PfPopup, ensure_overlay_root, spawn_backdrop};

        let placement = match node.attribute("Placement") {
            Some(XamlValue::Str(s)) if s.eq_ignore_ascii_case("right") => PfPlacement::Right,
            _ => PfPlacement::Bottom,
        };
        let is_open = match node.attribute("IsOpen") {
            Some(XamlValue::Str(s)) => s.eq_ignore_ascii_case("true"),
            _ => false,
        };

        let overlay = ensure_overlay_root(self.world);
        let popup = self
            .world
            .spawn((
                PfPopup {
                    // Placeholder anchor; resolve_popup_sources swaps in the
                    // XAML parent once the tree exists.
                    anchor: entity,
                    placement,
                    open: is_open,
                    match_anchor_width: false,
                },
                crate::components::PfLogicalParent(entity),
                Node {
                    position_type: PositionType::Absolute,
                    display: Display::None,
                    flex_direction: FlexDirection::Column,
                    border: UiRect::all(Val::Px(1.0)),
                    ..Default::default()
                },
                BackgroundColor(Color::WHITE),
                BorderColor::all(Color::srgb_u8(0xAD, 0xAD, 0xAD)),
                bevy::ui::GlobalZIndex(i32::MAX - 900),
            ))
            .id();
        let backdrop = spawn_backdrop(self.world, popup);
        self.world
            .entity_mut(overlay)
            .add_children(&[backdrop, popup]);
        let children = self.spawn_child_elements(node, ParentKind::FlexColumn)?;
        self.add_children(popup, &children);
        self.world
            .entity_mut(entity)
            .insert(PfPopupSource { popup });
        let popup_entity = popup;
        self.world.entity_mut(backdrop).observe(
            move |_click: On<Pointer<Click>>, mut popups: Query<&mut PfPopup>| {
                if let Ok(mut p) = popups.get_mut(popup_entity) {
                    p.open = false;
                }
            },
        );
        Ok(())
    }

    /// WPF `GridSplitter`: dragging resizes the two neighboring tracks of
    /// the parent Grid (explicit definitions become pixel tracks).
    fn spawn_grid_splitter(&mut self, entity: Entity, node: &XamlNode) {
        use crate::components::PfGridSplitter;

        let columns = match node.attribute("ResizeDirection") {
            Some(XamlValue::Str(s)) => s.eq_ignore_ascii_case("columns"),
            // WPF auto: a thin-width splitter resizes columns.
            _ => node.attribute("Width").is_some() || node.attribute("Height").is_none(),
        };
        self.world
            .entity_mut(entity)
            .insert(PfGridSplitter { columns });
        self.world.entity_mut(entity).observe(
            move |drag: On<Pointer<Drag>>, mut commands: Commands| {
                let splitter = drag.entity;
                let delta = drag.delta;
                commands.queue(move |world: &mut World| {
                    splitter_drag(world, splitter, delta);
                });
            },
        );
    }

    /// WPF `Calendar` month view; also the dropdown content of `DatePicker`.
    fn spawn_calendar(
        &mut self,
        entity: Entity,
        node: &XamlNode,
        owner_picker: Option<Entity>,
    ) -> Result<(), PfError> {
        use crate::components::PfCalendar;

        // Initial month: SelectedDate/DisplayDate ("YYYY-MM-DD") or today.
        let parse_date = |value: Option<&XamlValue>| -> Option<(i32, u32, u32)> {
            if let Some(XamlValue::Str(s)) = value {
                let mut it = s.split('-');
                let y = it.next()?.parse().ok()?;
                let m = it.next()?.parse().ok()?;
                let d = it.next()?.parse().ok()?;
                return Some((y, m, d));
            }
            None
        };
        let selected = parse_date(node.attribute("SelectedDate"));
        let (year, month) = selected
            .or(parse_date(node.attribute("DisplayDate")))
            .map(|(y, m, _)| (y, m))
            .unwrap_or_else(today_year_month);

        let prev = self.spawn_nav_button("<");
        let next = self.spawn_nav_button(">");
        let title = self.spawn_text_child(String::new());
        {
            let mut n = self.node_mut(title);
            n.flex_grow = 1.0;
            n.justify_content = JustifyContent::Center;
        }
        let header = self
            .world
            .spawn(Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                ..Default::default()
            })
            .id();
        self.add_children(header, &[prev, title, next]);

        let weekdays = self
            .world
            .spawn(Node {
                display: Display::Grid,
                grid_template_columns: vec![RepeatedGridTrack::fr(7, 1.0)],
                ..Default::default()
            })
            .id();
        for (i, wd) in ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"]
            .iter()
            .enumerate()
        {
            let cell = self.spawn_text_child(wd.to_string());
            {
                let mut n = self.node_mut(cell);
                n.grid_column = GridPlacement::start_span(i as i16 + 1, 1);
                n.justify_content = JustifyContent::Center;
                n.padding = UiRect::all(Val::Px(2.0));
            }
            self.add_children(weekdays, &[cell]);
        }

        let days_host = self
            .world
            .spawn(Node {
                display: Display::Grid,
                grid_template_columns: vec![RepeatedGridTrack::fr(7, 1.0)],
                ..Default::default()
            })
            .id();
        self.add_children(entity, &[header, weekdays, days_host]);
        self.world.entity_mut(entity).insert(PfCalendar {
            year,
            month,
            selected,
            days_host,
            title,
            owner_picker,
        });

        let cal = entity;
        self.world.entity_mut(prev).observe(
            move |_click: On<Pointer<Click>>, mut commands: Commands| {
                commands.queue(move |world: &mut World| calendar_shift_month(world, cal, -1));
            },
        );
        self.world.entity_mut(next).observe(
            move |_click: On<Pointer<Click>>, mut commands: Commands| {
                commands.queue(move |world: &mut World| calendar_shift_month(world, cal, 1));
            },
        );
        calendar_rebuild(self.world, entity);
        Ok(())
    }

    fn spawn_nav_button(&mut self, glyph: &str) -> Entity {
        let label = self.spawn_text_child(glyph.to_string());
        let button = self
            .world
            .spawn((
                Node {
                    display: Display::Flex,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    width: Val::Px(24.0),
                    height: Val::Px(22.0),
                    ..Default::default()
                },
                BackgroundColor(Color::srgb_u8(0xE8, 0xE8, 0xE8)),
                Interaction::default(),
            ))
            .id();
        self.add_children(button, &[label]);
        button
    }

    /// WPF `DatePicker`: a display box with a calendar dropdown.
    fn spawn_date_picker(&mut self, entity: Entity, node: &XamlNode) -> Result<(), PfError> {
        use crate::components::{PfCalendar, PfDatePicker};
        use crate::overlay::{PfPlacement, PfPopup, ensure_overlay_root, spawn_backdrop};

        let display = self.spawn_text_child("Select a date".to_string());
        {
            let mut n = self.node_mut(display);
            n.flex_grow = 1.0;
        }
        let arrow = self.spawn_text_child("\u{25BE}".to_string());
        self.add_children(entity, &[display, arrow]);

        let overlay = ensure_overlay_root(self.world);
        let popup = self
            .world
            .spawn((
                PfPopup {
                    anchor: entity,
                    placement: PfPlacement::Bottom,
                    open: false,
                    match_anchor_width: false,
                },
                crate::components::PfLogicalParent(entity),
                Node {
                    position_type: PositionType::Absolute,
                    display: Display::None,
                    flex_direction: FlexDirection::Column,
                    ..Default::default()
                },
                bevy::ui::GlobalZIndex(i32::MAX - 900),
            ))
            .id();
        let backdrop = spawn_backdrop(self.world, popup);
        self.world
            .entity_mut(overlay)
            .add_children(&[backdrop, popup]);

        let calendar = self
            .world
            .spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    width: Val::Px(252.0),
                    padding: UiRect::all(Val::Px(8.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    row_gap: Val::Px(4.0),
                    ..Default::default()
                },
                BackgroundColor(Color::WHITE),
                BorderColor::all(Color::srgb_u8(0xAD, 0xAD, 0xAD)),
                PfElementKind("Calendar".to_string()),
            ))
            .id();
        self.spawn_calendar(calendar, node, Some(entity))?;
        self.add_children(popup, &[calendar]);

        let selected = self
            .world
            .get::<PfCalendar>(calendar)
            .and_then(|c| c.selected);
        self.world.entity_mut(entity).insert((
            PfDatePicker {
                calendar,
                popup,
                display,
                selected,
            },
            Interaction::default(),
        ));
        if let Some((y, m, d)) = selected {
            set_text(self.world, display, format!("{y:04}-{m:02}-{d:02}"));
        }

        let popup_entity = popup;
        self.world.entity_mut(entity).observe(
            move |_click: On<Pointer<Click>>, mut popups: Query<&mut PfPopup>| {
                if let Ok(mut p) = popups.get_mut(popup_entity) {
                    p.open = !p.open;
                }
            },
        );
        self.world.entity_mut(backdrop).observe(
            move |_click: On<Pointer<Click>>, mut popups: Query<&mut PfPopup>| {
                if let Ok(mut p) = popups.get_mut(popup_entity) {
                    p.open = false;
                }
            },
        );
        Ok(())
    }

    /// Map a recorded `{Binding}` onto its concrete target entity + slot.
    fn attach_binding(
        &mut self,
        entity: Entity,
        kind: ElemKind,
        property: &str,
        spec: crate::binding::BindingSpec,
    ) {
        use crate::binding::{BindingTarget, PfBinding, PfBindings};
        // `DataContext="{Binding path}"` re-scopes the inherited context for
        // this element and its descendants — not a scalar binding. Resolution
        // is lazy (a marker consumed by `find_context`) because the model
        // usually attaches from Rust after instantiation.
        if property == "DataContext" {
            self.world
                .entity_mut(entity)
                .insert(crate::binding::DataContextScope(spec.path));
            return;
        }
        // Content on a ContentControl is not a scalar binding: it selects a
        // DataTemplate (explicit ContentTemplate, else DataType-implicit)
        // and regenerates the subtree on model change.
        if property == "Content" && self.pending.is_content_control {
            let scopes = std::sync::Arc::new(self.scopes.clone());
            self.world
                .entity_mut(entity)
                .insert(crate::items::PfContentSource {
                    path: spec.path,
                    template: self.pending.content_template.clone(),
                    scopes: Some(scopes),
                    seen_version: 0,
                });
            return;
        }
        // ItemsSource is not a scalar binding: it generates children.
        if property == "ItemsSource" {
            let host = match kind {
                // A ListView in GridView (column) mode hosts rows like a DataGrid.
                ElemKind::ListBox
                    if self
                        .world
                        .get::<crate::components::PfDataGrid>(entity)
                        .is_some() =>
                {
                    crate::items::ItemsHostKind::DataGrid
                }
                ElemKind::ListBox => crate::items::ItemsHostKind::ListBox,
                ElemKind::ItemsControl => crate::items::ItemsHostKind::ItemsControl,
                ElemKind::ComboBox => crate::items::ItemsHostKind::ComboBox,
                ElemKind::DataGrid => crate::items::ItemsHostKind::DataGrid,
                _ => {
                    self.warn(format!("ItemsSource on `{kind:?}` is not supported yet"));
                    return;
                }
            };
            let scopes = std::sync::Arc::new(self.scopes.clone());
            self.world
                .entity_mut(entity)
                .insert(crate::items::PfItemsSource {
                    path: spec.path,
                    template: self.pending.item_template.clone(),
                    scopes: Some(scopes),
                    display_member: self.pending.display_member.clone(),
                    container_style: self.pending.item_container_style.clone(),
                    kind: host,
                    seen_version: 0,
                });
            return;
        }
        if property == "Text" && kind == ElemKind::TextBox && self.pending.text_input.is_none() {
            // Only reachable for a templated TextBox missing PART_ContentHost;
            // binding into the component-less root would be a silent no-op.
            self.warn(
                "Text binding skipped: templated TextBox has no PART_ContentHost".to_string(),
            );
            return;
        }
        let (target_entity, target, default_mode) = match property {
            "Text" if kind == ElemKind::TextBox => (
                self.pending.text_input.unwrap_or(entity),
                BindingTarget::EditableText,
                v::BindingMode::TwoWay,
            ),
            "SelectedIndex" if matches!(kind, ElemKind::ListBox | ElemKind::ComboBox) => {
                (entity, BindingTarget::SelectedIndex, v::BindingMode::TwoWay)
            }
            // TwoWay like WPF: the item flows both ways. Both are
            // resolved against the view model rather than through a scalar,
            // so an ElementName/RelativeSource source has nothing to compare
            // the collection against and is refused rather than half-working.
            "SelectedItem" | "SelectedValue"
                if matches!(kind, ElemKind::ListBox | ElemKind::ComboBox) =>
            {
                if spec.element_name.is_some() || spec.relative.is_some() {
                    self.warn(format!(
                        "{property} only binds to the DataContext, not to an element or \
                         RelativeSource; binding skipped"
                    ));
                    return;
                }
                let target = if property == "SelectedItem" {
                    BindingTarget::SelectedItem
                } else {
                    BindingTarget::SelectedValue
                };
                (entity, target, v::BindingMode::TwoWay)
            }
            "Text" => (entity, BindingTarget::Text, v::BindingMode::OneWay),
            "Content" | "Header" => (
                self.pending.content_text.unwrap_or(entity),
                BindingTarget::Text,
                v::BindingMode::OneWay,
            ),
            "IsChecked" => (entity, BindingTarget::IsChecked, v::BindingMode::TwoWay),
            "Value" if matches!(kind, ElemKind::Slider | ElemKind::ScrollBar) => {
                (entity, BindingTarget::SliderValue, v::BindingMode::TwoWay)
            }
            "Value" if kind == ElemKind::ProgressBar => {
                (entity, BindingTarget::ProgressValue, v::BindingMode::OneWay)
            }
            // Some dialects spell visibility as a bool. The Visibility target
            // already accepts booleans as well as the WPF enum strings, so
            // the two names share it.
            "Visibility" | "IsVisible" => {
                (entity, BindingTarget::Visibility, v::BindingMode::OneWay)
            }
            "Width" => (entity, BindingTarget::Width, v::BindingMode::OneWay),
            "Height" => (entity, BindingTarget::Height, v::BindingMode::OneWay),
            "FontSize" => (entity, BindingTarget::FontSize, v::BindingMode::OneWay),
            "Foreground" => (
                if kind == ElemKind::TextBlock {
                    entity
                } else {
                    self.pending.content_text.unwrap_or(entity)
                },
                BindingTarget::Foreground,
                v::BindingMode::OneWay,
            ),
            "Background" => (entity, BindingTarget::Background, v::BindingMode::OneWay),
            "BorderBrush" => (entity, BindingTarget::BorderBrush, v::BindingMode::OneWay),
            "Stroke" if kind == ElemKind::Shape => {
                (entity, BindingTarget::Stroke, v::BindingMode::OneWay)
            }
            "Fill" if kind == ElemKind::Shape => {
                (entity, BindingTarget::Fill, v::BindingMode::OneWay)
            }
            other => {
                // Not one of the control-state targets: any store-managed
                // property binds OneWay through the precedence store.
                if let Some(target) = crate::provider::property_target_for(other) {
                    (entity, BindingTarget::Store(target), v::BindingMode::OneWay)
                } else {
                    self.warn(format!(
                        "binding on property `{other}` is not supported yet"
                    ));
                    return;
                }
            }
        };
        let mode = match spec.mode {
            v::BindingMode::Default => default_mode,
            explicit => explicit,
        };
        let source = match (&spec.relative, spec.element_name) {
            (Some(crate::binding::PfRelativeSource::SelfSource), _) => {
                crate::binding::PfBindingSource::Element(entity)
            }
            (Some(crate::binding::PfRelativeSource::TemplatedParent), _) => {
                match self
                    .world
                    .get::<crate::components::PfTemplatedParent>(entity)
                {
                    Some(parent) => crate::binding::PfBindingSource::Element(parent.0),
                    None => {
                        self.warn(
                            "RelativeSource TemplatedParent outside a template; binding skipped"
                                .to_string(),
                        );
                        return;
                    }
                }
            }
            (Some(crate::binding::PfRelativeSource::Ancestor(ty)), _) => {
                // Parents attach after the child spawns, so the walk is
                // deferred to resolve_deferred_sources.
                crate::binding::PfBindingSource::Ancestor(ty.clone())
            }
            (None, Some(name)) => crate::binding::PfBindingSource::Named(name),
            (None, None) => crate::binding::PfBindingSource::DataContext,
        };
        let binding = PfBinding {
            target,
            source,
            path: spec.path,
            mode,
            string_format: spec.string_format,
            converter: spec.converter,
            converter_parameter: spec.converter_parameter,
            fallback: spec.fallback,
            target_null: spec.target_null,
            multi_paths: spec.multi_paths,
            seen_version: 0,
        };
        let mut e = self.world.entity_mut(target_entity);
        if let Some(mut bindings) = e.get_mut::<PfBindings>() {
            bindings.0.push(binding);
        } else {
            e.insert(PfBindings(vec![binding]));
        }
        self.binding_entities.push(target_entity);
    }

    /// Build the nested-wrapper chain that emulates WPF DockPanel layout.
    fn build_dock_chain(&mut self, children: &[Entity], last_fill: bool) -> Option<Entity> {
        if children.is_empty() {
            return None;
        }
        let (docked, fill_child) = if last_fill {
            let (last, rest) = children.split_last().unwrap();
            (rest, Some(*last))
        } else {
            (children, None)
        };

        // Innermost node: the fill child (stretched in a single-cell grid) or
        // an empty filler.
        let mut inner = match fill_child {
            Some(c) => {
                let wrapper = self
                    .world
                    .spawn((
                        Node {
                            display: Display::Grid,
                            grid_template_rows: vec![GridTrack::fr(1.0)],
                            grid_template_columns: vec![GridTrack::fr(1.0)],
                            flex_grow: 1.0,
                            ..Default::default()
                        },
                        PfElementKind("DockPanel.Slot".to_string()),
                    ))
                    .id();
                self.place_grid_child(c, 1, 1);
                // Deferred alignment: fill child resolves on both axes.
                let (h, v) = self.take_dock_alignment(c);
                {
                    let mut n = self.node_mut(c);
                    if let Some(h) = h {
                        n.justify_self = convert::h_justify_self(h);
                    }
                    if let Some(v) = v {
                        n.align_self = convert::v_align_self(v);
                    }
                }
                self.add_children(wrapper, &[c]);
                wrapper
            }
            None => self
                .world
                .spawn((
                    Node {
                        flex_grow: 1.0,
                        ..Default::default()
                    },
                    PfElementKind("DockPanel.Slot".to_string()),
                ))
                .id(),
        };

        for &child in docked.iter().rev() {
            let dock: v::Dock = self
                .world
                .get::<PfAttachedProps>(child)
                .and_then(|a| a.parse("DockPanel", "Dock"))
                .unwrap_or(v::Dock::Left); // WPF default
            let (dir, child_first) = match dock {
                v::Dock::Top => (FlexDirection::Column, true),
                v::Dock::Bottom => (FlexDirection::Column, false),
                v::Dock::Left => (FlexDirection::Row, true),
                v::Dock::Right => (FlexDirection::Row, false),
            };
            // Deferred alignment: the docked slot spans the full remaining
            // cross extent; only cross-axis alignment applies
            // (DockPanel.cs:278-308; conformance note L5). Docked children
            // keep their desired size on the dock axis (never shrink).
            let (h, v) = self.take_dock_alignment(child);
            {
                let mut n = self.node_mut(child);
                n.flex_shrink = 0.0;
                match dir {
                    FlexDirection::Row => {
                        if let Some(v) = v {
                            n.align_self = convert::v_align_self(v);
                        }
                    }
                    _ => {
                        if let Some(h) = h {
                            n.align_self = convert::h_align_self(h);
                        }
                    }
                }
            }
            let wrapper = self
                .world
                .spawn((
                    Node {
                        display: Display::Flex,
                        flex_direction: dir,
                        align_items: AlignItems::Stretch,
                        flex_grow: 1.0,
                        ..Default::default()
                    },
                    PfElementKind("DockPanel.Slot".to_string()),
                ))
                .id();
            if child_first {
                self.add_children(wrapper, &[child, inner]);
            } else {
                self.add_children(wrapper, &[inner, child]);
            }
            inner = wrapper;
        }
        Some(inner)
    }

    /// Read (and consume) alignment stashed on a DockPanel child.
    fn take_dock_alignment(
        &mut self,
        child: Entity,
    ) -> (Option<v::HorizontalAlignment>, Option<v::VerticalAlignment>) {
        let Some(mut attached) = self.world.get_mut::<PfAttachedProps>(child) else {
            return (None, None);
        };
        let h = attached.0.remove("Pf.HAlign").and_then(|s| s.parse().ok());
        let v = attached.0.remove("Pf.VAlign").and_then(|s| s.parse().ok());
        (h, v)
    }
}

/// Parse `"Auto,*,2*,100"` shorthand into grid tracks.
fn parse_track_list(s: &str) -> Result<Vec<RepeatedGridTrack>, PfError> {
    let mut tracks = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        tracks.push(convert::grid_track(part.parse::<v::GridLength>()?));
    }
    if tracks.is_empty() {
        tracks.push(GridTrack::fr(1.0));
    }
    Ok(tracks)
}

/// Flatten a TextBlock's inline content (text runs, Run/Bold/... elements,
/// LineBreak) into a single string. Rich inline formatting is planned via
/// text spans.
fn collect_inline_text(node: &XamlNode) -> String {
    fn walk(children: &[XamlChild], out: &mut String) {
        for child in children {
            match child {
                XamlChild::Text(t) => out.push_str(t),
                XamlChild::Element(el) => match el.name.as_str() {
                    "LineBreak" => out.push('\n'),
                    "Run" | "Bold" | "Italic" | "Underline" | "Span" | "Hyperlink" => {
                        if let Some(XamlValue::Str(t)) = el.attribute("Text") {
                            out.push_str(t);
                        } else {
                            walk(&el.children, out);
                        }
                    }
                    _ => walk(&el.children, out),
                },
            }
        }
    }
    let mut out = String::new();
    walk(&node.children, &mut out);
    out
}

/// Instantiate a stored XAML subtree (a `DataTemplate` body) under `parent`
/// at runtime. Resources resolve against the application tier.
pub fn instantiate_template(
    world: &mut World,
    parent: Entity,
    node: &XamlNode,
    scopes: Option<&ResourceScopes>,
) -> Result<Entity, PfError> {
    let mut ctx = Ctx::new(world, &XamlEnv::default());
    if let Some(declaring_scope) = scopes {
        // Templates resolve resources against the lexical scope where they
        // were declared (WPF semantics), falling back to app resources.
        let fresh_app = ctx.scopes.app.take();
        ctx.scopes = declaring_scope.clone();
        if ctx.scopes.app.is_none() {
            ctx.scopes.app = fresh_app;
        }
    }
    let entity = ctx.spawn_element(node, ParentKind::FlexColumn, None)?;
    let names: bevy::platform::collections::HashMap<String, Entity> =
        ctx.names.iter().cloned().collect();
    let binding_entities = std::mem::take(&mut ctx.binding_entities);
    let mut warnings = std::mem::take(&mut ctx.warnings);
    world.entity_mut(parent).add_children(&[entity]);
    // Deferred sources resolve AFTER the attach so FindAncestor can walk
    // out of the template into the surrounding tree.
    crate::binding::resolve_deferred_sources(world, &binding_entities, &names, &mut warnings);
    for w in warnings {
        bevy::log::warn!("bevy_pf (template): {w}");
    }
    Ok(entity)
}

/// Apply a parsed `Style` to an existing entity at runtime (generated item
/// containers): setters, then trigger attach — the same path spawn-time
/// styling takes, minus event triggers (no page scope to defer into).
pub(crate) fn apply_style_runtime(
    world: &mut World,
    entity: Entity,
    kind_name: &str,
    style: &crate::resources::PfStyle,
    scopes: Option<&ResourceScopes>,
) {
    let mut ctx = Ctx::new(world, &XamlEnv::default());
    if let Some(declaring_scope) = scopes {
        let fresh_app = ctx.scopes.app.take();
        ctx.scopes = declaring_scope.clone();
        if ctx.scopes.app.is_none() {
            ctx.scopes.app = fresh_app;
        }
    }
    let kind = ElemKind::from_name(kind_name);
    for setter in style.setters.clone() {
        ctx.apply_setter(entity, kind, ParentKind::FlexColumn, &setter);
    }
    if !style.triggers.is_empty() {
        ctx.attach_triggers(entity, &style.triggers, None);
    }
    for w in std::mem::take(&mut ctx.warnings) {
        bevy::log::warn!("bevy_pf (item style): {w}");
    }
}

/// A custom-element builder: runs after the framework spawned the entity,
/// applied common properties (Width, Background, Margin, bindings, ...),
/// and spawned child elements. Read custom attributes off `node` and
/// insert your components/children.
type ElementBuilder = dyn Fn(&mut World, Entity, &XamlNode) + Send + Sync;

/// Registered custom XAML elements, by local type name (`<local:Gauge>`
/// matches a registration for `"Gauge"`).
#[derive(Resource, Default, Clone)]
pub struct PfElements(
    pub(crate) bevy::platform::collections::HashMap<String, std::sync::Arc<ElementBuilder>>,
);

/// `App` extension: register a custom element usable from XAML.
pub trait PfElementAppExt {
    fn register_element(
        &mut self,
        name: impl Into<String>,
        builder: impl Fn(&mut World, Entity, &XamlNode) + Send + Sync + 'static,
    ) -> &mut Self;
}

impl PfElementAppExt for bevy::app::App {
    fn register_element(
        &mut self,
        name: impl Into<String>,
        builder: impl Fn(&mut World, Entity, &XamlNode) + Send + Sync + 'static,
    ) -> &mut Self {
        let world = self.world_mut();
        if !world.contains_resource::<PfElements>() {
            world.init_resource::<PfElements>();
        }
        world
            .resource_mut::<PfElements>()
            .0
            .insert(name.into(), std::sync::Arc::new(builder));
        self
    }
}

/// Nudge a ScrollBar by +/- SmallChange (what the line buttons run).
pub fn scroll_bar_nudge(world: &mut World, bar: Entity, dir: f32) {
    let Some(sb) = world.get::<crate::components::PfScrollBar>(bar) else {
        return;
    };
    let step = sb.small_change * dir;
    let (min, max) = match world.get::<bevy::ui_widgets::SliderRange>(bar) {
        Some(r) => (r.start(), r.end()),
        None => (0.0, 100.0),
    };
    if let Some(v) = world.get::<bevy::ui_widgets::SliderValue>(bar).map(|v| v.0) {
        let next = (v + step).clamp(min, max);
        if next != v {
            world
                .entity_mut(bar)
                .insert(bevy::ui_widgets::SliderValue(next));
        }
    }
}

/// Convert a thumb drag delta into a value change using live track pixels.
fn scroll_bar_drag(world: &mut World, bar: Entity, delta: Vec2) {
    let Some(sb) = world.get::<crate::components::PfScrollBar>(bar).cloned() else {
        return;
    };
    let axis_px = if sb.horizontal { delta.x } else { delta.y };
    if axis_px == 0.0 {
        return;
    }
    let track_len = world
        .get::<bevy::ui::ComputedNode>(sb.track)
        .map(|n| {
            let size = n.size() * n.inverse_scale_factor();
            if sb.horizontal { size.x } else { size.y }
        })
        .unwrap_or(0.0);
    let thumb_len = world
        .get::<bevy::ui::ComputedNode>(sb.thumb)
        .map(|n| {
            let size = n.size() * n.inverse_scale_factor();
            if sb.horizontal { size.x } else { size.y }
        })
        .unwrap_or(0.0);
    let usable = (track_len - thumb_len).max(1.0);
    let (min, max) = match world.get::<bevy::ui_widgets::SliderRange>(bar) {
        Some(r) => (r.start(), r.end()),
        None => (0.0, 100.0),
    };
    if let Some(v) = world.get::<bevy::ui_widgets::SliderValue>(bar).map(|v| v.0) {
        let next = (v + axis_px / usable * (max - min)).clamp(min, max);
        if next != v {
            world
                .entity_mut(bar)
                .insert(bevy::ui_widgets::SliderValue(next));
        }
    }
}

/// Select a ComboBox item by index: update the text presenter, remember the
/// selection, and close the dropdown.
pub fn select_combo_index(world: &mut World, combo: Entity, index: usize) {
    let Some(state) = world.get::<crate::components::PfComboBox>(combo).cloned() else {
        return;
    };
    let items: Vec<Entity> = world
        .get::<Children>(state.popup)
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    let Some(&item) = items.get(index) else {
        return;
    };
    let text = first_text_in(world, item).unwrap_or_default();
    if let Some(mut t) = world.get_mut::<bevy::ui::widget::Text>(state.text) {
        t.0 = text;
    }
    if let Some(mut c) = world.get_mut::<crate::components::PfComboBox>(combo) {
        c.selected = Some(index);
        c.open = false;
    }
}

/// The first `Text` string in an entity's subtree (selection presenter text).
/// First descendant whose `PfElementKind` matches `kind` (self included).
fn find_kind_entity(world: &World, root: Entity, kind: &str) -> Option<Entity> {
    if world
        .get::<crate::components::PfElementKind>(root)
        .is_some_and(|k| k.0 == kind)
    {
        return Some(root);
    }
    let children = world.get::<Children>(root)?;
    let kids: Vec<Entity> = children.iter().collect();
    kids.into_iter()
        .find_map(|c| find_kind_entity(world, c, kind))
}

/// Every descendant whose `PfElementKind` matches `kind`, depth-first.
fn collect_kind_entities(world: &World, root: Entity, kind: &str) -> Vec<Entity> {
    let mut out = Vec::new();
    fn walk(world: &World, e: Entity, kind: &str, out: &mut Vec<Entity>) {
        if world
            .get::<crate::components::PfElementKind>(e)
            .is_some_and(|k| k.0 == kind)
        {
            out.push(e);
        }
        if let Some(children) = world.get::<Children>(e) {
            let kids: Vec<Entity> = children.iter().collect();
            for c in kids {
                walk(world, c, kind, out);
            }
        }
    }
    walk(world, root, kind, &mut out);
    out
}

fn is_descendant_of(world: &World, entity: Entity, ancestor: Entity) -> bool {
    let mut cursor = entity;
    while let Some(parent) = world.get::<ChildOf>(cursor) {
        cursor = parent.parent();
        if cursor == ancestor {
            return true;
        }
    }
    false
}

/// First descendant ENTITY carrying a `Text` component.
fn find_text_entity(world: &World, root: Entity) -> Option<Entity> {
    if world.get::<bevy::ui::widget::Text>(root).is_some() {
        return Some(root);
    }
    let children = world.get::<Children>(root)?;
    let kids: Vec<Entity> = children.iter().collect();
    kids.into_iter().find_map(|c| find_text_entity(world, c))
}

pub(crate) fn first_text_in(world: &World, root: Entity) -> Option<String> {
    if let Some(t) = world.get::<bevy::ui::widget::Text>(root) {
        return Some(t.0.clone());
    }
    let children = world.get::<Children>(root)?;
    for child in children.iter() {
        if let Some(t) = first_text_in(world, child) {
            return Some(t);
        }
    }
    None
}

/// Select a tab by index: toggle content visibility and header chrome.
pub fn select_tab(world: &mut World, tab_control: Entity, index: usize) {
    let Some(state) = world
        .get::<crate::components::PfTabControl>(tab_control)
        .cloned()
    else {
        return;
    };
    if state.contents.is_empty() {
        return;
    }
    let index = index.min(state.contents.len() - 1);
    for (i, &content) in state.contents.iter().enumerate() {
        if let Some(mut node) = world.get_mut::<Node>(content) {
            node.display = if i == index {
                Display::Grid
            } else {
                Display::None
            };
        }
    }
    for (i, &header) in state.headers.iter().enumerate() {
        let selected = i == index;
        if let Some(mut bg) = world.get_mut::<BackgroundColor>(header) {
            bg.0 = if selected {
                Color::WHITE
            } else {
                Color::srgb_u8(0xF0, 0xF0, 0xF0)
            };
        }
    }
    if let Some(mut state) = world.get_mut::<crate::components::PfTabControl>(tab_control) {
        state.selected = index;
    }
}

/// Select a tree item and toggle its expansion (WPF expands on the arrow;
/// v1 toggles on any header click).
pub fn toggle_tree_item(world: &mut World, tree: Entity, item: Entity) {
    if let Some(state) = world.get::<crate::components::PfTreeItem>(item).cloned()
        && state.has_children
    {
        let expanded = !state.expanded;
        if let Some(mut node) = world.get_mut::<Node>(state.container) {
            node.display = if expanded {
                Display::Flex
            } else {
                Display::None
            };
        }
        if let Some(mut text) = world.get_mut::<bevy::ui::widget::Text>(state.arrow) {
            text.0 = if expanded { "−" } else { "+" }.to_string();
        }
        if let Some(mut s) = world.get_mut::<crate::components::PfTreeItem>(item) {
            s.expanded = expanded;
        }
    }
    // Selection highlight on the header rows.
    let previous = world
        .get::<crate::components::PfTreeView>(tree)
        .and_then(|t| t.selected);
    if let Some(prev) = previous {
        let prev_header = world.get::<Children>(prev).and_then(|c| c.iter().next());
        if let Some(h) = prev_header
            && let Some(mut bg) = world.get_mut::<BackgroundColor>(h)
        {
            bg.0 = Color::NONE;
        }
    }
    let selection_fill = world
        .get_resource::<crate::components::PfControlTheme>()
        .map(|t| t.selection_fill)
        .unwrap_or(crate::plugin::LIST_SELECTED_BG);
    let header = world.get::<Children>(item).and_then(|c| c.iter().next());
    if let Some(h) = header
        && let Some(mut bg) = world.get_mut::<BackgroundColor>(h)
    {
        bg.0 = selection_fill;
    }
    if let Some(mut t) = world.get_mut::<crate::components::PfTreeView>(tree) {
        t.selected = Some(item);
    }
}

/// Menu item activation: parents toggle their submenu; leaves close the
/// whole menu (users observe `Pointer<Click>` on named items for actions).
pub fn activate_menu_item(world: &mut World, item: Entity) {
    let Some(state) = world.get::<crate::components::PfMenuItem>(item).cloned() else {
        return;
    };
    match state.submenu {
        Some(popup) => {
            let is_open = world
                .get::<crate::overlay::PfPopup>(popup)
                .is_some_and(|p| p.open);
            if !is_open {
                // Close sibling popups of the same menu, then open this one.
                close_menu_popups(world, state.menu_root);
                open_menu_chain(world, item);
            } else {
                close_popup_subtree(world, popup);
            }
        }
        None => {
            toggle_checkable_menu_item(world, item);
            close_menu_popups(world, state.menu_root);
        }
    }
}

/// Flip a checkable MenuItem's `Checked` state and its glyph.
fn toggle_checkable_menu_item(world: &mut World, item: Entity) {
    let Some(state) = world
        .get::<crate::components::PfCheckableMenuItem>(item)
        .cloned()
    else {
        return;
    };
    let was_checked = world.get::<bevy::ui::Checked>(item).is_some();
    if was_checked {
        world.entity_mut(item).remove::<bevy::ui::Checked>();
    } else {
        world.entity_mut(item).insert(bevy::ui::Checked);
    }
    if let Some(mut n) = world.get_mut::<Node>(state.glyph) {
        n.display = if was_checked {
            Display::None
        } else {
            Display::Flex
        };
    }
}

/// Open the submenu of `item` plus every ancestor submenu on its chain.
fn open_menu_chain(world: &mut World, mut item: Entity) {
    loop {
        if let Some(state) = world.get::<crate::components::PfMenuItem>(item).cloned()
            && let Some(popup) = state.submenu
            && let Some(mut p) = world.get_mut::<crate::overlay::PfPopup>(popup)
        {
            p.open = true;
        }
        // Walk up through popup logical parents to keep ancestors open.
        let Some(parent) = world
            .get::<ChildOf>(item)
            .map(|c| c.parent())
            .and_then(|p| {
                world
                    .get::<crate::components::PfLogicalParent>(p)
                    .map(|l| l.0)
            })
        else {
            break;
        };
        item = parent;
    }
}

/// Close every popup belonging to a menu root.
pub fn close_menu_popups(world: &mut World, menu_root: Entity) {
    let mut query = world.query::<(Entity, &crate::components::PfMenuPopup)>();
    let popups: Vec<Entity> = query
        .iter(world)
        .filter(|(_, m)| m.menu_root == menu_root)
        .map(|(e, _)| e)
        .collect();
    for popup in popups {
        if let Some(mut p) = world.get_mut::<crate::overlay::PfPopup>(popup)
            && p.open
        {
            p.open = false;
        }
    }
}

fn close_popup_subtree(world: &mut World, popup: Entity) {
    if let Some(mut p) = world.get_mut::<crate::overlay::PfPopup>(popup) {
        p.open = false;
    }
    let children: Vec<Entity> = world
        .get::<Children>(popup)
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    for child in children {
        if let Some(state) = world.get::<crate::components::PfMenuItem>(child).cloned()
            && let Some(sub) = state.submenu
        {
            close_popup_subtree(world, sub);
        }
    }
}

/// Replace the string of a text entity (or its first text descendant).
pub fn set_text(world: &mut World, entity: Entity, text: String) {
    fn find(world: &World, e: Entity) -> Option<Entity> {
        if world.get::<bevy::ui::widget::Text>(e).is_some() {
            return Some(e);
        }
        let children: Vec<Entity> = world.get::<Children>(e)?.iter().collect();
        children.into_iter().find_map(|c| find(world, c))
    }
    if let Some(target) = find(world, entity)
        && let Some(mut t) = world.get_mut::<bevy::ui::widget::Text>(target)
    {
        t.0 = text;
    }
}

/// Today's (year, month). `std::time` does not exist on wasm32 targets
/// (`SystemTime::now()` aborts), so the browser clock is used there.
fn today_year_month() -> (i32, u32) {
    #[cfg(target_arch = "wasm32")]
    {
        let date = js_sys::Date::new_0();
        // JS months are 0-based.
        (date.get_full_year() as i32, date.get_month() + 1)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        if secs == 0 {
            return (2026, 1);
        }
        let (y, m, _) = civil_from_days(secs.div_euclid(86_400));
        (y, m)
    }
}

/// Days-since-epoch -> (year, month, day). Howard Hinnant's algorithm.
#[cfg(not(target_arch = "wasm32"))] // wasm reads the browser clock instead
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((if m <= 2 { y + 1 } else { y }) as i32, m, d)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
    }
}

/// Day of week for a date, 0 = Sunday (Sakamoto's method).
fn day_of_week(year: i32, month: u32, day: u32) -> u32 {
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if month < 3 { year - 1 } else { year };
    ((y + y / 4 - y / 100 + y / 400 + T[(month - 1) as usize] + day as i32) % 7).rem_euclid(7)
        as u32
}

const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Repaint a Calendar's day grid + title for its current month/selection.
pub fn calendar_rebuild(world: &mut World, calendar: Entity) {
    use crate::components::PfCalendar;

    let Some(state) = world.get::<PfCalendar>(calendar).cloned() else {
        return;
    };
    set_text(
        world,
        state.title,
        format!("{} {}", MONTH_NAMES[(state.month - 1) as usize], state.year),
    );
    world.entity_mut(state.days_host).despawn_children();

    let first_dow = day_of_week(state.year, state.month, 1);
    let days = days_in_month(state.year, state.month);
    let mut cells = Vec::new();
    for day in 1..=days {
        let slot = first_dow + day - 1;
        let selected = state.selected == Some((state.year, state.month, day));
        let label = crate::items::spawn_runtime_text(world, &day.to_string());
        let cell = world
            .spawn((
                Node {
                    display: Display::Flex,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    padding: UiRect::all(Val::Px(3.0)),
                    grid_column: GridPlacement::start_span((slot % 7) as i16 + 1, 1),
                    grid_row: GridPlacement::start_span((slot / 7) as i16 + 1, 1),
                    ..Default::default()
                },
                if selected {
                    BackgroundColor(crate::components::ACCENT)
                } else {
                    BackgroundColor(Color::NONE)
                },
                Interaction::default(),
            ))
            .id();
        if selected && let Some(mut color) = world.get_mut::<bevy::text::TextColor>(label) {
            color.0 = Color::WHITE;
        }
        world.entity_mut(cell).add_children(&[label]);
        let cal = calendar;
        let (y, m) = (state.year, state.month);
        world.entity_mut(cell).observe(
            move |_click: On<Pointer<Click>>, mut commands: Commands| {
                commands.queue(move |world: &mut World| calendar_select(world, cal, y, m, day));
            },
        );
        cells.push(cell);
    }
    world.entity_mut(state.days_host).add_children(&cells);
}

/// Move a Calendar by whole months (negative = back).
pub fn calendar_shift_month(world: &mut World, calendar: Entity, delta: i32) {
    use crate::components::PfCalendar;
    if let Some(mut state) = world.get_mut::<PfCalendar>(calendar) {
        let mut m = state.month as i32 + delta;
        let mut y = state.year;
        while m < 1 {
            m += 12;
            y -= 1;
        }
        while m > 12 {
            m -= 12;
            y += 1;
        }
        state.month = m as u32;
        state.year = y;
    }
    calendar_rebuild(world, calendar);
}

/// Set a TimePicker's hour and/or minute; updates the display and closes the
/// popup once a minute is chosen.
pub fn time_picker_set(world: &mut World, picker: Entity, hour: Option<u32>, minute: Option<u32>) {
    use crate::components::PfTimePicker;
    let Some(mut p) = world.get_mut::<PfTimePicker>(picker) else {
        return;
    };
    if hour.is_some() {
        p.hour = hour;
    }
    if minute.is_some() {
        p.minute = minute;
    }
    let (h, m, display, popup) = (p.hour, p.minute, p.display, p.popup);
    let text = match (h, m) {
        (Some(h), Some(m)) => format!("{h:02}:{m:02}"),
        (Some(h), None) => format!("{h:02}:--"),
        (None, Some(m)) => format!("--:{m:02}"),
        (None, None) => "Select time".to_string(),
    };
    set_text(world, display, text);
    if minute.is_some()
        && let Some(mut pop) = world.get_mut::<crate::overlay::PfPopup>(popup)
    {
        pop.open = false;
    }
}

/// `#RRGGBB` for a color (alpha dropped).
pub fn color_to_hex(color: Color) -> String {
    let c = color.to_srgba();
    format!(
        "#{:02X}{:02X}{:02X}",
        (c.red * 255.0).round() as u8,
        (c.green * 255.0).round() as u8,
        (c.blue * 255.0).round() as u8
    )
}

/// Set a ColorPicker's color; updates the swatch (and the hex entry unless
/// the change came from typing in it).
pub fn color_picker_set(world: &mut World, picker: Entity, color: Color, update_hex: bool) {
    use crate::components::PfColorPicker;
    let Some(mut p) = world.get_mut::<PfColorPicker>(picker) else {
        return;
    };
    p.selected = color;
    let (swatch, hex_input) = (p.swatch, p.hex_input);
    if let Some(mut bg) = world.get_mut::<BackgroundColor>(swatch) {
        bg.0 = color;
    }
    if update_hex && let Some(mut et) = world.get_mut::<bevy::text::EditableText>(hex_input) {
        et.editor.set_text(&color_to_hex(color));
    }
}

/// Refilter an AutoSuggestBox's dropdown against its current text.
pub(crate) fn rebuild_suggestions(world: &mut World, owner: Entity) {
    use crate::components::PfAutoSuggestBox;
    use crate::overlay::PfPopup;

    let Some(sugg) = world.get::<PfAutoSuggestBox>(owner).cloned() else {
        return;
    };
    let text = world
        .get::<bevy::text::EditableText>(sugg.input)
        .map(|et| et.editor().text().to_string())
        .unwrap_or_default();
    let needle = text.trim().to_lowercase();
    let matches: Vec<String> = if needle.is_empty() {
        Vec::new()
    } else {
        sugg.items
            .iter()
            .filter(|item| {
                let lower = item.to_lowercase();
                lower.starts_with(&needle) && lower != needle
            })
            .take(8)
            .cloned()
            .collect()
    };

    let old: Vec<Entity> = world
        .get::<Children>(sugg.popup)
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    for row in old {
        world.entity_mut(row).despawn();
    }
    let open = !matches.is_empty();
    for item in matches {
        let label = world
            .spawn((
                Node::default(),
                bevy::ui::widget::Text::new(&item),
                bevy::text::TextFont {
                    font_size: bevy::text::FontSize::Px(13.0),
                    font: crate::fonts::default_font(),
                    ..Default::default()
                },
                bevy::text::TextColor(Color::srgb_u8(0x1A, 0x1A, 0x1A)),
            ))
            .id();
        let row = world
            .spawn((
                Node {
                    padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                    ..Default::default()
                },
                BackgroundColor(Color::NONE),
                Interaction::default(),
            ))
            .id();
        world.entity_mut(row).add_children(&[label]);
        let (input, popup) = (sugg.input, sugg.popup);
        world
            .entity_mut(row)
            .observe(move |_click: On<Pointer<Click>>, mut commands: Commands| {
                let chosen = item.clone();
                commands.queue(move |world: &mut World| {
                    if let Some(mut et) = world.get_mut::<bevy::text::EditableText>(input) {
                        et.editor.set_text(&chosen);
                    }
                    if let Some(mut p) = world.get_mut::<PfPopup>(popup) {
                        p.open = false;
                    }
                });
            });
        world.entity_mut(sugg.popup).add_children(&[row]);
    }
    if let Some(mut p) = world.get_mut::<PfPopup>(sugg.popup) {
        p.open = open;
    }
}

/// Select a NavigationView item: highlight it and navigate the embedded frame.
pub fn nav_view_invoke(world: &mut World, view: Entity, index: usize, route: &str) {
    use crate::components::PfNavigationView;
    let Some(mut nav) = world.get_mut::<PfNavigationView>(view) else {
        return;
    };
    nav.selected = Some(index);
    let (frame, items) = (nav.frame, nav.items.clone());
    for (i, row) in items.into_iter().enumerate() {
        if let Some(mut bg) = world.get_mut::<BackgroundColor>(row) {
            bg.0 = if i == index {
                Color::srgb_u8(0xDD, 0xE6, 0xF5)
            } else {
                Color::NONE
            };
        }
    }
    crate::navigation::navigate(world, frame, route);
}

/// Select a date on a Calendar; reports to the owning DatePicker if any.
pub fn calendar_select(world: &mut World, calendar: Entity, year: i32, month: u32, day: u32) {
    use crate::components::{PfCalendar, PfDatePicker};
    use crate::overlay::PfPopup;

    let owner = {
        let Some(mut state) = world.get_mut::<PfCalendar>(calendar) else {
            return;
        };
        state.selected = Some((year, month, day));
        state.owner_picker
    };
    calendar_rebuild(world, calendar);

    if let Some(picker) = owner {
        let (display, popup) = {
            let Some(mut p) = world.get_mut::<PfDatePicker>(picker) else {
                return;
            };
            p.selected = Some((year, month, day));
            (p.display, p.popup)
        };
        set_text(world, display, format!("{year:04}-{month:02}-{day:02}"));
        if let Some(mut pop) = world.get_mut::<PfPopup>(popup) {
            pop.open = false;
        }
    }
}

/// Resize the two grid tracks around a GridSplitter by a drag delta.
pub fn splitter_drag(world: &mut World, splitter: Entity, delta: Vec2) {
    use crate::components::{PfAttachedProps, PfGridSplitter};

    let Some(config) = world.get::<PfGridSplitter>(splitter).cloned() else {
        return;
    };
    let Some(grid) = world.get::<ChildOf>(splitter).map(|c| c.parent()) else {
        return;
    };
    let (owner, prop, axis_delta) = if config.columns {
        ("Grid", "Column", delta.x)
    } else {
        ("Grid", "Row", delta.y)
    };
    let Some(index) = world
        .get::<PfAttachedProps>(splitter)
        .and_then(|a| a.parse::<usize>(owner, prop))
    else {
        return;
    };
    if index == 0 {
        return;
    }

    // Current sizes of the neighbor cells, from any child occupying them.
    let mut before_size: Option<f32> = None;
    let mut after_size: Option<f32> = None;
    let children: Vec<Entity> = world
        .get::<Children>(grid)
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    for child in children {
        let Some(attached) = world.get::<PfAttachedProps>(child) else {
            continue;
        };
        let Some(i) = attached.parse::<usize>(owner, prop) else {
            continue;
        };
        let Some(computed) = world.get::<bevy::ui::ComputedNode>(child) else {
            continue;
        };
        let size = computed.size() * computed.inverse_scale_factor();
        let logical = if config.columns { size.x } else { size.y };
        if i == index - 1 {
            before_size.get_or_insert(logical);
        } else if i == index + 1 {
            after_size.get_or_insert(logical);
        }
    }
    let (Some(before), Some(after)) = (before_size, after_size) else {
        return;
    };
    let new_before = (before + axis_delta).max(24.0);
    let new_after = (after - axis_delta).max(24.0);

    let Some(mut node) = world.get_mut::<Node>(grid) else {
        return;
    };
    let tracks = if config.columns {
        &mut node.grid_template_columns
    } else {
        &mut node.grid_template_rows
    };
    if index + 1 < tracks.len() + 1 && index >= 1 && tracks.len() > index {
        tracks[index - 1] = GridTrack::px(new_before);
        tracks[index + 1] = GridTrack::px(new_after);
    }
}
