mod common;
#[path = "common/screen.rs"]
mod screen;

use common::*;
use rspigot_core::events::{ClickInput, Inbound, PlayerInput};
use rspigot_core::plugins::events::{EnchantItem, EnchantItemPatch, PrepareItemEnchant};
use rspigot_core::plugins::{EventKind, EventPatch, EventPayload};
use rspigot_domain::BlockPos;
use screen::*;

const TABLE: BlockPos = BlockPos::new(0, 64, 2);
const SWORD: &str = "minecraft:diamond_sword";
const LAPIS: &str = "minecraft:lapis_lazuli";
const HOTBAR_ZERO: i16 = 29;

fn shelves(s: &mut Scene) {
    for z in 0..=4 {
        set_block(s, BlockPos::new(-2, 64, z), "minecraft:bookshelf");
        set_block(s, BlockPos::new(2, 64, z), "minecraft:bookshelf");
    }
    for x in -1..=1 {
        set_block(s, BlockPos::new(x, 64, 0), "minecraft:bookshelf");
        set_block(s, BlockPos::new(x, 64, 4), "minecraft:bookshelf");
    }
    for z in 0..=4 {
        set_block(s, BlockPos::new(-2, 65, z), "minecraft:bookshelf");
    }
}

fn table_scene(handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> (Scene, Player, View) {
    let mut s = scene_with(&["Alice"], handlers);
    let alice = s.players[0].clone();
    set_block(&mut s, TABLE, "minecraft:enchanting_table");
    shelves(&mut s);
    s.server.give_experience(alice.id, 3000);
    alice.link.take();
    let table = open_block(&mut s, &alice, TABLE);
    (s, alice, table)
}

fn put(
    s: &mut Scene,
    alice: &Player,
    table: &mut View,
    hotbar: i32,
    key: &str,
    count: u8,
    to: i16,
) {
    give(s, alice, hotbar, key, i32::from(count));
    alice.link.take();
    refresh(alice, table);
    let from = HOTBAR_ZERO + hotbar as i16;
    let take = predict(
        table,
        press(from, 0, ClickInput::Pickup),
        &[(from, None)],
        stack(key, count),
    );
    send_click(s, alice, table, take);
    let place = predict(
        table,
        press(to, 0, ClickInput::Pickup),
        &[(to, stack(key, count))],
        None,
    );
    send_click(s, alice, table, place);
}

fn datum(table: &View, id: i16) -> i16 {
    table
        .data
        .iter()
        .rev()
        .find(|(known, _)| *known == id)
        .map_or(0, |(_, value)| *value)
}

fn contents(alice: &Player, table: &mut View) -> Vec<rspigot_domain::item::Slot> {
    refresh(alice, table);
    table.slots.clone()
}

fn button(s: &mut Scene, alice: &Player, table: &View, button: i32) {
    s.server.handle(Inbound::Input {
        player: alice.id,
        input: PlayerInput::ContainerButtonClick {
            window: table.window,
            button,
        },
    });
    s.server.deliver();
}

fn level_of(s: &Scene, player: &Player) -> i32 {
    s.server.player_experience(player.id).expect("a player").0
}

#[test]
fn an_item_in_the_table_shows_three_offers_with_a_clue_each() {
    let (mut s, alice, mut table) = table_scene(Vec::new());
    put(&mut s, &alice, &mut table, 0, SWORD, 1, 0);
    refresh(&alice, &mut table);
    assert!(datum(&table, 0) >= 1);
    assert!(datum(&table, 2) >= 30);
    for slot in 0..3 {
        assert!(datum(&table, 4 + slot) >= 0);
        assert!(datum(&table, 7 + slot) >= 1);
    }
}

#[test]
fn a_button_spends_lapis_and_levels_and_enchants_the_item() {
    let (mut s, alice, mut table) = table_scene(Vec::new());
    put(&mut s, &alice, &mut table, 0, SWORD, 1, 0);
    put(&mut s, &alice, &mut table, 1, LAPIS, 3, 1);
    let before = level_of(&s, &alice);
    button(&mut s, &alice, &table, 2);
    refresh(&alice, &mut table);
    assert_eq!(table.at(1), None);
    assert_eq!(level_of(&s, &alice), before - 3);
    let enchanted = contents(&alice, &mut table);
    let sword = enchanted[0].as_ref().expect("sword");
    assert!(sword.components.contains_key("minecraft:enchantments"));
}

#[test]
fn a_button_without_lapis_does_nothing() {
    let (mut s, alice, mut table) = table_scene(Vec::new());
    put(&mut s, &alice, &mut table, 0, SWORD, 1, 0);
    let before = level_of(&s, &alice);
    button(&mut s, &alice, &table, 0);
    assert_eq!(level_of(&s, &alice), before);
    let items = contents(&alice, &mut table);
    assert!(
        !items[0]
            .as_ref()
            .expect("sword")
            .components
            .contains_key("minecraft:enchantments")
    );
}

#[test]
fn prepare_item_enchant_asks_with_the_offers_and_a_cancel_clears_them() {
    let (mut s, alice, mut table) = table_scene(vec![
        deciding(1, EventKind::PrepareItemEnchant),
        observe(2, EventKind::PrepareItemEnchant),
    ]);
    put(&mut s, &alice, &mut table, 0, SWORD, 1, 0);
    let asked = last_asked(&s.plugin, EventKind::PrepareItemEnchant);
    let EventPayload::PrepareItemEnchant(PrepareItemEnchant {
        enchant_block,
        enchantment_bonus,
        offers,
        item,
        inventory,
        ..
    }) = &asked.payload
    else {
        panic!("an enchant prepare");
    };
    assert_eq!(*enchant_block, TABLE);
    assert_eq!(*enchantment_bonus, 21);
    assert_eq!(item.key, SWORD);
    assert_eq!(offers.len(), 3);
    assert_eq!(inventory.kind, "ENCHANTING");
    refresh(&alice, &mut table);
    assert_eq!(datum(&table, 0), 0);
    rule_on(&mut s.server, 1, asked.id, true, None);
    refresh(&alice, &mut table);
    assert_eq!(datum(&table, 2), 0);
}

#[test]
fn prepare_item_enchant_lets_the_offers_through_when_it_is_not_cancelled() {
    let (mut s, alice, mut table) = table_scene(vec![deciding(1, EventKind::PrepareItemEnchant)]);
    put(&mut s, &alice, &mut table, 0, SWORD, 1, 0);
    let asked = last_asked(&s.plugin, EventKind::PrepareItemEnchant);
    rule_on(&mut s.server, 1, asked.id, false, None);
    refresh(&alice, &mut table);
    assert!(datum(&table, 2) >= 30);
}

#[test]
fn enchant_item_carries_the_choice_and_a_cancel_keeps_everything() {
    let (mut s, alice, mut table) = table_scene(vec![deciding(1, EventKind::EnchantItem)]);
    put(&mut s, &alice, &mut table, 0, SWORD, 1, 0);
    put(&mut s, &alice, &mut table, 1, LAPIS, 3, 1);
    let before = level_of(&s, &alice);
    button(&mut s, &alice, &table, 1);
    let asked = last_asked(&s.plugin, EventKind::EnchantItem);
    let EventPayload::EnchantItem(EnchantItem {
        enchant_block,
        enchants_to_add,
        exp_level_cost,
        item,
        which_button,
        level_hint,
        ..
    }) = &asked.payload
    else {
        panic!("an enchant item");
    };
    assert_eq!(*enchant_block, TABLE);
    assert_eq!(*which_button, 1);
    assert_eq!(item.key, SWORD);
    assert!(*exp_level_cost >= 2);
    assert!(!enchants_to_add.is_empty());
    assert!(*level_hint >= 1);
    rule_on(&mut s.server, 1, asked.id, true, None);
    refresh(&alice, &mut table);
    assert_eq!(level_of(&s, &alice), before);
    let items = contents(&alice, &mut table);
    assert_eq!(items[1].as_ref().map(|lapis| lapis.count.get()), Some(3));
    assert!(
        !items[0]
            .as_ref()
            .expect("sword")
            .components
            .contains_key("minecraft:enchantments")
    );
}

#[test]
fn enchant_item_allowed_applies_and_a_patched_cost_above_the_levels_refuses() {
    let (mut s, alice, mut table) = table_scene(vec![deciding(1, EventKind::EnchantItem)]);
    put(&mut s, &alice, &mut table, 0, SWORD, 1, 0);
    put(&mut s, &alice, &mut table, 1, LAPIS, 3, 1);
    button(&mut s, &alice, &table, 0);
    let asked = last_asked(&s.plugin, EventKind::EnchantItem);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::EnchantItem(EnchantItemPatch {
            exp_level_cost: Some(5000),
        })),
    );
    let items = contents(&alice, &mut table);
    assert_eq!(items[1].as_ref().map(|lapis| lapis.count.get()), Some(3));
    button(&mut s, &alice, &table, 0);
    let again = last_asked(&s.plugin, EventKind::EnchantItem);
    rule_on(&mut s.server, 1, again.id, false, None);
    let items = contents(&alice, &mut table);
    assert_eq!(items[1].as_ref().map(|lapis| lapis.count.get()), Some(2));
    assert!(
        items[0]
            .as_ref()
            .expect("sword")
            .components
            .contains_key("minecraft:enchantments")
    );
}
