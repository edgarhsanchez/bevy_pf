//! WPF `MessageBox`-style modal dialogs.
//!
//! [`show_message`] spawns a modal overlay (scrim + centered panel) above
//! everything, wires each button, and returns the dialog root. Clicking any
//! button despawns the dialog and writes a [`PfDialogResult`] message:
//!
//! ```ignore
//! // From an exclusive system or `commands.queue`:
//! bevy_pf::dialog::show_message(world, "Quit?", "Save before leaving?",
//!                               &["Save", "Discard", "Cancel"]);
//!
//! // Elsewhere, react to the answer:
//! fn on_answer(mut results: MessageReader<PfDialogResult>) {
//!     for r in results.read() {
//!         if r.button == "Save" { /* ... */ }
//!     }
//! }
//! ```

use bevy::prelude::*;

use crate::XamlEnv;
use crate::components::XamlNames;

/// Written when a dialog button is clicked (after the dialog despawns).
#[derive(Message, Debug, Clone)]
pub struct PfDialogResult {
    /// The root entity `show_message` returned (already despawned).
    pub dialog: Entity,
    /// The clicked button's label.
    pub button: String,
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Show a modal message box. Returns the dialog root entity.
pub fn show_message(world: &mut World, title: &str, body: &str, buttons: &[&str]) -> Entity {
    let mut btns = String::new();
    for (i, label) in buttons.iter().enumerate() {
        btns.push_str(&format!(
            r##"<Button x:Name="PfDlgBtn{i}" Content="{}" MinWidth="88" Margin="8,0,0,0"/>"##,
            escape(label)
        ));
    }
    let xaml = format!(
        r##"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                  xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                  Background="#66000000">
              <Border Background="#FFF7F7F7" BorderBrush="#FF8A8A8A" BorderThickness="1"
                      CornerRadius="8" Padding="16" MinWidth="320" MaxWidth="480"
                      HorizontalAlignment="Center" VerticalAlignment="Center">
                <StackPanel>
                  <TextBlock Text="{}" FontSize="16" FontWeight="Bold" Foreground="#FF1A1A1A" Margin="0,0,0,8"/>
                  <TextBlock Text="{}" TextWrapping="Wrap" Foreground="#FF333333" Margin="0,0,0,16"/>
                  <StackPanel Orientation="Horizontal" HorizontalAlignment="Right">{btns}</StackPanel>
                </StackPanel>
              </Border>
            </Grid>"##,
        escape(title),
        escape(body),
    );

    let doc = bevy_pf_xaml::parse(&xaml).expect("dialog template is valid XAML");
    let root = world.spawn_empty().id();
    if let Err(e) = crate::instantiate_document_env(world, root, &doc, &XamlEnv::default()) {
        warn!("bevy_pf: dialog failed to instantiate: {e}");
        return root;
    }

    // Cover the whole viewport, above every UI layer (including popups).
    if let Some(mut node) = world.get_mut::<Node>(root) {
        node.position_type = PositionType::Absolute;
        // Center via flex, not grid: a grid item with Min/MaxWidth hands its
        // child the OUTER width instead of the padded content box (taffy
        // 0.10 grid sizing bug — see tests/layout_geometry.rs), which hung
        // dialog buttons past the panel edge. Flex sizes the content box
        // correctly.
        node.display = Display::Flex;
        node.flex_direction = FlexDirection::Column;
        node.align_items = AlignItems::Center;
        node.justify_content = JustifyContent::Center;
        node.left = Val::Px(0.0);
        node.top = Val::Px(0.0);
        node.width = Val::Percent(100.0);
        node.height = Val::Percent(100.0);
    }
    world
        .entity_mut(root)
        .insert((bevy::ui::GlobalZIndex(i32::MAX - 64), Interaction::default()));

    // Wire each button: despawn + report.
    let names: Vec<(usize, Entity)> = world
        .get::<XamlNames>(root)
        .map(|n| {
            (0..buttons.len())
                .filter_map(|i| n.get(&format!("PfDlgBtn{i}")).map(|e| (i, e)))
                .collect()
        })
        .unwrap_or_default();
    for (i, button_entity) in names {
        let label = buttons[i].to_string();
        world.entity_mut(button_entity).observe(
            move |_: On<Pointer<Click>>, mut commands: Commands| {
                let label = label.clone();
                commands.queue(move |world: &mut World| {
                    close_dialog(world, root, &label);
                });
            },
        );
    }
    root
}

/// WinUI-style `ContentDialog`: a modal that hosts an arbitrary XAML scene
/// above a scrim, with a row of result buttons. Buttons close the dialog and
/// write [`PfDialogResult`], exactly like [`show_message`].
///
/// ```ignore
/// bevy_pf::dialog::show_content(
///     world,
///     "About",
///     &xaml!(r#"<StackPanel xmlns="..."> ... </StackPanel>"#),
///     &["OK", "Cancel"],
/// );
/// ```
pub fn show_content(
    world: &mut World,
    title: &str,
    content: &crate::XamlScene,
    buttons: &[&str],
) -> Entity {
    let mut btns = String::new();
    for (i, label) in buttons.iter().enumerate() {
        btns.push_str(&format!(
            r##"<Button x:Name="PfDlgBtn{i}" Content="{}" MinWidth="88" Margin="8,0,0,0"/>"##,
            escape(label)
        ));
    }
    let xaml = format!(
        r##"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                  xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                  Background="#66000000">
              <Border Background="#FFF7F7F7" BorderBrush="#FF8A8A8A" BorderThickness="1"
                      CornerRadius="8" Padding="16" MinWidth="320" MaxWidth="520"
                      HorizontalAlignment="Center" VerticalAlignment="Center">
                <StackPanel>
                  <TextBlock Text="{}" FontSize="16" FontWeight="Bold" Foreground="#FF1A1A1A" Margin="0,0,0,10"/>
                  <Border x:Name="PfDlgContent" Margin="0,0,0,16"/>
                  <StackPanel Orientation="Horizontal" HorizontalAlignment="Right">{btns}</StackPanel>
                </StackPanel>
              </Border>
            </Grid>"##,
        escape(title),
    );

    let doc = bevy_pf_xaml::parse(&xaml).expect("dialog template is valid XAML");
    let root = world.spawn_empty().id();
    if let Err(e) = crate::instantiate_document_env(world, root, &doc, &XamlEnv::default()) {
        warn!("bevy_pf: content dialog failed to instantiate: {e}");
        return root;
    }

    // Instantiate the caller's scene into the content host.
    let host = world
        .get::<XamlNames>(root)
        .and_then(|n| n.get("PfDlgContent"));
    if let Some(host) = host {
        let inner = world.spawn_empty().id();
        match crate::instantiate_document_env(world, inner, &content.document(), &XamlEnv::default())
        {
            Ok(_) => {
                world.entity_mut(host).add_children(&[inner]);
            }
            Err(e) => {
                warn!("bevy_pf: content dialog body failed to instantiate: {e}");
                world.entity_mut(inner).despawn();
            }
        }
    }

    // Cover the whole viewport, above every UI layer (including popups).
    if let Some(mut node) = world.get_mut::<Node>(root) {
        node.position_type = PositionType::Absolute;
        // Center via flex, not grid: a grid item with Min/MaxWidth hands its
        // child the OUTER width instead of the padded content box (taffy
        // 0.10 grid sizing bug — see tests/layout_geometry.rs), which hung
        // dialog buttons past the panel edge. Flex sizes the content box
        // correctly.
        node.display = Display::Flex;
        node.flex_direction = FlexDirection::Column;
        node.align_items = AlignItems::Center;
        node.justify_content = JustifyContent::Center;
        node.left = Val::Px(0.0);
        node.top = Val::Px(0.0);
        node.width = Val::Percent(100.0);
        node.height = Val::Percent(100.0);
    }
    world
        .entity_mut(root)
        .insert((bevy::ui::GlobalZIndex(i32::MAX - 64), Interaction::default()));

    // Wire each button: despawn + report.
    let names: Vec<(usize, Entity)> = world
        .get::<XamlNames>(root)
        .map(|n| {
            (0..buttons.len())
                .filter_map(|i| n.get(&format!("PfDlgBtn{i}")).map(|e| (i, e)))
                .collect()
        })
        .unwrap_or_default();
    for (i, button_entity) in names {
        let label = buttons[i].to_string();
        world.entity_mut(button_entity).observe(
            move |_: On<Pointer<Click>>, mut commands: Commands| {
                let label = label.clone();
                commands.queue(move |world: &mut World| {
                    close_dialog(world, root, &label);
                });
            },
        );
    }
    root
}

/// Close a dialog programmatically, reporting `button` as the result.
pub fn close_dialog(world: &mut World, dialog: Entity, button: &str) {
    if world.get_entity(dialog).is_err() {
        return; // already closed
    }
    world.write_message(PfDialogResult {
        dialog,
        button: button.to_string(),
    });
    world.entity_mut(dialog).despawn();
}
