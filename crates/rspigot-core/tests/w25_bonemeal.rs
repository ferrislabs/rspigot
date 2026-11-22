mod common;
#[path = "common/screen.rs"]
mod screen;
#[path = "common/seeded.rs"]
mod seeded;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::Inbound;
use rspigot_core::plugins::{BlockOrder, Order};
use rspigot_core::ports::{ChunkGenerator, PlaceFailure, PlaceRequest, PlacedBlock};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Vec3};
use rspigot_worldgen::WorldGenerator;
use screen::use_block;
use seeded::{Seed, world};

const FLOOR: i32 = 63;
const AT: BlockPos = BlockPos::new(41, 64, 42);

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

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn state(kind: BlockKind, properties: &[(&str, &str)]) -> BlockStateId {
    kind.state_with(properties).expect("a known state")
}

fn meadow(blocks: Vec<(BlockPos, BlockStateId)>, ground: BlockKind) -> Scene {
    let config = GameConfig {
        view_distance: 4,
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
    let mut server: Server<Link> = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(Meadow { blocks, ground }),
        StatusBoard::new(20),
    )
    .with_rng_seed(17);
    server.wait_for_spawn(Duration::from_secs(10));
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, AT);
    s.tick(2);
    console(&mut s, "give Alice minecraft:bone_meal 64");
    s.tick(1);
    s
}

fn meal(s: &mut Scene, pos: BlockPos) {
    let alice = s.players[0].clone();
    use_block(s, &alice, pos);
    std::thread::sleep(Duration::from_millis(8));
    s.tick(2);
}

fn kind(s: &Scene, pos: BlockPos) -> Option<BlockKind> {
    s.server.block_state_at(pos).and_then(BlockStateId::kind)
}

fn prop(s: &Scene, pos: BlockPos, name: &str) -> Option<String> {
    s.server
        .block_state_at(pos)
        .and_then(|state| state.property(name).map(str::to_owned))
}

fn up(pos: BlockPos, dy: i32) -> BlockPos {
    pos.offset(0, dy, 0)
}

#[test]
fn bone_meal_ripens_stems_cocoa_berries_torchflowers_and_pitcher_crops() {
    let mut s = meadow(
        vec![
            (up(AT, -1), BlockKind::Farmland.default_state()),
            (AT.offset(1, -1, 1), BlockKind::Farmland.default_state()),
            (AT.offset(2, -1, 0), BlockKind::Farmland.default_state()),
            (AT.offset(0, 0, 2), BlockKind::JungleLog.default_state()),
            (AT, state(BlockKind::PumpkinStem, &[("age", "0")])),
            (
                AT.offset(1, 0, 0),
                state(BlockKind::SweetBerryBush, &[("age", "0")]),
            ),
            (
                AT.offset(2, 0, 0),
                state(BlockKind::TorchflowerCrop, &[("age", "0")]),
            ),
            (
                AT.offset(0, 0, 1),
                state(BlockKind::Cocoa, &[("age", "0"), ("facing", "south")]),
            ),
            (
                AT.offset(1, 0, 1),
                state(BlockKind::PitcherCrop, &[("age", "0"), ("half", "lower")]),
            ),
        ],
        BlockKind::GrassBlock,
    );
    meal(&mut s, AT);
    assert!(prop(&s, AT, "age").unwrap().parse::<i32>().unwrap() >= 2);
    meal(&mut s, AT.offset(1, 0, 0));
    assert_eq!(prop(&s, AT.offset(1, 0, 0), "age").as_deref(), Some("1"));
    meal(&mut s, AT.offset(2, 0, 0));
    assert_eq!(prop(&s, AT.offset(2, 0, 0), "age").as_deref(), Some("1"));
    meal(&mut s, AT.offset(2, 0, 0));
    assert_eq!(kind(&s, AT.offset(2, 0, 0)), Some(BlockKind::Torchflower));
    meal(&mut s, AT.offset(0, 0, 1));
    assert_eq!(prop(&s, AT.offset(0, 0, 1), "age").as_deref(), Some("1"));
    meal(&mut s, AT.offset(1, 0, 1));
    assert_eq!(prop(&s, AT.offset(1, 0, 1), "age").as_deref(), Some("1"));
}

#[test]
fn bone_meal_lengthens_kelp_and_the_two_nether_vines() {
    let mut s = meadow(
        vec![
            (AT, state(BlockKind::Kelp, &[("age", "0")])),
            (up(AT, 1), BlockKind::Water.default_state()),
            (up(AT, 2), BlockKind::Water.default_state()),
            (
                AT.offset(2, 6, 0),
                state(BlockKind::WeepingVines, &[("age", "3")]),
            ),
            (
                AT.offset(0, 0, 2),
                state(BlockKind::TwistingVines, &[("age", "3")]),
            ),
        ],
        BlockKind::GrassBlock,
    );
    meal(&mut s, AT);
    assert_eq!(kind(&s, up(AT, 1)), Some(BlockKind::Kelp));
    assert_eq!(prop(&s, up(AT, 1), "age").as_deref(), Some("1"));
    assert_eq!(kind(&s, AT), Some(BlockKind::KelpPlant));
    meal(&mut s, AT.offset(2, 6, 0));
    assert_eq!(kind(&s, AT.offset(2, 5, 0)), Some(BlockKind::WeepingVines));
    assert_eq!(
        kind(&s, AT.offset(2, 6, 0)),
        Some(BlockKind::WeepingVinesPlant)
    );
    meal(&mut s, AT.offset(0, 0, 2));
    assert_eq!(kind(&s, AT.offset(0, 1, 2)), Some(BlockKind::TwistingVines));
    assert_eq!(
        kind(&s, AT.offset(0, 0, 2)),
        Some(BlockKind::TwistingVinesPlant)
    );
}

#[test]
fn bone_meal_sets_cave_vine_berries_and_grows_bamboo_from_sapling_and_stalk() {
    let mut s = meadow(
        vec![
            (
                AT,
                state(BlockKind::CaveVines, &[("age", "3"), ("berries", "false")]),
            ),
            (AT.offset(1, 0, 0), BlockKind::BambooSapling.default_state()),
            (
                AT.offset(2, 0, 0),
                state(
                    BlockKind::Bamboo,
                    &[("age", "0"), ("leaves", "none"), ("stage", "0")],
                ),
            ),
        ],
        BlockKind::GrassBlock,
    );
    meal(&mut s, AT);
    assert_eq!(prop(&s, AT, "berries").as_deref(), Some("true"));
    meal(&mut s, AT.offset(1, 0, 0));
    assert_eq!(kind(&s, AT.offset(1, 1, 0)), Some(BlockKind::Bamboo));
    meal(&mut s, AT.offset(2, 0, 0));
    assert_eq!(kind(&s, AT.offset(2, 1, 0)), Some(BlockKind::Bamboo));
}

#[test]
fn bone_meal_drops_roots_propagules_and_hangs_moss_downwards() {
    let high = up(AT, 3);
    let mut s = meadow(
        vec![
            (high, BlockKind::RootedDirt.default_state()),
            (
                high.offset(1, 0, 0),
                BlockKind::MangroveLeaves.default_state(),
            ),
            (
                high.offset(0, 0, 1),
                state(BlockKind::PaleHangingMoss, &[("tip", "true")]),
            ),
        ],
        BlockKind::GrassBlock,
    );
    meal(&mut s, high);
    assert_eq!(kind(&s, up(high, -1)), Some(BlockKind::HangingRoots));
    meal(&mut s, high.offset(1, 0, 0));
    assert_eq!(
        kind(&s, high.offset(1, -1, 0)),
        Some(BlockKind::MangrovePropagule)
    );
    assert_eq!(
        prop(&s, high.offset(1, -1, 0), "hanging").as_deref(),
        Some("true")
    );
    meal(&mut s, high.offset(0, 0, 1));
    assert_eq!(
        kind(&s, high.offset(0, -1, 1)),
        Some(BlockKind::PaleHangingMoss)
    );
    assert_eq!(
        prop(&s, high.offset(0, -1, 1), "tip").as_deref(),
        Some("true")
    );
    assert_eq!(
        prop(&s, high.offset(0, 0, 1), "tip").as_deref(),
        Some("false")
    );
}

#[test]
fn bone_meal_adds_petals_ages_shelf_mushrooms_and_pops_tall_flowers() {
    let mut s = meadow(
        vec![
            (
                AT,
                state(
                    BlockKind::PinkPetals,
                    &[("facing", "north"), ("flower_amount", "1")],
                ),
            ),
            (
                AT.offset(1, 0, 0),
                state(
                    BlockKind::ShelfMushroom,
                    &[("age", "0"), ("facing", "north")],
                ),
            ),
            (
                AT.offset(2, 0, 0),
                state(BlockKind::Sunflower, &[("half", "lower")]),
            ),
        ],
        BlockKind::GrassBlock,
    );
    meal(&mut s, AT);
    assert_eq!(prop(&s, AT, "flower_amount").as_deref(), Some("2"));
    meal(&mut s, AT.offset(1, 0, 0));
    assert_eq!(prop(&s, AT.offset(1, 0, 0), "age").as_deref(), Some("1"));
    let before = s.server.ground_item_count();
    meal(&mut s, AT.offset(2, 0, 0));
    assert_eq!(s.server.ground_item_count(), before + 1);
}

#[test]
fn bone_meal_doubles_short_grass_and_fern_and_dry_grass() {
    let mut s = meadow(
        vec![
            (AT, BlockKind::ShortGrass.default_state()),
            (AT.offset(2, 0, 0), BlockKind::Fern.default_state()),
        ],
        BlockKind::GrassBlock,
    );
    meal(&mut s, AT);
    assert_eq!(kind(&s, AT), Some(BlockKind::TallGrass));
    assert_eq!(kind(&s, up(AT, 1)), Some(BlockKind::TallGrass));
    meal(&mut s, AT.offset(2, 0, 0));
    assert_eq!(kind(&s, AT.offset(2, 0, 0)), Some(BlockKind::LargeFern));
    let mut s = meadow(
        vec![(AT, BlockKind::ShortDryGrass.default_state())],
        BlockKind::Sand,
    );
    meal(&mut s, AT);
    assert_eq!(kind(&s, AT), Some(BlockKind::TallDryGrass));
}

#[test]
fn bone_meal_spreads_bushes_and_tall_dry_grass_to_a_free_neighbour() {
    let mut s = meadow(
        vec![(AT, BlockKind::Bush.default_state())],
        BlockKind::GrassBlock,
    );
    meal(&mut s, AT);
    let around = [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .filter(|(dx, dz)| kind(&s, AT.offset(*dx, 0, *dz)) == Some(BlockKind::Bush))
        .count();
    assert_eq!(around, 1);
    let mut s = meadow(
        vec![(AT, BlockKind::TallDryGrass.default_state())],
        BlockKind::Sand,
    );
    meal(&mut s, AT);
    let around = [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .filter(|(dx, dz)| kind(&s, AT.offset(*dx, 0, *dz)) == Some(BlockKind::ShortDryGrass))
        .count();
    assert_eq!(around, 1);
}

#[test]
fn bone_meal_makes_tall_seagrass_under_still_water_sometimes() {
    let mut s = meadow(
        vec![
            (AT, BlockKind::Seagrass.default_state()),
            (up(AT, 1), BlockKind::Water.default_state()),
            (up(AT, 2), BlockKind::Water.default_state()),
        ],
        BlockKind::GrassBlock,
    );
    for _ in 0..60 {
        meal(&mut s, AT);
        if kind(&s, AT) == Some(BlockKind::TallSeagrass) {
            assert_eq!(kind(&s, up(AT, 1)), Some(BlockKind::TallSeagrass));
            return;
        }
    }
    panic!("the seagrass never doubled");
}

#[test]
fn bone_meal_turns_netherrack_beside_nylium_into_nylium() {
    let mut s = meadow(
        vec![
            (AT, BlockKind::Netherrack.default_state()),
            (AT.offset(1, 0, 0), BlockKind::WarpedNylium.default_state()),
        ],
        BlockKind::Netherrack,
    );
    meal(&mut s, AT);
    assert_eq!(kind(&s, AT), Some(BlockKind::WarpedNylium));
}

#[test]
fn bone_meal_spreads_glow_lichen_over_a_neighbouring_floor_cell() {
    let mut s = meadow(
        vec![(AT, state(BlockKind::GlowLichen, &[("down", "true")]))],
        BlockKind::Stone,
    );
    meal(&mut s, AT);
    let spread = [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .filter(|(dx, dz)| kind(&s, AT.offset(*dx, 0, *dz)) == Some(BlockKind::GlowLichen))
        .count();
    assert_eq!(spread, 1);
}

fn changed_around(s: &Scene, center: BlockPos, matching: impl Fn(BlockKind) -> bool) -> usize {
    let mut found = 0;
    for dx in -8..=8 {
        for dy in -1..=12 {
            for dz in -8..=8 {
                if kind(s, center.offset(dx, dy, dz)).is_some_and(&matching) {
                    found += 1;
                }
            }
        }
    }
    found
}

#[test]
fn bone_meal_grows_huge_mushrooms_from_both_small_ones() {
    for (small, cap) in [
        (BlockKind::RedMushroom, BlockKind::RedMushroomBlock),
        (BlockKind::BrownMushroom, BlockKind::BrownMushroomBlock),
    ] {
        let mut s = meadow(vec![(AT, small.default_state())], BlockKind::Mycelium);
        let mut grew = false;
        for _ in 0..80 {
            meal(&mut s, AT);
            if changed_around(&s, AT, |found| found == cap) > 4 {
                grew = true;
                break;
            }
        }
        assert!(grew, "{small:?} never became a huge mushroom");
        assert_eq!(kind(&s, AT), Some(BlockKind::MushroomStem));
    }
}

#[test]
fn bone_meal_grows_a_huge_fungus_on_its_own_nylium_only() {
    let mut s = meadow(
        vec![(AT, BlockKind::CrimsonFungus.default_state())],
        BlockKind::CrimsonNylium,
    );
    let mut grew = false;
    for _ in 0..80 {
        meal(&mut s, AT);
        if changed_around(&s, AT, |found| found == BlockKind::NetherWartBlock) > 4 {
            grew = true;
            break;
        }
    }
    assert!(grew);
    let mut s = meadow(
        vec![(AT, BlockKind::CrimsonFungus.default_state())],
        BlockKind::WarpedNylium,
    );
    for _ in 0..10 {
        meal(&mut s, AT);
    }
    assert_eq!(kind(&s, AT), Some(BlockKind::CrimsonFungus));
}

fn eventually(s: &mut Scene, found: impl Fn(&Scene) -> bool) -> bool {
    for _ in 0..150 {
        if found(s) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
        s.tick(1);
    }
    false
}

#[test]
fn bone_meal_spreads_vegetation_over_nylium_and_moss_patches_over_moss() {
    for (ground, wanted) in [
        (
            BlockKind::CrimsonNylium,
            vec![
                BlockKind::CrimsonRoots,
                BlockKind::CrimsonFungus,
                BlockKind::WarpedFungus,
            ],
        ),
        (
            BlockKind::MossBlock,
            vec![
                BlockKind::MossCarpet,
                BlockKind::ShortGrass,
                BlockKind::Azalea,
                BlockKind::FloweringAzalea,
                BlockKind::TallGrass,
            ],
        ),
        (
            BlockKind::PaleMossBlock,
            vec![
                BlockKind::PaleMossCarpet,
                BlockKind::ShortGrass,
                BlockKind::TallGrass,
            ],
        ),
    ] {
        let mut s = meadow(vec![], ground);
        meal(&mut s, AT.offset(0, -1, 0));
        let grew = eventually(&mut s, |s| {
            changed_around(s, AT, |found| wanted.contains(&found)) > 0
        });
        assert!(grew, "{ground:?} grew nothing");
    }
}

#[test]
fn a_dispenser_bone_meals_a_berry_bush_in_front_of_it() {
    let mut s = world(
        vec![
            Seed::block(BlockPos::new(3, 64, 3), BlockKind::Dispenser)
                .with("facing", "east")
                .holding("minecraft:bone_meal", 3),
            Seed::block(BlockPos::new(4, 63, 3), BlockKind::Dirt),
            Seed::block(BlockPos::new(4, 64, 3), BlockKind::SweetBerryBush).with("age", "0"),
        ],
        Vec::new(),
    );
    s.act(Order::Block {
        pos: BlockPos::new(3, 64, 2),
        order: BlockOrder::Set("minecraft:redstone_block".to_owned()),
    });
    s.tick(6);
    let age = s
        .server
        .block_state_at(BlockPos::new(4, 64, 3))
        .and_then(|state| state.property("age").map(str::to_owned));
    assert_eq!(age.as_deref(), Some("1"));
}
