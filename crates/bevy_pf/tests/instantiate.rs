//! Headless instantiation tests: XAML -> bevy_ui entity trees.

use bevy::prelude::*;
use bevy::text::{FontSize, TextColor, TextFont};
use bevy::ui::widget::Text;
use bevy::ui::{BorderColor, GridPlacement, Val};
use bevy_pf::prelude::*;
use bevy_pf::{ButtonVisual, PfElementKind, instantiate_document};

const PRES: &str = r#"xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation""#;
const X: &str = r#"xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml""#;

/// Instantiate XAML into a fresh world; returns (world, root).
fn spawn(xaml: &str) -> (World, Entity) {
    let mut world = World::new();
    let doc = bevy_pf_xaml::parse(xaml).expect("test XAML parses");
    let root = world.spawn_empty().id();
    let result = instantiate_document(&mut world, root, &doc).expect("instantiates");
    (world, result.root)
}

fn children_of(world: &World, e: Entity) -> Vec<Entity> {
    world
        .get::<Children>(e)
        .map(|c| c.iter().collect())
        .unwrap_or_default()
}

fn find_by_kind(world: &mut World, kind: &str) -> Vec<Entity> {
    let mut q = world.query::<(Entity, &PfElementKind)>();
    q.iter(world)
        .filter(|(_, k)| k.0 == kind)
        .map(|(e, _)| e)
        .collect()
}

#[test]
fn hello_world_tree() {
    let (mut world, root) = spawn(&format!(
        r#"<Window {PRES} {X} Title="Hi" Width="800" Height="450">
             <Grid>
               <TextBlock VerticalAlignment="Center" HorizontalAlignment="Center">
                 Hello, World!
               </TextBlock>
             </Grid>
           </Window>"#
    ));

    let node = world.get::<Node>(root).unwrap();
    assert_eq!(node.width, Val::Px(800.0));
    assert_eq!(node.display, Display::Grid);
    // Window gets a white default background.
    assert_eq!(
        world.get::<BackgroundColor>(root).unwrap().0,
        Color::WHITE
    );

    let grid = children_of(&world, root)[0];
    assert_eq!(world.get::<PfElementKind>(grid).unwrap().0, "Grid");

    let tb = children_of(&world, grid)[0];
    assert_eq!(world.get::<Text>(tb).unwrap().0, "Hello, World!");
    // Centered in the grid parent.
    let tb_node = world.get::<Node>(tb).unwrap();
    assert_eq!(tb_node.justify_self, JustifySelf::Center);
    assert_eq!(tb_node.align_self, AlignSelf::Center);

    let _ = find_by_kind(&mut world, "TextBlock");
}

#[test]
fn grid_definitions_and_placement() {
    let (world, root) = spawn(&format!(
        r#"<Grid {PRES}>
             <Grid.RowDefinitions>
               <RowDefinition Height="Auto"/>
               <RowDefinition Height="*"/>
               <RowDefinition Height="2*"/>
               <RowDefinition Height="100"/>
             </Grid.RowDefinitions>
             <Grid.ColumnDefinitions>
               <ColumnDefinition Width="230"/>
               <ColumnDefinition/>
             </Grid.ColumnDefinitions>
             <TextBlock Text="a"/>
             <TextBlock Text="b" Grid.Row="1" Grid.Column="1"/>
             <TextBlock Text="c" Grid.Row="3" Grid.ColumnSpan="2"/>
             <TextBlock Text="clamped" Grid.Row="99"/>
           </Grid>"#
    ));

    let node = world.get::<Node>(root).unwrap();
    assert_eq!(node.grid_template_rows.len(), 4);
    assert_eq!(node.grid_template_columns.len(), 2);

    let kids = children_of(&world, root);
    let n = |i: usize| world.get::<Node>(kids[i]).unwrap();
    assert_eq!(n(0).grid_row, GridPlacement::start_span(1, 1));
    assert_eq!(n(0).grid_column, GridPlacement::start_span(1, 1));
    assert_eq!(n(1).grid_row, GridPlacement::start_span(2, 1));
    assert_eq!(n(1).grid_column, GridPlacement::start_span(2, 1));
    assert_eq!(n(2).grid_row, GridPlacement::start_span(4, 1));
    assert_eq!(n(2).grid_column, GridPlacement::start_span(1, 2));
    // Out-of-range row clamps to the last row, like WPF.
    assert_eq!(n(3).grid_row, GridPlacement::start_span(4, 1));
}

#[test]
fn grid_shorthand_definitions() {
    // .NET 10 / Avalonia string shorthand.
    let (world, root) = spawn(&format!(
        r#"<Grid {PRES} RowDefinitions="Auto, *, Auto" ColumnDefinitions="*, 2*"/>"#
    ));
    let node = world.get::<Node>(root).unwrap();
    assert_eq!(node.grid_template_rows.len(), 3);
    assert_eq!(node.grid_template_columns.len(), 2);
}

#[test]
fn stackpanel_orientation_and_spacing() {
    let (world, root) = spawn(&format!(
        r#"<StackPanel {PRES} Orientation="Horizontal" Spacing="8" Margin="24">
             <Button Content="A"/>
             <Button Content="B"/>
           </StackPanel>"#
    ));
    let node = world.get::<Node>(root).unwrap();
    assert_eq!(node.display, Display::Flex);
    assert_eq!(node.flex_direction, FlexDirection::Row);
    assert_eq!(node.column_gap, Val::Px(8.0));
    assert_eq!(node.margin, UiRect::all(Val::Px(24.0)));
    assert_eq!(children_of(&world, root).len(), 2);
}

#[test]
fn alignment_in_vertical_stack_maps_to_align_self() {
    let (world, root) = spawn(&format!(
        r#"<StackPanel {PRES}>
             <Button Content="L" HorizontalAlignment="Left"/>
             <Button Content="C" HorizontalAlignment="Center"/>
             <Button Content="R" HorizontalAlignment="Right"/>
           </StackPanel>"#
    ));
    let kids = children_of(&world, root);
    let align = |i: usize| world.get::<Node>(kids[i]).unwrap().align_self;
    assert_eq!(align(0), AlignSelf::FlexStart);
    assert_eq!(align(1), AlignSelf::Center);
    assert_eq!(align(2), AlignSelf::FlexEnd);
}

#[test]
fn border_and_brushes() {
    let (world, root) = spawn(&format!(
        r##"<Border {PRES} Background="AliceBlue" BorderBrush="#FF0000"
                   BorderThickness="2" CornerRadius="8" Padding="5,10">
             <TextBlock Text="in border"/>
           </Border>"##
    ));
    let node = world.get::<Node>(root).unwrap();
    assert_eq!(node.border, UiRect::all(Val::Px(2.0)));
    assert_eq!(node.padding.left, Val::Px(5.0));
    assert_eq!(node.padding.top, Val::Px(10.0));
    assert_eq!(node.border_radius.top_left, Val::Px(8.0));
    assert_eq!(
        world.get::<BackgroundColor>(root).unwrap().0,
        Color::srgba_u8(0xF0, 0xF8, 0xFF, 0xFF)
    );
    let expected = Color::srgba_u8(255, 0, 0, 255);
    assert_eq!(
        *world.get::<BorderColor>(root).unwrap(),
        BorderColor::all(expected)
    );
}

#[test]
fn button_content_and_visuals() {
    let (world, root) = spawn(&format!(
        r#"<Button {PRES} Content="Click me" Background="Green"/>"#
    ));
    assert!(world.get::<ButtonVisual>(root).is_some());
    assert!(world.get::<Interaction>(root).is_some());
    // Background override updates both current color and the normal state.
    let green = Color::srgba_u8(0, 128, 0, 255);
    assert_eq!(world.get::<BackgroundColor>(root).unwrap().0, green);
    assert_eq!(world.get::<ButtonVisual>(root).unwrap().normal_bg, green);
    // Content string became a text child.
    let kids = children_of(&world, root);
    assert_eq!(world.get::<Text>(kids[0]).unwrap().0, "Click me");
}

#[test]
fn resources_and_static_resource() {
    let (world, root) = spawn(&format!(
        r#"<StackPanel {PRES} {X}>
             <StackPanel.Resources>
               <SolidColorBrush x:Key="Bg" Color="CornflowerBlue"/>
               <x:Double x:Key="Big">32</x:Double>
             </StackPanel.Resources>
             <Border Background="{{StaticResource Bg}}">
               <TextBlock Text="hi" FontSize="{{StaticResource Big}}"/>
             </Border>
           </StackPanel>"#
    ));
    let border = children_of(&world, root)[0];
    assert_eq!(
        world.get::<BackgroundColor>(border).unwrap().0,
        Color::srgba_u8(0x64, 0x95, 0xED, 0xFF)
    );
    let tb = children_of(&world, border)[0];
    assert_eq!(
        world.get::<TextFont>(tb).unwrap().font_size,
        FontSize::Px(32.0)
    );
}

#[test]
fn implicit_and_explicit_styles() {
    let (mut world, root) = spawn(&format!(
        r#"<StackPanel {PRES} {X}>
             <StackPanel.Resources>
               <Style TargetType="TextBlock">
                 <Setter Property="FontSize" Value="20"/>
                 <Setter Property="Foreground" Value="Gray"/>
               </Style>
               <Style x:Key="Title" TargetType="TextBlock" BasedOn="{{StaticResource {{x:Type TextBlock}}}}">
                 <Setter Property="FontSize" Value="28"/>
               </Style>
             </StackPanel.Resources>
             <TextBlock Text="implicit"/>
             <TextBlock Text="explicit" Style="{{StaticResource Title}}"/>
             <TextBlock Text="local wins" FontSize="10"/>
           </StackPanel>"#
    ));
    let kids = children_of(&world, root);
    let size = |world: &World, e: Entity| world.get::<TextFont>(e).unwrap().font_size;
    let gray = Color::srgba_u8(0x80, 0x80, 0x80, 0xFF);

    assert_eq!(size(&world, kids[0]), FontSize::Px(20.0));
    assert_eq!(world.get::<TextColor>(kids[0]).unwrap().0, gray);
    // Explicit style: BasedOn keeps Foreground, overrides size.
    assert_eq!(size(&world, kids[1]), FontSize::Px(28.0));
    assert_eq!(world.get::<TextColor>(kids[1]).unwrap().0, gray);
    // Local attribute beats style setter.
    assert_eq!(size(&world, kids[2]), FontSize::Px(10.0));

    let _ = find_by_kind(&mut world, "TextBlock");
}

#[test]
fn font_properties_inherit_down_the_tree() {
    let (world, root) = spawn(&format!(
        r#"<StackPanel {PRES} FontSize="18" Foreground="Navy">
             <Border><TextBlock Text="deep"/></Border>
           </StackPanel>"#
    ));
    let border = children_of(&world, root)[0];
    let tb = children_of(&world, border)[0];
    assert_eq!(
        world.get::<TextFont>(tb).unwrap().font_size,
        FontSize::Px(18.0)
    );
    assert_eq!(
        world.get::<TextColor>(tb).unwrap().0,
        Color::srgba_u8(0, 0, 0x80, 0xFF)
    );
}

#[test]
fn names_are_registered() {
    let (world, root) = spawn(&format!(
        r#"<StackPanel {PRES} {X}>
             <Button x:Name="Go" Content="go"/>
             <TextBlock Name="Status" Text="ready"/>
           </StackPanel>"#
    ));
    let names = world.get::<XamlNames>(root).unwrap();
    let go = names.get("Go").unwrap();
    let status = names.get("Status").unwrap();
    assert_eq!(world.get::<PfElementKind>(go).unwrap().0, "Button");
    assert_eq!(world.get::<Text>(status).unwrap().0, "ready");
}

#[test]
fn visibility_maps_to_display_and_visibility() {
    let (world, root) = spawn(&format!(
        r#"<StackPanel {PRES}>
             <TextBlock Text="h" Visibility="Hidden"/>
             <TextBlock Text="c" Visibility="Collapsed"/>
           </StackPanel>"#
    ));
    let kids = children_of(&world, root);
    assert_eq!(
        *world.get::<Visibility>(kids[0]).unwrap(),
        Visibility::Hidden
    );
    assert_ne!(world.get::<Node>(kids[0]).unwrap().display, Display::None);
    assert_eq!(world.get::<Node>(kids[1]).unwrap().display, Display::None);
}

#[test]
fn canvas_positions_children_absolutely() {
    let (world, root) = spawn(&format!(
        r#"<Canvas {PRES}>
             <TextBlock Text="a" Canvas.Left="10" Canvas.Top="20"/>
             <TextBlock Text="b" Canvas.Right="5" Canvas.Bottom="15"/>
           </Canvas>"#
    ));
    let kids = children_of(&world, root);
    let a = world.get::<Node>(kids[0]).unwrap();
    assert_eq!(a.position_type, PositionType::Absolute);
    assert_eq!(a.left, Val::Px(10.0));
    assert_eq!(a.top, Val::Px(20.0));
    let b = world.get::<Node>(kids[1]).unwrap();
    assert_eq!(b.right, Val::Px(5.0));
    assert_eq!(b.bottom, Val::Px(15.0));
}

#[test]
fn gradient_backgrounds() {
    let (world, root) = spawn(&format!(
        r##"<Border {PRES}>
             <Border.Background>
               <LinearGradientBrush StartPoint="0.5,0" EndPoint="0.5,1">
                 <GradientStop Offset="0.0" Color="#90DDDD"/>
                 <GradientStop Offset="1.0" Color="#5BFFFF"/>
               </LinearGradientBrush>
             </Border.Background>
           </Border>"##
    ));
    let gradient = world.get::<bevy::ui::BackgroundGradient>(root).unwrap();
    assert_eq!(gradient.0.len(), 1);
}

#[test]
fn corpus_wpf_files_instantiate() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/wpf");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("xaml") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        let doc = bevy_pf_xaml::parse(&src).unwrap();
        // ResourceDictionary roots (Styles.xaml) are not spawnable scenes.
        if doc.root.name == "ResourceDictionary" {
            continue;
        }
        let mut world = World::new();
        let root = world.spawn_empty().id();
        let result = instantiate_document(&mut world, root, &doc)
            .unwrap_or_else(|e| panic!("{} failed: {e}", path.display()));
        // Unsupported features must degrade to warnings, never errors.
        let mut q = world.query::<&PfElementKind>();
        assert!(
            q.iter(&world).count() >= 1,
            "{} spawned nothing",
            path.display()
        );
        let _ = result.warnings;
    }
}

// ---------------------------------------------------------------------------
// Controls added in milestone 2
// ---------------------------------------------------------------------------

#[test]
fn checkbox_structure_and_state() {
    use bevy::ui::Checked;
    let (world, root) = spawn(&format!(
        r#"<StackPanel {PRES}>
             <CheckBox Content="Enable sound" IsChecked="True"/>
             <CheckBox Content="Fullscreen"/>
           </StackPanel>"#
    ));
    let kids = children_of(&world, root);
    let checked_box = kids[0];
    let unchecked_box = kids[1];

    assert!(world.get::<Checked>(checked_box).is_some());
    assert!(world.get::<Checked>(unchecked_box).is_none());

    let visual = world.get::<bevy_pf::components::PfCheckVisual>(checked_box).unwrap();
    assert_eq!(
        *world.get::<Visibility>(visual.glyph).unwrap(),
        Visibility::Inherited
    );
    // Checked box is accent-filled.
    assert_eq!(
        world.get::<BackgroundColor>(visual.box_node).unwrap().0,
        bevy_pf::components::ACCENT
    );
    // Content became a text child after the box.
    let cb_kids = children_of(&world, checked_box);
    assert_eq!(world.get::<Text>(cb_kids[1]).unwrap().0, "Enable sound");
}

#[test]
fn radio_buttons_have_groups() {
    use bevy::ui::Checked;
    let (mut world, root) = spawn(&format!(
        r#"<StackPanel {PRES}>
             <RadioButton Content="A" GroupName="G1" IsChecked="True"/>
             <RadioButton Content="B" GroupName="G1"/>
             <RadioButton Content="C"/>
           </StackPanel>"#
    ));
    let kids = children_of(&world, root);
    assert!(world.get::<Checked>(kids[0]).is_some());
    assert_eq!(
        world
            .get::<bevy_pf::components::PfRadioGroup>(kids[0])
            .unwrap()
            .0,
        "G1"
    );
    assert_eq!(
        world
            .get::<bevy_pf::components::PfRadioGroup>(kids[2])
            .unwrap()
            .0,
        ""
    );
    let _ = find_by_kind(&mut world, "RadioButton");
}

#[test]
fn textbox_creates_editable_text() {
    let (mut world, root) = spawn(&format!(
        r#"<StackPanel {PRES}>
             <TextBox Text="hello" MaxLength="10"/>
             <TextBox AcceptsReturn="True"/>
           </StackPanel>"#
    ));
    let kids = children_of(&world, root);
    let input1 = children_of(&world, kids[0])[0];
    let et = world.get::<bevy::text::EditableText>(input1).unwrap();
    assert_eq!(et.editor().text(), "hello");
    assert_eq!(et.max_characters, Some(10));
    assert!(!et.allow_newlines);

    let input2 = children_of(&world, kids[1])[0];
    assert!(
        world
            .get::<bevy::text::EditableText>(input2)
            .unwrap()
            .allow_newlines
    );
    let _ = find_by_kind(&mut world, "TextBox");
}

#[test]
fn slider_components_and_thumb() {
    use bevy::ui_widgets::{SliderRange, SliderValue};
    let (world, root) = spawn(&format!(
        r#"<Slider {PRES} Minimum="0" Maximum="100" Value="25" Width="200"/>"#
    ));
    assert_eq!(world.get::<SliderValue>(root).unwrap().0, 25.0);
    let range = world.get::<SliderRange>(root).unwrap();
    assert_eq!((range.start(), range.end()), (0.0, 100.0));
    let visual = world.get::<bevy_pf::components::PfSliderVisual>(root).unwrap();
    let thumb_node = world.get::<Node>(visual.thumb).unwrap();
    assert_eq!(thumb_node.left, Val::Percent(25.0));
}

#[test]
fn progress_bar_fill() {
    let (world, root) = spawn(&format!(
        r#"<ProgressBar {PRES} Minimum="0" Maximum="200" Value="50" Height="15"/>"#
    ));
    let progress = world.get::<bevy_pf::components::PfProgress>(root).unwrap();
    assert_eq!(progress.fraction(), 0.25);
    let visual = world
        .get::<bevy_pf::components::PfProgressVisual>(root)
        .unwrap();
    assert_eq!(
        world.get::<Node>(visual.fill).unwrap().width,
        Val::Percent(25.0)
    );
}

#[test]
fn uniform_grid_computes_tracks() {
    let (world, root) = spawn(&format!(
        r#"<UniformGrid {PRES}>
             <TextBlock Text="1"/><TextBlock Text="2"/><TextBlock Text="3"/>
             <TextBlock Text="4"/><TextBlock Text="5"/>
           </UniformGrid>"#
    ));
    // 5 children -> 3 columns, 2 rows (ceil(sqrt), like WPF).
    let node = world.get::<Node>(root).unwrap();
    assert_eq!(node.display, Display::Grid);
    assert_eq!(
        node.grid_template_columns,
        vec![bevy::ui::RepeatedGridTrack::fr(3u16, 1.0)]
    );
    assert_eq!(
        node.grid_template_rows,
        vec![bevy::ui::RepeatedGridTrack::fr(2u16, 1.0)]
    );
}

#[test]
fn uniform_grid_explicit_columns() {
    let (world, root) = spawn(&format!(
        r#"<UniformGrid {PRES} Columns="2">
             <TextBlock Text="1"/><TextBlock Text="2"/><TextBlock Text="3"/>
           </UniformGrid>"#
    ));
    let node = world.get::<Node>(root).unwrap();
    assert_eq!(
        node.grid_template_columns,
        vec![bevy::ui::RepeatedGridTrack::fr(2u16, 1.0)]
    );
    assert_eq!(
        node.grid_template_rows,
        vec![bevy::ui::RepeatedGridTrack::fr(2u16, 1.0)]
    );
}

#[test]
fn dock_panel_builds_nested_wrappers() {
    let (mut world, root) = spawn(&format!(
        r#"<DockPanel {PRES}>
             <Border DockPanel.Dock="Top" Height="50"/>
             <Border DockPanel.Dock="Left" Width="100"/>
             <Border/>
           </DockPanel>"#
    ));
    // Root hosts a single chain wrapper.
    let chain = children_of(&world, root);
    assert_eq!(chain.len(), 1);
    // Outermost wrapper: column (Top dock), [top child, rest].
    let outer = chain[0];
    let outer_node = world.get::<Node>(outer).unwrap();
    assert_eq!(outer_node.flex_direction, FlexDirection::Column);
    let outer_kids = children_of(&world, outer);
    assert_eq!(outer_kids.len(), 2);
    assert_eq!(
        world.get::<Node>(outer_kids[0]).unwrap().height,
        Val::Px(50.0)
    );
    // Next wrapper: row (Left dock), [left child, fill slot].
    let mid = outer_kids[1];
    let mid_node = world.get::<Node>(mid).unwrap();
    assert_eq!(mid_node.flex_direction, FlexDirection::Row);
    assert_eq!(mid_node.flex_grow, 1.0);
    let mid_kids = children_of(&world, mid);
    assert_eq!(
        world.get::<Node>(mid_kids[0]).unwrap().width,
        Val::Px(100.0)
    );
    // Innermost: single-cell grid holding the fill child.
    let fill_slot = mid_kids[1];
    assert_eq!(world.get::<Node>(fill_slot).unwrap().display, Display::Grid);
    assert_eq!(children_of(&world, fill_slot).len(), 1);
    let slots = find_by_kind(&mut world, "DockPanel.Slot");
    assert_eq!(slots.len(), 3);
}

#[test]
fn dock_panel_right_and_bottom_order() {
    let (world, root) = spawn(&format!(
        r#"<DockPanel {PRES} LastChildFill="False">
             <Border DockPanel.Dock="Bottom" Height="30"/>
           </DockPanel>"#
    ));
    let outer = children_of(&world, root)[0];
    let node = world.get::<Node>(outer).unwrap();
    assert_eq!(node.flex_direction, FlexDirection::Column);
    let kids = children_of(&world, outer);
    // Bottom dock: filler first, child last.
    assert_eq!(kids.len(), 2);
    assert_eq!(world.get::<Node>(kids[1]).unwrap().height, Val::Px(30.0));
}

#[test]
fn listbox_wraps_items_and_selects() {
    let (world, root) = spawn(&format!(
        r#"<ListBox {PRES} SelectedIndex="1">
             <ListBoxItem>First</ListBoxItem>
             <TextBlock Text="Second (auto-wrapped)"/>
           </ListBox>"#
    ));
    let items = children_of(&world, root);
    assert_eq!(items.len(), 2);
    for &item in &items {
        assert!(world.get::<bevy_pf::components::PfListBoxItem>(item).is_some());
        assert!(world.get::<Interaction>(item).is_some());
    }
    let list = world.get::<bevy_pf::components::PfListBox>(root).unwrap();
    assert_eq!(list.selected, Some(items[1]));
}

#[test]
fn separator_and_toggle_button() {
    use bevy::ui::Checked;
    let (world, root) = spawn(&format!(
        r#"<StackPanel {PRES}>
             <ToggleButton Content="Bold" IsChecked="True"/>
             <Separator/>
           </StackPanel>"#
    ));
    let kids = children_of(&world, root);
    assert!(world.get::<Checked>(kids[0]).is_some());
    assert!(
        world
            .get::<bevy_pf::components::PfToggleButton>(kids[0])
            .is_some()
    );
    assert_eq!(world.get::<Node>(kids[1]).unwrap().height, Val::Px(1.0));
}

#[test]
fn shapes_from_xaml() {
    use bevy_pf::shapes::{PfShape, ShapeGeometry};
    let (world, root) = spawn(&format!(
        r#"<StackPanel {PRES}>
             <Rectangle Fill="Red" Width="100" Height="40" RadiusX="6" RadiusY="6"/>
             <Ellipse Fill="Gold" Stroke="Black" StrokeThickness="2" Width="60" Height="60"/>
             <Path Fill="Green" Data="M 0,40 L 20,0 L 40,40 Z"/>
             <Polygon Points="0,0 30,0 15,20" Fill="Blue"/>
             <Line X1="0" Y1="0" X2="80" Y2="20" Stroke="Gray" StrokeThickness="3"/>
           </StackPanel>"#
    ));
    let kids = children_of(&world, root);
    assert_eq!(kids.len(), 5);

    let rect = world.get::<PfShape>(kids[0]).unwrap();
    assert!(matches!(
        rect.geometry,
        ShapeGeometry::Rectangle { radius_x, .. } if radius_x == 6.0
    ));
    assert!(rect.fill.is_some());

    let ellipse = world.get::<PfShape>(kids[1]).unwrap();
    assert!(matches!(ellipse.geometry, ShapeGeometry::Ellipse));
    assert_eq!(ellipse.stroke_thickness, 2.0);

    // Path with Stretch=None gets its natural size on the node.
    let path_node = world.get::<Node>(kids[2]).unwrap();
    assert_eq!(path_node.width, Val::Px(40.0));
    assert_eq!(path_node.height, Val::Px(40.0));

    let poly = world.get::<PfShape>(kids[3]).unwrap();
    assert!(matches!(
        &poly.geometry,
        ShapeGeometry::Polyline { points, closed: true } if points.len() == 3
    ));

    let line = world.get::<PfShape>(kids[4]).unwrap();
    assert!(matches!(
        line.geometry,
        ShapeGeometry::Line { x2: 80.0, y2: 20.0, .. }
    ));
}

#[test]
fn groupbox_and_expander() {
    use bevy::ui::Checked;
    let (world, root) = spawn(&format!(
        r#"<StackPanel {PRES}>
             <GroupBox Header="Settings">
               <TextBlock Text="content"/>
             </GroupBox>
             <Expander Header="Details" IsExpanded="True">
               <TextBlock Text="expanded content"/>
             </Expander>
             <Expander Header="Hidden">
               <TextBlock Text="collapsed content"/>
             </Expander>
           </StackPanel>"#
    ));
    let kids = children_of(&world, root);

    // GroupBox: header text + content child.
    let gb_kids = children_of(&world, kids[0]);
    assert_eq!(gb_kids.len(), 2);
    assert_eq!(world.get::<Text>(gb_kids[0]).unwrap().0, "Settings");

    // Expanded expander: content visible, Checked present.
    let exp = world.get::<bevy_pf::components::PfExpander>(kids[1]).unwrap();
    assert!(world.get::<Checked>(kids[1]).is_some());
    assert_ne!(world.get::<Node>(exp.content).unwrap().display, Display::None);
    assert_eq!(world.get::<Text>(exp.arrow).unwrap().0, "−");

    // Collapsed expander: content hidden.
    let exp2 = world.get::<bevy_pf::components::PfExpander>(kids[2]).unwrap();
    assert!(world.get::<Checked>(kids[2]).is_none());
    assert_eq!(world.get::<Node>(exp2.content).unwrap().display, Display::None);
}

// ---------------------------------------------------------------------------
// Features driven by the NoesisGUI compatibility sweep
// ---------------------------------------------------------------------------

#[test]
fn render_transform_maps_to_ui_transform() {
    let (world, root) = spawn(&format!(
        r#"<StackPanel {PRES}>
             <Border Width="40" Height="40">
               <Border.RenderTransform>
                 <TransformGroup>
                   <TranslateTransform X="10" Y="5"/>
                   <ScaleTransform ScaleX="2" ScaleY="2"/>
                   <RotateTransform Angle="45"/>
                 </TransformGroup>
               </Border.RenderTransform>
             </Border>
           </StackPanel>"#
    ));
    let border = children_of(&world, root)[0];
    let t = world.get::<bevy::ui::UiTransform>(border).unwrap();
    assert_eq!(t.scale, Vec2::new(2.0, 2.0));
    assert_eq!(t.translation.x, Val::Px(10.0));
    assert_eq!(t.translation.y, Val::Px(5.0));
}

#[test]
fn structured_path_geometry() {
    use bevy_pf::shapes::{PfShape, ShapeGeometry};
    let (world, root) = spawn(&format!(
        r#"<StackPanel {PRES}>
             <Path Fill="Red">
               <Path.Data>
                 <PathGeometry>
                   <PathFigure StartPoint="0,0" IsClosed="True">
                     <LineSegment Point="10,0"/>
                     <BezierSegment Point1="12,2" Point2="12,8" Point3="10,10"/>
                     <ArcSegment Point="0,10" Size="5,5" SweepDirection="Clockwise"/>
                   </PathFigure>
                 </PathGeometry>
               </Path.Data>
             </Path>
             <Path Fill="Blue">
               <Path.Data>
                 <EllipseGeometry Center="20,20" RadiusX="10" RadiusY="8"/>
               </Path.Data>
             </Path>
           </StackPanel>"#
    ));
    let kids = children_of(&world, root);
    let path1 = world.get::<PfShape>(kids[0]).unwrap();
    let ShapeGeometry::Path(data) = &path1.geometry else {
        panic!("expected path geometry")
    };
    assert_eq!(data.figures.len(), 1);
    assert_eq!(data.figures[0].segments.len(), 3);
    assert!(data.figures[0].closed);

    let path2 = world.get::<PfShape>(kids[1]).unwrap();
    let ShapeGeometry::Path(data2) = &path2.geometry else {
        panic!("expected path geometry")
    };
    assert!(!data2.figures.is_empty());
}

#[test]
fn stroke_dashes_and_caps_from_xaml() {
    use bevy_pf::shapes::PfShape;
    use bevy_pf_xaml::value::{PenLineCap, PenLineJoin};
    let (world, root) = spawn(&format!(
        r#"<StackPanel {PRES}>
             <Line X1="0" Y1="0" X2="100" Y2="0" Stroke="Black" StrokeThickness="2"
                   StrokeDashArray="2 4" StrokeStartLineCap="Round"
                   StrokeLineJoin="Bevel" StrokeMiterLimit="4"/>
           </StackPanel>"#
    ));
    let line = world
        .get::<PfShape>(children_of(&world, root)[0])
        .unwrap();
    assert_eq!(line.stroke_dash_array, vec![2.0, 4.0]);
    assert_eq!(line.stroke_cap, PenLineCap::Round);
    assert_eq!(line.stroke_join, PenLineJoin::Bevel);
    assert_eq!(line.stroke_miter_limit, 4.0);
}

#[test]
fn tag_x_static_zindex_and_qualified_setters() {
    let (world, root) = spawn(&format!(
        r#"<Grid {PRES} {X}>
             <Grid.Resources>
               <Style TargetType="TextBlock">
                 <Setter Property="Control.FontSize" Value="19"/>
               </Style>
             </Grid.Resources>
             <TextBlock Tag="user-data" Text="a"
                        Visibility="{{x:Static Visibility.Collapsed}}"/>
             <Border Panel.ZIndex="5"/>
           </Grid>"#
    ));
    let kids = children_of(&world, root);
    assert_eq!(
        world.get::<bevy_pf::components::PfTag>(kids[0]).unwrap().0,
        "user-data"
    );
    // x:Static enum resolved through the normal converter.
    assert_eq!(world.get::<Node>(kids[0]).unwrap().display, Display::None);
    // Qualified setter applied as a plain property.
    assert_eq!(
        world.get::<bevy::text::TextFont>(kids[0]).unwrap().font_size,
        bevy::text::FontSize::Px(19.0)
    );
    assert_eq!(world.get::<bevy::ui::ZIndex>(kids[1]).unwrap().0, 5);
}

#[test]
fn gradient_stop_colors_from_resources() {
    let (world, root) = spawn(&format!(
        r#"<Border {PRES} {X}>
             <Border.Resources>
               <Color x:Key="Accent">#FF0078D7</Color>
             </Border.Resources>
             <Border.Background>
               <LinearGradientBrush>
                 <GradientStop Offset="0" Color="{{StaticResource Accent}}"/>
                 <GradientStop Offset="1" Color="White"/>
               </LinearGradientBrush>
             </Border.Background>
           </Border>"#
    ));
    assert!(world.get::<bevy::ui::BackgroundGradient>(root).is_some());
}

#[test]
fn numeric_attributes_accept_wpf_length_units() {
    // WPF LengthConverter units flow through the generic numeric path:
    // Height="30px", Width="1in", FontSize="14pt" (96dpi: 1pt = 4/3 px).
    let (world, root) = spawn(
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                       xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
             <Border x:Name="B" Height="30px" Width="1in"/>
             <TextBlock x:Name="T" FontSize="14pt" Text="pt-sized"/>
           </StackPanel>"#,
    );
    let names = world.get::<XamlNames>(root).unwrap();
    let b = names.get("B").unwrap();
    let node = world.get::<Node>(b).unwrap();
    assert_eq!(node.height, Val::Px(30.0));
    assert_eq!(node.width, Val::Px(96.0));
    let t = names.get("T").unwrap();
    let font = world.get::<bevy::text::TextFont>(t).unwrap();
    let bevy::text::FontSize::Px(px) = font.font_size else {
        panic!("expected px font size");
    };
    assert!((px - 14.0 * 96.0 / 72.0).abs() < 0.01, "14pt = {px}px");
}
