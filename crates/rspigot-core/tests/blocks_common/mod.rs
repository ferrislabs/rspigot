#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::Inbound;
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::{BlockEntity, ChunkColumn};
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Identifier, Vec3};
use rspigot_nbt::Compound;

use super::{Link, NoStore, Scene, scene_on};

#[derive(Clone, Default)]
pub struct Cells {
    pub blocks: Vec<(BlockPos, BlockStateId)>,
    pub entities: Vec<(BlockPos, &'static str, Compound)>,
}

impl Cells {
    pub fn block(mut self, pos: BlockPos, kind: BlockKind) -> Self {
        self.blocks.push((pos, kind.default_state()));
        self
    }

    pub fn state(mut self, pos: BlockPos, kind: BlockKind, properties: &[(&str, &str)]) -> Self {
        self.blocks
            .push((pos, kind.state_with(properties).expect("a known state")));
        self
    }

    pub fn entity(mut self, pos: BlockPos, kind: &'static str, data: Compound) -> Self {
        self.entities.push((pos, kind, data));
        self
    }
}

impl ChunkGenerator for Cells {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(0));
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
        for (at, kind, data) in &self.entities {
            if at.chunk() == pos {
                chunk.set_block_entity(BlockEntity {
                    pos: *at,
                    kind: Identifier::minecraft(kind),
                    data: data.clone(),
                });
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }

    fn place(
        &self,
        dimension: &Dimension,
        request: &rspigot_core::ports::PlaceRequest,
    ) -> Result<Vec<rspigot_core::ports::PlacedBlock>, rspigot_core::ports::PlaceFailure> {
        rspigot_worldgen::WorldGenerator::from_level_type("void", 1, "").place(dimension, request)
    }
}

pub fn scene_of(cells: Cells, seed: u64, random_tick_speed: i32) -> Scene {
    let mut config = GameConfig {
        view_distance: 4,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    config.rules.random_tick_speed = random_tick_speed;
    let data = LevelData {
        spawn_mobs: false,
        day_time: 18_000,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let mut server: Server<Link> = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(cells),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    scene_on(server, &["Alice"], Vec::new())
}

pub fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

pub fn kind_at(s: &Scene, pos: BlockPos) -> Option<BlockKind> {
    s.server.block_state_at(pos).and_then(BlockStateId::kind)
}

pub fn property_at(s: &Scene, pos: BlockPos, name: &str) -> Option<String> {
    s.server
        .block_state_at(pos)
        .and_then(|state| state.property(name).map(str::to_owned))
}
