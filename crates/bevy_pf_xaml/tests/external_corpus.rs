//! Optional sweep over external XAML corpora that cannot be redistributed
//! (e.g. the NoesisGUI SDK's samples and themes, which are EULA-licensed).
//!
//! Nothing from these directories is copied into the repository — the harness
//! only *reads* local files as a compatibility oracle.
//!
//! Run with:
//! ```sh
//! BEVY_PF_EXTERNAL_XAML_DIRS=/path/to/sdk cargo test -p bevy_pf_xaml \
//!     --test external_corpus -- --ignored --nocapture
//! ```
//! Multiple roots are separated by `:`.

use std::path::PathBuf;

fn collect_xaml(root: &std::path::Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_xaml(&path, out);
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("xaml") | Some("axaml")
        ) {
            out.push(path);
        }
    }
}

#[test]
#[ignore = "requires BEVY_PF_EXTERNAL_XAML_DIRS pointing at local-only XAML corpora"]
fn parse_external_corpora() {
    let Ok(dirs) = std::env::var("BEVY_PF_EXTERNAL_XAML_DIRS") else {
        eprintln!("BEVY_PF_EXTERNAL_XAML_DIRS not set; nothing to do");
        return;
    };

    let mut files = Vec::new();
    for dir in dirs.split(':').filter(|d| !d.is_empty()) {
        collect_xaml(std::path::Path::new(dir), &mut files);
    }
    files.sort();
    assert!(!files.is_empty(), "no .xaml files found under {dirs}");

    // Known-untestable files: WPF DRTs for `MarkupExtensionBracketCharacters`
    // exercise custom bracket pairs (`$`...`^`) declared by type attributes —
    // untokenizable without that type metadata (accepted deviation, see
    // docs/wpf-conformance-notes.md).
    let skip_patterns = ["BracketCharacterAttribute.xaml"];
    let before = files.len();
    files.retain(|p| {
        !skip_patterns
            .iter()
            .any(|pat| p.to_string_lossy().contains(pat))
    });
    let skipped_known = before - files.len();
    if skipped_known > 0 {
        eprintln!("skipping {skipped_known} known type-driven scanner-metadata DRT file(s)");
    }

    let mut passed = 0usize;
    let mut wrapped = 0usize;
    let mut preprocessor = 0usize;
    let mut failures: Vec<(PathBuf, String)> = Vec::new();
    for path in &files {
        let Ok(source) = std::fs::read_to_string(path) else {
            failures.push((path.clone(), "unreadable / not UTF-8".to_string()));
            continue;
        };
        match bevy_pf_xaml::parse(&source) {
            Ok(_) => passed += 1,
            Err(first_error) => {
                // Theme *fragments* (e.g. dotnet/wpf `Themes/XAML/*.xaml`) are
                // concatenated into full dictionaries at build time and don't
                // declare their namespaces. Retry inside the same envelope the
                // real build uses; the fragment content still gets exercised.
                let enveloped = format!(
                    concat!(
                        r#"<ResourceDictionary xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation""#,
                        r#" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml""#,
                        r#" xmlns:theme="theme-fragment""#,
                        r#" xmlns:s="clr-namespace:System;assembly=mscorlib""#,
                        r#" xmlns:sys="clr-namespace:System;assembly=mscorlib""#,
                        r#" xmlns:po="fragment-options""#,
                        r#" xmlns:ribbon="fragment-ribbon""#,
                        r#" xmlns:vsm="fragment-vsm""#,
                        r#" xmlns:ui="fragment-ui">{}</ResourceDictionary>"#
                    ),
                    source.trim_start_matches('\u{feff}')
                );
                match bevy_pf_xaml::parse(&enveloped) {
                    Ok(_) => {
                        passed += 1;
                        wrapped += 1;
                    }
                    // Still not well-formed XML even in an envelope: these are
                    // build-time preprocessor fragments (conditional regions
                    // open/close tags across branches). No XML parser can read
                    // them pre-expansion, so they are excluded, not failed.
                    Err(bevy_pf_xaml::XamlError::Xml(_)) => preprocessor += 1,
                    Err(wrapped_error) => failures.push((
                        path.clone(),
                        format!("{first_error} (wrapped: {wrapped_error})"),
                    )),
                }
            }
        }
    }

    eprintln!("\n=== external corpus sweep ===");
    eprintln!(
        "parsed OK: {passed}/{} files ({wrapped} as namespace-wrapped fragments; {preprocessor} preprocessor fragments excluded as non-XML)",
        files.len()
    );
    for (path, error) in &failures {
        eprintln!("FAIL {}\n     {error}", path.display());
    }
    assert!(
        failures.is_empty(),
        "{} of {} external XAML files failed to parse (see stderr)",
        failures.len(),
        files.len()
    );
}
