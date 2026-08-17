//! Runtime `.xaml` asset loading with hot reload.
//!
//! Spawn an entity with a [`XamlView`] pointing at a `.xaml`/`.axaml` asset;
//! the view is instantiated when the asset loads and **re-instantiated
//! whenever the file changes** (enable Bevy's `file_watcher` feature — or
//! bevy_pf's `hot_reload` passthrough — for live editing).

use bevy::asset::{AssetLoader, LoadContext, io::Reader};
use bevy::prelude::*;

/// A validated XAML document loaded as an asset, with all
/// `ResourceDictionary Source=` dependencies prefetched (recursively) so
/// instantiation never does IO. Prefetching goes through
/// `LoadContext::read_asset_bytes`, which registers load dependencies —
/// editing a merged dictionary hot-reloads every view that uses it.
#[derive(Asset, TypePath, Debug, Clone)]
pub struct XamlAsset {
    pub source: String,
    /// Directory of this asset (asset-root-relative, `/` separators).
    pub base_dir: String,
    /// Resolved path -> source text for every reachable merged dictionary
    /// (shared: cloned per rebuild as a pointer, not a deep copy).
    pub merged: std::sync::Arc<bevy::platform::collections::HashMap<String, String>>,
}

impl XamlAsset {
    pub fn document(&self) -> bevy_pf_xaml::XamlDocument {
        bevy_pf_xaml::parse(&self.source).expect("XamlAsset was validated at load time")
    }

    /// The loading environment for instantiating this asset.
    pub fn env(&self) -> crate::instantiate::XamlEnv {
        let merged = self.merged.clone();
        crate::instantiate::XamlEnv {
            base_dir: self.base_dir.clone(),
            loader: Some(std::sync::Arc::new(move |assembly, path| {
                if assembly.is_some() {
                    return None; // assembly routing not supported via assets yet
                }
                merged.get(path).cloned()
            })),
        }
    }
}

/// Collect every `ResourceDictionary Source=` reference in a document.
fn collect_rd_sources(node: &bevy_pf_xaml::XamlNode, out: &mut Vec<String>) {
    if node.name == "ResourceDictionary"
        && let Some(bevy_pf_xaml::XamlValue::Str(src)) = node.attribute("Source")
    {
        out.push(src.clone());
    }
    for pe in &node.property_elements {
        for el in pe.elements() {
            collect_rd_sources(el, out);
        }
    }
    for el in node.child_elements() {
        collect_rd_sources(el, out);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum XamlAssetError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("utf-8 error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("xaml error: {0}")]
    Xaml(#[from] bevy_pf_xaml::XamlError),
}

#[derive(Default, TypePath)]
pub struct XamlAssetLoader;

impl AssetLoader for XamlAssetLoader {
    type Asset = XamlAsset;
    type Settings = ();
    type Error = XamlAssetError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let source = String::from_utf8(bytes)?;
        let doc = bevy_pf_xaml::parse(&source)?; // validate now; spawn can't fail later

        let base_dir = load_context
            .path()
            .parent()
            .map(|p| p.to_string().replace('\\', "/"))
            .unwrap_or_default();

        // Prefetch merged dictionaries breadth-first (each file's relative
        // Sources resolve against its own directory).
        let mut merged = bevy::platform::collections::HashMap::default();
        let mut visited = std::collections::HashSet::new();
        let mut work: Vec<(String, String)> = Vec::new(); // (base_dir, source uri)
        let mut roots = Vec::new();
        collect_rd_sources(&doc.root, &mut roots);
        for src in roots {
            work.push((base_dir.clone(), src));
        }
        while let Some((base, src)) = work.pop() {
            let Ok(uri) = bevy_pf_xaml::uri::PfUri::parse(&src) else {
                continue; // instantiation will warn with details
            };
            if uri.assembly.is_some() {
                continue;
            }
            let resolved = uri.resolve(&base);
            if !visited.insert(resolved.clone()) {
                continue;
            }
            let Ok(dep_bytes) = load_context.read_asset_bytes(resolved.clone()).await else {
                continue; // missing file -> instantiation warns
            };
            let Ok(text) = String::from_utf8(dep_bytes) else {
                continue;
            };
            if let Ok(dep_doc) = bevy_pf_xaml::parse(&text) {
                let sub_base = resolved
                    .rsplit_once('/')
                    .map(|(d, _)| d.to_string())
                    .unwrap_or_default();
                let mut subs = Vec::new();
                collect_rd_sources(&dep_doc.root, &mut subs);
                for s in subs {
                    work.push((sub_base.clone(), s));
                }
            }
            merged.insert(resolved, text);
        }

        Ok(XamlAsset {
            source,
            base_dir,
            merged: std::sync::Arc::new(merged),
        })
    }

    fn extensions(&self) -> &[&str] {
        &["xaml", "axaml"]
    }
}

/// Component that instantiates a XAML asset under this entity. The subtree is
/// rebuilt when the asset (re)loads.
#[derive(Component, Debug, Clone)]
pub struct XamlView(pub Handle<XamlAsset>);

/// Queue of views whose subtree must be (re)built.
#[derive(Resource, Default)]
pub(crate) struct PendingXamlViews(Vec<Entity>);

/// Collect views that need (re)instantiation: newly added views whose asset is
/// already available, and all views of an asset that was (re)loaded.
pub(crate) fn queue_xaml_views(
    mut events: MessageReader<AssetEvent<XamlAsset>>,
    assets: Res<Assets<XamlAsset>>,
    views: Query<(Entity, &XamlView)>,
    added: Query<(Entity, &XamlView), Added<XamlView>>,
    mut pending: ResMut<PendingXamlViews>,
) {
    for (entity, view) in &added {
        if assets.contains(&view.0) {
            pending.0.push(entity);
        }
    }
    for event in events.read() {
        let (AssetEvent::LoadedWithDependencies { id } | AssetEvent::Modified { id }) = event
        else {
            continue;
        };
        for (entity, view) in &views {
            if view.0.id() == *id && !pending.0.contains(&entity) {
                pending.0.push(entity);
            }
        }
    }
}

/// Exclusive system: rebuild queued views.
pub(crate) fn apply_xaml_views(world: &mut World) {
    let pending = std::mem::take(&mut world.resource_mut::<PendingXamlViews>().0);
    for entity in pending {
        let Ok(entity_ref) = world.get_entity(entity) else {
            continue; // view despawned in the meantime
        };
        let Some(view) = entity_ref.get::<XamlView>() else {
            continue;
        };
        let handle = view.0.clone();
        let Some((doc, env)) = world
            .resource::<Assets<XamlAsset>>()
            .get(&handle)
            .map(|a| (a.document(), a.env()))
        else {
            continue;
        };
        // Clear any previous instantiation, then rebuild in place. The root
        // entity is reused, so components the previous run accumulated must
        // go too — otherwise dynamic-resource entries and bindings duplicate
        // on every reload and stale ones keep firing.
        let mut root_entity = world.entity_mut(entity);
        root_entity.despawn_children();
        root_entity.remove::<(
            crate::dynamic::PfDynamicResources,
            crate::app_theme::PfAppThemeRefs,
            crate::dynamic::PfResources,
            crate::binding::PfBindings,
            crate::components::PfAttachedProps,
            crate::components::XamlNames,
        )>();
        match crate::instantiate::instantiate_document_env(world, entity, &doc, &env) {
            Ok(result) => {
                for w in &result.warnings {
                    warn!("bevy_pf: {w}");
                }
                // Rebuilding replaces Node etc., but XamlView must survive.
                world.entity_mut(entity).insert(XamlView(handle));
            }
            Err(e) => error!("bevy_pf: failed to instantiate XAML view: {e}"),
        }
    }
}
