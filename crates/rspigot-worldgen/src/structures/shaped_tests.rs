use super::jigsaw::Ground;
use super::library::Library;
use super::placement::BiomeProbe;
use super::shaped::Shaped;
use super::structure::{Kind, Structures, valid_biomes};
use crate::nether::Nether;
use crate::overworld::Overworld;
use crate::resources::Resources;
use rspigot_data::multi_noise::ClimateVariant;
use rspigot_domain::world::Dimension;

fn describe(shape: &Shaped) -> String {
    let b = shape.bbox();
    format!(
        "{} {} {} {} {} {} {} {}",
        shape.class(),
        b.min.0,
        b.min.1,
        b.min.2,
        b.max.0,
        b.max.1,
        b.max.2,
        shape.facing_name()
    )
}

type Start = ((i32, i32), Option<Vec<String>>);

fn expected(fixture: &str) -> Vec<Start> {
    let mut starts: Vec<Start> = Vec::new();
    for line in fixture.lines() {
        let parts: Vec<&str> = line.split(' ').collect();
        match parts[0] {
            "START" => {
                let chunk = (parts[2].parse().unwrap(), parts[3].parse().unwrap());
                let valid = parts[4] == "valid=true";
                starts.push((chunk, valid.then(Vec::new)));
            }
            "PIECE" | "TPL" | "CHILD" => {
                if let Some((_, Some(lines))) = starts.last_mut() {
                    lines.push(parts[0..1].concat() + " " + &parts[2..].join(" "));
                }
            }
            _ => {}
        }
    }
    starts
}

fn run<G: Ground, P: BiomeProbe>(seed: i64, name: &str, fixture: &str, ground: &G, probe: &P) {
    let library = Library::embedded();
    let structures = Structures::load(library);
    let def = structures.get(name).unwrap();
    let Kind::Shaped(other) = &def.kind else {
        panic!("not a shaped structure");
    };
    let valid = valid_biomes(def, probe);
    let mut checked = 0;
    for (chunk, want) in expected(fixture) {
        let got = other.generate(library, ground, &valid, probe, seed, chunk);
        match want {
            None => assert!(got.is_none(), "{chunk:?} must not start"),
            Some(want) => {
                let got = got.unwrap_or_else(|| panic!("{chunk:?} must start"));
                let lines: Vec<String> = got
                    .iter()
                    .flat_map(|shape| {
                        let mut lines = vec![format!("PIECE {}", describe(shape))];
                        lines.extend(shape.extra_lines());
                        lines
                    })
                    .collect();
                assert_eq!(lines, want, "{chunk:?}");
                checked += 1;
            }
        }
    }
    assert!(checked > 0);
}

pub(super) fn check(name: &str, fixture: &str) {
    check_seed(1, name, fixture);
}

pub(super) fn check_seed(seed: i64, name: &str, fixture: &str) {
    let resources = Resources::resolve();
    let overworld = Overworld::with_library(
        seed as u64,
        ClimateVariant::Normal,
        &resources.blocks,
        Library::embedded(),
    );
    let dimension = Dimension::overworld();
    run(
        seed,
        name,
        fixture,
        &overworld.ground(&dimension),
        &overworld.biome_probe(),
    );
}

pub(super) fn check_nether(name: &str, fixture: &str) {
    let resources = Resources::resolve();
    let nether = Nether::with_library(1, &resources.blocks, Library::embedded());
    let dimension = Dimension::nether();
    run(
        1,
        name,
        fixture,
        &nether.ground(&dimension),
        &nether.biome_probe(),
    );
}

#[test]
fn buried_treasure_starts_match_vanilla() {
    check(
        "buried_treasure",
        include_str!("fixtures/shaped_buried_treasure_1.txt"),
    );
}

fn count_blocks(
    overworld: &Overworld,
    resources: &Resources,
    chunk: (i32, i32),
    wanted: impl Fn(rspigot_blocks::BlockKind) -> bool,
) -> usize {
    use rspigot_domain::{BlockPos, ChunkPos};
    let dimension = Dimension::overworld();
    let column = overworld.generate(resources, &dimension, ChunkPos::new(chunk.0, chunk.1));
    let mut found = 0;
    for y in -64..200 {
        for x in 0..16 {
            for z in 0..16 {
                let at = BlockPos::new(chunk.0 * 16 + x, y, chunk.1 * 16 + z);
                found += usize::from(
                    column
                        .block(at)
                        .and_then(|state| state.kind())
                        .is_some_and(&wanted),
                );
            }
        }
    }
    found
}

#[test]
fn a_buried_treasure_chest_is_built_into_the_generated_chunk() {
    use rspigot_blocks::BlockKind;
    let resources = Resources::resolve();
    let overworld = Overworld::with_library(
        1,
        ClimateVariant::Normal,
        &resources.blocks,
        Library::embedded(),
    );
    let chests = count_blocks(&overworld, &resources, (27, -41), |kind| {
        kind == BlockKind::Chest
    });
    assert_eq!(chests, 1);
}

#[test]
fn nether_fossil_starts_match_vanilla() {
    check_nether(
        "nether_fossil",
        include_str!("fixtures/shaped_nether_fossil_1.txt"),
    );
}

#[test]
fn a_nether_fossil_is_built_into_the_nether() {
    use rspigot_blocks::BlockKind;
    use rspigot_domain::{BlockPos, ChunkPos};
    let resources = Resources::resolve();
    let nether = Nether::with_library(1, &resources.blocks, Library::embedded());
    let dimension = Dimension::nether();
    let fixture = include_str!("fixtures/shaped_nether_fossil_1.txt");
    let mut found = None;
    for (chunk, want) in expected(fixture) {
        if want.is_none() {
            continue;
        }
        let starts = nether.starts_near(&dimension, ChunkPos::new(chunk.0, chunk.1));
        if let Some(start) = starts
            .iter()
            .find(|start| start.name == "nether_fossil" && start.chunk == chunk)
        {
            found = Some((chunk, start.bbox));
            break;
        }
    }
    let (chunk, bbox) = found.expect("a placed nether fossil");
    let mut bones = 0;
    for dx in -1..=1 {
        for dz in -1..=1 {
            let at = ChunkPos::new(chunk.0 + dx, chunk.1 + dz);
            let column = nether.generate(&resources, &dimension, at);
            for y in bbox.min.1..=bbox.max.1 {
                for x in 0..16 {
                    for z in 0..16 {
                        let pos = BlockPos::new(at.x * 16 + x, y, at.z * 16 + z);
                        let kind = column.block(pos).and_then(|state| state.kind());
                        bones += usize::from(kind == Some(BlockKind::BoneBlock));
                    }
                }
            }
        }
    }
    assert!(bones >= 3, "bone blocks {bones}");
}

#[test]
fn igloo_starts_match_vanilla() {
    check("igloo", include_str!("fixtures/shaped_igloo_1.txt"));
}

#[test]
fn an_igloo_with_a_laboratory_is_built_into_the_generated_chunks() {
    use rspigot_blocks::BlockKind;
    let resources = Resources::resolve();
    let overworld = Overworld::with_library(
        1,
        ClimateVariant::Normal,
        &resources.blocks,
        Library::embedded(),
    );
    let mut chests = 0;
    let mut ladders = 0;
    for chunk in [(-9, -60), (-9, -59)] {
        chests += count_blocks(&overworld, &resources, chunk, |kind| {
            kind == BlockKind::Chest
        });
        ladders += count_blocks(&overworld, &resources, chunk, |kind| {
            kind == BlockKind::Ladder
        });
    }
    assert_eq!(chests, 1);
    assert!(ladders >= 3, "ladders {ladders}");
}

#[test]
fn ocean_shipwreck_starts_match_vanilla() {
    check("shipwreck", include_str!("fixtures/shaped_shipwreck_1.txt"));
}

#[test]
fn beached_shipwreck_starts_match_vanilla() {
    check(
        "shipwreck_beached",
        include_str!("fixtures/shaped_shipwreck_beached_1.txt"),
    );
}

#[test]
fn a_shipwreck_is_built_into_the_generated_chunks() {
    use rspigot_blocks::BlockKind;
    let resources = Resources::resolve();
    let overworld = Overworld::with_library(
        1,
        ClimateVariant::Normal,
        &resources.blocks,
        Library::embedded(),
    );
    let mut chests = 0;
    let mut planks = 0;
    for chunk in [(-13, -20), (-13, -19)] {
        chests += count_blocks(&overworld, &resources, chunk, |kind| {
            kind == BlockKind::Chest
        });
        planks += count_blocks(&overworld, &resources, chunk, |kind| {
            kind.name().ends_with("_planks")
        });
    }
    assert!(chests >= 1, "chests {chests}");
    assert!(planks >= 20, "planks {planks}");
}

#[test]
fn shipwreck_templates_keep_every_wood_palette() {
    let blueprint = Library::embedded()
        .blueprint("minecraft:shipwreck/with_mast")
        .unwrap();
    assert_eq!(blueprint.palette_count(), 8);
    assert_eq!(blueprint.palette(0).len(), blueprint.palette(5).len());
    assert!(
        blueprint
            .palette(0)
            .iter()
            .zip(blueprint.palette(5))
            .any(|(a, b)| a.state != b.state)
    );
}

#[test]
fn cold_ocean_ruin_starts_match_vanilla() {
    check(
        "ocean_ruin_cold",
        include_str!("fixtures/shaped_ocean_ruin_cold_1.txt"),
    );
}

#[test]
fn warm_ocean_ruin_starts_match_vanilla() {
    check(
        "ocean_ruin_warm",
        include_str!("fixtures/shaped_ocean_ruin_warm_1.txt"),
    );
}

fn first_placed(
    overworld: &Overworld,
    seed: i64,
    set_name: &str,
    structure: &str,
    range: std::ops::RangeInclusive<i32>,
) -> (i32, i32) {
    use super::sets::{Chunks, StructureSets};
    let library = Library::embedded();
    let dimension = Dimension::overworld();
    let probe = overworld.biome_probe();
    let ground = overworld.ground(&dimension);
    let sets = StructureSets::load(library);
    let world = Chunks {
        sets: &sets,
        seed,
        biomes: &probe,
    };
    let set = sets.all().iter().find(|set| set.name == set_name).unwrap();
    let structures = Structures::load(library);
    let def = structures.get(structure).unwrap();
    let Kind::Shaped(other) = &def.kind else {
        panic!("not a shaped structure");
    };
    let valid = valid_biomes(def, &probe);
    for x in range.clone() {
        for z in range.clone() {
            if world.is_structure_chunk(set, x, z)
                && other
                    .generate(library, &ground, &valid, &probe, seed, (x, z))
                    .is_some()
            {
                return (x, z);
            }
        }
    }
    panic!("no {structure} start");
}

#[test]
fn ocean_ruins_are_built_into_the_generated_chunks() {
    use rspigot_blocks::BlockKind;
    let resources = Resources::resolve();
    let overworld = Overworld::with_library(
        1,
        ClimateVariant::Normal,
        &resources.blocks,
        Library::embedded(),
    );
    for structure in ["ocean_ruin_cold", "ocean_ruin_warm"] {
        let (cx, cz) = first_placed(&overworld, 1, "ocean_ruins", structure, -80..=80);
        let mut stone = 0;
        for dx in -1..=1 {
            for dz in -1..=1 {
                stone += count_blocks(&overworld, &resources, (cx + dx, cz + dz), |kind| {
                    matches!(
                        kind,
                        BlockKind::StoneBricks
                            | BlockKind::MossyStoneBricks
                            | BlockKind::CrackedStoneBricks
                            | BlockKind::CutSandstone
                            | BlockKind::ChiseledSandstone
                            | BlockKind::DarkPrismarine
                            | BlockKind::PrismarineBricks
                    )
                });
            }
        }
        assert!(stone >= 5, "{structure} at {cx},{cz}: {stone}");
    }
}

#[test]
fn desert_pyramid_starts_match_vanilla() {
    check_seed(
        3,
        "desert_pyramid",
        include_str!("fixtures/shaped_desert_pyramid_3.txt"),
    );
}

#[test]
fn jungle_temple_starts_match_vanilla() {
    check_seed(
        3,
        "jungle_pyramid",
        include_str!("fixtures/shaped_jungle_pyramid_3.txt"),
    );
}

#[test]
fn swamp_hut_starts_match_vanilla() {
    check_seed(
        5,
        "swamp_hut",
        include_str!("fixtures/shaped_swamp_hut_5.txt"),
    );
}

#[test]
fn a_desert_pyramid_is_built_into_the_generated_chunks() {
    use rspigot_blocks::BlockKind;
    let resources = Resources::resolve();
    let overworld = Overworld::with_library(
        3,
        ClimateVariant::Normal,
        &resources.blocks,
        Library::embedded(),
    );
    let (cx, cz) = first_placed(
        &overworld,
        3,
        "desert_pyramids",
        "desert_pyramid",
        -300..=300,
    );
    let mut tnt = 0;
    let mut chests = 0;
    let mut cut = 0;
    let mut dusty = 0;
    for dx in 0..=1 {
        for dz in 0..=1 {
            let chunk = (cx + dx, cz + dz);
            tnt += count_blocks(&overworld, &resources, chunk, |kind| kind == BlockKind::Tnt);
            chests += count_blocks(&overworld, &resources, chunk, |kind| {
                kind == BlockKind::Chest
            });
            cut += count_blocks(&overworld, &resources, chunk, |kind| {
                kind == BlockKind::CutSandstone
            });
            dusty += count_blocks(&overworld, &resources, chunk, |kind| {
                kind == BlockKind::SuspiciousSand
            });
        }
    }
    assert_eq!(tnt, 9, "tnt at {cx},{cz}");
    assert_eq!(chests, 4, "chests at {cx},{cz}");
    assert!(cut > 100, "cut sandstone {cut}");
    assert!((5..=8).contains(&dusty), "suspicious sand {dusty}");
}

#[test]
fn a_jungle_temple_is_built_into_the_generated_chunks() {
    use rspigot_blocks::BlockKind;
    let resources = Resources::resolve();
    let overworld = Overworld::with_library(
        3,
        ClimateVariant::Normal,
        &resources.blocks,
        Library::embedded(),
    );
    let (cx, cz) = first_placed(
        &overworld,
        3,
        "jungle_temples",
        "jungle_pyramid",
        -300..=300,
    );
    let mut chests = 0;
    let mut dispensers = 0;
    let mut hooks = 0;
    for dx in 0..=1 {
        for dz in 0..=1 {
            let chunk = (cx + dx, cz + dz);
            chests += count_blocks(&overworld, &resources, chunk, |kind| {
                kind == BlockKind::Chest
            });
            dispensers += count_blocks(&overworld, &resources, chunk, |kind| {
                kind == BlockKind::Dispenser
            });
            hooks += count_blocks(&overworld, &resources, chunk, |kind| {
                kind == BlockKind::TripwireHook
            });
        }
    }
    assert_eq!(chests, 2, "chests at {cx},{cz}");
    assert_eq!(dispensers, 2);
    assert_eq!(hooks, 4);
}

#[test]
fn a_swamp_hut_is_built_into_the_generated_chunks() {
    use rspigot_blocks::BlockKind;
    use rspigot_domain::ChunkPos;
    let resources = Resources::resolve();
    let overworld = Overworld::with_library(
        5,
        ClimateVariant::Normal,
        &resources.blocks,
        Library::embedded(),
    );
    let dimension = Dimension::overworld();
    let (cx, cz) = first_placed(&overworld, 5, "swamp_huts", "swamp_hut", -300..=300);
    let mut tables = 0;
    let mut cauldrons = 0;
    let mut witches = 0;
    let mut cats = 0;
    for dx in 0..=1 {
        for dz in 0..=1 {
            let chunk = (cx + dx, cz + dz);
            tables += count_blocks(&overworld, &resources, chunk, |kind| {
                kind == BlockKind::CraftingTable
            });
            cauldrons += count_blocks(&overworld, &resources, chunk, |kind| {
                kind == BlockKind::Cauldron
            });
            let spawned = overworld.structure_entities(&dimension, ChunkPos::new(chunk.0, chunk.1));
            witches += spawned
                .iter()
                .filter(|entity| entity.get_str("id") == Some("minecraft:witch"))
                .count();
            cats += spawned
                .iter()
                .filter(|entity| entity.get_str("id") == Some("minecraft:cat"))
                .count();
        }
    }
    assert_eq!((tables, cauldrons), (1, 1), "hut at {cx},{cz}");
    assert_eq!((witches, cats), (1, 1));
}

#[test]
fn ocean_monument_layouts_match_vanilla() {
    check("monument", include_str!("fixtures/shaped_monument_1.txt"));
}

#[test]
fn an_ocean_monument_is_built_into_the_generated_chunks() {
    use rspigot_blocks::BlockKind;
    use rspigot_domain::{BlockPos, ChunkPos};
    let resources = Resources::resolve();
    let overworld = Overworld::with_library(
        1,
        ClimateVariant::Normal,
        &resources.blocks,
        Library::embedded(),
    );
    let dimension = Dimension::overworld();
    let (cx, cz) = first_placed(&overworld, 1, "ocean_monuments", "monument", -80..=80);
    let mut gold = 0;
    let mut lanterns = 0;
    let mut bricks = 0;
    let mut elders = 0;
    for dx in -2..=1 {
        for dz in -2..=1 {
            let at = ChunkPos::new(cx + dx, cz + dz);
            let column = overworld.generate(&resources, &dimension, at);
            for y in 30..70 {
                for x in 0..16 {
                    for z in 0..16 {
                        let pos = BlockPos::new(at.x * 16 + x, y, at.z * 16 + z);
                        match column.block(pos).and_then(|state| state.kind()) {
                            Some(BlockKind::GoldBlock) => gold += 1,
                            Some(BlockKind::SeaLantern) => lanterns += 1,
                            Some(BlockKind::PrismarineBricks) => bricks += 1,
                            _ => {}
                        }
                    }
                }
            }
            elders += overworld
                .structure_entities(&dimension, at)
                .iter()
                .filter(|entity| entity.get_str("id") == Some("minecraft:elder_guardian"))
                .count();
        }
    }
    assert_eq!(gold, 8, "monument at {cx},{cz}");
    assert!(lanterns > 40, "sea lanterns {lanterns}");
    assert!(bricks > 500, "prismarine bricks {bricks}");
    assert_eq!(elders, 3);
}

#[test]
fn generated_chunks_carry_monument_starts_with_piece_boxes_and_references() {
    use rspigot_domain::ChunkPos;
    let resources = Resources::resolve();
    let overworld = Overworld::with_library(
        1,
        ClimateVariant::Normal,
        &resources.blocks,
        Library::embedded(),
    );
    let dimension = Dimension::overworld();
    let (cx, cz) = first_placed(&overworld, 1, "ocean_monuments", "monument", -80..=80);
    let origin = overworld.generate(&resources, &dimension, ChunkPos::new(cx, cz));
    let start = origin
        .extra
        .get_compound("structures")
        .and_then(|structures| structures.get_compound("starts"))
        .and_then(|starts| starts.get_compound("minecraft:monument"))
        .expect("the monument start");
    assert_eq!(start.get_i32("ChunkX"), Some(cx));
    assert_eq!(start.get_i32("ChunkZ"), Some(cz));
    let children = start.get_list("Children").unwrap();
    assert!(!children.is_empty());
    for child in children {
        let bounds = child.as_compound().unwrap().get_int_array("BB").unwrap();
        assert_eq!(bounds.len(), 6);
        assert!(bounds[0] <= bounds[3] && bounds[1] <= bounds[4] && bounds[2] <= bounds[5]);
    }
    let references = origin
        .extra
        .get_compound("structures")
        .and_then(|structures| structures.get_compound("References"))
        .and_then(|references| references.get_long_array("minecraft:monument"))
        .expect("the monument reference");
    let packed = (i64::from(cx) & 0xFFFF_FFFF) | ((i64::from(cz) & 0xFFFF_FFFF) << 32);
    assert!(references.contains(&packed));
}
