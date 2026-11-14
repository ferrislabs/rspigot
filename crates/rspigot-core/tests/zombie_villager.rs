mod common;

use common::*;
use rspigot_core::events::{ClientEvent, Inbound, MetadataValue};
use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::EntityKind as Kind;

fn console(s: &mut Scene, line: &str) {
    s.server
        .handle(Inbound::Console(line.trim_end().to_owned()));
    s.server.deliver();
}

fn ready() -> Scene {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s.players[0].link.take();
    s
}

fn summon(s: &mut Scene, at: &str, nbt: &str) -> EntityId {
    console(s, &format!("summon minecraft:zombie_villager {at} {nbt}"));
    s.server
        .entities_of(Kind::ZombieVillager)
        .into_iter()
        .map(|entity| entity.id)
        .max()
        .expect("a zombie villager")
}

fn villager_data(events: &[ClientEvent], id: EntityId) -> Option<(u32, u32, i32)> {
    events.iter().rev().find_map(|event| match event {
        ClientEvent::EntityMetadata { entity, entries } if *entity == id => {
            entries.iter().find_map(|entry| match entry.value {
                MetadataValue::VillagerData {
                    kind,
                    profession,
                    level,
                } if entry.index == 20 => Some((kind, profession, level)),
                _ => None,
            })
        }
        _ => None,
    })
}

const FULL: &str = "{VillagerData:{type:\"minecraft:desert\",profession:\"minecraft:farmer\",level:2},\
VillagerDataFinalized:1b,Xp:30,\
Offers:{Recipes:[{uses:3,maxUses:12,buy:{id:\"minecraft:wheat\",count:20},sell:{id:\"minecraft:emerald\",count:1}}]},\
Gossips:[{Type:\"major_positive\",Value:5,Target:[I;1,2,3,4]}]}";

#[test]
fn villager_data_reaches_the_clients_and_the_save_keeps_everything() {
    let mut s = ready();
    let id = summon(&mut s, "1.5 64 1.5", FULL);
    s.tick(2);
    let events = s.players[0].link.take();
    assert_eq!(villager_data(&events, id), Some((0, 5, 2)));
    let saved = s.server.saved_entity(id).expect("saved");
    let data = saved.get_compound("VillagerData").expect("villager data");
    assert_eq!(data.get_str("type"), Some("minecraft:desert"));
    assert_eq!(data.get_str("profession"), Some("minecraft:farmer"));
    assert_eq!(data.get_i32("level"), Some(2));
    assert_eq!(saved.get_i32("Xp"), Some(30));
    assert_eq!(saved.get_i32("ConversionTime"), Some(-1));
    assert!(saved.get("Gossips").is_some());
    let offers = format!("{:?}", saved.get_compound("Offers"));
    assert!(offers.contains("minecraft:emerald"), "{offers}");
    assert!(offers.contains("maxUses"), "{offers}");
}

#[test]
fn a_fresh_zombie_villager_gets_the_biome_type_and_a_profession() {
    let mut s = ready();
    let id = summon(&mut s, "1.5 64 1.5", "");
    s.tick(2);
    let saved = s.server.saved_entity(id).expect("saved");
    assert_eq!(saved.get_bool("VillagerDataFinalized"), Some(true));
    let data = saved.get_compound("VillagerData").unwrap();
    assert_eq!(data.get_str("type"), Some("minecraft:desert"));
    assert!(data.get_str("profession").is_some());
    assert_eq!(saved.get_i32("Xp"), Some(0));
}

#[test]
fn a_zombie_villager_hunts_players_like_a_zombie() {
    let mut s = ready();
    let id = summon(&mut s, "7.5 64 0.5", "{}");
    let alice = s.server.player_entity_id(s.players[0].id).unwrap();
    s.tick(120);
    assert_eq!(s.server.mob_target(id), Some(alice));
}

#[test]
fn a_cure_in_progress_is_kept_in_the_save_for_the_villager_support_to_resume() {
    let mut s = ready();
    let nbt = "{ConversionTime:2400,ConversionPlayer:[I;7,8,9,10]}";
    let id = summon(&mut s, "1.5 64 1.5", nbt);
    s.tick(2);
    let events = s.players[0].link.take();
    let flagged = events.iter().any(|event| match event {
        ClientEvent::EntityMetadata { entity, entries } if *entity == id => entries
            .iter()
            .any(|entry| entry.index == 19 && entry.value == MetadataValue::Boolean(true)),
        _ => false,
    });
    assert!(flagged, "the converting flag is synchronised");
    let saved = s.server.saved_entity(id).expect("saved");
    assert_eq!(saved.get_i32("ConversionTime"), Some(2400));
    assert!(saved.get("ConversionPlayer").is_some());
}
