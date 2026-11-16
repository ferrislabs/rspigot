#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::plugins::HandlerSpec;
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::{BlockEntity, ChunkColumn};
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Identifier, Vec3};
use rspigot_nbt::{Compound, Tag};

use super::screen::{close_window, open_block};
use super::{Link, NoStore, Player, Scene, scene_on};

#[derive(Debug, Clone)]
pub struct Seed {
    pub pos: BlockPos,
    pub kind: BlockKind,
    pub properties: Vec<(&'static str, &'static str)>,
    pub items: Vec<Option<(&'static str, u8)>>,
    pub data: Compound,
}

impl Seed {
    pub fn block(pos: BlockPos, kind: BlockKind) -> Self {
        Self {
            pos,
            kind,
            properties: Vec::new(),
            items: Vec::new(),
            data: Compound::new(),
        }
    }

    pub fn carrying(mut self, key: &'static str, value: impl Into<Tag>) -> Self {
        self.data.insert(key, value);
        self
    }

    fn block_entity_name(&self) -> &str {
        match self.kind {
            BlockKind::Spawner => "mob_spawner",
            kind => kind.name().trim_start_matches("minecraft:"),
        }
    }

    pub fn with(mut self, name: &'static str, value: &'static str) -> Self {
        self.properties.push((name, value));
        self
    }

    pub fn holding(mut self, key: &'static str, count: u8) -> Self {
        self.items.push(Some((key, count)));
        self
    }

    pub fn gap(mut self) -> Self {
        self.items.push(None);
        self
    }
}

struct Seeded(Vec<Seed>);

impl ChunkGenerator for Seeded {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(0));
        for x in 0..16 {
            for z in 0..16 {
                for y in 60..=63 {
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        BlockStateId(1),
                    );
                }
            }
        }
        for seed in self.0.iter().filter(|seed| seed.pos.chunk() == pos) {
            let mut state = seed.kind.default_state();
            for (name, value) in &seed.properties {
                state = state.with_property(name, value).expect("a known property");
            }
            chunk.set_block(seed.pos, state);
            let list = seed
                .items
                .iter()
                .enumerate()
                .filter_map(|(slot, item)| item.map(|item| (slot, item)))
                .map(|(slot, (key, count))| {
                    Tag::Compound(
                        Compound::new()
                            .with("Slot", slot as i8)
                            .with("id", key)
                            .with("count", i32::from(count)),
                    )
                })
                .collect();
            let mut data = Compound::new().with("Items", Tag::List(list));
            for (key, value) in seed.data.iter() {
                data.insert(key, value.clone());
            }
            chunk.set_block_entity(BlockEntity {
                pos: seed.pos,
                kind: Identifier::minecraft(seed.block_entity_name()),
                data,
            });
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

pub fn world(seeds: Vec<Seed>, handlers: Vec<HandlerSpec>) -> Scene {
    world_spawning(seeds, handlers, false)
}

pub fn world_spawning(seeds: Vec<Seed>, handlers: Vec<HandlerSpec>, spawn_mobs: bool) -> Scene {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    let level = LevelData {
        spawn_mobs,
        ..LevelData::default()
    };
    let mut server: Server<Link> = Server::new(
        config,
        level,
        Arc::new(NoStore),
        Arc::new(Seeded(seeds)),
        StatusBoard::new(20),
    );
    server.wait_for_spawn(Duration::from_secs(10));
    scene_on(server, &["Alice"], handlers)
}

pub fn contents(
    s: &mut Scene,
    player: &Player,
    pos: BlockPos,
    size: usize,
) -> Vec<Option<(&'static str, u8)>> {
    let mut view = open_block(s, player, pos);
    let shown = (0..size).map(|slot| view.at(slot)).collect();
    close_window(s, player, &mut view);
    shown
}
