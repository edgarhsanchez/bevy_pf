//! Data binding: `{Binding Path=...}` against a reflected view-model.
//!
//! The WPF pattern maps onto Rust like this:
//!
//! - **View-model**: any `#[derive(Reflect)]` struct wrapped in a [`Bindable`]
//!   (the `INotifyPropertyChanged` analog). Mutations notify bindings: a
//!   whole-model [`Bindable::update`] re-applies everything, a
//!   [`Bindable::update_named`] (or a `#[derive(Bindable)]`-generated
//!   equality-checked setter) re-applies only bindings whose path overlaps
//!   the change — WPF's `OnPropertyChanged(name)`.
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
    commands:
        std::sync::RwLock<bevy::platform::collections::HashMap<String, std::sync::Arc<CommandFn>>>,
    /// Change log: one `(version, path)` entry per mutation; `None` path
    /// means "anything may have changed" (whole-model [`Bindable::update`]).
    /// Capped at [`CHANGE_LOG_CAP`]; consumers whose last-seen version has
    /// been truncated out fall back to everything-changed.
    changes: std::sync::RwLock<std::collections::VecDeque<(u64, Option<Arc<str>>)>>,
}

/// Entries retained in the per-model change log. Old entries fall off; a
/// binding that has not been applied for longer than this many mutations is
/// conservatively re-applied.
const CHANGE_LOG_CAP: usize = 64;

/// Do two reflection paths (from the model root) read/write overlapping
/// data? True when equal, or when one extends the other at a `.`/`[`
/// boundary — `user` overlaps `user.name`, `score` does not overlap
/// `scoreboard`. The empty path is the whole model and overlaps everything.
fn paths_overlap(a: &str, b: &str) -> bool {
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    if short.is_empty() {
        return true;
    }
    if !long.starts_with(short) {
        return false;
    }
    long.len() == short.len() || matches!(long.as_bytes()[short.len()], b'.' | b'[')
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
                changes: Default::default(),
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
    ///
    /// The mutation is logged as "anything may have changed", so every
    /// binding on the context re-applies. When the closure touches one
    /// property, [`Self::update_named`] keeps the invalidation to bindings
    /// that actually read it — WPF's `OnPropertyChanged(name)`.
    pub fn update<T: Reflect, R>(&self, f: impl FnOnce(&mut T) -> R) -> Option<R> {
        let mut guard = self.inner.value.write().unwrap();
        let target = guard.as_any_mut().downcast_mut::<T>()?;
        let result = f(target);
        self.log_change(None);
        Some(result)
    }

    /// Mutate one property and notify only the bindings that read it.
    /// `path` is relative to this view's scope (like [`Self::read_path`]);
    /// the closure still receives the ROOT model. The closure returns
    /// whether it changed anything — `false` skips the notify entirely,
    /// which is what makes equality-checked setters cheap.
    pub fn update_named<T: Reflect>(
        &self,
        path: &str,
        f: impl FnOnce(&mut T) -> bool,
    ) -> Option<bool> {
        let changed = {
            let mut guard = self.inner.value.write().unwrap();
            let target = guard.as_any_mut().downcast_mut::<T>()?;
            f(target)
        };
        if changed {
            self.log_change(Some(self.full_path(path).into()));
        }
        Some(changed)
    }

    /// Record a mutation done outside [`Self::update`]/[`Self::update_named`]
    /// (e.g. through interior mutability): notify bindings reading `path`.
    pub fn notify(&self, path: &str) {
        self.log_change(Some(self.full_path(path).into()));
    }

    fn log_change(&self, path: Option<Arc<str>>) {
        let version = self.inner.version.fetch_add(1, Ordering::AcqRel) + 1;
        let mut log = self.inner.changes.write().unwrap();
        if log.len() == CHANGE_LOG_CAP {
            log.pop_front();
        }
        log.push_back((version, path));
    }

    /// Has anything a binding with these relative `paths` reads changed
    /// since `seen`? Conservative: unnamed mutations, a truncated log, and
    /// `seen == 0` (never applied) all answer yes.
    pub(crate) fn changed_since<'a>(
        &self,
        seen: u64,
        paths: impl Iterator<Item = &'a str> + Clone,
    ) -> bool {
        if seen == 0 || self.version() <= seen {
            return seen == 0;
        }
        let log = self.inner.changes.read().unwrap();
        // The log no longer reaches back to `seen`: assume changed.
        match log.front() {
            Some((oldest, _)) if *oldest <= seen + 1 => {}
            _ => return true,
        }
        for (version, path) in log.iter() {
            if *version <= seen {
                continue;
            }
            match path {
                None => return true,
                Some(p) => {
                    if paths
                        .clone()
                        .any(|rel| paths_overlap(p, &self.full_path(rel)))
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Read the model through a typed closure WITHOUT bumping the version.
    ///
    /// The mutating [`Self::update`] bumps the change version even when the
    /// closure only looks, which forces every binding on the context through a
    /// re-apply. A host system that polls the model each frame (e.g. mirroring
    /// control write-backs into an app resource) must use this instead, or the
    /// poll itself keeps the bindings permanently dirty.
    pub fn read<T: Reflect, R>(&self, f: impl FnOnce(&T) -> R) -> Option<R> {
        let guard = self.inner.value.read().unwrap();
        let target = guard.as_any().downcast_ref::<T>()?;
        Some(f(target))
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

    /// The short type name (ident) of the value at a path — what WPF's
    /// DataType-implicit template selection matches against.
    pub fn type_ident(&self, path: &str) -> Option<String> {
        let full = self.full_path(path);
        let guard = self.inner.value.read().unwrap();
        let reflected: &dyn PartialReflect = if full.is_empty() {
            guard.as_partial_reflect()
        } else {
            guard.reflect_path(full.as_str()).ok()?
        };
        let info = reflected.get_represented_type_info()?;
        Some(
            info.type_path_table()
                .ident()
                .unwrap_or_else(|| info.type_path())
                .to_string(),
        )
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

    /// Which element of `list_path` is the item at `value_path`?
    ///
    /// This is what makes `SelectedItem` real. The bound item never leaves
    /// the model — both paths resolve against the SAME view model under one
    /// read guard, so the question "which row is this?" is answered where
    /// the objects actually live, by comparing them, rather than by
    /// comparing rendered text or trusting an index.
    ///
    /// `preferred` is tried before the scan. That is WPF's `CoerceSelectedItem`
    /// fast path, and it is what makes DUPLICATES behave: clicking the second
    /// of two equal rows keeps the second selected instead of snapping to the
    /// first.
    pub fn selection_match(
        &self,
        list_path: &str,
        value_path: &str,
        preferred: Option<usize>,
    ) -> SelectionMatch {
        use bevy::reflect::ReflectRef;
        let guard = self.inner.value.read().unwrap();
        let resolve = |full: String| -> Option<&dyn PartialReflect> {
            if full.is_empty() {
                Some(guard.as_partial_reflect())
            } else {
                guard.reflect_path(full.as_str()).ok()
            }
        };

        let Some(mut value) = resolve(self.full_path(value_path)) else {
            return SelectionMatch::NotFound;
        };
        // Option<T>: None IS a selection — "nothing" — not a failure.
        if let ReflectRef::Enum(e) = value.reflect_ref()
            && e.reflect_type_path().starts_with("core::option::Option")
        {
            match e.field_at(0) {
                Some(inner) => value = inner,
                None => return SelectionMatch::Null,
            }
        }

        let Some(list) = resolve(self.full_path(list_path)) else {
            return SelectionMatch::NotFound;
        };
        let items: Vec<&dyn PartialReflect> = match list.reflect_ref() {
            ReflectRef::List(l) => (0..l.len()).filter_map(|i| l.get(i)).collect(),
            ReflectRef::Array(a) => (0..a.len()).filter_map(|i| a.get(i)).collect(),
            _ => return SelectionMatch::NotFound,
        };
        let len = items.len();
        if len == 0 {
            // Nothing to match against YET. Clearing here would discard a
            // selection set before the collection arrived.
            return SelectionMatch::Pending;
        }

        let mut undecidable = false;
        let mut compare = |i: usize| -> bool {
            match items.get(i).and_then(|item| items_equal(*item, value)) {
                Some(hit) => hit,
                None => {
                    undecidable = true;
                    false
                }
            }
        };
        if let Some(p) = preferred
            && p < len
            && compare(p)
        {
            return SelectionMatch::Index(p);
        }
        for i in 0..len {
            if compare(i) {
                return SelectionMatch::Index(i);
            }
        }
        if undecidable {
            warn!(
                "SelectedItem: `{}` items do not support reflected equality, so the \
                 selection cannot be located; give the type #[derive(PartialEq)] or \
                 bind SelectedValue with a SelectedValuePath instead",
                list.reflect_type_path()
            );
        }
        SelectionMatch::NotFound
    }

    /// Copy the value at `src_path` into `dest_path` — the write half of
    /// `SelectedItem`, which moves an OBJECT rather than a scalar.
    ///
    /// The element is snapshotted with `to_dynamic()` first: that ends the
    /// shared borrow so the destination can be taken mutably from the same
    /// guard, and it preserves the represented type so the type gate in
    /// [`items_equal`] still recognises it afterwards.
    pub fn set_from(&self, dest_path: &str, src_path: &str) -> bool {
        use bevy::reflect::ReflectRef;
        use bevy::reflect::enums::{DynamicEnum, DynamicVariant};
        let dest_full = self.full_path(dest_path);
        let src_full = self.full_path(src_path);
        let mut guard = self.inner.value.write().unwrap();

        let Ok(source) = guard.reflect_path(src_full.as_str()) else {
            return false;
        };
        let snapshot = source.to_dynamic();

        let Ok(target) = guard.reflect_path_mut(dest_full.as_str()) else {
            return false;
        };
        // An Option destination takes Some(item), not the bare item.
        let wraps_option = matches!(
            target.reflect_ref(),
            ReflectRef::Enum(e) if e.reflect_type_path().starts_with("core::option::Option")
        );
        let applied = if wraps_option {
            let mut tuple = bevy::reflect::tuple::DynamicTuple::default();
            tuple.insert_boxed(snapshot);
            target
                .try_apply(&DynamicEnum::new("Some", DynamicVariant::Tuple(tuple)))
                .is_ok()
        } else {
            target.try_apply(snapshot.as_ref()).is_ok()
        };
        if !applied {
            return false;
        }
        drop(guard);
        self.log_change(Some(dest_full.into()));
        true
    }

    /// Clear an `Option` destination — "nothing is selected".
    pub fn set_null(&self, dest_path: &str) -> bool {
        use bevy::reflect::ReflectRef;
        use bevy::reflect::enums::{DynamicEnum, DynamicVariant};
        let full = self.full_path(dest_path);
        let mut guard = self.inner.value.write().unwrap();
        let Ok(target) = guard.reflect_path_mut(full.as_str()) else {
            return false;
        };
        if !matches!(
            target.reflect_ref(),
            ReflectRef::Enum(e) if e.reflect_type_path().starts_with("core::option::Option")
        ) {
            warn!("SelectedItem: `{full}` is not an Option, so it cannot be cleared");
            return false;
        }
        if target
            .try_apply(&DynamicEnum::new("None", DynamicVariant::Unit))
            .is_err()
        {
            return false;
        }
        drop(guard);
        self.log_change(Some(full.into()));
        true
    }

    /// Write a value at a reflection path (two-way bindings), then notify
    /// the bindings that read that path.
    pub fn write_path(&self, path: &str, value: &BoundValue) -> bool {
        let full = self.full_path(path);
        let mut guard = self.inner.value.write().unwrap();
        let Ok(target) = guard.reflect_path_mut(full.as_str()) else {
            return false;
        };
        if !bound_to_reflect(target, value) {
            return false;
        }
        drop(guard);
        self.log_change(Some(full.into()));
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

/// A XAML-authored re-scope of the inherited context:
/// `DataContext="{Binding path}"`. The element and its descendants see the
/// nearest ancestor context narrowed to `path` (via [`Bindable::at`]).
/// Stored as a marker and resolved lazily in [`find_context`] because the
/// actual view-model usually attaches from Rust after instantiation. A real
/// [`DataContext`] component on the same entity wins outright — it IS a
/// context, not a modifier of one.
#[derive(Component, Clone, Debug)]
pub struct DataContextScope(pub String);

// ---------------------------------------------------------------------------
// Bound values
// ---------------------------------------------------------------------------

/// A value read from / written to a view-model.
#[derive(Debug, Clone, PartialEq)]
pub enum BoundValue {
    Str(String),
    Num(f64),
    Bool(bool),
    /// An `Option` field that is `None` — what WPF calls a null source
    /// value; `TargetNullValue=` substitutes for it at the target.
    Null,
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
            BoundValue::Null => String::new(),
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            BoundValue::Num(n) => Some(*n),
            BoundValue::Str(s) => s.trim().parse().ok(),
            BoundValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            BoundValue::Null => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            BoundValue::Bool(b) => Some(*b),
            BoundValue::Str(s) => Some(s.trim().eq_ignore_ascii_case("true")),
            BoundValue::Num(n) => Some(*n != 0.0),
            BoundValue::Null => Some(false),
        }
    }
}

/// The result of locating a bound item inside an items collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMatch {
    /// The item is element `n` of the collection.
    Index(usize),
    /// The bound property is `None` — an explicit "nothing selected".
    Null,
    /// A value that is not in the collection. WPF clears the selection.
    NotFound,
    /// The collection is not there yet (empty, or items not generated).
    /// Distinct from `NotFound`: the caller must RETRY rather than clear,
    /// or a selection set before the list arrives is silently discarded.
    Pending,
}

/// Are these two reflected values the same item?
///
/// Structural equality, gated on the represented type. The gate is not
/// optional: bevy's `struct_partial_eq` compares field NAMES and values
/// only, so without it a `Player { name }` would equal an `Enemy { name }`.
/// WPF's own comparison is likewise type-paranoid.
///
/// `None` means "cannot say" — a type whose `reflect_partial_eq` is the
/// trait default. It is never coerced to `true`, because a false positive
/// here silently selects the wrong row.
fn items_equal(a: &dyn PartialReflect, b: &dyn PartialReflect) -> Option<bool> {
    if let (Some(ta), Some(tb)) = (a.get_represented_type_info(), b.get_represented_type_info())
        && ta.type_id() != tb.type_id()
    {
        return Some(false);
    }
    a.reflect_partial_eq(b)
}

fn reflect_to_bound(value: &dyn PartialReflect) -> BoundValue {
    // Option<T>: None is a null source value; Some unwraps to the inner.
    if let bevy::reflect::ReflectRef::Enum(e) = value.reflect_ref()
        && e.reflect_type_path().starts_with("core::option::Option")
    {
        return match e.field_at(0) {
            Some(inner) => reflect_to_bound(inner),
            None => BoundValue::Null,
        };
    }
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
        let Some(b) = value.as_bool() else {
            return false;
        };
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
    /// ListBox/ComboBox `SelectedIndex` (TwoWay: selection writes back).
    SelectedIndex,
    /// ListBox/ComboBox `SelectedItem` — the ITEM, not its position.
    ///
    /// Located by comparing the bound value against the collection's
    /// elements inside the view model (see [`Bindable::selection_match`]):
    /// structural equality, gated on the represented type, first match wins,
    /// with the currently-selected row preferred so duplicates behave. A
    /// value that is not in the collection CLEARS the selection, as WPF
    /// does; `None` clears it too, and an empty collection is retried rather
    /// than treated as "not there".
    SelectedItem,
    /// ListBox/ComboBox `SelectedValue` — a KEY read out of each item via
    /// `SelectedValuePath`, for models that identify rows by id rather than
    /// by value.
    SelectedValue,
    /// ProgressBar value.
    ProgressValue,
    /// `Visibility` (accepts "Visible/Hidden/Collapsed" strings or booleans).
    Visibility,
    Width,
    Height,
    FontSize,
    Foreground,
    Background,
    /// `BorderBrush`. Only solid colours: a bound value is a string, and a
    /// gradient has no string form in WPF's converter grammar. Applies to all
    /// four sides — per-side border brushes are not a WPF concept either
    /// (`BorderThickness` varies per side, `BorderBrush` does not).
    BorderBrush,
    /// A `Shape`'s `Stroke`. The shape counterpart of [`Self::BorderBrush`],
    /// and the only way to give each item of a bound list its own outline
    /// colour.
    Stroke,
    /// A `Shape`'s `Fill`. The shape counterpart of [`Self::Background`].
    Fill,
    /// Any other store-managed property (Margin, Padding, BorderThickness,
    /// CornerRadius, Opacity, ...): the value lands in the property store's
    /// `Local` tier, so styles, triggers, and animations keep their WPF
    /// precedence around it. OneWay only — nothing writes these back.
    Store(crate::provider::PropertyTarget),
}

/// Where a binding reads its value from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PfBindingSource {
    /// The inherited `DataContext` (default).
    DataContext,
    /// Another element, by `x:Name` — not yet resolved to an entity.
    Named(String),
    /// `RelativeSource FindAncestor`: resolved to `Element` after the tree
    /// is built by walking `ChildOf` for a matching element kind.
    Ancestor(String),
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
    pub target_null: Option<String>,
    /// MultiBinding member paths (empty = a normal single-path binding).
    pub multi_paths: Vec<String>,
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

/// `{N[:spec]}` positional formatting over several values (MultiBinding
/// StringFormat). Reuses the single-value spec formatter per slot.
fn apply_multi_format(fmt: &str, values: &[BoundValue]) -> String {
    let mut out = String::with_capacity(fmt.len() + 8 * values.len());
    let mut rest = fmt;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            out.push_str(rest);
            return out;
        };
        let body = &after[..end];
        let (index, spec) = match body.split_once(':') {
            Some((i, sp)) => (i, sp),
            None => (body, ""),
        };
        match index.parse::<usize>() {
            Ok(i) => {
                out.push_str(&rest[..start]);
                let raw = values.get(i).map(|v| v.to_display()).unwrap_or_default();
                out.push_str(&format_with_spec(spec, &raw));
            }
            // `{}` escape prefix and friends: emit literally.
            Err(_) => out.push_str(&rest[..start + 1 + end + 1]),
        }
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
            BoundValue::Null => false,
        };
        Some(BoundValue::Str(
            if b { "Visible" } else { "Collapsed" }.to_string(),
        ))
    }
}

/// The `IMultiValueConverter` analog: combine several bound values.
pub trait PfMultiValueConverter: Send + Sync + 'static {
    fn convert(&self, values: &[BoundValue], parameter: Option<&str>) -> Option<BoundValue>;
}

/// Registered multi-value converters, by `Converter={StaticResource name}`.
#[derive(Resource, Default)]
pub struct PfMultiConverters(
    pub bevy::platform::collections::HashMap<String, std::sync::Arc<dyn PfMultiValueConverter>>,
);

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

    /// Register an `IMultiValueConverter` analog for `<MultiBinding
    /// Converter={StaticResource name}>`.
    fn register_multi_converter(
        &mut self,
        name: impl Into<String>,
        converter: impl PfMultiValueConverter + 'static,
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

    fn register_multi_converter(
        &mut self,
        name: impl Into<String>,
        converter: impl PfMultiValueConverter + 'static,
    ) -> &mut Self {
        let world = self.world_mut();
        if !world.contains_resource::<PfMultiConverters>() {
            world.init_resource::<PfMultiConverters>();
        }
        world
            .resource_mut::<PfMultiConverters>()
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
    /// `TargetNullValue=` substituted when the source resolves to null
    /// (an `Option` field that is `None`).
    pub target_null: Option<String>,
    /// MultiBinding: the child `<Binding Path=...>` paths (empty = normal).
    pub multi_paths: Vec<String>,
    /// `RelativeSource={RelativeSource Self|TemplatedParent}`.
    pub relative: Option<PfRelativeSource>,
    /// The first unsupported argument encountered (`Source`, `Converter`,
    /// `RelativeSource`, ...), if any — such bindings are skipped with a
    /// warning.
    pub unsupported: Option<String>,
}

/// `RelativeSource=` modes supported so far (AncestorType is tracked work).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PfRelativeSource {
    SelfSource,
    TemplatedParent,
    /// `FindAncestor` with `AncestorType`: nearest ancestor of the kind.
    Ancestor(String),
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
        target_null: None,
        multi_paths: Vec::new(),
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
                    bevy_pf_xaml::markup::MarkupValue::Extension(inner) => {
                        inner.first_positional_str().map(|k| k.to_string())
                    }
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
                        .or_else(|| inner.arg("Mode").and_then(|m| m.as_str()).map(String::from)),
                    _ => Some(value_str.to_string()),
                };
                let ancestor_type = match value {
                    bevy_pf_xaml::markup::MarkupValue::Extension(inner) => {
                        inner.arg("AncestorType").and_then(|at| match at {
                            bevy_pf_xaml::markup::MarkupValue::Extension(t) => t
                                .first_positional_str()
                                .map(|n| n.rsplit(':').next().unwrap_or(n).to_string()),
                            other => other
                                .as_str()
                                .map(|n| n.rsplit(':').next().unwrap_or(n).to_string()),
                        })
                    }
                    _ => None,
                };
                match mode.as_deref() {
                    Some("Self") => spec.relative = Some(PfRelativeSource::SelfSource),
                    Some("TemplatedParent") => {
                        spec.relative = Some(PfRelativeSource::TemplatedParent)
                    }
                    Some("FindAncestor") => match ancestor_type {
                        Some(ty) => spec.relative = Some(PfRelativeSource::Ancestor(ty)),
                        None => {
                            if spec.unsupported.is_none() {
                                spec.unsupported =
                                    Some("FindAncestor without AncestorType".to_string());
                            }
                        }
                    },
                    other => {
                        if spec.unsupported.is_none() {
                            spec.unsupported =
                                Some(format!("RelativeSource {}", other.unwrap_or("?")));
                        }
                    }
                }
            }
            "TargetNullValue" => spec.target_null = Some(value_str.to_string()),
            // Harmless to ignore.
            "UpdateSourceTrigger" => {}
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
    // `DataContextScope`s encountered on the way up narrow the context found
    // above them, outermost first: ctx.at(outer_scope).at(inner_scope).
    let mut scopes: Vec<&str> = Vec::new();
    loop {
        if let Some(ctx) = world.get::<DataContext>(entity) {
            let mut ctx = ctx.0.clone();
            for path in scopes.into_iter().rev() {
                ctx = ctx.at(path);
            }
            return Some(ctx);
        }
        if let Some(scope) = world.get::<DataContextScope>(entity) {
            scopes.push(scope.0.as_str());
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
        // Bindings whose reads are untouched by the change log: no re-apply,
        // just advance seen_version so the log scan stays in-window.
        let mut fast_forward: Vec<usize> = Vec::new();
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
                            && b.seen_version != ctx.version()
                        {
                            // A selection binding also depends on the
                            // COLLECTION: after a rebuild the item must
                            // be re-located, or the selection is left
                            // pointing at a despawned row. This is WPF's
                            // OnItemsChanged -> CoerceValue(SelectedItem).
                            let items_path = matches!(
                                b.target,
                                BindingTarget::SelectedItem | BindingTarget::SelectedValue
                            )
                            .then(|| world.get::<crate::items::PfItemsSource>(entity))
                            .flatten()
                            .map(|s| s.path.as_str());
                            let reads = std::iter::once(b.path.as_str())
                                .chain(b.multi_paths.iter().map(String::as_str))
                                .chain(items_path);
                            if ctx.changed_since(b.seen_version, reads) {
                                updates.push((i, b.clone()));
                            } else {
                                fast_forward.push(i);
                            }
                        }
                    }
                    // Element sources have no version counter; re-evaluate
                    // every frame (applies are no-ops when values match).
                    PfBindingSource::Element(_) => updates.push((i, b.clone())),
                    // Unresolved (never happens post-instantiation); skip.
                    PfBindingSource::Named(_) | PfBindingSource::Ancestor(_) => {}
                }
            }
        }
        if !fast_forward.is_empty()
            && let Some(version) = ctx.as_ref().map(|c| c.version())
            && let Some(mut bindings) = world.get_mut::<PfBindings>(entity)
        {
            for i in fast_forward {
                bindings.0[i].seen_version = version;
            }
        }

        for (index, binding) in updates {
            // MultiBinding: read every member path, combine via the
            // registered multi converter or positional StringFormat.
            if !binding.multi_paths.is_empty() {
                let ctx = ctx.as_ref().expect("multi bindings use the data context");
                let values: Vec<BoundValue> = binding
                    .multi_paths
                    .iter()
                    .map(|p| ctx.read_path(p).unwrap_or(BoundValue::Null))
                    .collect();
                let combined = match &binding.converter {
                    Some(key) => {
                        let conv = world
                            .get_resource::<PfMultiConverters>()
                            .and_then(|c| c.0.get(key).cloned());
                        match conv {
                            Some(c) => c.convert(&values, binding.converter_parameter.as_deref()),
                            None => {
                                warn!("bevy_pf: multi converter `{key}` is not registered");
                                None
                            }
                        }
                    }
                    None => binding
                        .string_format
                        .as_deref()
                        .map(|fmt| BoundValue::Str(apply_multi_format(fmt, &values))),
                };
                let combined = combined.or_else(|| binding.fallback.clone().map(BoundValue::Str));
                if let Some(value) = combined {
                    // Without a converter the positional format is already
                    // consumed; with one, StringFormat applies to the
                    // converted result (as {0}) through the normal path.
                    let mut binding = binding.clone();
                    if binding.converter.is_none() {
                        binding.string_format = None;
                    }
                    apply_binding_value(world, entity, &binding, &value);
                }
                let version = ctx.version();
                if let Some(mut bindings) = world.get_mut::<PfBindings>(entity) {
                    bindings.0[index].seen_version = if binding.mode == v::BindingMode::OneTime {
                        version.max(1)
                    } else {
                        version
                    };
                }
                continue;
            }
            // SelectedItem is resolved BEFORE any value read: the bound item
            // is an object, and reading it as a scalar would fall through to
            // reflect_to_bound's Debug representation — the very "compare the
            // rendered text" mistake this target exists to avoid.
            if binding.target == BindingTarget::SelectedItem
                && matches!(binding.source, PfBindingSource::DataContext)
            {
                let ctx = ctx.as_ref().expect("checked above");
                let Some(items) = world
                    .get::<crate::items::PfItemsSource>(entity)
                    .map(|s| s.path.clone())
                else {
                    warn!("SelectedItem needs an ItemsSource on the same control");
                    continue;
                };
                // Prefer the row already selected, so that among equal items
                // the user's actual pick is kept rather than the first match.
                let preferred =
                    if let Some(combo) = world.get::<crate::components::PfComboBox>(entity) {
                        combo.selected
                    } else {
                        let selected = world
                            .get::<crate::components::PfListBox>(entity)
                            .and_then(|l| l.selected);
                        selected.and_then(|sel| {
                            let container = crate::items::items_container(world, entity);
                            world
                                .get::<Children>(container)
                                .and_then(|c| c.iter().position(|child| child == sel))
                        })
                    };

                let outcome = ctx.selection_match(&items, &binding.path, preferred);
                let settled = apply_selection(world, entity, outcome, preferred);
                if settled {
                    let version = ctx.version();
                    if let Some(mut bindings) = world.get_mut::<PfBindings>(entity) {
                        bindings.0[index].seen_version = version;
                    }
                }
                // Not settled (collection not generated yet): leave
                // seen_version alone so the next frame retries.
                continue;
            }
            let (value, mark_version) = match &binding.source {
                PfBindingSource::DataContext => {
                    let ctx = ctx.as_ref().expect("checked above");
                    (ctx.read_path(&binding.path), Some(ctx.version()))
                }
                PfBindingSource::Element(source) => {
                    (read_element_value(world, *source, &binding.path), None)
                }
                PfBindingSource::Named(_) | PfBindingSource::Ancestor(_) => (None, None),
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
            // WPF order: TargetNullValue replaces a null (post-converter)
            // value; FallbackValue covers resolution failure.
            let value = match value {
                Some(BoundValue::Null) => Some(
                    binding
                        .target_null
                        .clone()
                        .map(BoundValue::Str)
                        .unwrap_or(BoundValue::Null),
                ),
                other => other,
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

/// Drive the control's selection from a resolved [`SelectionMatch`].
///
/// Returns whether the outcome is SETTLED — i.e. whether the binding may
/// record that it has seen this model version. `Pending` (and an index whose
/// row has not been generated yet) are not settled, so the binding retries
/// next frame instead of losing a selection made before the list arrived.
fn apply_selection(
    world: &mut World,
    entity: Entity,
    outcome: SelectionMatch,
    preferred: Option<usize>,
) -> bool {
    let is_combo = world.get::<crate::components::PfComboBox>(entity).is_some();
    match outcome {
        SelectionMatch::Pending => false,
        SelectionMatch::Index(i) => {
            if Some(i) == preferred {
                return true; // already there; do not churn the UI
            }
            if is_combo {
                crate::instantiate::select_combo_index(world, entity, i);
                world
                    .get::<crate::components::PfComboBox>(entity)
                    .and_then(|c| c.selected)
                    == Some(i)
            } else {
                let container = crate::items::items_container(world, entity);
                let child = world
                    .get::<Children>(container)
                    .and_then(|c| c.iter().nth(i));
                let Some(child) = child else {
                    return false; // rows not generated yet — retry
                };
                if let Some(mut list) = world.get_mut::<crate::components::PfListBox>(entity) {
                    list.selected = Some(child);
                }
                true
            }
        }
        // WPF clears the selection for a value that is not in the list, and
        // `None` means "nothing selected" outright. Both land here.
        SelectionMatch::Null | SelectionMatch::NotFound => {
            if is_combo {
                if let Some(state) = world.get::<crate::components::PfComboBox>(entity).cloned() {
                    if let Some(mut t) = world.get_mut::<bevy::ui::widget::Text>(state.text) {
                        t.0.clear();
                    }
                    if let Some(mut c) = world.get_mut::<crate::components::PfComboBox>(entity) {
                        c.selected = None;
                    }
                }
            } else if let Some(mut list) = world.get_mut::<crate::components::PfListBox>(entity) {
                list.selected = None;
            }
            true
        }
    }
}

/// `SelectedValue`: locate the row whose `SelectedValuePath` member equals
/// the bound key. Unlike `SelectedItem` this compares SCALARS, which is the
/// whole point — it is the escape hatch for models that identify rows by id
/// rather than by value.
fn apply_selected_value(world: &mut World, entity: Entity, value: &BoundValue) {
    let Some(ctx) = find_context(world, entity) else {
        return;
    };
    let Some(items) = world
        .get::<crate::items::PfItemsSource>(entity)
        .map(|s| s.path.clone())
    else {
        warn!("SelectedValue needs an ItemsSource on the same control");
        return;
    };
    let key = world
        .get::<crate::components::PfSelectedValuePath>(entity)
        .map(|p| p.0.clone())
        .unwrap_or_default();
    let Some(len) = ctx.list_len(&items) else {
        return;
    };
    if len == 0 {
        return; // not generated yet; retried on the next version bump
    }
    let outcome = (0..len)
        .find(|i| {
            let path = if key.is_empty() {
                format!("{items}[{i}]")
            } else {
                format!("{items}[{i}].{key}")
            };
            ctx.read_path(&path).is_some_and(|v| &v == value)
        })
        .map_or(SelectionMatch::NotFound, SelectionMatch::Index);
    apply_selection(world, entity, outcome, None);
}

fn apply_binding_value(world: &mut World, entity: Entity, binding: &PfBinding, value: &BoundValue) {
    use bevy::ui::widget::Text;
    match binding.target {
        // Never routed here: SelectedItem is resolved against the model in
        // `apply_bindings` before any value is read, precisely so an object
        // never arrives as a scalar.
        BindingTarget::SelectedItem => {}
        BindingTarget::SelectedValue => apply_selected_value(world, entity, value),
        BindingTarget::Text => {
            let text = binding.format(value);
            if let Some(mut t) = world.get_mut::<Text>(entity)
                && t.0 != text
            {
                t.0 = text;
            }
        }
        BindingTarget::EditableText => {
            let text = binding.format(value);
            if let Some(mut et) = world.get_mut::<bevy::text::EditableText>(entity)
                && et.editor().text() != text.as_str()
            {
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
        BindingTarget::SelectedIndex => {
            let Some(n) = value.as_f64() else { return };
            let index = n as usize;
            if n < 0.0 {
                return; // WPF: -1 = no selection; leave as-is
            }
            if world.get::<crate::components::PfComboBox>(entity).is_some() {
                let already = world
                    .get::<crate::components::PfComboBox>(entity)
                    .and_then(|c| c.selected);
                if already != Some(index) {
                    crate::instantiate::select_combo_index(world, entity, index);
                }
                return;
            }
            let container = crate::items::items_container(world, entity);
            let child = world
                .get::<Children>(container)
                .and_then(|c| c.iter().nth(index));
            let Some(child) = child else { return };
            if let Some(mut list) = world.get_mut::<crate::components::PfListBox>(entity)
                && list.selected != Some(child)
            {
                list.selected = Some(child);
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
                && p.value != n as f32
            {
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
            crate::provider::apply_wpf_visibility(world, entity, vis);
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
                // A bound brush is as non-null as a literal one — WPF's
                // Panel hit-test rule follows the property, not the parse.
                crate::provider::mark_background_assigned(world, entity, true);
                world
                    .entity_mut(entity)
                    .insert(BackgroundColor(convert::color(color)));
            }
        }
        BindingTarget::BorderBrush => {
            if let Ok(color) = value.to_display().parse::<v::PfColor>() {
                let color = convert::color(color);
                // A BUTTON REPAINTS ITS OWN BORDER, so setting BorderColor
                // alone does not hold. `button_interaction_visuals` writes
                // `BorderColor::all(visual.normal_border)` on every
                // `Changed<Interaction>` — which includes the frame the
                // control spawns, because that is when `Interaction` is
                // first added. A bound BorderBrush was therefore painted
                // over by the style's colour before it was ever seen, and
                // any later hover undid it again.
                //
                // The provider path already keeps the two in step
                // (provider.rs, PropertyTarget::BorderBrush); this is the
                // same rule for the binding path.
                if let Some(mut visual) = world.get_mut::<crate::components::ButtonVisual>(entity) {
                    visual.normal_border = color;
                }
                world.entity_mut(entity).insert(bevy::ui::BorderColor::all(color));
            }
        }
        BindingTarget::Stroke | BindingTarget::Fill => {
            let Ok(color) = value.to_display().parse::<v::PfColor>() else {
                return;
            };
            let brush = v::PfBrush::Solid(color);
            let Some(mut shape) = world.get_mut::<crate::shapes::PfShape>(entity) else {
                return;
            };
            // `get_mut` flags the component as changed, which is what makes
            // `rasterize_shapes` redraw at the same size.
            match binding.target {
                BindingTarget::Stroke => shape.stroke = Some(brush),
                _ => shape.fill = Some(brush),
            }
        }
        BindingTarget::Store(target) => {
            use crate::resources::PfValue;
            let pf = match value {
                BoundValue::Str(s) => PfValue::String(s.clone()),
                BoundValue::Num(n) => PfValue::Double(*n),
                BoundValue::Bool(b) => PfValue::Bool(*b),
                // A null source clears the Local tier: lower tiers (style,
                // theme, default) become effective again.
                BoundValue::Null => {
                    crate::provider::store_and_apply(
                        world,
                        entity,
                        target,
                        crate::provider::ValueSource::Local,
                        None,
                    );
                    return;
                }
            };
            crate::provider::set_local(world, entity, target, pf);
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
/// Write a control-surface property on an element (the inverse of
/// [`read_element_value`]) — the write half of two-way TemplatedParent /
/// ElementName bindings. Writes are no-ops when the value already matches.
pub(crate) fn write_element_value(
    world: &mut World,
    source: Entity,
    path: &str,
    value: &BoundValue,
) {
    match path {
        "Value" => {
            let Some(n) = value.as_f64() else { return };
            if world.get::<bevy::ui_widgets::SliderValue>(source).is_some() {
                if world
                    .get::<bevy::ui_widgets::SliderValue>(source)
                    .is_some_and(|v| v.0 != n as f32)
                {
                    world
                        .entity_mut(source)
                        .insert(bevy::ui_widgets::SliderValue(n as f32));
                }
            } else if let Some(mut p) = world.get_mut::<crate::components::PfProgress>(source)
                && p.value != n as f32
            {
                p.value = n as f32;
            }
        }
        "IsChecked" | "IsExpanded" => {
            let Some(b) = value.as_bool() else { return };
            let has = world.get::<Checked>(source).is_some();
            if b && !has {
                world.entity_mut(source).insert(Checked);
            } else if !b && has {
                world.entity_mut(source).remove::<Checked>();
            }
        }
        "IsOpen" => {
            let Some(b) = value.as_bool() else { return };
            if let Some(mut combo) = world.get_mut::<crate::components::PfComboBox>(source) {
                if combo.open != b {
                    combo.open = b;
                }
            } else if let Some(mut popup) = world.get_mut::<crate::overlay::PfPopup>(source)
                && popup.open != b
            {
                popup.open = b;
            }
        }
        "Text" => {
            let text = value.to_display();
            if let Some(mut t) = world.get_mut::<bevy::ui::widget::Text>(source) {
                if t.0 != text {
                    t.0 = text;
                }
                return;
            }
            let editable = {
                let mut found = None;
                if world.get::<bevy::text::EditableText>(source).is_some() {
                    found = Some(source);
                } else if let Some(children) = world.get::<Children>(source) {
                    let kids: Vec<Entity> = children.iter().collect();
                    found = kids
                        .into_iter()
                        .find(|c| world.get::<bevy::text::EditableText>(*c).is_some());
                }
                found
            };
            if let Some(e) = editable
                && let Some(mut et) = world.get_mut::<bevy::text::EditableText>(e)
                && et.editor().text() != text.as_str()
            {
                et.editor.set_text(&text);
            }
        }
        _ => {}
    }
}

/// TwoWay element-source bindings (RelativeSource TemplatedParent /
/// ElementName): when the TARGET's control state changes, mirror it back
/// onto the source element. Change-detected on the target (not
/// value-diffed), so the per-frame element re-apply never masquerades as a
/// user edit; `write_element_value` no-ops on equal values, so the apply
/// echo self-cancels.
type ChangedSliders<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static bevy::ui_widgets::SliderValue),
    (Changed<bevy::ui_widgets::SliderValue>, With<PfBindings>),
>;
type ChangedEditables<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static bevy::text::EditableText),
    (Changed<bevy::text::EditableText>, With<PfBindings>),
>;

pub(crate) fn element_write_back(
    checked_added: Query<Entity, (Added<Checked>, With<PfBindings>)>,
    mut checked_removed: RemovedComponents<Checked>,
    sliders: ChangedSliders,
    texts: ChangedEditables,
    bindings_q: Query<&PfBindings>,
    mut commands: Commands,
) {
    let mut queue = |entity: Entity, target: BindingTarget, value: BoundValue| {
        let Ok(bindings) = bindings_q.get(entity) else {
            return;
        };
        for binding in &bindings.0 {
            let PfBindingSource::Element(source) = binding.source else {
                continue;
            };
            if binding.target != target || !binding.is_to_source() {
                continue;
            }
            let path = binding.path.clone();
            let value = value.clone();
            commands.queue(move |world: &mut World| {
                write_element_value(world, source, &path, &value);
            });
        }
    };
    for entity in &checked_added {
        queue(entity, BindingTarget::IsChecked, BoundValue::Bool(true));
    }
    for entity in checked_removed.read() {
        queue(entity, BindingTarget::IsChecked, BoundValue::Bool(false));
    }
    for (entity, value) in &sliders {
        queue(
            entity,
            BindingTarget::SliderValue,
            BoundValue::Num(value.0 as f64),
        );
    }
    for (entity, et) in &texts {
        queue(
            entity,
            BindingTarget::EditableText,
            BoundValue::Str(et.editor().text().to_string()),
        );
    }
}

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
                && ctx.read_path(&binding.path).and_then(|b| b.as_bool()) != Some(value)
            {
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
        (Entity, Ref<bevy::ui_widgets::SliderValue>, &PfBindings),
        Changed<bevy::ui_widgets::SliderValue>,
    >,
    parents: Query<&ChildOf>,
    contexts: Query<&DataContext>,
) {
    for (entity, value, bindings) in &sliders {
        // Added-component echo guard: the spawn tick would write the
        // control's default over the source before the initial apply.
        if value.is_added() {
            continue;
        }
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

type ChangedList<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static crate::components::PfListBox,
        Option<&'static crate::components::PfItemsPanel>,
        &'static PfBindings,
        Option<&'static crate::items::PfItemsSource>,
        Ref<'static, crate::components::PfListBox>,
    ),
    Changed<crate::components::PfListBox>,
>;
type ChangedCombo<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static crate::components::PfComboBox,
        &'static PfBindings,
        Option<&'static crate::items::PfItemsSource>,
        Ref<'static, crate::components::PfComboBox>,
    ),
    Changed<crate::components::PfComboBox>,
>;

/// TwoWay: ListBox/ComboBox selection flows back to the source.
pub(crate) fn selection_write_back(
    lists: ChangedList,
    combos: ChangedCombo,
    parents: Query<&ChildOf>,
    contexts: Query<&DataContext>,
    children_q: Query<&Children>,
) {
    let write =
        |entity: Entity, bindings: &PfBindings, items: Option<&str>, index: Option<usize>| {
            for binding in &bindings.0 {
                if !binding.is_to_source() || binding.source != PfBindingSource::DataContext {
                    continue;
                }
                let Some(ctx) = find_context_via_queries(entity, &parents, &contexts) else {
                    continue;
                };
                match binding.target {
                    BindingTarget::SelectedIndex => {
                        let value = index.map(|i| i as f64).unwrap_or(-1.0);
                        let current = ctx.read_path(&binding.path).and_then(|b| b.as_f64());
                        if current != Some(value) {
                            ctx.write_path(&binding.path, &BoundValue::Num(value));
                        }
                    }
                    // Write the OBJECT back, not its position. The guard is a
                    // selection_match against the row the user picked: if the
                    // model already holds that exact row there is nothing to do,
                    // and skipping the write avoids a version bump that would
                    // ripple back through every binding reading this path.
                    BindingTarget::SelectedItem => {
                        let Some(items) = items else {
                            continue;
                        };
                        match index {
                            Some(i) => {
                                if ctx.selection_match(items, &binding.path, Some(i))
                                    != SelectionMatch::Index(i)
                                {
                                    ctx.set_from(&binding.path, &format!("{items}[{i}]"));
                                }
                            }
                            None => {
                                if ctx.selection_match(items, &binding.path, None)
                                    != SelectionMatch::Null
                                {
                                    ctx.set_null(&binding.path);
                                }
                            }
                        }
                    }
                    _ => continue,
                }
            }
        };
    for (entity, list, panel, bindings, source, added) in &lists {
        // Added-component echo guard: the first Changed tick is the spawn
        // itself — writing -1 back would clobber the source before the
        // initial to-target apply runs.
        if added.is_added() {
            continue;
        }
        let container = panel.map(|p| p.panel).unwrap_or(entity);
        let index = list.selected.and_then(|sel| {
            children_q
                .get(container)
                .ok()
                .and_then(|children| children.iter().position(|c| c == sel))
        });
        write(entity, bindings, source.map(|s| s.path.as_str()), index);
    }
    for (entity, combo, bindings, source, added) in &combos {
        if added.is_added() {
            continue;
        }
        write(
            entity,
            bindings,
            source.map(|s| s.path.as_str()),
            combo.selected,
        );
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
            target_null: None,
            multi_paths: Vec::new(),
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

/// Resolve deferred binding sources once a spawned tree is complete:
/// `Named` (ElementName) against the scene's name registry, `Ancestor`
/// (RelativeSource FindAncestor) by walking `ChildOf` for a matching
/// element kind.
pub(crate) fn resolve_deferred_sources(
    world: &mut World,
    entities: &[Entity],
    names: &bevy::platform::collections::HashMap<String, Entity>,
    warnings: &mut Vec<String>,
) {
    for &entity in entities {
        // Two-phase: walk the tree immutably, then patch the bindings.
        let mut resolved: Vec<(usize, PfBindingSource)> = Vec::new();
        {
            let Some(bindings) = world.get::<PfBindings>(entity) else {
                continue;
            };
            for (i, binding) in bindings.0.iter().enumerate() {
                match &binding.source {
                    PfBindingSource::Named(name) => match names.get(name.as_str()) {
                        Some(&source) => {
                            resolved.push((i, PfBindingSource::Element(source)));
                        }
                        None => warnings.push(format!(
                            "binding ElementName `{name}` does not match any x:Name in this scene"
                        )),
                    },
                    PfBindingSource::Ancestor(ty) => {
                        let mut cursor = entity;
                        let mut found = None;
                        while let Some(parent) = world.get::<ChildOf>(cursor) {
                            cursor = parent.parent();
                            let kind = world
                                .get::<crate::components::PfElementKind>(cursor)
                                .map(|k| k.0.as_str());
                            if kind == Some(ty.as_str()) {
                                found = Some(cursor);
                                break;
                            }
                        }
                        match found {
                            Some(source) => {
                                resolved.push((i, PfBindingSource::Element(source)));
                            }
                            None => warnings.push(format!(
                                "FindAncestor: no `{ty}` ancestor found; binding skipped"
                            )),
                        }
                    }
                    _ => {}
                }
            }
        }
        if resolved.is_empty() {
            continue;
        }
        let Some(mut bindings) = world.get_mut::<PfBindings>(entity) else {
            continue;
        };
        for (i, source) in resolved {
            bindings.0[i].source = source;
        }
    }
}
