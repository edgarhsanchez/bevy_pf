//! WPF navigation: Frame + journal + page-navigating Hyperlinks.

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy_pf::components::PfFrame;
use bevy_pf::navigation::{
    PfNavigated, can_go_back, can_go_forward, follow_hyperlink, go_back, go_forward, navigate,
};
use bevy_pf::prelude::*;
use bevy_pf::{XamlEnv, instantiate_document_env};

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()));
    app.add_plugins(PfUiPlugin);
    app.register_page(
        "home.xaml",
        XamlScene::parse(
            r#"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                     xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="Home">
                 <StackPanel>
                   <TextBlock x:Name="HomeText" Text="home page"/>
                   <Hyperlink x:Name="ToDetails" NavigateUri="details.xaml">Details</Hyperlink>
                 </StackPanel>
               </Page>"#,
        )
        .unwrap(),
    );
    app.register_page(
        "details.xaml",
        XamlScene::parse(
            r#"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                     xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="Details">
                 <TextBlock x:Name="DetailsText" Text="details page"/>
               </Page>"#,
        )
        .unwrap(),
    );
    app
}

fn spawn_frame(app: &mut App) -> Entity {
    let doc = bevy_pf_xaml::parse(
        r#"<Frame xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                  xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                  Source="home.xaml"/>"#,
    )
    .unwrap();
    let world = app.world_mut();
    let root = world.spawn_empty().id();
    let result = instantiate_document_env(world, root, &doc, &XamlEnv::default()).unwrap();
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    app.update(); // init_pending_frames resolves Source=
    root
}

/// The current page's XamlNames map (pages are scenes of their own).
fn page_names(app: &mut App, frame: Entity) -> Vec<String> {
    let content = app.world().get::<PfFrame>(frame).unwrap().content;
    let page_root = app
        .world()
        .get::<Children>(content)
        .and_then(|c| c.iter().next())
        .expect("frame has a page");
    app.world()
        .get::<XamlNames>(page_root)
        .map(|n| n.0.keys().cloned().collect())
        .unwrap_or_default()
}

#[test]
fn frame_source_journal_and_titles() {
    let mut app = test_app();
    let frame = spawn_frame(&mut app);

    // Source= landed on the home page, title captured.
    let f = app.world().get::<PfFrame>(frame).unwrap();
    assert_eq!(f.current.as_deref(), Some("home.xaml"));
    assert_eq!(f.current_title.as_deref(), Some("Home"));
    assert!(page_names(&mut app, frame).contains(&"HomeText".to_string()));
    assert!(!can_go_back(app.world(), frame));

    // Navigate: journal grows, content replaced, KeepAlive=false semantics.
    assert!(navigate(app.world_mut(), frame, "details.xaml"));
    assert!(page_names(&mut app, frame).contains(&"DetailsText".to_string()));
    assert!(can_go_back(app.world(), frame));
    assert!(!can_go_forward(app.world(), frame));
    assert_eq!(
        app.world()
            .get::<PfFrame>(frame)
            .unwrap()
            .current_title
            .as_deref(),
        Some("Details")
    );

    // Back restores home (re-instantiated), forward becomes available.
    assert!(go_back(app.world_mut(), frame));
    assert!(page_names(&mut app, frame).contains(&"HomeText".to_string()));
    assert!(can_go_forward(app.world(), frame));
    assert!(go_forward(app.world_mut(), frame));
    assert!(page_names(&mut app, frame).contains(&"DetailsText".to_string()));

    // Every hop reported a PfNavigated message.
    let sources: Vec<String> = app
        .world_mut()
        .resource_mut::<Messages<PfNavigated>>()
        .drain()
        .map(|n| n.source)
        .collect();
    assert_eq!(
        sources,
        vec!["home.xaml", "details.xaml", "home.xaml", "details.xaml"]
    );
}

#[test]
fn hyperlinks_navigate_the_enclosing_frame() {
    let mut app = test_app();
    let frame = spawn_frame(&mut app);

    // The home page's relative Hyperlink navigates the frame, not a browser.
    let content = app.world().get::<PfFrame>(frame).unwrap().content;
    let page_root = app
        .world()
        .get::<Children>(content)
        .and_then(|c| c.iter().next())
        .unwrap();
    let link = app
        .world()
        .get::<XamlNames>(page_root)
        .unwrap()
        .get("ToDetails")
        .unwrap();
    follow_hyperlink(app.world_mut(), link, "details.xaml");

    let f = app.world().get::<PfFrame>(frame).unwrap();
    assert_eq!(f.current.as_deref(), Some("details.xaml"));
    assert_eq!(f.back, vec!["home.xaml".to_string()]);
}
