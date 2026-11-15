mod common;
#[path = "common/screen.rs"]
mod screen;

use common::*;
use rspigot_core::plugins::{Answer, BlockOrder, EntityOrder, Order, Question};
use rspigot_domain::{BlockPos, Vec3};

fn put(s: &mut Scene, pos: BlockPos, kind: &str, properties: &[(&str, &str)]) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::SetWith {
            kind: kind.to_owned(),
            properties: properties
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
        },
    });
}

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

#[test]
fn a_detector_rail_powers_a_lamp_beside_it_while_a_minecart_rides_it() {
    let mut s = scene(&["Alice"]);
    let rail = BlockPos::new(4, 64, 5);
    let lamp = BlockPos::new(5, 64, 5);
    put(&mut s, rail, "minecraft:detector_rail", &[]);
    put(&mut s, lamp, "minecraft:redstone_lamp", &[]);
    assert_eq!(prop(&mut s, lamp, "lit"), "false");
    let Answer::Spawned(cart) = s.ask(Question::Spawn {
        kind: "minecraft:minecart".to_owned(),
        position: Vec3::new(4.5, 64.1, 5.5),
    }) else {
        panic!("a spawned minecart");
    };
    s.tick(2);
    assert_eq!(prop(&mut s, rail, "powered"), "true");
    assert_eq!(prop(&mut s, lamp, "lit"), "true");
    s.act(Order::Entity {
        entity: cart.id,
        order: EntityOrder::Remove,
    });
    s.tick(30);
    assert_eq!(prop(&mut s, lamp, "lit"), "false");
}

fn shoot_arrow(s: &mut Scene, from: Vec3, velocity: Vec3) {
    let Answer::Spawned(arrow) = s.ask(Question::Spawn {
        kind: "minecraft:arrow".to_owned(),
        position: from,
    }) else {
        panic!("a spawned arrow");
    };
    s.act(Order::Entity {
        entity: arrow.id,
        order: EntityOrder::SetVelocity(velocity),
    });
}

#[test]
fn an_arrow_into_the_centre_of_a_target_gives_nearly_fifteen_for_twenty_ticks() {
    let mut s = scene(&["Alice"]);
    let target = BlockPos::new(8, 64, 5);
    let lamp = BlockPos::new(8, 65, 5);
    put(&mut s, target, "minecraft:target", &[]);
    put(&mut s, lamp, "minecraft:redstone_lamp", &[]);
    shoot_arrow(&mut s, Vec3::new(4.5, 64.5, 5.5), Vec3::new(2.0, 0.0, 0.0));
    s.tick(4);
    let power: i32 = prop(&mut s, target, "power").parse().unwrap();
    assert!(power >= 13, "power {power}");
    assert_eq!(prop(&mut s, lamp, "lit"), "true");
    s.tick(25);
    assert_eq!(prop(&mut s, target, "power"), "0");
}

#[test]
fn an_arrow_near_the_edge_of_a_target_gives_less_power() {
    let mut s = scene(&["Alice"]);
    let target = BlockPos::new(8, 64, 5);
    put(&mut s, target, "minecraft:target", &[]);
    shoot_arrow(&mut s, Vec3::new(4.5, 64.5, 5.05), Vec3::new(2.0, 0.0, 0.0));
    s.tick(4);
    let power: i32 = prop(&mut s, target, "power").parse().unwrap();
    assert!((1..15).contains(&power), "power {power}");
}

fn set_time(s: &mut Scene, ticks: i64) {
    s.server
        .handle(rspigot_core::events::Inbound::Console(format!(
            "time set {ticks}"
        )));
    s.server.deliver();
}

#[test]
fn a_daylight_detector_reads_the_sky_every_twenty_ticks() {
    let mut s = scene(&["Alice"]);
    let at = BlockPos::new(3, 64, 3);
    let lamp = BlockPos::new(3, 64, 4);
    set_time(&mut s, 6000);
    put(&mut s, at, "minecraft:daylight_detector", &[]);
    put(&mut s, lamp, "minecraft:redstone_lamp", &[]);
    s.tick(40);
    assert_eq!(prop(&mut s, at, "power"), "15");
    assert_eq!(prop(&mut s, lamp, "lit"), "true");
    set_time(&mut s, 18000);
    s.tick(60);
    assert_eq!(prop(&mut s, at, "power"), "0");
}

#[test]
fn using_a_daylight_detector_inverts_it() {
    let mut s = scene(&["Alice"]);
    let at = BlockPos::new(3, 64, 3);
    set_time(&mut s, 18000);
    put(&mut s, at, "minecraft:daylight_detector", &[]);
    s.tick(25);
    assert_eq!(prop(&mut s, at, "power"), "0");
    let alice = s.players[0].clone();
    screen::use_block(&mut s, &alice, at);
    assert_eq!(prop(&mut s, at, "inverted"), "true");
    assert_eq!(prop(&mut s, at, "power"), "11");
}
