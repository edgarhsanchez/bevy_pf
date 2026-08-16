//! Parse every file in the shared XAML compatibility corpus
//! (`<workspace>/tests/corpus/`). These are verbatim open-source XAML files
//! from microsoft/WPF-Samples (MIT).

use std::path::PathBuf;

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/corpus")
        .canonicalize()
        .expect("corpus directory exists")
}

fn corpus_files(sub: &str) -> Vec<PathBuf> {
    let dir = corpus_dir().join(sub);
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("xaml") | Some("axaml")
            )
        })
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no corpus files found in {}", dir.display());
    files
}

#[test]
fn parses_all_wpf_corpus_files() {
    for path in corpus_files("wpf") {
        let src = std::fs::read_to_string(&path).unwrap();
        let doc = bevy_pf_xaml::parse(&src)
            .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()));
        assert!(
            !doc.root.name.is_empty(),
            "{} produced an empty root",
            path.display()
        );
    }
}

/// Verbatim files from Microsoft's WPF reference implementation
/// (dotnet/wpf, MIT). See `tests/corpus/wpf-upstream/README.md` for the
/// commit hash and per-file provenance. These are parse-only: files with a
/// `ResourceDictionary` root are intentionally excluded from the `bevy_pf`
/// instantiation corpus test.
#[test]
fn parses_all_wpf_upstream_corpus_files() {
    for path in corpus_files("wpf-upstream") {
        let src = std::fs::read_to_string(&path).unwrap();
        let doc = bevy_pf_xaml::parse(&src)
            .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()));
        assert!(
            !doc.root.name.is_empty(),
            "{} produced an empty root",
            path.display()
        );
    }
}

#[test]
fn hello_world_shape_is_correct() {
    let src =
        std::fs::read_to_string(corpus_dir().join("wpf/hello.xaml")).unwrap();
    let doc = bevy_pf_xaml::parse(&src).unwrap();
    assert_eq!(doc.root.name, "Window");
    assert_eq!(doc.root.x_class.as_deref(), Some("HelloWorld.MainWindow"));
    let grid = doc.root.child_elements().next().unwrap();
    assert_eq!(grid.name, "Grid");
    let tb = grid.child_elements().next().unwrap();
    assert_eq!(tb.name, "TextBlock");
    assert_eq!(tb.text_content().as_deref(), Some("Hello, World!"));
}

#[test]
fn expenseit_home_structure() {
    let src =
        std::fs::read_to_string(corpus_dir().join("wpf/expenseithome.xaml")).unwrap();
    let doc = bevy_pf_xaml::parse(&src).unwrap();
    assert_eq!(doc.root.name, "Page");
    let grid = doc.root.child_elements().next().unwrap();
    assert_eq!(grid.name, "Grid");
    // Grid.Resources, Grid.Background, Grid.ColumnDefinitions, Grid.RowDefinitions
    assert!(grid.property_element("Resources").is_some());
    assert!(grid.property_element("Background").is_some());
    let cols = grid.property_element("ColumnDefinitions").unwrap();
    assert_eq!(cols.elements().count(), 2);
    let rows = grid.property_element("RowDefinitions").unwrap();
    assert_eq!(rows.elements().count(), 4);
}
