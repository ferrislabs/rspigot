mod common;
#[path = "common/screen.rs"]
mod screen;

use common::*;
use rspigot_core::events::{ClickInput, Inbound, PlayerInput};
use rspigot_core::plugins::events::{PrepareAnvil, PrepareAnvilPatch};
use rspigot_core::plugins::{EventKind, EventPatch, EventPayload};
use rspigot_domain::BlockPos;
use screen::*;

const ANVIL: BlockPos = BlockPos::new(0, 64, 2);
const SWORD: &str = "minecraft:diamond_sword";
const HOTBAR_ZERO: i16 = 30;

fn rename(s: &mut Scene, player: &Player, name: &str) {
    s.server.handle(Inbound::Input {
        player: player.id,
        input: PlayerInput::RenameItem(name.to_owned()),
    });
    s.server.deliver();
}

fn place_sword(s: &mut Scene, alice: &Player, anvil: &mut View) {
    give(s, alice, 0, SWORD, 1);
    alice.link.take();
    refresh(alice, anvil);
    let take = predict(
        anvil,
        press(HOTBAR_ZERO, 0, ClickInput::Pickup),
        &[(HOTBAR_ZERO, None)],
        stack(SWORD, 1),
    );
    send_click(s, alice, anvil, take);
    let put = predict(
        anvil,
        press(0, 0, ClickInput::Pickup),
        &[(0, stack(SWORD, 1))],
        None,
    );
    send_click(s, alice, anvil, put);
}

fn anvil_scene(handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> (Scene, Player, View) {
    let mut s = scene_with(&["Alice"], handlers);
    let alice = s.players[0].clone();
    set_block(&mut s, ANVIL, "minecraft:anvil");
    alice.link.take();
    let anvil = open_block(&mut s, &alice, ANVIL);
    (s, alice, anvil)
}

fn level_of(s: &Scene, player: &Player) -> i32 {
    s.server.player_experience(player.id).expect("a player").0
}

#[test]
fn renaming_prices_the_result_and_taking_it_needs_and_spends_the_levels() {
    let (mut s, alice, mut anvil) = anvil_scene(Vec::new());
    place_sword(&mut s, &alice, &mut anvil);
    assert_eq!(anvil.at(2), None);
    rename(&mut s, &alice, "Blade");
    refresh(&alice, &mut anvil);
    assert_eq!(anvil.at(2), Some((SWORD, 1)));
    assert_eq!(anvil.data.last(), Some(&(0, 1)));
    let broke = press(2, 0, ClickInput::Pickup);
    send_click(&mut s, &alice, &mut anvil, broke);
    assert_eq!(anvil.held(), None);
    assert_eq!(anvil.at(0), Some((SWORD, 1)));
    s.server.give_experience(alice.id, 100);
    let before = level_of(&s, &alice);
    let take = predict(
        &mut anvil,
        press(2, 0, ClickInput::Pickup),
        &[(2, None)],
        stack(SWORD, 1),
    );
    send_click(&mut s, &alice, &mut anvil, take);
    assert_eq!(anvil.held(), Some((SWORD, 1)));
    assert_eq!(anvil.at(0), None);
    assert_eq!(anvil.at(2), None);
    assert_eq!(level_of(&s, &alice), before - 1);
}

#[test]
fn prepare_anvil_asks_with_the_computed_result_and_a_patch_replaces_it() {
    let (mut s, alice, mut anvil) = anvil_scene(vec![
        deciding(1, EventKind::PrepareAnvil),
        observe(2, EventKind::PrepareAnvil),
    ]);
    place_sword(&mut s, &alice, &mut anvil);
    let first = last_asked(&s.plugin, EventKind::PrepareAnvil);
    rule_on(&mut s.server, 1, first.id, false, None);
    rename(&mut s, &alice, "Blade");
    let asked = last_asked(&s.plugin, EventKind::PrepareAnvil);
    assert_ne!(asked.id, first.id);
    let EventPayload::PrepareAnvil(PrepareAnvil {
        result, inventory, ..
    }) = &asked.payload
    else {
        panic!("an anvil prepare");
    };
    assert_eq!(result.as_ref().map(|item| item.key.as_str()), Some(SWORD));
    assert_eq!(inventory.kind, "ANVIL");
    refresh(&alice, &mut anvil);
    assert_eq!(anvil.at(2), None);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::PrepareAnvil(PrepareAnvilPatch {
            result: Some(Some(spec("minecraft:golden_sword", 1))),
        })),
    );
    refresh(&alice, &mut anvil);
    assert_eq!(anvil.at(2), Some(("minecraft:golden_sword", 1)));
}

#[test]
fn prepare_anvil_fires_when_the_inputs_change_and_a_patch_can_empty_the_result() {
    let (mut s, alice, mut anvil) = anvil_scene(vec![deciding(1, EventKind::PrepareAnvil)]);
    place_sword(&mut s, &alice, &mut anvil);
    let asked = last_asked(&s.plugin, EventKind::PrepareAnvil);
    let EventPayload::PrepareAnvil(PrepareAnvil { result, .. }) = &asked.payload else {
        panic!("an anvil prepare");
    };
    assert_eq!(result, &None);
    rule_on(&mut s.server, 1, asked.id, false, None);
    rename(&mut s, &alice, "Blade");
    let renamed = last_asked(&s.plugin, EventKind::PrepareAnvil);
    rule_on(
        &mut s.server,
        1,
        renamed.id,
        false,
        Some(EventPatch::PrepareAnvil(PrepareAnvilPatch {
            result: Some(None),
        })),
    );
    refresh(&alice, &mut anvil);
    assert_eq!(anvil.at(2), None);
}
