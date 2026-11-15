mod common;

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
