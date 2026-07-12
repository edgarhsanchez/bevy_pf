//! Data binding: `{Binding Path=...}` against a reflected view-model.
//!
//! The WPF pattern maps onto Rust like this:
//!
//! - **View-model**: any `#[derive(Reflect)]` struct wrapped in a [`Bindable`]
//!   (the `INotifyPropertyChanged` analog — mutations through
//!   [`Bindable::update`] bump a version counter that drives change
//!   propagation).
//! - **`DataContext`**: a component holding a [`Bindable`]; children inherit
//!   the nearest ancestor's context, like WPF.
//! - **`{Binding Path=Score}`** in XAML records a [`PfBinding`] on the target
//!   entity; systems apply source values to targets (`OneWay`) and write
//!   control state back (`TwoWay` — TextBox text, CheckBox `IsChecked`,
//!   Slider `Value`).
//!
//! Binding paths use Bevy's reflection path syntax, which matches WPF's
//! (`Player.Name`, `Items[0].Score`).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use bevy::prelude::*;
use bevy::reflect::{GetPath, PartialReflect, Reflect};
use bevy::ui::Checked;
use bevy_pf_xaml::value as v;

use crate::convert;

// ---------------------------------------------------------------------------
// Bindable source
// ---------------------------------------------------------------------------

type CommandFn = dyn Fn(&mut World, Option<String>) + Send + Sync;

struct BindableInner {
    value: std::sync::RwLock<Box<dyn Reflect + Send + Sync>>,
    version: AtomicU64,
    /// ICommand analog: named commands invokable from `Command=` bindings.
    commands: std::sync::RwLock<
        bevy::platform::collections::HashMap<String, std::sync::Arc<CommandFn>>,
    >,
}

/// A shared, change-tracked view-model. Clone freely; clones share state.
/// A `Bindable` may be scoped to a sub-path of the root model (list items:
/// `vm.at("items[3]")`) — reads/writes join the prefix, the version counter
/// is shared.
#[derive(Clone)]
pub struct Bindable {
    inner: Arc<BindableInner>,
    prefix: Option<Arc<str>>,
}

impl Bindable {
    pub fn new<T: Reflect + Send + Sync>(value: T) -> Self {
        Self {
            inner: Arc::new(BindableInner {
                value: std::sync::RwLock::new(Box::new(value)),
                version: AtomicU64::new(1),
                commands: Default::default(),
            }),
            prefix: None,
        }
    }

    /// A view of this model scoped to `path` (e.g. `"items[3]"`). Bindings
    /// against the returned `Bindable` resolve relative to that path.
    pub fn at(&self, path: impl AsRef<str>) -> Bindable {
        Bindable {
            inner: self.inner.clone(),
            prefix: Some(Self::join(self.prefix.as_deref(), path.as_ref()).into()),
        }
    }

    fn join(prefix: Option<&str>, path: &str) -> String {
        match (prefix, path) {
            (None, p) | (Some(""), p) => p.to_string(),
            (Some(pre), "") => pre.to_string(),
            // Indexers attach without a dot: "items" + "[3]" -> "items[3]".
            (Some(pre), p) if p.starts_with('[') => format!("{pre}{p}"),
            (Some(pre), p) => format!("{pre}.{p}"),
        }
    }

    fn full_path(&self, path: &str) -> String {
        Self::join(self.prefix.as_deref(), path)
    }

    /// Register a named command (the `ICommand` analog): invoked by
    /// `Command="{Binding Name}"` (or `Command="Name"`) on Button-family
    /// controls, MenuItems, and Hyperlinks, with the resolved
    /// `CommandParameter` as the argument. Registration is shared by every
    /// clone and scoped view of this model.
    pub fn on_command(
        &self,
        name: impl Into<String>,
        f: impl Fn(&mut World, Option<String>) + Send + Sync + 'static,
    ) -> &Self {
        self.inner
            .commands
            .write()
            .unwrap()
            .insert(name.into(), std::sync::Arc::new(f));
        self
    }

    /// Look up a registered command by name.
    pub(crate) fn command(&self, name: &str) -> Option<std::sync::Arc<CommandFn>> {
        self.inner.commands.read().unwrap().get(name).cloned()
    }

    /// Current change version (bumped by every mutation).
    pub fn version(&self) -> u64 {
        self.inner.version.load(Ordering::Acquire)
    }

    /// Mutate the model through a typed closure, then notify bindings.
    /// `T` is always the ROOT model type, regardless of any prefix.
    /// Returns `None` if the model is not a `T`.
    pub fn update<T: Reflect, R>(&self, f: impl FnOnce(&mut T) -> R) -> Option<R> {
        let mut guard = self.inner.value.write().unwrap();
        let target = guard.as_any_mut().downcast_mut::<T>()?;
        let result = f(target);
        self.inner.version.fetch_add(1, Ordering::AcqRel);
        Some(result)
    }

    /// Read the value at a reflection path, converted to a [`BoundValue`].
    pub fn read_path(&self, path: &str) -> Option<BoundValue> {
        let full = self.full_path(path);
        let guard = self.inner.value.read().unwrap();
        let reflected: &dyn PartialReflect = if full.is_empty() {
            guard.as_partial_reflect()
        } else {
            guard.reflect_path(full.as_str()).ok()?
        };
        Some(reflect_to_bound(reflected))
    }

    /// The number of elements if the path resolves to a list/array.
    pub fn list_len(&self, path: &str) -> Option<usize> {
        use bevy::reflect::ReflectRef;
        let full = self.full_path(path);
        let guard = self.inner.value.read().unwrap();
        let reflected: &dyn PartialReflect = if full.is_empty() {
            guard.as_partial_reflect()
        } else {
            guard.reflect_path(full.as_str()).ok()?
        };
        match reflected.reflect_ref() {
            ReflectRef::List(l) => Some(l.len()),
            ReflectRef::Array(a) => Some(a.len()),
            _ => None,
        }
    }

    /// Write a value at a reflection path (two-way bindings), then notify.
    pub fn write_path(&self, path: &str, value: &BoundValue) -> bool {
        let full = self.full_path(path);
        let mut guard = self.inner.value.write().unwrap();
        let Ok(target) = guard.reflect_path_mut(full.as_str()) else {
            return false;
        };
        if !bound_to_reflect(target, value) {
            return false;
        }
        self.inner.version.fetch_add(1, Ordering::AcqRel);
        true
    }
}

impl std::fmt::Debug for Bindable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bindable(v{})", self.version())
    }
}

/// The WPF `DataContext`: attach to any entity; descendants inherit it.
#[derive(Component, Clone, Debug)]
pub struct DataContext(pub Bindable);

// ---------------------------------------------------------------------------
// Bound values
// ---------------------------------------------------------------------------

/// A value read from / written to a view-model.
#[derive(Debug, Clone, PartialEq)]
pub enum BoundValue {
    Str(String),
    Num(f64),
    Bool(bool),
}

impl BoundValue {
    pub fn to_display(&self) -> String {
        match self {
            BoundValue::Str(s) => s.clone(),
            BoundValue::Num(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    format!("{}", *n as i64)
                } else {
                    format!("{n}")
                }
            }
            BoundValue::Bool(b) => if *b { "True" } else { "False" }.to_string(),
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            BoundValue::Num(n) => Some(*n),
            BoundValue::Str(s) => s.trim().parse().ok(),
            BoundValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            BoundValue::Bool(b) => Some(*b),
            BoundValue::Str(s) => Some(s.trim().eq_ignore_ascii_case("true")),
            BoundValue::Num(n) => Some(*n != 0.0),
        }
    }
}

fn reflect_to_bound(value: &dyn PartialReflect) -> BoundValue {
    macro_rules! try_num {
        ($($t:ty),+) => {
            $(if let Some(n) = value.try_downcast_ref::<$t>() {
                return BoundValue::Num(*n as f64);
            })+
        };
    }
    if let Some(s) = value.try_downcast_ref::<String>() {
        return BoundValue::Str(s.clone());
    }
    if let Some(b) = value.try_downcast_ref::<bool>() {
        return BoundValue::Bool(*b);
    }
    try_num!(f32, f64, i8, i16, i32, i64, u8, u16, u32, u64, usize, isize);
    // Fallback: the reflected debug representation.
    BoundValue::Str(format!("{value:?}"))
}

fn bound_to_reflect(target: &mut dyn PartialReflect, value: &BoundValue) -> bool {
    macro_rules! try_num {
        ($($t:ty),+) => {
            $(if let Some(slot) = target.try_downcast_mut::<$t>() {
                let Some(n) = value.as_f64() else { return false };
                *slot = n as $t;
                return true;
            })+
        };
    }
    if let Some(slot) = target.try_downcast_mut::<String>() {
        *slot = value.to_display();
        return true;
    }
    if let Some(slot) = target.try_downcast_mut::<bool>() {
        let Some(b) = value.as_bool() else { return false };
        *slot = b;
        return true;
    }
    try_num!(f32, f64, i8, i16, i32, i64, u8, u16, u32, u64, usize, isize);
    false
}

// ---------------------------------------------------------------------------
// Binding targets
// ---------------------------------------------------------------------------

/// What a binding writes into on its target entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingTarget {
    /// UI `Text` (TextBlock or generated content text).
    Text,
    /// `EditableText` content (TextBox input).
    EditableText,
    /// CheckBox/RadioButton/ToggleButton `Checked` state.
    IsChecked,
    /// Slider `SliderValue`.
    SliderValue,
    /// ProgressBar value.
    ProgressValue,
    /// `Visibility` (accepts "Visible/Hidden/Collapsed" strings or booleans).
    Visibility,
    Width,
    Height,
    FontSize,
    Foreground,
    Background,
}

/// Where a binding reads its value from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PfBindingSource {
    /// The inherited `DataContext` (default).
    DataContext,
    /// Another element, by `x:Name` — not yet resolved to an entity.
    Named(String),
    /// Another element (resolved `ElementName=...`).
    Element(Entity),
}

/// One `{Binding}` on a target entity.
#[derive(Debug, Clone)]
pub struct PfBinding {
    pub target: BindingTarget,
    pub source: PfBindingSource,
    pub path: String,
    pub mode: v::BindingMode,
    pub string_format: Option<String>,
    pub converter: Option<String>,
    pub converter_parameter: Option<String>,
    pub fallback: Option<String>,
    /// Source version last applied (0 = never). Unused for element sources,
    /// which are re-evaluated every frame (writes are no-ops when equal).
    pub seen_version: u64,
}

impl PfBinding {
    fn is_to_target(&self) -> bool {
        !matches!(self.mode, v::BindingMode::OneWayToSource)
    }

    fn is_to_source(&self) -> bool {
        matches!(
            self.mode,
            v::BindingMode::TwoWay | v::BindingMode::OneWayToSource
        )
    }

    fn format(&self, value: &BoundValue) -> String {
        let s = value.to_display();
        match &self.string_format {
            Some(fmt) if fmt.contains("{0") => apply_string_format(fmt, &s),
            Some(fmt) => format!("{fmt}{s}"),
            None => s,
        }
    }
}


/// Expand `{0}` / `{0:spec}` placeholders with .NET's common numeric format
/// specifiers (`C` currency, `F`/`N` fixed decimals, `P` percent). Unknown
/// specs fall back to the raw value.
fn apply_string_format(fmt: &str, raw: &str) -> String {
    let mut out = String::with_capacity(fmt.len() + raw.len());
    let mut rest = fmt;
    while let Some(start) = rest.find("{0") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let spec = &after[..end];
        out.push_str(&format_with_spec(spec.strip_prefix(':').unwrap_or(""), raw));
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

fn format_with_spec(spec: &str, raw: &str) -> String {
    if spec.is_empty() {
        return raw.to_string();
    }
    let Ok(number) = raw.trim().parse::<f64>() else {
        return raw.to_string();
    };
    let (kind, digits) = spec.split_at(1);
    let digits: usize = digits.parse().unwrap_or(2);
    match kind {
        "c" | "C" => format!("${number:.digits$}", digits = digits),
        "f" | "F" | "n" | "N" => format!("{number:.digits$}", digits = digits),
        "p" | "P" => format!("{:.digits$}%", number * 100.0, digits = digits),
        _ => raw.to_string(),
    }
}

/// The `IValueConverter` analog: transforms a bound value on its way to
/// the target (and optionally back). Converters are registered by name
/// (`app.register_converter("MyConverter", ...)`) and referenced from
/// markup as `Converter={StaticResource MyConverter}` — the resource key IS
/// the registry name (a documented deviation: converters are Rust values,
/// not parseable XAML resources).
pub trait PfValueConverter: Send + Sync {
    fn convert(&self, value: &BoundValue, parameter: Option<&str>) -> Option<BoundValue>;
    fn convert_back(&self, _value: &BoundValue, _parameter: Option<&str>) -> Option<BoundValue> {
        None
    }
}

/// Registered converters. `BooleanToVisibilityConverter` ships built in.
#[derive(Resource, Default, Clone)]
pub struct PfConverters(
    pub bevy::platform::collections::HashMap<String, std::sync::Arc<dyn PfValueConverter>>,
);

struct BoolToVisibility;
impl PfValueConverter for BoolToVisibility {
    fn convert(&self, value: &BoundValue, _parameter: Option<&str>) -> Option<BoundValue> {
        let b = match value {
            BoundValue::Bool(b) => *b,
            BoundValue::Num(n) => *n != 0.0,
            BoundValue::Str(s) => s.eq_ignore_ascii_case("true"),
        };
        Some(BoundValue::Str(
            if b { "Visible" } else { "Collapsed" }.to_string(),
        ))
    }
}

pub(crate) fn builtin_converters() -> PfConverters {
    let mut map = PfConverters::default();
    map.0.insert(
        "BooleanToVisibilityConverter".to_string(),
        std::sync::Arc::new(BoolToVisibility),
    );
    map
}

/// `App` extension for registering converters.
pub trait PfConverterAppExt {
    fn register_converter(
        &mut self,
        name: impl Into<String>,
        converter: impl PfValueConverter + 'static,
    ) -> &mut Self;
}

impl PfConverterAppExt for bevy::app::App {
    fn register_converter(
        &mut self,
        name: impl Into<String>,
        converter: impl PfValueConverter + 'static,
    ) -> &mut Self {
        let world = self.world_mut();
        if !world.contains_resource::<PfConverters>() {
            let built = builtin_converters();
            world.insert_resource(built);
        }
        world
            .resource_mut::<PfConverters>()
            .0
            .insert(name.into(), std::sync::Arc::new(converter));
        self
    }
}

/// All bindings targeting this entity.
#[derive(Component, Debug, Default, Clone)]
pub struct PfBindings(pub Vec<PfBinding>);

/// A `{Binding ...}` parsed from markup, before its target is resolved.
#[derive(Debug, Clone)]
pub struct BindingSpec {
    pub path: String,
    pub element_name: Option<String>,
    pub mode: v::BindingMode,
    pub string_format: Option<String>,
    /// `Converter={StaticResource name}` — the registry name.
    pub converter: Option<String>,
    pub converter_parameter: Option<String>,
    /// `FallbackValue=` used when the source path fails to resolve.
    pub fallback: Option<String>,
    /// `RelativeSource={RelativeSource Self|TemplatedParent}`.
    pub relative: Option<PfRelativeSource>,
    /// The first unsupported argument encountered (`Source`, `Converter`,
    /// `RelativeSource`, ...), if any — such bindings are skipped with a
    /// warning.
    pub unsupported: Option<String>,
}

/// `RelativeSource=` modes supported so far (AncestorType is tracked work).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PfRelativeSource {
    SelfSource,
    TemplatedParent,
}

/// Parse a `{Binding ...}` markup extension into a spec.
pub fn parse_binding_extension(ext: &bevy_pf_xaml::MarkupExtension) -> BindingSpec {
    let mut spec = BindingSpec {
        path: ext.first_positional_str().unwrap_or_default().to_string(),
        element_name: None,
        mode: v::BindingMode::Default,
        string_format: None,
        converter: None,
        converter_parameter: None,
        fallback: None,
        relative: None,
        unsupported: None,
    };
    for (key, value) in &ext.named {
        let value_str = value.as_str().unwrap_or_default();
        match key.as_str() {
            "Path" => spec.path = value_str.to_string(),
            "ElementName" => spec.element_name = Some(value_str.to_string()),
            "Mode" => {
                if let Ok(mode) = value_str.parse() {
                    spec.mode = mode;
                }
            }
            "StringFormat" => spec.string_format = Some(value_str.to_string()),
            "Converter" => {
                // {StaticResource key} or a bare name: the key is the
                // converter's registry name either way.
                spec.converter = match value {
                    bevy_pf_xaml::markup::MarkupValue::Extension(inner) => inner
                        .first_positional_str()
                        .map(|k| k.to_string()),
                    _ => Some(value_str.to_string()),
                };
            }
            "ConverterParameter" => {
                spec.converter_parameter = Some(value_str.to_string());
            }
            "FallbackValue" => spec.fallback = Some(value_str.to_string()),
            "RelativeSource" => {
                let mode = match value {
                    bevy_pf_xaml::markup::MarkupValue::Extension(inner) => inner
                        .first_positional_str()
                        .map(|m| m.to_string())
                        .or_else(|| {
                            inner.arg("Mode").and_then(|m| m.as_str()).map(String::from)
                        }),
                    _ => Some(value_str.to_string()),
                };
                match mode.as_deref() {
                    Some("Self") => spec.relative = Some(PfRelativeSource::SelfSource),
                    Some("TemplatedParent") => {
                        spec.relative = Some(PfRelativeSource::TemplatedParent)
                    }
                    other => {
                        if spec.unsupported.is_none() {
                            spec.unsupported =
                                Some(format!("RelativeSource {}", other.unwrap_or("?")));
                        }
                    }
                }
            }
            // Harmless to ignore.
            "UpdateSourceTrigger" | "TargetNullValue" => {}
            other => {
                if spec.unsupported.is_none() {
                    spec.unsupported = Some(other.to_string());
                }
            }
        }
    }
    spec
}

/// Read a bindable property from another element (`ElementName` sources).
pub fn read_element_value(world: &World, source: Entity, path: &str) -> Option<BoundValue> {
    match path {
        "Value" => {
            if let Some(v) = world.get::<bevy::ui_widgets::SliderValue>(source) {
                return Some(BoundValue::Num(v.0 as f64));
            }
            world
                .get::<crate::components::PfProgress>(source)
                .map(|p| BoundValue::Num(p.value as f64))
        }
        "IsChecked" => Some(BoundValue::Bool(world.get::<Checked>(source).is_some())),
        "Text" => {
            if let Some(t) = world.get::<bevy::ui::widget::Text>(source) {
                return Some(BoundValue::Str(t.0.clone()));
            }
            if let Some(et) = world.get::<bevy::text::EditableText>(source) {
                return Some(BoundValue::Str(et.editor().text().to_string()));
            }
            // TextBox root: the EditableText lives on the input child.
            let children = world.get::<Children>(source)?;
            for child in children.iter() {
                if let Some(et) = world.get::<bevy::text::EditableText>(child) {
                    return Some(BoundValue::Str(et.editor().text().to_string()));
                }
            }
            None
        }
        "ActualWidth" => world
            .get::<bevy::ui::ComputedNode>(source)
            .map(|c| BoundValue::Num(c.size().x as f64)),
        "ActualHeight" => world
            .get::<bevy::ui::ComputedNode>(source)
            .map(|c| BoundValue::Num(c.size().y as f64)),
        _ => None,
    }
}

/// Find the effective data context for an entity (self or nearest ancestor).
/// Follows [`crate::components::PfLogicalParent`] links where present, so
/// popup content reparented under the overlay root still inherits from its
/// logical owner.
pub fn find_context(world: &World, mut entity: Entity) -> Option<Bindable> {
    loop {
        if let Some(ctx) = world.get::<DataContext>(entity) {
            return Some(ctx.0.clone());
        }
        entity = match world.get::<crate::components::PfLogicalParent>(entity) {
            Some(l) => l.0,
            None => world.get::<ChildOf>(entity)?.parent(),
        };
    }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Apply source values to binding targets when the source version changes.
/// Exclusive: targets span many component types.
pub(crate) fn apply_bindings(world: &mut World) {
    let mut query = world.query_filtered::<Entity, With<PfBindings>>();
    let entities: Vec<Entity> = query.iter(world).collect();

    for entity in entities {
        let ctx = find_context(world, entity);

        // Collect the updates to run for this entity.
        let mut updates: Vec<(usize, PfBinding)> = Vec::new();
        {
            let Some(bindings) = world.get::<PfBindings>(entity) else {
                continue;
            };
            for (i, b) in bindings.0.iter().enumerate() {
                if !b.is_to_target() {
                    continue;
                }
                match &b.source {
                    PfBindingSource::DataContext => {
                        // WPF OneTime: applied once, then frozen forever.
                        if b.mode == v::BindingMode::OneTime && b.seen_version != 0 {
                            continue;
                        }
                        if let Some(ctx) = &ctx
                            && b.seen_version != ctx.version() {
                                updates.push((i, b.clone()));
                            }
                    }
                    // Element sources have no version counter; re-evaluate
                    // every frame (applies are no-ops when values match).
                    PfBindingSource::Element(_) => updates.push((i, b.clone())),
                    PfBindingSource::Named(_) => {} // unresolved; skip
                }
            }
        }

        for (index, binding) in updates {
            let (value, mark_version) = match &binding.source {
                PfBindingSource::DataContext => {
                    let ctx = ctx.as_ref().expect("checked above");
                    (ctx.read_path(&binding.path), Some(ctx.version()))
                }
                PfBindingSource::Element(source) => {
                    (read_element_value(world, *source, &binding.path), None)
                }
                PfBindingSource::Named(_) => (None, None),
            };
            // Converter, then FallbackValue when the source (or the
            // converter) yields nothing.
            let value = match (value, &binding.converter) {
                (Some(v), Some(key)) => {
                    let conv = world
                        .get_resource::<PfConverters>()
                        .and_then(|c| c.0.get(key).cloned());
                    match conv {
                        Some(c) => c.convert(&v, binding.converter_parameter.as_deref()),
                        None => {
                            warn!("bevy_pf: converter `{key}` is not registered");
                            None
                        }
                    }
                }
                (v, _) => v,
            };
            let value = value.or_else(|| binding.fallback.clone().map(BoundValue::Str));
            match value {
                Some(value) => apply_binding_value(world, entity, &binding, &value),
                None => {
                    if mark_version.is_some() && binding.fallback.is_none() {
                        warn!(
                            "bevy_pf: binding path `{}` not found on data context",
                            binding.path
                        );
                    }
                }
            }
            if let Some(version) = mark_version {
                // Mark seen (also for misses, so we do not spam every frame).
                // OneTime bindings freeze on a nonzero version sentinel.
                let seen = if binding.mode == v::BindingMode::OneTime {
                    version.max(1)
                } else {
                    version
                };
                if let Some(mut bindings) = world.get_mut::<PfBindings>(entity) {
                    bindings.0[index].seen_version = seen;
                }
            }
        }
    }
}

fn apply_binding_value(world: &mut World, entity: Entity, binding: &PfBinding, value: &BoundValue) {
    use bevy::ui::widget::Text;
    match binding.target {
        BindingTarget::Text => {
            let text = binding.format(value);
            if let Some(mut t) = world.get_mut::<Text>(entity)
                && t.0 != text {
                    t.0 = text;
                }
        }
        BindingTarget::EditableText => {
            let text = binding.format(value);
            if let Some(mut et) = world.get_mut::<bevy::text::EditableText>(entity)
                && et.editor().text() != text.as_str() {
                    et.editor.set_text(&text);
                }
        }
        BindingTarget::IsChecked => {
            let Some(b) = value.as_bool() else { return };
            let has = world.get::<Checked>(entity).is_some();
            if b && !has {
                world.entity_mut(entity).insert(Checked);
            } else if !b && has {
                world.entity_mut(entity).remove::<Checked>();
            }
        }
        BindingTarget::SliderValue => {
            let Some(n) = value.as_f64() else { return };
            let current = world
                .get::<bevy::ui_widgets::SliderValue>(entity)
                .map(|v| v.0);
            if current != Some(n as f32) {
                world
                    .entity_mut(entity)
                    .insert(bevy::ui_widgets::SliderValue(n as f32));
            }
        }
        BindingTarget::ProgressValue => {
            let Some(n) = value.as_f64() else { return };
            if let Some(mut p) = world.get_mut::<crate::components::PfProgress>(entity)
                && p.value != n as f32 {
                    p.value = n as f32;
                }
        }
        BindingTarget::Visibility => {
            let vis = match value {
                BoundValue::Bool(true) => v::Visibility::Visible,
                BoundValue::Bool(false) => v::Visibility::Collapsed,
                other => other
                    .to_display()
                    .parse::<v::Visibility>()
                    .unwrap_or(v::Visibility::Visible),
            };
            let (visibility, display) = convert::visibility(vis);
            world.entity_mut(entity).insert(visibility);
            if let Some(mut node) = world.get_mut::<Node>(entity) {
                node.display = display.unwrap_or(Display::DEFAULT);
            }
        }
        BindingTarget::Width | BindingTarget::Height => {
            let Some(n) = value.as_f64() else { return };
            if let Some(mut node) = world.get_mut::<Node>(entity) {
                if binding.target == BindingTarget::Width {
                    node.width = Val::Px(n as f32);
                } else {
                    node.height = Val::Px(n as f32);
                }
            }
        }
        BindingTarget::FontSize => {
            let Some(n) = value.as_f64() else { return };
            if let Some(mut font) = world.get_mut::<bevy::text::TextFont>(entity) {
                font.font_size = bevy::text::FontSize::Px(n as f32);
            }
        }
        BindingTarget::Foreground => {
            if let Ok(color) = value.to_display().parse::<v::PfColor>() {
                world
                    .entity_mut(entity)
                    .insert(bevy::text::TextColor(convert::color(color)));
            }
        }
        BindingTarget::Background => {
            if let Ok(color) = value.to_display().parse::<v::PfColor>() {
                world
                    .entity_mut(entity)
                    .insert(BackgroundColor(convert::color(color)));
            }
        }
    }
}

/// TwoWay: TextBox edits flow back to the source.
pub(crate) fn textbox_write_back(
    inputs: Query<
        (Entity, Ref<bevy::text::EditableText>, &PfBindings),
        Changed<bevy::text::EditableText>,
    >,
    parents: Query<&ChildOf>,
    contexts: Query<&DataContext>,
) {
    for (entity, editable, bindings) in &inputs {
        // A freshly spawned input hasn't received its initial bound value
        // yet; writing back now would clobber the source with "".
        if editable.is_added() {
            continue;
        }
        for binding in &bindings.0 {
            if binding.target != BindingTarget::EditableText
                || !binding.is_to_source()
                || binding.source != PfBindingSource::DataContext
            {
                continue;
            }
            let Some(ctx) = find_context_via_queries(entity, &parents, &contexts) else {
                continue;
            };
            let text = editable.editor().text().to_string();
            // Skip the echo: only write when the value actually differs.
            if ctx.read_path(&binding.path).map(|b| b.to_display()) != Some(text.clone()) {
                ctx.write_path(&binding.path, &BoundValue::Str(text));
            }
        }
    }
}

/// TwoWay: `Checked` add/remove flows back to the source.
pub(crate) fn checked_write_back(
    added: Query<Entity, (Added<Checked>, With<PfBindings>)>,
    mut removed: RemovedComponents<Checked>,
    bindings_q: Query<&PfBindings>,
    parents: Query<&ChildOf>,
    contexts: Query<&DataContext>,
) {
    let write = |entity: Entity, value: bool| {
        let Ok(bindings) = bindings_q.get(entity) else {
            return;
        };
        for binding in &bindings.0 {
            if binding.target != BindingTarget::IsChecked
                || !binding.is_to_source()
                || binding.source != PfBindingSource::DataContext
            {
                continue;
            }
            if let Some(ctx) = find_context_via_queries(entity, &parents, &contexts)
                && ctx.read_path(&binding.path).and_then(|b| b.as_bool()) != Some(value) {
                    ctx.write_path(&binding.path, &BoundValue::Bool(value));
                }
        }
    };
    for entity in &added {
        write(entity, true);
    }
    for entity in removed.read() {
        write(entity, false);
    }
}

/// TwoWay: slider drags flow back to the source.
pub(crate) fn slider_write_back(
    sliders: Query<
        (Entity, &bevy::ui_widgets::SliderValue, &PfBindings),
        Changed<bevy::ui_widgets::SliderValue>,
    >,
    parents: Query<&ChildOf>,
    contexts: Query<&DataContext>,
) {
    for (entity, value, bindings) in &sliders {
        for binding in &bindings.0 {
            if binding.target != BindingTarget::SliderValue
                || !binding.is_to_source()
                || binding.source != PfBindingSource::DataContext
            {
                continue;
            }
            if let Some(ctx) = find_context_via_queries(entity, &parents, &contexts) {
                let current = ctx.read_path(&binding.path).and_then(|b| b.as_f64());
                if current != Some(value.0 as f64) {
                    ctx.write_path(&binding.path, &BoundValue::Num(value.0 as f64));
                }
            }
        }
    }
}

fn find_context_via_queries(
    mut entity: Entity,
    parents: &Query<&ChildOf>,
    contexts: &Query<&DataContext>,
) -> Option<Bindable> {
    // Note: write-back systems run on entities instantiated in place (no
    // logical reparenting between a TextBox and its context), so the plain
    // entity-tree walk suffices here.
    loop {
        if let Ok(ctx) = contexts.get(entity) {
            return Some(ctx.0.clone());
        }
        entity = parents.get(entity).ok()?.parent();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Reflect, Default)]
    struct TestVm {
        name: String,
        score: u32,
        health: f32,
        alive: bool,
        nested: Nested,
    }

    #[derive(Reflect, Default)]
    struct Nested {
        label: String,
    }

    #[test]
    fn bindable_read_paths() {
        let vm = Bindable::new(TestVm {
            name: "Ada".into(),
            score: 42,
            health: 0.5,
            alive: true,
            nested: Nested {
                label: "deep".into(),
            },
        });
        assert_eq!(vm.read_path("name"), Some(BoundValue::Str("Ada".into())));
        assert_eq!(vm.read_path("score"), Some(BoundValue::Num(42.0)));
        assert_eq!(vm.read_path("alive"), Some(BoundValue::Bool(true)));
        assert_eq!(
            vm.read_path("nested.label"),
            Some(BoundValue::Str("deep".into()))
        );
        assert!(vm.read_path("missing").is_none());
    }

    #[test]
    fn bindable_update_bumps_version() {
        let vm = Bindable::new(TestVm::default());
        let v1 = vm.version();
        vm.update(|m: &mut TestVm| m.score = 7);
        assert!(vm.version() > v1);
        assert_eq!(vm.read_path("score"), Some(BoundValue::Num(7.0)));
    }

    #[test]
    fn bindable_write_paths_with_coercion() {
        let vm = Bindable::new(TestVm::default());
        assert!(vm.write_path("name", &BoundValue::Str("Bob".into())));
        assert!(vm.write_path("score", &BoundValue::Num(9.0)));
        assert!(vm.write_path("health", &BoundValue::Str("0.25".into())));
        assert!(vm.write_path("alive", &BoundValue::Bool(true)));
        assert_eq!(vm.read_path("name"), Some(BoundValue::Str("Bob".into())));
        assert_eq!(vm.read_path("score"), Some(BoundValue::Num(9.0)));
        assert_eq!(vm.read_path("health"), Some(BoundValue::Num(0.25)));
        assert_eq!(vm.read_path("alive"), Some(BoundValue::Bool(true)));
    }

    #[test]
    fn bound_value_display() {
        assert_eq!(BoundValue::Num(42.0).to_display(), "42");
        assert_eq!(BoundValue::Num(1.5).to_display(), "1.5");
        assert_eq!(BoundValue::Bool(true).to_display(), "True");
    }

    #[test]
    fn binding_string_format() {
        let b = PfBinding {
            target: BindingTarget::Text,
            source: PfBindingSource::DataContext,
            path: "score".into(),
            mode: v::BindingMode::OneWay,
            string_format: Some("Score: {0} pts".into()),
            converter: None,
            converter_parameter: None,
            fallback: None,
            seen_version: 0,
        };
        assert_eq!(b.format(&BoundValue::Num(3.0)), "Score: 3 pts");
    }
}

/// Written whenever a `Command=` control is activated. If the DataContext's
/// `Bindable` has a matching registered command it runs first; the message
/// is written either way, so apps can also handle commands centrally.
#[derive(bevy::ecs::message::Message, Debug, Clone)]
pub struct PfCommandInvoked {
    pub source: Entity,
    pub command: String,
    pub parameter: Option<String>,
}

/// How a `CommandParameter=` resolves at activation time.
#[derive(Debug, Clone)]
pub enum PfCommandParameter {
    Literal(String),
    /// `{Binding path}` against the activation-time DataContext.
    Path(String),
}

/// Activate a command control: resolve the parameter, run the registered
/// command (if any), and write [`PfCommandInvoked`].
pub fn invoke_command(
    world: &mut World,
    source: Entity,
    name: &str,
    parameter: Option<&PfCommandParameter>,
) {
    let ctx = find_context(world, source);
    let parameter = parameter.and_then(|p| match p {
        PfCommandParameter::Literal(s) => Some(s.clone()),
        PfCommandParameter::Path(path) => ctx
            .as_ref()
            .and_then(|c| c.read_path(path))
            .map(|v| v.to_display()),
    });
    if let Some(cmd) = ctx.as_ref().and_then(|c| c.command(name)) {
        cmd(world, parameter.clone());
    }
    world.write_message(PfCommandInvoked {
        source,
        command: name.to_string(),
        parameter,
    });
}
