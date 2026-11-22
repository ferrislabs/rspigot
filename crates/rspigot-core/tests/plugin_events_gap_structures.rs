mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::plugins::events::AsyncStructureGenerateCause;
use rspigot_core::plugins::{EventKind, EventPayload};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, Vec3};
use rspigot_nbt::{Compound, Tag};

const START: ChunkPos = ChunkPos::new(12, 0);

struct Village;

impl ChunkGenerator for Village {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                chunk.set_block(
                    BlockPos::new(pos.min_block_x() + x, 63, pos.min_block_z() + z),
                    BlockKind::Stone.default_state(),
                );
            }
        }
        if pos == START {
            let piece = |bounds: [i32; 6]| {
                Tag::Compound(Compound::new().with("BB", Tag::IntArray(bounds.to_vec())))
            };
            let start = Compound::new()
                .with("ChunkX", START.x)
                .with("ChunkZ", START.z)
                .with(
                    "Children",
                    Tag::List(vec![
                        piece([194, 60, 3, 201, 70, 12]),
                        piece([197, 58, 1, 206, 66, 8]),
                    ]),
                );
            chunk.extra.insert(
                "structures",
                Compound::new().with(
                    "starts",
                    Compound::new().with("minecraft:village_plains", start),
                ),
            );
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn village() -> Scene {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    let data = LevelData {
        spawn_mobs: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(Village),
        StatusBoard::new(20),
    );
    server.wait_for_spawn(Duration::from_secs(10));
    let mut s = scene_on(
        server,
        &["Alice"],
        vec![
            observe(1, EventKind::AsyncStructureGenerate),
            observe(2, EventKind::AsyncStructureSpawn),
        ],
    );
    s.server.handle(rspigot_core::events::Inbound::Console(
        "tp Alice 200 64 8".to_owned(),
    ));
    s.server.deliver();
    s.server.wait_for_spawn(Duration::from_secs(10));
    s.tick(2);
    s
}

#[test]
fn a_generated_chunk_with_a_structure_start_fires_generate_once() {
    let s = village();
    let events = settled_for(&s.plugin, EventKind::AsyncStructureGenerate);
    assert_eq!(events.len(), 1);
    let EventPayload::AsyncStructureGenerate(event) = &events[0].payload else {
        panic!("expected an AsyncStructureGenerate");
    };
    assert_eq!(event.structure, "minecraft:village_plains");
    assert_eq!((event.chunk_x, event.chunk_z), (12, 0));
    assert_eq!(event.cause, AsyncStructureGenerateCause::WorldGeneration);
    assert_eq!(event.bounding_box.min, Vec3::new(194.0, 58.0, 1.0));
    assert_eq!(event.bounding_box.max, Vec3::new(207.0, 71.0, 13.0));
}

#[test]
fn the_structure_spawn_is_asked_with_the_same_box() {
    let s = village();
    let events = settled_for(&s.plugin, EventKind::AsyncStructureSpawn);
    assert_eq!(events.len(), 1);
    let EventPayload::AsyncStructureSpawn(event) = &events[0].payload else {
        panic!("expected an AsyncStructureSpawn");
    };
    assert_eq!(event.structure, "minecraft:village_plains");
    assert_eq!(event.bounding_box.max, Vec3::new(207.0, 71.0, 13.0));
}
