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
    pub display_member: Option<String>,
    pub kind: ItemsHostKind,
    pub seen_version: u64,
}

/// Rebuild generated items when the source list's model changes.
///
/// v1 rebuilds all items on any model version bump; granular diffing arrives
/// with `ObservableList` change events (roadmap 2.2).
pub(crate) fn sync_items_sources(world: &mut World) {
    let mut query = world.query::<(Entity, &PfItemsSource)>();
    let hosts: Vec<(Entity, PfItemsSource)> = query
        .iter(world)
        .map(|(e, s)| (e, s.clone()))
        .collect();

    for (entity, source) in hosts {
        let Some(ctx) = find_context(world, entity) else {
            continue;
        };
        let version = ctx.version();
        if version == source.seen_version {
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
            ItemsHostKind::ComboBox => {
                match world.get::<crate::components::PfComboBox>(entity) {
                    Some(combo) => combo.popup,
                    None => continue,
                }
            }
            ItemsHostKind::DataGrid => {
                match world.get::<crate::components::PfDataGrid>(entity) {
                    Some(grid) => grid.rows_host,
                    None => continue,
                }
            }
            _ => entity,
        };
        world.entity_mut(container).despawn_children();

        let mut items = Vec::with_capacity(len);
        for index in 0..len {
            let item_ctx = ctx.at(format!("{}[{index}]", source.path));

            // DataGrid rows are fully generated here (template columns).
            if source.kind == ItemsHostKind::DataGrid {
                let Some(grid) = world.get::<crate::components::PfDataGrid>(entity).cloned()
                else {
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
                        if let Err(e) =
                            crate::instantiate::instantiate_template(world, wrapper, template)
                        {
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
            let wrapper = match source.kind {
                ItemsHostKind::DataGrid => unreachable!("handled above"),
                ItemsHostKind::ItemsControl => world.spawn(Node::default()).id(),
                ItemsHostKind::ListBox => {
                    let wrapper = world
                        .spawn((
                            Node {
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
                    if let Err(e) =
                        crate::instantiate::instantiate_template(world, wrapper, template)
                    {
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
            items.push(wrapper);
        }
        world.entity_mut(container).add_children(&items);
    }
}

/// Spawn a plain text node with the engine's default text style (used for
/// runtime-generated items, where no lexical font inheritance exists).
pub(crate) fn spawn_runtime_text(world: &mut World, text: &str) -> Entity {
    world
        .spawn((
            Node::default(),
            bevy::ui::widget::Text::new(text),
            bevy::text::TextFont {
                font_size: bevy::text::FontSize::Px(12.0),
                ..Default::default()
            },
            bevy::text::TextColor(Color::BLACK),
        ))
        .id()
}
