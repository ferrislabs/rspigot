mod common;
#[path = "common/screen.rs"]
mod screen;
#[path = "common/seeded.rs"]
mod seeded;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::plugins::{EventKind, EventPayload, HolderRef};
use rspigot_domain::BlockPos;
use screen::{close_window, open_block, opened_view, use_block};
use seeded::{Seed, world};

const CHEST: BlockPos = BlockPos::new(3, 64, 3);
const TABLE: &str = "minecraft:gameplay/hero_of_the_village/farmer_gift";

fn looted_chest() -> Seed {
    Seed::block(CHEST, BlockKind::Chest)
        .carrying("LootTable", TABLE)
        .carrying("LootTableSeed", 7i64)
}

fn stocked(view: &screen::View) -> usize {
    (0..27).filter(|slot| view.at(*slot).is_some()).count()
}

#[test]
fn opening_a_loot_chest_fires_loot_generate_and_fills_the_chest() {
    let mut s = world(
        vec![looted_chest()],
        vec![observe(1, EventKind::LootGenerate)],
    );
    let alice = s.players[0].clone();
    let mut view = open_block(&mut s, &alice, CHEST);
    assert!(stocked(&view) > 0);
    close_window(&mut s, &alice, &mut view);
    let events = settled_for(&s.plugin, EventKind::LootGenerate);
    assert_eq!(events.len(), 1);
    let EventPayload::LootGenerate(generated) = &events[0].payload else {
        panic!("expected a loot generate");
    };
    assert_eq!(generated.loot_table, TABLE);
    assert!(!generated.plugin);
    assert_eq!(generated.inventory_holder, Some(HolderRef::Block(CHEST)));
    assert_eq!(
        generated.entity.as_ref().map(|e| e.kind.as_str()),
        Some("minecraft:player")
    );
    assert!(!generated.loot.is_empty());
    assert_eq!(generated.loot_context.location.position.x, 3.5);
}

#[test]
fn a_loot_chest_generates_once_then_keeps_its_items() {
    let mut s = world(
        vec![looted_chest()],
        vec![observe(1, EventKind::LootGenerate)],
    );
    let alice = s.players[0].clone();
    let mut first = open_block(&mut s, &alice, CHEST);
    let count = stocked(&first);
    close_window(&mut s, &alice, &mut first);
    let second = open_block(&mut s, &alice, CHEST);
    assert_eq!(stocked(&second), count);
    assert_eq!(settled_for(&s.plugin, EventKind::LootGenerate).len(), 1);
}

#[test]
fn cancelling_loot_generate_opens_an_empty_chest_and_spends_the_table() {
    let mut s = world(
        vec![looted_chest()],
        vec![deciding(1, EventKind::LootGenerate)],
    );
    let alice = s.players[0].clone();
    use_block(&mut s, &alice, CHEST);
    let asked = last_asked(&s.plugin, EventKind::LootGenerate);
    rule_on(&mut s.server, 1, asked.id, true, None);
    let view = opened_view(&alice).expect("the chest opens after the ruling");
    assert_eq!(stocked(&view), 0);
}

#[test]
fn a_chest_without_a_loot_table_fires_nothing() {
    let mut s = world(
        vec![Seed::block(CHEST, BlockKind::Chest)],
        vec![observe(1, EventKind::LootGenerate)],
    );
    let alice = s.players[0].clone();
    open_block(&mut s, &alice, CHEST);
    assert!(settled_for(&s.plugin, EventKind::LootGenerate).is_empty());
}
