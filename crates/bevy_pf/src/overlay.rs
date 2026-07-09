//! The popup overlay layer: a top-Z root hosting dropdowns, tooltips, and
//! (later) menus, positioned against their anchor elements after layout.
//!
//! Popup content is *entity-tree* parented to the overlay root but keeps a
//! [`crate::components::PfLogicalParent`] link to its owner, so DataContext,
//! dynamic resources, and trigger data all inherit through the logical tree,
//! exactly like WPF.

use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::ui::{ComputedNode, GlobalZIndex, UiGlobalTransform};

use crate::components::PfLogicalParent;

/// Marker for the global overlay root.
#[derive(Component, Debug)]
pub struct PfOverlayRoot;

/// Where a popup sits relative to its anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PfPlacement {
    /// Under the anchor, left-aligned (dropdowns).
    #[default]
    Bottom,
    /// To the right of the anchor (submenus).
    Right,
}

/// A popup under the overlay root, positioned against `anchor` after layout.
#[derive(Component, Debug, Clone)]
pub struct PfPopup {
    pub anchor: Entity,
    pub placement: PfPlacement,
    pub open: bool,
    /// Match the popup's min-width to the anchor's width (dropdowns).
    pub match_anchor_width: bool,
}

/// Full-screen click-catcher behind an open popup; clicking it closes the
/// popup (light dismiss).
#[derive(Component, Debug, Clone)]
pub struct PfPopupBackdrop {
    pub popup: Entity,
}

/// Get or create the overlay root (a non-pickable full-screen layer).
pub fn ensure_overlay_root(world: &mut World) -> Entity {
    let mut query = world.query_filtered::<Entity, With<PfOverlayRoot>>();
    if let Some(e) = query.iter(world).next() {
        return e;
    }
    world
        .spawn((
            PfOverlayRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..Default::default()
            },
            GlobalZIndex(i32::MAX - 1000),
            Pickable::IGNORE,
        ))
        .id()
}

/// Spawn a backdrop for a popup (initially hidden). The caller adds both to
/// the overlay root, backdrop first (below the popup in paint order).
pub fn spawn_backdrop(world: &mut World, popup: Entity) -> Entity {
    world
        .spawn((
            PfPopupBackdrop { popup },
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                display: Display::None,
                ..Default::default()
            },
            Interaction::default(),
            Pickable {
                should_block_lower: true,
                is_hoverable: true,
            },
        ))
        .id()
}

/// Sync popup/backdrop visibility with `PfPopup::open`.
pub(crate) fn sync_popup_visibility(
    mut popups: Query<(Entity, &PfPopup, &mut Node), Changed<PfPopup>>,
    mut backdrops: Query<(&PfPopupBackdrop, &mut Node), Without<PfPopup>>,
) {
    for (popup_entity, popup, mut node) in &mut popups {
        node.display = if popup.open {
            Display::Flex
        } else {
            Display::None
        };
        for (backdrop, mut backdrop_node) in &mut backdrops {
            if backdrop.popup == popup_entity {
                backdrop_node.display = node.display;
            }
        }
    }
}

/// Position open popups against their anchors (runs after layout; positions
/// settle one frame after opening, which matches typical popup behavior).
pub(crate) fn position_popups(
    mut popups: Query<(&PfPopup, &mut Node)>,
    anchors: Query<(&ComputedNode, &UiGlobalTransform)>,
) {
    for (popup, mut node) in &mut popups {
        if !popup.open {
            continue;
        }
        let Ok((computed, transform)) = anchors.get(popup.anchor) else {
            continue;
        };
        let size = computed.size();
        if size == Vec2::ZERO {
            continue;
        }
        let inv = computed.inverse_scale_factor();
        let center: Vec2 = transform.translation;
        let top_left = (center - size / 2.0) * inv;
        let logical_size = size * inv;
        let (left, top) = match popup.placement {
            PfPlacement::Bottom => (top_left.x, top_left.y + logical_size.y),
            PfPlacement::Right => (top_left.x + logical_size.x, top_left.y),
        };
        node.position_type = PositionType::Absolute;
        if node.left != Val::Px(left) {
            node.left = Val::Px(left);
        }
        if node.top != Val::Px(top) {
            node.top = Val::Px(top);
        }
        if popup.match_anchor_width && node.min_width != Val::Px(logical_size.x) {
            node.min_width = Val::Px(logical_size.x);
        }
    }
}

// ---------------------------------------------------------------------------
// ToolTip
// ---------------------------------------------------------------------------

/// A WPF `ToolTip` string; shown in a popup after a short hover delay.
#[derive(Component, Debug, Clone)]
pub struct PfToolTip(pub String);

/// The currently showing tooltip, if any.
#[derive(Resource, Default)]
pub(crate) struct PfActiveTooltip(Option<(Entity, Entity)>); // (owner, popup)

const TOOLTIP_DELAY: f64 = 0.5;

/// Track hover times on tooltip owners; show/hide the tooltip popup.
pub(crate) fn tooltip_system(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();

    // Find the hovered tooltip owner, if any.
    let mut query = world.query::<(Entity, &Interaction, &PfToolTip)>();
    let mut hovered: Option<(Entity, String)> = None;
    for (entity, interaction, tip) in query.iter(world) {
        if matches!(interaction, Interaction::Hovered) {
            hovered = Some((entity, tip.0.clone()));
            break;
        }
    }

    #[derive(Default)]
    struct HoverState {
        owner: Option<Entity>,
        since: f64,
    }
    let mut state = world.remove_resource::<TooltipHover>().unwrap_or_default();

    let active = world.resource::<PfActiveTooltip>().0;
    match hovered {
        Some((owner, text)) => {
            if state.0.owner != Some(owner) {
                state.0 = HoverState {
                    owner: Some(owner),
                    since: now,
                };
            }
            let elapsed = now - state.0.since;
            if elapsed >= TOOLTIP_DELAY && active.is_none() {
                let popup = spawn_tooltip_popup(world, owner, &text);
                world.resource_mut::<PfActiveTooltip>().0 = Some((owner, popup));
            } else if let Some((active_owner, popup)) = active {
                if active_owner != owner {
                    world.entity_mut(popup).despawn();
                    world.resource_mut::<PfActiveTooltip>().0 = None;
                }
            }
        }
        None => {
            state.0 = HoverState::default();
            if let Some((_, popup)) = active {
                world.entity_mut(popup).despawn();
                world.resource_mut::<PfActiveTooltip>().0 = None;
            }
        }
    }
    world.insert_resource(state);

    #[derive(Resource, Default)]
    struct TooltipHover(HoverState);
}

fn spawn_tooltip_popup(world: &mut World, owner: Entity, text: &str) -> Entity {
    let overlay = ensure_overlay_root(world);
    let label = world
        .spawn((
            bevy::ui::widget::Text::new(text),
            bevy::text::TextFont {
                font_size: bevy::text::FontSize::Px(12.0),
                ..Default::default()
            },
            bevy::text::TextColor(Color::WHITE),
        ))
        .id();
    let popup = world
        .spawn((
            PfPopup {
                anchor: owner,
                placement: PfPlacement::Bottom,
                open: true,
                match_anchor_width: false,
            },
            PfLogicalParent(owner),
            Node {
                position_type: PositionType::Absolute,
                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..Default::default()
            },
            BackgroundColor(Color::srgba(0.15, 0.15, 0.15, 0.95)),
            Pickable::IGNORE,
        ))
        .id();
    world.entity_mut(popup).add_children(&[label]);
    world.entity_mut(overlay).add_children(&[popup]);
    popup
}
