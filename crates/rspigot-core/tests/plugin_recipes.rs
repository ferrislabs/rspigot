mod common;
#[path = "common/screen.rs"]
mod screen;

use common::*;
use rspigot_core::events::{BookDisplay, BookEntry, ClientEvent, PlayerInput};
use rspigot_core::plugins::events::{
    PlayerRecipeBookClick, PlayerRecipeBookClickPatch, PlayerRecipeDiscover,
};
use rspigot_core::plugins::{EventKind, EventPatch, EventPayload};
use rspigot_domain::BlockPos;
use screen::*;

const STICK: &str = "minecraft:stick";
const DOOR: &str = "minecraft:oak_door";
const TABLE: BlockPos = BlockPos::new(0, 64, 2);

fn added(player: &Player) -> Vec<BookEntry> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::RecipeBookAdd { entries, .. } => Some(entries),
            _ => None,
        })
        .flatten()
        .collect()
}

fn display_id_of(s: &mut Scene, player: &Player, recipe: &str) -> i32 {
    player.link.take();
    s.server.award_recipes(player.id, &[recipe]);
    let id = added(player)
        .first()
        .map(|entry| entry.display_id)
        .expect("a book entry");
    s.server.take_recipes(player.id, &[recipe]);
    player.link.take();
    id
}

fn place(s: &mut Scene, player: &Player, window: i32, display_id: i32, max: bool) {
    send_input(
        &mut s.server,
        player,
        PlayerInput::PlaceRecipe {
            window,
            display_id,
            max,
        },
    );
}

#[test]
fn awarding_a_recipe_sends_its_display_and_remembers_it() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    assert_eq!(
        s.server.award_recipes(alice.id, &[STICK, "minecraft:nope"]),
        1
    );
    let entries = added(&alice);
    assert_eq!(entries.len(), 1);
    assert!(entries[0].notify && entries[0].highlight);
    assert!(matches!(
        &entries[0].display,
        BookDisplay::Shaped {
            width: 1,
            height: 2,
            ..
        }
    ));
    assert_eq!(s.server.known_recipes(alice.id), vec![STICK]);
    assert_eq!(s.server.award_recipes(alice.id, &[STICK]), 0);
    assert!(added(&alice).is_empty());
}

#[test]
fn taking_a_recipe_sends_its_display_id_and_forgets_it() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.server.award_recipes(alice.id, &[STICK]);
    let wanted = added(&alice)[0].display_id;
    assert_eq!(s.server.take_recipes(alice.id, &[STICK]), 1);
    let removed: Vec<i32> = alice
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::RecipeBookRemove(ids) => Some(ids),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(removed, vec![wanted]);
    assert!(s.server.known_recipes(alice.id).is_empty());
}

#[test]
fn player_recipe_discover_carries_the_recipe_id() {
    let mut s = scene_with(
        &["Alice"],
        vec![observe(1, EventKind::PlayerRecipeDiscover)],
    );
    let alice = s.players[0].clone();
    s.server.award_recipes(alice.id, &[STICK]);
    let events = fired(&s.plugin);
    let EventPayload::PlayerRecipeDiscover(PlayerRecipeDiscover { recipe, .. }) =
        &events[0].payload
    else {
        panic!("a discovery");
    };
    assert_eq!(recipe, STICK);
    assert_eq!(s.server.known_recipes(alice.id), vec![STICK]);
}

#[test]
fn a_cancelled_discovery_keeps_the_recipe_out_of_the_book() {
    let mut s = scene_with(
        &["Alice"],
        vec![deciding(1, EventKind::PlayerRecipeDiscover)],
    );
    let alice = s.players[0].clone();
    s.server.award_recipes(alice.id, &[STICK]);
    let asked = last_asked(&s.plugin, EventKind::PlayerRecipeDiscover);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(s.server.known_recipes(alice.id).is_empty());
    assert!(added(&alice).is_empty());
}

#[test]
fn an_allowed_discovery_reaches_the_book_once_the_plugin_answers() {
    let mut s = scene_with(
        &["Alice"],
        vec![deciding(1, EventKind::PlayerRecipeDiscover)],
    );
    let alice = s.players[0].clone();
    s.server.award_recipes(alice.id, &[STICK]);
    assert!(s.server.known_recipes(alice.id).is_empty());
    let asked = last_asked(&s.plugin, EventKind::PlayerRecipeDiscover);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert_eq!(s.server.known_recipes(alice.id), vec![STICK]);
    assert_eq!(added(&alice).len(), 1);
}

#[test]
fn known_recipes_survive_a_restart_and_are_sent_again_at_login() {
    let (mut s, _saves) = saving_scene(&["Alice"], Vec::new());
    let alice = s.players[0].clone();
    s.server.award_recipes(alice.id, &[STICK]);
    s.server.handle(leave_of(&alice));
    s.server.deliver();
    let back = arrive(&mut s.server, "Alice");
    let again: Vec<ClientEvent> = back.link.take();
    let replaced = again.iter().any(|event| {
        matches!(event, ClientEvent::RecipeBookAdd { entries, replace: true }
            if entries.len() == 1 && !entries[0].notify && entries[0].highlight)
    });
    assert!(
        replaced,
        "{:?}",
        again
            .iter()
            .filter(|e| matches!(
                e,
                ClientEvent::RecipeBookAdd { .. } | ClientEvent::RecipeBookSettings(_)
            ))
            .collect::<Vec<_>>()
    );
    assert_eq!(s.server.known_recipes(back.id), vec![STICK]);
}

#[test]
fn placing_a_recipe_fills_the_player_grid_and_shows_the_result() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let display = display_id_of(&mut s, &alice, STICK);
    s.server.award_recipes(alice.id, &[STICK]);
    give(&mut s, &alice, 0, "minecraft:oak_planks", 4);
    alice.link.take();
    place(&mut s, &alice, 0, display, false);
    let mut view = View::inventory();
    view.apply(&alice.link.take());
    assert_eq!(view.at(1), Some(("minecraft:oak_planks", 1)));
    assert_eq!(view.at(3), Some(("minecraft:oak_planks", 1)));
    assert_eq!(view.at(2), None);
    assert_eq!(view.at(0), Some((STICK, 4)));
    assert_eq!(view.at(36), Some(("minecraft:oak_planks", 2)));
}

#[test]
fn the_maximum_places_as_many_crafts_as_the_items_allow() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let display = display_id_of(&mut s, &alice, STICK);
    s.server.award_recipes(alice.id, &[STICK]);
    give(&mut s, &alice, 0, "minecraft:oak_planks", 6);
    alice.link.take();
    place(&mut s, &alice, 0, display, true);
    let mut view = View::inventory();
    view.apply(&alice.link.take());
    assert_eq!(view.at(1), Some(("minecraft:oak_planks", 3)));
    assert_eq!(view.at(3), Some(("minecraft:oak_planks", 3)));
    assert_eq!(view.at(36), None);
}

#[test]
fn a_recipe_the_player_does_not_know_places_nothing() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let display = display_id_of(&mut s, &alice, STICK);
    give(&mut s, &alice, 0, "minecraft:oak_planks", 4);
    alice.link.take();
    place(&mut s, &alice, 0, display, false);
    let mut view = View::inventory();
    view.apply(&alice.link.take());
    assert_eq!(view.at(1), None);
}

#[test]
fn missing_items_send_the_ghost_recipe_and_leave_the_grid_empty() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let display = display_id_of(&mut s, &alice, STICK);
    s.server.award_recipes(alice.id, &[STICK]);
    alice.link.take();
    place(&mut s, &alice, 0, display, false);
    let events = alice.link.take();
    assert!(events.iter().any(|event| matches!(
        event,
        ClientEvent::GhostRecipe {
            window: 0,
            display: BookDisplay::Shaped { .. }
        }
    )));
}

#[test]
fn a_crafting_table_receives_the_recipe_in_its_three_by_three_grid() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let display = display_id_of(&mut s, &alice, DOOR);
    s.server.award_recipes(alice.id, &[DOOR]);
    set_block(&mut s, TABLE, "minecraft:crafting_table");
    give(&mut s, &alice, 0, "minecraft:oak_planks", 12);
    alice.link.take();
    let mut table = open_block(&mut s, &alice, TABLE);
    place(&mut s, &alice, table.window, display, false);
    refresh(&alice, &mut table);
    for slot in [1, 2, 4, 5, 7, 8] {
        assert_eq!(table.at(slot), Some(("minecraft:oak_planks", 1)), "{slot}");
    }
    assert_eq!(table.at(0), Some((DOOR, 3)));
}

#[test]
fn player_recipe_book_click_carries_both_recipes_and_a_patch_replaces_the_recipe() {
    let mut s = scene_with(
        &["Alice"],
        vec![deciding(1, EventKind::PlayerRecipeBookClick)],
    );
    let alice = s.players[0].clone();
    let stick = display_id_of(&mut s, &alice, STICK);
    s.server
        .award_recipes(alice.id, &[STICK, "minecraft:oak_planks"]);
    give(&mut s, &alice, 0, "minecraft:oak_log", 2);
    alice.link.take();
    place(&mut s, &alice, 0, stick, true);
    let asked = last_asked(&s.plugin, EventKind::PlayerRecipeBookClick);
    let EventPayload::PlayerRecipeBookClick(PlayerRecipeBookClick {
        original_recipe,
        recipe,
        shift_click,
        ..
    }) = &asked.payload
    else {
        panic!("a click");
    };
    assert_eq!((original_recipe.as_str(), recipe.as_str()), (STICK, STICK));
    assert!(*shift_click);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::PlayerRecipeBookClick(
            PlayerRecipeBookClickPatch {
                recipe: Some("minecraft:oak_planks".to_owned()),
                shift_click: Some(false),
            },
        )),
    );
    let mut view = View::inventory();
    view.apply(&alice.link.take());
    assert_eq!(view.at(1), Some(("minecraft:oak_log", 1)));
    assert_eq!(view.at(0), Some(("minecraft:oak_planks", 4)));
}
