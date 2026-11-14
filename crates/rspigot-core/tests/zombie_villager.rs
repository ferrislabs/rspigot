mod common;

use common::*;
use rspigot_core::commands::Target;
use rspigot_core::events::{ClientEvent, Hand, Inbound, MetadataValue, PlayerInput};
use rspigot_domain::{BlockPos, EntityId, Vec3};
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
    let remaining = saved.get_i32("ConversionTime").expect("a timer");
    assert!((2300..=2398).contains(&remaining), "{remaining}");
    assert!(saved.get("ConversionPlayer").is_some());
}

const WEAK: &str =
    "{NoAI:1b,active_effects:[{id:\"minecraft:weakness\",duration:6000,amplifier:0b}]}";

fn feed(s: &mut Scene, target: EntityId) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target,
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::new(0.0, 0.5, 0.0),
        },
    );
}

fn with_apple(s: &mut Scene, count: i32) {
    let alice = s.players[0].clone();
    s.act(rspigot_core::plugins::Order::Inventory {
        player: alice.id,
        order: rspigot_core::plugins::InventoryOrder::Set {
            slot: 0,
            item: Some(rspigot_core::plugins::ItemSpec::of(
                "minecraft:golden_apple",
                count,
            )),
        },
    });
}

fn apples(s: &mut Scene) -> i32 {
    use rspigot_core::plugins::{Answer, Question};
    let Answer::Inventory(state) = s.ask(Question::Inventory(s.players[0].id)) else {
        panic!("expected Alice's inventory");
    };
    state
        .slots
        .iter()
        .flatten()
        .filter(|item| item.key == "minecraft:golden_apple")
        .map(|item| item.count)
        .sum()
}

fn effects(s: &Scene, id: EntityId) -> Vec<String> {
    s.server
        .saved_entity(id)
        .and_then(|nbt| nbt.get_list("active_effects").map(<[_]>::to_vec))
        .map(|list| {
            list.iter()
                .filter_map(|effect| effect.as_compound())
                .filter_map(|effect| effect.get_str("id").map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_golden_apple_on_a_weakened_zombie_villager_starts_the_cure_with_strength() {
    let mut s = ready();
    let id = summon(&mut s, "2.5 64 0.5", WEAK);
    with_apple(&mut s, 2);
    feed(&mut s, id);
    assert_eq!(apples(&mut s), 1);
    let nbt = s.server.saved_entity(id).unwrap();
    let time = nbt.get_i32("ConversionTime").unwrap();
    assert!((3600..=6000).contains(&time), "{time}");
    assert!(nbt.get("ConversionPlayer").is_some());
    let active = effects(&s, id);
    assert!(active.iter().any(|effect| effect == "minecraft:strength"));
    assert!(!active.iter().any(|effect| effect == "minecraft:weakness"));
    let events = s.players[0].link.take();
    assert!(events.iter().any(|event| matches!(
        event,
        ClientEvent::EntityStatus { entity, .. } if *entity == id
    )));
}

#[test]
fn a_golden_apple_is_kept_when_the_zombie_villager_is_not_weakened() {
    let mut s = ready();
    let id = summon(&mut s, "2.5 64 0.5", "{NoAI:1b}");
    with_apple(&mut s, 2);
    feed(&mut s, id);
    assert_eq!(apples(&mut s), 2);
    assert_eq!(
        s.server.saved_entity(id).unwrap().get_i32("ConversionTime"),
        Some(-1)
    );
}

#[test]
fn a_finished_cure_gives_back_the_villager_with_its_data_reputation_and_nausea() {
    let mut s = ready();
    let alice = s.players[0].id;
    let parts = alice.0.as_u128();
    let nbt = format!(
        "{{NoAI:1b,ConversionTime:3,ConversionPlayer:[I;{},{},{},{}],{}",
        (parts >> 96) as u32 as i32,
        (parts >> 64) as u32 as i32,
        (parts >> 32) as u32 as i32,
        parts as u32 as i32,
        &FULL[1..]
    );
    let zombie = summon(&mut s, "2.5 64 0.5", &nbt);
    s.tick(10);
    assert!(
        s.server.entities_of(Kind::ZombieVillager).is_empty(),
        "{zombie:?}"
    );
    let cured = s.server.entities_of(Kind::Villager);
    assert_eq!(cured.len(), 1);
    let saved = s.server.saved_entity(cured[0].id).unwrap();
    let data = saved.get_compound("VillagerData").unwrap();
    assert_eq!(data.get_str("profession"), Some("minecraft:farmer"));
    assert_eq!(data.get_str("type"), Some("minecraft:desert"));
    assert_eq!(data.get_i32("level"), Some(2));
    assert_eq!(saved.get_i32("Xp"), Some(30));
    let recipe = saved
        .get_compound("Offers")
        .and_then(|offers| offers.get_list("Recipes"))
        .and_then(|recipes| recipes.first())
        .and_then(|recipe| recipe.as_compound())
        .expect("the offers are carried");
    assert_eq!(recipe.get_i32("uses"), Some(3));
    let gossips = saved.get_list("Gossips").expect("gossips");
    let types: Vec<&str> = gossips
        .iter()
        .filter_map(|gossip| gossip.as_compound())
        .filter_map(|gossip| gossip.get_str("Type"))
        .collect();
    assert!(types.contains(&"major_positive"));
    assert!(types.contains(&"minor_positive"));
    assert!(
        effects(&s, cured[0].id)
            .iter()
            .any(|effect| effect == "minecraft:nausea")
    );
}

#[test]
fn a_zombie_that_kills_a_villager_on_hard_turns_it_into_a_zombie_villager_with_its_data() {
    let mut s = ready();
    console(&mut s, "difficulty hard");
    let villager = {
        console(
            &mut s,
            "summon minecraft:villager 4.5 64 0.5 {NoAI:1b,Health:1f,Xp:12,VillagerDataFinalized:1b,VillagerData:{type:\"minecraft:snow\",profession:\"minecraft:librarian\",level:3}}",
        );
        s.server.entities_of(Kind::Villager)[0].id
    };
    console(&mut s, "summon minecraft:zombie 5.5 64 0.5 {NoAI:1b}");
    let zombie = s.server.entities_of(Kind::Zombie)[0].id;
    assert!(s.server.do_hurt_target(zombie, Target::Entity(villager)));
    s.tick(2);
    assert!(s.server.entities_of(Kind::Villager).is_empty());
    let infected = s.server.entities_of(Kind::ZombieVillager);
    assert_eq!(infected.len(), 1);
    let saved = s.server.saved_entity(infected[0].id).unwrap();
    let data = saved.get_compound("VillagerData").unwrap();
    assert_eq!(data.get_str("profession"), Some("minecraft:librarian"));
    assert_eq!(data.get_str("type"), Some("minecraft:snow"));
    assert_eq!(data.get_i32("level"), Some(3));
    assert_eq!(saved.get_i32("Xp"), Some(12));
    assert!(saved.get_compound("Offers").is_some());
    assert!(saved.get("Gossips").is_some());
}

#[test]
fn on_easy_a_zombie_just_kills_the_villager() {
    let mut s = ready();
    console(&mut s, "difficulty easy");
    console(
        &mut s,
        "summon minecraft:villager 4.5 64 0.5 {NoAI:1b,Health:1f}",
    );
    let villager = s.server.entities_of(Kind::Villager)[0].id;
    console(&mut s, "summon minecraft:zombie 5.5 64 0.5 {NoAI:1b}");
    let zombie = s.server.entities_of(Kind::Zombie)[0].id;
    assert!(s.server.do_hurt_target(zombie, Target::Entity(villager)));
    s.tick(40);
    assert!(s.server.entities_of(Kind::ZombieVillager).is_empty());
    assert!(s.server.entities_of(Kind::Villager).is_empty());
}
