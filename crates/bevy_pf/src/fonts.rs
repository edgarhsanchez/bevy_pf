//! The built-in UI font family and WPF font-family resolution.
//!
//! Bevy's text stack (Parley) resolves `FontSource::Family` names against the
//! fonts registered in `Assets<Font>` plus, on native platforms, the system
//! font database. The web has **no** system fonts, so without a registered
//! family every `FontFamily`, `FontWeight`, and `FontStyle` silently degrades
//! to the single embedded fallback face. `PfUiPlugin` therefore embeds
//! **Fira Sans** (SIL OFL 1.1 — see `assets/fonts/LICENSE-FiraSans.txt`) in
//! regular/bold/italic/bold-italic and uses it as the default UI family, so
//! text renders identically on native and wasm.
//!
//! WPF family names that cannot exist cross-platform (`Segoe UI`, `Arial`,
//! ...) are mapped onto the built-in family; monospace names map to the
//! generic monospace source. Any other name is passed through, so natively
//! installed families and `Font` assets you register yourself still resolve.

use bevy::prelude::*;
use bevy::text::{Font, FontSource};

/// The family name of the embedded UI font.
pub const DEFAULT_FAMILY: &str = "Fira Sans";

/// Keeps the embedded faces alive in `Assets<Font>`.
#[derive(Resource, Debug)]
pub struct PfFonts {
    pub faces: Vec<Handle<Font>>,
}

pub(crate) fn register_builtin_fonts(app: &mut App) {
    let Some(mut fonts) = app.world_mut().get_resource_mut::<Assets<Font>>() else {
        // Headless test worlds without the text plugin: nothing renders text,
        // so there is nothing to register.
        return;
    };
    let faces = vec![
        fonts.add(Font::from_bytes(
            include_bytes!("../assets/fonts/FiraSans-Regular.ttf").to_vec(),
        )),
        fonts.add(Font::from_bytes(
            include_bytes!("../assets/fonts/FiraSans-Bold.ttf").to_vec(),
        )),
        fonts.add(Font::from_bytes(
            include_bytes!("../assets/fonts/FiraSans-Italic.ttf").to_vec(),
        )),
        fonts.add(Font::from_bytes(
            include_bytes!("../assets/fonts/FiraSans-BoldItalic.ttf").to_vec(),
        )),
    ];
    app.insert_resource(PfFonts { faces });
}

/// The default font source for text without an explicit `FontFamily`.
pub fn default_font() -> FontSource {
    FontSource::Family(DEFAULT_FAMILY.into())
}

/// Resolve a XAML `FontFamily` to a source the font database can satisfy on
/// every platform.
pub fn resolve_family(family: &str) -> FontSource {
    let name = family.trim();
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        // Common WPF/Windows UI families: map to the embedded family so the
        // same markup looks right on macOS, Linux, and the web.
        "segoe ui" | "arial" | "helvetica" | "helvetica neue" | "tahoma" | "verdana"
        | "calibri" | "trebuchet ms" | "global user interface" | "sans-serif"
        | "sans serif" => default_font(),
        "consolas" | "courier new" | "courier" | "cascadia code" | "cascadia mono"
        | "lucida console" | "monospace" | "global monospace" => FontSource::Monospace,
        "times new roman" | "georgia" | "cambria" | "serif" | "global serif" => {
            FontSource::Serif
        }
        _ => FontSource::Family(name.into()),
    }
}
