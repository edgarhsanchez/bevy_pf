//! Finding XAML elements from any Bevy system.
//!
//! Every identity XAML gives an element becomes a plain ECS component, so
//! ordinary Bevy queries always work:
//!
//! - `x:Name` / `Name`  -> [`PfName`] (also indexed in [`XamlNames`] on the
//!   scene root, WPF-namescope style)
//! - `x:Uid`            -> [`PfUid`]
//! - `AutomationProperties.AutomationId` -> [`PfAutomationId`]
//! - the XAML type name -> [`PfElementKind`] (`"Button"`, `"DataGrid"`, ...)
//! - control state components (`PfComboBox`, `PfTabControl`, `PfTreeView`,
//!   `PfDataGrid`, ...) are public and queryable directly.
//!
//! [`PfQuery`] bundles those lookups into one [`SystemParam`] so any system
//! can locate elements without plumbing:
//!
//! ```ignore
//! fn score_changed(score: Res<Score>, ui: PfQuery, mut texts: Query<&mut Text>) {
//!     if !score.is_changed() { return; }
//!     if let Some(label) = ui.by_name("ScoreLabel")
//!         && let Some(text_entity) = ui.first_text_in(label)
//!         && let Ok(mut t) = texts.get_mut(text_entity) {
//!         t.0 = format!("{} pts", score.0);
//!     }
//! }
//! ```

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::ui::widget::Text;

use crate::components::{PfAutomationId, PfElementKind, PfName, PfUid, XamlNames};

/// One-stop element lookup for systems. All methods are plain query scans —
/// standard Bevy under the hood, no hidden registries beyond [`XamlNames`].
#[derive(SystemParam)]
pub struct PfQuery<'w, 's> {
    names: Query<'w, 's, (Entity, &'static PfName)>,
    uids: Query<'w, 's, (Entity, &'static PfUid)>,
    automation: Query<'w, 's, (Entity, &'static PfAutomationId)>,
    kinds: Query<'w, 's, (Entity, &'static PfElementKind)>,
    scopes: Query<'w, 's, &'static XamlNames>,
    parents: Query<'w, 's, &'static ChildOf>,
    children: Query<'w, 's, &'static Children>,
    // Archetype filter only — no data access, so it composes with a
    // caller-side `Query<&mut Text>` without conflicts.
    texts: Query<'w, 's, (), With<Text>>,
}

impl PfQuery<'_, '_> {
    /// The first element whose `x:Name` matches. If several scenes reuse the
    /// same name, prefer [`Self::named_in`] with a scene root.
    pub fn by_name(&self, name: &str) -> Option<Entity> {
        self.names.iter().find(|(_, n)| n.0 == name).map(|(e, _)| e)
    }

    /// Every element with this `x:Name` (across all instantiated scenes).
    pub fn all_by_name(&self, name: &str) -> Vec<Entity> {
        self.names
            .iter()
            .filter(|(_, n)| n.0 == name)
            .map(|(e, _)| e)
            .collect()
    }

    /// Namescope lookup: the element named `name` inside the scene rooted at
    /// `root` (the entity returned by `spawn_xaml`), like WPF `FindName`.
    pub fn named_in(&self, root: Entity, name: &str) -> Option<Entity> {
        self.scopes.get(root).ok().and_then(|s| s.get(name))
    }

    /// The first element whose `x:Uid` matches.
    pub fn by_uid(&self, uid: &str) -> Option<Entity> {
        self.uids.iter().find(|(_, u)| u.0 == uid).map(|(e, _)| e)
    }

    /// The first element whose `AutomationProperties.AutomationId` matches.
    pub fn by_automation_id(&self, id: &str) -> Option<Entity> {
        self.automation
            .iter()
            .find(|(_, a)| a.0 == id)
            .map(|(e, _)| e)
    }

    /// Every element instantiated from the given XAML type
    /// (`"Button"`, `"TextBox"`, ...).
    pub fn by_kind(&self, kind: &str) -> Vec<Entity> {
        self.kinds
            .iter()
            .filter(|(_, k)| k.0 == kind)
            .map(|(e, _)| e)
            .collect()
    }

    /// This element's `x:Name`, if it has one.
    pub fn name_of(&self, entity: Entity) -> Option<&str> {
        self.names.get(entity).ok().map(|(_, n)| n.0.as_str())
    }

    /// Walk up `ChildOf` links to the scene root carrying the [`XamlNames`]
    /// namescope this element belongs to.
    pub fn scope_root(&self, entity: Entity) -> Option<Entity> {
        let mut current = entity;
        loop {
            if self.scopes.contains(current) {
                return Some(current);
            }
            current = self.parents.get(current).ok()?.parent();
        }
    }

    /// The first `Text` entity under `element` (depth-first). Content
    /// controls keep their text on a child entity, not on themselves; pair
    /// this with a caller-side `Query<&mut Text>` to rewrite labels.
    pub fn first_text_in(&self, element: Entity) -> Option<Entity> {
        if self.texts.contains(element) {
            return Some(element);
        }
        let children = self.children.get(element).ok()?;
        for child in children.iter() {
            if let Some(found) = self.first_text_in(child) {
                return Some(found);
            }
        }
        None
    }
}
