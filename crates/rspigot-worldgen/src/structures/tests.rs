use super::library::Library;
use super::placement::BiomeProbe;
use super::sets::{Chunks, StructureSets};
use crate::climate::BiomeSource;
use rspigot_data::multi_noise::{ClimateVariant, overworld_biomes};
use std::cell::RefCell;

struct Overworld<'a> {
    source: &'a BiomeSource,
    scratch: RefCell<crate::climate::ClimateScratch>,
}

impl BiomeProbe for Overworld<'_> {
    fn at(&self, qx: i32, qy: i32, qz: i32) -> u8 {
        self.source
            .biome_index(&mut self.scratch.borrow_mut(), qx, qy, qz)
    }

    fn at_after(&self, qx: i32, qy: i32, qz: i32, last: &mut Option<usize>) -> u8 {
        self.source
            .biome_index_after(&mut self.scratch.borrow_mut(), last, qx, qy, qz)
    }

    fn id(&self, name: &str) -> Option<u8> {
        overworld_biomes()
            .iter()
            .position(|candidate| candidate.strip_prefix("minecraft:") == Some(name))
            .map(|index| index as u8)
    }
}

pub(super) fn with_extras() -> &'static Library {
    static FULL: std::sync::OnceLock<&'static Library> = std::sync::OnceLock::new();
    FULL.get_or_init(|| {
        let pack =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../datapacks/vanilla-extras");
        let (library, issues) = Library::build(&[pack]);
        assert!(
            issues
                .iter()
                .all(|issue| issue.reason.contains("the missing template")),
            "{issues:?}"
        );
        Box::leak(Box::new(library))
    })
}

type Expected = (Vec<(String, i32, i32)>, Vec<(i32, i32)>);

fn expected(fixture: &str) -> Expected {
    let mut chunks = Vec::new();
    let mut rings = Vec::new();
    for line in fixture.lines() {
        let parts: Vec<&str> = line.split(' ').collect();
        match parts[0] {
            "CHUNK" => chunks.push((
                parts[1].to_owned(),
                parts[2].parse().unwrap(),
                parts[3].parse().unwrap(),
            )),
            "RINGS" => {
                for pair in &parts[2..] {
                    let (x, z) = pair.split_once(',').unwrap();
                    rings.push((x.parse().unwrap(), z.parse().unwrap()));
                }
            }
            _ => {}
        }
    }
    (chunks, rings)
}

fn check(seed: i64, fixture: &str) {
    let (chunks, rings) = expected(fixture);
    let source = BiomeSource::new(seed, ClimateVariant::Normal);
    let probe = Overworld {
        scratch: RefCell::new(source.scratch()),
        source: &source,
    };
    let sets = StructureSets::load(Library::embedded());
    let world = Chunks {
        sets: &sets,
        seed,
        biomes: &probe,
    };
    let mut got = Vec::new();
    for set in sets.all() {
        if set.name == "strongholds" {
            let ours = set.placement.rings(seed, &probe);
            assert_eq!(ours.len(), rings.len());
            let differing = ours.iter().zip(&rings).filter(|(a, b)| a != b).count();
            assert_eq!(differing, 0, "{differing} rings differ");
            continue;
        }
        let in_world = chunks.iter().any(|(name, _, _)| *name == set.name);
        if !in_world {
            continue;
        }
        for x in -60..=60 {
            for z in -60..=60 {
                if world.is_structure_chunk(set, x, z) {
                    got.push((set.name.clone(), x, z));
                }
            }
        }
    }
    got.sort();
    let mut want = chunks;
    want.sort();
    assert_eq!(got, want);
}

#[test]
fn placement_matches_vanilla_for_seed_one() {
    check(1, include_str!("fixtures/placement_1.txt"));
}

#[test]
fn placement_matches_vanilla_for_a_negative_seed() {
    check(
        -4172144997902289642,
        include_str!("fixtures/placement_-4172144997902289642.txt"),
    );
}

mod jigsaw {
    use super::super::jigsaw::Piece;
    use super::super::pool::{Element, Projection};
    use super::super::structure::{Kind, Structures, generate_jigsaw, valid_biomes};
    use super::with_extras;
    use crate::overworld::Overworld;
    use crate::resources::Resources;
    use rspigot_data::multi_noise::ClimateVariant;
    use rspigot_domain::world::Dimension;

    fn describe(piece: &Piece) -> String {
        let rotation = match piece.rotation {
            crate::overworld::features::structure::Rotation::None => "NONE",
            crate::overworld::features::structure::Rotation::Clockwise90 => "CLOCKWISE_90",
            crate::overworld::features::structure::Rotation::Clockwise180 => "CLOCKWISE_180",
            crate::overworld::features::structure::Rotation::Counterclockwise90 => {
                "COUNTERCLOCKWISE_90"
            }
        };
        let label = match piece.element {
            Element::Single {
                template, legacy, ..
            } => format!(
                "{}Single[Left[{template}]]",
                if *legacy { "Legacy" } else { "" }
            ),
            Element::Feature { feature, .. } => {
                format!("Feature[{feature}]")
            }
            Element::List { elements, .. } => {
                let parts: Vec<String> = elements
                    .iter()
                    .map(|inner| match inner {
                        Element::Single {
                            template, legacy, ..
                        } => format!(
                            "{}Single[Left[{template}]]",
                            if *legacy { "Legacy" } else { "" }
                        ),
                        _ => "?".to_owned(),
                    })
                    .collect();
                format!("List[{}]", parts.join(", "))
            }
            _ => "?".to_owned(),
        };
        let _ = Projection::Rigid;
        let (b, p) = (piece.bbox, piece.pos);
        format!(
            "{label} {} {} {} {rotation} {} {} {} {} {} {} {} {}",
            p.0,
            p.1,
            p.2,
            b.min.0,
            b.min.1,
            b.min.2,
            b.max.0,
            b.max.1,
            b.max.2,
            piece.ground_level_delta,
            piece.junctions.len()
        )
    }

    fn check(seed: i64, name: &str, chunk: (i32, i32), fixture: &str) {
        let resources = Resources::resolve();
        let overworld = Overworld::with_library(
            seed as u64,
            ClimateVariant::Normal,
            &resources.blocks,
            with_extras(),
        );
        let dimension = Dimension::overworld();
        let probe = overworld.biome_probe();
        let ground = overworld.ground(&dimension);
        let structures = Structures::load(with_extras());
        let def = structures.get(name).unwrap();
        let Kind::Jigsaw(jigsaw) = &def.kind else {
            panic!("not a jigsaw structure");
        };
        let pools = with_extras().pools();
        let valid = valid_biomes(def, &probe);
        let assembly = generate_jigsaw(jigsaw, &valid, pools, &ground, &probe, seed, chunk)
            .expect("start must be valid");
        let got: Vec<String> = assembly.pieces.iter().map(describe).collect();
        let want: Vec<String> = fixture
            .lines()
            .filter(|line| line.starts_with("PIECE"))
            .map(|line| line.splitn(3, ' ').nth(2).unwrap().to_owned())
            .collect();
        for (index, (got, want)) in got.iter().zip(&want).enumerate() {
            assert_eq!(got, want, "piece {index}");
        }
        assert_eq!(got.len(), want.len(), "piece count");
    }

    #[test]
    fn plains_village_matches_vanilla_for_seed_one() {
        check(
            1,
            "village_plains",
            (40, 51),
            include_str!("fixtures/jigsaw_village_plains_1_40_51.txt"),
        );
    }

    #[test]
    fn snowy_village_matches_vanilla_for_seed_one() {
        check(
            1,
            "village_snowy",
            (-15, -57),
            include_str!("fixtures/jigsaw_village_snowy_1_-15_-57.txt"),
        );
    }

    #[test]
    fn plains_village_matches_vanilla_for_a_negative_seed() {
        check(
            -4172144997902289642,
            "village_plains",
            (-47, -48),
            include_str!("fixtures/jigsaw_village_plains_-4172144997902289642_-47_-48.txt"),
        );
    }

    #[test]
    fn pillager_outpost_matches_vanilla_for_seed_one() {
        check(
            1,
            "pillager_outpost",
            (40, 51),
            include_str!("fixtures/jigsaw_pillager_outpost_1_40_51.txt"),
        );
    }

    #[test]
    fn savanna_village_matches_vanilla_for_a_negative_seed() {
        check(
            -4172144997902289642,
            "village_savanna",
            (-32, -23),
            include_str!("fixtures/jigsaw_village_savanna_-4172144997902289642_-32_-23.txt"),
        );
    }

    #[test]
    fn forest_abandoned_camp_matches_vanilla_for_seed_one() {
        check(
            1,
            "abandoned_camp_forest",
            (-280, -72),
            include_str!("fixtures/jigsaw_abandoned_camp_forest_1_-280_-72.txt"),
        );
    }

    #[test]
    fn savanna_abandoned_camp_matches_vanilla_for_seed_one() {
        check(
            1,
            "abandoned_camp_savanna",
            (-291, -125),
            include_str!("fixtures/jigsaw_abandoned_camp_savanna_1_-291_-125.txt"),
        );
    }

    #[test]
    fn ancient_city_matches_vanilla_for_seed_one() {
        check(
            1,
            "ancient_city",
            (-201, -36),
            include_str!("fixtures/jigsaw_ancient_city_1_-201_-36.txt"),
        );
    }
}

mod world {
    use super::with_extras;
    use crate::overworld::Overworld;
    use crate::resources::Resources;
    use rspigot_blocks::BlockKind;
    use rspigot_data::multi_noise::ClimateVariant;
    use rspigot_domain::world::Dimension;
    use rspigot_domain::{BlockPos, ChunkPos};

    #[test]
    fn a_village_is_built_into_the_generated_chunks() {
        let resources = Resources::resolve();
        let overworld =
            Overworld::with_library(1, ClimateVariant::Normal, &resources.blocks, with_extras());
        let dimension = Dimension::overworld();
        let mut bells = 0;
        let mut planks = 0;
        let mut recorded = false;
        let mut chests = 0;
        for cx in 39..=41 {
            for cz in 50..=52 {
                let column = overworld.generate(&resources, &dimension, ChunkPos::new(cx, cz));
                for y in 40..100 {
                    for z in 0..16 {
                        for x in 0..16 {
                            let pos = BlockPos::new(cx * 16 + x, y, cz * 16 + z);
                            match column.block(pos).and_then(|state| state.kind()) {
                                Some(BlockKind::Bell) => bells += 1,
                                Some(BlockKind::OakPlanks) => planks += 1,
                                _ => {}
                            }
                        }
                    }
                }
                chests += column.block_entities.len();
                if (cx, cz) == (40, 51) {
                    recorded = column
                        .extra
                        .get_compound("structures")
                        .and_then(|tag| tag.get_compound("starts"))
                        .is_some_and(|starts| starts.contains_key("minecraft:village_plains"));
                }
            }
        }
        assert!(bells >= 1, "bells {bells}");
        assert!(planks > 100, "planks {planks}");
        assert!(chests > 0, "block entities {chests}");
        assert!(recorded);
    }

    #[test]
    fn village_templates_spawn_their_villagers() {
        let resources = Resources::resolve();
        let overworld =
            Overworld::with_library(1, ClimateVariant::Normal, &resources.blocks, with_extras());
        let dimension = Dimension::overworld();
        let mut villagers = 0;
        for cx in 38..=42 {
            for cz in 49..=53 {
                villagers += overworld
                    .structure_entities(&dimension, ChunkPos::new(cx, cz))
                    .iter()
                    .filter(|entity| entity.get_str("id") == Some("minecraft:villager"))
                    .count();
            }
        }
        assert!(villagers > 0, "villagers {villagers}");
    }

    #[test]
    fn an_abandoned_camp_is_built_into_the_generated_chunks() {
        let resources = Resources::resolve();
        let overworld =
            Overworld::with_library(1, ClimateVariant::Normal, &resources.blocks, with_extras());
        let dimension = Dimension::overworld();
        let mut chests = 0;
        let mut recorded = false;
        for cx in -281..=-278 {
            for cz in -74..=-70 {
                let column = overworld.generate(&resources, &dimension, ChunkPos::new(cx, cz));
                chests += column
                    .block_entities
                    .iter()
                    .filter(|entity| entity.data.get_str("LootTable").is_some())
                    .count();
                if (cx, cz) == (-280, -72) {
                    recorded = column
                        .extra
                        .get_compound("structures")
                        .and_then(|tag| tag.get_compound("starts"))
                        .is_some_and(|starts| {
                            starts.contains_key("minecraft:abandoned_camp_forest")
                        });
                }
            }
        }
        assert!(chests >= 1, "chests {chests}");
        assert!(recorded);
    }
}

mod embedded {
    use super::super::library::Library;
    use super::super::structure::Structures;
    use super::with_extras;

    #[test]
    fn the_binary_only_embeds_villages_and_outposts_among_jigsaw_structures() {
        let embedded = Structures::load(Library::embedded());
        assert!(embedded.get("village_plains").is_some());
        assert!(embedded.get("pillager_outpost").is_some());
        assert!(embedded.get("mineshaft").is_some());
        for gone in [
            "abandoned_camp_forest",
            "ancient_city",
            "bastion_remnant",
            "trial_chambers",
            "trail_ruins",
        ] {
            assert!(embedded.get(gone).is_none(), "{gone}");
        }
    }

    #[test]
    fn the_extras_pack_brings_the_other_structures_back() {
        let full = Structures::load(with_extras());
        for back in [
            "abandoned_camp_forest",
            "ancient_city",
            "bastion_remnant",
            "trial_chambers",
            "trail_ruins",
        ] {
            assert!(full.get(back).is_some(), "{back}");
        }
    }
}

mod custom {
    use super::super::library::Library;
    use crate::overworld::Overworld;
    use crate::resources::Resources;
    use rspigot_blocks::BlockKind;
    use rspigot_data::multi_noise::ClimateVariant;
    use rspigot_domain::world::Dimension;
    use rspigot_domain::{BlockPos, ChunkPos};

    fn library() -> &'static Library {
        static CUSTOM: std::sync::OnceLock<&'static Library> = std::sync::OnceLock::new();
        CUSTOM.get_or_init(|| {
            let pack = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../example/datapacks/custom-hut");
            let (library, issues) = Library::build(&[pack]);
            assert!(issues.is_empty(), "{issues:?}");
            Box::leak(Box::new(library))
        })
    }

    #[test]
    fn the_example_hut_is_built_from_its_template() {
        let resources = Resources::resolve();
        let overworld =
            Overworld::with_library(1, ClimateVariant::Normal, &resources.blocks, library());
        let dimension = Dimension::overworld();
        let mut start = None;
        'search: for cx in -8..8 {
            for cz in -8..8 {
                let column = overworld.generate(&resources, &dimension, ChunkPos::new(cx, cz));
                let recorded = column
                    .extra
                    .get_compound("structures")
                    .and_then(|tag| tag.get_compound("starts"))
                    .is_some_and(|starts| starts.contains_key("example:hut"));
                if recorded {
                    start = Some((cx, cz));
                    break 'search;
                }
            }
        }
        let (sx, sz) = start.expect("the hut must start in the searched area");
        let (mut gold, mut planks) = (0, 0);
        for cx in sx - 1..=sx + 1 {
            for cz in sz - 1..=sz + 1 {
                let column = overworld.generate(&resources, &dimension, ChunkPos::new(cx, cz));
                for y in -64..200 {
                    for z in 0..16 {
                        for x in 0..16 {
                            let pos = BlockPos::new(cx * 16 + x, y, cz * 16 + z);
                            match column.block(pos).and_then(|state| state.kind()) {
                                Some(BlockKind::GoldBlock) => gold += 1,
                                Some(BlockKind::OakPlanks) => planks += 1,
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(gold, 25);
        assert_eq!(planks, 32 + 25);
    }
}

mod nether {
    use super::with_extras;
    use crate::nether::Nether;
    use crate::resources::Resources;
    use rspigot_blocks::BlockKind;
    use rspigot_domain::world::Dimension;
    use rspigot_domain::{BlockPos, ChunkPos};

    #[test]
    fn a_bastion_remnant_is_built_into_the_nether() {
        let resources = Resources::resolve();
        let nether = Nether::with_library(1, &resources.blocks, with_extras());
        let dimension = Dimension::nether();
        let mut origin = None;
        'scan: for cx in -60..60 {
            for cz in -60..60 {
                if nether
                    .starts_near(&dimension, ChunkPos::new(cx, cz))
                    .iter()
                    .any(|start| start.name == "bastion_remnant" && start.chunk == (cx, cz))
                {
                    origin = Some((cx, cz));
                    break 'scan;
                }
            }
        }
        let (ox, oz) = origin.expect("a bastion start");
        let mut bricks = 0;
        let mut recorded = false;
        for cx in ox - 2..=ox + 2 {
            for cz in oz - 2..=oz + 2 {
                let column = nether.generate(&resources, &dimension, ChunkPos::new(cx, cz));
                for y in 0..128 {
                    for z in 0..16 {
                        for x in 0..16 {
                            let pos = BlockPos::new(cx * 16 + x, y, cz * 16 + z);
                            if column.block(pos).and_then(|state| state.kind())
                                == Some(BlockKind::PolishedBlackstoneBricks)
                            {
                                bricks += 1;
                            }
                        }
                    }
                }
                if (cx, cz) == (ox, oz) {
                    recorded = column
                        .extra
                        .get_compound("structures")
                        .and_then(|tag| tag.get_compound("starts"))
                        .is_some_and(|starts| starts.contains_key("minecraft:bastion_remnant"));
                }
            }
        }
        assert!(bricks > 200, "bricks {bricks}");
        assert!(recorded);
    }
}

mod shaped {
    use super::super::blueprint::Dir;
    use super::super::shaped::mineshaft::{self, Piece, Type};
    use super::super::structure::Structures;
    use super::super::structure::valid_biomes;
    use super::with_extras;
    use crate::overworld::Overworld;
    use crate::resources::Resources;
    use rspigot_data::multi_noise::ClimateVariant;
    use rspigot_domain::world::Dimension;

    fn facing(dir: Option<Dir>) -> &'static str {
        match dir {
            None => "null",
            Some(Dir::North) => "north",
            Some(Dir::South) => "south",
            Some(Dir::West) => "west",
            Some(Dir::East) => "east",
            Some(_) => "?",
        }
    }

    fn describe(piece: &Piece) -> String {
        let b = piece.bbox;
        format!(
            "{} {} {} {} {} {} {} {}",
            piece.class(),
            b.min.0,
            b.min.1,
            b.min.2,
            b.max.0,
            b.max.1,
            b.max.2,
            facing(piece.facing)
        )
    }

    fn check_mine(seed: i64, name: &str, ty: Type, chunk: (i32, i32), fixture: &str) {
        let resources = Resources::resolve();
        let overworld = Overworld::with_library(
            seed as u64,
            ClimateVariant::Normal,
            &resources.blocks,
            with_extras(),
        );
        let dimension = Dimension::overworld();
        let probe = overworld.biome_probe();
        let ground = overworld.ground(&dimension);
        let structures = Structures::load(with_extras());
        let valid = valid_biomes(structures.get(name).unwrap(), &probe);
        let pieces = mineshaft::generate(ty, &ground, &valid, &probe, seed, chunk)
            .expect("start must be valid");
        let want: Vec<&str> = fixture
            .lines()
            .filter(|line| line.starts_with("PIECE"))
            .map(|line| line.splitn(3, ' ').nth(2).unwrap())
            .collect();
        for (index, (piece, want)) in pieces.iter().zip(&want).enumerate() {
            assert_eq!(describe(piece), *want, "piece {index}");
        }
        assert_eq!(pieces.len(), want.len(), "piece count");
    }

    #[test]
    fn mineshaft_layout_matches_vanilla_for_seed_one() {
        check_mine(
            1,
            "mineshaft",
            Type::Normal,
            (10, 10),
            include_str!("fixtures/mineshaft_1_10_10.txt"),
        );
    }

    #[test]
    fn mesa_mineshaft_layout_matches_vanilla() {
        check_mine(
            1,
            "mineshaft_mesa",
            Type::Mesa,
            (-3000, 2901),
            include_str!("fixtures/mineshaft_mesa_1_-3000_2901.txt"),
        );
    }

    #[test]
    fn a_mineshaft_is_built_into_the_generated_chunks() {
        use rspigot_blocks::BlockKind;
        use rspigot_domain::{BlockPos, ChunkPos};
        let resources = Resources::resolve();
        let overworld =
            Overworld::with_library(1, ClimateVariant::Normal, &resources.blocks, with_extras());
        let dimension = Dimension::overworld();
        let mut recorded = false;
        let mut cave_air = 0;
        let mut planks = 0;
        for cx in -36..=-30 {
            for cz in 2..=8 {
                let column = overworld.generate(&resources, &dimension, ChunkPos::new(cx, cz));
                if (cx, cz) == (-33, 5) {
                    recorded = column
                        .extra
                        .get_compound("structures")
                        .and_then(|tag| tag.get_compound("starts"))
                        .is_some_and(|starts| starts.contains_key("minecraft:mineshaft"));
                }
                for y in -40..-8 {
                    for x in 0..16 {
                        for z in 0..16 {
                            let state = column
                                .block(BlockPos::new(cx * 16 + x, y, cz * 16 + z))
                                .and_then(|state| state.kind());
                            cave_air += usize::from(state == Some(BlockKind::CaveAir));
                            planks += usize::from(state == Some(BlockKind::OakPlanks));
                        }
                    }
                }
            }
        }
        assert!(recorded);
        assert!(cave_air > 200, "cave air {cave_air}");
        assert!(planks > 10, "planks {planks}");
    }

    fn check_stronghold(seed: i64, chunk: (i32, i32), fixture: &str) {
        use super::super::shaped::stronghold;
        let resources = Resources::resolve();
        let overworld = Overworld::with_library(
            seed as u64,
            ClimateVariant::Normal,
            &resources.blocks,
            with_extras(),
        );
        let dimension = Dimension::overworld();
        let probe = overworld.biome_probe();
        let ground = overworld.ground(&dimension);
        let structures = Structures::load(with_extras());
        let valid = valid_biomes(structures.get("stronghold").unwrap(), &probe);
        let pieces = stronghold::generate(&ground, &valid, &probe, seed, chunk)
            .expect("start must be valid");
        let want: Vec<&str> = fixture
            .lines()
            .filter(|line| line.starts_with("PIECE"))
            .map(|line| line.splitn(3, ' ').nth(2).unwrap())
            .collect();
        for (index, (piece, want)) in pieces.iter().zip(&want).enumerate() {
            let b = piece.bbox;
            let got = format!(
                "{} {} {} {} {} {} {} {}",
                piece.class(),
                b.min.0,
                b.min.1,
                b.min.2,
                b.max.0,
                b.max.1,
                b.max.2,
                facing(Some(piece.facing))
            );
            assert_eq!(got, *want, "piece {index}");
        }
        assert_eq!(pieces.len(), want.len(), "piece count");
    }

    #[test]
    fn stronghold_layout_matches_vanilla_for_the_first_ring() {
        check_stronghold(
            1,
            (-9, -113),
            include_str!("fixtures/stronghold_1_-9_-113.txt"),
        );
    }

    #[test]
    fn a_small_stronghold_matches_vanilla() {
        check_stronghold(
            1,
            (108, 44),
            include_str!("fixtures/stronghold_1_108_44.txt"),
        );
    }

    #[test]
    fn a_stronghold_with_libraries_matches_vanilla() {
        check_stronghold(
            1,
            (-71, 53),
            include_str!("fixtures/stronghold_1_-71_53.txt"),
        );
    }

    #[test]
    fn a_west_facing_stronghold_matches_vanilla() {
        check_stronghold(
            1,
            (165, -260),
            include_str!("fixtures/stronghold_1_165_-260.txt"),
        );
    }

    #[test]
    fn a_stronghold_portal_room_is_built_into_the_generated_chunks() {
        use rspigot_blocks::BlockKind;
        use rspigot_domain::{BlockPos, ChunkPos};
        let resources = Resources::resolve();
        let overworld =
            Overworld::with_library(1, ClimateVariant::Normal, &resources.blocks, with_extras());
        let dimension = Dimension::overworld();
        let mut frames = 0;
        let mut spawners = 0;
        for cx in -7..=-6 {
            let column = overworld.generate(&resources, &dimension, ChunkPos::new(cx, -112));
            spawners += column
                .block_entities
                .iter()
                .filter(|entity| entity.kind.as_str() == "minecraft:mob_spawner")
                .count();
            for y in -44..=-37 {
                for x in 0..16 {
                    for z in 0..16 {
                        let at = BlockPos::new(cx * 16 + x, y, -112 * 16 + z);
                        let kind = column.block(at).and_then(|state| state.kind());
                        frames += usize::from(kind == Some(BlockKind::EndPortalFrame));
                    }
                }
            }
        }
        assert_eq!(frames, 12);
        assert_eq!(spawners, 1);
    }
}
