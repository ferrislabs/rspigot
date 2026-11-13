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

struct Meadow;

impl ChunkGenerator for Meadow {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                let at = |y: i32| BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z);
                for y in 60..=62 {
                    chunk.set_block(at(y), BlockStateId(1));
                }
                chunk.set_block(at(63), BlockKind::GrassBlock.default_state());
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn meadow_server(seed: u64) -> Server<Link> {
    let config = GameConfig {
        view_distance: 5,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    let data = LevelData {
        spawn_mobs: false,
        day_time: 6_000,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(Meadow),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn ready(seed: u64) -> Scene {
    let mut s = scene_on(meadow_server(seed), &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(8, 64, 8));
    s.tick(40);
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
fn a_vex_floats_without_gravity_and_holds_an_iron_sword() {
    let mut s = ready(3);
    let vex = summon(&mut s, Kind::Vex, "8.5 80 8.5", "");
    s.tick(60);
    let at = position_of(&s, Kind::Vex, vex);
    assert!(at.y > 74.0 && at.y < 90.0, "{at:?}");
    assert!((at.x - 8.5).abs() < 20.0 && (at.z - 8.5).abs() < 20.0);
    let nbt = saved(&s, vex);
    let main = nbt
        .get_compound("equipment")
        .and_then(|equipment| equipment.get_compound("mainhand"));
    assert_eq!(
        main.and_then(|stack| stack.get_str("id")),
        Some("minecraft:iron_sword")
    );
}

#[test]
fn a_vex_with_a_limited_life_starves_when_it_runs_out() {
    let mut s = ready(3);
    let vex = summon(
        &mut s,
        Kind::Vex,
        "8.5 80 8.5",
        "{NoAI:1b,Health:3f,life_ticks:5}",
    );
    assert!(exists(&s, Kind::Vex, vex));
    s.tick(25);
    let health = s
        .server
        .entities_of(Kind::Vex)
        .into_iter()
        .find(|entity| entity.id == vex)
        .and_then(|entity| entity.health);
    assert!(health.is_some_and(|health| health < 3.0), "{health:?}");
    s.tick(60);
    assert!(!exists(&s, Kind::Vex, vex));
}

#[test]
fn a_vex_keeps_its_bound_position_life_and_no_gravity_across_a_save() {
    let mut s = ready(3);
    let vex = summon(
        &mut s,
        Kind::Vex,
        "8.5 80 8.5",
        "{NoAI:1b,bound_pos:[I;4,70,4],life_ticks:900}",
    );
    let nbt = saved(&s, vex);
    assert_eq!(nbt.get_int_array("bound_pos"), Some(&[4, 70, 4][..]));
    assert_eq!(nbt.get_i32("life_ticks"), Some(900));
}
