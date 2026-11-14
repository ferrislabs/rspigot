mod common;

use common::*;
use rspigot_core::events::{ClientEvent, Hand, Inbound, InputState, MovementAction, PlayerInput};
use rspigot_core::plugins::EventKind;
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;

const SADDLE: &str = "equipment:{saddle:{id:\"minecraft:saddle\",count:1}}";

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

fn interact(s: &mut Scene, target: EntityId, sneaking: bool) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target,
            hand: Hand::Main,
            sneaking,
            location: Vec3::new(0.0, 0.5, 0.0),
        },
    );
    s.tick(2);
}

fn pose_tick(s: &Scene, id: EntityId) -> i64 {
    saved(s, id).get_i64("LastPoseTick").expect("a pose tick")
}

#[test]
fn a_camel_summons_always_tame_and_persists_its_pose_clock() {
    let mut s = world();
    let camel = summon_at(&mut s, Kind::Camel, "3.5 64 0.5", "{LastPoseTick:-100L}");
    let nbt = saved(&s, camel);
    assert_eq!(nbt.get_bool("Tame"), Some(true));
    assert_eq!(nbt.get_i64("LastPoseTick"), Some(-100));
    assert!(nbt.get_i32("Temper").is_some());
    let husk = summon_at(&mut s, Kind::CamelHusk, "6.5 64 0.5", "{}");
    assert_eq!(saved(&s, husk).get_bool("Tame"), Some(true));
}

#[test]
fn a_tempted_sitting_camel_stands_up_to_follow_a_player_holding_a_cactus() {
    let mut s = world();
    let camel = summon_at(&mut s, Kind::Camel, "6.5 64 0.5", "{LastPoseTick:-1L}");
    assert!(pose_tick(&s, camel) < 0);
    console(&mut s, "give Alice minecraft:cactus 1");
    s.tick(160);
    assert!(pose_tick(&s, camel) > 0, "the camel stood up");
}

#[test]
fn a_player_rides_a_camel_by_hand_and_a_sneaking_one_opens_its_inventory() {
    let mut s = world();
    let camel = summon_at(&mut s, Kind::Camel, "2.5 64 0.5", "{NoAI:1b}");
    s.players[0].link.take();
    interact(&mut s, camel, false);
    let events = s.players[0].link.take();
    assert!(
        events.iter().any(|event| matches!(event, ClientEvent::Passengers { vehicle, passengers } if *vehicle == camel && passengers.len() == 1)),
        "{events:?}"
    );
    let other = summon_at(&mut s, Kind::Camel, "2.5 64 4.5", "{NoAI:1b}");
    console(&mut s, "tp Alice 2.5 64 3.5");
    s.tick(2);
    interact(&mut s, other, true);
    let events = s.players[0].link.take();
    assert!(
        events.iter().any(|event| matches!(event, ClientEvent::OpenMountScreen { entity, columns: 0, .. } if *entity == other)),
        "{events:?}"
    );
}

#[test]
fn a_cactus_heals_a_camel_and_a_saddle_fits_it_with_the_camel_sound() {
    let mut s = world();
    let camel = summon_at(&mut s, Kind::Camel, "2.5 64 0.5", "{NoAI:1b,Health:10f}");
    console(&mut s, "give Alice minecraft:cactus 2");
    interact(&mut s, camel, false);
    let health = saved(&s, camel).get_f64("Health").expect("a health");
    assert!(health > 11.5, "{health}");
    console(&mut s, "clear Alice");
    console(&mut s, "give Alice minecraft:saddle 1");
    interact(&mut s, camel, false);
    assert!(format!("{:?}", saved(&s, camel)).contains("saddle"));
}

#[test]
fn a_husk_camel_eats_a_rabbit_foot_and_refuses_to_breed() {
    let mut s = world();
    let husk = summon_at(
        &mut s,
        Kind::CamelHusk,
        "2.5 64 0.5",
        "{NoAI:1b,Health:10f}",
    );
    console(&mut s, "give Alice minecraft:rabbit_foot 1");
    interact(&mut s, husk, false);
    let health = saved(&s, husk).get_f64("Health").expect("a health");
    assert!(health > 11.5, "{health}");
    assert!(
        saved(&s, husk)
            .get_i32("InLove")
            .is_none_or(|love| love == 0)
    );
}

#[test]
fn a_saddled_rider_dashes_with_a_horse_jump_event_and_the_camel_dashes() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::HorseJump)]);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s.tick(60);
    let camel = summon_at(
        &mut s,
        Kind::Camel,
        "3.5 64 0.5",
        &format!("{{NoAI:1b,{SADDLE}}}"),
    );
    interact(&mut s, camel, false);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Movement(MovementAction::StartRidingJump(45)),
    );
    s.tick(2);
    assert_eq!(settled_for(&s.plugin, EventKind::HorseJump).len(), 1);
    s.players[0].link.take();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Input(InputState {
            shift: true,
            ..Default::default()
        }),
    );
    s.tick(2);
}

#[test]
fn two_camels_in_love_breed_a_calf_and_a_pair_of_riders_fit_on_one_camel() {
    let mut s = world();
    summon_at(&mut s, Kind::Camel, "3.5 64 0.5", "{InLove:600}");
    summon_at(&mut s, Kind::Camel, "5.5 64 0.5", "{InLove:600}");
    s.tick(400);
    let camels = s.server.entities_of(Kind::Camel);
    assert!(camels.len() >= 3, "a calf was born: {}", camels.len());
}
