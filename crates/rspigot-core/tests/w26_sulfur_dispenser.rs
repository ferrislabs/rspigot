mod common;
#[path = "common/screen.rs"]
mod screen;
#[path = "common/seeded.rs"]
mod seeded;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::plugins::{Answer, BlockOrder, Order, Question};
use rspigot_domain::BlockPos;
use seeded::{Seed, contents, world};

const DISPENSER: BlockPos = BlockPos::new(3, 64, 3);
const FRONT: BlockPos = BlockPos::new(4, 64, 3);
const POWER: BlockPos = BlockPos::new(3, 64, 2);

fn fire(item: &'static str, extra: Vec<Seed>) -> Scene {
    let mut seeds = vec![
        Seed::block(DISPENSER, BlockKind::Dispenser)
            .with("facing", "east")
            .holding(item, 1),
    ];
    seeds.extend(extra);
    let mut s = world(seeds, Vec::new());
    s.act(Order::Block {
        pos: POWER,
        order: BlockOrder::Set("minecraft:redstone_block".to_owned()),
    });
    s.tick(5);
    s
}

fn count_of(s: &mut Scene, kind: &str) -> usize {
    match s.ask(Question::Entities {
        near: None,
        kind: Some(kind.to_owned()),
    }) {
        Answer::Entities(entities) => entities.len(),
        other => panic!("expected entities, got {other:?}"),
    }
}

fn block_at(s: &mut Scene, pos: BlockPos) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state.kind,
        other => panic!("expected a block, got {other:?}"),
    }
}

fn left(s: &mut Scene) -> Vec<Option<(&'static str, u8)>> {
    let alice = s.players[0].clone();
    contents(s, &alice, DISPENSER, 1)
}

#[test]
fn a_sulfur_cube_bucket_releases_the_cube_without_pouring_any_water() {
    let mut s = fire("minecraft:sulfur_cube_bucket", Vec::new());
    assert_eq!(count_of(&mut s, "minecraft:sulfur_cube"), 1);
    assert_eq!(block_at(&mut s, FRONT), "minecraft:air");
    assert_eq!(left(&mut s), [Some(("minecraft:bucket", 1))]);
}

#[test]
fn a_sulfur_cube_bucket_in_front_of_a_wall_is_dropped() {
    let mut s = fire(
        "minecraft:sulfur_cube_bucket",
        vec![Seed::block(FRONT, BlockKind::Stone)],
    );
    assert_eq!(count_of(&mut s, "minecraft:sulfur_cube"), 0);
    assert_eq!(s.server.ground_items().len(), 1);
}
