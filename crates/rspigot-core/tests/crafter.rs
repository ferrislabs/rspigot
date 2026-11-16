mod common;
#[path = "common/screen.rs"]
mod screen;
#[path = "common/seeded.rs"]
mod seeded;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::plugins::{Answer, Question};
use rspigot_domain::BlockPos;
use rspigot_nbt::Tag;
use screen::set_block;
use seeded::{Seed, world};

const CRAFTER: BlockPos = BlockPos::new(3, 64, 3);
const COMPARATOR: BlockPos = BlockPos::new(4, 64, 3);

fn prop(s: &mut Scene, pos: BlockPos, name: &str) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state
            .properties
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .unwrap_or_default(),
        other => panic!("expected a block, got {other:?}"),
    }
}

fn read_by_comparator(crafter: Seed) -> bool {
    let mut s = world(
        vec![
            crafter,
            Seed::block(COMPARATOR, BlockKind::Comparator).with("facing", "west"),
        ],
        Vec::new(),
    );
    set_block(&mut s, BlockPos::new(4, 65, 3), "minecraft:stone");
    s.tick(4);
    prop(&mut s, COMPARATOR, "powered") == "true"
}

#[test]
fn an_empty_crafter_gives_no_comparator_signal() {
    assert!(!read_by_comparator(Seed::block(
        CRAFTER,
        BlockKind::Crafter
    )));
}

#[test]
fn a_crafter_with_an_item_gives_a_comparator_signal() {
    assert!(read_by_comparator(
        Seed::block(CRAFTER, BlockKind::Crafter).holding("minecraft:stone", 1)
    ));
}

#[test]
fn a_disabled_slot_counts_in_the_comparator_signal_like_an_item() {
    assert!(read_by_comparator(
        Seed::block(CRAFTER, BlockKind::Crafter).carrying("disabled_slots", Tag::IntArray(vec![4]))
    ));
}

#[test]
fn losing_power_clears_the_crafting_flag_with_the_triggered_one() {
    let mut s = world(
        vec![
            Seed::block(CRAFTER, BlockKind::Crafter)
                .with("triggered", "true")
                .with("crafting", "true"),
        ],
        Vec::new(),
    );
    set_block(&mut s, BlockPos::new(3, 65, 3), "minecraft:stone");
    s.tick(2);
    assert_eq!(prop(&mut s, CRAFTER, "triggered"), "false");
    assert_eq!(prop(&mut s, CRAFTER, "crafting"), "false");
}
