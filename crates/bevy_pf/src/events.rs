//! The pointer-event surface (Noesis-plan increment 7): WPF code-behind
//! event attributes (`MouseDown="OnPickUp"`) route to Rust handlers
//! registered by name — the code-behind analog for drag/drop, hit
//! feedback, and custom interaction.
//!
//! ```ignore
//! app.on_ui_event("OnPickUp", |world, e, info| { /* start a drag */ });
//! // XAML: <Border MouseDown="OnPickUp" Drag="OnMove" DragEnd="OnDrop">
//! ```
//!
//! Supported events: Click, MouseDoubleClick, MouseEnter, MouseLeave,
//! MouseDown, MouseUp, MouseMove (over the element), and — bevy picking's
//! capture-like gesture set — DragStart, Drag, DragEnd, Drop. Unregistered
//! names stay silent, preserving verbatim-WPF instantiation.

use bevy::prelude::*;

/// What a handler learns about the pointer at fire time.
#[derive(Debug, Clone, Copy, Default)]
pub struct PfPointerInfo {
    /// Pointer position in window coordinates.
    pub position: Vec2,
    /// Movement delta for Move/Drag events (zero otherwise).
    pub delta: Vec2,
}

type Handler = dyn Fn(&mut World, Entity, PfPointerInfo) + Send + Sync;

/// Named UI event handlers (the code-behind registry).
#[derive(Resource, Default, Clone)]
pub struct PfEventHandlers(
    pub bevy::platform::collections::HashMap<String, std::sync::Arc<Handler>>,
);

/// `App` extension: register a named handler for XAML event attributes.
pub trait PfEventAppExt {
    fn on_ui_event(
        &mut self,
        name: impl Into<String>,
        f: impl Fn(&mut World, Entity, PfPointerInfo) + Send + Sync + 'static,
    ) -> &mut Self;
}

impl PfEventAppExt for bevy::app::App {
    fn on_ui_event(
        &mut self,
        name: impl Into<String>,
        f: impl Fn(&mut World, Entity, PfPointerInfo) + Send + Sync + 'static,
    ) -> &mut Self {
        let world = self.world_mut();
        if !world.contains_resource::<PfEventHandlers>() {
            world.init_resource::<PfEventHandlers>();
        }
        world
            .resource_mut::<PfEventHandlers>()
            .0
            .insert(name.into(), std::sync::Arc::new(f));
        self
    }
}

/// Run a named handler if registered (silent otherwise — WPF markup often
/// names code-behind that a port hasn't wired yet).
pub fn dispatch(world: &mut World, name: &str, entity: Entity, info: PfPointerInfo) {
    let handler = world
        .get_resource::<PfEventHandlers>()
        .and_then(|h| h.0.get(name).cloned());
    if let Some(handler) = handler {
        handler(world, entity, info);
    }
}

/// Wire a XAML event attribute (`MouseDown="OnPickUp"`) as a picking
/// observer that dispatches to the named handler. bevy_picking's drag
/// events already implement pointer capture (Drag/DragEnd keep firing on
/// the pressed entity while the pointer roams), so WPF CaptureMouse
/// patterns map onto DragStart/Drag/DragEnd directly.
pub(crate) fn attach_event_handler(
    world: &mut World,
    entity: Entity,
    event: &str,
    handler: String,
) {
    use bevy::picking::events::{
        Click, Drag, DragDrop, DragEnd, DragStart, Move, Out, Over, Pointer, Press, Release,
    };

    // One observer per (event, handler); the queued command re-reads the
    // registry at fire time so late registration still works.
    macro_rules! wire {
        (@delta $e:ident, zero) => { Vec2::ZERO };
        (@delta $e:ident, delta) => { $e.delta };
        ($ev:ty, $delta:ident) => {{
            world.entity_mut(entity).observe(
                move |ev: On<Pointer<$ev>>, mut commands: Commands| {
                    let info = PfPointerInfo {
                        position: ev.pointer_location.position,
                        delta: wire!(@delta ev, $delta),
                    };
                    let handler = handler.clone();
                    commands.queue(move |world: &mut World| {
                        dispatch(world, &handler, entity, info);
                    });
                },
            );
        }};
    }
    match event {
        // No double-click counter in bevy_picking: MouseDoubleClick
        // degrades to every click, which keeps verbatim markup functional.
        "Click" | "MouseDoubleClick" => wire!(Click, zero),
        "MouseEnter" => wire!(Over, zero),
        "MouseLeave" => wire!(Out, zero),
        "MouseDown" => wire!(Press, zero),
        "MouseUp" => wire!(Release, zero),
        "MouseMove" => wire!(Move, delta),
        "DragStart" => wire!(DragStart, zero),
        "Drag" => wire!(Drag, delta),
        "DragEnd" => wire!(DragEnd, zero),
        "Drop" => wire!(DragDrop, zero),
        _ => {}
    }
}

/// Every element under the window position, topmost-ish first (larger
/// z/later spawn last) — the hit-test helper for drag-drop targets.
pub fn hit_test(world: &mut World, position: Vec2) -> Vec<Entity> {
    let mut query = world.query::<(
        Entity,
        &bevy::ui::ComputedNode,
        &bevy::ui::UiGlobalTransform,
    )>();
    let mut hits = Vec::new();
    for (entity, node, transform) in query.iter(world) {
        let scale = node.inverse_scale_factor();
        let size = node.size() * scale;
        let center: Vec2 = transform.translation * scale;
        let half = size / 2.0;
        if (position.x - center.x).abs() <= half.x && (position.y - center.y).abs() <= half.y {
            hits.push(entity);
        }
    }
    hits
}
