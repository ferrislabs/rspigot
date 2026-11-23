#[path = "blocks_common/mod.rs"]
mod blocks_common;
mod common;
#[path = "common/screen.rs"]
mod screen;

use blocks_common::*;
use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::plugins::{Answer, Question};
use rspigot_domain::BlockPos;
use screen::use_block;

const ANCHOR: BlockPos = BlockPos::new(8, 64, 10);

fn rig(charges: &str) -> Scene {
    let cells = Cells::default().state(ANCHOR, BlockKind::RespawnAnchor, &[("charges", charges)]);
    let mut s = scene_of(cells, 5, 0);
    stream_chunk_of(&mut s, ANCHOR);
    s.tick(2);
    s
}

fn glowstone_left(s: &mut Scene) -> i32 {
    let alice = s.players[0].clone();
    match s.ask(Question::Inventory(alice.id)) {
        Answer::Inventory(inventory) => inventory
            .slots
            .into_iter()
            .flatten()
            .filter(|spec| spec.key == "minecraft:glowstone")
            .map(|spec| spec.count)
            .sum(),
        other => panic!("expected the inventory, got {other:?}"),
    }
}

fn hold_glowstone(s: &mut Scene, count: u8) {
    console(s, &format!("give Alice minecraft:glowstone {count}"));
    s.tick(1);
}

#[test]
fn glowstone_charges_an_anchor_one_step_and_is_consumed() {
    let mut s = rig("0");
    hold_glowstone(&mut s, 2);
    let alice = s.players[0].clone();
    use_block(&mut s, &alice, ANCHOR);
    assert_eq!(property_at(&s, ANCHOR, "charges").as_deref(), Some("1"));
    use_block(&mut s, &alice, ANCHOR);
    assert_eq!(property_at(&s, ANCHOR, "charges").as_deref(), Some("2"));
    assert_eq!(glowstone_left(&mut s), 0);
}

#[test]
fn a_full_anchor_held_against_glowstone_still_explodes_outside_the_nether() {
    let mut s = rig("4");
    hold_glowstone(&mut s, 1);
    let alice = s.players[0].clone();
    use_block(&mut s, &alice, ANCHOR);
    s.tick(2);
    assert_eq!(kind_at(&s, ANCHOR), Some(BlockKind::Air));
}

#[test]
fn an_empty_anchor_does_nothing_without_glowstone() {
    let mut s = rig("0");
    let alice = s.players[0].clone();
    use_block(&mut s, &alice, ANCHOR);
    assert_eq!(kind_at(&s, ANCHOR), Some(BlockKind::RespawnAnchor));
    assert_eq!(property_at(&s, ANCHOR, "charges").as_deref(), Some("0"));
}

#[test]
fn a_charged_anchor_used_outside_the_nether_explodes() {
    let mut s = rig("1");
    let alice = s.players[0].clone();
    use_block(&mut s, &alice, ANCHOR);
    s.tick(2);
    assert_eq!(kind_at(&s, ANCHOR), Some(BlockKind::Air));
    let crater = (-2..=2)
        .flat_map(|dx| (-2..=2).map(move |dz| BlockPos::new(8 + dx, 63, 10 + dz)))
        .filter(|pos| kind_at(&s, *pos) != Some(BlockKind::Stone))
        .count();
    assert!(crater > 0);
}
