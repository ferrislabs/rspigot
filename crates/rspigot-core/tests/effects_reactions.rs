mod common;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::events::{ClientEvent, Inbound};
use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::EntityKind as Kind;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn settled() -> Scene {
    let mut s = scene(&["Alice"]);
    s.tick(60);
    s
}

fn afflicted(s: &mut Scene, kind: &str, effect: &str) -> EntityId {
    console(
        s,
        &format!("summon minecraft:{kind} 5.5 64 5.5 {{NoAI:1b}}"),
    );
    let id = s.server.entities_of(Kind::from_name(kind).unwrap())[0].id;
    console(
        s,
        &format!("effect give @e[type={kind}] minecraft:{effect} 600"),
    );
    id
}

fn count(s: &Scene, kind: Kind) -> usize {
    s.server.entities_of(kind).len()
}

fn cobwebs_around(s: &Scene) -> usize {
    let mut found = 0;
    for x in 4..=7 {
        for y in 63..=66 {
            for z in 4..=7 {
                let state = s.server.block_at(BlockPos::new(x, y, z));
                if state.and_then(|state| state.kind()) == Some(BlockKind::Cobweb) {
                    found += 1;
                }
            }
        }
    }
    found
}

#[test]
fn an_oozing_mob_that_dies_leaves_slimes() {
    let mut s = settled();
    afflicted(&mut s, "cow", "oozing");
    assert_eq!(count(&s, Kind::Slime), 0);
    console(&mut s, "kill @e[type=cow]");
    s.tick(30);
    assert!(count(&s, Kind::Slime) >= 1);
}

#[test]
fn a_mob_without_oozing_leaves_no_slimes() {
    let mut s = settled();
    console(&mut s, "summon minecraft:cow 5.5 64 5.5 {NoAI:1b}");
    console(&mut s, "kill @e[type=cow]");
    s.tick(30);
    assert_eq!(count(&s, Kind::Slime), 0);
}

#[test]
fn a_weaving_mob_that_dies_spins_cobwebs() {
    let mut s = settled();
    afflicted(&mut s, "cow", "weaving");
    assert_eq!(cobwebs_around(&s), 0);
    console(&mut s, "kill @e[type=cow]");
    s.tick(30);
    assert!(cobwebs_around(&s) >= 1);
}

#[test]
fn a_wind_charged_mob_that_dies_bursts() {
    let mut s = settled();
    afflicted(&mut s, "cow", "wind_charged");
    s.players[0].link.take();
    console(&mut s, "kill @e[type=cow]");
    s.tick(30);
    let burst = s.players[0]
        .link
        .take()
        .iter()
        .any(|event| matches!(event, ClientEvent::Explode { gust: Some(_), .. }));
    assert!(burst);
}

#[test]
fn an_infested_mob_sometimes_releases_silverfish_when_hurt() {
    let mut s = settled();
    afflicted(&mut s, "iron_golem", "infested");
    for _ in 0..120 {
        console(&mut s, "damage @e[type=iron_golem] 1 minecraft:generic");
        s.tick(12);
        if count(&s, Kind::Silverfish) > 0 {
            break;
        }
    }
    assert!(count(&s, Kind::Silverfish) >= 1);
}

#[test]
fn a_mob_that_is_not_infested_releases_no_silverfish() {
    let mut s = settled();
    console(&mut s, "summon minecraft:iron_golem 5.5 64 5.5 {NoAI:1b}");
    for _ in 0..40 {
        console(&mut s, "damage @e[type=iron_golem] 1 minecraft:generic");
        s.tick(12);
    }
    assert_eq!(count(&s, Kind::Silverfish), 0);
}

#[test]
fn breath_of_the_nautilus_keeps_a_stranded_fish_breathing() {
    let mut s = settled();
    console(&mut s, "summon minecraft:cod 5.5 64 5.5 {NoAI:1b}");
    let cod = s.server.entities_of(Kind::Cod)[0].id;
    let air = |s: &Scene| {
        s.server
            .saved_entity(cod)
            .and_then(|n| n.get_i64("Air"))
            .unwrap()
    };
    s.tick(20);
    assert!(air(&s) < 300);
    console(
        &mut s,
        "effect give @e[type=cod] minecraft:breath_of_the_nautilus 600",
    );
    s.tick(5);
    assert_eq!(air(&s), 300);
    s.tick(60);
    assert_eq!(air(&s), 300);
}

#[test]
fn an_infested_player_sometimes_releases_silverfish_when_hurt() {
    let mut s = settled();
    console(&mut s, "effect give Alice minecraft:infested 600");
    console(&mut s, "effect give Alice minecraft:regeneration 600 3");
    for _ in 0..150 {
        console(&mut s, "damage Alice 1 minecraft:generic");
        s.tick(12);
        if count(&s, Kind::Silverfish) > 0 {
            break;
        }
    }
    assert!(count(&s, Kind::Silverfish) >= 1);
}
