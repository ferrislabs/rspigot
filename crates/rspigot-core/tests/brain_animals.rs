mod common;

use common::*;
use rspigot_core::events::Inbound;
use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn summon(s: &mut Scene, kind: Kind, at: &str, nbt: &str) -> EntityId {
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

fn seeded(seed: u64) -> Scene {
    scene_on(
        server(rspigot_core::config::PluginSettings::default()).with_rng_seed(seed),
        &["Alice"],
        Vec::new(),
    )
}

fn ready(mut s: Scene) -> Scene {
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn memories(s: &Scene, id: EntityId) -> Compound {
    s.server
        .saved_entity(id)
        .and_then(|nbt| nbt.get_compound("Brain").cloned())
        .and_then(|brain| brain.get_compound("memories").cloned())
        .unwrap_or_default()
}

fn int_memory(memories: &Compound, name: &str) -> Option<i32> {
    memories.get_compound(name)?.get_i32("value")
}

#[test]
fn a_summoned_goat_starts_with_both_cooldowns_and_a_regular_voice() {
    let mut s = ready(scene(&["Alice"]));
    let goat = summon(&mut s, Kind::Goat, "5.5 64 5.5", "");
    s.tick(1);
    let memories = memories(&s, goat);
    let jump = int_memory(&memories, "minecraft:long_jump_cooling_down").expect("a cooldown");
    let ram = int_memory(&memories, "minecraft:ram_cooldown_ticks").expect("a cooldown");
    assert!((590..=1200).contains(&jump), "{jump}");
    assert!((590..=6000).contains(&ram), "{ram}");
    let saved = s.server.saved_entity(goat).unwrap();
    assert!(saved.get_bool("HasLeftHorn").unwrap() || saved.get_bool("HasRightHorn").unwrap());
}

fn creative(mut s: Scene) -> Scene {
    console(&mut s, "gamemode creative Alice");
    s
}

fn health_of(s: &Scene, id: EntityId) -> f64 {
    s.server
        .saved_entity(id)
        .and_then(|nbt| nbt.get_f64("Health"))
        .expect("a health")
}

#[test]
fn a_screaming_goat_with_one_horn_round_trips_through_the_save() {
    let mut s = ready(scene(&["Alice"]));
    let goat = summon(
        &mut s,
        Kind::Goat,
        "5.5 64 5.5",
        "{IsScreamingGoat:1b,HasLeftHorn:0b,HasRightHorn:1b}",
    );
    s.tick(2);
    let saved = s.server.saved_entity(goat).unwrap();
    assert_eq!(saved.get_bool("IsScreamingGoat"), Some(true));
    assert_eq!(saved.get_bool("HasLeftHorn"), Some(false));
    assert_eq!(saved.get_bool("HasRightHorn"), Some(true));
}

#[test]
fn a_goat_rams_a_target_a_few_blocks_away_and_then_rests() {
    let mut s = creative(ready(seeded(3)));
    let goat = summon(
        &mut s,
        Kind::Goat,
        "10.5 64 10.5",
        "{Brain:{memories:{\"minecraft:long_jump_cooling_down\":{value:5000}}}}",
    );
    let victim = summon(&mut s, Kind::Cow, "16.5 64 10.5", "{NoAI:1b}");
    let before = health_of(&s, victim);
    s.tick(500);
    assert!(health_of(&s, victim) < before, "{}", health_of(&s, victim));
    let memories = memories(&s, goat);
    let rest = int_memory(&memories, "minecraft:ram_cooldown_ticks").expect("a rest");
    assert!((1..=6000).contains(&rest), "{rest}");
}

fn state_of(s: &Scene, id: EntityId) -> String {
    s.server
        .saved_entity(id)
        .and_then(|nbt| nbt.get_str("state").map(str::to_owned))
        .expect("a state")
}

#[test]
fn an_armadillo_saves_its_state_and_scute_timer() {
    let mut s = ready(scene(&["Alice"]));
    let armadillo = summon(
        &mut s,
        Kind::Armadillo,
        "5.5 64 5.5",
        "{state:\"scared\",scute_time:5000,Brain:{memories:{\"minecraft:danger_detected_recently\":{value:1b,ttl:200L}}}}",
    );
    s.tick(2);
    let saved = s.server.saved_entity(armadillo).unwrap();
    assert_eq!(saved.get_str("state"), Some("scared"));
    let left = saved.get_i32("scute_time").expect("a timer");
    assert!((4990..5000).contains(&left), "{left}");
}

#[test]
fn an_armadillo_rolls_up_near_a_husk_and_unrolls_once_the_danger_has_passed() {
    let mut s = creative(ready(scene(&["Alice"])));
    let armadillo = summon(&mut s, Kind::Armadillo, "10.5 64 10.5", "{}");
    let husk = summon(&mut s, Kind::Husk, "13.5 64 10.5", "{NoAI:1b}");
    s.tick(30);
    assert_ne!(state_of(&s, armadillo), "idle");
    let _ = husk;
    console(&mut s, "kill @e[type=husk]");
    s.tick(400);
    assert_eq!(state_of(&s, armadillo), "idle");
}

fn count_items(s: &Scene, kind: Kind) -> usize {
    s.server.entities_of(kind).len()
}

#[test]
fn an_armadillo_sheds_a_scute_when_its_timer_runs_out() {
    let mut s = ready(scene(&["Alice"]));
    summon(&mut s, Kind::Armadillo, "5.5 64 5.5", "{scute_time:3}");
    let before = count_items(&s, Kind::Item);
    s.tick(10);
    assert_eq!(count_items(&s, Kind::Item), before + 1);
}
