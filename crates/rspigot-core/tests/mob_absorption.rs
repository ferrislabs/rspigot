mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{Inbound, PlayerInput};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::EntityId;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Meadow;

impl ChunkGenerator for Meadow {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                for y in 60..=63 {
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        BlockKind::Stone.default_state(),
                    );
                }
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn meadow() -> Scene {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    let data = LevelData {
        spawn_mobs: false,
        day_time: 18_000,
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
    .with_rng_seed(7);
    server.wait_for_spawn(Duration::from_secs(10));
    let mut s = scene_on(server, &["Alice"], Vec::new());
    s.tick(30);
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn zombie(s: &mut Scene) -> EntityId {
    console(
        s,
        "summon minecraft:zombie 8.5 64 10.5 {NoAI:1b,Silent:1b,PersistenceRequired:1b}",
    );
    s.tick(2);
    s.server.entities_of(Kind::Zombie)[0].id
}

fn absorption(s: &Scene, id: EntityId) -> f64 {
    s.server
        .saved_entity(id)
        .and_then(|nbt| nbt.get("AbsorptionAmount").and_then(|tag| tag.as_f64()))
        .unwrap_or(-1.0)
}

#[test]
fn the_absorption_effect_gives_a_mob_absorption_hearts() {
    let mut s = meadow();
    let id = zombie(&mut s);
    assert_eq!(absorption(&s, id), 0.0);
    console(
        &mut s,
        "effect give @e[type=zombie] minecraft:absorption 60 1",
    );
    s.tick(1);
    assert_eq!(absorption(&s, id), 8.0);
}

#[test]
fn a_stronger_absorption_effect_adds_more_hearts() {
    let mut s = meadow();
    let id = zombie(&mut s);
    console(
        &mut s,
        "effect give @e[type=zombie] minecraft:absorption 60 0",
    );
    s.tick(1);
    assert_eq!(absorption(&s, id), 4.0);
}

#[test]
fn damage_drains_the_absorption_before_the_mobs_health() {
    let mut s = meadow();
    let id = zombie(&mut s);
    console(
        &mut s,
        "effect give @e[type=zombie] minecraft:absorption 60 1",
    );
    s.tick(1);
    let before = s.server.entities_of(Kind::Zombie)[0].health.unwrap();
    let alice = s.players[0].clone();
    send_input(&mut s.server, &alice, PlayerInput::Attack(id));
    s.tick(1);
    let after = s.server.entities_of(Kind::Zombie)[0].health.unwrap();
    assert_eq!(after, before);
    assert!(absorption(&s, id) < 8.0);
}

#[test]
fn a_mob_without_the_effect_keeps_no_absorption_and_loses_health() {
    let mut s = meadow();
    let id = zombie(&mut s);
    let before = s.server.entities_of(Kind::Zombie)[0].health.unwrap();
    let alice = s.players[0].clone();
    send_input(&mut s.server, &alice, PlayerInput::Attack(id));
    s.tick(1);
    assert!(s.server.entities_of(Kind::Zombie)[0].health.unwrap() < before);
    assert_eq!(absorption(&s, id), 0.0);
}

#[test]
fn clearing_the_effect_takes_the_absorption_hearts_away() {
    let mut s = meadow();
    let id = zombie(&mut s);
    console(
        &mut s,
        "effect give @e[type=zombie] minecraft:absorption 60 1",
    );
    s.tick(1);
    console(&mut s, "effect clear @e[type=zombie] minecraft:absorption");
    s.tick(2);
    assert_eq!(absorption(&s, id), 0.0);
}
