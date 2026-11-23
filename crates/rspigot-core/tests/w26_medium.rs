mod common;

use common::*;
use rspigot_core::events::Inbound;
use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::EntityKind as Kind;

fn ready() -> Scene {
    let server = configured_level(|_| {}, |level| level.day_time = 18_000).with_rng_seed(5);
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s.tick(5);
    s
}

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

const EFFECT_CREEPER: &str = "{NoAI:1b,Silent:1b,ignited:1b,Fuse:2s,active_effects:[{id:\"minecraft:speed\",amplifier:1b,duration:600}]}";

#[test]
fn a_creeper_with_effects_leaves_a_lingering_cloud_when_it_explodes() {
    let mut s = ready();
    summon(&mut s, Kind::Creeper, "8.5 64 8.5", EFFECT_CREEPER);
    s.tick(10);
    assert!(s.server.entities_of(Kind::Creeper).is_empty());
    let clouds = s.server.entities_of(Kind::AreaEffectCloud);
    assert_eq!(clouds.len(), 1);
    let nbt = s.server.saved_entity(clouds[0].id).unwrap();
    assert_eq!(nbt.get_i32("Duration"), Some(300));
    assert_eq!(
        nbt.get_f64("Radius").map(|radius| (radius * 10.0).round()),
        Some(25.0)
    );
    assert_eq!(nbt.get_f64("potion_duration_scale"), Some(0.25));
    assert!(
        nbt.get_compound("potion_contents")
            .and_then(|contents| contents.get_list("custom_effects"))
            .is_some_and(|list| list.iter().any(|effect| effect
                .as_compound()
                .and_then(|effect| effect.get_str("id"))
                == Some("minecraft:speed"))),
        "{nbt:?}"
    );
}

#[test]
fn a_plain_creeper_leaves_no_cloud() {
    let mut s = ready();
    summon(
        &mut s,
        Kind::Creeper,
        "8.5 64 8.5",
        "{NoAI:1b,Silent:1b,ignited:1b,Fuse:2s}",
    );
    s.tick(10);
    assert!(s.server.entities_of(Kind::Creeper).is_empty());
    assert!(s.server.entities_of(Kind::AreaEffectCloud).is_empty());
}

fn air_of(s: &Scene, id: EntityId) -> i32 {
    s.server
        .saved_entity(id)
        .and_then(|nbt| nbt.get_i32("Air"))
        .unwrap()
}

const WATER_POTION: &str = "{Item:{id:\"minecraft:splash_potion\",count:1,components:{\"minecraft:potion_contents\":{potion:\"minecraft:water\"}}}}";

#[test]
fn a_splash_water_bottle_rehydrates_the_axolotls_around_it() {
    let mut s = ready();
    let axolotl = summon(
        &mut s,
        Kind::Axolotl,
        "8.5 64 8.5",
        "{NoAI:1b,Silent:1b,Air:100}",
    );
    summon(&mut s, Kind::SplashPotion, "9.5 64.5 8.5", WATER_POTION);
    s.tick(5);
    assert!(air_of(&s, axolotl) > 1500, "{}", air_of(&s, axolotl));
}

#[test]
fn a_splash_water_bottle_far_away_leaves_an_axolotl_dry() {
    let mut s = ready();
    let axolotl = summon(
        &mut s,
        Kind::Axolotl,
        "8.5 64 8.5",
        "{NoAI:1b,Silent:1b,Air:100}",
    );
    summon(&mut s, Kind::SplashPotion, "20.5 64.5 8.5", WATER_POTION);
    s.tick(5);
    assert!(air_of(&s, axolotl) < 1000, "{}", air_of(&s, axolotl));
}

#[test]
fn rehydration_never_passes_the_axolotl_maximum() {
    let mut s = ready();
    let axolotl = summon(
        &mut s,
        Kind::Axolotl,
        "8.5 64 8.5",
        "{NoAI:1b,Silent:1b,Air:5900}",
    );
    summon(&mut s, Kind::SplashPotion, "9.5 64.5 8.5", WATER_POTION);
    s.tick(5);
    assert!(air_of(&s, axolotl) <= 6000);
}
