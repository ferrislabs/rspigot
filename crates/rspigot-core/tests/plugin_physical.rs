mod common;

use common::*;
use rspigot_core::plugins::events::{Action, EventResult, Face, PlayerInteractPatch};
use rspigot_core::plugins::{
    Answer, BlockOrder, EventKind, EventPatch, EventPayload, Order, Question,
};
use rspigot_domain::{BlockPos, Vec3};

const PLATE: BlockPos = BlockPos::new(3, 64, 3);

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

fn cart_on(s: &mut Scene, pos: BlockPos) {
    let Answer::Spawned(_) = s.ask(Question::Spawn {
        kind: "minecraft:minecart".to_owned(),
        position: Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.1,
            f64::from(pos.z) + 0.5,
        ),
    }) else {
        panic!("a spawned minecart");
    };
}

#[test]
fn a_player_stepping_on_a_plate_fires_a_physical_interact() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerInteract)]);
    let plate = BlockPos::new(0, 64, 0);
    put(&mut s, plate, "minecraft:stone_pressure_plate", &[]);
    s.tick(2);
    let heard = settled_for(&s.plugin, EventKind::PlayerInteract);
    let EventPayload::PlayerInteract(event) = &heard.first().expect("an interact").payload else {
        panic!("an interact event");
    };
    assert_eq!(event.action, Action::Physical);
    assert_eq!(event.clicked_block, Some(plate));
    assert_eq!(event.block_face, Face::Itself);
    assert!(!event.has_item);
    assert_eq!(prop(&mut s, plate, "powered"), "true");
}

#[test]
fn a_denied_physical_interact_keeps_the_plate_up() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerInteract)]);
    let plate = BlockPos::new(0, 64, 0);
    put(&mut s, plate, "minecraft:stone_pressure_plate", &[]);
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::PlayerInteract);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::PlayerInteract(PlayerInteractPatch {
            use_interacted_block: Some(EventResult::Deny),
            use_item_in_hand: None,
        })),
    );
    assert_eq!(prop(&mut s, plate, "powered"), "false");
}

#[test]
fn an_entity_on_a_plate_fires_an_entity_interact_and_a_cancel_keeps_it_up() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::EntityInteract)]);
    put(&mut s, PLATE, "minecraft:oak_pressure_plate", &[]);
    cart_on(&mut s, PLATE);
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::EntityInteract);
    let EventPayload::EntityInteract(event) = &asked.payload else {
        panic!("an entity interact");
    };
    assert_eq!(event.block, PLATE);
    assert_eq!(event.entity_type, "minecraft:minecart");
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(prop(&mut s, PLATE, "powered"), "false");
}

#[test]
fn an_allowed_entity_interact_presses_the_plate() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::EntityInteract)]);
    put(&mut s, PLATE, "minecraft:oak_pressure_plate", &[]);
    cart_on(&mut s, PLATE);
    s.tick(2);
    assert_eq!(prop(&mut s, PLATE, "powered"), "true");
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
fn a_cancelled_entity_interact_keeps_an_attached_tripwire_from_tripping() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::EntityInteract)]);
    trip_line(&mut s);
    cart_on(&mut s, BlockPos::new(3, 64, 5));
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::EntityInteract);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(prop(&mut s, BlockPos::new(3, 64, 5), "powered"), "false");
    assert_eq!(prop(&mut s, BlockPos::new(2, 64, 5), "powered"), "false");
}

#[test]
fn an_allowed_entity_interact_trips_an_attached_tripwire() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::EntityInteract)]);
    trip_line(&mut s);
    cart_on(&mut s, BlockPos::new(3, 64, 5));
    s.tick(3);
    assert_eq!(prop(&mut s, BlockPos::new(3, 64, 5), "powered"), "true");
    assert_eq!(prop(&mut s, BlockPos::new(2, 64, 5), "powered"), "true");
}
