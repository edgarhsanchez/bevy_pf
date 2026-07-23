//! E2E: wheel/trackpad scrolling — `Pointer<Scroll>` on a descendant walks
//! up to the nearest scrollable ancestor and moves its `ScrollPosition`.

use bevy::asset::AssetPlugin;
use bevy::picking::backend::HitData;
use bevy::picking::events::{Pointer, Scroll};
use bevy::picking::pointer::{Location, PointerId};
use bevy::prelude::*;
use bevy_pf::prelude::*;
use bevy_pf::{XamlEnv, instantiate_document_env};

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()));
    app.add_plugins(PfUiPlugin);
    app
}

fn location(world: &mut World, position: Vec2) -> Location {
    let window = world.spawn(bevy::window::Window::default()).id();
    Location {
        target: bevy::camera::RenderTarget::Window(bevy::window::WindowRef::Entity(window))
            .normalize(None)
            .expect("explicit window ref always normalizes"),
        position,
    }
}

#[test]
fn wheel_scrolls_nearest_scrollable_ancestor() {
    let mut app = test_app();
    let doc = bevy_pf_xaml::parse(
        r##"<ScrollViewer xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                          xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
                          x:Name="SV" Height="100">
              <StackPanel>
                <TextBlock x:Name="Row" Text="row 0"/>
                <TextBlock Text="row 1"/>
                <TextBlock Text="row 2"/>
              </StackPanel>
            </ScrollViewer>"##,
    )
    .expect("parses");
    let world = app.world_mut();
    let root = world.spawn_empty().id();
    let result =
        instantiate_document_env(world, root, &doc, &XamlEnv::default()).expect("instantiates");
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    app.update();

    let names = app.world().get::<XamlNames>(root).unwrap();
    let viewer = names.get("SV").unwrap();
    let row = names.get("Row").unwrap();

    // Scrolling starts at the top.
    let start = app
        .world()
        .get::<bevy::ui::ScrollPosition>(viewer)
        .map(|p| p.0.y)
        .unwrap_or(0.0);
    assert_eq!(start, 0.0);

    // A wheel tick over an inner row (3 lines down = -3 in wheel units).
    let loc = location(app.world_mut(), Vec2::new(10.0, 10.0));
    app.world_mut().trigger(Pointer::new(
        PointerId::Mouse,
        loc,
        Scroll {
            unit: bevy::input::mouse::MouseScrollUnit::Line,
            x: 0.0,
            y: -3.0,
            hit: HitData::new(Entity::PLACEHOLDER, 0.0, None, None),
            phase: bevy::input::touch::TouchPhase::Moved,
        },
        row,
    ));
    app.update();

    // The walk found the ScrollViewer (not the row or the stack) and moved
    // it down by 3 lines x 20px.
    let pos = app
        .world()
        .get::<bevy::ui::ScrollPosition>(viewer)
        .expect("scroll position on the viewer");
    assert_eq!(pos.0.y, 60.0, "3 wheel lines scroll 60px down");
}

// Focus feedback: the focused text input's control root wears the accent
// border; moving focus restores it and paints the next control.
#[test]
fn focused_text_box_wears_accent_border_and_restores_on_blur() {
    let mut app = test_app();
    let doc = bevy_pf_xaml::parse(
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
              <TextBox x:Name="A" Text="first"/>
              <TextBox x:Name="B" Text="second"/>
            </StackPanel>"##,
    )
    .expect("parses");
    let world = app.world_mut();
    let root = world.spawn_empty().id();
    let result =
        instantiate_document_env(world, root, &doc, &XamlEnv::default()).expect("instantiates");
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    app.update();

    let names = app.world().get::<XamlNames>(root).unwrap();
    let (a, b) = (names.get("A").unwrap(), names.get("B").unwrap());
    let original = *app.world().get::<bevy::ui::BorderColor>(a).unwrap();
    let accent = Color::srgb_u8(0x56, 0x9D, 0xE5);

    // Text inputs are tab stops inside a root tab group.
    let input_a = app
        .world()
        .get::<Children>(a)
        .unwrap()
        .iter()
        .find(|&c| app.world().get::<bevy::text::EditableText>(c).is_some())
        .expect("editable input child");
    assert!(
        app.world()
            .get::<bevy::input_focus::tab_navigation::TabIndex>(input_a)
            .is_some(),
        "inputs are tab stops"
    );
    assert!(
        app.world()
            .get::<bevy::input_focus::tab_navigation::TabGroup>(root)
            .is_some(),
        "scene root is a tab group"
    );

    // Focusing the INNER editable highlights the OUTER TextBox chrome.
    app.world_mut()
        .insert_resource(bevy::input_focus::InputFocus::from_entity(input_a));
    app.update();
    assert_eq!(
        app.world().get::<bevy::ui::BorderColor>(a).unwrap().top,
        accent,
        "focused TextBox wears the accent border"
    );

    // Moving focus to the other TextBox restores A and paints B.
    let input_b = app
        .world()
        .get::<Children>(b)
        .unwrap()
        .iter()
        .find(|&c| app.world().get::<bevy::text::EditableText>(c).is_some())
        .unwrap();
    app.world_mut()
        .insert_resource(bevy::input_focus::InputFocus::from_entity(input_b));
    app.update();
    assert_eq!(
        *app.world().get::<bevy::ui::BorderColor>(a).unwrap(),
        original,
        "blurred TextBox border restored"
    );
    assert_eq!(
        app.world().get::<bevy::ui::BorderColor>(b).unwrap().top,
        accent
    );

    // Clearing focus restores everything.
    app.world_mut()
        .insert_resource(bevy::input_focus::InputFocus::default());
    app.update();
    assert_eq!(
        *app.world().get::<bevy::ui::BorderColor>(b).unwrap(),
        original,
        "cleared focus restores the last control"
    );
}

// Keyboard interaction: focused buttons wear an outline and activate on
// Space; a focused ListBox moves its selection with the arrow keys.
#[test]
fn keyboard_activation_and_arrow_selection() {
    let mut app = test_app();
    // The synthetic click needs a window to anchor its pointer location.
    app.world_mut().spawn(bevy::window::Window::default());
    let doc = bevy_pf_xaml::parse(
        r##"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
              <Button x:Name="Go" Content="Go"/>
              <ListBox x:Name="L">
                <ListBoxItem>one</ListBoxItem>
                <ListBoxItem>two</ListBoxItem>
                <ListBoxItem>three</ListBoxItem>
              </ListBox>
            </StackPanel>"##,
    )
    .expect("parses");
    let world = app.world_mut();
    let root = world.spawn_empty().id();
    let result =
        instantiate_document_env(world, root, &doc, &XamlEnv::default()).expect("instantiates");
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    app.update();

    let names = app.world().get::<XamlNames>(root).unwrap();
    let (button, list) = (names.get("Go").unwrap(), names.get("L").unwrap());

    // Controls are tab stops.
    assert!(
        app.world()
            .get::<bevy::input_focus::tab_navigation::TabIndex>(button)
            .is_some()
    );

    // Focusing the button draws the focus outline, not a border rewrite.
    let clicks = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let counter = clicks.clone();
    app.world_mut().entity_mut(button).observe(
        move |_: On<bevy::picking::events::Pointer<bevy::picking::events::Click>>| {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        },
    );
    app.world_mut()
        .insert_resource(bevy::input_focus::InputFocus::from_entity(button));
    app.update();
    assert!(
        app.world().get::<bevy::ui::Outline>(button).is_some(),
        "focused button wears the outline"
    );

    // Space activates it.
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Space);
    app.update();
    assert_eq!(clicks.load(std::sync::atomic::Ordering::SeqCst), 1, "Space clicked");
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();

    // Focus the ListBox; ArrowDown selects the first, then second item.
    app.world_mut()
        .insert_resource(bevy::input_focus::InputFocus::from_entity(list));
    app.update();
    assert!(
        app.world().get::<bevy::ui::Outline>(button).is_none(),
        "outline moved off the button"
    );
    let items: Vec<Entity> = app.world().get::<Children>(list).unwrap().iter().collect();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ArrowDown);
    app.update();
    assert_eq!(
        app.world()
            .get::<bevy_pf::components::PfListBox>(list)
            .unwrap()
            .selected,
        Some(items[0]),
        "first ArrowDown selects the first item"
    );
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ArrowDown);
    app.update();
    assert_eq!(
        app.world()
            .get::<bevy_pf::components::PfListBox>(list)
            .unwrap()
            .selected,
        Some(items[1]),
        "second ArrowDown advances"
    );
}
