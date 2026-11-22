mod common;

use common::*;
use rspigot_core::plugins::{Answer, BlockOrder, Order, Question};
use rspigot_core::world_keys::Crop;
use rspigot_domain::BlockPos;

const SPOT: BlockPos = BlockPos::new(2, 64, 2);

fn farm(percent: i32) -> Scene {
    let server = configured(|config| {
        config.rules.random_tick_speed = 4096;
        config.world_keys.growth.set(Crop::Cactus, percent);
    });
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s.act(Order::Block {
        pos: SPOT.offset(0, -1, 0),
        order: BlockOrder::Set("minecraft:sand".to_owned()),
    });
    s.act(Order::Block {
        pos: SPOT,
        order: BlockOrder::Set("minecraft:cactus".to_owned()),
    });
    s
}

fn is_cactus(s: &mut Scene, pos: BlockPos) -> bool {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state.kind == "minecraft:cactus",
        _ => false,
    }
}

#[test]
fn a_vanilla_modifier_grows_a_cactus_taller_quickly_under_heavy_random_ticks() {
    let mut s = farm(100);
    s.tick(80);
    assert!(is_cactus(&mut s, SPOT.offset(0, 1, 0)));
}

#[test]
fn a_one_percent_modifier_almost_stops_the_growth() {
    let mut s = farm(1);
    s.tick(80);
    assert!(!is_cactus(&mut s, SPOT.offset(0, 1, 0)));
}

#[test]
fn a_zero_modifier_is_read_as_one_hundred_like_spigot() {
    let mut s = farm(0);
    s.tick(80);
    assert!(is_cactus(&mut s, SPOT.offset(0, 1, 0)));
}
