use super::*;
use crate::overworld::features::tests::Bench;
use crate::overworld::region::Region;
use crate::resources::BlockKind;
use rspigot_blocks::BlockKind as Kind;

fn positions(region: &mut Region, name: &str) -> Vec<Pos> {
    let mut found = Vec::new();
    for x in -24..40 {
        for z in -24..40 {
            for y in 40..130 {
                if region.get(x, y, z).kind().map(Kind::name) == Some(name) {
                    found.push((x, y, z));
                }
            }
        }
    }
    found
}

fn cells(region: &mut Region) -> Vec<(Pos, BlockStateId)> {
    let mut states = Vec::new();
    for x in -24..40 {
        for z in -24..40 {
            for y in 40..130 {
                states.push(((x, y, z), region.get(x, y, z)));
            }
        }
    }
    states
}

fn snapshot(region: &mut Region) -> Vec<BlockStateId> {
    cells(region).into_iter().map(|(_, state)| state).collect()
}

struct Shape {
    feature: &'static str,
    log: &'static str,
    leaves: &'static str,
    rise: (i32, i32),
}

const SHAPES: [Shape; 13] = [
    Shape {
        feature: "cherry",
        log: "minecraft:cherry_log",
        leaves: "minecraft:cherry_leaves",
        rise: (4, 8),
    },
    Shape {
        feature: "cherry_bees_005",
        log: "minecraft:cherry_log",
        leaves: "minecraft:cherry_leaves",
        rise: (4, 8),
    },
    Shape {
        feature: "azalea_tree",
        log: "minecraft:oak_log",
        leaves: "minecraft:azalea_leaves",
        rise: (4, 6),
    },
    Shape {
        feature: "mangrove",
        log: "minecraft:mangrove_log",
        leaves: "minecraft:mangrove_leaves",
        rise: (2, 14),
    },
    Shape {
        feature: "tall_mangrove",
        log: "minecraft:mangrove_log",
        leaves: "minecraft:mangrove_leaves",
        rise: (4, 28),
    },
    Shape {
        feature: "orange_poplar",
        log: "minecraft:poplar_log",
        leaves: "minecraft:orange_poplar_leaves",
        rise: (6, 10),
    },
    Shape {
        feature: "red_poplar",
        log: "minecraft:poplar_log",
        leaves: "minecraft:red_poplar_leaves",
        rise: (6, 10),
    },
    Shape {
        feature: "yellow_poplar",
        log: "minecraft:poplar_log",
        leaves: "minecraft:yellow_poplar_leaves",
        rise: (6, 10),
    },
    Shape {
        feature: "orange_poplar_leaf_litter",
        log: "minecraft:poplar_log",
        leaves: "minecraft:orange_poplar_leaves",
        rise: (6, 10),
    },
    Shape {
        feature: "red_poplar_leaf_litter",
        log: "minecraft:poplar_log",
        leaves: "minecraft:red_poplar_leaves",
        rise: (6, 10),
    },
    Shape {
        feature: "yellow_poplar_leaf_litter",
        log: "minecraft:poplar_log",
        leaves: "minecraft:yellow_poplar_leaves",
        rise: (6, 10),
    },
    Shape {
        feature: "pale_oak",
        log: "minecraft:pale_oak_log",
        leaves: "minecraft:pale_oak_leaves",
        rise: (5, 10),
    },
    Shape {
        feature: "pale_oak_creaking",
        log: "minecraft:pale_oak_log",
        leaves: "minecraft:pale_oak_leaves",
        rise: (5, 10),
    },
];

fn ground<'a>(bench: &'a Bench, feature: &str) -> Region<'a> {
    if feature.contains("mangrove") {
        swamp(bench)
    } else {
        bench.floor(BlockKind::GrassBlock, 69)
    }
}

#[test]
fn new_tree_families_grow_on_soil_within_their_height_bounds() {
    let bench = Bench::new();
    for shape in &SHAPES {
        let mut grown = 0;
        let mut region_count = 0;
        for seed in 0..12 {
            region_count += 1;
            let mut region = ground(&bench, shape.feature);
            if !bench.place(&mut region, shape.feature, (8, 70, 8), seed) {
                continue;
            }
            grown += 1;
            let logs = positions(&mut region, shape.log);
            let leaves = positions(&mut region, shape.leaves);
            assert!(
                !logs.is_empty(),
                "{} seed {seed} has no trunk",
                shape.feature
            );
            assert!(
                !leaves.is_empty(),
                "{} seed {seed} has no leaves",
                shape.feature
            );
            let top = logs.iter().map(|pos| pos.1).max().unwrap_or(0) - 70;
            assert!(
                (shape.rise.0..=shape.rise.1).contains(&top),
                "{} seed {seed} trunk rises {top}",
                shape.feature
            );
        }
        let needed = if shape.feature == "tall_mangrove" {
            3
        } else {
            10
        };
        assert!(
            grown >= needed,
            "{} grew {grown} of {region_count}",
            shape.feature
        );
    }
}

#[test]
fn new_tree_families_are_deterministic_per_seed() {
    let bench = Bench::new();
    for shape in &SHAPES {
        let run = |seed: i64| {
            let mut region = ground(&bench, shape.feature);
            bench.place(&mut region, shape.feature, (8, 70, 8), seed);
            snapshot(&mut region)
        };
        assert_eq!(run(3), run(3), "{}", shape.feature);
        assert_ne!(run(3), run(4), "{}", shape.feature);
    }
}

#[test]
fn azalea_trunks_lean_away_from_the_vertical() {
    let bench = Bench::new();
    for seed in 0..12 {
        let mut region = bench.floor(BlockKind::GrassBlock, 69);
        assert!(bench.place(&mut region, "azalea_tree", (8, 70, 8), seed));
        let logs = positions(&mut region, "minecraft:oak_log");
        let reach = logs
            .iter()
            .map(|pos| (pos.0 - 8).abs().max((pos.2 - 8).abs()))
            .max()
            .unwrap_or(0);
        assert!((1..=4).contains(&reach), "seed {seed} reaches {reach}");
        assert!(logs.iter().any(|pos| (pos.0, pos.2) == (8, 8)));
    }
}

fn swamp(bench: &Bench) -> Region<'_> {
    let mut region = bench.floor(BlockKind::Dirt, 69);
    let water = bench.resources.blocks[BlockKind::Water];
    let mud = crate::datapack::state_named("minecraft:mud", &[]).unwrap_or(water);
    for x in -24..40 {
        for z in -24..40 {
            for y in 67..70 {
                region.set(x, y, z, mud);
            }
            for y in 70..76 {
                region.set(x, y, z, water);
            }
        }
    }
    region
}

fn grow_mangrove<'a>(bench: &'a Bench, feature: &str, seed: i64) -> Option<Region<'a>> {
    let mut region = swamp(bench);
    bench
        .place(&mut region, feature, (8, 70, 8), seed)
        .then_some(region)
}

#[test]
fn mangroves_root_in_water_and_mud() {
    let bench = Bench::new();
    for feature in ["mangrove", "tall_mangrove"] {
        let mut muddy = 0;
        let mut wet = 0;
        for seed in 0..12 {
            let Some(mut region) = grow_mangrove(&bench, feature, seed) else {
                continue;
            };
            let roots = positions(&mut region, "minecraft:mangrove_roots");
            assert!(!roots.is_empty(), "{feature} seed {seed} has no roots");
            muddy += positions(&mut region, "minecraft:muddy_mangrove_roots").len();
            wet += roots
                .iter()
                .filter(|pos| {
                    region.get(pos.0, pos.1, pos.2).property("waterlogged") == Some("true")
                })
                .count();
        }
        assert!(muddy > 0, "{feature} never muddied a root");
        assert!(wet > 0, "{feature} roots are never waterlogged");
    }
}

#[test]
fn mangroves_hang_propagules_under_their_leaves() {
    let bench = Bench::new();
    let mut hanging = 0;
    for seed in 0..40 {
        let Some(mut region) = grow_mangrove(&bench, "mangrove", seed) else {
            continue;
        };
        for pos in positions(&mut region, "minecraft:mangrove_propagule") {
            let state = region.get(pos.0, pos.1, pos.2);
            assert_eq!(state.property("hanging"), Some("true"));
            let above = region.get(pos.0, pos.1 + 1, pos.2);
            assert_eq!(
                above.kind().map(Kind::name),
                Some("minecraft:mangrove_leaves")
            );
            hanging += 1;
        }
    }
    assert!(hanging > 0);
}

#[test]
fn mangrove_trunks_stay_within_the_configured_height() {
    let bench = Bench::new();
    for seed in 0..12 {
        let Some(mut region) = grow_mangrove(&bench, "mangrove", seed) else {
            continue;
        };
        let logs = positions(&mut region, "minecraft:mangrove_log");
        let column: Vec<i32> = logs
            .iter()
            .filter(|pos| (pos.0, pos.2) == (8, 8))
            .map(|pos| pos.1)
            .collect();
        let low = column.iter().copied().min().unwrap_or(0);
        assert!((71..=73).contains(&low), "trunk starts at {low}");
        assert!(
            (2..=7).contains(&column.len()),
            "trunk is {} tall",
            column.len()
        );
    }
}

fn describe(state: BlockStateId) -> String {
    let mut text = state
        .kind()
        .map_or("air", Kind::name)
        .trim_start_matches("minecraft:")
        .to_owned();
    for key in [
        "age",
        "axis",
        "creaking_heart_state",
        "distance",
        "facing",
        "hanging",
        "natural",
        "persistent",
        "segment_amount",
        "tip",
        "waterlogged",
    ] {
        if let Some(value) = state.property(key) {
            text.push_str(&format!(" {key}={value}"));
        }
    }
    text
}

fn fingerprint(bench: &Bench, feature: &str, seed: i64) -> (usize, u64) {
    let mut region = ground(bench, feature);
    let before = cells(&mut region);
    bench.place(&mut region, feature, (8, 70, 8), seed);
    let after = cells(&mut region);
    let mut lines: Vec<String> = before
        .iter()
        .zip(&after)
        .filter(|(old, new)| old.1 != new.1)
        .map(|(_, new)| {
            let (x, y, z) = new.0;
            format!("{x},{y},{z} {}", describe(new.1))
        })
        .collect();
    lines.sort();
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for line in &lines {
        for byte in line.bytes().chain(std::iter::once(b'\n')) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    (lines.len(), hash)
}

const REFERENCE: [(&str, i64, usize, u64); 116] = [
    ("acacia", 1, 92, 0xc2385bb24eb84a74),
    ("acacia", 2, 94, 0x1f983ae25b632b84),
    ("birch", 1, 60, 0xce6936f52009cf57),
    ("birch", 2, 64, 0x2db97713236867a9),
    ("birch_bees_0002", 1, 60, 0xce6936f52009cf57),
    ("birch_bees_0002", 2, 64, 0x2db97713236867a9),
    ("birch_leaf_litter", 1, 89, 0xf246f39cfa6250dd),
    ("birch_leaf_litter", 2, 87, 0x466d5c00851c862f),
    ("dark_oak", 1, 180, 0xd52d87f162cd438b),
    ("dark_oak", 2, 157, 0x7e973d7ad7ebcc20),
    ("dark_oak_leaf_litter", 1, 205, 0xbe4d6d97aae48738),
    ("dark_oak_leaf_litter", 2, 184, 0x1f9e5cb4deb4f5ae),
    ("fancy_oak", 1, 78, 0x134e2d7d2febc3f0),
    ("fancy_oak", 2, 162, 0x83e174d93b34b6d7),
    ("fancy_oak_bees_005", 1, 78, 0x134e2d7d2febc3f0),
    ("fancy_oak_bees_005", 2, 162, 0x83e174d93b34b6d7),
    ("jungle_bush", 1, 30, 0x74e8dbd6eb649533),
    ("jungle_bush", 2, 31, 0xf21670d1fb309f56),
    ("jungle_tree", 1, 119, 0x850b4593e1296de9),
    ("jungle_tree", 2, 129, 0x11a339ae2cd4fcfd),
    ("jungle_tree_no_vine", 1, 61, 0xfa96cffa95cc0f8b),
    ("jungle_tree_no_vine", 2, 66, 0x5e53c6d9309e9d79),
    ("mega_jungle_tree", 1, 475, 0x1b724c207ec04cdf),
    ("mega_jungle_tree", 2, 471, 0x6595409aac66ec6c),
    ("mega_pine", 1, 243, 0x5b5314969fbd62ee),
    ("mega_pine", 2, 246, 0x6dfdbc278bfa1113),
    ("mega_spruce", 1, 463, 0x639111099d27bed7),
    ("mega_spruce", 2, 474, 0x104f27ab08986180),
    ("oak", 1, 59, 0xdce38fcff4c1916e),
    ("oak", 2, 63, 0xe2f7cbedb9e061b2),
    ("oak_bees_005", 1, 59, 0xdce38fcff4c1916e),
    ("oak_bees_005", 2, 63, 0xe2f7cbedb9e061b2),
    ("oak_leaf_litter", 1, 88, 0x945aad59729a1098),
    ("oak_leaf_litter", 2, 86, 0xed4648de52f48a40),
    ("pine", 1, 39, 0xcfdf72ad2d3333c7),
    ("pine", 2, 39, 0xcfdf72ad2d3333c7),
    ("spruce", 1, 60, 0x96ffaff31ca002e7),
    ("spruce", 2, 60, 0x96ffaff31ca002e7),
    ("super_birch_bees", 1, 62, 0x41640715ed8bdd5),
    ("super_birch_bees", 2, 65, 0x36b2df5cac9b73e4),
    ("swamp_oak", 1, 197, 0xcf4e2bd9c4d1f051),
    ("swamp_oak", 2, 205, 0xdc99b0626fca1648),
    ("cherry", 1, 150, 0xdd6eb43efcaa6847),
    ("cherry", 2, 148, 0x812c1fff4ab7d64d),
    ("cherry", 3, 401, 0x3207cc5895e40fe6),
    ("cherry", 4, 305, 0x66c3d8d8c3f5b214),
    ("cherry", 5, 153, 0x545201570883fbb0),
    ("cherry", 6, 289, 0x7bfc3e9b514b502f),
    ("cherry_bees_005", 1, 150, 0xdd6eb43efcaa6847),
    ("cherry_bees_005", 2, 148, 0x812c1fff4ab7d64d),
    ("cherry_bees_005", 3, 402, 0xf539783adc80a86c),
    ("cherry_bees_005", 4, 305, 0x66c3d8d8c3f5b214),
    ("cherry_bees_005", 5, 153, 0x545201570883fbb0),
    ("cherry_bees_005", 6, 289, 0x7bfc3e9b514b502f),
    ("azalea_tree", 1, 97, 0x48f7f6c66a3b30cf),
    ("azalea_tree", 2, 107, 0x1037da9e72f33db7),
    ("azalea_tree", 3, 125, 0x5baa0679a23ac206),
    ("azalea_tree", 4, 105, 0x78cbca49e1968167),
    ("azalea_tree", 5, 92, 0xe529fa3f7dcd2f5b),
    ("azalea_tree", 6, 75, 0x6a259e8c3c0c8c0),
    ("mangrove", 1, 66, 0xb4c3fcf274e1bfd2),
    ("mangrove", 2, 62, 0x9e98fd73b9b49080),
    ("mangrove", 3, 322, 0x783c4a94ef7cc05a),
    ("mangrove", 4, 78, 0xf4db2cff986e6eb1),
    ("tall_mangrove", 1, 373, 0xc6bbee63b652564d),
    ("tall_mangrove", 2, 290, 0xf0ce290f1c0057cb),
    ("tall_mangrove", 3, 0, 0xcbf29ce484222325),
    ("tall_mangrove", 4, 513, 0x6193c4211a08ba8e),
    ("orange_poplar", 1, 227, 0x5489b4a5829b7859),
    ("orange_poplar", 2, 234, 0xdedb29b066d2329b),
    ("orange_poplar", 3, 232, 0x6b07f82ef702d5fc),
    ("orange_poplar", 4, 153, 0xfd21c1dacc71a77d),
    ("orange_poplar", 5, 159, 0x92454f708178b3dc),
    ("orange_poplar", 6, 297, 0x734da067ee9016b7),
    ("red_poplar", 1, 227, 0x97d01e4c84a613ae),
    ("red_poplar", 2, 234, 0x1dffbaf2bc4922a4),
    ("red_poplar", 3, 232, 0xbd0a30bbd8112131),
    ("red_poplar", 4, 153, 0xafb54af88cfb3e16),
    ("red_poplar", 5, 159, 0x406de3feb8104921),
    ("red_poplar", 6, 297, 0x3470046e3c8c3f42),
    ("yellow_poplar", 1, 227, 0x4e6a9369bb570223),
    ("yellow_poplar", 2, 234, 0x6593d9cc688ebef5),
    ("yellow_poplar", 3, 232, 0x2652e0a2b06dc69e),
    ("yellow_poplar", 4, 153, 0x803fc7ea855fd4f),
    ("yellow_poplar", 5, 159, 0x32c6f2dcfe80fe66),
    ("yellow_poplar", 6, 297, 0xddb3b18110e7e4ad),
    ("orange_poplar_leaf_litter", 1, 248, 0x42b38f52c9d7497a),
    ("orange_poplar_leaf_litter", 2, 262, 0xb81c3da6136e9ed1),
    ("orange_poplar_leaf_litter", 3, 256, 0x8ce88852224ea9ac),
    ("orange_poplar_leaf_litter", 4, 177, 0x3be69dcb618aadb8),
    ("orange_poplar_leaf_litter", 5, 183, 0x7924b907b742e49a),
    ("orange_poplar_leaf_litter", 6, 326, 0x44fd020adc8dd1ad),
    ("red_poplar_leaf_litter", 1, 248, 0xea06bf345831fa13),
    ("red_poplar_leaf_litter", 2, 262, 0x9dd74c113cbd5dbc),
    ("red_poplar_leaf_litter", 3, 256, 0x97602106c70a390d),
    ("red_poplar_leaf_litter", 4, 177, 0x32d59fa2ee6c0ff7),
    ("red_poplar_leaf_litter", 5, 183, 0xd862c94f33385f1d),
    ("red_poplar_leaf_litter", 6, 326, 0xc95b51e76a499122),
    ("yellow_poplar_leaf_litter", 1, 248, 0xec75ce2c39ae6628),
    ("yellow_poplar_leaf_litter", 2, 262, 0xdca0d938321e6293),
    ("yellow_poplar_leaf_litter", 3, 256, 0x97c1b73166af305e),
    ("yellow_poplar_leaf_litter", 4, 177, 0x5efcf93663a40c6a),
    ("yellow_poplar_leaf_litter", 5, 183, 0x92bb2e330c5a8784),
    ("yellow_poplar_leaf_litter", 6, 326, 0x7b980dbf612fedf3),
    ("pale_oak", 1, 211, 0x98245722b41b33f0),
    ("pale_oak", 2, 177, 0xd825cce59bda306d),
    ("pale_oak", 3, 185, 0xed4dbfdf02d33f7b),
    ("pale_oak", 4, 171, 0xaee5d0afe062add1),
    ("pale_oak", 5, 175, 0x1863f2ebd6f07480),
    ("pale_oak", 6, 171, 0x417cc746d5016853),
    ("pale_oak_creaking", 1, 211, 0x98245722b41b33f0),
    ("pale_oak_creaking", 2, 177, 0xd825cce59bda306d),
    ("pale_oak_creaking", 3, 185, 0xed4dbfdf02d33f7b),
    ("pale_oak_creaking", 4, 171, 0x78136f24310337e),
    ("pale_oak_creaking", 5, 175, 0x1863f2ebd6f07480),
    ("pale_oak_creaking", 6, 171, 0x417cc746d5016853),
];

#[test]
fn trees_match_the_vanilla_feature_cell_for_cell() {
    let bench = Bench::new();
    for (feature, seed, count, hash) in REFERENCE {
        assert_eq!(
            fingerprint(&bench, feature, seed),
            (count, hash),
            "{feature} seed {seed}"
        );
    }
}
