mod common;

use common::*;
use rspigot_core::events::{Direction, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::{Answer, BlockOrder, Order, Question};
use rspigot_domain::{BlockPos, Vec3};

const FLOOR: BlockPos = BlockPos::new(2, 63, 0);
const FOOT: BlockPos = BlockPos::new(2, 64, 0);
const HEAD: BlockPos = BlockPos::new(2, 64, 1);

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn state_at(s: &mut Scene, pos: BlockPos) -> (String, Vec<(String, String)>) {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => (state.kind, state.properties),
        other => panic!("expected a block, got {other:?}"),
    }
}

fn kind_at(s: &mut Scene, pos: BlockPos) -> String {
    state_at(s, pos).0
}

fn property_at(s: &mut Scene, pos: BlockPos, name: &str) -> Option<String> {
    state_at(s, pos)
        .1
        .into_iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
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

fn click(s: &mut Scene, pos: BlockPos) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 1,
        },
    );
}

fn bed_scene() -> Scene {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, FLOOR);
    s
}

#[test]
fn placing_a_bed_places_the_foot_and_the_head_in_front_of_it() {
    let mut s = bed_scene();
    console(&mut s, "give Alice red_bed");
    click(&mut s, FLOOR);
    assert_eq!(kind_at(&mut s, FOOT), "minecraft:red_bed");
    assert_eq!(kind_at(&mut s, HEAD), "minecraft:red_bed");
    assert_eq!(property_at(&mut s, FOOT, "part").as_deref(), Some("foot"));
    assert_eq!(property_at(&mut s, HEAD, "part").as_deref(), Some("head"));
    assert_eq!(
        property_at(&mut s, HEAD, "facing").as_deref(),
        Some("south")
    );
}

#[test]
fn a_bed_is_not_placed_when_its_head_has_no_room() {
    let mut s = bed_scene();
    console(&mut s, "give Alice red_bed");
    set(&mut s, HEAD, "minecraft:stone", &[]);
    click(&mut s, FLOOR);
    assert_eq!(kind_at(&mut s, FOOT), "minecraft:air");
}

#[test]
fn breaking_one_half_of_a_bed_removes_the_other() {
    let mut s = bed_scene();
    set(
        &mut s,
        FOOT,
        "minecraft:red_bed",
        &[("part", "foot"), ("facing", "south")],
    );
    set(
        &mut s,
        HEAD,
        "minecraft:red_bed",
        &[("part", "head"), ("facing", "south")],
    );
    assert_eq!(kind_at(&mut s, HEAD), "minecraft:red_bed");
    set(&mut s, FOOT, "minecraft:air", &[]);
    assert_eq!(kind_at(&mut s, HEAD), "minecraft:air");
}
