mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::Inbound;
use rspigot_core::plugins::events::LightningStrikeCause;
use rspigot_core::plugins::{EventKind, EventPayload};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::{Difficulty, Dimension};
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Meadow {
    blocks: Vec<(BlockPos, BlockStateId)>,
}

impl ChunkGenerator for Meadow {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        chunk.inhabited_time = 72_000_000;
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
        for (at, state) in &self.blocks {
            if at.chunk() == pos {
                chunk.set_block(*at, *state);
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn storm(chance: i32, spawn_mobs: bool, blocks: Vec<(BlockPos, BlockStateId)>) -> Scene {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        difficulty: Difficulty::Hard,
        thunder_chance: chance,
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
        Arc::new(Meadow { blocks }),
        StatusBoard::new(20),
    )
    .with_rng_seed(7);
    server.wait_for_spawn(Duration::from_secs(10));
    scene_on(
        server,
        &["Alice"],
        vec![
            observe(1, EventKind::LightningStrike),
            deciding(2, EventKind::LightningStrike),
        ],
    )
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn bolt_positions_over(s: &mut Scene, ticks: u32) -> Vec<Vec3> {
    let mut seen: Vec<Vec3> = Vec::new();
    for _ in 0..ticks {
        s.tick(1);
        for bolt in s.server.entities_of(Kind::LightningBolt) {
            if !seen.contains(&bolt.position) {
                seen.push(bolt.position);
            }
        }
    }
    seen
}

#[test]
fn a_thunderstorm_strikes_a_bolt_with_the_weather_cause() {
    let mut s = storm(1, false, Vec::new());
    console(&mut s, "weather thunder");
    assert!(!bolt_positions_over(&mut s, 40).is_empty());
    let asked = last_asked(&s.plugin, EventKind::LightningStrike);
    let EventPayload::LightningStrike(event) = &asked.payload else {
        panic!("expected a lightning strike");
    };
    assert_eq!(event.cause, LightningStrikeCause::Weather);
}

#[test]
fn clear_and_rainy_weather_never_strike() {
    let mut s = storm(1, false, Vec::new());
    assert!(bolt_positions_over(&mut s, 40).is_empty());
    console(&mut s, "weather rain");
    assert!(bolt_positions_over(&mut s, 40).is_empty());
}

#[test]
fn a_disabled_chance_never_strikes() {
    let mut s = storm(0, false, Vec::new());
    console(&mut s, "weather thunder");
    assert!(bolt_positions_over(&mut s, 40).is_empty());
}

#[test]
fn every_storm_bolt_is_drawn_to_a_lightning_rod_within_reach() {
    let rod = BlockPos::new(8, 64, 8);
    let mut s = storm(
        1,
        false,
        vec![(rod, BlockKind::LightningRod.default_state())],
    );
    console(&mut s, "weather thunder");
    let seen = bolt_positions_over(&mut s, 40);
    assert!(!seen.is_empty());
    assert!(
        seen.iter()
            .all(|at| at.x == 8.5 && at.y == 65.0 && at.z == 8.5),
        "{seen:?}"
    );
}

#[test]
fn a_hard_storm_sometimes_brings_a_skeleton_trap_under_a_visual_bolt() {
    let mut s = storm(1, true, Vec::new());
    console(&mut s, "weather thunder");
    bolt_positions_over(&mut s, 200);
    assert!(!s.server.entities_of(Kind::SkeletonHorse).is_empty());
}
