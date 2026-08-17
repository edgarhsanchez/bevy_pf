//! Application-level resources and deferred `{DynamicResource}` lookup.
//!
//! WPF semantics implemented here:
//! - `Application.Resources` is the global fallback tier after the element
//!   tree ([`PfApplicationResources`]); replace it at runtime (theme swap)
//!   and every `{DynamicResource}` re-resolves.
//! - Elements that declare `Resources` carry them at runtime
//!   ([`PfResources`]), so dynamic lookup walks the live entity tree exactly
//!   like WPF walks the logical tree.
//! - A `{DynamicResource}` reference is applied immediately if resolvable,
//!   and *recorded* either way — a key that only appears later (e.g. a theme
//!   dictionary merged after styles load, as WPF's Fluent theme does) applies
//!   when it shows up, instead of warning.

use std::sync::Arc;

use bevy::prelude::*;

use crate::resources::{PfValue, ResourceDictionary, ResourceKey};

/// The application-wide resource dictionary (WPF `Application.Resources`).
#[derive(Resource, Default, Clone)]
pub struct PfApplicationResources {
    pub dict: Arc<ResourceDictionary>,
    /// Bumped on every replacement; drives dynamic-resource refresh.
    pub revision: u64,
}

/// The resources an element declared in XAML, kept for runtime
/// (`DynamicResource`) lookups up the entity tree.
#[derive(Component, Debug, Clone)]
pub struct PfResources(pub Arc<ResourceDictionary>);

/// One recorded `{DynamicResource}` reference.
#[derive(Debug, Clone)]
pub struct DynEntry {
    pub key: ResourceKey,
    pub target: crate::provider::PropertyTarget,
    /// The value-provider tier this reference writes at (Local for
    /// attributes, Style for style setters). Precedence is handled
    /// structurally by the store: refresh can never clobber a higher tier.
    pub tier: crate::provider::ValueSource,
}

/// All dynamic-resource references on an entity.
#[derive(Component, Debug, Default, Clone)]
pub struct PfDynamicResources(pub Vec<DynEntry>);

/// Tracks the last app-resource revision the refresh system applied.
#[derive(Resource, Default)]
pub(crate) struct LastDynRevision(pub u64);

/// Resolve a key the WPF way: element's own resources, then ancestors, then
/// the application dictionary.
pub fn resolve_dynamic(world: &World, entity: Entity, key: &ResourceKey) -> Option<PfValue> {
    let mut current = Some(entity);
    while let Some(e) = current {
        if let Some(res) = world.get::<PfResources>(e)
            && let Some(v) = res.0.get(key)
        {
            return Some(v.clone());
        }
        // Follow logical links first (popup content under the overlay root).
        current = world
            .get::<crate::components::PfLogicalParent>(e)
            .map(|l| l.0)
            .or_else(|| world.get::<ChildOf>(e).map(|c| c.parent()));
    }
    world
        .get_resource::<PfApplicationResources>()
        .and_then(|app| app.dict.get(key).cloned())
}

/// Re-resolve all `{DynamicResource}` references when the application
/// dictionary changes (theme swap, late merges).
/// Also re-picks every `{AppThemeBinding}` — the two share a pass because
/// either signal can matter to either kind of reference: a theme flip changes
/// which arm an AppThemeBinding picks, and a dictionary merge changes what the
/// key inside a `{DynamicResource}` arm resolves to.
pub(crate) fn refresh_dynamic_resources(world: &mut World) {
    let revision = world
        .get_resource::<PfApplicationResources>()
        .map(|r| r.revision)
        .unwrap_or(0);
    let theme_gen = world
        .get_resource::<crate::app_theme::PfAppTheme>()
        .map(|t| t.generation())
        .unwrap_or(0);
    let rev_changed = world
        .get_resource::<LastDynRevision>()
        .is_none_or(|l| l.0 != revision);
    let theme_changed = world
        .get_resource::<crate::app_theme::LastAppThemeGen>()
        .is_none_or(|l| l.0 != theme_gen);
    if !rev_changed && !theme_changed {
        return;
    }
    world.insert_resource(LastDynRevision(revision));
    world.insert_resource(crate::app_theme::LastAppThemeGen(theme_gen));

    // A theme change alone cannot move a plain {DynamicResource}.
    if rev_changed {
        let mut query = world.query::<(Entity, &PfDynamicResources)>();
        let targets: Vec<(Entity, Vec<DynEntry>)> =
            query.iter(world).map(|(e, d)| (e, d.0.clone())).collect();
        for (entity, entries) in targets {
            for entry in entries {
                if let Some(value) = resolve_dynamic(world, entity, &entry.key) {
                    crate::provider::store_and_apply(
                        world,
                        entity,
                        entry.target,
                        entry.tier,
                        Some(value),
                    );
                }
            }
        }
    }

    let theme = crate::app_theme::app_theme(world);
    let mut query = world.query::<(Entity, &crate::app_theme::PfAppThemeRefs)>();
    let themed: Vec<(Entity, Vec<crate::app_theme::AppThemeEntry>)> =
        query.iter(world).map(|(e, r)| (e, r.0.clone())).collect();
    for (entity, entries) in themed {
        for entry in entries {
            let value = match entry.arms.pick(theme) {
                Some(crate::app_theme::ThemeArm::Value(v)) => v.clone(),
                Some(crate::app_theme::ThemeArm::Dynamic(key)) => {
                    match resolve_dynamic(world, entity, key) {
                        Some(v) => Some(v),
                        None => continue, // key absent; leave what is there
                    }
                }
                // No arm matched and no Default: MAUI writes null.
                None => None,
            };
            crate::provider::store_and_apply(world, entity, entry.target, entry.tier, value);
        }
    }
}

/// Replace the application resources (theme swap). All `{DynamicResource}`
/// references re-resolve on the next update.
pub fn set_application_resources_dict(world: &mut World, dict: ResourceDictionary) {
    let revision = world
        .get_resource::<PfApplicationResources>()
        .map(|r| r.revision)
        .unwrap_or(0);
    world.insert_resource(PfApplicationResources {
        dict: Arc::new(dict),
        revision: revision + 1,
    });
}

/// Merge entries into the application resources (used by `Application` roots
/// and incremental theme composition).
pub fn merge_application_resources(world: &mut World, entries: ResourceDictionary) {
    let (mut dict, revision) = world
        .get_resource::<PfApplicationResources>()
        .map(|r| ((*r.dict).clone(), r.revision))
        .unwrap_or_default();
    dict.extend(entries);
    world.insert_resource(PfApplicationResources {
        dict: Arc::new(dict),
        revision: revision + 1,
    });
}
