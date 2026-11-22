mod common;

use common::*;
use rspigot_core::plugins::events::BlockIgniteCause;
use rspigot_core::plugins::{
    Answer, BlockOrder, EventKind, EventPayload, HandlerSpec, Order, Question,
};
use rspigot_domain::BlockPos;

const LAVA: BlockPos = BlockPos::new(4, 64, 4);
const PLANKS: BlockPos = BlockPos::new(5, 64, 4);

fn lava_scene(handlers: Vec<HandlerSpec>) -> Scene {
    let server = configured(|config| config.rules.random_tick_speed = 3000);
    let mut s = scene_on(server, &["Alice"], handlers);
    for (pos, kind) in [(LAVA, "minecraft:lava"), (PLANKS, "minecraft:oak_planks")] {
        s.act(Order::Block {
            pos,
            order: BlockOrder::Set(kind.to_owned()),
        });
    }
    s
}

fn kind_at(s: &mut Scene, pos: BlockPos) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state.kind,
        other => panic!("expected a block, got {other:?}"),
    }
}

fn lava_ignitions(s: &Scene) -> Vec<BlockPos> {
    settled_for(&s.plugin, EventKind::BlockIgnite)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::BlockIgnite(ignite) if ignite.cause == BlockIgniteCause::Lava => {
                Some(ignite.block)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn lava_next_to_a_flammable_block_asks_block_ignite_with_the_lava_cause() {
    let mut s = lava_scene(vec![observe(1, EventKind::BlockIgnite)]);
    s.tick(100);
    let lit = lava_ignitions(&s);
    assert!(!lit.is_empty());
    for pos in &lit {
        assert_eq!(kind_at(&mut s, *pos), "minecraft:fire");
    }
}

#[test]
fn a_cancelled_lava_ignition_keeps_the_air() {
    let mut s = lava_scene(vec![deciding(1, EventKind::BlockIgnite)]);
    let mut asked = None;
    for _ in 0..200 {
        s.tick(1);
        asked = asked_for(&s.plugin, EventKind::BlockIgnite).pop();
        if asked.is_some() {
            break;
        }
    }
    let asked = asked.expect("lava asked an ignition");
    let EventPayload::BlockIgnite(ignite) = &asked.payload else {
        panic!("expected a BlockIgnite");
    };
    assert_eq!(ignite.cause, BlockIgniteCause::Lava);
    let pos = ignite.block;
    judge(&mut s.server, 1, asked.id, 1, true, None);
    assert_eq!(kind_at(&mut s, pos), "minecraft:air");
}

#[test]
fn the_fire_spread_radius_rule_at_zero_stops_lava_from_lighting_fires() {
    let server = configured(|config| {
        config.rules.random_tick_speed = 3000;
        config.rules.fire_spread_radius = 0;
    });
    let mut s = scene_on(server, &["Alice"], vec![observe(1, EventKind::BlockIgnite)]);
    for (pos, kind) in [(LAVA, "minecraft:lava"), (PLANKS, "minecraft:oak_planks")] {
        s.act(Order::Block {
            pos,
            order: BlockOrder::Set(kind.to_owned()),
        });
    }
    s.tick(100);
    assert!(lava_ignitions(&s).is_empty());
}
