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

#[test]
fn a_lightning_rod_powers_its_neighbours_for_eight_ticks_after_a_strike() {
    let mut s = scene(&["Alice"]);
    let rod = BlockPos::new(6, 64, 6);
    let lamp = BlockPos::new(7, 64, 6);
    put(&mut s, rod, "minecraft:lightning_rod", &[("facing", "up")]);
    put(&mut s, lamp, "minecraft:redstone_lamp", &[]);
    assert_eq!(prop(&mut s, rod, "powered"), "false");
    s.server.handle(rspigot_core::events::Inbound::Console(
        "summon minecraft:lightning_bolt 6.5 65 6.5".to_owned(),
    ));
    s.server.deliver();
    s.tick(2);
    assert_eq!(prop(&mut s, rod, "powered"), "true");
    assert_eq!(prop(&mut s, lamp, "lit"), "true");
    s.tick(12);
    assert_eq!(prop(&mut s, rod, "powered"), "false");
    s.tick(8);
    assert_eq!(prop(&mut s, lamp, "lit"), "false");
}

#[test]
fn a_lightning_rod_powers_the_block_it_is_planted_on() {
    let mut s = scene(&["Alice"]);
    let rod = BlockPos::new(6, 64, 6);
    let base = BlockPos::new(6, 63, 6);
    let lamp = BlockPos::new(6, 63, 7);
    put(&mut s, base, "minecraft:stone", &[]);
    put(&mut s, lamp, "minecraft:redstone_lamp", &[]);
    put(&mut s, rod, "minecraft:lightning_rod", &[("facing", "up")]);
    s.server.handle(rspigot_core::events::Inbound::Console(
        "summon minecraft:lightning_bolt 6.5 65 6.5".to_owned(),
    ));
    s.server.deliver();
    s.tick(2);
    assert_eq!(prop(&mut s, rod, "powered"), "true");
    assert_eq!(prop(&mut s, lamp, "lit"), "true");
}

fn trip_line(s: &mut Scene) {
    put(s, BlockPos::new(1, 64, 5), "minecraft:stone", &[]);
    put(s, BlockPos::new(6, 64, 5), "minecraft:stone", &[]);
    put(
        s,
        BlockPos::new(2, 64, 5),
        "minecraft:tripwire_hook",
        &[("facing", "east")],
    );
    put(s, BlockPos::new(3, 64, 5), "minecraft:tripwire", &[]);
    put(s, BlockPos::new(4, 64, 5), "minecraft:tripwire", &[]);
    put(
        s,
        BlockPos::new(5, 64, 5),
        "minecraft:tripwire_hook",
        &[("facing", "west")],
    );
    s.tick(2);
}

#[test]
fn two_hooks_with_a_wire_between_them_attach_and_the_wires_connect() {
    let mut s = scene(&["Alice"]);
    trip_line(&mut s);
    for hook in [BlockPos::new(2, 64, 5), BlockPos::new(5, 64, 5)] {
        assert_eq!(prop(&mut s, hook, "attached"), "true");
        assert_eq!(prop(&mut s, hook, "powered"), "false");
    }
    for wire in [BlockPos::new(3, 64, 5), BlockPos::new(4, 64, 5)] {
        assert_eq!(prop(&mut s, wire, "attached"), "true");
    }
    assert_eq!(prop(&mut s, BlockPos::new(3, 64, 5), "east"), "true");
}

#[test]
fn a_lone_hook_stays_detached() {
    let mut s = scene(&["Alice"]);
    put(&mut s, BlockPos::new(1, 64, 5), "minecraft:stone", &[]);
    put(
        &mut s,
        BlockPos::new(2, 64, 5),
        "minecraft:tripwire_hook",
        &[("facing", "east")],
    );
    assert_eq!(prop(&mut s, BlockPos::new(2, 64, 5), "attached"), "false");
}

#[test]
fn something_walking_into_the_wire_powers_both_hooks_for_a_while() {
    let mut s = scene(&["Alice"]);
    trip_line(&mut s);
    let lamp = BlockPos::new(2, 64, 6);
    put(&mut s, lamp, "minecraft:redstone_lamp", &[]);
    let Answer::Spawned(cart) = s.ask(Question::Spawn {
        kind: "minecraft:minecart".to_owned(),
        position: Vec3::new(3.5, 64.1, 5.5),
    }) else {
        panic!("a spawned minecart");
    };
    s.tick(3);
    assert_eq!(prop(&mut s, BlockPos::new(3, 64, 5), "powered"), "true");
    assert_eq!(prop(&mut s, BlockPos::new(2, 64, 5), "powered"), "true");
    assert_eq!(prop(&mut s, BlockPos::new(5, 64, 5), "powered"), "true");
    assert_eq!(prop(&mut s, lamp, "lit"), "true");
    s.act(Order::Entity {
        entity: cart.id,
        order: EntityOrder::Remove,
    });
    s.tick(30);
    assert_eq!(prop(&mut s, BlockPos::new(3, 64, 5), "powered"), "false");
    assert_eq!(prop(&mut s, BlockPos::new(2, 64, 5), "powered"), "false");
    assert_eq!(prop(&mut s, lamp, "lit"), "false");
}

#[test]
fn a_disarmed_wire_does_not_power_the_hooks() {
    let mut s = scene(&["Alice"]);
    trip_line(&mut s);
    put(
        &mut s,
        BlockPos::new(3, 64, 5),
        "minecraft:tripwire",
        &[
            ("disarmed", "true"),
            ("attached", "true"),
            ("east", "true"),
            ("west", "true"),
        ],
    );
    let Answer::Spawned(_) = s.ask(Question::Spawn {
        kind: "minecraft:minecart".to_owned(),
        position: Vec3::new(3.5, 64.1, 5.5),
    }) else {
        panic!("a spawned minecart");
    };
    s.tick(3);
    assert_eq!(prop(&mut s, BlockPos::new(2, 64, 5), "powered"), "false");
}

#[test]
fn breaking_a_wire_trips_the_hooks_once_then_detaches_them() {
    let mut s = scene(&["Alice"]);
    trip_line(&mut s);
    put(&mut s, BlockPos::new(4, 64, 5), "minecraft:air", &[]);
    s.tick(2);
    assert_eq!(prop(&mut s, BlockPos::new(2, 64, 5), "powered"), "true");
    s.tick(15);
    assert_eq!(prop(&mut s, BlockPos::new(2, 64, 5), "powered"), "false");
    for hook in [BlockPos::new(2, 64, 5), BlockPos::new(5, 64, 5)] {
        assert_eq!(prop(&mut s, hook, "attached"), "false");
    }
}

#[test]
fn the_new_sources_report_a_redstone_event_and_a_cancel_keeps_them_off() {
    use rspigot_core::plugins::events::BlockRedstonePatch;
    use rspigot_core::plugins::{EventKind, EventPatch, EventPayload};
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::BlockRedstone)]);
    let rod = BlockPos::new(6, 64, 6);
    put(&mut s, rod, "minecraft:lightning_rod", &[("facing", "up")]);
    s.server.handle(rspigot_core::events::Inbound::Console(
        "summon minecraft:lightning_bolt 6.5 65 6.5".to_owned(),
    ));
    s.server.deliver();
    s.tick(1);
    let asked = asked_for(&s.plugin, EventKind::BlockRedstone);
    let first = asked
        .iter()
        .find(|fired| matches!(&fired.payload, EventPayload::BlockRedstone(event) if event.block == rod))
        .expect("the rod asks");
    assert!(
        matches!(&first.payload, EventPayload::BlockRedstone(event) if event.old_current == 0 && event.new_current == 15)
    );
    let id = first.id;
    rule_on(
        &mut s.server,
        1,
        id,
        false,
        Some(EventPatch::BlockRedstone(BlockRedstonePatch {
            new_current: Some(0),
        })),
    );
    s.tick(2);
    assert_eq!(prop(&mut s, rod, "powered"), "false");
}

#[test]
fn a_comparator_reads_an_item_frame_behind_a_solid_block() {
    let mut s = scene(&["Alice"]);
    put(
        &mut s,
        BlockPos::new(3, 64, 5),
        "minecraft:comparator",
        &[("facing", "east")],
    );
    put(&mut s, BlockPos::new(4, 64, 5), "minecraft:stone", &[]);
    put(
        &mut s,
        BlockPos::new(2, 64, 5),
        "minecraft:redstone_wire",
        &[],
    );
    s.server.handle(rspigot_core::events::Inbound::Console(
        "summon minecraft:item_frame 5.5 64.5 5.5 {Facing:5b,TileX:5,TileY:64,TileZ:5,Item:{id:\"minecraft:stone\",count:1},ItemRotation:3b}"
            .to_owned(),
    ));
    s.server.deliver();
    assert_eq!(
        s.server
            .entities_of(rspigot_entities::EntityKind::ItemFrame)
            .len(),
        1
    );
    put(&mut s, BlockPos::new(3, 64, 5), "minecraft:air", &[]);
    put(
        &mut s,
        BlockPos::new(3, 64, 5),
        "minecraft:comparator",
        &[("facing", "east")],
    );
    s.tick(6);
    assert_eq!(prop(&mut s, BlockPos::new(2, 64, 5), "power"), "4");
}

#[test]
fn a_comparator_two_blocks_from_a_cauldron_follows_its_level() {
    let mut s = scene(&["Alice"]);
    put(
        &mut s,
        BlockPos::new(3, 64, 5),
        "minecraft:comparator",
        &[("facing", "east")],
    );
    put(&mut s, BlockPos::new(4, 64, 5), "minecraft:stone", &[]);
    put(
        &mut s,
        BlockPos::new(2, 64, 5),
        "minecraft:redstone_wire",
        &[],
    );
    put(
        &mut s,
        BlockPos::new(5, 64, 5),
        "minecraft:water_cauldron",
        &[("level", "3")],
    );
    s.tick(6);
    assert_eq!(prop(&mut s, BlockPos::new(2, 64, 5), "power"), "3");
    put(
        &mut s,
        BlockPos::new(5, 64, 5),
        "minecraft:water_cauldron",
        &[("level", "1")],
    );
    s.tick(6);
    assert_eq!(prop(&mut s, BlockPos::new(2, 64, 5), "power"), "1");
}
