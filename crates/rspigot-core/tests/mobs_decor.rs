mod common;

use common::*;
use rspigot_core::events::{ClientEvent, EntityKind, Inbound, MetadataEntry, MetadataValue};
use rspigot_domain::entity::Pose;
use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::EntityKind as Kind;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn ready() -> Scene {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn summon(s: &mut Scene, kind: Kind, nbt: &str) -> EntityId {
    s.players[0].link.take();
    console(s, &format!("summon {} 2.5 64 2.5 {nbt}", kind.name()));
    s.tick(2);
    s.server
        .entities_of(kind)
        .first()
        .map(|entity| entity.id)
        .expect("the entity")
}

fn heard(s: &Scene) -> Vec<ClientEvent> {
    s.players[0].link.take()
}

fn spawned(events: &[ClientEvent], kind: Kind) -> bool {
    events.iter().any(|event| {
        matches!(event, ClientEvent::SpawnEntity { kind: wire, .. }
            if *wire == EntityKind::Other(kind.id().0))
    })
}

fn metadata_of(events: &[ClientEvent], id: EntityId) -> Vec<MetadataEntry> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::EntityMetadata { entity, entries } if *entity == id => {
                Some(entries.clone())
            }
            _ => None,
        })
        .flatten()
        .collect()
}

fn value(entries: &[MetadataEntry], index: u8) -> Option<&MetadataValue> {
    entries
        .iter()
        .find(|entry| entry.index == index)
        .map(|entry| &entry.value)
}

const MANNEQUIN: &str = "{pose:\"crouching\",immovable:1b,main_hand:\"left\",hidden_layers:[\"hat\"],description:\"Bob\",profile:{name:\"Notch\"}}";

#[test]
fn a_summoned_mannequin_is_shown_to_nearby_players_with_its_options() {
    let mut s = ready();
    let id = summon(&mut s, Kind::Mannequin, MANNEQUIN);
    let events = heard(&s);
    assert!(spawned(&events, Kind::Mannequin));
    let entries = metadata_of(&events, id);
    assert_eq!(
        value(&entries, 6),
        Some(&MetadataValue::Pose(Pose::Crouching))
    );
    assert_eq!(
        value(&entries, 15),
        Some(&MetadataValue::State(
            rspigot_core::events::StateKind::HumanoidArm,
            0
        ))
    );
    assert!(matches!(
        value(&entries, 17),
        Some(MetadataValue::Profile(_))
    ));
    assert_eq!(value(&entries, 16), Some(&MetadataValue::Byte(0x3F)));
    assert_eq!(value(&entries, 18), Some(&MetadataValue::Boolean(true)));
    assert!(matches!(
        value(&entries, 19),
        Some(MetadataValue::OptionalComponent(Some(_)))
    ));
}

#[test]
fn a_mannequin_keeps_its_options_and_profile_when_saved() {
    let mut s = ready();
    let id = summon(
        &mut s,
        Kind::Mannequin,
        "{pose:\"sleeping\",immovable:1b,hide_description:1b,profile:{name:\"Notch\"}}",
    );
    let saved = s.server.saved_entity(id).expect("saved");
    assert_eq!(saved.get_str("pose"), Some("sleeping"));
    assert_eq!(saved.get_bool("immovable"), Some(true));
    assert_eq!(saved.get_bool("hide_description"), Some(true));
    assert_eq!(
        saved
            .get_compound("profile")
            .and_then(|profile| profile.get_str("name")),
        Some("Notch")
    );
    assert_eq!(saved.get_f64("Health"), Some(20.0));
}

#[test]
fn a_mannequin_stands_still_without_ai() {
    let mut s = ready();
    let id = summon(&mut s, Kind::Mannequin, "{}");
    let before = s
        .server
        .entities_of(Kind::Mannequin)
        .into_iter()
        .find(|e| e.id == id)
        .map(|e| e.position)
        .expect("mannequin");
    s.tick(100);
    let after = s
        .server
        .entities_of(Kind::Mannequin)
        .into_iter()
        .find(|e| e.id == id)
        .map(|e| e.position)
        .expect("mannequin");
    assert!((after.x - before.x).abs() < 1.0e-9 && (after.z - before.z).abs() < 1.0e-9);
}

#[test]
fn a_mannequin_can_be_killed() {
    let mut s = ready();
    summon(&mut s, Kind::Mannequin, "{}");
    console(&mut s, "kill @e[type=minecraft:mannequin]");
    s.tick(40);
    assert!(s.server.entities_of(Kind::Mannequin).is_empty());
}

#[test]
fn a_text_display_is_shown_with_its_text_and_options() {
    let mut s = ready();
    let id = summon(
        &mut s,
        Kind::TextDisplay,
        "{text:\"Hello\",billboard:\"center\",line_width:120,shadow:1b,alignment:\"left\"}",
    );
    let events = heard(&s);
    assert!(spawned(&events, Kind::TextDisplay));
    let entries = metadata_of(&events, id);
    assert!(matches!(
        value(&entries, 23),
        Some(MetadataValue::Component(text)) if text.plain_text() == "Hello"
    ));
    assert_eq!(value(&entries, 15), Some(&MetadataValue::Byte(3)));
    assert_eq!(value(&entries, 24), Some(&MetadataValue::VarInt(120)));
    assert_eq!(value(&entries, 27), Some(&MetadataValue::Byte(1 | 8)));
}

#[test]
fn an_item_display_is_shown_with_its_item() {
    let mut s = ready();
    let id = summon(
        &mut s,
        Kind::ItemDisplay,
        "{item:{id:\"minecraft:apple\",count:1},item_display:\"head\"}",
    );
    let events = heard(&s);
    assert!(spawned(&events, Kind::ItemDisplay));
    let entries = metadata_of(&events, id);
    assert!(matches!(
        value(&entries, 23),
        Some(MetadataValue::Item(Some(_)))
    ));
    assert_eq!(value(&entries, 24), Some(&MetadataValue::Byte(5)));
}

#[test]
fn a_block_display_is_shown_and_keeps_its_block_state_when_saved() {
    let mut s = ready();
    let id = summon(
        &mut s,
        Kind::BlockDisplay,
        "{block_state:{Name:\"minecraft:stone\"},view_range:2.0f}",
    );
    assert!(spawned(&heard(&s), Kind::BlockDisplay));
    let saved = s.server.saved_entity(id).expect("saved");
    assert_eq!(
        saved
            .get_compound("block_state")
            .and_then(|state| state.get_str("Name")),
        Some("minecraft:stone")
    );
}

#[test]
fn a_display_transformation_and_block_state_reach_the_client() {
    let mut s = ready();
    let id = summon(
        &mut s,
        Kind::BlockDisplay,
        "{block_state:{Name:\"minecraft:stone\"},transformation:{translation:[1f,2f,3f],scale:[2f,2f,2f],left_rotation:[0f,0f,0f,1f],right_rotation:[0f,1f,0f,0f]}}",
    );
    let entries = metadata_of(&heard(&s), id);
    assert_eq!(
        value(&entries, 11),
        Some(&MetadataValue::Vector3([1.0, 2.0, 3.0]))
    );
    assert_eq!(value(&entries, 12), Some(&MetadataValue::Vector3([2.0; 3])));
    assert_eq!(
        value(&entries, 14),
        Some(&MetadataValue::Quaternion([0.0, 1.0, 0.0, 0.0]))
    );
    assert!(matches!(value(&entries, 23), Some(MetadataValue::BlockState(state)) if *state != 0));
}

#[test]
fn display_entities_keep_their_whole_data_when_saved() {
    let mut s = ready();
    let id = summon(
        &mut s,
        Kind::TextDisplay,
        "{text:\"Hi\",background:123,brightness:{sky:3,block:4}}",
    );
    let saved = s.server.saved_entity(id).expect("saved");
    assert_eq!(saved.get_i32("background"), Some(123));
    assert!(saved.get_compound("brightness").is_some());
}

#[test]
fn an_interaction_is_shown_and_remembers_who_used_it() {
    let mut s = ready();
    let id = summon(
        &mut s,
        Kind::Interaction,
        "{width:2.0f,height:3.0f,response:1b}",
    );
    let events = heard(&s);
    assert!(spawned(&events, Kind::Interaction));
    let entries = metadata_of(&events, id);
    assert_eq!(value(&entries, 8), Some(&MetadataValue::Float(2.0)));
    assert_eq!(value(&entries, 9), Some(&MetadataValue::Float(3.0)));
    assert_eq!(value(&entries, 10), Some(&MetadataValue::Boolean(true)));
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        rspigot_core::events::PlayerInput::Interact {
            target: id,
            hand: rspigot_core::events::Hand::Main,
            sneaking: false,
            location: rspigot_domain::Vec3::new(0.0, 0.5, 0.0),
        },
    );
    let saved = s.server.saved_entity(id).expect("saved");
    assert!(
        saved
            .get_compound("interaction")
            .is_some_and(|entry| entry.get_i64("timestamp").is_some())
    );
}

fn ground_items(s: &Scene, key: &str) -> usize {
    s.server
        .entities_of(Kind::Item)
        .into_iter()
        .filter_map(|item| s.server.saved_entity(item.id))
        .filter(|saved| {
            saved
                .get_compound("Item")
                .and_then(|stack| stack.get_str("id"))
                == Some(key)
        })
        .count()
}

#[test]
fn an_ominous_item_spawner_is_an_entity_that_drops_its_item_then_vanishes() {
    let mut s = ready();
    let id = summon(
        &mut s,
        Kind::OminousItemSpawner,
        "{item:{id:\"minecraft:apple\",count:1},spawn_item_after_ticks:40L}",
    );
    let events = heard(&s);
    assert!(spawned(&events, Kind::OminousItemSpawner));
    let entries = metadata_of(&events, id);
    assert!(matches!(
        value(&entries, 8),
        Some(MetadataValue::Item(Some(_)))
    ));
    assert_eq!(ground_items(&s, "minecraft:apple"), 0);
    s.tick(60);
    assert_eq!(ground_items(&s, "minecraft:apple"), 1);
    assert!(s.server.entities_of(Kind::OminousItemSpawner).is_empty());
}

#[test]
fn an_ominous_item_spawner_plays_its_warning_sound_before_dropping() {
    let mut s = ready();
    summon(
        &mut s,
        Kind::OminousItemSpawner,
        "{item:{id:\"minecraft:arrow\",count:1},spawn_item_after_ticks:60L}",
    );
    heard(&s);
    s.tick(20);
    let early = heard(&s);
    let warns = |events: &[ClientEvent]| {
        events.iter().any(|event| {
            matches!(event, ClientEvent::Sound { sound, .. }
                if sound.contains("about_to_spawn_item"))
        })
    };
    assert!(!warns(&early));
    s.tick(20);
    assert!(warns(&heard(&s)));
}

#[test]
fn hitting_an_interaction_records_the_attack_and_leaves_it_alone() {
    let mut s = ready();
    let id = summon(&mut s, Kind::Interaction, "{width:2.0f,height:2.0f}");
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        rspigot_core::events::PlayerInput::Attack(id),
    );
    let saved = s.server.saved_entity(id).expect("saved");
    assert!(
        saved
            .get_compound("attack")
            .is_some_and(|entry| entry.get_i64("timestamp").is_some())
    );
    assert_eq!(s.server.entities_of(Kind::Interaction).len(), 1);
}

#[test]
fn an_ominous_item_spawner_fires_a_projectile_item_downward() {
    let mut s = ready();
    summon(
        &mut s,
        Kind::OminousItemSpawner,
        "{item:{id:\"minecraft:arrow\",count:1},spawn_item_after_ticks:10L}",
    );
    s.tick(11);
    let arrows = s.server.entities_of(Kind::Arrow);
    assert_eq!(arrows.len(), 1);
    assert_eq!(ground_items(&s, "minecraft:arrow"), 0);
}
