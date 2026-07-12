//! Toast / Snackbar / Growl notifications (MaterialDesign Snackbar,
//! HandyControl Growl, Notifications.Wpf equivalents).
//!
//! ```ignore
//! bevy_pf::toast::show(world, "Saved!");
//! bevy_pf::toast::show_with(world, "Disk full", toast::Severity::Error, 6.0);
//! ```
//!
//! Toasts stack bottom-right on the overlay layer and dismiss themselves.

use bevy::prelude::*;

use crate::overlay::ensure_overlay_root;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Severity {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

impl Severity {
    fn colors(self) -> (Color, Color) {
        match self {
            Severity::Info => (Color::srgb_u8(0x2B, 0x33, 0x42), Color::WHITE),
            Severity::Success => (Color::srgb_u8(0x1E, 0x6E, 0x34), Color::WHITE),
            Severity::Warning => (Color::srgb_u8(0x8A, 0x64, 0x00), Color::WHITE),
            Severity::Error => (Color::srgb_u8(0x8F, 0x1F, 0x1F), Color::WHITE),
        }
    }
}

/// A live toast; despawned by the expiry system.
#[derive(Component, Debug)]
pub struct PfToast {
    pub expires_at: f32,
}

/// The bottom-right stack that hosts toasts.
#[derive(Component, Debug)]
pub struct PfToastHost;

fn ensure_host(world: &mut World) -> Entity {
    let mut query = world.query_filtered::<Entity, With<PfToastHost>>();
    if let Some(host) = query.iter(world).next() {
        return host;
    }
    let overlay = ensure_overlay_root(world);
    let host = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(16.0),
                bottom: Val::Px(16.0),
                display: Display::Flex,
                flex_direction: FlexDirection::ColumnReverse,
                row_gap: Val::Px(8.0),
                align_items: AlignItems::FlexEnd,
                ..Default::default()
            },
            bevy::picking::Pickable::IGNORE,
            PfToastHost,
        ))
        .id();
    world.entity_mut(overlay).add_children(&[host]);
    host
}

/// Show an info toast for 4 seconds.
pub fn show(world: &mut World, message: &str) -> Entity {
    show_with(world, message, Severity::Info, 4.0)
}

/// Show a toast with severity colors and a custom duration.
pub fn show_with(world: &mut World, message: &str, severity: Severity, secs: f32) -> Entity {
    let host = ensure_host(world);
    let now = world
        .get_resource::<Time>()
        .map(|t| t.elapsed_secs())
        .unwrap_or(0.0);
    let (bg, fg) = severity.colors();
    let label = world
        .spawn((
            Node::default(),
            bevy::ui::widget::Text::new(message),
            bevy::text::TextFont {
                font_size: bevy::text::FontSize::Px(13.0),
                font: crate::fonts::default_font(),
                ..Default::default()
            },
            bevy::text::TextColor(fg),
        ))
        .id();
    let toast = world
        .spawn((
            Node {
                padding: UiRect::axes(Val::Px(14.0), Val::Px(9.0)),
                border_radius: bevy::ui::BorderRadius::all(Val::Px(6.0)),
                ..Default::default()
            },
            BackgroundColor(bg),
            PfToast { expires_at: now + secs.max(0.5) },
        ))
        .id();
    world.entity_mut(toast).add_children(&[label]);
    world.entity_mut(host).add_children(&[toast]);
    toast
}

/// Despawn toasts whose time is up.
pub(crate) fn expire_toasts(
    time: Res<Time>,
    toasts: Query<(Entity, &PfToast)>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs();
    for (entity, toast) in &toasts {
        if now >= toast.expires_at {
            commands.entity(entity).despawn();
        }
    }
}
