mod common;

use common::*;
use rspigot_core::plugins::events::{ExplosionPrimePatch, TNTPrimeCause};
use rspigot_core::plugins::{
    Answer, BlockOrder, EventKind, EventPatch, EventPayload, Order, Question,
};
use rspigot_domain::BlockPos;

const TNT: BlockPos = BlockPos::new(3, 64, 3);
const POWER: BlockPos = BlockPos::new(3, 64, 4);

fn put(s: &mut Scene, pos: BlockPos, kind: &str) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::Set(kind.to_owned()),
    });
}

fn kind_at(s: &mut Scene, pos: BlockPos) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state.kind,
        other => panic!("expected a block, got {other:?}"),
    }
}

fn primed(s: &mut Scene) -> usize {
    match s.ask(Question::Entities {
        near: None,
        kind: Some("minecraft:tnt".to_owned()),
    }) {
        Answer::Entities(entities) => entities.len(),
        other => panic!("expected entities, got {other:?}"),
    }
}

#[test]
fn a_powered_tnt_block_becomes_a_primed_tnt() {
    let mut s = scene(&["Alice"]);
    put(&mut s, TNT, "minecraft:tnt");
    put(&mut s, POWER, "minecraft:redstone_block");
    assert_eq!(kind_at(&mut s, TNT), "minecraft:air");
    assert_eq!(primed(&mut s), 1);
}

#[test]
fn a_primed_tnt_explodes_after_four_seconds_and_breaks_the_blocks_around_it() {
    let mut s = scene(&["Alice"]);
    put(&mut s, BlockPos::new(3, 64, 2), "minecraft:dirt");
    put(&mut s, TNT, "minecraft:tnt");
    put(&mut s, POWER, "minecraft:redstone_block");
    s.tick(60);
    assert_eq!(primed(&mut s), 1);
    s.tick(30);
    assert_eq!(primed(&mut s), 0);
    assert_eq!(kind_at(&mut s, BlockPos::new(3, 64, 2)), "minecraft:air");
}

#[test]
fn a_tnt_prime_event_names_the_cause_and_the_block_that_powered_it() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::TNTPrime)]);
    put(&mut s, TNT, "minecraft:tnt");
    put(&mut s, POWER, "minecraft:redstone_block");
    let heard = settled_for(&s.plugin, EventKind::TNTPrime);
    let EventPayload::TNTPrime(event) = &heard.last().expect("a prime").payload else {
        panic!("a prime event");
    };
    assert_eq!(event.block, TNT);
    assert_eq!(event.cause, TNTPrimeCause::Redstone);
    assert_eq!(event.priming_block, Some(POWER));
    assert!(event.priming_entity.is_none());
}

#[test]
fn a_cancelled_prime_keeps_the_tnt_block() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::TNTPrime)]);
    put(&mut s, TNT, "minecraft:tnt");
    put(&mut s, POWER, "minecraft:redstone_block");
    let asked = last_asked(&s.plugin, EventKind::TNTPrime);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(kind_at(&mut s, TNT), "minecraft:tnt");
    assert_eq!(primed(&mut s), 0);
}

#[test]
fn a_cancelled_explosion_prime_removes_the_tnt_without_a_blast() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::ExplosionPrime)]);
    put(&mut s, BlockPos::new(3, 64, 2), "minecraft:dirt");
    put(&mut s, TNT, "minecraft:tnt");
    put(&mut s, POWER, "minecraft:redstone_block");
    s.tick(75);
    while asked_for(&s.plugin, EventKind::ExplosionPrime).is_empty() {
        s.tick(1);
    }
    let asked = last_asked(&s.plugin, EventKind::ExplosionPrime);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(primed(&mut s), 0);
    assert_eq!(kind_at(&mut s, BlockPos::new(3, 64, 2)), "minecraft:dirt");
}

#[test]
fn a_patched_radius_changes_the_size_of_the_blast() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::ExplosionPrime)]);
    put(&mut s, BlockPos::new(3, 64, 2), "minecraft:dirt");
    put(&mut s, TNT, "minecraft:tnt");
    put(&mut s, POWER, "minecraft:redstone_block");
    s.tick(75);
    while asked_for(&s.plugin, EventKind::ExplosionPrime).is_empty() {
        s.tick(1);
    }
    let asked = last_asked(&s.plugin, EventKind::ExplosionPrime);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::ExplosionPrime(ExplosionPrimePatch {
            radius: Some(0.0),
            fire: None,
        })),
    );
    assert_eq!(kind_at(&mut s, BlockPos::new(3, 64, 2)), "minecraft:dirt");
}

#[test]
fn a_tnt_block_caught_in_a_blast_is_primed_again_with_a_short_fuse() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::TNTPrime)]);
    put(&mut s, BlockPos::new(2, 64, 3), "minecraft:tnt");
    put(&mut s, TNT, "minecraft:tnt");
    put(&mut s, POWER, "minecraft:redstone_block");
    s.tick(90);
    let causes: Vec<TNTPrimeCause> = settled_for(&s.plugin, EventKind::TNTPrime)
        .iter()
        .filter_map(|fired| match &fired.payload {
            EventPayload::TNTPrime(event) => Some(event.cause),
            _ => None,
        })
        .collect();
    assert!(causes.contains(&TNTPrimeCause::Redstone));
    assert!(causes.contains(&TNTPrimeCause::Explosion));
}

#[test]
fn the_tnt_explodes_rule_off_keeps_a_powered_tnt_block_unprimed() {
    let mut s = scene_on(
        configured(|config| config.rules.tnt_explodes = false),
        &["Alice"],
        Vec::new(),
    );
    put(&mut s, TNT, "minecraft:tnt");
    put(&mut s, POWER, "minecraft:redstone_block");
    assert_eq!(kind_at(&mut s, TNT), "minecraft:tnt");
    assert_eq!(primed(&mut s), 0);
}

#[test]
fn a_tnt_blast_ignores_mob_griefing_and_follows_its_own_rules() {
    let mut s = scene_on(
        configured(|config| config.rules.mob_griefing = false),
        &["Alice"],
        Vec::new(),
    );
    put(&mut s, BlockPos::new(3, 64, 2), "minecraft:dirt");
    put(&mut s, TNT, "minecraft:tnt");
    put(&mut s, POWER, "minecraft:redstone_block");
    s.tick(90);
    assert_eq!(kind_at(&mut s, BlockPos::new(3, 64, 2)), "minecraft:air");
}

fn primed_after_the_fuse(limit: i32) -> usize {
    let mut s = scene_on(
        configured(|config| config.max_tnt_per_tick = limit),
        &["Alice"],
        Vec::new(),
    );
    let far = BlockPos::new(9, 64, 3);
    put(&mut s, TNT, "minecraft:tnt");
    put(&mut s, far, "minecraft:tnt");
    put(&mut s, POWER, "minecraft:redstone_block");
    put(&mut s, far.offset(0, 0, 1), "minecraft:redstone_block");
    s.tick(85);
    primed(&mut s)
}

#[test]
fn the_tnt_per_tick_limit_delays_the_blasts_beyond_it() {
    assert_eq!(primed_after_the_fuse(100), 0);
    assert_eq!(primed_after_the_fuse(1), 1);
}
