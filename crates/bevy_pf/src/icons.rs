//! Vector icon set for `PackIcon` / `SymbolIcon` / `FontIcon`
//! (MaterialDesignInXaml `PackIcon`, MahApps `IconPacks`, WinUI `SymbolIcon`
//! equivalents).
//!
//! Icons are 24x24 path geometry rendered by the framework's own shape
//! engine — no icon font, so output is identical on native and wasm and
//! there is no fallback-font lottery. Subpaths rely on the WPF default
//! even-odd fill rule for holes (rings, cut-outs).
//!
//! ```xml
//! <PackIcon Kind="Home" Width="20" Height="20" Foreground="#FF444444"/>
//! ```

/// 24x24 path data for a named icon kind (case-insensitive), or `None`.
pub fn icon_path(kind: &str) -> Option<&'static str> {
    let idx = ICONS
        .binary_search_by(|(name, _)| {
            name.bytes()
                .map(|b| b.to_ascii_lowercase())
                .cmp(kind.bytes().map(|b| b.to_ascii_lowercase()))
        })
        .ok()?;
    Some(ICONS[idx].1)
}

/// Every icon kind this set defines, sorted.
pub fn icon_names() -> impl Iterator<Item = &'static str> {
    ICONS.iter().map(|(name, _)| *name)
}

/// Sorted by name (case-insensitive) for binary search.
static ICONS: &[(&str, &str)] = &[
    (
        "ArrowDown",
        "M11 4 H13 V16.2 L18.6 10.6 L20 12 L12 20 L4 12 L5.4 10.6 L11 16.2 Z",
    ),
    (
        "ArrowLeft",
        "M12 4 L13.4 5.4 L7.8 11 H20 V13 H7.8 L13.4 18.6 L12 20 L4 12 Z",
    ),
    (
        "ArrowRight",
        "M12 4 L10.6 5.4 L16.2 11 H4 V13 H16.2 L10.6 18.6 L12 20 L20 12 Z",
    ),
    (
        "ArrowUp",
        "M11 20 H13 V7.8 L18.6 13.4 L20 12 L12 4 L4 12 L5.4 13.4 L11 7.8 Z",
    ),
    (
        "Bell",
        "M12 2 C10.9 2 10 2.9 10 4 V4.6 C7.1 5.5 5 8.2 5 11.3 V17 L3 19 V20 H21 V19 L19 17 V11.3 C19 8.2 16.9 5.5 14 4.6 V4 C14 2.9 13.1 2 12 2 Z M9.5 21 A2.5 2.5 0 0 0 14.5 21 Z",
    ),
    ("Bookmark", "M6 2 H18 V22 L12 17.5 L6 22 Z"),
    (
        "Calendar",
        "M7 2 H9 V4 H15 V2 H17 V4 H20 V22 H4 V4 H7 Z M6 9 H18 V20 H6 Z",
    ),
    (
        "Check",
        "M9.5 16.2 L5.3 12 L3.9 13.4 L9.5 19 L20.1 8.4 L18.7 7 Z",
    ),
    (
        "ChevronDown",
        "M4.6 9 L3.2 10.4 L12 19.2 L20.8 10.4 L19.4 9 L12 16.4 Z",
    ),
    (
        "ChevronLeft",
        "M15 4.6 L13.6 3.2 L4.8 12 L13.6 20.8 L15 19.4 L7.6 12 Z",
    ),
    (
        "ChevronRight",
        "M9 4.6 L10.4 3.2 L19.2 12 L10.4 20.8 L9 19.4 L16.4 12 Z",
    ),
    (
        "ChevronUp",
        "M4.6 15 L3.2 13.6 L12 4.8 L20.8 13.6 L19.4 15 L12 7.6 Z",
    ),
    (
        "Clock",
        "M12 2 A10 10 0 1 0 12 22 A10 10 0 1 0 12 2 Z M12 4 A8 8 0 1 1 12 20 A8 8 0 1 1 12 4 Z M11.2 6 H12.8 V12.4 L17 14.8 L16.2 16.2 L11.2 13.3 Z",
    ),
    (
        "Close",
        "M5.7 4.3 L12 10.6 L18.3 4.3 L19.7 5.7 L13.4 12 L19.7 18.3 L18.3 19.7 L12 13.4 L5.7 19.7 L4.3 18.3 L10.6 12 L4.3 5.7 Z",
    ),
    (
        "Cloud",
        "M6.5 19 A4.5 4.5 0 0 1 6.1 10.1 A6 6 0 0 1 17.7 11.1 A4 4 0 0 1 17.5 19 Z",
    ),
    (
        "Delete",
        "M9 3 H15 V5 H20 V7 H4 V5 H9 Z M5 8 H19 L18 22 H6 Z",
    ),
    ("Document", "M5 2 H14 L19 7 V22 H5 Z M13 3.5 V8 H17.5 Z"),
    (
        "Download",
        "M11 3 H13 V12.2 L16.6 8.6 L18 10 L12 16 L6 10 L7.4 8.6 L11 12.2 Z M4 18 H20 V20 H4 Z",
    ),
    (
        "Edit",
        "M17.7 2.9 L21.1 6.3 L19 8.4 L15.6 5 Z M14.2 6.4 L17.6 9.8 L8.4 19 L4.4 19.6 L5 15.6 Z",
    ),
    (
        "Eye",
        "M12 5 C7 5 3.3 8.1 1.5 12 C3.3 15.9 7 19 12 19 C17 19 20.7 15.9 22.5 12 C20.7 8.1 17 5 12 5 Z M12 7.5 A4.5 4.5 0 1 0 12 16.5 A4.5 4.5 0 1 0 12 7.5 Z M12 9.5 A2.5 2.5 0 1 1 12 14.5 A2.5 2.5 0 1 1 12 9.5 Z",
    ),
    ("Filter", "M3 4 H21 L14 12.5 V20 L10 22 V12.5 Z"),
    ("Flag", "M5 2 H7 V22 H5 Z M8 3 H19 L16.5 7.5 L19 12 H8 Z"),
    ("Folder", "M3 4 H10 L12 7 H21 V20 H3 Z"),
    (
        "Grid",
        "M4 4 H11 V11 H4 Z M13 4 H20 V11 H13 Z M4 13 H11 V20 H4 Z M13 13 H20 V20 H13 Z",
    ),
    (
        "Heart",
        "M12 21 C5 14.5 2 11 2 7.5 C2 4.5 4.5 2.5 7 2.5 C9 2.5 11 3.5 12 5.5 C13 3.5 15 2.5 17 2.5 C19.5 2.5 22 4.5 22 7.5 C22 11 19 14.5 12 21 Z",
    ),
    ("Home", "M12 3 L21 11 H18 V20 H14 V14 H10 V20 H6 V11 H3 Z"),
    (
        "Info",
        "M12 2 A10 10 0 1 0 12 22 A10 10 0 1 0 12 2 Z M10.8 6.2 H13.2 V8.6 H10.8 Z M10.8 10.4 H13.2 V17.8 H10.8 Z",
    ),
    (
        "List",
        "M4 5 H6 V7 H4 Z M8 5 H20 V7 H8 Z M4 11 H6 V13 H4 Z M8 11 H20 V13 H8 Z M4 17 H6 V19 H4 Z M8 17 H20 V19 H8 Z",
    ),
    (
        "Location",
        "M12 2 C8 2 5 5 5 9 C5 14 12 22 12 22 C12 22 19 14 19 9 C19 5 16 2 12 2 Z M12 6 A3 3 0 1 0 12 12 A3 3 0 1 0 12 6 Z",
    ),
    (
        "Lock",
        "M12 2 C9.2 2 7 4.2 7 7 V10 H5 V22 H19 V10 H17 V7 C17 4.2 14.8 2 12 2 Z M12 4 C13.7 4 15 5.3 15 7 V10 H9 V7 C9 5.3 10.3 4 12 4 Z M11 14 H13 V18 H11 Z",
    ),
    (
        "Mail",
        "M2 5 H22 V19 H2 Z M4 7.3 V17 H20 V7.3 L12 13.3 Z M5.7 7 H18.3 L12 11.7 Z",
    ),
    (
        "Menu",
        "M3 6 H21 V8 H3 Z M3 11 H21 V13 H3 Z M3 16 H21 V18 H3 Z",
    ),
    ("Minus", "M5 11 H19 V13 H5 Z"),
    (
        "Moon",
        "M14.5 2.5 A10 10 0 1 0 21.5 9.5 A8 8 0 0 1 14.5 2.5 Z",
    ),
    ("Pause", "M6 4 H10 V20 H6 Z M14 4 H18 V20 H14 Z"),
    (
        "Person",
        "M12 3 A4.5 4.5 0 1 0 12 12 A4.5 4.5 0 1 0 12 3 Z M4 21 C4 16.6 7.6 13.5 12 13.5 C16.4 13.5 20 16.6 20 21 Z",
    ),
    ("Play", "M7 4 L20 12 L7 20 Z"),
    ("Plus", "M11 5 H13 V11 H19 V13 H13 V19 H11 V13 H5 V11 H11 Z"),
    (
        "Power",
        "M11 2 H13 V11 H11 Z M7.2 5.3 L8.6 6.7 A6.5 6.5 0 1 0 15.4 6.7 L16.8 5.3 A8.5 8.5 0 1 1 7.2 5.3 Z",
    ),
    (
        "Refresh",
        "M16.8 3.7 A9.6 9.6 0 1 1 3.7 7.2 L5.8 8.4 A7.2 7.2 0 1 0 15.6 5.8 Z M11.7 2.1 L18.5 0.7 L13.9 8.7 Z",
    ),
    (
        "Search",
        "M10 2 A8 8 0 1 0 10 18 A8 8 0 1 0 10 2 Z M10 5 A5 5 0 1 1 10 15 A5 5 0 1 1 10 5 Z M15.9 14.5 L21.5 20.1 L20.1 21.5 L14.5 15.9 Z",
    ),
    (
        "Settings",
        "M19.0 7.7 L20.0 10.2 L22.6 10.1 L22.6 13.9 L20.0 13.8 L19.0 16.3 L20.8 18.2 L18.2 20.8 L16.3 19.0 L13.8 20.0 L13.9 22.6 L10.1 22.6 L10.2 20.0 L7.7 19.0 L5.8 20.8 L3.2 18.2 L5.0 16.3 L4.0 13.8 L1.4 13.9 L1.4 10.1 L4.0 10.2 L5.0 7.7 L3.2 5.8 L5.8 3.2 L7.7 5.0 L10.2 4.0 L10.1 1.4 L13.9 1.4 L13.8 4.0 L16.3 5.0 L18.2 3.2 L20.8 5.8 L19.0 7.7 Z M15.6 12 A3.6 3.6 0 1 0 8.4 12 A3.6 3.6 0 1 0 15.6 12 Z",
    ),
    (
        "Star",
        "M12.0 2.4 L14.4 9.4 L21.7 9.4 L15.8 13.8 L18.0 20.9 L12.0 16.6 L6.0 20.9 L8.2 13.8 L2.3 9.4 L9.6 9.4 Z",
    ),
    ("Stop", "M5 5 H19 V19 H5 Z"),
    (
        "Sun",
        "M16.2 12 A4.2 4.2 0 1 0 7.8 12 A4.2 4.2 0 1 0 16.2 12 Z M18.5 11.3 L22.8 11.4 L22.8 12.6 L18.5 12.7 Z M17.1 16.1 L20.1 19.2 L19.2 20.1 L16.1 17.1 Z M12.7 18.5 L12.6 22.8 L11.4 22.8 L11.3 18.5 Z M7.9 17.1 L4.8 20.1 L3.9 19.2 L6.9 16.1 Z M5.5 12.7 L1.2 12.6 L1.2 11.4 L5.5 11.3 Z M6.9 7.9 L3.9 4.8 L4.8 3.9 L7.9 6.9 Z M11.3 5.5 L11.4 1.2 L12.6 1.2 L12.7 5.5 Z M16.1 6.9 L19.2 3.9 L20.1 4.8 L17.1 7.9 Z",
    ),
    (
        "Upload",
        "M12 3 L18 9 L16.6 10.4 L13 6.8 V16 H11 V6.8 L7.4 10.4 L6 9 Z M4 18 H20 V20 H4 Z",
    ),
    (
        "Warning",
        "M12 2 L23 21 H1 Z M11 9 H13 V14.5 H11 Z M11 16.3 H13 V18.3 H11 Z",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_sorted_for_binary_search() {
        for pair in ICONS.windows(2) {
            assert!(
                pair[0].0.to_ascii_lowercase() < pair[1].0.to_ascii_lowercase(),
                "ICONS out of order: {} >= {}",
                pair[0].0,
                pair[1].0
            );
        }
    }

    #[test]
    fn every_icon_parses() {
        for (name, data) in ICONS {
            let parsed = bevy_pf_xaml::geometry::parse_path_data(data)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(!parsed.figures.is_empty(), "{name} has no figures");
            // control_bounds is conservative around arcs, so only figure
            // start anchors are checked against the 24x24 grid.
            for fig in &parsed.figures {
                let p = fig.start;
                assert!(
                    p.x >= -0.5 && p.y >= -0.5 && p.x <= 24.5 && p.y <= 24.5,
                    "{name} start escapes the grid"
                );
            }
        }
    }

    #[test]
    fn lookup_is_case_insensitive() {
        assert!(icon_path("home").is_some());
        assert!(icon_path("HOME").is_some());
        assert!(icon_path("Settings").is_some());
        assert!(icon_path("NoSuchIcon").is_none());
    }
}
