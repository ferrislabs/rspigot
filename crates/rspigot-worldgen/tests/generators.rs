use std::collections::BTreeSet;

use rspigot_blocks::{BlockKind, STATE_COUNT};
use rspigot_core::ports::ChunkGenerator;
use rspigot_data::registries;
use rspigot_domain::chunk::{ChunkColumn, ChunkStatus, HeightmapKind};
use rspigot_domain::world::Dimension;
use rspigot_domain::{BlockPos, ChunkPos};
use rspigot_worldgen::{
    FlatGenerator, FlatSettings, NoiseGenerator, NoiseOptions, VoidGenerator, WorldGenerator,
};

const NON_BLOCKING: [&str; 9] = [
    "minecraft:short_grass",
    "minecraft:tall_grass",
    "minecraft:fern",
    "minecraft:dandelion",
    "minecraft:poppy",
    "minecraft:azure_bluet",
    "minecraft:oxeye_daisy",
    "minecraft:cornflower",
    "minecraft:dead_bush",
];

fn name_at(column: &ChunkColumn, x: i32, y: i32, z: i32) -> &'static str {
    let pos = BlockPos::new(
        column.pos().min_block_x() + x,
        y,
        column.pos().min_block_z() + z,
    );
    column.block(pos).unwrap().kind().unwrap().name()
}

fn for_each_block(column: &ChunkColumn, mut visit: impl FnMut(i32, i32, i32, &'static str)) {
    let min_y = column.height().min_y;
    for (index, section) in column.sections().iter().enumerate() {
        for y in 0..16 {
            for z in 0..16 {
                for x in 0..16 {
                    let id = section.block(x, y, z);
                    visit(
                        x as i32,
                        min_y + index as i32 * 16 + y as i32,
                        z as i32,
                        id.kind().unwrap().name(),
                    );
                }
            }
        }
    }
}

fn height_of(column: &ChunkColumn, kind: HeightmapKind, x: usize, z: usize) -> i32 {
    i32::from(column.heightmaps.get(kind).unwrap().get(x, z))
}

fn top_y(column: &ChunkColumn, kind: HeightmapKind, x: usize, z: usize) -> i32 {
    column.height().min_y + height_of(column, kind, x, z) - 1
}

fn contains_name(column: &ChunkColumn, wanted: &str) -> bool {
    let mut found = false;
    for_each_block(column, |_, _, _, name| found |= name == wanted);
    found
}

fn count_names(column: &ChunkColumn, matches: impl Fn(&str) -> bool) -> usize {
    let mut count = 0;
    for_each_block(column, |_, _, _, name| count += usize::from(matches(name)));
    count
}

fn assert_send_sync<T: Send + Sync + 'static>() {}

#[test]
fn generators_are_thread_safe_adapters() {
    assert_send_sync::<WorldGenerator>();
    assert_send_sync::<FlatGenerator>();
    assert_send_sync::<VoidGenerator>();
    assert_send_sync::<NoiseGenerator>();
}

#[test]
fn flat_default_layout_and_heightmaps() {
    let generator = FlatGenerator::vanilla(1);
    let dimension = Dimension::overworld();
    let column = generator.generate(&dimension, ChunkPos::new(3, -2));
    assert_eq!(column.pos(), ChunkPos::new(3, -2));
    assert_eq!(column.status, ChunkStatus::Full);
    for (x, z) in [(0, 0), (7, 12), (15, 15)] {
        assert_eq!(name_at(&column, x, -65 + 1, z), "minecraft:bedrock");
        assert_eq!(name_at(&column, x, -63, z), "minecraft:dirt");
        assert_eq!(name_at(&column, x, -62, z), "minecraft:dirt");
        assert_eq!(name_at(&column, x, -61, z), "minecraft:grass_block");
        assert_eq!(name_at(&column, x, -60, z), "minecraft:air");
        assert_eq!(name_at(&column, x, 319, z), "minecraft:air");
    }
    for kind in HeightmapKind::ALL {
        for z in 0..16 {
            for x in 0..16 {
                assert_eq!(height_of(&column, kind, x, z), 4, "{kind:?}");
            }
        }
    }
    let plains = registries::biome_id("minecraft:plains").unwrap();
    for section in column.sections() {
        assert_eq!(section.biomes().single_value().map(|b| b.0), Some(plains));
    }
    assert!(column.light.sky.iter().all(Option::is_none));
    assert!(column.light.block.iter().all(Option::is_none));
    assert_eq!(
        column.sections().iter().filter(|s| !s.is_empty()).count(),
        1
    );
}

#[test]
fn flat_settings_json_drives_layers_and_biome() {
    let json = r#"{"layers":[{"block":"minecraft:bedrock","height":1},
        {"block":"minecraft:stone","height":9},{"block":"minecraft:water","height":3}],
        "biome":"minecraft:desert"}"#;
    let generator = FlatGenerator::from_json(json, 0).unwrap();
    let dimension = Dimension::overworld();
    let column = generator.generate(&dimension, ChunkPos::new(0, 0));
    assert_eq!(name_at(&column, 4, -64, 4), "minecraft:bedrock");
    assert_eq!(name_at(&column, 4, -63, 4), "minecraft:stone");
    assert_eq!(name_at(&column, 4, -55, 4), "minecraft:stone");
    assert_eq!(name_at(&column, 4, -54, 4), "minecraft:water");
    assert_eq!(name_at(&column, 4, -52, 4), "minecraft:water");
    assert_eq!(name_at(&column, 4, -51, 4), "minecraft:air");
    assert_eq!(height_of(&column, HeightmapKind::WorldSurface, 0, 0), 13);
    assert_eq!(height_of(&column, HeightmapKind::MotionBlocking, 0, 0), 13);
    assert_eq!(height_of(&column, HeightmapKind::OceanFloor, 0, 0), 10);
    assert_eq!(height_of(&column, HeightmapKind::OceanFloorWg, 5, 5), 10);
    let desert = registries::biome_id("minecraft:desert").unwrap();
    assert_eq!(column.sections()[0].biome(1, 1, 1).0, desert);
    let spawn = generator.spawn_point(&dimension);
    assert_eq!(spawn.y, -64.0 + 10.0);
}

#[test]
fn flat_spawn_stands_on_the_grass() {
    let generator = FlatGenerator::vanilla(1);
    let spawn = generator.spawn_point(&Dimension::overworld());
    assert_eq!((spawn.x, spawn.y, spawn.z), (0.5, -60.0, 0.5));
}

fn assert_vanilla_other_dimensions<G: ChunkGenerator>(generator: &G) {
    let nether = generator.generate(&Dimension::nether(), ChunkPos::new(0, 0));
    assert!(contains_name(&nether, "minecraft:netherrack"));
    let end = generator.generate(&Dimension::end(), ChunkPos::new(0, 0));
    assert!(contains_name(&end, "minecraft:end_stone"));
}

#[test]
fn flat_and_void_keep_vanilla_terrain_in_other_dimensions() {
    assert_vanilla_other_dimensions(&FlatGenerator::vanilla(9));
    assert_vanilla_other_dimensions(&VoidGenerator::new(9));
}

#[test]
fn void_has_a_single_stone_platform_in_the_spawn_chunk() {
    let generator = VoidGenerator::new(5);
    let dimension = Dimension::overworld();
    let origin = generator.generate(&dimension, ChunkPos::new(0, 0));
    let stones = count_names(&origin, |name| name != "minecraft:air");
    assert_eq!(stones, 1);
    assert_eq!(name_at(&origin, 0, 63, 0), "minecraft:stone");
    assert_eq!(height_of(&origin, HeightmapKind::WorldSurface, 0, 0), 128);
    assert_eq!(height_of(&origin, HeightmapKind::MotionBlocking, 0, 0), 128);
    assert_eq!(height_of(&origin, HeightmapKind::WorldSurface, 1, 0), 0);
    let the_void = registries::biome_id("minecraft:the_void").unwrap();
    for section in origin.sections() {
        assert_eq!(section.biomes().single_value().map(|b| b.0), Some(the_void));
    }
    for pos in [
        ChunkPos::new(1, 0),
        ChunkPos::new(0, -1),
        ChunkPos::new(-7, 9),
    ] {
        let column = generator.generate(&dimension, pos);
        assert!(column.sections().iter().all(|s| s.is_empty()));
        for kind in HeightmapKind::ALL {
            assert_eq!(height_of(&column, kind, 3, 3), 0);
        }
    }
    let spawn = generator.spawn_point(&dimension);
    assert_eq!((spawn.x, spawn.y, spawn.z), (0.5, 64.0, 0.5));
}

#[test]
fn every_generator_reports_full_chunks_with_all_heightmaps_and_no_light() {
    let seed = 77;
    let generators = [
        WorldGenerator::from_level_type("minecraft:flat", seed, ""),
        WorldGenerator::from_level_type("void", seed, ""),
        WorldGenerator::from_level_type("minecraft:normal", seed, ""),
    ];
    for generator in &generators {
        for dimension in [
            Dimension::overworld(),
            Dimension::nether(),
            Dimension::end(),
        ] {
            let column = generator.generate(&dimension, ChunkPos::new(-1, 0));
            assert_eq!(column.status, ChunkStatus::Full);
            assert_eq!(column.height(), dimension.height);
            assert_eq!(column.sections().len(), dimension.height.section_count());
            assert!(column.light.sky.iter().all(Option::is_none));
            assert!(column.light.block.iter().all(Option::is_none));
            assert!(column.block_entities.is_empty());
            for kind in HeightmapKind::ALL {
                assert!(column.heightmaps.get(kind).is_some(), "{kind:?}");
            }
            assert!(!column.is_dirty());
        }
    }
}

#[test]
fn generation_is_deterministic_and_seed_dependent() {
    for dimension in [
        Dimension::overworld(),
        Dimension::nether(),
        Dimension::end(),
    ] {
        let a = NoiseGenerator::new(1234);
        let b = NoiseGenerator::new(1234);
        let other = NoiseGenerator::new(4321);
        let far = ChunkPos::new(900, -1200);
        for pos in [ChunkPos::new(0, 0), ChunkPos::new(-13, 21), far] {
            assert_eq!(a.generate(&dimension, pos), b.generate(&dimension, pos));
            assert_eq!(a.generate(&dimension, pos), a.generate(&dimension, pos));
        }
        let differs =
            |pos: ChunkPos| a.generate(&dimension, pos) != other.generate(&dimension, pos);
        if dimension.key.path() == "the_end" {
            assert!((0..40).any(|i| differs(ChunkPos::new(70 + i, 80 + i))));
        } else {
            assert!(differs(ChunkPos::new(0, 0)));
        }
    }
    let flat_a = FlatGenerator::vanilla(1);
    let flat_b = FlatGenerator::vanilla(1);
    let dimension = Dimension::overworld();
    assert_eq!(
        flat_a.generate(&dimension, ChunkPos::new(4, 4)),
        flat_b.generate(&dimension, ChunkPos::new(4, 4))
    );
}

#[test]
fn noise_chunk_has_jagged_bedrock_floor_and_clear_sky() {
    let generator = NoiseGenerator::new(31337);
    let dimension = Dimension::overworld();
    for pos in [
        ChunkPos::new(0, 0),
        ChunkPos::new(-20, 14),
        ChunkPos::new(55, -3),
    ] {
        let column = generator.generate(&dimension, pos);
        let mut lowest_free = 0;
        let mut max_surface = i32::MIN;
        for z in 0..16 {
            for x in 0..16 {
                assert_eq!(name_at(&column, x, -64, z), "minecraft:bedrock");
                assert_ne!(name_at(&column, x, -59, z), "minecraft:bedrock");
                for y in -63..=-60 {
                    if name_at(&column, x, y, z) != "minecraft:bedrock" {
                        lowest_free += 1;
                    }
                }
                max_surface = max_surface.max(top_y(
                    &column,
                    HeightmapKind::WorldSurface,
                    x as usize,
                    z as usize,
                ));
            }
        }
        assert!(lowest_free > 100, "{lowest_free}");
        assert!(max_surface < 319);
        for z in 0..16 {
            for x in 0..16 {
                assert_eq!(name_at(&column, x, 319, z), "minecraft:air");
                for y in (max_surface + 1)..=319 {
                    assert_eq!(name_at(&column, x, y, z), "minecraft:air");
                }
            }
        }
    }
}

#[test]
fn noise_heightmaps_match_the_blocks() {
    let generator = NoiseGenerator::new(2024);
    let dimension = Dimension::overworld();
    for pos in [
        ChunkPos::new(0, 0),
        ChunkPos::new(-8, 5),
        ChunkPos::new(30, 30),
        ChunkPos::new(7, -40),
    ] {
        let column = generator.generate(&dimension, pos);
        for z in 0..16usize {
            for x in 0..16usize {
                let mut surface = 0;
                let mut floor = 0;
                let mut motion = 0;
                let mut no_leaves = 0;
                for y in (-64..320).rev() {
                    let name = name_at(&column, x as i32, y, z as i32);
                    if name == "minecraft:air" {
                        continue;
                    }
                    let relative = y + 65;
                    let fluid = name == "minecraft:water" || name == "minecraft:lava";
                    let solid = !fluid
                        && !NON_BLOCKING.contains(&name)
                        && (name == "minecraft:snow"
                            || !column
                                .block(BlockPos::new(
                                    column.pos().min_block_x() + x as i32,
                                    y,
                                    column.pos().min_block_z() + z as i32,
                                ))
                                .unwrap()
                                .collision_shape()
                                .is_empty());
                    let leaves = name.ends_with("_leaves");
                    if surface == 0 {
                        surface = relative;
                    }
                    if floor == 0 && solid {
                        floor = relative;
                    }
                    if motion == 0 && (solid || fluid) {
                        motion = relative;
                    }
                    if no_leaves == 0 && (solid || fluid) && !leaves {
                        no_leaves = relative;
                    }
                }
                assert_eq!(
                    height_of(&column, HeightmapKind::WorldSurface, x, z),
                    surface
                );
                assert_eq!(
                    height_of(&column, HeightmapKind::WorldSurfaceWg, x, z),
                    surface
                );
                assert_eq!(height_of(&column, HeightmapKind::OceanFloor, x, z), floor);
                assert_eq!(height_of(&column, HeightmapKind::OceanFloorWg, x, z), floor);
                assert_eq!(
                    height_of(&column, HeightmapKind::MotionBlocking, x, z),
                    motion
                );
                assert_eq!(
                    height_of(&column, HeightmapKind::MotionBlockingNoLeaves, x, z),
                    no_leaves
                );
            }
        }
    }
}

#[test]
fn oceans_hold_water_up_to_sea_level_for_some_seed() {
    let dimension = Dimension::overworld();
    let sea_level = dimension.sea_level;
    let mut found = None;
    'seeds: for seed in 0..8 {
        let generator = NoiseGenerator::new(seed);
        for cz in -6..6 {
            for cx in -6..6 {
                let column = generator.generate(&dimension, ChunkPos::new(cx * 5, cz * 5));
                for z in 0..16usize {
                    for x in 0..16usize {
                        let floor = top_y(&column, HeightmapKind::OceanFloor, x, z);
                        let top = top_y(&column, HeightmapKind::WorldSurface, x, z);
                        if floor < sea_level - 10 && top == sea_level - 1 {
                            found = Some((seed, column, x as i32, z as i32));
                            break 'seeds;
                        }
                    }
                }
            }
        }
    }
    let (_, column, x, z) = found.unwrap();
    let floor = top_y(&column, HeightmapKind::OceanFloor, x as usize, z as usize);
    assert_eq!(name_at(&column, x, sea_level - 1, z), "minecraft:water");
    assert_eq!(name_at(&column, x, floor + 1, z), "minecraft:water");
    assert_ne!(name_at(&column, x, floor, z), "minecraft:water");
    assert!(["minecraft:air", "minecraft:ice"].contains(&name_at(&column, x, sea_level, z)));
}

fn assert_standing_spot(generator: &NoiseGenerator, dimension: &Dimension, context: &str) {
    let spawn = generator.spawn_point(dimension);
    assert!(spawn.is_finite(), "{context}");
    let feet = spawn.block();
    let column = generator.generate(dimension, feet.chunk());
    let at = |dy: i32| {
        let pos = feet.offset(0, dy, 0);
        column.block(pos).unwrap().kind().unwrap().name()
    };
    let floor = at(-1);
    assert_ne!(floor, "minecraft:air", "{context}");
    for liquid in ["minecraft:water", "minecraft:lava"] {
        assert_ne!(floor, liquid, "{context}");
        assert_ne!(at(0), liquid, "{context}");
    }
    assert!(!floor.ends_with("_leaves"), "{context} {floor}");
    assert!(!floor.ends_with("_log"), "{context} {floor}");
    for dy in 0..2 {
        let name = at(dy);
        let unobstructed = column
            .block(feet.offset(0, dy, 0))
            .is_some_and(|state| state.collision_shape().is_empty());
        let passable = name == "minecraft:air"
            || name == "minecraft:snow"
            || NON_BLOCKING.contains(&name)
            || unobstructed;
        assert!(passable, "{context}: {name}");
    }
}

#[test]
fn overworld_spawn_is_dry_solid_ground_near_the_origin() {
    let dimension = Dimension::overworld();
    for seed in [0, 1, 2, 3, 42, 777, -5, 123_456_789, i64::MAX, 2024] {
        let generator = NoiseGenerator::new(seed);
        assert_standing_spot(&generator, &dimension, &format!("seed {seed}"));
        let spawn = generator.spawn_point(&dimension);
        assert!(
            spawn.y >= f64::from(dimension.sea_level),
            "seed {seed} spawn {spawn:?}"
        );
        assert!(
            spawn.x.abs() < 2000.0 && spawn.z.abs() < 2000.0,
            "seed {seed} spawn {spawn:?}"
        );
        assert_eq!(spawn, generator.spawn_point(&dimension));
    }
}

#[test]
fn nether_and_end_spawns_stand_on_solid_ground() {
    let generator = NoiseGenerator::new(99);
    assert_standing_spot(&generator, &Dimension::nether(), "nether");
    assert_standing_spot(&generator, &Dimension::end(), "end");
    let nether = generator.spawn_point(&Dimension::nether());
    assert!(nether.y >= 32.0 && nether.y < 125.0);
    let end = generator.spawn_point(&Dimension::end());
    assert!(end.x.abs() < 80.0 && end.z.abs() < 80.0);
}

#[test]
fn negative_chunk_coordinates_are_handled() {
    let generator = NoiseGenerator::new(555);
    let dimension = Dimension::overworld();
    for pos in [
        ChunkPos::new(-1, -1),
        ChunkPos::new(-1, 0),
        ChunkPos::new(0, -1),
        ChunkPos::new(-100, -57),
        ChunkPos::new(i32::from(i16::MIN), 3),
    ] {
        let column = generator.generate(&dimension, pos);
        assert_eq!(column.pos(), pos);
        let block = BlockPos::new(pos.x * 16 + 15, -64, pos.z * 16);
        assert_eq!(block.chunk(), pos);
        assert_eq!(
            column.block(block).unwrap().kind().map(BlockKind::name),
            Some("minecraft:bedrock")
        );
        assert_eq!(
            column.block(BlockPos::new(pos.x * 16 + 16, 0, pos.z * 16)),
            None
        );
        assert!(column.sections().iter().any(|s| !s.is_empty()));
    }
    let flat = FlatGenerator::vanilla(0).generate(&dimension, ChunkPos::new(-1, -1));
    assert_eq!(name_at(&flat, 15, -61, 15), "minecraft:grass_block");
}

#[test]
fn terrain_is_continuous_across_chunk_borders() {
    let generator = NoiseGenerator::new(808);
    let dimension = Dimension::overworld();
    let mut checked = 0;
    for cx in -3..3 {
        let left = generator.generate(&dimension, ChunkPos::new(cx, -1));
        let right = generator.generate(&dimension, ChunkPos::new(cx + 1, -1));
        for z in 0..16 {
            let a = height_of(&left, HeightmapKind::OceanFloor, 15, z);
            let b = height_of(&right, HeightmapKind::OceanFloor, 0, z);
            assert!((a - b).abs() <= 24, "{cx} {a} {b}");
            checked += 1;
        }
    }
    assert_eq!(checked, 96);
    let above = generator.generate(&dimension, ChunkPos::new(2, 3));
    let below = generator.generate(&dimension, ChunkPos::new(2, 4));
    for x in 0..16 {
        let a = height_of(&above, HeightmapKind::OceanFloor, x, 15);
        let b = height_of(&below, HeightmapKind::OceanFloor, x, 0);
        assert!((a - b).abs() <= 24, "{x} {a} {b}");
    }
}

#[test]
fn parallel_generation_matches_sequential() {
    let generator = NoiseGenerator::new(4242);
    let positions: Vec<(Dimension, ChunkPos)> = (-3..5)
        .flat_map(|x| (-2..3).map(move |z| ChunkPos::new(x * 7, z * 11)))
        .enumerate()
        .map(|(i, pos)| {
            let dimension = match i % 5 {
                3 => Dimension::nether(),
                4 => Dimension::end(),
                _ => Dimension::overworld(),
            };
            (dimension, pos)
        })
        .collect();
    let sequential: Vec<ChunkColumn> = positions
        .iter()
        .map(|(dimension, pos)| generator.generate(dimension, *pos))
        .collect();
    let mut parallel: Vec<Option<ChunkColumn>> = vec![None; positions.len()];
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|worker| {
                let generator = &generator;
                let positions = &positions;
                scope.spawn(move || {
                    positions
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| i % 8 == worker)
                        .map(|(i, (dimension, pos))| (i, generator.generate(dimension, *pos)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for handle in handles {
            for (i, column) in handle.join().unwrap() {
                parallel[i] = Some(column);
            }
        }
    });
    for (expected, actual) in sequential.iter().zip(&parallel) {
        assert_eq!(Some(expected), actual.as_ref());
    }
}

#[test]
fn world_generator_enum_is_shared_between_threads() {
    let generator = std::sync::Arc::new(WorldGenerator::from_level_type("minecraft:normal", 6, ""));
    let dimension = Dimension::overworld();
    let expected = generator.generate(&dimension, ChunkPos::new(-4, 4));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let generator = std::sync::Arc::clone(&generator);
            let dimension = dimension.clone();
            std::thread::spawn(move || generator.generate(&dimension, ChunkPos::new(-4, 4)))
        })
        .collect();
    for handle in handles {
        assert_eq!(handle.join().unwrap(), expected);
    }
}

#[test]
fn underground_has_deepslate_ores_caves_and_lava() {
    let generator = NoiseGenerator::new(1001);
    let dimension = Dimension::overworld();
    let mut names: BTreeSet<&'static str> = BTreeSet::new();
    let mut caves = 0usize;
    let mut diamond_heights = Vec::new();
    for cz in -4..4 {
        for cx in -4..4 {
            let column = generator.generate(&dimension, ChunkPos::new(cx, cz));
            for_each_block(&column, |x, y, z, name| {
                names.insert(name);
                if name.ends_with("diamond_ore") {
                    diamond_heights.push(y);
                }
                if (-50..40).contains(&y) && name == "minecraft:air" {
                    let top = top_y(&column, HeightmapKind::WorldSurface, x as usize, z as usize);
                    if y < top - 8 {
                        caves += 1;
                    }
                }
            });
        }
    }
    for expected in [
        "minecraft:stone",
        "minecraft:deepslate",
        "minecraft:bedrock",
        "minecraft:coal_ore",
        "minecraft:iron_ore",
        "minecraft:deepslate_iron_ore",
        "minecraft:copper_ore",
        "minecraft:deepslate_gold_ore",
        "minecraft:deepslate_redstone_ore",
        "minecraft:deepslate_lapis_ore",
        "minecraft:deepslate_diamond_ore",
        "minecraft:lava",
        "minecraft:granite",
        "minecraft:tuff",
        "minecraft:gravel",
    ] {
        assert!(names.contains(expected), "{expected}");
    }
    assert!(caves > 2000, "{caves}");
    assert!(!diamond_heights.is_empty());
    assert!(diamond_heights.iter().all(|y| *y <= 16));
}

#[test]
fn ore_depths_follow_vanilla_ranges() {
    let generator = NoiseGenerator::new(77);
    let dimension = Dimension::overworld();
    for cz in -3..3 {
        for cx in -3..3 {
            let column = generator.generate(&dimension, ChunkPos::new(cx, cz));
            for_each_block(&column, |_, y, _, name| {
                if name == "minecraft:deepslate_coal_ore" || name == "minecraft:deepslate_iron_ore"
                {
                    assert!(y < 8, "{name} at {y}");
                }
                if name == "minecraft:gold_ore" || name == "minecraft:redstone_ore" {
                    assert!(y >= -64, "{name} at {y}");
                }
                if name == "minecraft:redstone_ore" || name == "minecraft:deepslate_redstone_ore" {
                    assert!(y <= 17, "{name} at {y}");
                }
                if name == "minecraft:stone" {
                    assert!(y >= 0, "stone at {y}");
                }
                if name == "minecraft:deepslate" {
                    assert!(y < 8, "deepslate at {y}");
                }
            });
        }
    }
}

#[test]
fn surface_features_trees_snow_sand_and_water_all_appear() {
    const EXPECTED: [&str; 15] = [
        "minecraft:grass_block",
        "minecraft:dirt",
        "minecraft:sand",
        "minecraft:water",
        "minecraft:snow",
        "minecraft:snow_block",
        "minecraft:ice",
        "minecraft:gravel",
        "minecraft:short_grass",
        "minecraft:oak_log",
        "minecraft:oak_leaves",
        "minecraft:birch_log",
        "minecraft:spruce_log",
        "minecraft:spruce_leaves",
        "minecraft:dandelion",
    ];
    let dimension = Dimension::overworld();
    let mut names: BTreeSet<&'static str> = BTreeSet::new();
    for seed in [3, 4, 5, 6] {
        let generator = NoiseGenerator::new(seed);
        for cz in -10..10 {
            for cx in -10..10 {
                let column = generator.generate(&dimension, ChunkPos::new(cx * 4, cz * 4));
                for z in 0..16 {
                    for x in 0..16 {
                        let top = top_y(&column, HeightmapKind::WorldSurface, x, z);
                        for y in (top - 12).max(-64)..=top {
                            names.insert(name_at(&column, x as i32, y, z as i32));
                        }
                    }
                }
            }
        }
        if EXPECTED.iter().all(|name| names.contains(name)) {
            break;
        }
    }
    for expected in EXPECTED {
        assert!(names.contains(expected), "{expected}");
    }
}

#[test]
fn biomes_are_registered_and_varied() {
    let generator = NoiseGenerator::new(12345);
    let dimension = Dimension::overworld();
    let mut seen: BTreeSet<&'static str> = BTreeSet::new();
    for cz in -12..12 {
        for cx in -12..12 {
            let column = generator.generate(&dimension, ChunkPos::new(cx * 9, cz * 9));
            for section in column.sections() {
                for id in section.biomes().values() {
                    seen.insert(registries::biome_name(id.0).unwrap());
                }
            }
        }
    }
    for expected in [
        "minecraft:forest",
        "minecraft:ocean",
        "minecraft:beach",
        "minecraft:jungle",
        "minecraft:dark_forest",
        "minecraft:deep_ocean",
    ] {
        assert!(seen.contains(expected), "{expected} {seen:?}");
    }
    println!("SEEN {} {seen:?}", seen.len());
    assert!(seen.len() >= 25, "{seen:?}");
}

#[test]
fn block_states_in_every_section_are_valid() {
    let generator = NoiseGenerator::new(8);
    for dimension in [
        Dimension::overworld(),
        Dimension::nether(),
        Dimension::end(),
    ] {
        for pos in [
            ChunkPos::new(0, 0),
            ChunkPos::new(-5, 3),
            ChunkPos::new(2, -9),
        ] {
            let column = generator.generate(&dimension, pos);
            for section in column.sections() {
                for state in section.blocks().palette() {
                    assert!(usize::from(state.0) < STATE_COUNT);
                }
                for biome in section.biomes().palette() {
                    assert!(registries::biome_name(biome.0).is_some());
                }
                let counted = (0..4096)
                    .filter(|i| !section.blocks().get(*i).is_air())
                    .count();
                assert_eq!(usize::from(section.non_air_count()), counted);
            }
        }
    }
}

#[test]
fn nether_has_bedrock_shell_lava_sea_and_open_sky_above_the_roof() {
    let generator = NoiseGenerator::new(31);
    let dimension = Dimension::nether();
    let mut names: BTreeSet<&'static str> = BTreeSet::new();
    for pos in [
        ChunkPos::new(0, 0),
        ChunkPos::new(-3, 5),
        ChunkPos::new(8, -8),
    ] {
        let column = generator.generate(&dimension, pos);
        for z in 0..16 {
            for x in 0..16 {
                assert_eq!(name_at(&column, x, 0, z), "minecraft:bedrock");
                assert_eq!(name_at(&column, x, 127, z), "minecraft:bedrock");
                for y in 128..256 {
                    assert_eq!(name_at(&column, x, y, z), "minecraft:air");
                }
            }
        }
        for_each_block(&column, |_, y, _, name| {
            names.insert(name);
            if name == "minecraft:lava" {
                assert!(y < 32, "lava at {y}");
            }
        });
        assert_eq!(
            registries::biome_name(column.sections()[0].biome(0, 0, 0).0),
            Some("minecraft:nether_wastes")
        );
    }
    for expected in [
        "minecraft:netherrack",
        "minecraft:lava",
        "minecraft:nether_quartz_ore",
    ] {
        assert!(names.contains(expected), "{expected}");
    }
}

#[test]
fn end_has_a_central_island_and_empty_ring() {
    let generator = NoiseGenerator::new(6);
    let dimension = Dimension::end();
    let center = generator.generate(&dimension, ChunkPos::new(0, 0));
    assert!(count_names(&center, |name| name == "minecraft:end_stone") > 1000);
    assert_eq!(
        count_names(&center, |name| name != "minecraft:air"
            && name != "minecraft:end_stone"),
        0
    );
    let gap = generator.generate(&dimension, ChunkPos::new(30, 30));
    assert!(
        gap.sections().iter().all(|s| s.is_empty())
            || count_names(&gap, |n| n != "minecraft:air") < 400
    );
    let mut outer_columns = 0;
    for i in 0..40 {
        let column = generator.generate(&dimension, ChunkPos::new(90 + i, -90 - i));
        outer_columns += count_names(&column, |name| name == "minecraft:end_stone");
    }
    assert!(outer_columns > 0);
    assert_eq!(
        registries::biome_name(center.sections()[5].biome(0, 0, 0).0),
        Some("minecraft:the_end")
    );
}

#[test]
fn level_types_select_generators() {
    let seed = 3;
    assert!(matches!(
        WorldGenerator::from_level_type("minecraft:normal", seed, ""),
        WorldGenerator::Noise(_)
    ));
    assert!(matches!(
        WorldGenerator::from_level_type("minecraft:large_biomes", seed, ""),
        WorldGenerator::Noise(_)
    ));
    assert!(matches!(
        WorldGenerator::from_level_type("minecraft:amplified", seed, ""),
        WorldGenerator::Noise(_)
    ));
    assert!(matches!(
        WorldGenerator::from_level_type("minecraft:flat", seed, ""),
        WorldGenerator::Flat(_)
    ));
    assert!(matches!(
        WorldGenerator::from_level_type("FLAT", seed, "{}"),
        WorldGenerator::Flat(_)
    ));
    assert!(matches!(
        WorldGenerator::from_level_type("void", seed, ""),
        WorldGenerator::Void(_)
    ));
    assert!(matches!(
        WorldGenerator::from_level_type("minecraft:void", seed, ""),
        WorldGenerator::Void(_)
    ));
    assert!(matches!(
        WorldGenerator::from_level_type("something_else", seed, ""),
        WorldGenerator::Noise(_)
    ));
    assert!(matches!(
        WorldGenerator::from_level_type("", seed, ""),
        WorldGenerator::Noise(_)
    ));
}

#[test]
fn flat_level_type_reads_generator_settings_and_falls_back_to_default() {
    let dimension = Dimension::overworld();
    let custom = WorldGenerator::from_level_type(
        "minecraft:flat",
        1,
        r#"{"layers":[{"block":"minecraft:sandstone","height":2}],"biome":"minecraft:desert"}"#,
    );
    let column = custom.generate(&dimension, ChunkPos::new(0, 0));
    assert_eq!(name_at(&column, 0, -64, 0), "minecraft:sandstone");
    assert_eq!(name_at(&column, 0, -62, 0), "minecraft:air");
    let broken = WorldGenerator::from_level_type("minecraft:flat", 1, "{not json");
    let fallback = broken.generate(&dimension, ChunkPos::new(0, 0));
    assert_eq!(name_at(&fallback, 0, -61, 0), "minecraft:grass_block");
    let unknown_block = WorldGenerator::from_level_type(
        "minecraft:flat",
        1,
        r#"{"layers":[{"block":"minecraft:not_a_block","height":2}]}"#,
    );
    assert_eq!(
        name_at(
            &unknown_block.generate(&dimension, ChunkPos::new(0, 0)),
            0,
            -61,
            0
        ),
        "minecraft:grass_block"
    );
    let default = FlatSettings::vanilla();
    assert_eq!(default.layers.len(), 3);
}

#[test]
fn amplified_is_taller_and_large_biomes_are_calmer() {
    let dimension = Dimension::overworld();
    let normal = NoiseGenerator::new(12);
    let amplified = NoiseGenerator::with_options(12, NoiseOptions::AMPLIFIED);
    let large = NoiseGenerator::with_options(12, NoiseOptions::LARGE_BIOMES);
    let tallest = |generator: &NoiseGenerator| {
        let mut best = 0;
        for cz in -16..16 {
            for cx in -16..16 {
                let column = generator.generate(&dimension, ChunkPos::new(cx * 6, cz * 6));
                best = best.max(height_of(&column, HeightmapKind::WorldSurface, 8, 8));
            }
        }
        best
    };
    assert!(tallest(&amplified) > tallest(&normal));
    let transitions = |generator: &NoiseGenerator| {
        let mut previous = None;
        let mut count = 0;
        for cx in -150..150 {
            let column = generator.generate(&dimension, ChunkPos::new(cx, 40));
            let id = column.sections()[12].biome(0, 0, 0);
            if previous.is_some_and(|p| p != id) {
                count += 1;
            }
            previous = Some(id);
        }
        count
    };
    assert!(transitions(&large) < transitions(&normal));
}

#[test]
fn custom_dimension_heights_are_respected() {
    use rspigot_domain::world::WorldHeight;
    let generator = NoiseGenerator::new(5);
    let mut dimension = Dimension::overworld();
    dimension.height = WorldHeight::new(-32, 192).unwrap();
    dimension.sea_level = 40;
    let column = generator.generate(&dimension, ChunkPos::new(2, 2));
    assert_eq!(column.sections().len(), 12);
    assert_eq!(name_at(&column, 3, -32, 3), "minecraft:bedrock");
    let spawn = generator.spawn_point(&dimension);
    assert!(spawn.y > f64::from(dimension.sea_level) - 1.0);
    assert!(spawn.y <= f64::from(dimension.height.max_y()));
}

#[test]
fn emeralds_only_appear_inside_mountain_biomes() {
    const MOUNTAINS: [&str; 8] = [
        "minecraft:windswept_gravelly_hills",
        "minecraft:windswept_forest",
        "minecraft:windswept_savanna",
        "minecraft:jagged_peaks",
        "minecraft:frozen_peaks",
        "minecraft:stony_peaks",
        "minecraft:snowy_slopes",
        "minecraft:windswept_hills",
    ];
    let generator = NoiseGenerator::new(12345);
    let dimension = Dimension::overworld();
    let near_surface = |column: &rspigot_domain::chunk::ChunkColumn, x: usize, z: usize| {
        let top = top_y(column, HeightmapKind::WorldSurface, x, z);
        let min_y = column.height().min_y;
        (-24..=14).any(|shift| {
            let y = (top + shift).clamp(min_y, column.height().max_y() - 1);
            let layer = ((y - min_y) >> 2) as usize;
            let id = column.sections()[layer >> 2].biome(x >> 2, layer & 3, z >> 2);
            MOUNTAINS.contains(&registries::biome_name(id.0).unwrap())
        })
    };
    let mut emeralds = 0;
    let mut highest = i32::MIN;
    let mut mountain_chunks = Vec::new();
    for cz in -20..20 {
        for cx in -20..20 {
            let pos = ChunkPos::new(cx * 4, cz * 4);
            if near_surface(&generator.generate(&dimension, pos), 8, 8) {
                mountain_chunks.push(pos);
            }
        }
    }
    assert!(!mountain_chunks.is_empty());
    for pos in mountain_chunks.iter().take(150) {
        let column = generator.generate(&dimension, *pos);
        let min_y = column.height().min_y;
        for (index, section) in column.sections().iter().enumerate() {
            for i in 0..4096usize {
                let name = section.blocks().get(i).kind().unwrap().name();
                if name.ends_with("emerald_ore") {
                    let (x, y, z) = (i & 15, (i >> 8) & 15, (i >> 4) & 15);
                    let (cx, cz) = ((x >> 2) as i32, (z >> 2) as i32);
                    let layer = ((index * 16 + y) >> 2) as i32;
                    let layers = column.sections().len() as i32 * 4;
                    let inside_mountain = (-1..=1).any(|dl| {
                        (-1..=1).any(|dz| {
                            (-1..=1).any(|dx| {
                                let (nx, nz) = (
                                    (cx + dx).clamp(0, 3) as usize,
                                    (cz + dz).clamp(0, 3) as usize,
                                );
                                let l = (layer + dl).clamp(0, layers - 1) as usize;
                                let id = column.sections()[l >> 2].biome(nx, l & 3, nz);
                                MOUNTAINS.contains(&registries::biome_name(id.0).unwrap())
                            })
                        })
                    });
                    assert!(inside_mountain || !(4..12).contains(&x) || !(4..12).contains(&z));
                    emeralds += 1;
                    highest = highest.max(min_y + index as i32 * 16 + y as i32);
                }
            }
        }
    }
    assert!(emeralds > 0);
    assert!(highest >= 80, "{highest}");
}

#[test]
fn generated_chunks_store_the_vanilla_biome_of_each_quart_cell() {
    const VANILLA: [(i32, i32, i32, &str); 14] = [
        (-513, -14, -1908, "minecraft:plains"),
        (1424, -11, -1494, "minecraft:savanna"),
        (-146, 10, 2691, "minecraft:river"),
        (-2679, 71, 1831, "minecraft:old_growth_birch_forest"),
        (747, 52, 2560, "minecraft:lukewarm_ocean"),
        (2090, -9, 3978, "minecraft:lukewarm_ocean"),
        (2017, 20, 17, "minecraft:swamp"),
        (-835, 76, -3192, "minecraft:jungle"),
        (-1239, 76, 1402, "minecraft:forest"),
        (2647, 16, 3715, "minecraft:old_growth_birch_forest"),
        (-1311, -3, 1292, "minecraft:flower_forest"),
        (-2871, 22, -2293, "minecraft:old_growth_birch_forest"),
        (-3054, -15, 543, "minecraft:river"),
        (-367, -5, -1116, "minecraft:dripstone_caves"),
    ];
    let generator = NoiseGenerator::new(12345);
    let dimension = Dimension::overworld();
    for (qx, qy, qz, expected) in VANILLA {
        let column = generator.generate(&dimension, ChunkPos::new(qx >> 2, qz >> 2));
        let layer = (qy + 16) as usize;
        let section = &column.sections()[layer >> 2];
        let id = section.biome((qx & 3) as usize, layer & 3, (qz & 3) as usize);
        assert_eq!(
            registries::biome_name(id.0),
            Some(expected),
            "{qx} {qy} {qz}"
        );
    }
}

#[test]
fn cave_biomes_only_exist_below_the_surface() {
    const CAVES: [&str; 3] = [
        "minecraft:lush_caves",
        "minecraft:dripstone_caves",
        "minecraft:deep_dark",
    ];
    let generator = NoiseGenerator::new(12345);
    let dimension = Dimension::overworld();
    let mut found = 0;
    for cz in -20..20 {
        for cx in -20..20 {
            let column = generator.generate(&dimension, ChunkPos::new(cx * 3, cz * 3));
            let surface = top_y(&column, HeightmapKind::WorldSurface, 8, 8);
            for (index, section) in column.sections().iter().enumerate() {
                let section_top = column.height().min_y + index as i32 * 16 + 15;
                for y in 0..4 {
                    let name = registries::biome_name(section.biome(2, y, 2).0).unwrap();
                    if CAVES.contains(&name) {
                        found += 1;
                        assert!(
                            section_top - 12 + y as i32 * 4 < surface + 8,
                            "{name} {section_top} {surface}"
                        );
                    }
                }
            }
        }
    }
    assert!(found > 0);
}
