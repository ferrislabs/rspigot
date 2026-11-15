mod common;

use common::*;
use rspigot_core::plugins::events::Face;
use rspigot_core::plugins::{Answer, BlockOrder, EventKind, EventPayload, Order, Question};
use rspigot_domain::BlockPos;

const PISTON: BlockPos = BlockPos::new(3, 64, 3);
const POWER: BlockPos = BlockPos::new(3, 64, 2);

fn at(x: i32) -> BlockPos {
    BlockPos::new(x, 64, 3)
}

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

fn state(s: &mut Scene, pos: BlockPos) -> (String, Vec<(String, String)>) {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => (state.kind, state.properties),
        other => panic!("expected a block, got {other:?}"),
    }
}

fn kind_at(s: &mut Scene, pos: BlockPos) -> String {
    state(s, pos).0
}

fn prop(s: &mut Scene, pos: BlockPos, name: &str) -> String {
    state(s, pos)
        .1
        .into_iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
        .unwrap_or_default()
}

fn piston(s: &mut Scene, kind: &str) {
    put(
        s,
        PISTON,
        kind,
        &[("facing", "east"), ("extended", "false")],
    );
}

fn power_on(s: &mut Scene) {
    put(s, POWER, "minecraft:redstone_block", &[]);
}

fn power_off(s: &mut Scene) {
    put(s, POWER, "minecraft:air", &[]);
}

#[test]
fn a_powered_piston_pushes_the_blocks_in_front_of_it() {
    let mut s = scene(&["Alice"]);
    put(&mut s, at(4), "minecraft:stone", &[]);
    put(&mut s, at(5), "minecraft:dirt", &[]);
    piston(&mut s, "minecraft:piston");
    power_on(&mut s);
    s.tick(1);
    assert_eq!(kind_at(&mut s, at(4)), "minecraft:moving_piston");
    s.tick(3);
    assert_eq!(prop(&mut s, PISTON, "extended"), "true");
    assert_eq!(kind_at(&mut s, at(4)), "minecraft:piston_head");
    assert_eq!(kind_at(&mut s, at(5)), "minecraft:stone");
    assert_eq!(kind_at(&mut s, at(6)), "minecraft:dirt");
}

#[test]
fn an_unpowered_piston_pulls_its_head_back() {
    let mut s = scene(&["Alice"]);
    piston(&mut s, "minecraft:piston");
    power_on(&mut s);
    s.tick(4);
    assert_eq!(kind_at(&mut s, at(4)), "minecraft:piston_head");
    power_off(&mut s);
    s.tick(4);
    assert_eq!(prop(&mut s, PISTON, "extended"), "false");
    assert_eq!(kind_at(&mut s, PISTON), "minecraft:piston");
    assert_eq!(kind_at(&mut s, at(4)), "minecraft:air");
}

#[test]
fn a_sticky_piston_pulls_the_block_it_pushed() {
    let mut s = scene(&["Alice"]);
    put(&mut s, at(4), "minecraft:stone", &[]);
    piston(&mut s, "minecraft:sticky_piston");
    power_on(&mut s);
    s.tick(4);
    assert_eq!(kind_at(&mut s, at(5)), "minecraft:stone");
    power_off(&mut s);
    s.tick(5);
    assert_eq!(kind_at(&mut s, at(4)), "minecraft:stone");
    assert_eq!(kind_at(&mut s, at(5)), "minecraft:air");
    assert_eq!(kind_at(&mut s, PISTON), "minecraft:sticky_piston");
}

#[test]
fn a_piston_cannot_push_more_than_twelve_blocks() {
    let mut s = scene(&["Alice"]);
    for x in 4..=16 {
        put(&mut s, at(x), "minecraft:stone", &[]);
    }
    piston(&mut s, "minecraft:piston");
    power_on(&mut s);
    s.tick(4);
    assert_eq!(prop(&mut s, PISTON, "extended"), "false");
    assert_eq!(kind_at(&mut s, at(4)), "minecraft:stone");
}

#[test]
fn twelve_blocks_move_together() {
    let mut s = scene(&["Alice"]);
    for x in 4..=15 {
        put(&mut s, at(x), "minecraft:stone", &[]);
    }
    piston(&mut s, "minecraft:piston");
    power_on(&mut s);
    s.tick(4);
    assert_eq!(prop(&mut s, PISTON, "extended"), "true");
    assert_eq!(kind_at(&mut s, at(16)), "minecraft:stone");
}

#[test]
fn obsidian_stops_a_piston() {
    let mut s = scene(&["Alice"]);
    put(&mut s, at(4), "minecraft:obsidian", &[]);
    piston(&mut s, "minecraft:piston");
    power_on(&mut s);
    s.tick(4);
    assert_eq!(prop(&mut s, PISTON, "extended"), "false");
}

#[test]
fn a_torch_in_the_way_is_broken_by_the_piston() {
    let mut s = scene(&["Alice"]);
    put(&mut s, at(4), "minecraft:torch", &[]);
    piston(&mut s, "minecraft:piston");
    power_on(&mut s);
    s.tick(4);
    assert_eq!(prop(&mut s, PISTON, "extended"), "true");
    assert_eq!(kind_at(&mut s, at(4)), "minecraft:piston_head");
}

#[test]
fn slime_drags_a_block_resting_on_it() {
    let mut s = scene(&["Alice"]);
    let high = |x: i32, y: i32| BlockPos::new(x, y, 3);
    put(&mut s, high(4, 70), "minecraft:slime_block", &[]);
    put(&mut s, high(4, 71), "minecraft:stone", &[]);
    put(
        &mut s,
        high(3, 70),
        "minecraft:piston",
        &[("facing", "east"), ("extended", "false")],
    );
    put(
        &mut s,
        BlockPos::new(3, 70, 2),
        "minecraft:redstone_block",
        &[],
    );
    s.tick(4);
    assert_eq!(kind_at(&mut s, high(5, 70)), "minecraft:slime_block");
    assert_eq!(kind_at(&mut s, high(5, 71)), "minecraft:stone");
}

#[test]
fn an_extend_event_lists_the_blocks_it_moves_and_breaks() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::BlockPistonExtend)]);
    put(&mut s, at(4), "minecraft:stone", &[]);
    put(&mut s, at(5), "minecraft:torch", &[]);
    piston(&mut s, "minecraft:piston");
    power_on(&mut s);
    s.tick(2);
    let heard = settled_for(&s.plugin, EventKind::BlockPistonExtend);
    assert_eq!(heard.len(), 1);
    let EventPayload::BlockPistonExtend(event) = &heard[0].payload else {
        panic!("an extend event");
    };
    assert_eq!(event.block, PISTON);
    assert_eq!(event.blocks, vec![at(4), at(5)]);
    assert_eq!(event.direction, Face::East);
    assert!(!event.sticky);
}

#[test]
fn a_cancelled_extension_leaves_everything_in_place() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::BlockPistonExtend)]);
    put(&mut s, at(4), "minecraft:stone", &[]);
    piston(&mut s, "minecraft:sticky_piston");
    power_on(&mut s);
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::BlockPistonExtend);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(4);
    assert_eq!(prop(&mut s, PISTON, "extended"), "false");
    assert_eq!(kind_at(&mut s, at(4)), "minecraft:stone");
}

#[test]
fn a_retract_event_comes_for_a_plain_piston_with_no_blocks() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::BlockPistonRetract)]);
    piston(&mut s, "minecraft:piston");
    power_on(&mut s);
    s.tick(4);
    power_off(&mut s);
    s.tick(4);
    let heard = settled_for(&s.plugin, EventKind::BlockPistonRetract);
    assert_eq!(heard.len(), 1);
    let EventPayload::BlockPistonRetract(event) = &heard[0].payload else {
        panic!("a retract event");
    };
    assert_eq!(event.block, PISTON);
    assert!(event.blocks.is_empty());
    assert_eq!(event.direction, Face::East);
}

#[test]
fn a_sticky_retract_event_lists_the_pulled_block() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::BlockPistonRetract)]);
    put(&mut s, at(4), "minecraft:stone", &[]);
    piston(&mut s, "minecraft:sticky_piston");
    power_on(&mut s);
    s.tick(4);
    power_off(&mut s);
    s.tick(4);
    let heard = settled_for(&s.plugin, EventKind::BlockPistonRetract);
    assert_eq!(heard.len(), 1);
    let EventPayload::BlockPistonRetract(event) = &heard[0].payload else {
        panic!("a retract event");
    };
    assert_eq!(event.blocks, vec![at(5)]);
    assert!(event.sticky);
}

#[test]
fn a_cancelled_retraction_keeps_the_piston_out() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::BlockPistonRetract)]);
    piston(&mut s, "minecraft:piston");
    power_on(&mut s);
    s.tick(4);
    power_off(&mut s);
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::BlockPistonRetract);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(4);
    assert_eq!(prop(&mut s, PISTON, "extended"), "true");
    assert_eq!(kind_at(&mut s, at(4)), "minecraft:piston_head");
}

#[test]
fn a_moving_block_carries_the_player_standing_in_its_way() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].id;
    let before = match s.ask(Question::Player(alice)) {
        Answer::Player(state) => state.player.position.x,
        other => panic!("expected a player, got {other:?}"),
    };
    put(
        &mut s,
        BlockPos::new(-1, 64, 0),
        "minecraft:piston",
        &[("facing", "east"), ("extended", "false")],
    );
    put(&mut s, BlockPos::new(0, 64, 0), "minecraft:stone", &[]);
    put(
        &mut s,
        BlockPos::new(-1, 64, -1),
        "minecraft:redstone_block",
        &[],
    );
    s.tick(5);
    let after = match s.ask(Question::Player(alice)) {
        Answer::Player(state) => state.player.position.x,
        other => panic!("expected a player, got {other:?}"),
    };
    assert!(after > before + 0.9, "{before} -> {after}");
}
