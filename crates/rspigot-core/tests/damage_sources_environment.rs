mod common;

use common::*;
use rspigot_core::events::{Inbound, PlayerInput};
use rspigot_core::plugins::{Answer, EntityView, Question};
use rspigot_domain::{EntityId, Vec3};

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn health(s: &Scene) -> f32 {
    s.server.player_health(s.players[0].id).unwrap()
}

fn settled() -> Scene {
    let mut s = scene(&["Alice"]);
    s.tick(60);
    s
}

fn spawn(s: &mut Scene, kind: &str, position: Vec3) -> EntityView {
    let Answer::Spawned(view) = s.ask(Question::Spawn {
        kind: kind.into(),
        position,
    }) else {
        panic!("expected the spawned {kind}");
    };
    view
}

fn mob_health(s: &mut Scene, id: EntityId) -> Option<f32> {
    match s.ask(Question::Entity(id)) {
        Answer::Entity(details) => details.health,
        _ => None,
    }
}

#[test]
fn a_cactus_hurts_the_player_standing_in_it() {
    let mut s = settled();
    assert_eq!(health(&s), 20.0);
    console(&mut s, "setblock 0 64 0 minecraft:cactus");
    s.tick(2);
    assert!(health(&s) < 20.0);
}

#[test]
fn a_cactus_hurts_a_mob_standing_in_it() {
    let mut s = settled();
    let cow = spawn(&mut s, "minecraft:cow", Vec3::new(5.5, 64.0, 5.5));
    console(&mut s, "setblock 5 64 5 minecraft:cactus");
    s.tick(3);
    assert!(mob_health(&mut s, cow.id).is_none_or(|h| h < 10.0));
}

fn step(s: &mut Scene, position: Vec3) {
    let player = s.players[0].clone();
    send_input(
        &mut s.server,
        &player,
        PlayerInput::Move {
            position: Some(position),
            rotation: None,
            on_ground: true,
        },
    );
}

#[test]
fn sweet_berries_hurt_only_a_moving_player() {
    let mut s = settled();
    console(&mut s, "setblock 0 64 0 minecraft:sweet_berry_bush[age=3]");
    s.tick(2);
    assert_eq!(health(&s), 20.0);
    step(&mut s, Vec3::new(0.7, 64.0, 0.5));
    s.tick(1);
    assert!(health(&s) < 20.0);
}

#[test]
fn a_young_sweet_berry_bush_does_not_hurt() {
    let mut s = settled();
    console(&mut s, "setblock 0 64 0 minecraft:sweet_berry_bush[age=0]");
    step(&mut s, Vec3::new(0.7, 64.0, 0.5));
    s.tick(1);
    assert_eq!(health(&s), 20.0);
}

#[test]
fn a_lit_campfire_burns_and_an_unlit_one_does_not() {
    let mut s = settled();
    console(&mut s, "setblock 0 64 0 minecraft:campfire[lit=false]");
    s.tick(2);
    assert_eq!(health(&s), 20.0);
    console(&mut s, "setblock 0 64 0 minecraft:campfire[lit=true]");
    s.tick(2);
    assert!(health(&s) < 20.0);
}

#[test]
fn a_soul_campfire_hurts_twice_as_much() {
    let mut s = settled();
    console(&mut s, "setblock 0 64 0 minecraft:soul_campfire[lit=true]");
    s.tick(1);
    assert_eq!(health(&s), 18.0);
}

#[test]
fn a_block_in_the_head_suffocates() {
    let mut s = settled();
    console(&mut s, "setblock 0 65 0 minecraft:stone");
    s.tick(2);
    assert!(health(&s) < 20.0);
}

#[test]
fn standing_on_magma_burns_the_feet() {
    let mut s = settled();
    console(&mut s, "setblock 0 63 0 minecraft:magma_block");
    step(&mut s, Vec3::new(0.5, 64.0, 0.5));
    s.tick(2);
    assert!(health(&s) < 20.0);
}
