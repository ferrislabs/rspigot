mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::plugins::events::{Face, FluidLevelChangePatch};
use rspigot_core::plugins::{
    Answer, BlockOrder, BlockState, EventKind, EventPatch, EventPayload, HandlerSpec, Order,
    Question,
};
use rspigot_domain::BlockPos;

const SOURCE: BlockPos = BlockPos::new(4, 64, 4);

fn waiting_scene(handlers: Vec<HandlerSpec>) -> Scene {
    let mut server = server(PluginSettings {
        decision_ticks: 1_000,
        ..PluginSettings::default()
    });
    let plugin = connect_with(&mut server, 1, "tester", handlers);
    let alice = join(&mut server, "Alice");
    server.deliver();
    alice.link.take();
    let mut s = Scene {
        server,
        plugin,
        players: vec![alice],
    };
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
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

fn kind_at(s: &mut Scene, pos: BlockPos) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(block) => block.kind,
        other => panic!("{other:?}"),
    }
}

fn flows(s: &Scene) -> Vec<rspigot_core::plugins::events::BlockFromTo> {
    fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::BlockFromTo(flow) => Some(flow),
            _ => None,
        })
        .collect()
}

#[test]
fn a_water_source_asks_once_for_each_side_it_spreads_to() {
    let mut s = waiting_scene(vec![observe(1, EventKind::BlockFromTo)]);
    put(&mut s, SOURCE, "minecraft:water", &[]);
    s.tick(10);
    let mut seen: Vec<(Face, BlockPos)> = flows(&s)
        .into_iter()
        .filter(|flow| flow.block == SOURCE)
        .map(|flow| (flow.face, flow.to_block))
        .collect();
    seen.sort_by_key(|(_, pos)| (pos.x, pos.z));
    assert_eq!(
        seen,
        [
            (Face::West, SOURCE.offset(-1, 0, 0)),
            (Face::North, SOURCE.offset(0, 0, -1)),
            (Face::South, SOURCE.offset(0, 0, 1)),
            (Face::East, SOURCE.offset(1, 0, 0)),
        ]
    );
    assert_eq!(kind_at(&mut s, SOURCE.offset(1, 0, 0)), "minecraft:water");
}

#[test]
fn a_cancelled_flow_leaves_the_target_dry_and_the_others_flow() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::BlockFromTo)]);
    put(&mut s, SOURCE, "minecraft:water", &[]);
    s.tick(10);
    let asked = asked_for(&s.plugin, EventKind::BlockFromTo);
    assert_eq!(asked.len(), 4);
    for event in &asked {
        let EventPayload::BlockFromTo(flow) = &event.payload else {
            panic!("a flow");
        };
        rule_on(&mut s.server, 1, event.id, flow.face == Face::East, None);
    }
    s.tick(2);
    assert_eq!(kind_at(&mut s, SOURCE.offset(1, 0, 0)), "minecraft:air");
    assert_eq!(kind_at(&mut s, SOURCE.offset(-1, 0, 0)), "minecraft:water");
    assert_eq!(kind_at(&mut s, SOURCE.offset(0, 0, 1)), "minecraft:water");
}

#[test]
fn a_flowing_block_with_no_feeder_asks_to_become_air_and_a_cancel_keeps_it() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::FluidLevelChange)]);
    put(&mut s, SOURCE, "minecraft:water", &[("level", "3")]);
    s.tick(10);
    let asked = last_asked(&s.plugin, EventKind::FluidLevelChange);
    let EventPayload::FluidLevelChange(change) = &asked.payload else {
        panic!("a level change");
    };
    assert_eq!(change.block, SOURCE);
    assert_eq!(change.new_data.kind, "minecraft:air");
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(2);
    assert_eq!(kind_at(&mut s, SOURCE), "minecraft:water");
}

#[test]
fn an_allowed_level_change_dries_the_block_and_a_patch_sets_what_replaces_it() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::FluidLevelChange)]);
    put(&mut s, SOURCE, "minecraft:water", &[("level", "3")]);
    s.tick(10);
    let asked = last_asked(&s.plugin, EventKind::FluidLevelChange);
    rule_on(&mut s.server, 1, asked.id, false, None);
    s.tick(2);
    assert_eq!(kind_at(&mut s, SOURCE), "minecraft:air");
    let other = SOURCE.offset(5, 0, 0);
    put(&mut s, other, "minecraft:water", &[("level", "3")]);
    s.tick(10);
    let asked = last_asked(&s.plugin, EventKind::FluidLevelChange);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::FluidLevelChange(FluidLevelChangePatch {
            new_data: Some(BlockState {
                kind: "minecraft:stone".to_owned(),
                properties: Vec::new(),
            }),
        })),
    );
    s.tick(2);
    assert_eq!(kind_at(&mut s, other), "minecraft:stone");
}

#[test]
fn water_over_a_hole_asks_to_fall_and_a_cancel_keeps_the_hole_dry() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::BlockFromTo)]);
    let hole = BlockPos::new(8, 63, 8);
    put(&mut s, hole, "minecraft:air", &[]);
    put(&mut s, hole.offset(0, 1, 0), "minecraft:water", &[]);
    s.tick(10);
    let asked = asked_for(&s.plugin, EventKind::BlockFromTo);
    assert_eq!(asked.len(), 1);
    let EventPayload::BlockFromTo(flow) = &asked[0].payload else {
        panic!("a flow");
    };
    assert_eq!((flow.face, flow.to_block), (Face::Down, hole));
    rule_on(&mut s.server, 1, asked[0].id, true, None);
    s.tick(2);
    assert_eq!(kind_at(&mut s, hole), "minecraft:air");
}
