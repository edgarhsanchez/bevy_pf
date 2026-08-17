//! WPF-style navigation: `Frame`, the journal, and page-navigating
//! `Hyperlink`s.
//!
//! Pages are registered up front as routes (compile-time `xaml!` scenes, so
//! navigation works identically on native and wasm):
//!
//! ```ignore
//! app.add_plugins(PfUiPlugin)
//!     .register_page("home.xaml", xaml!(r#"<Page ...>...</Page>"#))
//!     .register_page("settings.xaml", xaml!(r#"<Page ...>...</Page>"#));
//! ```
//!
//! A `<Frame Source="home.xaml"/>` shows a page; `<Hyperlink
//! NavigateUri="settings.xaml">` inside it navigates the enclosing frame
//! (absolute `http(s)`/`mailto` URIs still open the browser, like WPF).
//! From systems, use [`navigate`], [`go_back`], [`go_forward`] — each
//! navigation writes a [`PfNavigated`] message.
//!
//! Journal semantics follow WPF's URI navigation with `KeepAlive="False"`
//! (the default there too): pages re-instantiate on every visit; persistent
//! state belongs in the `DataContext`, which frames pass to their pages like
//! any other logical parent.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::XamlEnv;
use crate::XamlScene;
use crate::binding::DataContext;
use crate::components::{PfFrame, PfLogicalParent};

/// The route registry: URI -> page scene.
#[derive(Resource, Default)]
pub struct PfPages(pub HashMap<String, XamlScene>);

/// Written after a frame shows a new page (navigate, back, or forward).
#[derive(Message, Debug, Clone)]
pub struct PfNavigated {
    pub frame: Entity,
    pub source: String,
    /// The page's `Title`, when it declares one.
    pub title: Option<String>,
}

/// `App` extension: register a page route.
pub trait PfNavigationAppExt {
    fn register_page(&mut self, route: impl Into<String>, scene: XamlScene) -> &mut Self;
}

impl PfNavigationAppExt for App {
    fn register_page(&mut self, route: impl Into<String>, scene: XamlScene) -> &mut Self {
        self.world_mut()
            .get_resource_or_insert_with::<PfPages>(Default::default)
            .0
            .insert(route.into(), scene);
        self
    }
}

/// Resolve a route to its page document. Registry first; on native, loose
/// `.xaml` files under `assets/` work as a fallback (hot-editable).
fn resolve(world: &World, route: &str) -> Option<bevy_pf_xaml::XamlDocument> {
    if let Some(pages) = world.get_resource::<PfPages>()
        && let Some(scene) = pages.0.get(route)
    {
        return Some(scene.document());
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = std::path::Path::new("assets").join(route);
        if let Ok(source) = std::fs::read_to_string(path) {
            return bevy_pf_xaml::parse(&source).ok();
        }
    }
    None
}

/// Navigate a frame to `route`, pushing the current page onto the back stack
/// and clearing the forward stack (WPF journal semantics).
pub fn navigate(world: &mut World, frame: Entity, route: &str) -> bool {
    let Some(current) = world.get::<PfFrame>(frame).map(|f| f.current.clone()) else {
        warn!("bevy_pf: navigate() target is not a Frame");
        return false;
    };
    if !show_page(world, frame, route) {
        return false;
    }
    if let Some(mut f) = world.get_mut::<PfFrame>(frame) {
        if let Some(prev) = current {
            f.back.push(prev);
        }
        f.forward.clear();
    }
    sync_chrome(world, frame);
    true
}

/// WPF `NavigationService.GoBack`.
pub fn go_back(world: &mut World, frame: Entity) -> bool {
    let Some((prev, current)) = world.get_mut::<PfFrame>(frame).and_then(|mut f| {
        let prev = f.back.pop()?;
        let current = f.current.clone();
        Some((prev, current))
    }) else {
        return false;
    };
    if !show_page(world, frame, &prev) {
        return false;
    }
    if let Some(mut f) = world.get_mut::<PfFrame>(frame)
        && let Some(current) = current
    {
        f.forward.push(current);
    }
    sync_chrome(world, frame);
    true
}

/// WPF `NavigationService.GoForward`.
pub fn go_forward(world: &mut World, frame: Entity) -> bool {
    let Some((next, current)) = world.get_mut::<PfFrame>(frame).and_then(|mut f| {
        let next = f.forward.pop()?;
        let current = f.current.clone();
        Some((next, current))
    }) else {
        return false;
    };
    if !show_page(world, frame, &next) {
        return false;
    }
    if let Some(mut f) = world.get_mut::<PfFrame>(frame)
        && let Some(current) = current
    {
        f.back.push(current);
    }
    sync_chrome(world, frame);
    true
}

pub fn can_go_back(world: &World, frame: Entity) -> bool {
    world
        .get::<PfFrame>(frame)
        .is_some_and(|f| !f.back.is_empty())
}

pub fn can_go_forward(world: &World, frame: Entity) -> bool {
    world
        .get::<PfFrame>(frame)
        .is_some_and(|f| !f.forward.is_empty())
}

/// Instantiate `route` into the frame's content host (replacing the current
/// page), record it as current, and report [`PfNavigated`].
fn show_page(world: &mut World, frame: Entity, route: &str) -> bool {
    let Some(doc) = resolve(world, route) else {
        warn!("bevy_pf: page `{route}` is not registered and no file was found");
        return false;
    };
    let Some(content) = world.get::<PfFrame>(frame).map(|f| f.content) else {
        return false;
    };

    world.entity_mut(content).despawn_children();
    let page_root = world.spawn(PfLogicalParent(frame)).id();
    // Pages inherit the frame's DataContext through the logical link; an own
    // DataContext on the page (rare) would win, matching WPF.
    let _ = world.get::<DataContext>(frame); // inheritance is by tree walk
    match crate::instantiate_document_env(world, page_root, &doc, &XamlEnv::default()) {
        Ok(result) => {
            for w in &result.warnings {
                warn!("bevy_pf: page `{route}`: {w}");
            }
        }
        Err(e) => {
            warn!("bevy_pf: page `{route}` failed to instantiate: {e}");
            world.entity_mut(page_root).despawn();
            return false;
        }
    }
    world.entity_mut(content).add_children(&[page_root]);

    let title = match doc.root.attribute("Title") {
        Some(bevy_pf_xaml::XamlValue::Str(s)) => Some(s.clone()),
        _ => None,
    };
    if let Some(mut f) = world.get_mut::<PfFrame>(frame) {
        f.current = Some(route.to_string());
        f.current_title = title.clone();
    }
    world.write_message(PfNavigated {
        frame,
        source: route.to_string(),
        title,
    });
    true
}

/// Hyperlink activation: relative URIs navigate the nearest enclosing frame;
/// absolute web/mail URIs open externally — WPF's split exactly.
pub fn follow_hyperlink(world: &mut World, link: Entity, uri: &str) {
    let external =
        uri.starts_with("http://") || uri.starts_with("https://") || uri.starts_with("mailto:");
    if external {
        crate::util::open_url(uri);
        return;
    }
    // Walk up (child links first, then logical links) to the nearest Frame.
    let mut current = link;
    loop {
        if world.get::<PfFrame>(current).is_some() {
            navigate(world, current, uri);
            return;
        }
        current = match world.get::<ChildOf>(current) {
            Some(c) => c.parent(),
            None => match world.get::<PfLogicalParent>(current) {
                Some(l) => l.0,
                None => break,
            },
        };
    }
    warn!("bevy_pf: Hyperlink `{uri}` has no enclosing Frame; opening externally");
    crate::util::open_url(uri);
}

/// Show/hide the built-in back/forward chrome to match the journal.
fn sync_chrome(world: &mut World, frame: Entity) {
    let Some((back_btn, fwd_btn)) = world
        .get::<PfFrame>(frame)
        .and_then(|f| f.chrome.map(|c| (c.back_button, c.forward_button)))
    else {
        return;
    };
    let back_ok = can_go_back(world, frame);
    let fwd_ok = can_go_forward(world, frame);
    for (button, enabled) in [(back_btn, back_ok), (fwd_btn, fwd_ok)] {
        if let Some(mut bg) = world.get_mut::<BackgroundColor>(button) {
            bg.0 = if enabled {
                Color::srgb_u8(0xE6, 0xE6, 0xE6)
            } else {
                Color::srgb_u8(0xF4, 0xF4, 0xF4)
            };
        }
        if let Some(mut text_color) = world
            .get::<Children>(button)
            .and_then(|c| c.iter().next())
            .and_then(|t| world.get_mut::<bevy::text::TextColor>(t))
        {
            text_color.0 = if enabled {
                Color::srgb_u8(0x1A, 0x1A, 0x1A)
            } else {
                Color::srgb_u8(0xAF, 0xAF, 0xAF)
            };
        }
    }
}

/// Frames declared with `Source=` navigate once the app is running (the page
/// registry may be filled after the scene spawns).
pub(crate) fn init_pending_frames(world: &mut World) {
    let mut pending: Vec<(Entity, String)> = Vec::new();
    let mut frames = world.query::<(Entity, &PfFrame)>();
    for (entity, frame) in frames.iter(world) {
        if let Some(source) = &frame.pending_source {
            pending.push((entity, source.clone()));
        }
    }
    for (entity, source) in pending {
        let resolvable = resolve(world, &source).is_some();
        if resolvable {
            if let Some(mut f) = world.get_mut::<PfFrame>(entity) {
                f.pending_source = None;
            }
            show_page(world, entity, &source);
            sync_chrome(world, entity);
        }
    }
}
