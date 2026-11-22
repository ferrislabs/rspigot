mod common;

use common::*;
use rspigot_core::events::Inbound;
use rspigot_domain::BlockPos;
use rspigot_entities::EntityKind as Kind;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn herd(limit: i32, cows: usize) -> Scene {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    console(&mut s, &format!("gamerule max_entity_cramming {limit}"));
    for _ in 0..cows {
        console(&mut s, "summon minecraft:cow 8.5 64 8.5 {NoAI:1b}");
    }
    s.tick(120);
    s
}

fn unharmed(s: &Scene) -> usize {
    s.server
        .entities_of(Kind::Cow)
        .iter()
        .filter(|cow| cow.health == Some(10.0))
        .count()
}

#[test]
fn a_crowd_over_the_limit_takes_cramming_damage() {
    let s = herd(2, 5);
    assert!(unharmed(&s) < 5);
}

#[test]
fn a_crowd_under_the_limit_is_left_alone() {
    let s = herd(24, 5);
    assert_eq!(unharmed(&s), 5);
}

#[test]
fn a_limit_of_zero_disables_cramming() {
    let s = herd(0, 5);
    assert_eq!(unharmed(&s), 5);
}
