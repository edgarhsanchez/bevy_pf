//! MessageBox-style modal dialogs.

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy_pf::dialog::{PfDialogResult, close_dialog, show_message};
use bevy_pf::prelude::*;

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()));
    app.add_plugins(PfUiPlugin);
    app
}

#[test]
fn dialog_shows_buttons_and_reports_result() {
    let mut app = test_app();
    let dialog = show_message(
        app.world_mut(),
        "Quit?",
        "Save your work before leaving?",
        &["Save", "Discard", "Cancel"],
    );

    // Modal chrome: fullscreen scrim on top of everything, three wired buttons.
    let node = app.world().get::<Node>(dialog).unwrap();
    assert_eq!(node.position_type, PositionType::Absolute);
    assert_eq!(node.width, Val::Percent(100.0));
    assert!(app.world().get::<bevy::ui::GlobalZIndex>(dialog).is_some());
    let names = app.world().get::<XamlNames>(dialog).unwrap();
    for i in 0..3 {
        assert!(names.get(&format!("PfDlgBtn{i}")).is_some(), "button {i}");
    }

    // Closing reports the label and despawns the tree.
    close_dialog(app.world_mut(), dialog, "Discard");
    let results: Vec<PfDialogResult> = app
        .world_mut()
        .resource_mut::<Messages<PfDialogResult>>()
        .drain()
        .collect();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].button, "Discard");
    assert_eq!(results[0].dialog, dialog);
    assert!(app.world().get_entity(dialog).is_err());

    // Double-close is a no-op.
    close_dialog(app.world_mut(), dialog, "Save");
    assert!(
        app.world_mut()
            .resource_mut::<Messages<PfDialogResult>>()
            .drain()
            .next()
            .is_none()
    );
}
