mod common;

use common::*;
use rspigot_core::events::{Direction, Hand, PlayerInput};
use rspigot_core::plugins::events::{EquipmentSlot, PlayerHarvestBlockPatch};
use rspigot_core::plugins::{
    Answer, BlockOrder, EventKind, EventPatch, EventPayload, ItemSpec, Order, Question,
};
use rspigot_domain::{BlockPos, Vec3};

const BUSH: BlockPos = BlockPos::new(2, 64, 1);

fn click(s: &mut Scene, pos: BlockPos) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 0.5, 0.5),
            inside_block: false,
            sequence: 1,
        },
    );
}

fn age_at(s: &mut Scene, pos: BlockPos) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state
            .properties
            .iter()
            .find(|(name, _)| name == "age")
            .map(|(_, value)| value.clone())
            .unwrap_or_default(),
        other => panic!("expected a block, got {other:?}"),
    }
}

fn set(s: &mut Scene, pos: BlockPos, kind: &str, properties: &[(&str, &str)]) {
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

fn plant(s: &mut Scene, age: u8) {
    stream_chunk_of(s, BUSH);
    set(
        s,
        BUSH,
        "minecraft:sweet_berry_bush",
        &[("age", &age.to_string())],
    );
}

#[test]
fn picking_a_ripe_bush_fires_the_harvest_with_the_berries_and_resets_the_age() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerHarvestBlock)]);
    plant(&mut s, 3);
    click(&mut s, BUSH);
    assert_eq!(age_at(&mut s, BUSH), "1");
    assert!(s.server.items_on_ground() > 0);
    let events = settled_for(&s.plugin, EventKind::PlayerHarvestBlock);
    let EventPayload::PlayerHarvestBlock(event) = &events[0].payload else {
        panic!("expected a harvest event");
    };
    assert_eq!(event.harvested_block, BUSH);
    assert_eq!(event.hand, EquipmentSlot::Hand);
    assert!(
        event
            .items_harvested
            .iter()
            .all(|item| item.key == "minecraft:sweet_berries")
    );
    assert!(!event.items_harvested.is_empty());
}

#[test]
fn a_young_bush_is_not_harvested() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerHarvestBlock)]);
    plant(&mut s, 1);
    click(&mut s, BUSH);
    assert!(settled_for(&s.plugin, EventKind::PlayerHarvestBlock).is_empty());
    assert_eq!(s.server.items_on_ground(), 0);
}

#[test]
fn a_cancelled_harvest_keeps_the_berries_on_the_bush() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerHarvestBlock)]);
    plant(&mut s, 3);
    click(&mut s, BUSH);
    let asked = last_asked(&s.plugin, EventKind::PlayerHarvestBlock);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(age_at(&mut s, BUSH), "3");
    assert_eq!(s.server.items_on_ground(), 0);
}

#[test]
fn a_patched_harvest_drops_the_replaced_items() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerHarvestBlock)]);
    plant(&mut s, 3);
    click(&mut s, BUSH);
    let asked = last_asked(&s.plugin, EventKind::PlayerHarvestBlock);
    let EventPayload::PlayerHarvestBlock(event) = &asked.payload else {
        panic!("expected a harvest event");
    };
    let patch = PlayerHarvestBlockPatch {
        items_harvested: replacing(
            &event.items_harvested,
            vec![ItemSpec::of("minecraft:diamond", 2)],
        ),
    };
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::PlayerHarvestBlock(patch)),
    );
    let diamonds = s
        .server
        .ground_items()
        .into_iter()
        .filter(|(_, stack)| stack.item.kind().map(|kind| kind.name()) == Some("minecraft:diamond"))
        .count();
    assert_eq!(diamonds, 1);
    assert_eq!(age_at(&mut s, BUSH), "1");
}

#[test]
fn picking_glow_berries_fires_the_harvest_too() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerHarvestBlock)]);
    stream_chunk_of(&mut s, BUSH);
    set(&mut s, BUSH.offset(0, 1, 0), "minecraft:stone", &[]);
    set(
        &mut s,
        BUSH,
        "minecraft:cave_vines",
        &[("age", "0"), ("berries", "true")],
    );
    click(&mut s, BUSH);
    let events = settled_for(&s.plugin, EventKind::PlayerHarvestBlock);
    assert_eq!(events.len(), 1);
}
