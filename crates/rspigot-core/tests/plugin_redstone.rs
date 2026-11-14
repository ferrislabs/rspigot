mod common;
#[path = "common/screen.rs"]
mod screen;

use common::*;
use rspigot_core::plugins::events::BlockRedstonePatch;
use rspigot_core::plugins::{
    Answer, BlockOrder, EventKind, EventPatch, EventPayload, Order, Question,
};
use rspigot_domain::BlockPos;
use screen::use_block;

const LEVER: BlockPos = BlockPos::new(2, 64, 3);
const FIRST: BlockPos = BlockPos::new(3, 64, 3);
const SECOND: BlockPos = BlockPos::new(4, 64, 3);
const THIRD: BlockPos = BlockPos::new(5, 64, 3);
const LAMP: BlockPos = BlockPos::new(6, 64, 3);

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

fn kind_at(s: &mut Scene, pos: BlockPos) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state.kind,
        other => panic!("expected a block, got {other:?}"),
    }
}

fn lever_line(s: &mut Scene) {
    put(
        s,
        LEVER,
        "minecraft:lever",
        &[("face", "floor"), ("facing", "north")],
    );
    for pos in [FIRST, SECOND, THIRD] {
        put(s, pos, "minecraft:redstone_wire", &[]);
    }
    put(s, LAMP, "minecraft:redstone_lamp", &[]);
}

#[test]
fn a_lever_powers_a_wire_line_that_lights_a_lamp() {
    let mut s = scene(&["Alice"]);
    lever_line(&mut s);
    assert_eq!(prop(&mut s, FIRST, "power"), "0");
    assert_eq!(prop(&mut s, LAMP, "lit"), "false");
    let alice = s.players[0].clone();
    use_block(&mut s, &alice, LEVER);
    assert_eq!(prop(&mut s, LEVER, "powered"), "true");
    assert_eq!(prop(&mut s, FIRST, "power"), "15");
    assert_eq!(prop(&mut s, SECOND, "power"), "14");
    assert_eq!(prop(&mut s, THIRD, "power"), "13");
    assert_eq!(prop(&mut s, LAMP, "lit"), "true");
}

#[test]
fn a_lamp_stays_lit_four_ticks_after_its_power_leaves() {
    let mut s = scene(&["Alice"]);
    lever_line(&mut s);
    let alice = s.players[0].clone();
    use_block(&mut s, &alice, LEVER);
    use_block(&mut s, &alice, LEVER);
    assert_eq!(prop(&mut s, FIRST, "power"), "0");
    assert_eq!(prop(&mut s, THIRD, "power"), "0");
    assert_eq!(prop(&mut s, LAMP, "lit"), "true");
    s.tick(3);
    assert_eq!(prop(&mut s, LAMP, "lit"), "true");
    s.tick(2);
    assert_eq!(prop(&mut s, LAMP, "lit"), "false");
}

#[test]
fn a_wire_without_support_breaks() {
    let mut s = scene(&["Alice"]);
    put(&mut s, FIRST, "minecraft:redstone_wire", &[]);
    put(&mut s, BlockPos::new(3, 63, 3), "minecraft:glass_pane", &[]);
    assert_eq!(kind_at(&mut s, FIRST), "minecraft:air");
}

#[test]
fn a_redstone_torch_turns_off_two_ticks_after_its_support_is_powered() {
    let mut s = scene(&["Alice"]);
    let torch = BlockPos::new(3, 64, 6);
    put(&mut s, torch, "minecraft:redstone_torch", &[]);
    assert_eq!(prop(&mut s, torch, "lit"), "true");
    put(
        &mut s,
        BlockPos::new(3, 63, 6),
        "minecraft:redstone_block",
        &[],
    );
    s.tick(1);
    assert_eq!(prop(&mut s, torch, "lit"), "true");
    s.tick(2);
    assert_eq!(prop(&mut s, torch, "lit"), "false");
}

#[test]
fn a_torch_that_toggles_too_often_burns_out() {
    let mut s = scene(&["Alice"]);
    let torch = BlockPos::new(3, 64, 6);
    let support = BlockPos::new(3, 63, 6);
    put(&mut s, torch, "minecraft:redstone_torch", &[]);
    for _ in 0..9 {
        put(&mut s, support, "minecraft:redstone_block", &[]);
        s.tick(3);
        put(&mut s, support, "minecraft:stone", &[]);
        s.tick(3);
    }
    assert_eq!(prop(&mut s, torch, "lit"), "false");
    s.tick(3);
    assert_eq!(prop(&mut s, torch, "lit"), "false");
}

#[test]
fn a_stone_button_stays_pressed_for_twenty_ticks() {
    let mut s = scene(&["Alice"]);
    let button = BlockPos::new(3, 64, 1);
    put(
        &mut s,
        button,
        "minecraft:stone_button",
        &[("face", "floor"), ("facing", "north")],
    );
    let alice = s.players[0].clone();
    use_block(&mut s, &alice, button);
    assert_eq!(prop(&mut s, button, "powered"), "true");
    s.tick(19);
    assert_eq!(prop(&mut s, button, "powered"), "true");
    s.tick(2);
    assert_eq!(prop(&mut s, button, "powered"), "false");
}

#[test]
fn a_stone_pressure_plate_under_a_player_is_pressed() {
    let mut s = scene(&["Alice"]);
    put(
        &mut s,
        BlockPos::new(0, 64, 0),
        "minecraft:stone_pressure_plate",
        &[],
    );
    s.tick(2);
    assert_eq!(prop(&mut s, BlockPos::new(0, 64, 0), "powered"), "true");
}

#[test]
fn a_redstone_event_reports_the_old_and_new_current_of_every_block() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::BlockRedstone)]);
    lever_line(&mut s);
    let alice = s.players[0].clone();
    use_block(&mut s, &alice, LEVER);
    let heard: Vec<(BlockPos, i32, i32)> = settled_for(&s.plugin, EventKind::BlockRedstone)
        .iter()
        .filter_map(|fired| match &fired.payload {
            EventPayload::BlockRedstone(event) => {
                Some((event.block, event.old_current, event.new_current))
            }
            _ => None,
        })
        .collect();
    assert!(heard.contains(&(LEVER, 0, 15)));
    assert!(heard.contains(&(FIRST, 0, 15)));
    assert!(heard.contains(&(SECOND, 0, 14)));
    assert!(heard.contains(&(THIRD, 0, 13)));
    assert!(heard.contains(&(LAMP, 0, 15)));
}

#[test]
fn a_patched_current_decides_the_power_a_wire_takes() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::BlockRedstone)]);
    put(
        &mut s,
        BlockPos::new(3, 63, 3),
        "minecraft:redstone_block",
        &[],
    );
    put(&mut s, FIRST, "minecraft:redstone_wire", &[]);
    let asked = last_asked(&s.plugin, EventKind::BlockRedstone);
    let EventPayload::BlockRedstone(event) = &asked.payload else {
        panic!("a redstone event");
    };
    assert_eq!(
        (event.block, event.old_current, event.new_current),
        (FIRST, 0, 15)
    );
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::BlockRedstone(BlockRedstonePatch {
            new_current: Some(7),
        })),
    );
    assert_eq!(prop(&mut s, FIRST, "power"), "7");
}

#[test]
fn a_lever_whose_current_is_patched_to_zero_stays_off() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::BlockRedstone)]);
    put(
        &mut s,
        LEVER,
        "minecraft:lever",
        &[("face", "floor"), ("facing", "north")],
    );
    let alice = s.players[0].clone();
    use_block(&mut s, &alice, LEVER);
    let asked = last_asked(&s.plugin, EventKind::BlockRedstone);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::BlockRedstone(BlockRedstonePatch {
            new_current: Some(0),
        })),
    );
    assert_eq!(prop(&mut s, LEVER, "powered"), "false");
}
