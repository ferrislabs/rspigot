mod common;

use common::*;
use rspigot_core::events::{ClientEvent, Hand, Inbound, PlayerInput};
use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;

const HARNESS: &str = "equipment:{body:{id:\"minecraft:blue_harness\",count:1}}";

fn world() -> Scene {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s.tick(60);
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn summon_at(s: &mut Scene, kind: Kind, at: &str, nbt: &str) -> EntityId {
    let known: Vec<EntityId> = s
        .server
        .entities_of(kind)
        .into_iter()
        .map(|entity| entity.id)
        .collect();
    console(s, &format!("summon {} {at} {nbt}", kind.name()));
    s.server
        .entities_of(kind)
        .into_iter()
        .map(|entity| entity.id)
        .find(|id| !known.contains(id))
        .expect("a new entity")
}

fn saved(s: &Scene, id: EntityId) -> Compound {
    s.server.saved_entity(id).expect("a saved mob")
}

fn interact_with(s: &mut Scene, target: EntityId, sneaking: bool) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target,
            hand: Hand::Main,
            sneaking,
            location: rspigot_domain::Vec3::new(0.0, 0.5, 0.0),
        },
    );
    s.tick(2);
}

fn body_id(nbt: &Compound) -> Option<String> {
    nbt.get_compound("equipment")?
        .get_compound("body")?
        .get_str("id")
        .map(str::to_owned)
}

fn passengers(nbt: &Compound) -> usize {
    nbt.get_list("Passengers").map_or(0, <[_]>::len)
}

fn sounds(events: &[ClientEvent]) -> Vec<&'static str> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Sound { sound, .. } => Some(*sound),
            _ => None,
        })
        .collect()
}

#[test]
fn a_happy_ghast_keeps_its_harness_and_still_timeout_in_the_vanilla_layout() {
    let mut s = world();
    let ghast = summon_at(
        &mut s,
        Kind::HappyGhast,
        "3.5 70 0.5",
        &format!("{{NoAI:1b,still_timeout:0,{HARNESS}}}"),
    );
    let nbt = saved(&s, ghast);
    assert_eq!(
        body_id(&nbt).as_deref(),
        Some("minecraft:blue_harness"),
        "{nbt:?}"
    );
    assert_eq!(nbt.get_i32("still_timeout"), Some(0));
}

#[test]
fn a_harness_fits_an_adult_with_the_equip_sound_and_never_a_ghastling() {
    let mut s = world();
    let ghast = summon_at(&mut s, Kind::HappyGhast, "3.5 66 0.5", "{NoAI:1b}");
    console(&mut s, "give Alice minecraft:blue_harness 2");
    s.players[0].link.take();
    interact_with(&mut s, ghast, true);
    let events = s.players[0].link.take();
    assert_eq!(
        body_id(&saved(&s, ghast)).as_deref(),
        Some("minecraft:blue_harness")
    );
    assert!(
        sounds(&events).contains(&"minecraft:entity.happy_ghast.equip"),
        "{events:?}"
    );
    let baby = summon_at(
        &mut s,
        Kind::HappyGhast,
        "3.5 66 6.5",
        "{NoAI:1b,Age:-24000}",
    );
    interact_with(&mut s, baby, true);
    assert_eq!(body_id(&saved(&s, baby)), None);
}

#[test]
fn a_harnessed_ghast_carries_the_player_who_uses_it_unless_sneaking() {
    let mut s = world();
    let ghast = summon_at(
        &mut s,
        Kind::HappyGhast,
        "3.5 66 0.5",
        &format!("{{NoAI:1b,{HARNESS}}}"),
    );
    s.players[0].link.take();
    interact_with(&mut s, ghast, true);
    assert_eq!(passengers(&saved(&s, ghast)), 0);
    interact_with(&mut s, ghast, false);
    let events = s.players[0].link.take();
    assert!(
        events.iter().any(|event| matches!(event, ClientEvent::Passengers { vehicle, passengers } if *vehicle == ghast && passengers.len() == 1)),
        "{events:?}"
    );
    assert!(sounds(&events).contains(&"minecraft:entity.happy_ghast.harness_goggles_down"));
}

#[test]
fn a_bare_adult_cannot_be_ridden() {
    let mut s = world();
    let ghast = summon_at(&mut s, Kind::HappyGhast, "3.5 66 0.5", "{NoAI:1b}");
    s.players[0].link.take();
    interact_with(&mut s, ghast, false);
    let events = s.players[0].link.take();
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, ClientEvent::Passengers { .. })),
        "{events:?}"
    );
}

#[test]
fn shears_take_the_harness_off_with_the_unequip_sound() {
    let mut s = world();
    let ghast = summon_at(
        &mut s,
        Kind::HappyGhast,
        "3.5 66 0.5",
        &format!("{{NoAI:1b,{HARNESS}}}"),
    );
    console(&mut s, "give Alice minecraft:shears 1");
    s.players[0].link.take();
    interact_with(&mut s, ghast, false);
    let events = s.players[0].link.take();
    assert_eq!(body_id(&saved(&s, ghast)), None);
    assert!(
        sounds(&events).contains(&"minecraft:entity.happy_ghast.unequip"),
        "{events:?}"
    );
}

#[test]
fn a_player_standing_on_the_back_makes_the_ghast_stay_still() {
    let mut s = world();
    let ghast = summon_at(&mut s, Kind::HappyGhast, "3.5 66 0.5", "{}");
    s.tick(5);
    assert_eq!(saved(&s, ghast).get_i32("still_timeout"), Some(0));
    console(&mut s, "tp Alice 3.5 70.2 0.5");
    s.tick(3);
    let timeout = saved(&s, ghast)
        .get_i32("still_timeout")
        .expect("a timeout");
    assert!(timeout > 0, "{timeout}");
}

#[test]
fn a_ghast_picks_a_home_radius_by_harness_and_age() {
    let mut s = world();
    let bare = summon_at(&mut s, Kind::HappyGhast, "3.5 66 0.5", "{}");
    let harnessed = summon_at(
        &mut s,
        Kind::HappyGhast,
        "10.5 66 0.5",
        &format!("{{{HARNESS}}}"),
    );
    let baby = summon_at(&mut s, Kind::HappyGhast, "17.5 66 0.5", "{Age:-24000}");
    s.tick(5);
    assert_eq!(saved(&s, bare).get_i32("home_radius"), Some(64));
    assert_eq!(saved(&s, harnessed).get_i32("home_radius"), Some(32));
    assert_eq!(saved(&s, baby).get_i32("home_radius"), Some(32));
}

#[test]
fn a_hurt_ghast_slowly_heals() {
    let mut s = world();
    let ghast = summon_at(
        &mut s,
        Kind::HappyGhast,
        "3.5 66 0.5",
        "{NoAI:1b,Health:10f}",
    );
    s.tick(700);
    let health = saved(&s, ghast).get_f64("Health").expect("a health");
    assert!(health > 10.0, "{health}");
}
