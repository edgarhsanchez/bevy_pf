//! A wrapping TextBlock with no explicit Width, stacked vertically inside a
//! Border (or any grid cell), used to hand the panel a phantom band below its
//! content: roughly (words - rendered lines) x line height per TextBlock, so
//! a 20-word FontSize 9 line that renders on two lines added ~195 px.
//!
//! The mechanism is taffy's layout cache, not text measurement itself. The
//! cell's column asks the StackPanel for its minimum contribution, measured
//! under a min-content constraint where each child is as narrow as it can
//! be (the wrapping text: one word per line). A sibling with an explicit
//! Width equal to the content box fixes the panel's min-content width at
//! exactly the width the cell later hands it, taffy reuses the cached
//! min-content size for that "known width" query, and the row track takes
//! the word-per-line height as its base size. The final layout wraps the
//! text to two lines at the full width; the band is the difference.
//!
//! The framework fix (`instantiate.rs`, `stretch_panels_to_cell`) writes
//! the width a stretching cell would give a vertical panel or nested Grid
//! down explicitly, as 100% of the cell, so the sizing pass resolves it
//! against the definite cell and measures the panel at the width it will
//! really get; there is no narrow measurement left to cache. A zero minimum
//! width was tried first and changed nothing: the poisoned entry is the
//! panel's own, and the row track takes whatever the panel reports.

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy_pf::prelude::*;

fn layout_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        bevy::window::WindowPlugin {
            primary_window: None,
            exit_condition: bevy::window::ExitCondition::DontExit,
            ..Default::default()
        },
        bevy::a11y::AccessibilityPlugin,
        bevy::input::InputPlugin,
        bevy::picking::DefaultPickingPlugins,
        bevy::text::TextPlugin,
        bevy::ui::UiPlugin,
    ));
    // Font atlases live in image/atlas assets; headless needs them
    // registered by hand (no ImagePlugin without a renderer).
    app.init_asset::<Image>();
    app.init_asset::<bevy::image::TextureAtlasLayout>();
    app.add_plugins(PfUiPlugin);
    app.world_mut().spawn((
        bevy::window::Window {
            resolution: bevy::window::WindowResolution::new(1280, 800),
            ..Default::default()
        },
        bevy::window::PrimaryWindow,
    ));
    // No renderer runs headless, so the camera never learns its target
    // size; stamp it by hand and percent-sized UI roots resolve normally.
    let mut camera = Camera::default();
    camera.computed.target_info = Some(bevy::camera::RenderTargetInfo {
        physical_size: UVec2::new(1280, 800),
        scale_factor: 1.0,
    });
    app.world_mut().spawn((Camera2d, camera));
    app
}

const NS: &str = r#"xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml""#;

/// Twenty words: at FontSize 9 they wrap to two or three lines in a 444 px
/// box, and to twenty lines one word at a time.
const TWENTY: &str = "alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo lima mike november oscar papa quebec romeo sierra tango";
const FIFTEEN: &str =
    "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen";

/// The Border in the game panel: 470 wide, 12/10 padding, a 1 px border,
/// so its content box is 444 wide — the width its fixed TextBlocks carry.
const BORDER_INSET_X: f32 = 2.0 * 12.0 + 2.0;
const BORDER_INSET_Y: f32 = 2.0 * 10.0 + 2.0;
const CONTENT_WIDTH: f32 = 470.0 - BORDER_INSET_X;

/// Well under any word-per-line height (20 x 9 px = 180 px at least) and
/// above any sane wrapped height for twenty 9 px words in 444 px.
const WRAPPED_HEIGHT_CEILING: f32 = 60.0;

fn instantiate(app: &mut App, xaml: &str) -> Entity {
    let doc = bevy_pf_xaml::parse(xaml).expect("parses");
    let world = app.world_mut();
    let root = world.spawn_empty().id();
    let result = bevy_pf::instantiate_document_env(world, root, &doc, &bevy_pf::XamlEnv::default())
        .expect("instantiates");
    for w in &result.warnings {
        eprintln!("WARN {w}");
    }
    // Text measures arrive a frame after the font resolves; give layout
    // several frames to settle exactly as the other geometry tests do.
    for _ in 0..6 {
        app.update();
    }
    root
}

fn size_of(world: &World, e: Entity) -> Vec2 {
    let node = world.get::<bevy::ui::ComputedNode>(e).expect("computed");
    node.size() * node.inverse_scale_factor()
}

fn named(world: &World, root: Entity, name: &str) -> Entity {
    world
        .get::<XamlNames>(root)
        .expect("XamlNames on root")
        .get(name)
        .unwrap_or_else(|| panic!("x:Name {name}"))
}

fn dump(world: &World, e: Entity, depth: usize) {
    let size = world
        .get::<bevy::ui::ComputedNode>(e)
        .map(|n| n.size() * n.inverse_scale_factor());
    let kind = world
        .get::<bevy_pf::PfElementKind>(e)
        .map(|k| k.0.clone())
        .unwrap_or_default();
    let node = world.get::<Node>(e);
    let text = world
        .get::<bevy::ui::widget::Text>(e)
        .map(|t| t.0.chars().take(24).collect::<String>())
        .unwrap_or_default();
    eprintln!(
        "DIAG {}{kind:<12} size {:?} | display {:?} dir {:?} w {:?} min_w {:?} {text:?}",
        "  ".repeat(depth),
        size,
        node.map(|n| n.display),
        node.map(|n| n.flex_direction),
        node.map(|n| n.width),
        node.map(|n| n.min_width),
    );
    if let Some(children) = world.get::<Children>(e) {
        let kids: Vec<Entity> = children.iter().collect();
        for c in kids {
            dump(world, c, depth + 1);
        }
    }
}

/// The panel under test: a fixed heading, the wrapping body (with or
/// without an explicit Width), a wrapping line that always carries the
/// content-box Width, and a trailing fixed line.
fn panel_xaml(body_width_attr: &str) -> String {
    format!(
        r#"<StackPanel {NS}>
             <Border x:Name="B" Width="470" Padding="12,10" BorderThickness="1" BorderBrush="Black">
               <StackPanel x:Name="S">
                 <TextBlock x:Name="T1" Text="Plain heading"/>
                 <TextBlock x:Name="T2" TextWrapping="Wrap" FontSize="9" {body_width_attr} Text="{TWENTY}"/>
                 <TextBlock x:Name="T3" TextWrapping="Wrap" Width="444" FontSize="9" Text="{FIFTEEN}"/>
                 <TextBlock x:Name="T4" Text="Last line"/>
               </StackPanel>
             </Border>
           </StackPanel>"#
    )
}

/// Border height must be exactly its padded content: the StackPanel's
/// height, which in turn is exactly the sum of its children.
fn assert_vertical_panel_is_tight(app: &App, label: &str, root: Entity) {
    let world = app.world();
    dump(world, root, 0);
    let border = size_of(world, named(world, root, "B"));
    let stack = size_of(world, named(world, root, "S"));
    let texts: Vec<Vec2> = ["T1", "T2", "T3", "T4"]
        .iter()
        .map(|n| size_of(world, named(world, root, n)))
        .collect();
    let sum: f32 = texts.iter().map(|t| t.y).sum();
    eprintln!(
        "{label}: border {border:?} stack {stack:?} texts {texts:?} sum {sum} \
         phantom(border - content) {}",
        border.y - (sum + BORDER_INSET_Y)
    );
    assert!(
        (border.x - 470.0).abs() <= 0.5,
        "{label}: Border keeps its explicit 470 width, got {}",
        border.x
    );
    for (i, t) in texts.iter().enumerate() {
        assert!(
            t.y < WRAPPED_HEIGHT_CEILING,
            "{label}: T{} is {} px tall — laid out one word per line",
            i + 1,
            t.y
        );
    }
    // Default HorizontalAlignment is Stretch: the wrapping text takes the
    // content box, with or without an explicit Width.
    assert!(
        (texts[1].x - CONTENT_WIDTH).abs() <= 1.0,
        "{label}: wrapping T2 fills the {CONTENT_WIDTH} px content box, got {}",
        texts[1].x
    );
    assert!(
        (stack.y - sum).abs() <= 1.0,
        "{label}: StackPanel {} px tall vs children {sum} px",
        stack.y
    );
    assert!(
        (border.y - (stack.y + BORDER_INSET_Y)).abs() <= 1.0,
        "{label}: Border {} px tall vs content {} px — phantom band of {} px",
        border.y,
        stack.y + BORDER_INSET_Y,
        border.y - (stack.y + BORDER_INSET_Y)
    );
}

#[test]
fn wrapping_textblock_without_width_adds_no_phantom_height() {
    let mut app = layout_app();
    app.update();
    let root = instantiate(&mut app, &panel_xaml(""));
    assert_vertical_panel_is_tight(&app, "no Width", root);
}

#[test]
fn wrapping_textblock_with_width_adds_no_phantom_height() {
    let mut app = layout_app();
    app.update();
    let root = instantiate(&mut app, &panel_xaml(r#"Width="444""#));
    assert_vertical_panel_is_tight(&app, "Width=444", root);
}

/// The same panel through a real Grid cell: the Grid stretches to the 444
/// content box, its implicit auto column is exactly the panel's min-content
/// width, and the same cache reuse applied.
#[test]
fn wrapping_textblock_in_grid_cell_adds_no_phantom_height() {
    let mut app = layout_app();
    app.update();
    let xaml = format!(
        r#"<StackPanel {NS}>
             <Border x:Name="B" Width="470" Padding="12,10" BorderThickness="1" BorderBrush="Black">
               <Grid>
                 <StackPanel x:Name="S">
                   <TextBlock x:Name="T1" Text="Plain heading"/>
                   <TextBlock x:Name="T2" TextWrapping="Wrap" FontSize="9" Text="{TWENTY}"/>
                   <TextBlock x:Name="T3" TextWrapping="Wrap" Width="444" FontSize="9" Text="{FIFTEEN}"/>
                   <TextBlock x:Name="T4" Text="Last line"/>
                 </StackPanel>
               </Grid>
             </Border>
           </StackPanel>"#
    );
    let root = instantiate(&mut app, &xaml);
    assert_vertical_panel_is_tight(&app, "Grid cell", root);
}

/// The control: a wrapping TextBlock in a HORIZONTAL StackPanel is a flex
/// row item, not a stretched column child. Pinned as it behaves today so
/// the fix (scoped to vertical panels) is seen not to touch it: the row
/// keeps an auto width, the text keeps its unwrapped width and runs past
/// the content box on one line, as WPF's unbounded row would have it, and
/// the row is exactly as tall as its tallest child — no band.
#[test]
fn wrapping_textblock_in_horizontal_stackpanel_keeps_row_behaviour() {
    let mut app = layout_app();
    app.update();
    let xaml = format!(
        r#"<StackPanel {NS}>
             <Border x:Name="B" Width="470" Padding="12,10" BorderThickness="1" BorderBrush="Black">
               <StackPanel x:Name="S" Orientation="Horizontal">
                 <TextBlock x:Name="T1" Text="Plain"/>
                 <TextBlock x:Name="T2" TextWrapping="Wrap" FontSize="9" Text="{TWENTY}"/>
               </StackPanel>
             </Border>
           </StackPanel>"#
    );
    let root = instantiate(&mut app, &xaml);
    let world = app.world();
    dump(world, root, 0);
    let border = size_of(world, named(world, root, "B"));
    let stack = size_of(world, named(world, root, "S"));
    let t1 = size_of(world, named(world, root, "T1"));
    let t2 = size_of(world, named(world, root, "T2"));
    eprintln!("horizontal: border {border:?} stack {stack:?} T1 {t1:?} T2 {t2:?}");
    // The horizontal panel is not a vertical panel: the fix leaves its
    // width and minimum width alone.
    let stack_node = world.get::<Node>(named(world, root, "S")).unwrap();
    assert_eq!(stack_node.flex_direction, FlexDirection::Row);
    assert_eq!(stack_node.width, Val::Auto);
    assert_eq!(stack_node.min_width, Val::Auto);
    // WPF hands a row's children unbounded width, so a wrapping TextBlock
    // in a horizontal StackPanel does not wrap: it runs past the content
    // box on one line. That is the behaviour before the fix, pinned so
    // the fix cannot quietly change it.
    assert!(
        t2.x > CONTENT_WIDTH,
        "a row item keeps its unwrapped width and overflows the {CONTENT_WIDTH} px box, got {}",
        t2.x
    );
    assert!(
        t2.y < WRAPPED_HEIGHT_CEILING,
        "one line, not one word per line: {} px",
        t2.y
    );
    let tallest = t1.y.max(t2.y);
    assert!(
        (stack.y - tallest).abs() <= 1.0,
        "row height {} is its tallest child {tallest}",
        stack.y
    );
    assert!(
        (border.y - (stack.y + BORDER_INSET_Y)).abs() <= 1.0,
        "Border {} px tall vs content {} px",
        border.y,
        stack.y + BORDER_INSET_Y
    );
}
