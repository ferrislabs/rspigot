mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::Inbound;
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Sea {
    biome: &'static str,
}

impl ChunkGenerator for Sea {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id(self.biome).unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        chunk.inhabited_time = 72_000_000;
        let wet = pos.x <= 0;
        for x in 0..16 {
            for z in 0..16 {
                let at = |y: i32| BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z);
                for y in 50..=55 {
                    chunk.set_block(at(y), BlockStateId(1));
                }
                if wet {
                    for y in 56..=62 {
                        chunk.set_block(at(y), BlockKind::Water.default_state());
                    }
                } else {
                    for y in 56..=62 {
                        chunk.set_block(at(y), BlockStateId(1));
                    }
                }
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn sea_server(seed: u64, biome: &'static str, spawn_mobs: bool) -> Server<Link> {
    let config = GameConfig {
        view_distance: 5,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    let data = LevelData {
        spawn_mobs,
        day_time: 6_000,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(Sea { biome }),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn ready(seed: u64) -> Scene {
    ready_with(seed, "minecraft:warm_ocean", false)
}

fn ready_with(seed: u64, biome: &'static str, spawn_mobs: bool) -> Scene {
    let mut s = scene_on(sea_server(seed, biome, spawn_mobs), &["Alice"], Vec::new());
    s.tick(120);
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

fn saved(s: &Scene, id: EntityId) -> rspigot_nbt::Compound {
    s.server.saved_entity(id).expect("a saved entity")
}

fn exists(s: &Scene, kind: Kind, id: EntityId) -> bool {
    s.server
        .entities_of(kind)
        .iter()
        .any(|entity| entity.id == id)
}

fn position_of(s: &Scene, kind: Kind, id: EntityId) -> Vec3 {
    s.server
        .entities_of(kind)
        .into_iter()
        .find(|entity| entity.id == id)
        .expect("the entity")
        .position
}

#[test]
fn a_cod_swims_about_inside_the_water_and_keeps_its_flags_across_a_save() {
    let mut s = ready(3);
    let cod = summon(&mut s, Kind::Cod, "-8.5 60 8.5", "{FromBucket:1b}");
    let start = position_of(&s, Kind::Cod, cod);
    let mut travelled = 0.0_f64;
    for _ in 0..8 {
        s.tick(25);
        let at = position_of(&s, Kind::Cod, cod);
        assert!(at.y > 55.9 && at.y < 63.2, "{at:?}");
        travelled = travelled.max(at.distance_squared(start));
    }
    assert!(travelled > 1.0, "{travelled}");
    let nbt = saved(&s, cod);
    assert_eq!(nbt.get_bool("FromBucket"), Some(true));
    assert_eq!(nbt.get_i32("Air"), Some(300));
}

#[test]
fn a_stranded_cod_flops_loses_air_and_drowns_in_a_few_hundred_ticks() {
    let mut s = ready(3);
    let cod = summon(&mut s, Kind::Cod, "24.5 64 8.5", "");
    s.tick(100);
    assert!(exists(&s, Kind::Cod, cod));
    let air = saved(&s, cod).get_i32("Air").unwrap();
    assert!(air < 300, "{air}");
    s.tick(400);
    assert!(!exists(&s, Kind::Cod, cod));
}

#[test]
fn a_bucketed_fish_never_despawns_but_a_wild_one_far_from_players_does_not_either_while_near() {
    let mut s = ready(5);
    let wild = summon(&mut s, Kind::Salmon, "-8.5 60 8.5", "");
    let kept = summon(&mut s, Kind::Salmon, "-6.5 60 8.5", "{FromBucket:1b}");
    s.tick(60);
    assert!(exists(&s, Kind::Salmon, wild));
    assert!(exists(&s, Kind::Salmon, kept));
}

#[test]
fn a_salmon_saves_its_size_and_the_matching_box() {
    let mut s = ready(3);
    let salmon = summon(&mut s, Kind::Salmon, "-8.5 60 8.5", "{type:\"large\"}");
    s.tick(2);
    assert_eq!(saved(&s, salmon).get_str("type"), Some("large"));
}

#[test]
fn a_tropical_fish_keeps_its_packed_variant() {
    let mut s = ready(3);
    let fish = summon(
        &mut s,
        Kind::TropicalFish,
        "-8.5 60 8.5",
        "{Variant:117899265}",
    );
    s.tick(2);
    assert_eq!(saved(&s, fish).get_i32("Variant"), Some(117_899_265));
}

#[test]
fn a_pufferfish_puffs_up_next_to_a_zombie_stings_it_and_poisons_it() {
    let mut s = ready(3);
    let puffer = summon(&mut s, Kind::Pufferfish, "-8.5 60 8.5", "");
    let zombie = summon(&mut s, Kind::Zombie, "-8.5 60 8.5", "{NoAI:1b,Health:20f}");
    let before = saved(&s, zombie).get_f64("Health").unwrap_or(20.0);
    s.tick(100);
    let puff = saved(&s, puffer).get_i32("PuffState").unwrap();
    assert!(puff >= 1, "{puff}");
    let after = saved(&s, zombie).get_f64("Health").unwrap_or(20.0);
    assert!(after < before, "{before} {after}");
    let effects = saved(&s, zombie);
    let poisoned = effects
        .get_list("active_effects")
        .is_some_and(|list| !list.is_empty());
    assert!(poisoned);
}

#[test]
fn a_pufferfish_ignores_a_cod_next_to_it() {
    let mut s = ready(3);
    let puffer = summon(&mut s, Kind::Pufferfish, "-8.5 60 8.5", "");
    let cod = summon(&mut s, Kind::Cod, "-8.5 60 8.5", "{NoAI:1b}");
    s.tick(100);
    assert_eq!(saved(&s, puffer).get_i32("PuffState"), Some(0));
    let health = saved(&s, cod).get_f64("Health").unwrap_or(0.0);
    assert!((health - 3.0).abs() < 1.0e-6, "{health}");
}

#[test]
fn a_squid_drifts_through_the_water_by_its_movement_vector() {
    let mut s = ready(3);
    let squid = summon(&mut s, Kind::Squid, "-8.5 60 8.5", "");
    let start = position_of(&s, Kind::Squid, squid);
    let mut travelled = 0.0_f64;
    for _ in 0..10 {
        s.tick(30);
        let at = position_of(&s, Kind::Squid, squid);
        assert!(at.y > 55.0 && at.y < 64.0, "{at:?}");
        travelled = travelled.max(at.distance_squared(start));
    }
    assert!(travelled > 0.25, "{travelled}");
}

#[test]
fn a_glow_squid_keeps_its_dark_ticks_and_a_baby_squid_stays_a_baby() {
    let mut s = ready(3);
    let glow = summon(
        &mut s,
        Kind::GlowSquid,
        "-8.5 60 8.5",
        "{DarkTicksRemaining:70}",
    );
    let baby = summon(&mut s, Kind::Squid, "-6.5 60 8.5", "{Age:-24000}");
    s.tick(10);
    let dark = saved(&s, glow).get_i32("DarkTicksRemaining").unwrap();
    assert!((50..=70).contains(&dark), "{dark}");
    assert_eq!(saved(&s, baby).get_i32("Age"), Some(-24_000 + 10));
}

#[test]
fn a_dolphin_uses_up_its_air_in_water_and_dries_out_on_land() {
    let mut s = ready(3);
    let dolphin = summon(&mut s, Kind::Dolphin, "-8.5 60 8.5", "");
    s.tick(60);
    let air = saved(&s, dolphin).get_i32("Air").unwrap();
    assert!(air < 4800 && air > 4500, "{air}");
    let stranded = summon(&mut s, Kind::Dolphin, "24.5 64 8.5", "{Moistness:5}");
    s.tick(100);
    let health = saved(&s, stranded).get_f64("Health").unwrap_or(0.0);
    assert!(health < 10.0, "{health}");
}

#[test]
fn a_dolphin_keeps_its_fish_and_moistness_across_a_save() {
    let mut s = ready(3);
    let dolphin = summon(
        &mut s,
        Kind::Dolphin,
        "-8.5 60 8.5",
        "{GotFish:1b,Moistness:1234}",
    );
    s.tick(2);
    let nbt = saved(&s, dolphin);
    assert_eq!(nbt.get_i32("Moistness"), Some(2400));
    assert!(nbt.get_bool("GotFish").is_some());
}

#[test]
fn water_creatures_appear_by_themselves_in_an_ocean() {
    let mut s = ready_with(11, "minecraft:warm_ocean", true);
    s.tick(2400);
    let kinds = [
        Kind::Cod,
        Kind::Salmon,
        Kind::Pufferfish,
        Kind::TropicalFish,
        Kind::Squid,
        Kind::Dolphin,
    ];
    let total: usize = kinds
        .iter()
        .map(|kind| s.server.entities_of(*kind).len())
        .sum();
    assert!(total > 0, "{total}");
}
