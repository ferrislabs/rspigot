mod common;
#[path = "common/screen.rs"]
mod screen;
#[path = "common/seeded.rs"]
mod seeded;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::plugins::{Answer, BlockOrder, Order, Question};
use rspigot_domain::{BlockPos, Vec3};
use seeded::{Seed, contents, world};

const DISPENSER: BlockPos = BlockPos::new(3, 64, 3);
const POWER: BlockPos = BlockPos::new(3, 64, 2);

fn fire_at(item: (&'static str, u8), mob: &str) -> Scene {
    let mut s = world(
        vec![
            Seed::block(DISPENSER, BlockKind::Dispenser)
                .with("facing", "east")
                .holding(item.0, item.1),
        ],
        Vec::new(),
    );
    let answer = s.ask(Question::Spawn {
        kind: mob.to_owned(),
        position: Vec3::new(4.5, 64.0, 3.5),
    });
    assert!(matches!(answer, Answer::Spawned(_)));
    s.act(Order::Block {
        pos: POWER,
        order: BlockOrder::Set("minecraft:redstone_block".to_owned()),
    });
    s.tick(5);
    s
}

fn left(s: &mut Scene) -> Vec<Option<(&'static str, u8)>> {
    let alice = s.players[0].clone();
    contents(s, &alice, DISPENSER, 1)
}

#[test]
fn a_dispenser_saddles_a_horse_in_front_of_it() {
    let mut s = fire_at(("minecraft:saddle", 1), "minecraft:horse");
    assert_eq!(left(&mut s), [None]);
    assert!(s.server.ground_items().is_empty());
}

#[test]
fn a_dispenser_puts_horse_armor_on_a_horse() {
    let mut s = fire_at(("minecraft:iron_horse_armor", 1), "minecraft:horse");
    assert_eq!(left(&mut s), [None]);
    assert!(s.server.ground_items().is_empty());
}

#[test]
fn a_dispenser_drops_a_saddle_when_the_mob_cannot_wear_one() {
    let mut s = fire_at(("minecraft:saddle", 1), "minecraft:cow");
    assert_eq!(left(&mut s), [None]);
    assert_eq!(s.server.ground_items().len(), 1);
}

#[test]
fn shears_take_the_pumpkin_off_a_snow_golem_in_front() {
    let mut s = fire_at(("minecraft:shears", 1), "minecraft:snow_golem");
    assert_eq!(left(&mut s), [Some(("minecraft:shears", 1))]);
    assert!(!s.server.ground_items().is_empty());
}
