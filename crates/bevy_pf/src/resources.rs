//! WPF-style resources: `ResourceDictionary` values, `Style` with setters,
//! and parsing of resource elements (`<SolidColorBrush x:Key="..."/>`,
//! `<Style TargetType="Button">`, `<x:Double x:Key="..">42</x:Double>`, ...).

use std::collections::HashMap;
use std::sync::Arc;

use bevy_pf_xaml::markup::MarkupValue;
use bevy_pf_xaml::value as v;
use bevy_pf_xaml::{XamlNode, XamlValue};

use crate::error::PfError;

/// Key of a resource in a dictionary: explicit `x:Key`, or the target type of
/// an implicit style (`{x:Type Button}`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResourceKey {
    Explicit(String),
    Type(String),
}

/// A value in a resource dictionary.
#[derive(Debug, Clone)]
pub enum PfValue {
    String(String),
    Double(f64),
    Bool(bool),
    Color(v::PfColor),
    Brush(v::PfBrush),
    Thickness(v::Thickness),
    CornerRadius(v::CornerRadius),
    Geometry(bevy_pf_xaml::geometry::PathData),
    Transform(v::PfTransform),
    Style(Arc<PfStyle>),
    /// An unexpanded `DataTemplate` subtree, instantiated per item.
    Template(Arc<XamlNode>),
    /// An unexpanded `ControlTemplate`: replaces a control's default chrome.
    ControlTemplate(Arc<PfControlTemplate>),
    /// A parsed `<Storyboard>`, startable by EventTriggers or from Rust.
    Storyboard(Arc<crate::animation::PfStoryboard>),
}

/// A parsed WPF `ControlTemplate`: the visual root subtree plus template
/// triggers (whose setters may carry `TargetName`, unlike style triggers).
#[derive(Debug, Clone)]
pub struct PfControlTemplate {
    pub target_type: Option<String>,
    /// The single visual root child.
    pub root: Arc<XamlNode>,
    /// From `<ControlTemplate.Triggers>`.
    pub triggers: Vec<PfTrigger>,
    /// `<VisualStateManager.VisualStateGroups>` on the template root.
    pub state_groups: Vec<crate::animation::PfVisualStateGroup>,
}

/// A parsed WPF `Style`: target type plus setters and triggers (BasedOn is
/// already merged at parse time, since `{StaticResource}` requires
/// lexically-earlier definitions just like WPF; base triggers precede derived
/// ones, matching WPF's lookup-list build order).
#[derive(Debug, Clone, Default)]
pub struct PfStyle {
    pub target_type: Option<String>,
    /// Avalonia's `Selector="Button.danger"`. Present instead of
    /// `target_type` when the style came from a `Styles` collection.
    pub selector: Option<Arc<crate::selector::Sel>>,
    pub setters: Vec<PfSetter>,
    pub triggers: Vec<PfTrigger>,
    /// `<EventTrigger RoutedEvent=...>` storyboard launchers.
    pub event_triggers: Vec<crate::animation::PfEventTrigger>,
}

/// A `Style.Triggers` entry: all conditions must hold (one for `Trigger` /
/// `DataTrigger`, several for `MultiTrigger` / `MultiDataTrigger`).
#[derive(Debug, Clone)]
pub struct PfTrigger {
    pub conditions: Vec<PfTriggerCondition>,
    pub setters: Vec<PfSetter>,
    /// `Trigger.EnterActions` BeginStoryboards (run when the trigger
    /// activates).
    pub enter_storyboards: Vec<Arc<crate::animation::PfStoryboard>>,
    /// `Trigger.ExitActions` BeginStoryboards (run when it deactivates).
    pub exit_storyboards: Vec<Arc<crate::animation::PfStoryboard>>,
}

#[derive(Debug, Clone)]
pub enum PfTriggerCondition {
    /// `<Trigger Property="IsMouseOver" Value="True">`
    Property {
        property: String,
        value: PfTriggerValue,
    },
    /// `<DataTrigger Binding="{Binding Path}" Value="...">`
    Data { path: String, value: PfTriggerValue },
}

/// A trigger condition's expected value. `Null` is parse-level support for
/// `Value="{x:Null}"` (e.g. indeterminate `IsChecked`); evaluation of
/// three-state conditions is not implemented — such triggers warn and skip
/// at attach time.
#[derive(Debug, Clone, PartialEq)]
pub enum PfTriggerValue {
    Text(String),
    Null,
}

#[derive(Debug, Clone)]
pub struct PfSetter {
    /// `Some("DockPanel")` for attached setters like `Property="DockPanel.Dock"`.
    pub owner: Option<String>,
    pub property: String,
    pub value: PfSetterValue,
    /// Template-trigger setters may target a named template element
    /// (`TargetName="border"`); always `None` outside ControlTemplates.
    pub target_name: Option<String>,
}

#[derive(Debug, Clone)]
pub enum PfSetterValue {
    /// A literal string, converted per-property at application time.
    Literal(String),
    /// `Value="{StaticResource key}"` — resolved at application time.
    Resource(ResourceKey),
    /// `Value="{DynamicResource key}"` — resolved at application time and
    /// re-resolved when dictionaries change.
    DynamicResource(ResourceKey),
    /// `Value="{x:Null}"` — clears the property (applied as a no-op for
    /// properties without null semantics).
    Null,
    /// A structured value from property-element syntax (e.g. a gradient brush).
    Value(PfValue),
    /// The named property read off the TEMPLATED PARENT, from either
    /// `Value="{TemplateBinding Prop}"` (MAUI's spelling) or
    /// `Value="{Binding Prop, RelativeSource={RelativeSource TemplatedParent}}"`
    /// (WPF's, which is what WPF requires inside `ControlTemplate.Triggers`
    /// because it rejects its own `TemplateBinding` there).
    ///
    /// Both dialects mean the same thing, so both parse to this. It is only
    /// meaningful inside a `ControlTemplate`: a Style setter that uses it has
    /// no templated parent to read, and is reported rather than silently
    /// painting nothing.
    TemplatedParent(String),
    /// `Value="{AppThemeBinding Light=a, Dark=b}"` — re-picked whenever the
    /// app theme changes. Kept in raw form because a setter is parsed with no
    /// entity and no property target to convert against.
    AppTheme(RawThemeArms),
    /// `Value="{OnPlatform ...}"` / `Value="{OnIdiom ...}"` — resolved once
    /// when the setter is applied, because neither can change at runtime.
    Selector(crate::device::SelectorKind, RawSelectorArms),
}

pub type ResourceDictionary = HashMap<ResourceKey, PfValue>;

/// A stack of lexically-scoped resource dictionaries (innermost last), with
/// the application-level dictionary as the final fallback tier (mirroring
/// WPF's element tree -> `Application.Resources` lookup order).
#[derive(Debug, Default, Clone)]
pub struct ResourceScopes {
    stack: Vec<Arc<ResourceDictionary>>,
    /// Snapshot of the application resources for this instantiation.
    pub app: Option<Arc<ResourceDictionary>>,
}

impl ResourceScopes {
    pub fn push(&mut self, dict: Arc<ResourceDictionary>) {
        self.stack.push(dict);
    }

    pub fn pop(&mut self) {
        self.stack.pop();
    }

    pub fn lookup(&self, key: &ResourceKey) -> Option<&PfValue> {
        self.stack
            .iter()
            .rev()
            .find_map(|d| d.get(key))
            .or_else(|| self.app.as_ref().and_then(|d| d.get(key)))
    }

    pub fn lookup_str(&self, key: &str) -> Option<&PfValue> {
        self.lookup(&ResourceKey::Explicit(key.to_string()))
    }

    /// Look up the implicit style for a XAML type name.
    pub fn implicit_style(&self, type_name: &str) -> Option<Arc<PfStyle>> {
        match self.lookup(&ResourceKey::Type(type_name.to_string())) {
            Some(PfValue::Style(s)) => Some(s.clone()),
            _ => None,
        }
    }

    /// Temporarily remove the lexical stack (keeping the app tier), for
    /// contexts that must not see the consumer's scopes (merged dictionary
    /// files, WPF semantics). Pair with [`Self::restore_stack`].
    pub fn isolate_stack(&mut self) -> Vec<Arc<ResourceDictionary>> {
        std::mem::take(&mut self.stack)
    }

    pub fn restore_stack(&mut self, stack: Vec<Arc<ResourceDictionary>>) {
        self.stack = stack;
    }
}

/// Extract the resource key from a `{StaticResource ...}` positional argument,
/// handling both plain string keys and `{x:Type Button}` style keys.
pub fn static_resource_key(ext: &bevy_pf_xaml::MarkupExtension) -> Result<ResourceKey, PfError> {
    match ext.positional.first() {
        Some(MarkupValue::Str(s)) => Ok(ResourceKey::Explicit(s.clone())),
        Some(MarkupValue::Extension(inner)) if inner.name == "x:Type" || inner.name == "Type" => {
            let ty = inner
                .first_positional_str()
                .ok_or_else(|| PfError::resource("x:Type missing a type name"))?;
            // Strip any namespace prefix ("local:Foo" -> "Foo").
            let ty = ty.rsplit(':').next().unwrap_or(ty);
            Ok(ResourceKey::Type(ty.to_string()))
        }
        _ => Err(PfError::resource(format!("unsupported {} key", ext.name))),
    }
}

/// One arm of an `{AppThemeBinding}` as written, before it is known which
/// property it targets (a setter is parsed with no entity and no target).
#[derive(Debug, Clone)]
pub enum RawArm {
    Text(String),
    Static(ResourceKey),
    Dynamic(ResourceKey),
    /// `{x:Null}` — SUPPLIED, and therefore stops the fallback to `Default`.
    Null,
}

/// The Light/Dark/Default arms as written. `None` means the arm is absent.
#[derive(Debug, Clone, Default)]
pub struct RawThemeArms {
    pub light: Option<RawArm>,
    pub dark: Option<RawArm>,
    pub default: Option<RawArm>,
}

impl RawThemeArms {
    /// The arm MAUI would pick: Dark under a dark theme, Light otherwise
    /// (`Unspecified` included), each falling back to `Default` and no
    /// further.
    pub fn pick(&self, theme: crate::app_theme::AppTheme) -> Option<&RawArm> {
        match theme {
            crate::app_theme::AppTheme::Dark => self.dark.as_ref().or(self.default.as_ref()),
            _ => self.light.as_ref().or(self.default.as_ref()),
        }
    }
}

/// The arms of an `{OnPlatform}` / `{OnIdiom}`: named arms in source order,
/// plus the `Default` that MAUI makes the ContentProperty.
#[derive(Debug, Clone, Default)]
pub struct RawSelectorArms {
    pub arms: Vec<(String, RawArm)>,
    pub default: Option<RawArm>,
}

impl RawSelectorArms {
    /// The arm describing this device, else `Default`.
    ///
    /// Source order decides only among arms that BOTH match, which happens
    /// for the one alias pair (`macOS` / `MacCatalyst`); the caller orders
    /// the names so `macOS` is offered first.
    pub fn pick(&self, device: &crate::device::PfDevice, kind: crate::device::SelectorKind)
    -> Option<&RawArm> {
        self.arms
            .iter()
            .find(|(name, _)| device.matches(kind, name))
            .map(|(_, arm)| arm)
            .or(self.default.as_ref())
    }
}

/// Parse `{OnPlatform Android=a, iOS=b, Default=c}` or the `{OnIdiom}` twin.
///
/// Arm names are validated against the selector's own set, so a typo is
/// reported with the alternatives instead of quietly resolving to `Default`
/// on every platform — the failure mode that makes these extensions hard to
/// debug. MAUI rejects an extension in which nothing at all is written; this
/// mirrors that, as a warning rather than a throw.
pub fn parse_selector_arms(
    ext: &bevy_pf_xaml::MarkupExtension,
    kind: crate::device::SelectorKind,
) -> Result<RawSelectorArms, PfError> {
    let mut arms = RawSelectorArms::default();
    for (key, value) in &ext.named {
        if key == "Default" {
            arms.default = Some(theme_arm(value, kind.extension_name())?);
            continue;
        }
        // Converter/ConverterParameter exist on MAUI's extensions; bevy_pf
        // has no per-value converter here, so say so rather than ignore it.
        if !kind.arm_names().contains(&key.as_str()) {
            return Err(PfError::resource(format!(
                "{} has no `{key}` argument; expected one of {} or Default",
                kind.extension_name(),
                kind.arm_names().join(", ")
            )));
        }
        arms.arms
            .push((key.clone(), theme_arm(value, kind.extension_name())?));
    }
    if let Some(positional) = ext.positional.first() {
        if arms.default.is_some() {
            return Err(PfError::resource(format!(
                "{} got Default both positionally and by name",
                kind.extension_name()
            )));
        }
        arms.default = Some(theme_arm(positional, kind.extension_name())?);
    }

    let holds_a_value = arms
        .arms
        .iter()
        .map(|(_, a)| a)
        .chain(arms.default.iter())
        .any(|a| !matches!(a, RawArm::Null));
    if !holds_a_value {
        // MAUI's own wording.
        return Err(PfError::resource(format!(
            "{} requires a value to be specified for at least one {} or Default",
            kind.extension_name(),
            match kind {
                crate::device::SelectorKind::Platform => "platform",
                crate::device::SelectorKind::Idiom => "idiom",
            }
        )));
    }
    // Order the arms so the one alias pair resolves the way the docs say.
    arms.arms
        .sort_by_key(|(name, _)| usize::from(name == "MacCatalyst"));
    Ok(arms)
}

/// One arm of a selector-style extension.
fn theme_arm(value: &MarkupValue, owner: &str) -> Result<RawArm, PfError> {
    match value {
        MarkupValue::Str(s) => Ok(RawArm::Text(s.clone())),
        MarkupValue::Extension(inner) => match inner.name.as_str() {
            "StaticResource" => Ok(RawArm::Static(static_resource_key(inner)?)),
            "DynamicResource" => Ok(RawArm::Dynamic(static_resource_key(inner)?)),
            "x:Null" | "Null" => Ok(RawArm::Null),
            other => Err(PfError::resource(format!(
                "{owner} arm `{{{other}}}` is not supported"
            ))),
        },
    }
}

/// Parse `{AppThemeBinding Light=a, Dark=b, Default=c}`.
///
/// `Default` is the extension's ContentProperty in MAUI, so a lone positional
/// argument sets it: `{AppThemeBinding Green}` is `Default=Green`.
///
/// MAUI rejects an extension in which no arm holds a NON-NULL value
/// (`AppThemeBindingExtension.cs`: the guard tests the values, not which keys
/// were written), so `{AppThemeBinding}` and `{AppThemeBinding Light={x:Null}}`
/// are both errors. MAUI throws there; bevy_pf has no throw idiom, so this is
/// an `Err` that the caller surfaces as a warning and skips.
pub fn parse_app_theme_arms(
    ext: &bevy_pf_xaml::MarkupExtension,
) -> Result<RawThemeArms, PfError> {
    let arm = |value: &MarkupValue| theme_arm(value, "AppThemeBinding");

    let mut arms = RawThemeArms::default();
    for (key, value) in &ext.named {
        match key.as_str() {
            "Light" => arms.light = Some(arm(value)?),
            "Dark" => arms.dark = Some(arm(value)?),
            "Default" => arms.default = Some(arm(value)?),
            other => {
                return Err(PfError::resource(format!(
                    "AppThemeBinding has no `{other}` argument"
                )));
            }
        }
    }
    if let Some(positional) = ext.positional.first() {
        if arms.default.is_some() {
            return Err(PfError::resource(
                "AppThemeBinding got Default both positionally and by name",
            ));
        }
        arms.default = Some(arm(positional)?);
    }

    let holds_a_value = [&arms.light, &arms.dark, &arms.default]
        .into_iter()
        .any(|a| matches!(a, Some(a) if !matches!(a, RawArm::Null)));
    if !holds_a_value {
        return Err(PfError::resource(
            "AppThemeBinding requires a non-null value for at least one theme or Default",
        ));
    }
    Ok(arms)
}

/// Parse the contents of a `Resources` property element into a dictionary.
/// `scopes` provides lexically-earlier resources for `BasedOn` / nested
/// `{StaticResource}` resolution.
/// NOTE on failure semantics (verified 2026-08-05, after a self-inflicted
/// outage): `parse_resource_entries_into` below is FAIL-SOFT — a resource
/// that fails to parse is warned about and skipped, and the rest of the
/// dictionary still loads. That is the right behaviour and it works.
///
/// What is NOT verified is what happens when the XML *around* a resource
/// dictionary is mis-parsed — e.g. an element in a screen file gaining
/// children it did not have. In one incident every style referenced by
/// `access_code.xaml` came back "not found" while `controls.xaml` itself was
/// fine, which points at the SCREEN's `Grid.Resources` scope failing rather
/// than the dictionary parser. Diagnose there first if styles vanish
/// wholesale; the dictionary loop is not the suspect it looks like.

pub fn parse_resource_dictionary<'a>(
    nodes: impl Iterator<Item = &'a XamlNode>,
    scopes: &ResourceScopes,
    warnings: &mut Vec<String>,
) -> ResourceDictionary {
    let mut dict = ResourceDictionary::new();
    parse_resource_entries_into(nodes, scopes, &mut dict, warnings);
    dict
}

/// Parse resource entries, merging into an existing dictionary (own entries
/// after merged dictionaries, so they win on key conflicts, like WPF).
pub fn parse_resource_entries_into<'a>(
    nodes: impl Iterator<Item = &'a XamlNode>,
    scopes: &ResourceScopes,
    dict: &mut ResourceDictionary,
    warnings: &mut Vec<String>,
) {
    for node in nodes {
        match parse_resource_entry(node, scopes, dict, warnings) {
            Ok(Some((key, value))) => {
                dict.insert(key, value);
            }
            Ok(None) => {}
            Err(e) => warnings.push(format!(
                "{}: skipped resource `{}`: {e}",
                node.pos, node.name
            )),
        }
    }
}

fn parse_resource_entry(
    node: &XamlNode,
    scopes: &ResourceScopes,
    local: &ResourceDictionary,
    warnings: &mut Vec<String>,
) -> Result<Option<(ResourceKey, PfValue)>, PfError> {
    let value = match parse_resource_value(node, scopes, local, warnings)? {
        Some(v) => v,
        None => return Ok(None),
    };

    let key = if let Some(k) = &node.x_key {
        // `x:Key="{x:Type TextBoxBase}"` keys the entry by TYPE so it meets
        // `{StaticResource {x:Type ...}}` lookups (which resolve to
        // ResourceKey::Type via static_resource_key).
        let trimmed = k.trim();
        if let Some(inner) = trimmed
            .strip_prefix('{')
            .and_then(|r| r.strip_suffix('}'))
            .map(str::trim)
            .filter(|r| r.starts_with("x:Type ") || r.starts_with("Type "))
        {
            let ty = inner.split_whitespace().nth(1).unwrap_or_default();
            let ty = ty.rsplit(':').next().unwrap_or(ty);
            if ty.is_empty() {
                return Err(PfError::resource("x:Key {x:Type} needs a type name"));
            }
            ResourceKey::Type(ty.to_string())
        } else if trimmed.starts_with('{') {
            warnings.push(format!(
                "{}: markup-extension x:Key `{k}` is not supported (only {{x:Type ...}}); used literally",
                node.pos
            ));
            ResourceKey::Explicit(k.clone())
        } else {
            ResourceKey::Explicit(k.clone())
        }
    } else if let PfValue::Style(style) = &value {
        match &style.target_type {
            Some(t) => ResourceKey::Type(t.clone()),
            None => return Err(PfError::resource("Style needs x:Key or TargetType")),
        }
    } else if let PfValue::Template(_) = &value {
        // DataType-implicit DataTemplate: keyed by the view-model's type
        // ident, matched by the ContentControl runtime selection.
        let ty = match node.attribute("DataType") {
            Some(XamlValue::Str(s)) => Some(s.as_str()),
            Some(XamlValue::Extension(ext)) if ext.name == "x:Type" || ext.name == "Type" => {
                ext.first_positional_str()
            }
            _ => None,
        };
        match ty {
            Some(t) => {
                let t = t.rsplit(':').next().unwrap_or(t);
                ResourceKey::Type(t.to_string())
            }
            None => return Err(PfError::resource("DataTemplate needs x:Key or DataType")),
        }
    } else {
        return Err(PfError::resource("resource needs x:Key"));
    };
    Ok(Some((key, value)))
}

/// Parse a single resource element into a [`PfValue`].
pub fn parse_resource_value(
    node: &XamlNode,
    scopes: &ResourceScopes,
    local: &ResourceDictionary,
    warnings: &mut Vec<String>,
) -> Result<Option<PfValue>, PfError> {
    let text = node.text_content().unwrap_or_default();
    let value = match node.name.as_str() {
        // x: primitives
        "Double" => PfValue::Double(
            text.trim()
                .parse()
                .map_err(|e| PfError::resource(format!("bad x:Double: {e}")))?,
        ),
        "String" => PfValue::String(text),
        "Boolean" => PfValue::Bool(text.trim().eq_ignore_ascii_case("true")),
        "Int32" | "Int64" => PfValue::Double(
            text.trim()
                .parse::<i64>()
                .map_err(|e| PfError::resource(format!("bad integer: {e}")))? as f64,
        ),

        "SolidColorBrush" => {
            let color = match node.attribute("Color") {
                Some(XamlValue::Str(s)) => s.parse::<v::PfColor>()?,
                Some(XamlValue::Extension(ext)) => resolve_color_resource(ext, scopes, local)?,
                None => text.trim().parse::<v::PfColor>()?,
            };
            PfValue::Brush(v::PfBrush::Solid(color))
        }
        "Color" => PfValue::Color(text.trim().parse()?),
        "Thickness" => PfValue::Thickness(text.trim().parse()?),
        "CornerRadius" => PfValue::CornerRadius(text.trim().parse()?),
        "LinearGradientBrush" => PfValue::Brush(parse_linear_gradient(node, scopes, local)?),
        "RadialGradientBrush" => PfValue::Brush(parse_radial_gradient(node, scopes, local)?),
        "Style" => PfValue::Style(Arc::new(parse_style(node, scopes, local, warnings)?)),
        "DataTemplate" => {
            let mut roots = node.child_elements();
            let root = roots
                .next()
                .ok_or_else(|| PfError::resource("DataTemplate needs a root element"))?;
            if roots.next().is_some() {
                return Err(PfError::resource("DataTemplate must have one root element"));
            }
            PfValue::Template(Arc::new(root.clone()))
        }
        "Storyboard" => PfValue::Storyboard(Arc::new(parse_storyboard(node, warnings)?)),
        "ControlTemplate" => {
            let target_type = match node.attribute("TargetType") {
                Some(XamlValue::Str(t)) => Some(t.rsplit(':').next().unwrap_or(t).to_string()),
                Some(XamlValue::Extension(ext)) if ext.name == "x:Type" || ext.name == "Type" => {
                    ext.first_positional_str()
                        .map(|t| t.rsplit(':').next().unwrap_or(t).to_string())
                }
                _ => None,
            };
            let mut roots = node.child_elements();
            let root = roots
                .next()
                .ok_or_else(|| PfError::resource("ControlTemplate needs a root element"))?;
            if roots.next().is_some() {
                return Err(PfError::resource(
                    "ControlTemplate must have one root element",
                ));
            }
            let mut triggers = Vec::new();
            if let Some(pe) = node.property_element("Triggers") {
                for trigger in pe.elements() {
                    match parse_trigger(trigger, scopes, local, warnings, true) {
                        Ok(Some(t)) => triggers.push(t),
                        Ok(None) => {}
                        Err(e) => {
                            warnings.push(format!("{}: skipped trigger: {e}", trigger.pos));
                        }
                    }
                }
            }
            // VSM groups live as an attached property element on the
            // template's visual root.
            let mut state_groups = Vec::new();
            if let Some(pe) = root
                .property_elements
                .iter()
                .find(|p| p.name == "VisualStateGroups")
            {
                for group in pe.elements() {
                    if group.name != "VisualStateGroup" {
                        continue;
                    }
                    let name = group
                        .x_name
                        .clone()
                        .unwrap_or_else(|| "CommonStates".to_string());
                    let mut states = Vec::new();
                    let mut transitions = Vec::new();
                    // Dotted form routes to property elements.
                    if let Some(tpe) = group
                        .property_elements
                        .iter()
                        .find(|p| p.name == "Transitions")
                    {
                        for t in tpe.elements() {
                            if t.name == "VisualTransition" {
                                transitions.push(parse_visual_transition(t, warnings));
                            }
                        }
                    }
                    for state in group.child_elements() {
                        match state.name.as_str() {
                            "VisualState" => {
                                let storyboard = state
                                    .child_elements()
                                    .find(|c| c.name == "Storyboard")
                                    .map(|sb| parse_storyboard(sb, warnings))
                                    .transpose()?
                                    .map(Arc::new);
                                states.push(crate::animation::PfVisualState {
                                    name: state.x_name.clone().unwrap_or_default(),
                                    storyboard,
                                });
                            }
                            "VisualStateGroup.Transitions" => {
                                for t in state.child_elements() {
                                    if t.name == "VisualTransition" {
                                        transitions.push(parse_visual_transition(t, warnings));
                                    }
                                }
                            }
                            "VisualTransition" => {
                                transitions.push(parse_visual_transition(state, warnings));
                            }
                            _ => {}
                        }
                    }
                    state_groups.push(crate::animation::PfVisualStateGroup {
                        name,
                        states,
                        transitions,
                    });
                }
            }
            PfValue::ControlTemplate(Arc::new(PfControlTemplate {
                target_type,
                root: Arc::new(root.clone()),
                triggers,
                state_groups,
            }))
        }
        // `<Geometry x:Key="X">M 0,0 ...</Geometry>` — mini-language text.
        "Geometry" => PfValue::Geometry(bevy_pf_xaml::geometry::parse_path_data(text.trim())?),
        "PathGeometry" | "StreamGeometry" | "EllipseGeometry" | "RectangleGeometry"
        | "LineGeometry" | "GeometryGroup" => {
            PfValue::Geometry(parse_geometry_element(node, warnings)?)
        }
        "TranslateTransform" | "ScaleTransform" | "RotateTransform" | "TransformGroup" => {
            PfValue::Transform(parse_transform_element(node, warnings)?)
        }
        other => {
            warnings.push(format!(
                "{}: resource type `{other}` is not supported yet",
                node.pos
            ));
            return Ok(None);
        }
    };
    Ok(Some(value))
}

/// Resolve a `Color="{StaticResource ...}"` attribute.
fn resolve_color_resource(
    ext: &bevy_pf_xaml::MarkupExtension,
    scopes: &ResourceScopes,
    local: &ResourceDictionary,
) -> Result<v::PfColor, PfError> {
    if ext.name != "StaticResource" && ext.name != "DynamicResource" {
        return Err(PfError::resource(format!(
            "Color extension `{{{}}}` not supported",
            ext.name
        )));
    }
    let key = static_resource_key(ext)?;
    match local.get(&key).or_else(|| scopes.lookup(&key)) {
        Some(PfValue::Color(c)) => Ok(*c),
        Some(PfValue::Brush(v::PfBrush::Solid(c))) => Ok(*c),
        Some(PfValue::String(s)) => Ok(s.parse()?),
        Some(_) => Err(PfError::resource("resource is not a color")),
        None => Err(PfError::resource(format!(
            "color resource `{key:?}` not found"
        ))),
    }
}

fn attr_parse<T: std::str::FromStr<Err = bevy_pf_xaml::XamlError>>(
    node: &XamlNode,
    name: &str,
) -> Result<Option<T>, PfError> {
    match node.attribute(name) {
        Some(XamlValue::Str(s)) => Ok(Some(s.parse::<T>()?)),
        _ => Ok(None),
    }
}

fn parse_gradient_stops(
    node: &XamlNode,
    scopes: &ResourceScopes,
    local: &ResourceDictionary,
) -> Result<Vec<v::GradientStop>, PfError> {
    // Stops come either as direct content children (content property) or via
    // <LinearGradientBrush.GradientStops>.
    let mut stops = Vec::new();
    let from_property = node
        .property_element("GradientStops")
        .map(|p| p.elements().collect::<Vec<_>>());
    let elems: Vec<&XamlNode> = match &from_property {
        Some(elems) => elems.clone(),
        None => node.child_elements().collect(),
    };
    for stop in elems {
        // Tolerate an explicit <GradientStopCollection> wrapper.
        let items: Vec<&XamlNode> = if stop.name == "GradientStopCollection" {
            stop.child_elements().collect()
        } else {
            vec![stop]
        };
        for item in items {
            if item.name != "GradientStop" {
                continue;
            }
            let color: v::PfColor = match item.attribute("Color") {
                Some(XamlValue::Str(s)) => s.parse()?,
                Some(XamlValue::Extension(ext)) => resolve_color_resource(ext, scopes, local)?,
                None => return Err(PfError::resource("GradientStop needs Color")),
            };
            let offset: f64 = match item.attribute("Offset") {
                Some(XamlValue::Str(s)) => s.trim().parse().unwrap_or(0.0),
                _ => 0.0,
            };
            stops.push(v::GradientStop {
                color,
                offset: offset as f32,
            });
        }
    }
    if stops.is_empty() {
        return Err(PfError::resource("gradient brush has no stops"));
    }
    Ok(stops)
}

fn parse_linear_gradient(
    node: &XamlNode,
    scopes: &ResourceScopes,
    local: &ResourceDictionary,
) -> Result<v::PfBrush, PfError> {
    let start: v::Point = attr_parse(node, "StartPoint")?.unwrap_or(v::Point::new(0.0, 0.0));
    let end: v::Point = attr_parse(node, "EndPoint")?.unwrap_or(v::Point::new(1.0, 1.0));
    Ok(v::PfBrush::LinearGradient {
        start,
        end,
        stops: parse_gradient_stops(node, scopes, local)?,
    })
}

fn parse_radial_gradient(
    node: &XamlNode,
    scopes: &ResourceScopes,
    local: &ResourceDictionary,
) -> Result<v::PfBrush, PfError> {
    let center: v::Point = attr_parse(node, "Center")?.unwrap_or(v::Point::new(0.5, 0.5));
    let radius_x = match node.attribute("RadiusX") {
        Some(XamlValue::Str(s)) => s.trim().parse().unwrap_or(0.5),
        _ => 0.5,
    };
    let radius_y = match node.attribute("RadiusY") {
        Some(XamlValue::Str(s)) => s.trim().parse().unwrap_or(0.5),
        _ => 0.5,
    };
    Ok(v::PfBrush::RadialGradient {
        center,
        radius_x,
        radius_y,
        stops: parse_gradient_stops(node, scopes, local)?,
    })
}

/// Parse a geometry element (`PathGeometry`, `StreamGeometry`,
/// `EllipseGeometry`, `RectangleGeometry`, `LineGeometry`, `GeometryGroup`)
/// into path data.
pub fn parse_geometry_element(
    node: &XamlNode,
    warnings: &mut Vec<String>,
) -> Result<bevy_pf_xaml::geometry::PathData, PfError> {
    use bevy_pf_xaml::geometry::parse_path_data;
    let data = match node.name.as_str() {
        "StreamGeometry" => {
            let text = node.text_content().unwrap_or_default();
            parse_path_data(text.trim())?
        }
        "PathGeometry" => {
            if let Some(XamlValue::Str(figures)) = node.attribute("Figures") {
                parse_path_data(figures)?
            } else {
                parse_structured_path(node, warnings)?
            }
        }
        "LineGeometry" => {
            let start: v::Point =
                attr_parse(node, "StartPoint")?.unwrap_or(v::Point::new(0.0, 0.0));
            let end: v::Point = attr_parse(node, "EndPoint")?.unwrap_or(v::Point::new(0.0, 0.0));
            parse_path_data(&format!("M {},{} L {},{}", start.x, start.y, end.x, end.y))?
        }
        "RectangleGeometry" => {
            let rect: v::Rect = attr_parse(node, "Rect")?.unwrap_or_default();
            parse_path_data(&format!(
                "M {},{} L {},{} L {},{} L {},{} Z",
                rect.x,
                rect.y,
                rect.x + rect.width,
                rect.y,
                rect.x + rect.width,
                rect.y + rect.height,
                rect.x,
                rect.y + rect.height,
            ))?
        }
        "EllipseGeometry" => {
            let center: v::Point = attr_parse(node, "Center")?.unwrap_or(v::Point::new(0.0, 0.0));
            let rx = match node.attribute("RadiusX") {
                Some(XamlValue::Str(s)) => s.trim().parse().unwrap_or(0.0),
                _ => 0.0,
            };
            let ry = match node.attribute("RadiusY") {
                Some(XamlValue::Str(s)) => s.trim().parse().unwrap_or(0.0),
                _ => 0.0,
            };
            parse_path_data(&format!(
                "M {},{} A {rx},{ry} 0 1 0 {},{} A {rx},{ry} 0 1 0 {},{} Z",
                center.x - rx,
                center.y,
                center.x + rx,
                center.y,
                center.x - rx,
                center.y,
            ))?
        }
        "GeometryGroup" => {
            let mut merged = bevy_pf_xaml::geometry::PathData::default();
            if let Some(XamlValue::Str(rule)) = node.attribute("FillRule")
                && rule.eq_ignore_ascii_case("nonzero")
            {
                merged.fill_rule = bevy_pf_xaml::geometry::FillRule::NonZero;
            }
            let children_pe = node
                .property_element("Children")
                .map(|p| p.elements().collect::<Vec<_>>());
            let children: Vec<&XamlNode> = match &children_pe {
                Some(elems) => elems.clone(),
                None => node.child_elements().collect(),
            };
            for child in children {
                match parse_geometry_element(child, warnings) {
                    Ok(sub) => merged.figures.extend(sub.figures),
                    Err(e) => warnings.push(format!("{}: {e}", child.pos)),
                }
            }
            merged
        }
        other => {
            return Err(PfError::resource(format!(
                "geometry `{other}` is not supported yet"
            )));
        }
    };
    Ok(data)
}

/// Parse a `<PathGeometry>` with structured `<PathFigure>` children.
fn parse_structured_path(
    node: &XamlNode,
    warnings: &mut Vec<String>,
) -> Result<bevy_pf_xaml::geometry::PathData, PfError> {
    use bevy_pf_xaml::geometry::{PathData, PathFigure, PathSegment};

    let mut data = PathData::default();
    if let Some(XamlValue::Str(rule)) = node.attribute("FillRule")
        && rule.eq_ignore_ascii_case("nonzero")
    {
        data.fill_rule = bevy_pf_xaml::geometry::FillRule::NonZero;
    }

    let figures_pe = node
        .property_element("Figures")
        .map(|p| p.elements().collect::<Vec<_>>());
    let figures: Vec<&XamlNode> = match &figures_pe {
        Some(elems) => elems.clone(),
        None => node.child_elements().collect(),
    };

    for fig in figures {
        if fig.name != "PathFigure" {
            warnings.push(format!(
                "{}: unexpected `{}` in PathGeometry",
                fig.pos, fig.name
            ));
            continue;
        }
        let start: v::Point = attr_parse(fig, "StartPoint")?.unwrap_or_default();
        let closed = matches!(
            fig.attribute("IsClosed"),
            Some(XamlValue::Str(s)) if s.eq_ignore_ascii_case("true")
        );
        let mut figure = PathFigure {
            start,
            segments: Vec::new(),
            closed,
        };

        let segs_pe = fig
            .property_element("Segments")
            .map(|p| p.elements().collect::<Vec<_>>());
        let segs: Vec<&XamlNode> = match &segs_pe {
            Some(elems) => elems.clone(),
            None => fig.child_elements().collect(),
        };

        for seg in segs {
            let point = |name: &str| -> Result<v::Point, PfError> {
                attr_parse::<v::Point>(seg, name)?
                    .ok_or_else(|| PfError::resource(format!("{} needs {name}", seg.name)))
            };
            let points = || -> Result<Vec<v::Point>, PfError> {
                match seg.attribute("Points") {
                    Some(XamlValue::Str(s)) => Ok(v::parse_points(s)?),
                    _ => Err(PfError::resource(format!("{} needs Points", seg.name))),
                }
            };
            match seg.name.as_str() {
                "LineSegment" => figure.segments.push(PathSegment::Line(point("Point")?)),
                "PolyLineSegment" => {
                    for p in points()? {
                        figure.segments.push(PathSegment::Line(p));
                    }
                }
                "BezierSegment" => figure.segments.push(PathSegment::Cubic(
                    point("Point1")?,
                    point("Point2")?,
                    point("Point3")?,
                )),
                "PolyBezierSegment" => {
                    for chunk in points()?.chunks_exact(3) {
                        figure
                            .segments
                            .push(PathSegment::Cubic(chunk[0], chunk[1], chunk[2]));
                    }
                }
                "QuadraticBezierSegment" => figure
                    .segments
                    .push(PathSegment::Quadratic(point("Point1")?, point("Point2")?)),
                "PolyQuadraticBezierSegment" => {
                    for chunk in points()?.chunks_exact(2) {
                        figure
                            .segments
                            .push(PathSegment::Quadratic(chunk[0], chunk[1]));
                    }
                }
                "ArcSegment" => {
                    let size: v::Size = attr_parse(seg, "Size")?.unwrap_or_default();
                    let rotation = match seg.attribute("RotationAngle") {
                        Some(XamlValue::Str(s)) => s.trim().parse().unwrap_or(0.0),
                        _ => 0.0,
                    };
                    let large_arc = matches!(
                        seg.attribute("IsLargeArc"),
                        Some(XamlValue::Str(s)) if s.eq_ignore_ascii_case("true")
                    );
                    // WPF SweepDirection: Clockwise == SVG sweep flag 1.
                    let sweep = matches!(
                        seg.attribute("SweepDirection"),
                        Some(XamlValue::Str(s)) if s.eq_ignore_ascii_case("clockwise")
                    );
                    figure.segments.push(PathSegment::Arc {
                        radii: v::Point::new(size.width, size.height),
                        rotation,
                        large_arc,
                        sweep,
                        to: point("Point")?,
                    });
                }
                other => warnings.push(format!(
                    "{}: path segment `{other}` is not supported yet",
                    seg.pos
                )),
            }
        }
        data.figures.push(figure);
    }

    if data.figures.is_empty() {
        return Err(PfError::resource("PathGeometry has no figures"));
    }
    Ok(data)
}

/// Parse a transform element into a decomposed [`v::PfTransform`].
pub fn parse_transform_element(
    node: &XamlNode,
    warnings: &mut Vec<String>,
) -> Result<v::PfTransform, PfError> {
    let attr_f32 = |name: &str, default: f32| -> f32 {
        match node.attribute(name) {
            Some(XamlValue::Str(s)) => s.trim().parse().unwrap_or(default),
            _ => default,
        }
    };
    let t = match node.name.as_str() {
        "TranslateTransform" => v::PfTransform {
            translate_x: attr_f32("X", 0.0),
            translate_y: attr_f32("Y", 0.0),
            ..Default::default()
        },
        "ScaleTransform" => v::PfTransform {
            scale_x: attr_f32("ScaleX", 1.0),
            scale_y: attr_f32("ScaleY", 1.0),
            ..Default::default()
        },
        "RotateTransform" => v::PfTransform {
            rotate_deg: attr_f32("Angle", 0.0),
            ..Default::default()
        },
        // bevy_ui's UiTransform has no skew; accepted as identity so themes
        // that skew decorative elements still load.
        "SkewTransform" | "MatrixTransform" => v::PfTransform::default(),
        "TransformGroup" => {
            let children_pe = node
                .property_element("Children")
                .map(|p| p.elements().collect::<Vec<_>>());
            let children: Vec<&XamlNode> = match &children_pe {
                Some(elems) => elems.clone(),
                None => node.child_elements().collect(),
            };
            let mut combined = v::PfTransform::default();
            for child in children {
                match parse_transform_element(child, warnings) {
                    Ok(t) => combined = combined.compose(&t),
                    Err(e) => warnings.push(format!("{}: {e}", child.pos)),
                }
            }
            combined
        }
        other => {
            return Err(PfError::resource(format!(
                "transform `{other}` is not supported yet"
            )));
        }
    };
    Ok(t)
}

/// Parse a `<Style>` element: `TargetType`, `BasedOn`, and `<Setter>`s.
pub fn parse_style(
    node: &XamlNode,
    scopes: &ResourceScopes,
    local: &ResourceDictionary,
    warnings: &mut Vec<String>,
) -> Result<PfStyle, PfError> {
    let mut style = PfStyle::default();

    // Avalonia's Selector=. A malformed one is an ERROR rather than a style
    // that quietly matches nothing — the silent version is near-impossible
    // to diagnose from the running UI.
    if let Some(XamlValue::Str(src)) = node.attribute("Selector") {
        match crate::selector::parse(src) {
            Ok(sel) => style.selector = Some(Arc::new(sel)),
            Err(e) => return Err(PfError::resource(format!("{e}"))),
        }
    }

    // TargetType="Button" or TargetType="{x:Type Button}"
    match node.attribute("TargetType") {
        Some(XamlValue::Str(s)) => {
            let t = s.rsplit(':').next().unwrap_or(s);
            style.target_type = Some(t.to_string());
        }
        Some(XamlValue::Extension(ext)) if ext.name == "x:Type" || ext.name == "Type" => {
            if let Some(t) = ext.first_positional_str() {
                let t = t.rsplit(':').next().unwrap_or(t);
                style.target_type = Some(t.to_string());
            }
        }
        _ => {}
    }

    // BasedOn="{StaticResource ...}" — merge base setters first.
    if let Some(XamlValue::Extension(ext)) = node.attribute("BasedOn")
        && ext.name == "StaticResource"
    {
        let key = static_resource_key(ext)?;
        let base = local.get(&key).or_else(|| scopes.lookup(&key));
        match base {
            Some(PfValue::Style(base)) => {
                style.setters.extend(base.setters.iter().cloned());
                style.triggers.extend(base.triggers.iter().cloned());
                if style.target_type.is_none() {
                    style.target_type = base.target_type.clone();
                }
            }
            _ => warnings.push(format!(
                "{}: BasedOn resource not found (must be defined before use)",
                node.pos
            )),
        }
    }

    // Setters: direct children (content property) or <Style.Setters>.
    let from_property = node
        .property_element("Setters")
        .map(|p| p.elements().collect::<Vec<_>>());
    let setter_elems: Vec<&XamlNode> = match &from_property {
        Some(elems) => elems.clone(),
        None => node.child_elements().collect(),
    };

    for setter in setter_elems {
        match setter.name.as_str() {
            "Setter" => match parse_setter(setter, scopes, local, warnings) {
                Ok(s) => style.setters.push(s),
                Err(e) => warnings.push(format!("{}: skipped setter: {e}", setter.pos)),
            },
            "EventSetter" => {
                warnings.push(format!("{}: EventSetter is not supported yet", setter.pos))
            }
            other => warnings.push(format!(
                "{}: `{other}` in Style is not supported yet",
                setter.pos
            )),
        }
    }

    // Style.Triggers.
    if let Some(triggers_pe) = node.property_element("Triggers") {
        for trigger in triggers_pe.elements() {
            if trigger.name == "EventTrigger" {
                if let Some(et) = parse_event_trigger(trigger, scopes, local, warnings) {
                    style.event_triggers.push(et);
                }
                continue;
            }
            match parse_trigger(trigger, scopes, local, warnings, false) {
                Ok(Some(t)) => style.triggers.push(t),
                Ok(None) => {}
                Err(e) => warnings.push(format!("{}: skipped trigger: {e}", trigger.pos)),
            }
        }
    }

    Ok(style)
}

/// Parse one `Style.Triggers` entry. `Ok(None)` = recognized-but-unsupported
/// (warned).
/// Parse the keyframes of a `*AnimationUsingKeyFrames` element.
fn parse_key_frames(
    node: &XamlNode,
    color: bool,
    warnings: &mut Vec<String>,
) -> Vec<crate::animation::PfKeyFrame> {
    use crate::animation::{PfKeyFrame, PfKeyInterp, PfKeyValue, parse_duration};
    let mut frames = Vec::new();
    for frame in node.child_elements() {
        let interp = if frame.name.starts_with("Discrete") {
            PfKeyInterp::Discrete
        } else if frame.name.starts_with("Linear") {
            PfKeyInterp::Linear
        } else if frame.name.starts_with("Easing") {
            let easing = frame
                .property_element("EasingFunction")
                .and_then(|pe| pe.single_element())
                .map(|el| parse_easing_element(el, warnings))
                .unwrap_or(crate::animation::PfEasing::Linear);
            PfKeyInterp::Easing(easing)
        } else if frame.name.starts_with("Spline") {
            let nums: Vec<f32> = match frame.attribute("KeySpline") {
                Some(XamlValue::Str(s)) => s
                    .split([',', ' '])
                    .filter(|t| !t.is_empty())
                    .filter_map(|t| t.trim().parse().ok())
                    .collect(),
                _ => Vec::new(),
            };
            if nums.len() == 4 {
                PfKeyInterp::Spline {
                    x1: nums[0],
                    y1: nums[1],
                    x2: nums[2],
                    y2: nums[3],
                }
            } else {
                warnings.push(format!("{}: bad KeySpline; Linear used", frame.pos));
                PfKeyInterp::Linear
            }
        } else {
            warnings.push(format!(
                "{}: keyframe `{}` is not supported; skipped",
                frame.pos, frame.name
            ));
            continue;
        };
        let time = match frame.attribute("KeyTime") {
            Some(XamlValue::Str(t)) if t.eq_ignore_ascii_case("uniform") => None,
            Some(XamlValue::Str(t)) => match parse_duration(t) {
                Some(secs) => Some(secs),
                None => {
                    warnings.push(format!(
                        "{}: KeyTime `{t}` is not supported (Paced/percent); Uniform used",
                        frame.pos
                    ));
                    None
                }
            },
            _ => None,
        };
        let value = match frame.attribute("Value") {
            Some(XamlValue::Str(text)) if color => match text.parse::<v::PfColor>() {
                Ok(c) => PfKeyValue::Color(c),
                Err(_) => {
                    warnings.push(format!("{}: bad keyframe color `{text}`", frame.pos));
                    continue;
                }
            },
            Some(XamlValue::Str(v)) => match v.trim().parse::<f64>() {
                Ok(d) => PfKeyValue::Double(d),
                Err(_) => {
                    warnings.push(format!("{}: bad keyframe value `{v}`", frame.pos));
                    continue;
                }
            },
            _ => {
                warnings.push(format!("{}: keyframe needs Value=", frame.pos));
                continue;
            }
        };
        frames.push(PfKeyFrame {
            time,
            value,
            interp,
        });
    }
    frames
}

/// Parse one easing-function element (`<CubicEase EasingMode=.../>`).
/// Reduce a WPF `Storyboard.TargetProperty` path to the dependency property
/// the store can write.
///
/// WPF paths address a property *chain*, and the animated property is the
/// FIRST step -- everything after it just names how to reach the value inside
/// that property. `ColorAnimation` is the common case, because a brush colour
/// is canonically written as a two-step path:
///
/// ```text
/// (Border.Background).(SolidColorBrush.Color)  -> Background
/// Background.Color                             -> Background
/// (Panel.Background)                           -> Background
/// Opacity                                      -> Opacity
/// ```
///
/// Taking the last segment (the old behaviour) yielded `Color` for the first
/// two, which matches no target and was silently skipped -- so a correctly
/// authored WPF brush animation did nothing. Take the first step instead, and
/// strip a trailing value-accessor step when one is present.
fn normalize_target_property(path: &str) -> String {
    let path = path.trim();
    let last_seg = |seg: &str| -> String {
        seg.rsplit('.').next().unwrap_or_default().trim().to_string()
    };

    // The parenthesized steps, in order: "(A.B).(C.D)" -> ["A.B", "C.D"].
    let mut groups: Vec<&str> = Vec::new();
    let mut rest = path;
    while let Some(open) = rest.find('(') {
        let Some(close) = rest[open..].find(')') else { break };
        groups.push(&rest[open + 1..open + close]);
        rest = &rest[open + close + 1..];
    }

    if groups.len() >= 2 {
        // A multi-step path addresses a property chain, and which END names
        // the animated value depends on what the chain walks INTO:
        //
        //   (Border.Background).(SolidColorBrush.Color)
        //       -> Background: the brush IS the animated property and
        //          ".Color" only says which part of it varies.
        //   (UIElement.RenderTransform).(...)[0].(ScaleTransform.ScaleX)
        //       -> ScaleX: transform fields are modelled as their own targets
        //          here (see TransformField), so the last step is the
        //          property and the chain to it is scaffolding.
        //
        // Getting this backwards silently breaks one or the other: a brush
        // path collapsing to "Color" matches nothing, and a transform path
        // collapsing to "RenderTransform" does the same.
        let final_group = groups.last().expect("len >= 2");
        let owner = final_group.split('.').next().unwrap_or_default().trim();
        if owner.ends_with("Transform") {
            return last_seg(final_group);
        }
        return last_seg(groups[0]);
    }

    if let Some(only) = groups.first() {
        // Owner-qualified single step: "(UIElement.Opacity)" is Opacity. The
        // last segment is the property even when named like an accessor.
        return last_seg(only);
    }

    // Unparenthesized. A trailing value accessor ("Background.Color") names a
    // field of the property before it; a bare "Owner.Prop" does not.
    let segments: Vec<&str> = path.split('.').collect();
    if segments.len() > 1 && matches!(segments[segments.len() - 1].trim(), "Color" | "Offset") {
        return segments[segments.len() - 2].trim().to_string();
    }
    last_seg(path)
}

/// `<VisualTransition From=.. To=.. GeneratedDuration=..>` with an
/// optional `GeneratedEasingFunction` property element.
fn parse_visual_transition(
    node: &XamlNode,
    warnings: &mut Vec<String>,
) -> crate::animation::PfVisualTransition {
    let attr = |name: &str| match node.attribute(name) {
        Some(XamlValue::Str(v)) => Some(v.clone()),
        _ => None,
    };
    let easing = node
        .property_element("GeneratedEasingFunction")
        .and_then(|pe| pe.elements().next())
        .map(|f| parse_easing_element(f, warnings));
    crate::animation::PfVisualTransition {
        from: attr("From"),
        to: attr("To"),
        duration: attr("GeneratedDuration")
            .and_then(|d| crate::animation::parse_duration(&d))
            .unwrap_or(0.0),
        easing,
    }
}

fn parse_easing_element(f: &XamlNode, warnings: &mut Vec<String>) -> crate::animation::PfEasing {
    use crate::animation::{EasingMode, PfEasing};
    let mode = match f.attribute("EasingMode") {
        Some(XamlValue::Str(m)) if m.eq_ignore_ascii_case("easein") => EasingMode::In,
        Some(XamlValue::Str(m)) if m.eq_ignore_ascii_case("easeinout") => EasingMode::InOut,
        _ => EasingMode::Out, // WPF default
    };
    match f.name.as_str() {
        "QuadraticEase" => PfEasing::Quadratic(mode),
        "CubicEase" => PfEasing::Cubic(mode),
        "QuarticEase" => PfEasing::Quartic(mode),
        "QuinticEase" => PfEasing::Quintic(mode),
        "SineEase" => PfEasing::Sine(mode),
        "CircleEase" => PfEasing::Circle(mode),
        "BackEase" => PfEasing::Back(mode),
        "ElasticEase" => PfEasing::Elastic(mode),
        "BounceEase" => PfEasing::Bounce(mode),
        "ExponentialEase" => PfEasing::Exponential(mode),
        "PowerEase" => PfEasing::Quadratic(mode),
        other => {
            warnings.push(format!(
                "{}: easing `{other}` is not supported; Linear used",
                f.pos
            ));
            PfEasing::Linear
        }
    }
}

/// Parse a `<Storyboard>`: Double/Color animations with Storyboard.Target*
/// attached attributes. Keyframe/easing forms warn (plan phase 2b).
pub(crate) fn parse_storyboard(
    node: &XamlNode,
    warnings: &mut Vec<String>,
) -> Result<crate::animation::PfStoryboard, PfError> {
    use crate::animation::{PfAnimKind, PfAnimationSpec, PfFill, PfRepeat, parse_duration};
    let mut children = Vec::new();
    for child in node.child_elements() {
        let attr = |name: &str| -> Option<String> {
            match child.attribute(name) {
                Some(XamlValue::Str(s)) => Some(s.clone()),
                _ => None,
            }
        };
        let attached = |name: &str| -> Option<String> {
            match child.attached_attribute("Storyboard", name) {
                Some(XamlValue::Str(s)) => Some(s.clone()),
                _ => None,
            }
        };
        let kind = match child.name.as_str() {
            "DoubleAnimation" => {
                let to = attr("To").and_then(|t| t.trim().parse::<f64>().ok());
                let by = attr("By").and_then(|t| t.trim().parse::<f64>().ok());
                if to.is_none() && by.is_none() {
                    warnings.push(format!(
                        "{}: DoubleAnimation needs To= or By=; skipped",
                        child.pos
                    ));
                    continue;
                }
                PfAnimKind::Double {
                    from: attr("From").and_then(|f| f.trim().parse().ok()),
                    to,
                    by,
                }
            }
            "ColorAnimation" => {
                let Some(to) = attr("To").and_then(|t| t.parse::<v::PfColor>().ok()) else {
                    warnings.push(format!("{}: ColorAnimation needs To=; skipped", child.pos));
                    continue;
                };
                PfAnimKind::Color {
                    from: attr("From").and_then(|f| f.parse().ok()),
                    to,
                }
            }
            "DoubleAnimationUsingKeyFrames" => PfAnimKind::DoubleKeyFrames {
                frames: parse_key_frames(child, false, warnings),
            },
            "ColorAnimationUsingKeyFrames" => PfAnimKind::ColorKeyFrames {
                frames: parse_key_frames(child, true, warnings),
            },
            other => {
                warnings.push(format!(
                    "{}: animation `{other}` is not supported yet; skipped",
                    child.pos
                ));
                continue;
            }
        };
        let Some(target_property) = attached("TargetProperty") else {
            warnings.push(format!(
                "{}: animation needs Storyboard.TargetProperty; skipped",
                child.pos
            ));
            continue;
        };
        let target_property = normalize_target_property(&target_property);
        let duration = attr("Duration")
            .and_then(|d| parse_duration(&d))
            .unwrap_or_else(|| match &kind {
                // Keyframe animations default to their last key time.
                PfAnimKind::DoubleKeyFrames { frames } | PfAnimKind::ColorKeyFrames { frames } => {
                    frames
                        .iter()
                        .filter_map(|f| f.time)
                        .fold(0.0_f32, f32::max)
                        .max(1.0 / 240.0)
                }
                _ => 1.0,
            });
        let begin_time = attr("BeginTime")
            .and_then(|d| parse_duration(&d))
            .unwrap_or(0.0);
        let repeat = match attr("RepeatBehavior").as_deref() {
            None => PfRepeat::Once,
            Some(r) if r.eq_ignore_ascii_case("forever") => PfRepeat::Forever,
            Some(r) => match r.trim().trim_end_matches(['x', 'X']).parse::<f32>() {
                Ok(n) => PfRepeat::Count(n),
                Err(_) => {
                    warnings.push(format!(
                        "{}: bad RepeatBehavior `{r}`; using Once",
                        child.pos
                    ));
                    PfRepeat::Once
                }
            },
        };
        let fill = match attr("FillBehavior").as_deref() {
            Some(f) if f.eq_ignore_ascii_case("stop") => PfFill::Stop,
            _ => PfFill::HoldEnd,
        };
        // <DoubleAnimation.EasingFunction><CubicEase EasingMode=".."/>
        // or AccelerationRatio/DecelerationRatio attrs.
        let easing = if let Some(pe) = child.property_element("EasingFunction") {
            match pe.single_element() {
                Some(f) => parse_easing_element(f, warnings),
                None => crate::animation::PfEasing::Linear,
            }
        } else {
            let accel = attr("AccelerationRatio")
                .and_then(|a| a.trim().parse::<f32>().ok())
                .unwrap_or(0.0);
            let decel = attr("DecelerationRatio")
                .and_then(|d| d.trim().parse::<f32>().ok())
                .unwrap_or(0.0);
            if accel > 0.0 || decel > 0.0 {
                crate::animation::PfEasing::AccelDecel { accel, decel }
            } else {
                crate::animation::PfEasing::Linear
            }
        };
        children.push(PfAnimationSpec {
            easing,
            target_name: attached("TargetName"),
            target_property,
            kind,
            duration,
            begin_time,
            repeat,
            auto_reverse: attr("AutoReverse")
                .map(|a| a.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            fill,
            speed_ratio: attr("SpeedRatio")
                .and_then(|s| s.trim().parse::<f32>().ok())
                .filter(|r| *r > 0.0)
                .unwrap_or(1.0),
        });
    }
    Ok(crate::animation::PfStoryboard { children })
}

/// Parse `<EventTrigger RoutedEvent="..."><BeginStoryboard>...` into a
/// storyboard launcher. `None` (with a warning) when unsupported.
pub(crate) fn parse_event_trigger(
    node: &XamlNode,
    scopes: &ResourceScopes,
    local: &ResourceDictionary,
    warnings: &mut Vec<String>,
) -> Option<crate::animation::PfEventTrigger> {
    let event = match node.attribute("RoutedEvent") {
        Some(XamlValue::Str(e)) => e.rsplit('.').next().unwrap_or(e).to_string(),
        _ => {
            warnings.push(format!("{}: EventTrigger needs RoutedEvent", node.pos));
            return None;
        }
    };
    if !matches!(
        event.as_str(),
        "Loaded" | "MouseEnter" | "MouseLeave" | "Click"
    ) {
        warnings.push(format!(
            "{}: EventTrigger RoutedEvent `{event}` is not supported yet",
            node.pos
        ));
        return None;
    }
    let begin = node
        .child_elements()
        .find(|c| c.name == "BeginStoryboard")?;
    // Inline <Storyboard> or Storyboard="{StaticResource key}".
    let storyboard = if let Some(sb) = begin.child_elements().find(|c| c.name == "Storyboard") {
        match parse_storyboard(sb, warnings) {
            Ok(sb) => std::sync::Arc::new(sb),
            Err(e) => {
                warnings.push(format!("{}: {e}", node.pos));
                return None;
            }
        }
    } else if let Some(XamlValue::Extension(ext)) = begin.attribute("Storyboard") {
        match static_resource_key(ext)
            .ok()
            .and_then(|key| local.get(&key).or_else(|| scopes.lookup(&key)))
        {
            Some(PfValue::Storyboard(sb)) => sb.clone(),
            _ => {
                warnings.push(format!(
                    "{}: BeginStoryboard Storyboard resource not found",
                    begin.pos
                ));
                return None;
            }
        }
    } else {
        warnings.push(format!(
            "{}: BeginStoryboard needs an inline Storyboard or a resource",
            begin.pos
        ));
        return None;
    };
    Some(crate::animation::PfEventTrigger { event, storyboard })
}

fn parse_trigger(
    node: &XamlNode,
    scopes: &ResourceScopes,
    local: &ResourceDictionary,
    warnings: &mut Vec<String>,
    // TargetName on trigger setters is only legal inside a ControlTemplate
    // (WPF hard-errors on it in style triggers, Style.cs:733-744).
    allow_target_name: bool,
) -> Result<Option<PfTrigger>, PfError> {
    let attr_str = |el: &XamlNode, name: &str| -> Option<String> {
        match el.attribute(name) {
            Some(XamlValue::Str(s)) => Some(s.clone()),
            _ => None,
        }
    };
    let binding_path = |el: &XamlNode| -> Option<String> {
        match el.attribute("Binding") {
            Some(XamlValue::Extension(ext)) if ext.name == "Binding" => {
                let spec = crate::binding::parse_binding_extension(ext);
                (spec.unsupported.is_none() && spec.element_name.is_none()).then_some(spec.path)
            }
            _ => None,
        }
    };
    // `Value` may be an attribute or a property element whose content is
    // text (or a single element with text content, e.g. an enum value).
    let trigger_value = |el: &XamlNode| -> Option<PfTriggerValue> {
        match el.attribute("Value") {
            Some(XamlValue::Str(v)) => return Some(PfTriggerValue::Text(v.clone())),
            Some(XamlValue::Extension(ext)) if ext.name == "x:Null" || ext.name == "Null" => {
                return Some(PfTriggerValue::Null);
            }
            _ => {}
        }
        let pe = el.property_element("Value")?;
        if let Some(inner) = pe.single_element() {
            return inner.text_content().map(PfTriggerValue::Text);
        }
        pe.values
            .iter()
            .find_map(|c| c.as_text())
            .map(|t| PfTriggerValue::Text(t.to_string()))
    };

    let mut conditions = Vec::new();
    match node.name.as_str() {
        "Trigger" => {
            let property = attr_str(node, "Property")
                .ok_or_else(|| PfError::resource("Trigger needs Property"))?;
            let value =
                trigger_value(node).ok_or_else(|| PfError::resource("Trigger needs Value"))?;
            conditions.push(PfTriggerCondition::Property { property, value });
        }
        "DataTrigger" => {
            let path = binding_path(node)
                .ok_or_else(|| PfError::resource("DataTrigger needs a plain-path Binding"))?;
            let value =
                trigger_value(node).ok_or_else(|| PfError::resource("DataTrigger needs Value"))?;
            conditions.push(PfTriggerCondition::Data { path, value });
        }
        "MultiTrigger" | "MultiDataTrigger" => {
            let Some(conds) = node.property_element("Conditions") else {
                return Err(PfError::resource("MultiTrigger needs Conditions"));
            };
            for cond in conds.elements() {
                if cond.name != "Condition" {
                    continue;
                }
                let value = trigger_value(cond)
                    .ok_or_else(|| PfError::resource("Condition needs Value"))?;
                if let Some(property) = attr_str(cond, "Property") {
                    conditions.push(PfTriggerCondition::Property { property, value });
                } else if let Some(path) = binding_path(cond) {
                    conditions.push(PfTriggerCondition::Data { path, value });
                } else {
                    return Err(PfError::resource(
                        "Condition needs Property or a plain-path Binding",
                    ));
                }
            }
            if conditions.is_empty() {
                return Err(PfError::resource("MultiTrigger has no conditions"));
            }
        }
        "EventTrigger" => {
            warnings.push(format!(
                "{}: EventTrigger (storyboards) is not supported yet",
                node.pos
            ));
            return Ok(None);
        }
        other => {
            warnings.push(format!(
                "{}: trigger `{other}` is not supported yet",
                node.pos
            ));
            return Ok(None);
        }
    }

    // Enter/ExitActions: BeginStoryboard launchers run on activation flips.
    let parse_actions =
        |pe_name: &str, warnings: &mut Vec<String>| -> Vec<Arc<crate::animation::PfStoryboard>> {
            let Some(pe) = node.property_element(pe_name) else {
                return Vec::new();
            };
            let mut out = Vec::new();
            for action in pe.elements() {
                if action.name != "BeginStoryboard" {
                    warnings.push(format!(
                        "{}: trigger action `{}` is not supported yet",
                        action.pos, action.name
                    ));
                    continue;
                }
                if let Some(sb) = action.child_elements().find(|c| c.name == "Storyboard") {
                    match parse_storyboard(sb, warnings) {
                        Ok(sb) => out.push(Arc::new(sb)),
                        Err(e) => warnings.push(format!("{}: {e}", action.pos)),
                    }
                } else if let Some(XamlValue::Extension(ext)) = action.attribute("Storyboard") {
                    match static_resource_key(ext).ok().and_then(|key| {
                        local
                            .get(&key)
                            .cloned()
                            .or_else(|| scopes.lookup(&key).cloned())
                    }) {
                        Some(PfValue::Storyboard(sb)) => out.push(sb),
                        _ => warnings.push(format!(
                            "{}: BeginStoryboard Storyboard resource not found",
                            action.pos
                        )),
                    }
                }
            }
            out
        };
    let enter_storyboards = parse_actions("EnterActions", warnings);
    let exit_storyboards = parse_actions("ExitActions", warnings);

    // Setters: direct children or <Trigger.Setters>.
    let from_property = node
        .property_element("Setters")
        .map(|p| p.elements().collect::<Vec<_>>());
    let setter_elems: Vec<&XamlNode> = match &from_property {
        Some(elems) => elems.clone(),
        None => node.child_elements().collect(),
    };
    let mut setters = Vec::new();
    for setter in setter_elems {
        if setter.name != "Setter" {
            continue;
        }
        if !allow_target_name && setter.attribute("TargetName").is_some() {
            warnings.push(format!(
                "{}: trigger setter TargetName is only valid in ControlTemplate triggers; skipped",
                setter.pos
            ));
            continue;
        }
        match parse_setter(setter, scopes, local, warnings) {
            Ok(s) => setters.push(s),
            Err(e) => warnings.push(format!("{}: skipped trigger setter: {e}", setter.pos)),
        }
    }

    Ok(Some(PfTrigger {
        conditions,
        setters,
        enter_storyboards,
        exit_storyboards,
    }))
}

fn parse_setter(
    node: &XamlNode,
    scopes: &ResourceScopes,
    local: &ResourceDictionary,
    warnings: &mut Vec<String>,
) -> Result<PfSetter, PfError> {
    let prop = match node.attribute("Property") {
        Some(XamlValue::Str(s)) => s.clone(),
        _ => return Err(PfError::resource("Setter needs Property")),
    };
    let (owner, property) = match prop.split_once('.') {
        // `Control.Background`-style self-qualified setters (common in
        // base styles shared across control types) are not attached.
        Some((o @ ("Control" | "FrameworkElement" | "UIElement" | "TextElement"), p)) => {
            let _ = o;
            (None, p.to_string())
        }
        Some((o, p)) => (Some(o.to_string()), p.to_string()),
        None => (None, prop),
    };

    let value = if let Some(value) = node.attribute("Value") {
        match value {
            XamlValue::Str(s) => PfSetterValue::Literal(s.clone()),
            XamlValue::Extension(ext) if ext.name == "StaticResource" => {
                PfSetterValue::Resource(static_resource_key(ext)?)
            }
            XamlValue::Extension(ext) if ext.name == "DynamicResource" => {
                PfSetterValue::DynamicResource(static_resource_key(ext)?)
            }
            XamlValue::Extension(ext) if ext.name == "x:Null" || ext.name == "Null" => {
                PfSetterValue::Null
            }
            XamlValue::Extension(ext) if ext.name == "AppThemeBinding" => {
                PfSetterValue::AppTheme(parse_app_theme_arms(ext)?)
            }
            XamlValue::Extension(ext)
                if ext.name == "OnPlatform" || ext.name == "OnIdiom" =>
            {
                let kind = if ext.name == "OnPlatform" {
                    crate::device::SelectorKind::Platform
                } else {
                    crate::device::SelectorKind::Idiom
                };
                PfSetterValue::Selector(kind, parse_selector_arms(ext, kind)?)
            }
            XamlValue::Extension(ext) if ext.name == "TemplateBinding" => {
                let src = ext
                    .first_positional_str()
                    .ok_or_else(|| PfError::resource("TemplateBinding needs a source property"))?;
                PfSetterValue::TemplatedParent(src.to_string())
            }
            // Only the TemplatedParent flavour of {Binding} means anything in
            // a setter: a setter has no data context of its own, so the other
            // flavours would need a live per-instance binding, not a value.
            XamlValue::Extension(ext) if ext.name == "Binding" => {
                let spec = crate::binding::parse_binding_extension(ext);
                match spec.relative {
                    Some(crate::binding::PfRelativeSource::TemplatedParent) if !spec.path.is_empty() => {
                        PfSetterValue::TemplatedParent(spec.path)
                    }
                    Some(crate::binding::PfRelativeSource::TemplatedParent) => {
                        return Err(PfError::resource(
                            "a TemplatedParent setter binding needs a source property",
                        ));
                    }
                    _ => {
                        return Err(PfError::resource(
                            "only `{Binding Prop, RelativeSource={RelativeSource TemplatedParent}}` \
                             is supported as a setter value",
                        ));
                    }
                }
            }
            XamlValue::Extension(ext) => {
                return Err(PfError::resource(format!(
                    "setter value extension `{}` not supported yet",
                    ext.name
                )));
            }
        }
    } else if let Some(pe) = node.property_element("Value") {
        let el = pe
            .single_element()
            .ok_or_else(|| PfError::resource("Setter.Value must hold one element"))?;
        match parse_resource_value(el, scopes, local, warnings)? {
            Some(v) => PfSetterValue::Value(v),
            None => {
                return Err(PfError::resource(format!(
                    "unsupported Setter.Value element `{}`",
                    el.name
                )));
            }
        }
    } else {
        return Err(PfError::resource("Setter needs Value"));
    };

    Ok(PfSetter {
        owner,
        property,
        value,
        target_name: match node.attribute("TargetName") {
            Some(XamlValue::Str(t)) => Some(t.clone()),
            _ => None,
        },
    })
}

#[cfg(test)]
mod target_property_tests {
    use super::normalize_target_property;

    #[test]
    fn plain_property() {
        assert_eq!(normalize_target_property("Opacity"), "Opacity");
        assert_eq!(normalize_target_property(" Width "), "Width");
    }

    #[test]
    fn single_parenthesized_step() {
        assert_eq!(
            normalize_target_property("(Panel.Background)"),
            "Background"
        );
    }

    /// The canonical WPF brush-colour path. Previously reduced to `Color`,
    /// which matched no target, so the animation was silently dropped.
    #[test]
    fn brush_color_chain_resolves_to_the_brush_property() {
        assert_eq!(
            normalize_target_property("(Border.Background).(SolidColorBrush.Color)"),
            "Background"
        );
        assert_eq!(
            normalize_target_property("(Control.Foreground).(SolidColorBrush.Color)"),
            "Foreground"
        );
    }

    #[test]
    fn unparenthesized_value_accessor() {
        assert_eq!(normalize_target_property("Background.Color"), "Background");
    }

    /// A bare `Owner.Property` is NOT an accessor chain -- the last segment is
    /// the property and must survive.
    /// Owner-qualified single group: the last segment is the property, and
    /// must NOT be mistaken for a value accessor even when it is named like
    /// one. `(UIElement.Opacity)` is Opacity, not UIElement.
    #[test]
    fn owner_qualified_property_named_like_an_accessor() {
        assert_eq!(normalize_target_property("(UIElement.Opacity)"), "Opacity");
        assert_eq!(normalize_target_property("(GradientStop.Color)"), "Color");
    }

    /// A transform path names a property the store cannot write; it must not
    /// silently collapse onto a real property. Either way it is unsupported,
    /// but it should not resolve to something writable by accident.
    #[test]
    fn transform_chain_resolves_to_the_field() {
        // Transform fields are their own animation targets (TransformField),
        // so a transform chain resolves to its LAST step -- the opposite end
        // from a brush chain. This test previously asserted "LayoutTransform",
        // which looked reasonable in isolation and broke every transform
        // animation in the suite.
        assert_eq!(
            normalize_target_property("(FrameworkElement.LayoutTransform).(RotateTransform.Angle)"),
            "Angle"
        );
        assert_eq!(
            normalize_target_property(
                "(UIElement.RenderTransform).(TransformGroup.Children)[0].(ScaleTransform.ScaleX)"
            ),
            "ScaleX"
        );
        assert_eq!(
            normalize_target_property("(UIElement.RenderTransform).(TranslateTransform.X)"),
            "X"
        );
    }

    #[test]
    fn attached_style_owner_property_keeps_last_segment() {
        assert_eq!(
            normalize_target_property("Border.BorderThickness"),
            "BorderThickness"
        );
        assert_eq!(normalize_target_property("(Grid.Row)"), "Row");
    }
}

#[cfg(test)]
mod storyboard_parse_tests {
    use super::parse_storyboard;
    use crate::animation::PfAnimKind;

    /// Parse a `<Storyboard>` through the real XAML parser.
    fn storyboard(inner: &str) -> crate::animation::PfStoryboard {
        let doc = bevy_pf_xaml::parse(&format!("<Storyboard>{inner}</Storyboard>"))
            .expect("test XAML must parse");
        let mut warnings = Vec::new();
        parse_storyboard(&doc.root, &mut warnings).expect("storyboard must parse")
    }

    #[test]
    fn by_is_accepted_without_to() {
        let sb = storyboard(
            r#"<DoubleAnimation Storyboard.TargetProperty="Opacity" By="0.25" Duration="0:0:1"/>"#,
        );
        assert_eq!(sb.children.len(), 1, "By= must no longer be skipped");
        match &sb.children[0].kind {
            PfAnimKind::Double { to, by, .. } => {
                assert_eq!(*by, Some(0.25));
                assert_eq!(*to, None);
            }
            other => panic!("expected Double, got {other:?}"),
        }
    }

    /// WPF: when an author supplies both, `To` wins and `By` is ignored.
    #[test]
    fn to_and_by_both_parsed_so_to_can_win() {
        let sb = storyboard(
            r#"<DoubleAnimation Storyboard.TargetProperty="Opacity" From="0" To="1" By="5"/>"#,
        );
        match &sb.children[0].kind {
            PfAnimKind::Double { from, to, by } => {
                assert_eq!(*from, Some(0.0));
                assert_eq!(*to, Some(1.0));
                assert_eq!(*by, Some(5.0));
            }
            other => panic!("expected Double, got {other:?}"),
        }
    }

    #[test]
    fn neither_to_nor_by_is_still_rejected() {
        let sb = storyboard(r#"<DoubleAnimation Storyboard.TargetProperty="Opacity"/>"#);
        assert!(sb.children.is_empty());
    }

    #[test]
    fn speed_ratio_defaults_to_one_and_rejects_nonpositive() {
        let sb = storyboard(
            r#"<DoubleAnimation Storyboard.TargetProperty="Opacity" To="1"/>
               <DoubleAnimation Storyboard.TargetProperty="Opacity" To="1" SpeedRatio="2.5"/>
               <DoubleAnimation Storyboard.TargetProperty="Opacity" To="1" SpeedRatio="0"/>"#,
        );
        assert_eq!(sb.children[0].speed_ratio, 1.0);
        assert_eq!(sb.children[1].speed_ratio, 2.5);
        // A zero ratio would divide the duration to infinity; fall back to 1.
        assert_eq!(sb.children[2].speed_ratio, 1.0);
    }

    /// The canonical WPF brush path must survive parsing, not just the
    /// normalizer -- this is the form a `ColorAnimation` is written in.
    #[test]
    fn color_animation_with_canonical_brush_path() {
        // r##..##: the `"#` in a colour literal would close an r#".."# string.
        let sb = storyboard(
            r##"<ColorAnimation To="#FF00FF"
                 Storyboard.TargetProperty="(Border.Background).(SolidColorBrush.Color)"/>"##,
        );
        assert_eq!(sb.children.len(), 1);
        assert_eq!(sb.children[0].target_property, "Background");
    }
}
