//! Object identity for `SelectedItem`, at the level where objects exist.
//!
//! The bound item never becomes a `BoundValue` — it never leaves the view
//! model. Both the items path and the selected path resolve against the same
//! model under one read guard, so "which row is this?" is answered by
//! comparing the objects themselves.
//!
//! These tests exist because the tempting shortcuts all pass a naive smoke
//! test and fail here: display-string matching cannot tell two equal-looking
//! rows apart, and index identity cannot survive a sort.

use bevy::prelude::*;
use bevy_pf::binding::{Bindable, SelectionMatch};

#[derive(Reflect, Clone, PartialEq, Debug)]
struct Player {
    name: String,
    score: u32,
}

#[derive(Reflect, Clone, PartialEq, Debug)]
struct Enemy {
    name: String,
    score: u32,
}

#[derive(Reflect)]
struct Roster {
    players: Vec<Player>,
    selected: Option<Player>,
    /// A same-shaped field of a DIFFERENT type, for the type-gate test.
    intruder: Option<Enemy>,
}

fn player(name: &str, score: u32) -> Player {
    Player { name: name.into(), score }
}

fn roster() -> Bindable {
    Bindable::new(Roster {
        players: vec![player("ana", 10), player("bo", 20), player("cy", 30)],
        selected: None,
        intruder: None,
    })
}

#[test]
fn a_selected_object_is_located_by_value() {
    let vm = roster();
    assert_eq!(vm.selection_match("players", "selected", None), SelectionMatch::Null);

    vm.update(|r: &mut Roster| r.selected = Some(player("bo", 20)));
    assert_eq!(
        vm.selection_match("players", "selected", None),
        SelectionMatch::Index(1),
        "the item is found by comparing objects, not by index or text"
    );
}

#[test]
fn an_item_not_in_the_list_is_not_found() {
    let vm = roster();
    vm.update(|r: &mut Roster| r.selected = Some(player("zed", 99)));
    assert_eq!(
        vm.selection_match("players", "selected", None),
        SelectionMatch::NotFound
    );
}

#[test]
fn null_is_a_selection_not_a_failure() {
    // "Nothing selected" and "the value could not be located" must not be
    // the same answer: one clears, the other is a diagnosis.
    let vm = roster();
    assert_eq!(vm.selection_match("players", "selected", None), SelectionMatch::Null);
    vm.update(|r: &mut Roster| r.selected = Some(player("ana", 10)));
    assert_eq!(vm.selection_match("players", "selected", None), SelectionMatch::Index(0));
    vm.update(|r: &mut Roster| r.selected = None);
    assert_eq!(vm.selection_match("players", "selected", None), SelectionMatch::Null);
}

#[test]
fn an_empty_collection_is_pending_not_missing() {
    // A selection set BEFORE the collection loads must survive. Answering
    // NotFound here would clear it and the app would silently lose state.
    let vm = Bindable::new(Roster {
        players: vec![],
        selected: Some(player("ana", 10)),
        intruder: None,
    });
    assert_eq!(
        vm.selection_match("players", "selected", None),
        SelectionMatch::Pending
    );
}

#[test]
fn duplicates_keep_the_row_the_user_actually_picked() {
    // THE test that display-string matching and a naive scan both fail.
    // Two equal rows: clicking the second must keep the second selected,
    // not snap to the first.
    let vm = Bindable::new(Roster {
        players: vec![player("ana", 10), player("ana", 10), player("bo", 20)],
        selected: Some(player("ana", 10)),
        intruder: None,
    });
    assert_eq!(
        vm.selection_match("players", "selected", None),
        SelectionMatch::Index(0),
        "with no hint, first match wins"
    );
    assert_eq!(
        vm.selection_match("players", "selected", Some(1)),
        SelectionMatch::Index(1),
        "the row the UI already has selected is preferred over an equal earlier row"
    );
    // A hint that does NOT match is ignored rather than trusted.
    assert_eq!(
        vm.selection_match("players", "selected", Some(2)),
        SelectionMatch::Index(0),
        "a stale hint must not select the wrong object"
    );
}

#[test]
fn identity_survives_a_reorder_where_an_index_would_not() {
    // Index identity is the other tempting shortcut. After a sort the
    // object is at a different index; matching by value follows it.
    let vm = roster();
    vm.update(|r: &mut Roster| r.selected = Some(player("ana", 10)));
    assert_eq!(vm.selection_match("players", "selected", None), SelectionMatch::Index(0));

    vm.update(|r: &mut Roster| r.players.reverse());
    assert_eq!(
        vm.selection_match("players", "selected", None),
        SelectionMatch::Index(2),
        "the same object moved; a stored index would now name a different row"
    );
}

#[test]
fn a_same_shaped_value_of_another_type_is_not_the_same_item() {
    // bevy's struct comparison matches field names and values only, so
    // without a type gate an Enemy{name,score} equals a Player{name,score}.
    let vm = Bindable::new(Roster {
        players: vec![player("ana", 10), player("bo", 20)],
        selected: None,
        intruder: Some(Enemy { name: "ana".into(), score: 10 }),
    });
    assert_eq!(
        vm.selection_match("players", "intruder", None),
        SelectionMatch::NotFound,
        "identical fields, different type — must NOT match"
    );
}

// ---------------------------------------------------------------------
// The write direction.
// ---------------------------------------------------------------------

#[test]
fn selecting_writes_the_object_back() {
    let vm = roster();
    assert!(vm.set_from("selected", "players[2]"), "write succeeded");
    assert_eq!(
        vm.read(|r: &Roster| r.selected.clone()),
        Some(Some(player("cy", 30))),
        "the OBJECT is written back, not a display string or an index"
    );
    // ...and it round-trips: what was written is now locatable.
    assert_eq!(vm.selection_match("players", "selected", None), SelectionMatch::Index(2));
}

#[test]
fn clearing_writes_none() {
    let vm = roster();
    vm.update(|r: &mut Roster| r.selected = Some(player("bo", 20)));
    assert!(vm.set_null("selected"));
    assert_eq!(vm.read(|r: &Roster| r.selected.clone()), Some(None));
    assert_eq!(vm.selection_match("players", "selected", None), SelectionMatch::Null);
}

#[test]
fn a_write_bumps_the_version_so_dependents_refresh() {
    let vm = roster();
    let before = vm.version();
    vm.set_from("selected", "players[1]");
    assert_ne!(vm.version(), before, "bindings reading `selected` must be told");
}

#[test]
fn writing_a_non_option_destination_still_works() {
    #[derive(Reflect)]
    struct Plain {
        players: Vec<Player>,
        current: Player,
    }
    let vm = Bindable::new(Plain {
        players: vec![player("ana", 10), player("bo", 20)],
        current: player("ana", 10),
    });
    assert!(vm.set_from("current", "players[1]"));
    assert_eq!(vm.read(|p: &Plain| p.current.clone()), Some(player("bo", 20)));
    assert_eq!(vm.selection_match("players", "current", None), SelectionMatch::Index(1));
}

// ---------------------------------------------------------------------
// End to end: the primitive actually driving a ListBox.
// ---------------------------------------------------------------------

use bevy::asset::AssetPlugin;
use bevy_pf::prelude::*;
use bevy_pf::{XamlEnv, instantiate_document_env};

const LIST_PAGE: &str = r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
  <ListBox x:Name="L" ItemsSource="{Binding players}"
           DisplayMemberPath="name"
           SelectedItem="{Binding selected}"/>
</StackPanel>"#;

fn list_app(vm: Bindable) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()));
    app.add_plugins(PfUiPlugin);
    let doc = bevy_pf_xaml::parse(LIST_PAGE).expect("parses");
    let world = app.world_mut();
    let root = world.spawn(DataContext(vm)).id();
    let result = instantiate_document_env(world, root, &doc, &XamlEnv::default()).expect("builds");
    assert_eq!(result.warnings, Vec::<String>::new(), "clean instantiation");
    let list = app.world().get::<XamlNames>(root).unwrap().get("L").unwrap();
    (app, list)
}

/// The index of the selected row, recovered from the entity the ListBox holds.
fn selected_row(app: &App, list: Entity) -> Option<usize> {
    let selected = app
        .world()
        .get::<bevy_pf::components::PfListBox>(list)
        .and_then(|l| l.selected)?;
    let container = bevy_pf::items::items_container(app.world(), list);
    app.world()
        .get::<Children>(container)
        .and_then(|c| c.iter().position(|child| child == selected))
}

#[test]
fn a_bound_object_selects_its_row() {
    let vm = roster();
    vm.update(|r: &mut Roster| r.selected = Some(player("cy", 30)));
    let (mut app, list) = list_app(vm.clone());
    app.update();
    app.update();
    assert_eq!(
        selected_row(&app, list),
        Some(2),
        "the row holding the bound object is selected"
    );
}

#[test]
fn changing_the_model_moves_the_selection() {
    let vm = roster();
    let (mut app, list) = list_app(vm.clone());
    app.update();
    app.update();
    assert_eq!(selected_row(&app, list), None, "nothing selected initially");

    vm.update(|r: &mut Roster| r.selected = Some(player("ana", 10)));
    app.update();
    assert_eq!(selected_row(&app, list), Some(0));

    vm.update(|r: &mut Roster| r.selected = Some(player("bo", 20)));
    app.update();
    assert_eq!(selected_row(&app, list), Some(1));

    vm.update(|r: &mut Roster| r.selected = None);
    app.update();
    assert_eq!(selected_row(&app, list), None, "None clears the selection");
}

#[test]
fn an_item_outside_the_collection_clears_the_selection() {
    let vm = roster();
    vm.update(|r: &mut Roster| r.selected = Some(player("bo", 20)));
    let (mut app, list) = list_app(vm.clone());
    app.update();
    app.update();
    assert_eq!(selected_row(&app, list), Some(1));

    vm.update(|r: &mut Roster| r.selected = Some(player("ghost", 0)));
    app.update();
    assert_eq!(
        selected_row(&app, list),
        None,
        "a value that is not in the list clears, as WPF does"
    );
}

#[test]
fn the_selection_follows_its_object_when_the_collection_is_rebuilt() {
    // The rebuild despawns every row. A selection stored as an entity would
    // dangle and a selection stored as an index would point at the wrong
    // object; re-locating by value is what makes this come out right.
    let vm = roster();
    vm.update(|r: &mut Roster| r.selected = Some(player("ana", 10)));
    let (mut app, list) = list_app(vm.clone());
    app.update();
    app.update();
    assert_eq!(selected_row(&app, list), Some(0));

    vm.update(|r: &mut Roster| r.players.reverse());
    app.update();
    app.update();
    assert_eq!(
        selected_row(&app, list),
        Some(2),
        "same object, new position — the selection moved with it"
    );
}

#[test]
fn clicking_a_row_writes_the_object_back() {
    let vm = roster();
    let (mut app, list) = list_app(vm.clone());
    app.update();
    app.update();

    // Simulate the control selecting its second row.
    let container = bevy_pf::items::items_container(app.world(), list);
    let row = app.world().get::<Children>(container).unwrap().iter().nth(1).unwrap();
    app.world_mut()
        .get_mut::<bevy_pf::components::PfListBox>(list)
        .unwrap()
        .selected = Some(row);
    app.update();

    assert_eq!(
        vm.read(|r: &Roster| r.selected.clone()),
        Some(Some(player("bo", 20))),
        "the OBJECT reached the view model, not an index"
    );
}
