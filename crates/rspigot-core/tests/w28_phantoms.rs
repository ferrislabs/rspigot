mod common;

use common::*;
use rspigot_core::events::{Direction, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::{BlockOrder, Order};
use rspigot_domain::{BlockPos, Vec3};

const FLOOR: BlockPos = BlockPos::new(2, 63, 0);
const FOOT: BlockPos = BlockPos::new(2, 64, 0);
const HEAD: BlockPos = BlockPos::new(2, 64, 1);

fn set(s: &mut Scene, pos: BlockPos, part: &str) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::SetWith {
            kind: "minecraft:red_bed".to_owned(),
            properties: vec![
                ("part".to_owned(), part.to_owned()),
                ("facing".to_owned(), "south".to_owned()),
            ],
        },
    });
}

fn night_scene() -> Scene {
    let server = configured_level(|_| {}, |level| level.day_time = 18_000);
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, FLOOR);
    set(&mut s, FOOT, "foot");
    set(&mut s, HEAD, "head");
    s
}

fn lie_down(s: &mut Scene) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: FOOT,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 0.5, 0.5),
            inside_block: false,
            sequence: 3,
        },
    );
    s.server.deliver();
}

#[test]
fn going_to_bed_resets_the_time_since_rest() {
    let mut s = night_scene();
    let alice = s.players[0].id;
    s.server.set_ticks_since_rest(alice, 100_000);
    lie_down(&mut s);
    assert_eq!(s.server.ticks_since_rest(alice), 0);
}

#[test]
fn a_sleeping_player_does_not_accumulate_time_since_rest() {
    let mut s = night_scene();
    let alice = s.players[0].id;
    lie_down(&mut s);
    s.tick(40);
    assert_eq!(s.server.ticks_since_rest(alice), 0);
}

#[test]
fn dying_resets_the_time_since_rest_and_the_dead_do_not_count() {
    let mut s = night_scene();
    let alice = s.players[0].id;
    s.server.set_ticks_since_rest(alice, 100_000);
    s.server.handle(Inbound::Console("kill Alice".to_owned()));
    s.server.deliver();
    assert_eq!(s.server.ticks_since_rest(alice), 0);
    s.tick(20);
    assert_eq!(s.server.ticks_since_rest(alice), 0);
}

#[test]
fn an_awake_player_counts_one_tick_per_tick() {
    let mut s = night_scene();
    let alice = s.players[0].id;
    s.server.set_ticks_since_rest(alice, 10);
    s.tick(20);
    assert_eq!(s.server.ticks_since_rest(alice), 30);
}

#[test]
fn chunks_around_a_player_gain_one_inhabited_tick_per_tick() {
    let mut s = night_scene();
    s.tick(40);
    let here = rspigot_domain::ChunkPos::new(0, 0);
    let before = s.server.inhabited_time(here).expect("loaded");
    s.tick(40);
    assert_eq!(s.server.inhabited_time(here), Some(before + 40));
}
