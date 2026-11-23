mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::ports::{ChunkGenerator, PlaceFailure, PlaceRequest, PlacedBlock};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Vec3};
use rspigot_worldgen::WorldGenerator;

const FLOOR: i32 = 63;

struct Meadow {
    blocks: Vec<(BlockPos, BlockStateId)>,
    ground: BlockKind,
}

impl ChunkGenerator for Meadow {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(0));
        for x in 0..16 {
            for z in 0..16 {
                for y in 60..FLOOR {
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        BlockKind::Stone.default_state(),
                    );
                }
                chunk.set_block(
                    BlockPos::new(pos.min_block_x() + x, FLOOR, pos.min_block_z() + z),
                    self.ground.default_state(),
                );
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
        Vec3::new(40.5, 64.0, 40.5)
    }

    fn place(
        &self,
        dimension: &Dimension,
        request: &PlaceRequest,
    ) -> Result<Vec<PlacedBlock>, PlaceFailure> {
        WorldGenerator::from_level_type("void", 1, "").place(dimension, request)
    }
}

fn stage_one(kind: BlockKind) -> BlockStateId {
    kind.state_with(&[("stage", "1")]).expect("a sapling")
}

fn meadow(blocks: Vec<(BlockPos, BlockStateId)>, ground: BlockKind, seed: u64) -> Scene {
    let mut config = GameConfig {
        view_distance: 4,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    config.rules.random_tick_speed = 4096;
    let data = LevelData {
        spawn_mobs: false,
        day_time: 6_000,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let mut server: Server<Link> = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(Meadow { blocks, ground }),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(40, 64, 40));
    s.tick(2);
    s
}

fn kind_at(s: &Scene, pos: BlockPos) -> Option<BlockKind> {
    s.server.block_state_at(pos).and_then(BlockStateId::kind)
}

fn count_in(s: &Scene, origin: BlockPos, kind: BlockKind) -> usize {
    let mut found = 0;
    for dx in -12..=12 {
        for dz in -12..=12 {
            for dy in 0..40 {
                if kind_at(s, origin.offset(dx, dy, dz)) == Some(kind) {
                    found += 1;
                }
            }
        }
    }
    found
}

const SPOT: BlockPos = BlockPos::new(40, 64, 40);

fn single(kind: BlockKind) -> Scene {
    meadow(vec![(SPOT, stage_one(kind))], BlockKind::GrassBlock, 11)
}

fn grown(s: &mut Scene) -> bool {
    for _ in 0..200 {
        s.tick(1);
        if !matches!(
            kind_at(s, SPOT),
            Some(
                BlockKind::OakSapling
                    | BlockKind::SpruceSapling
                    | BlockKind::BirchSapling
                    | BlockKind::JungleSapling
                    | BlockKind::AcaciaSapling
                    | BlockKind::CherrySapling
                    | BlockKind::DarkOakSapling
                    | BlockKind::PaleOakSapling
                    | BlockKind::PoplarSapling
                    | BlockKind::MangrovePropagule
            )
        ) {
            return true;
        }
    }
    false
}

#[test]
fn every_single_sapling_grows_its_vanilla_tree() {
    for (sapling, log, leaves) in [
        (
            BlockKind::OakSapling,
            BlockKind::OakLog,
            BlockKind::OakLeaves,
        ),
        (
            BlockKind::BirchSapling,
            BlockKind::BirchLog,
            BlockKind::BirchLeaves,
        ),
        (
            BlockKind::SpruceSapling,
            BlockKind::SpruceLog,
            BlockKind::SpruceLeaves,
        ),
        (
            BlockKind::JungleSapling,
            BlockKind::JungleLog,
            BlockKind::JungleLeaves,
        ),
        (
            BlockKind::AcaciaSapling,
            BlockKind::AcaciaLog,
            BlockKind::AcaciaLeaves,
        ),
        (
            BlockKind::CherrySapling,
            BlockKind::CherryLog,
            BlockKind::CherryLeaves,
        ),
        (
            BlockKind::PoplarSapling,
            BlockKind::PoplarLog,
            BlockKind::RedPoplarLeaves,
        ),
    ] {
        let mut s = single(sapling);
        assert!(grown(&mut s), "{sapling:?} never grew");
        assert_eq!(kind_at(&s, SPOT), Some(log), "{sapling:?}");
        assert!(count_in(&s, SPOT, log) >= 3, "{sapling:?} trunk");
        let crown = count_in(&s, SPOT, leaves)
            + count_in(&s, SPOT, BlockKind::OrangePoplarLeaves)
            + count_in(&s, SPOT, BlockKind::YellowPoplarLeaves);
        assert!(crown >= 5, "{sapling:?} crown");
    }
}

fn square(kind: BlockKind) -> Vec<(BlockPos, BlockStateId)> {
    [(0, 0), (1, 0), (0, 1), (1, 1)]
        .into_iter()
        .map(|(dx, dz)| (SPOT.offset(dx, 0, dz), stage_one(kind)))
        .collect()
}

#[test]
fn two_by_two_saplings_grow_the_mega_trees_and_fill_the_four_cells() {
    for (sapling, log) in [
        (BlockKind::DarkOakSapling, BlockKind::DarkOakLog),
        (BlockKind::PaleOakSapling, BlockKind::PaleOakLog),
        (BlockKind::JungleSapling, BlockKind::JungleLog),
        (BlockKind::SpruceSapling, BlockKind::SpruceLog),
    ] {
        let mut s = meadow(square(sapling), BlockKind::GrassBlock, 5);
        assert!(grown(&mut s), "{sapling:?} never grew");
        for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            assert_eq!(
                kind_at(&s, SPOT.offset(dx, 0, dz)),
                Some(log),
                "{sapling:?} at {dx},{dz}"
            );
        }
        assert!(count_in(&s, SPOT, log) >= 16, "{sapling:?}");
    }
}

#[test]
fn a_lone_dark_oak_or_pale_oak_sapling_never_grows() {
    for sapling in [BlockKind::DarkOakSapling, BlockKind::PaleOakSapling] {
        let mut s = single(sapling);
        assert!(!grown(&mut s), "{sapling:?} grew alone");
    }
}

#[test]
fn a_roofed_sapling_stays_a_sapling() {
    let mut blocks = vec![(SPOT, stage_one(BlockKind::BirchSapling))];
    for dx in -2..=2 {
        for dz in -2..=2 {
            blocks.push((SPOT.offset(dx, 2, dz), BlockKind::Stone.default_state()));
        }
    }
    let mut s = meadow(blocks, BlockKind::GrassBlock, 3);
    assert!(!grown(&mut s));
    assert_eq!(kind_at(&s, SPOT), Some(BlockKind::BirchSapling));
}

#[test]
fn a_mangrove_propagule_planted_on_mud_grows_a_mangrove_tree() {
    let planted = BlockKind::MangrovePropagule
        .state_with(&[("stage", "1"), ("hanging", "false")])
        .expect("a propagule");
    let mut s = meadow(vec![(SPOT, planted)], BlockKind::Mud, 9);
    assert!(grown(&mut s));
    assert!(count_in(&s, SPOT, BlockKind::MangroveLog) >= 2);
    assert!(count_in(&s, SPOT, BlockKind::MangroveLeaves) >= 5);
}
