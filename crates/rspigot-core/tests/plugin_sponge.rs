mod common;

use common::*;
use rspigot_core::plugins::{Answer, BlockOrder, EventKind, EventPayload, Order, Question};
use rspigot_domain::BlockPos;

const SPONGE: BlockPos = BlockPos::new(3, 64, 2);
const NEAR: BlockPos = BlockPos::new(3, 64, 3);
const FAR: BlockPos = BlockPos::new(3, 64, 4);

fn set(s: &mut Scene, pos: BlockPos, key: &str) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::Set(key.to_owned()),
    });
}

fn kind_at(s: &mut Scene, pos: BlockPos) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state.kind,
        other => panic!("expected a block, got {other:?}"),
    }
}

fn wet_pool(s: &mut Scene) {
    set(s, NEAR, "minecraft:water");
    set(s, FAR, "minecraft:water");
    set(s, SPONGE, "minecraft:sponge");
}

#[test]
fn a_sponge_drinks_the_connected_water_and_lists_the_blocks_it_replaces() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::SpongeAbsorb)]);
    wet_pool(&mut s);
    s.tick(2);
    let heard = settled_for(&s.plugin, EventKind::SpongeAbsorb);
    assert_eq!(heard.len(), 1);
    let EventPayload::SpongeAbsorb(event) = &heard[0].payload else {
        panic!("expected a sponge absorb");
    };
    assert_eq!(event.block, SPONGE);
    let listed: Vec<BlockPos> = event.blocks.iter().map(|block| block.pos).collect();
    assert_eq!(listed, vec![NEAR, FAR]);
    assert!(
        event
            .blocks
            .iter()
            .all(|block| block.state.kind == "minecraft:air")
    );
    assert_eq!(kind_at(&mut s, SPONGE), "minecraft:wet_sponge");
    assert_eq!(kind_at(&mut s, NEAR), "minecraft:air");
    assert_eq!(kind_at(&mut s, FAR), "minecraft:air");
}

#[test]
fn a_sponge_without_a_handler_still_absorbs() {
    let mut s = scene(&["Alice"]);
    wet_pool(&mut s);
    s.tick(2);
    assert_eq!(kind_at(&mut s, SPONGE), "minecraft:wet_sponge");
    assert_eq!(kind_at(&mut s, NEAR), "minecraft:air");
}

#[test]
fn a_cancelled_absorption_leaves_the_sponge_dry_and_the_water_in_place() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::SpongeAbsorb)]);
    wet_pool(&mut s);
    s.tick(2);
    let asked = last_asked(&s.plugin, EventKind::SpongeAbsorb);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(kind_at(&mut s, SPONGE), "minecraft:sponge");
    assert_eq!(kind_at(&mut s, NEAR), "minecraft:water");
    assert_eq!(kind_at(&mut s, FAR), "minecraft:water");
}

#[test]
fn a_sponge_in_dry_air_fires_nothing() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::SpongeAbsorb)]);
    set(&mut s, SPONGE, "minecraft:sponge");
    s.tick(2);
    assert!(settled_for(&s.plugin, EventKind::SpongeAbsorb).is_empty());
    assert_eq!(kind_at(&mut s, SPONGE), "minecraft:sponge");
}

#[test]
fn water_arriving_next_to_a_placed_sponge_is_absorbed() {
    let mut s = scene(&["Alice"]);
    set(&mut s, SPONGE, "minecraft:sponge");
    s.tick(2);
    set(&mut s, NEAR, "minecraft:water");
    s.tick(2);
    assert_eq!(kind_at(&mut s, SPONGE), "minecraft:wet_sponge");
    assert_eq!(kind_at(&mut s, NEAR), "minecraft:air");
}

#[test]
fn a_waterlogged_block_is_drained_by_a_sponge() {
    let mut s = scene(&["Alice"]);
    s.act(Order::Block {
        pos: NEAR,
        order: BlockOrder::SetWith {
            kind: "minecraft:oak_slab".to_owned(),
            properties: vec![("waterlogged".to_owned(), "true".to_owned())],
        },
    });
    set(&mut s, SPONGE, "minecraft:sponge");
    s.tick(2);
    let Answer::Block(state) = s.ask(Question::Block(NEAR)) else {
        panic!("a block");
    };
    assert_eq!(state.kind, "minecraft:oak_slab");
    assert!(
        state
            .properties
            .iter()
            .any(|(name, value)| name == "waterlogged" && value == "false")
    );
    assert_eq!(kind_at(&mut s, SPONGE), "minecraft:wet_sponge");
}
