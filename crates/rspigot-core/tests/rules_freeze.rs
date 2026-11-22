mod common;

use common::*;
use rspigot_core::events::{ClientEvent, Inbound};
use rspigot_domain::BlockPos;
use rspigot_domain::damage::DamageType;
use rspigot_entities::EntityKind as Kind;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn snowed_in(rule: bool) -> Scene {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    console(&mut s, &format!("gamerule freeze_damage {rule}"));
    console(&mut s, "setblock 0 64 0 minecraft:powder_snow");
    console(&mut s, "summon minecraft:cow 8.5 64 8.5 {NoAI:1b}");
    console(&mut s, "setblock 8 64 8 minecraft:powder_snow");
    s.players[0].link.take();
    s.tick(240);
    s
}

fn freeze_hits(s: &Scene) -> usize {
    let body = s.server.player_entity_id(s.players[0].id);
    s.players[0]
        .link
        .take()
        .iter()
        .filter(|e| {
            matches!(e, ClientEvent::DamageEvent { entity, kind: DamageType::Freeze, .. } if Some(*entity) == body)
        })
        .count()
}

#[test]
fn a_player_standing_in_powder_snow_is_hurt_by_the_cold_when_the_rule_is_true() {
    let s = snowed_in(true);
    assert!(freeze_hits(&s) >= 1);
}

#[test]
fn a_player_standing_in_powder_snow_is_spared_when_the_rule_is_false() {
    let s = snowed_in(false);
    assert_eq!(freeze_hits(&s), 0);
}

#[test]
fn the_rule_does_not_protect_mobs_from_the_cold() {
    let s = snowed_in(false);
    let cows = s.server.entities_of(Kind::Cow);
    assert!(cows.iter().all(|cow| cow.health.is_some_and(|h| h < 10.0)));
}
