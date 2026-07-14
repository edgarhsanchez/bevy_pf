//! Optional headless *instantiation* sweep over external, non-redistributable
//! XAML corpora (see `bevy_pf_xaml/tests/external_corpus.rs` for the parse
//! sweep). Prints a histogram of unsupported-feature warnings, which ranks
//! missing features by real-world usage.
//!
//! Run with:
//! ```sh
//! BEVY_PF_EXTERNAL_XAML_DIRS=/path/to/sdk cargo test -p bevy_pf \
//!     --test external_instantiate -- --ignored --nocapture
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use bevy::prelude::*;

fn collect_xaml(root: &std::path::Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_xaml(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("xaml") {
            out.push(path);
        }
    }
}

/// Collapse a warning message into a histogram bucket: strip `line:col`
/// position prefixes but keep identifiers, so the histogram names the exact
/// missing properties/elements/resource types.
fn bucket(warning: &str) -> String {
    match warning.split_once(": ") {
        Some((prefix, rest))
            if prefix.chars().all(|c| c.is_ascii_digit() || c == ':') =>
        {
            rest.to_string()
        }
        _ => warning.to_string(),
    }
}

#[test]
#[ignore = "requires BEVY_PF_EXTERNAL_XAML_DIRS pointing at local-only XAML corpora"]
fn instantiate_external_corpora() {
    let Ok(dirs) = std::env::var("BEVY_PF_EXTERNAL_XAML_DIRS") else {
        eprintln!("BEVY_PF_EXTERNAL_XAML_DIRS not set; nothing to do");
        return;
    };
    // (scan root, file) pairs so each file gets a loading environment rooted
    // at its corpus root — relative `Source=` references resolve for real.
    let mut files: Vec<(PathBuf, PathBuf)> = Vec::new();
    for dir in dirs.split(':').filter(|d| !d.is_empty()) {
        let root = PathBuf::from(dir);
        let mut found = Vec::new();
        collect_xaml(&root, &mut found);
        files.extend(found.into_iter().map(|f| (root.clone(), f)));
    }
    files.sort();

    let mut ok = 0usize;
    let mut with_warnings = 0usize;
    let mut errored: Vec<(PathBuf, String)> = Vec::new();
    let mut histogram: BTreeMap<String, usize> = BTreeMap::new();

    let mut unparseable = 0usize;
    for (scan_root, path) in &files {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        // Files that don't parse standalone (theme build fragments etc.) are
        // classified by the parse sweep (external_corpus.rs); they are not
        // instantiation failures.
        let Ok(doc) = bevy_pf_xaml::parse(&source) else {
            unparseable += 1;
            continue;
        };
        // ResourceDictionary roots are not spawnable scenes; count separately.
        if doc.root.name == "ResourceDictionary" {
            continue;
        }
        let mut env = bevy_pf::XamlEnv::from_fs_root(scan_root);
        env.base_dir = path
            .parent()
            .and_then(|p| p.strip_prefix(scan_root).ok())
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let mut world = World::new();
        let root = world.spawn_empty().id();
        match bevy_pf::instantiate_document_env(&mut world, root, &doc, &env) {
            Ok(result) => {
                if result.warnings.is_empty() {
                    ok += 1;
                } else {
                    with_warnings += 1;
                    for w in &result.warnings {
                        *histogram.entry(bucket(w)).or_default() += 1;
                    }
                }
            }
            Err(e) => errored.push((path.clone(), e.to_string())),
        }
    }

    let mut ranked: Vec<(usize, String)> =
        histogram.into_iter().map(|(k, v)| (v, k)).collect();
    ranked.sort_by_key(|entry| std::cmp::Reverse(entry.0));

    eprintln!("\n=== external instantiation sweep ===");
    eprintln!(
        "clean: {ok}, with warnings: {with_warnings}, hard errors: {}, \
         unparseable-standalone skipped: {unparseable}",
        errored.len()
    );
    for (path, e) in &errored {
        eprintln!("ERROR {} — {e}", path.display());
    }
    eprintln!("--- warning histogram (feature gaps by usage) ---");
    for (count, msg) in ranked.iter().take(40) {
        eprintln!("{count:5}  {msg}");
    }

    // Invariant: unsupported features must degrade to warnings, not errors.
    assert!(
        errored.is_empty(),
        "{} files hit hard instantiation errors",
        errored.len()
    );
}

#[test]
fn registered_custom_element_builds_from_markup() {
    use bevy_pf::prelude::PfElementAppExt;

    #[derive(Component)]
    struct Gauge {
        level: f32,
    }

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()));
    app.add_plugins(bevy_pf::prelude::PfUiPlugin);
    app.register_element("Gauge", |world, entity, node| {
        let level = match node.attribute("Level") {
            Some(bevy_pf::xaml_ast::XamlValue::Str(v)) => v.trim().parse().unwrap_or(0.0),
            _ => 0.0,
        };
        world.entity_mut(entity).insert(Gauge { level });
    });

    let doc = bevy_pf_xaml::parse(
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                        xmlns:local="clr-namespace:demo">
              <local:Gauge x:Name="G" Level="7" Width="120" Background="#FF224466">
                <TextBlock Text="inside a custom element"/>
              </local:Gauge>
            </StackPanel>"##,
    )
    .expect("parses");
    let world = app.world_mut();
    let root = world.spawn_empty().id();
    let result =
        bevy_pf::instantiate_document_env(world, root, &doc, &bevy_pf::XamlEnv::default())
            .expect("instantiates");
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);

    let gauge = app.world().get::<bevy_pf::XamlNames>(root).unwrap().get("G").unwrap();
    // The builder ran with the custom attribute...
    assert_eq!(app.world().get::<Gauge>(gauge).unwrap().level, 7.0);
    // ...and the framework handled the common surface: layout, paint,
    // children.
    assert_eq!(
        app.world().get::<Node>(gauge).unwrap().width,
        Val::Px(120.0)
    );
    let child = app
        .world()
        .get::<Children>(gauge)
        .unwrap()
        .iter()
        .next()
        .unwrap();
    assert_eq!(
        app.world().get::<bevy::ui::widget::Text>(child).unwrap().0,
        "inside a custom element"
    );
}
