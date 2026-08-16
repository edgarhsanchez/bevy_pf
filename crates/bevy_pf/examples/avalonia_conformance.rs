//! How much of the official Avalonia sample suite does bevy_pf understand?
//!
//! The 69 `.axaml` files under `examples/avalonia_samples/` are vendored from
//! AvaloniaUI/Avalonia.Samples (MIT) **byte for byte**. Nothing is edited to
//! suit bevy_pf — that is the whole point. A sample that needs a tweak to
//! load is a gap in bevy_pf, and this example is what finds them.
//!
//!     cargo run -p bevy_pf --example avalonia_conformance
//!     cargo run -p bevy_pf --example avalonia_conformance -- --verbose
//!
//! It prints, per file, whether the document parsed and instantiated, and
//! then ranks every distinct complaint by how many files hit it — so the next
//! thing worth implementing is the line at the top.
//!
//! Avalonia is a third dialect alongside WPF and MAUI. It shares XAML's shape
//! but not its vocabulary: `xmlns="https://github.com/avaloniaui"`,
//! `using:` namespaces rather than `clr-namespace:`, compact
//! `Grid ColumnDefinitions="Auto,*"`, `StackPanel Spacing`, `<Window>` roots
//! and design-time `d:`/`mc:` attributes. Reading this report as "bevy_pf is
//! broken" would be wrong; read it as the distance between the dialects.

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy_pf::prelude::*;
use bevy_pf::{XamlEnv, instantiate_document_env};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn sample_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "axaml") {
                out.push(path);
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/avalonia_samples");
    let mut out = Vec::new();
    walk(&root, &mut out);
    out.sort();
    out
}

/// Collapse a warning to the shape of the problem, so "property `Spacing`"
/// and "property `Classes`" rank separately but the same complaint about
/// twelve different files ranks once.
fn complaint(warning: &str) -> String {
    // Warnings look like "property `X`: ..." or "element `Y` ...". Keep the
    // leading category and the quoted subject, drop the rest.
    let mut out = String::new();
    let mut chars = warning.chars().peekable();
    let mut ticks = 0;
    for c in chars.by_ref() {
        if c == '`' {
            ticks += 1;
            out.push(c);
            if ticks == 2 {
                break;
            }
            continue;
        }
        if ticks < 2 {
            out.push(c);
        }
    }
    if ticks < 2 {
        // No quoted subject: keep the first clause.
        return warning.split(';').next().unwrap_or(warning).trim().to_string();
    }
    // Append the tail category (e.g. "is not supported yet").
    let tail: String = warning
        .rsplit('`')
        .next()
        .unwrap_or("")
        .trim_start_matches(|c: char| c == ':' || c.is_whitespace())
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if tail.is_empty() { out } else { format!("{out} — {tail}") }
}

fn main() {
    let verbose = std::env::args().any(|a| a == "--verbose");
    let files = sample_files();
    if files.is_empty() {
        eprintln!("no .axaml files found — is examples/avalonia_samples/ populated?");
        std::process::exit(1);
    }

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()));
    app.add_plugins(PfUiPlugin);

    let root_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/avalonia_samples");
    let mut parse_failures = 0usize;
    let mut clean = 0usize;
    let mut with_warnings = 0usize;
    // Complaint -> (files affected, total occurrences). FILES is the metric
    // that matters: one sample re-templating a control can raise the same
    // warning sixty times and look like the whole suite is blocked on it.
    let mut complaints: BTreeMap<String, (BTreeSet<String>, usize)> = BTreeMap::new();
    let mut per_file: Vec<(String, usize, Option<String>)> = Vec::new();

    for path in &files {
        let shown = path
            .strip_prefix(&root_dir)
            .unwrap_or(path)
            .display()
            .to_string();
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                per_file.push((shown, 0, Some(format!("unreadable: {e}"))));
                parse_failures += 1;
                continue;
            }
        };
        let doc = match bevy_pf_xaml::parse(&source) {
            Ok(doc) => doc,
            Err(e) => {
                let reason = format!("{e}");
                let e = complaints.entry(format!("PARSE: {}", complaint(&reason))).or_default();
                e.0.insert(shown.clone());
                e.1 += 1;
                per_file.push((shown, 0, Some(reason)));
                parse_failures += 1;
                continue;
            }
        };
        let world = app.world_mut();
        let root = world.spawn_empty().id();
        match instantiate_document_env(world, root, &doc, &XamlEnv::default()) {
            Ok(result) => {
                for w in &result.warnings {
                    let e = complaints.entry(complaint(w)).or_default();
                    e.0.insert(shown.clone());
                    e.1 += 1;
                }
                if result.warnings.is_empty() {
                    clean += 1;
                } else {
                    with_warnings += 1;
                }
                if verbose && !result.warnings.is_empty() {
                    for w in &result.warnings {
                        println!("    {shown}: {w}");
                    }
                }
                per_file.push((shown, result.warnings.len(), None));
            }
            Err(e) => {
                let reason = format!("{e}");
                let e = complaints.entry(format!("BUILD: {}", complaint(&reason))).or_default();
                e.0.insert(shown.clone());
                e.1 += 1;
                per_file.push((shown, 0, Some(reason)));
                parse_failures += 1;
            }
        }
    }

    println!();
    println!("Avalonia.Samples through bevy_pf — {} files, unaltered", files.len());
    println!("{}", "=".repeat(64));
    println!("  loaded clean          {clean}");
    println!("  loaded with warnings  {with_warnings}");
    println!("  did not load          {parse_failures}");
    println!();

    let mut ranked: Vec<(&String, &(BTreeSet<String>, usize))> = complaints.iter().collect();
    // By FILES first: breadth is what makes a gap worth closing. Occurrences
    // are shown too, because one file hitting something 59 times is a
    // different kind of problem from 59 files hitting it once.
    ranked.sort_by(|a, b| {
        b.1.0.len().cmp(&a.1.0.len()).then(b.1.1.cmp(&a.1.1)).then(a.0.cmp(b.0))
    });
    println!("What is missing — files affected (occurrences):");
    println!("{}", "-".repeat(64));
    for (reason, (files, total)) in ranked.iter().take(40) {
        println!("  {:3} files ({:4})  {reason}", files.len(), total);
    }
    if ranked.len() > 40 {
        println!("  ... and {} more", ranked.len() - 40);
    }

    if verbose {
        println!();
        println!("Per file:");
        println!("{}", "-".repeat(64));
        for (name, warnings, failure) in &per_file {
            match failure {
                Some(e) => println!("  ✗ {name}\n      {e}"),
                None if *warnings == 0 => println!("  ✓ {name}"),
                None => println!("  ~ {name} ({warnings} warnings)"),
            }
        }
    } else {
        println!();
        println!("Run with --verbose for per-file detail.");
    }
}
