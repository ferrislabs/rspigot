mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{ClientEvent, Inbound};
use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Tag;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn seeded(seed: u64) -> Scene {
    scene_on(
        server(PluginSettings::default()).with_rng_seed(seed),
        &["Alice"],
        Vec::new(),
    )
}

fn ready(mut s: Scene) -> Scene {
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn only(s: &Scene, kind: Kind) -> EntityId {
    let found = s.server.entities_of(kind);
    assert_eq!(found.len(), 1, "one {kind:?}");
    found[0].id
}

const JOCKEY: &str = "{Passengers:[{id:\"minecraft:zombie\",IsBaby:1b}]}";

fn passenger_lists(events: &[ClientEvent]) -> Vec<(EntityId, Vec<EntityId>)> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Passengers {
                vehicle,
                passengers,
            } => Some((*vehicle, passengers.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_summoned_stack_loads_its_passengers_onto_the_vehicle_and_tells_the_clients() {
    let mut s = ready(scene(&["Alice"]));
    s.players[0].link.take();
    console(
        &mut s,
        &format!("summon minecraft:chicken 5.5 64 5.5 {JOCKEY}"),
    );
    let chicken = only(&s, Kind::Chicken);
    let zombie = only(&s, Kind::Zombie);
    s.tick(1);
    let lists = passenger_lists(&s.players[0].link.take());
    assert!(lists.contains(&(chicken, vec![zombie])), "{lists:?}");
    let chicken_at = s.server.entities_of(Kind::Chicken)[0].position;
    let zombie_at = s.server.entities_of(Kind::Zombie)[0].position;
    assert!((zombie_at.x - chicken_at.x).abs() < 1e-6);
    assert!(
        (zombie_at.y - (chicken_at.y + 0.7 - 0.1875)).abs() < 1e-3,
        "{zombie_at:?} {chicken_at:?}"
    );
    assert!(
        (zombie_at.z - (chicken_at.z - 0.1)).abs() < 1e-3,
        "{zombie_at:?} {chicken_at:?}"
    );
}

#[test]
fn a_rider_follows_its_vehicle_and_is_saved_inside_it() {
    let mut s = ready(scene(&["Alice"]));
    console(
        &mut s,
        &format!("summon minecraft:chicken 5.5 64 5.5 {JOCKEY}"),
    );
    let chicken = only(&s, Kind::Chicken);
    let zombie = only(&s, Kind::Zombie);
    console(&mut s, "tp @e[type=chicken] 8.5 64 8.5");
    s.tick(2);
    let chicken_at = s.server.entities_of(Kind::Chicken)[0].position;
    let zombie_at = s.server.entities_of(Kind::Zombie)[0].position;
    assert!((zombie_at.x - chicken_at.x).abs() < 1e-6);
    let saved = s.server.saved_entity(chicken).unwrap();
    let Some(Tag::List(riders)) = saved.get("Passengers") else {
        panic!("passengers are saved in the vehicle");
    };
    assert_eq!(riders.len(), 1);
    assert!(format!("{riders:?}").contains("minecraft:zombie"));
    let alone = s.server.saved_entity(zombie).unwrap();
    assert!(alone.get("Passengers").is_none());
}

#[test]
fn killing_the_vehicle_drops_the_rider_beside_it_with_a_boarding_cooldown() {
    let mut s = ready(scene(&["Alice"]));
    s.players[0].link.take();
    console(
        &mut s,
        &format!("summon minecraft:chicken 5.5 64 5.5 {JOCKEY}"),
    );
    let chicken = only(&s, Kind::Chicken);
    let zombie = only(&s, Kind::Zombie);
    s.tick(1);
    s.players[0].link.take();
    console(&mut s, "kill @e[type=chicken]");
    s.tick(25);
    assert!(s.server.entities_of(Kind::Chicken).is_empty());
    let zombie_at = s.server.entities_of(Kind::Zombie)[0].position;
    assert!(zombie_at.y >= 63.9 && zombie_at.y < 66.0, "{zombie_at:?}");
    let lists = passenger_lists(&s.players[0].link.take());
    assert!(lists.contains(&(chicken, Vec::new())), "{lists:?}");
    assert_eq!(s.server.entities_of(Kind::Zombie)[0].id, zombie);
}

#[test]
fn some_spiders_spawn_with_a_skeleton_riding_them() {
    let mut s = ready(seeded(11));
    for _ in 0..700 {
        console(&mut s, "summon minecraft:spider 5.5 64 5.5");
    }
    s.tick(2);
    let skeletons = s.server.entities_of(Kind::Skeleton);
    assert!(!skeletons.is_empty(), "a skeleton jockey spawned");
    let carried = s
        .server
        .entities_of(Kind::Spider)
        .into_iter()
        .filter(|spider| {
            s.server
                .saved_entity(spider.id)
                .is_some_and(|nbt| nbt.get("Passengers").is_some())
        })
        .count();
    assert_eq!(carried, skeletons.len());
}

#[test]
fn some_baby_zombies_spawn_as_chicken_jockeys() {
    let mut s = ready(seeded(5));
    for _ in 0..1800 {
        console(&mut s, "summon minecraft:zombie 5.5 64 5.5");
    }
    s.tick(2);
    let chickens = s.server.entities_of(Kind::Chicken);
    assert!(!chickens.is_empty(), "a chicken jockey spawned");
    let jockeys = chickens
        .iter()
        .filter(|chicken| {
            s.server
                .saved_entity(chicken.id)
                .is_some_and(|nbt| nbt.get("Passengers").is_some())
        })
        .count();
    assert!(jockeys > 0, "a zombie rides a chicken");
}
