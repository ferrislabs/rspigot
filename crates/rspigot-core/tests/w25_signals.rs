mod common;
#[path = "common/screen.rs"]
mod screen;
#[path = "common/seeded.rs"]
mod seeded;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::events::PlayerInput;
use rspigot_core::plugins::{Answer, BlockOrder, Order, Question};
use rspigot_domain::{BlockPos, Vec3};
use screen::{close_window, open_block};
use seeded::{Seed, world};

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

fn put(s: &mut Scene, pos: BlockPos, kind: &str) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::Set(kind.to_owned()),
    });
}

fn walk(s: &mut Scene, from: Vec3, steps: i32, dx: f64) -> Vec3 {
    let mut at = from;
    for _ in 0..steps {
        at = Vec3::new(at.x + dx, at.y, at.z);
        send_input(
            &mut s.server,
            &s.players[0],
            PlayerInput::Move {
                position: Some(at),
                rotation: None,
                on_ground: true,
            },
        );
        s.tick(1);
    }
    at
}

fn ever_lit(s: &mut Scene, lamp: BlockPos, ticks: u32) -> bool {
    let mut lit = false;
    for _ in 0..ticks {
        s.tick(1);
        lit |= prop(s, lamp, "lit") == "true";
    }
    lit
}

const SENSOR: BlockPos = BlockPos::new(6, 64, 0);
const SENSOR_LAMP: BlockPos = BlockPos::new(6, 64, -1);
const MATCHING: BlockPos = BlockPos::new(6, 64, 4);
const MATCHING_LAMP: BlockPos = BlockPos::new(7, 64, 4);
const FILTERED: BlockPos = BlockPos::new(6, 64, -4);
const FILTERED_LAMP: BlockPos = BlockPos::new(7, 64, -4);

fn sculk_rig() -> Scene {
    let mut s = world(
        vec![
            Seed::block(SENSOR, BlockKind::SculkSensor),
            Seed::block(SENSOR_LAMP, BlockKind::RedstoneLamp),
            Seed::block(MATCHING, BlockKind::CalibratedSculkSensor).with("facing", "north"),
            Seed::block(MATCHING.offset(0, 0, 1), BlockKind::Target).with("power", "1"),
            Seed::block(MATCHING_LAMP, BlockKind::RedstoneLamp),
            Seed::block(FILTERED, BlockKind::CalibratedSculkSensor).with("facing", "south"),
            Seed::block(FILTERED.offset(0, 0, -1), BlockKind::Target).with("power", "5"),
            Seed::block(FILTERED_LAMP, BlockKind::RedstoneLamp),
        ],
        Vec::new(),
    );
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s.tick(2);
    s
}

#[test]
fn an_active_sculk_sensor_powers_the_lamp_beside_it() {
    let mut s = sculk_rig();
    walk(&mut s, Vec3::new(0.5, 64.0, 0.5), 12, 0.3);
    assert!(ever_lit(&mut s, SENSOR_LAMP, 40));
    assert_eq!(prop(&mut s, SENSOR_LAMP, "lit"), "false");
}

#[test]
fn a_calibrated_sensor_whose_back_signal_matches_the_frequency_reacts_and_powers_a_lamp() {
    let mut s = sculk_rig();
    walk(&mut s, Vec3::new(0.5, 64.0, 0.5), 12, 0.3);
    assert!(ever_lit(&mut s, MATCHING_LAMP, 40));
}

#[test]
fn a_calibrated_sensor_whose_back_signal_differs_from_the_frequency_ignores_it() {
    let mut s = sculk_rig();
    walk(&mut s, Vec3::new(0.5, 64.0, 0.5), 12, 0.3);
    assert!(!ever_lit(&mut s, FILTERED_LAMP, 40));
    assert_eq!(prop(&mut s, FILTERED, "sculk_sensor_phase"), "inactive");
}

const CHEST: BlockPos = BlockPos::new(3, 64, 3);
const CHEST_LAMP: BlockPos = BlockPos::new(4, 64, 3);

#[test]
fn a_trapped_chest_powers_a_lamp_for_as_long_as_it_is_open() {
    let mut s = world(
        vec![
            Seed::block(CHEST, BlockKind::TrappedChest),
            Seed::block(CHEST_LAMP, BlockKind::RedstoneLamp),
        ],
        Vec::new(),
    );
    assert_eq!(prop(&mut s, CHEST_LAMP, "lit"), "false");
    let alice = s.players[0].clone();
    let mut view = open_block(&mut s, &alice, CHEST);
    s.tick(6);
    assert_eq!(prop(&mut s, CHEST_LAMP, "lit"), "true");
    close_window(&mut s, &alice, &mut view);
    s.tick(6);
    assert_eq!(prop(&mut s, CHEST_LAMP, "lit"), "false");
}

#[test]
fn a_plain_chest_does_not_power_anything() {
    let mut s = world(
        vec![
            Seed::block(CHEST, BlockKind::Chest),
            Seed::block(CHEST_LAMP, BlockKind::RedstoneLamp),
        ],
        Vec::new(),
    );
    let alice = s.players[0].clone();
    let _view = open_block(&mut s, &alice, CHEST);
    s.tick(6);
    assert_eq!(prop(&mut s, CHEST_LAMP, "lit"), "false");
}

const BULB: BlockPos = BlockPos::new(4, 64, 4);
const BULB_POWER: BlockPos = BlockPos::new(4, 64, 5);

fn bulb_rig(kind: BlockKind) -> Scene {
    world(vec![Seed::block(BULB, kind)], Vec::new())
}

#[test]
fn a_copper_bulb_toggles_on_each_rising_edge_and_remembers_powered() {
    let mut s = bulb_rig(BlockKind::CopperBulb);
    assert_eq!(prop(&mut s, BULB, "lit"), "false");
    put(&mut s, BULB_POWER, "minecraft:redstone_block");
    s.tick(1);
    assert_eq!(prop(&mut s, BULB, "lit"), "true");
    assert_eq!(prop(&mut s, BULB, "powered"), "true");
    put(&mut s, BULB_POWER, "minecraft:air");
    s.tick(1);
    assert_eq!(prop(&mut s, BULB, "lit"), "true");
    assert_eq!(prop(&mut s, BULB, "powered"), "false");
    put(&mut s, BULB_POWER, "minecraft:redstone_block");
    s.tick(1);
    assert_eq!(prop(&mut s, BULB, "lit"), "false");
    assert_eq!(prop(&mut s, BULB, "powered"), "true");
}

#[test]
fn an_oxidised_waxed_copper_bulb_toggles_too() {
    let mut s = bulb_rig(BlockKind::WaxedOxidizedCopperBulb);
    put(&mut s, BULB_POWER, "minecraft:redstone_block");
    s.tick(1);
    assert_eq!(prop(&mut s, BULB, "lit"), "true");
}

#[test]
fn a_comparator_reads_fifteen_from_a_lit_copper_bulb() {
    let mut s = world(
        vec![
            Seed::block(BULB, BlockKind::CopperBulb).with("lit", "true"),
            Seed::block(BULB.offset(0, 0, 1), BlockKind::Comparator).with("facing", "north"),
        ],
        Vec::new(),
    );
    put(&mut s, BULB.offset(1, 0, 1), "minecraft:stone");
    s.tick(4);
    let saved = s
        .server
        .block_entity_nbt(BULB.offset(0, 0, 1))
        .expect("the comparator keeps its output");
    assert_eq!(saved.get_i32("OutputSignal"), Some(15));
}

const LEFT: BlockPos = BlockPos::new(3, 64, 3);
const RIGHT: BlockPos = BlockPos::new(4, 64, 3);

fn full_half(pos: BlockPos, kind: BlockKind, side: &'static str) -> Seed {
    let mut seed = Seed::block(pos, kind)
        .with("facing", "north")
        .with("type", side);
    for _ in 0..27 {
        seed = seed.holding("minecraft:cobblestone", 64);
    }
    seed
}

fn empty_half(pos: BlockPos, kind: BlockKind, side: &'static str) -> Seed {
    Seed::block(pos, kind)
        .with("facing", "north")
        .with("type", side)
}

#[test]
fn a_comparator_on_a_double_chest_averages_over_all_fifty_four_slots() {
    let mut s = world(
        vec![
            full_half(LEFT, BlockKind::Chest, "left"),
            empty_half(RIGHT, BlockKind::Chest, "right"),
            Seed::block(LEFT.offset(0, 0, 1), BlockKind::Comparator).with("facing", "north"),
        ],
        Vec::new(),
    );
    put(&mut s, LEFT.offset(-1, 0, 1), "minecraft:stone");
    s.tick(4);
    let saved = s
        .server
        .block_entity_nbt(LEFT.offset(0, 0, 1))
        .expect("the comparator keeps its output");
    assert_eq!(saved.get_i32("OutputSignal"), Some(8));
}

#[test]
fn a_comparator_on_a_single_chest_still_reads_its_twenty_seven_slots() {
    let mut s = world(
        vec![
            {
                let mut seed = Seed::block(LEFT, BlockKind::Chest).with("facing", "north");
                for _ in 0..27 {
                    seed = seed.holding("minecraft:cobblestone", 64);
                }
                seed
            },
            Seed::block(LEFT.offset(0, 0, 1), BlockKind::Comparator).with("facing", "north"),
        ],
        Vec::new(),
    );
    put(&mut s, LEFT.offset(-1, 0, 1), "minecraft:stone");
    s.tick(4);
    let saved = s
        .server
        .block_entity_nbt(LEFT.offset(0, 0, 1))
        .expect("the comparator keeps its output");
    assert_eq!(saved.get_i32("OutputSignal"), Some(15));
}
