//! `ItemsSource` + `DataTemplate`: data-driven item generation for
//! `ListBox`, `ItemsControl`, and `ComboBox`.
//!
//! Templates are stored as *unexpanded* XAML subtrees and instantiated per
//! item through the normal engine, with each item's `DataContext` scoped to
//! `<items-path>[i]` on the shared view-model — so `{Binding name}` inside a
//! template reads `items[i].name`, and TwoWay bindings write back into the
//! list element.

use std::sync::Arc;

use bevy::prelude::*;

use crate::binding::{DataContext, find_context};
use crate::components::{PfListBox, PfListBoxItem};

/// Which control hosts the generated items (decides wrapper chrome).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemsHostKind {
    ListBox,
    ItemsControl,
    ComboBox,
    DataGrid,
}

/// An `ItemsSource="{Binding path}"` on an items control.
#[derive(Component, Debug, Clone)]
pub struct PfItemsSource {
    pub path: String,
    pub template: Option<Arc<bevy_pf_xaml::XamlNode>>,
    /// Lexical resource scope where the template was declared, so
    /// `{StaticResource}` inside a `DataTemplate` resolves page resources.
    pub scopes: Option<Arc<crate::resources::ResourceScopes>>,
    pub display_member: Option<String>,
    /// `ItemContainerStyle`: applied to each generated item container.
    pub container_style: Option<Arc<crate::resources::PfStyle>>,
    pub kind: ItemsHostKind,
    pub seen_version: u64,
}

/// `Content="{Binding vm}"` on a ContentControl: content regenerated from
/// a template (explicit `ContentTemplate`, else DataType-implicit from the
/// bound value's type ident), falling back to display text for scalars.
#[derive(Component, Debug, Clone)]
pub struct PfContentSource {
    pub path: String,
    /// Explicit `ContentTemplate=` (wins over DataType selection).
    pub template: Option<Arc<bevy_pf_xaml::XamlNode>>,
    pub scopes: Option<Arc<crate::resources::ResourceScopes>>,
    pub seen_version: u64,
}

/// Rebuild ContentControl content when the bound model changes.
pub(crate) fn sync_content_sources(world: &mut World) {
    let mut query = world.query::<(Entity, &PfContentSource)>();
    let hosts: Vec<(Entity, PfContentSource)> =
        query.iter(world).map(|(e, s)| (e, s.clone())).collect();

    for (entity, source) in hosts {
        let Some(ctx) = find_context(world, entity) else {
            continue;
        };
        let version = ctx.version();
        if version == source.seen_version {
            continue;
        }
        if let Some(mut s) = world.get_mut::<PfContentSource>(entity) {
            s.seen_version = version;
        }

        // Explicit ContentTemplate, else implicit by the value's type ident.
        let template = source.template.clone().or_else(|| {
            let ident = ctx.type_ident(&source.path)?;
            match source
                .scopes
                .as_deref()?
                .lookup(&crate::resources::ResourceKey::Type(ident))
            {
                Some(crate::resources::PfValue::Template(t)) => Some(t.clone()),
                _ => None,
            }
        });

        world.entity_mut(entity).despawn_children();
        match template {
            Some(template) => {
                let wrapper = world
                    .spawn((Node::default(), DataContext(ctx.at(&source.path))))
                    .id();
                world.entity_mut(entity).add_children(&[wrapper]);
                if let Err(e) = crate::instantiate::instantiate_template(
                    world,
                    wrapper,
                    &template,
                    source.scopes.as_deref(),
                ) {
                    warn!("bevy_pf: content template failed: {e}");
                }
            }
            None => {
                let text = ctx
                    .read_path(&source.path)
                    .map(|v| v.to_display())
                    .unwrap_or_default();
                let label = spawn_runtime_text(world, &text);
                world.entity_mut(entity).add_children(&[label]);
            }
        }
    }
}

/// The entity that actually receives generated item containers: the
/// `ItemsPanel` panel when one was declared, else `host` itself.
pub fn items_container(world: &World, host: Entity) -> Entity {
    world
        .get::<crate::components::PfItemsPanel>(host)
        .map(|p| p.panel)
        .unwrap_or(host)
}

/// Rebuild generated items when the source list's model changes.
///
/// v1 rebuilds all items on any model version bump; granular diffing arrives
/// with `ObservableList` change events (roadmap 2.2).
pub(crate) fn sync_items_sources(world: &mut World) {
    let mut query = world.query::<(Entity, &PfItemsSource)>();
    let hosts: Vec<(Entity, PfItemsSource)> =
        query.iter(world).map(|(e, s)| (e, s.clone())).collect();

    for (entity, source) in hosts {
        let Some(ctx) = find_context(world, entity) else {
            continue;
        };
        let version = ctx.version();
        if version == source.seen_version {
            continue;
        }
        // A version bump alone is not a reason to regenerate: a busy model
        // (per-frame scalar setters on the same context) would churn item
        // entities every frame — visible as flicker, wasted layout, and
        // clicks that never land because the pressed entity is gone by
        // release. Only a change overlapping the items path rebuilds.
        if !ctx.changed_since(source.seen_version, std::iter::once(source.path.as_str())) {
            if let Some(mut s) = world.get_mut::<PfItemsSource>(entity) {
                s.seen_version = version;
            }
            continue;
        }
        if let Some(mut s) = world.get_mut::<PfItemsSource>(entity) {
            s.seen_version = version;
        }

        let Some(len) = ctx.list_len(&source.path) else {
            warn!(
                "bevy_pf: ItemsSource path `{}` is not a list on the data context",
                source.path
            );
            continue;
        };

        // The container whose children are the generated items.
        let container = match source.kind {
            ItemsHostKind::ComboBox => match world.get::<crate::components::PfComboBox>(entity) {
                Some(combo) => combo.popup,
                None => continue,
            },
            ItemsHostKind::DataGrid => match world.get::<crate::components::PfDataGrid>(entity) {
                Some(grid) => grid.rows_host,
                None => continue,
            },
            // ListBox/ItemsControl: an ItemsPanel redirects generation.
            _ => items_container(world, entity),
        };
        // The rebuild despawns every row, so a ListBox's `selected` entity is
        // about to dangle. Remember WHERE the selection was; a SelectedItem
        // binding will re-locate the item by value on the next apply, and
        // this keeps the position meanwhile so the row it prefers is the one
        // the user actually had.
        let previous_selection = world
            .get::<crate::components::PfListBox>(entity)
            .and_then(|l| l.selected)
            .and_then(|sel| {
                world
                    .get::<Children>(container)
                    .and_then(|c| c.iter().position(|child| child == sel))
            });
        // KEEP THE FOCUS RING ON THE ROW IT WAS ON.
        //
        // Every row is despawned and respawned below — there is no child
        // reuse — so a focused control inside one becomes a dangling
        // entity, and `focus_nav` then treats the next move as "nothing
        // focused" and lands top-left. For a keyboard that is a nuisance;
        // for a gamepad, whose ONLY pointer is the focus ring, it means
        // acting on a row throws the player back to the top of the
        // dialog and they cannot act on the same row twice.
        //
        // And this fires far more often than on activation: any change to
        // the bound collection rebuilds, so a list whose rows carry live
        // state (affordability colours, counts) destroys the ring while
        // the player is merely sitting still.
        //
        // Position, not identity: the same row index and the same tab
        // stop within that row. The rows are regenerated from one
        // template, so "the third tab stop of row 5" is stable across the
        // rebuild even though every entity id changed.
        let previous_focus = focus_position(world, container);
        world.entity_mut(container).despawn_children();

        let mut items = Vec::with_capacity(len);
        for index in 0..len {
            let item_ctx = ctx.at(format!("{}[{index}]", source.path));

            // DataGrid rows are fully generated here (template columns).
            if source.kind == ItemsHostKind::DataGrid {
                let Some(grid) = world.get::<crate::components::PfDataGrid>(entity).cloned() else {
                    continue;
                };
                let template: Vec<bevy::ui::RepeatedGridTrack> = grid
                    .columns
                    .iter()
                    .map(|c| match c.width {
                        bevy_pf_xaml::value::GridLength::Auto => bevy::ui::GridTrack::fr(1.0),
                        other => crate::convert::grid_track(other),
                    })
                    .collect();
                let row = world
                    .spawn((
                        Node {
                            display: Display::Grid,
                            grid_template_columns: template,
                            grid_template_rows: vec![bevy::ui::GridTrack::auto()],
                            ..Default::default()
                        },
                        PfListBoxItem,
                        Interaction::default(),
                        BackgroundColor(Color::NONE),
                    ))
                    .id();
                for (col_index, column) in grid.columns.iter().enumerate() {
                    let cell = if let Some(template) = &column.template {
                        // GridViewColumn.CellTemplate: expand with the row's
                        // scoped DataContext.
                        let wrapper = world
                            .spawn((Node::default(), DataContext(item_ctx.clone())))
                            .id();
                        if let Err(e) = crate::instantiate::instantiate_template(
                            world,
                            wrapper,
                            template,
                            source.scopes.as_deref(),
                        ) {
                            warn!("bevy_pf: cell template failed: {e}");
                        }
                        wrapper
                    } else {
                        let text = item_ctx
                            .read_path(&column.path)
                            .map(|v| v.to_display())
                            .unwrap_or_default();
                        spawn_runtime_text(world, &text)
                    };
                    if let Some(mut n) = world.get_mut::<Node>(cell) {
                        n.grid_column =
                            bevy::ui::GridPlacement::start_span(col_index as i16 + 1, 1);
                        n.grid_row = bevy::ui::GridPlacement::start_span(1, 1);
                        n.padding = UiRect::axes(Val::Px(8.0), Val::Px(3.0));
                    }
                    world.entity_mut(row).add_children(&[cell]);
                }
                let rows_host = container;
                world.entity_mut(row).observe(
                    move |click: On<Pointer<Click>>, mut lists: Query<&mut PfListBox>| {
                        let this = click.entity;
                        if let Ok(mut state) = lists.get_mut(rows_host) {
                            state.selected = Some(this);
                        }
                    },
                );
                items.push(row);
                continue;
            }

            // Wrapper per host kind.
            //
            // Item containers stack their content in a COLUMN, which is what
            // makes a template root fill the container's width: bevy_ui's
            // default cross-axis alignment stretches, and WPF's
            // `ContentPresenter` fills its container horizontally the same way.
            // A row-direction container would put the template on the main axis,
            // where it shrink-wraps and every `*` Grid track inside it collapses.
            let wrapper = match source.kind {
                ItemsHostKind::DataGrid => unreachable!("handled above"),
                ItemsHostKind::ItemsControl => world
                    .spawn(Node {
                        flex_direction: FlexDirection::Column,
                        ..Default::default()
                    })
                    .id(),
                ItemsHostKind::ListBox => {
                    let wrapper = world
                        .spawn((
                            Node {
                                flex_direction: FlexDirection::Column,
                                padding: UiRect::axes(Val::Px(4.0), Val::Px(2.0)),
                                ..Default::default()
                            },
                            crate::components::PfElementKind("ListBoxItem".to_string()),
                            PfListBoxItem,
                            Interaction::default(),
                            BackgroundColor(Color::NONE),
                        ))
                        .id();
                    let list = entity;
                    world.entity_mut(wrapper).observe(
                        move |click: On<Pointer<Click>>, mut lists: Query<&mut PfListBox>| {
                            let this = click.entity;
                            if let Ok(mut state) = lists.get_mut(list) {
                                state.selected = Some(this);
                            }
                        },
                    );
                    wrapper
                }
                ItemsHostKind::ComboBox => {
                    let wrapper = world
                        .spawn((
                            Node {
                                padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                                ..Default::default()
                            },
                            crate::components::PfComboItem {
                                combo: entity,
                                index,
                            },
                            Interaction::default(),
                            BackgroundColor(Color::NONE),
                        ))
                        .id();
                    let combo = entity;
                    world.entity_mut(wrapper).observe(
                        move |_click: On<Pointer<Click>>, mut commands: Commands| {
                            commands.queue(move |world: &mut World| {
                                crate::instantiate::select_combo_index(world, combo, index);
                            });
                        },
                    );
                    wrapper
                }
            };

            // Content: template expansion or display text.
            match &source.template {
                Some(template) => {
                    world
                        .entity_mut(wrapper)
                        .insert(DataContext(item_ctx.clone()));
                    if let Err(e) = crate::instantiate::instantiate_template(
                        world,
                        wrapper,
                        template,
                        source.scopes.as_deref(),
                    ) {
                        warn!("bevy_pf: item template failed: {e}");
                    }
                }
                None => {
                    let text = item_ctx
                        .read_path(source.display_member.as_deref().unwrap_or(""))
                        .map(|v| v.to_display())
                        .unwrap_or_default();
                    let label = spawn_runtime_text(world, &text);
                    world.entity_mut(wrapper).add_children(&[label]);
                }
            }
            // ItemContainerStyle lands on the container, after content so
            // font-flavored setters see the generated children.
            if let Some(style) = &source.container_style {
                crate::instantiate::apply_style_runtime(
                    world,
                    wrapper,
                    "ListBoxItem",
                    style,
                    source.scopes.as_deref(),
                );
            }
            items.push(wrapper);
        }

        // WPF gives each generated item container the full cross-axis extent of
        // a vertically-stacking items panel (`HorizontalContentAlignment`
        // defaults to `Stretch` on the container). Without this the wrapper
        // shrink-wraps its template, so every `*` Grid track *inside* an item
        // collapses to content width and a trailing flush-right button ends up
        // jammed against the label.
        //
        // Only for column panels: in a `WrapPanel` (a wrapping flex row) the
        // cross axis is height, and WPF measures a wrapped item to its natural
        // height, not the line height.
        if column_stacking(world, container) {
            for &item in &items {
                if let Some(mut node) = world.get_mut::<Node>(item) {
                    node.align_self = bevy::ui::AlignSelf::Stretch;
                }
            }
        }

        world.entity_mut(container).add_children(&items);
        restore_focus_position(world, &items, previous_focus);

        // Re-point the selection at the row in the same position, or clear it
        // when the list shrank past it. Without this the ListBox holds a
        // despawned entity and reports "something is selected" forever.
        if let Some(mut list) = world.get_mut::<crate::components::PfListBox>(entity)
            && list.selected.is_some()
        {
            list.selected = previous_selection.and_then(|i| items.get(i).copied());
        }
        if let Some(mut combo) = world.get_mut::<crate::components::PfComboBox>(entity)
            && combo.selected.is_some_and(|i| i >= items.len())
        {
            combo.selected = None;
        }
    }
}

/// Where the focus ring sat inside an items container, as (row, tab stop
/// within that row), or `None` if it was not in there at all.
///
/// Recorded before a rebuild despawns the rows and restored after, so a
/// pad that activates a row keeps its place instead of being thrown back
/// to the top of the panel.
fn focus_position(world: &mut World, container: Entity) -> Option<(usize, usize)> {
    let focused = world.get_resource::<bevy::input_focus::InputFocus>()?.get()?;
    let rows: Vec<Entity> = world.get::<Children>(container)?.iter().collect();
    let row = rows.iter().position(|&row| contains(world, row, focused))?;
    let within = tab_stops(world, rows[row]).iter().position(|&e| e == focused).unwrap_or(0);
    Some((row, within))
}

/// Put the ring back on the same row and the same control within it,
/// clamped when the list shrank under it.
fn restore_focus_position(world: &mut World, items: &[Entity], at: Option<(usize, usize)>) {
    let Some((row, within)) = at else { return };
    if items.is_empty() {
        return;
    }
    let row = items[row.min(items.len() - 1)];
    let stops = tab_stops(world, row);
    // A row with no tab stop of its own still gets the ring, so focus
    // stays inside the list rather than vanishing to the panel top.
    let target = stops.get(within.min(stops.len().saturating_sub(1))).copied().unwrap_or(row);
    if let Some(mut focus) = world.get_resource_mut::<bevy::input_focus::InputFocus>() {
        // Navigated, not Programmatic: to the player this IS the ring
        // they were already moving, surviving a rebuild they never asked
        // for and cannot see.
        focus.set(target, bevy::input_focus::FocusCause::Navigated);
    }
}

/// `entity` is `root` or sits beneath it.
fn contains(world: &World, root: Entity, entity: Entity) -> bool {
    let mut cursor = entity;
    loop {
        if cursor == root {
            return true;
        }
        match world.get::<ChildOf>(cursor) {
            Some(parent) => cursor = parent.parent(),
            None => return false,
        }
    }
}

/// Every focusable descendant of `root`, in tree order — the same order
/// `focus_nav` walks, so an index here means the same control there.
fn tab_stops(world: &World, root: Entity) -> Vec<Entity> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        if world.get::<bevy::input_focus::tab_navigation::TabIndex>(e).is_some() {
            out.push(e);
        }
        if let Some(children) = world.get::<Children>(e) {
            // Pushed in reverse so the stack pops them front-to-back.
            for child in children.iter().collect::<Vec<_>>().into_iter().rev() {
                stack.push(child);
            }
        }
    }
    out
}

/// Whether an items panel stacks its children top-to-bottom, and so hands them
/// its full width.
fn column_stacking(world: &World, container: Entity) -> bool {
    world.get::<Node>(container).is_some_and(|node| {
        matches!(
            node.flex_direction,
            bevy::ui::FlexDirection::Column | bevy::ui::FlexDirection::ColumnReverse
        ) && node.flex_wrap == bevy::ui::FlexWrap::NoWrap
    })
}

/// Spawn a plain text node with the engine's default text style (used for
/// runtime-generated items, where no lexical font inheritance exists).
pub(crate) fn spawn_runtime_text(world: &mut World, text: &str) -> Entity {
    // Generated rows sit under the overlay root, so there is no control to
    // inherit Foreground from; the theme supplies the colour instead. This
    // was hardcoded to BLACK, which is right for the default light theme and
    // invisible on any dark one.
    let color = world
        .get_resource::<crate::components::PfControlTheme>()
        .map(|theme| theme.item_text)
        .unwrap_or(Color::BLACK);
    world
        .spawn((
            Node::default(),
            bevy::ui::widget::Text::new(text),
            bevy::text::TextFont {
                font_size: bevy::text::FontSize::Px(12.0),
                font: crate::fonts::default_font(),
                ..Default::default()
            },
            bevy::text::TextColor(color),
        ))
        .id()
}
