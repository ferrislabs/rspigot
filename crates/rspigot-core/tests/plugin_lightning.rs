mod common;

use common::*;
use rspigot_core::events::Inbound;
use rspigot_core::plugins::events::{CreeperPowerCause, LightningStrikeCause};
use rspigot_core::plugins::{EventKind, EventPayload};
use rspigot_domain::EntityId;
use rspigot_entities::EntityKind as Kind;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn summoned(s: &mut Scene, line: &str, kind: Kind) -> EntityId {
    let before: Vec<EntityId> = s.server.entities_of(kind).iter().map(|e| e.id).collect();
    console(s, line);
    s.server
        .entities_of(kind)
        .iter()
        .map(|e| e.id)
        .find(|id| !before.contains(id))
        .expect("a new entity")
}

fn powered(s: &Scene, id: EntityId) -> bool {
    s.server
        .saved_entity(id)
        .and_then(|nbt| nbt.get_bool("powered"))
        .unwrap_or(false)
}

#[test]
fn summoning_a_bolt_fires_the_lightning_strike_with_the_command_cause_and_a_cancel_spawns_none() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            observe(1, EventKind::LightningStrike),
            deciding(2, EventKind::LightningStrike),
        ],
    );
    console(&mut s, "summon lightning_bolt 1.5 64 0.5");
    let asked = last_asked(&s.plugin, EventKind::LightningStrike);
    let EventPayload::LightningStrike(event) = &asked.payload else {
        panic!("expected a lightning strike");
    };
    assert_eq!(event.cause, LightningStrikeCause::Command);
    assert_eq!(event.lightning.kind, "minecraft:lightning_bolt");
    judge(&mut s.server, 1, asked.id, 2, true, None);
    assert!(s.server.entities_of(Kind::LightningBolt).is_empty());
    console(&mut s, "summon lightning_bolt 1.5 64 0.5");
    let asked = last_asked(&s.plugin, EventKind::LightningStrike);
    judge(&mut s.server, 1, asked.id, 2, false, None);
    assert_eq!(s.server.entities_of(Kind::LightningBolt).len(), 1);
}

#[test]
fn a_bolt_strikes_for_a_few_ticks_then_goes_out_and_hurts_what_stands_under_it() {
    let mut s = scene(&["Alice"]);
    let cow = summoned(&mut s, "summon cow 1.5 64 0.5 {NoAI:1b}", Kind::Cow);
    let before = s.server.entities_of(Kind::Cow)[0].health.unwrap();
    console(&mut s, "summon lightning_bolt 1.5 64 0.5");
    assert_eq!(s.server.entities_of(Kind::LightningBolt).len(), 1);
    s.tick(2);
    let after = s
        .server
        .entities_of(Kind::Cow)
        .iter()
        .find(|cow_now| cow_now.id == cow)
        .and_then(|cow_now| cow_now.health);
    assert!(after.is_none_or(|health| health < before), "{after:?}");
    s.tick(60);
    assert!(s.server.entities_of(Kind::LightningBolt).is_empty());
}

#[test]
fn a_cancelled_power_event_leaves_the_creeper_uncharged() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::CreeperPower)]);
    let creeper = summoned(&mut s, "summon creeper 1.5 64 0.5 {NoAI:1b}", Kind::Creeper);
    console(&mut s, "summon lightning_bolt 1.5 64 0.5");
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::CreeperPower);
    let EventPayload::CreeperPower(event) = &asked.payload else {
        panic!("expected a power event");
    };
    assert_eq!(event.cause, CreeperPowerCause::Lightning);
    assert_eq!(event.entity.id, creeper);
    assert!(event.lightning.is_some());
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(!powered(&s, creeper));
}

#[test]
fn an_allowed_power_event_charges_the_creeper() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::CreeperPower)]);
    let creeper = summoned(&mut s, "summon creeper 1.5 64 0.5 {NoAI:1b}", Kind::Creeper);
    console(&mut s, "summon lightning_bolt 1.5 64 0.5");
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::CreeperPower);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert!(powered(&s, creeper));
}

#[test]
fn a_creeper_under_a_bolt_is_charged_when_nobody_listens() {
    let mut s = scene(&["Alice"]);
    let creeper = summoned(&mut s, "summon creeper 1.5 64 0.5 {NoAI:1b}", Kind::Creeper);
    console(&mut s, "summon lightning_bolt 1.5 64 0.5");
    s.tick(1);
    assert!(powered(&s, creeper));
}

#[test]
fn a_cancelled_zap_event_keeps_the_pig() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PigZap)]);
    let pig = summoned(&mut s, "summon pig 1.5 64 0.5 {NoAI:1b}", Kind::Pig);
    console(&mut s, "summon lightning_bolt 1.5 64 0.5");
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::PigZap);
    let EventPayload::PigZap(event) = &asked.payload else {
        panic!("expected a zap event");
    };
    assert_eq!(event.entity.id, pig);
    assert_eq!(event.transformed_entity.kind, "minecraft:zombified_piglin");
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(s.server.entities_of(Kind::Pig).len(), 1);
    assert!(s.server.entities_of(Kind::ZombifiedPiglin).is_empty());
}

#[test]
fn an_allowed_zap_event_turns_the_pig_into_a_zombified_piglin() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PigZap)]);
    summoned(&mut s, "summon pig 1.5 64 0.5 {NoAI:1b}", Kind::Pig);
    console(&mut s, "summon lightning_bolt 1.5 64 0.5");
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::PigZap);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert!(s.server.entities_of(Kind::Pig).is_empty());
    assert_eq!(s.server.entities_of(Kind::ZombifiedPiglin).len(), 1);
}

#[test]
fn a_pig_under_a_bolt_turns_into_a_zombified_piglin_when_nobody_listens() {
    let mut s = scene(&["Alice"]);
    summoned(&mut s, "summon pig 1.5 64 0.5 {NoAI:1b}", Kind::Pig);
    console(&mut s, "summon lightning_bolt 1.5 64 0.5");
    s.tick(1);
    assert_eq!(s.server.entities_of(Kind::ZombifiedPiglin).len(), 1);
}
