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

fn expected(fixture: &str) -> Vec<((i32, i32), Option<Vec<String>>)> {
    let mut starts: Vec<((i32, i32), Option<Vec<String>>)> = Vec::new();
    for line in fixture.lines() {
        let parts: Vec<&str> = line.split(' ').collect();
        match parts[0] {
            "START" => {
                let chunk = (parts[2].parse().unwrap(), parts[3].parse().unwrap());
                let valid = parts[4] == "valid=true";
                starts.push((chunk, valid.then(Vec::new)));
            }
            "PIECE" | "TPL" => {
                if let Some((_, Some(lines))) = starts.last_mut() {
                    lines.push(parts[0..1].concat() + " " + &parts[2..].join(" "));
                }
            }
            _ => {}
        }
    }
    starts
}

fn run<G: Ground, P: BiomeProbe>(name: &str, fixture: &str, ground: &G, probe: &P) {
    let library = Library::embedded();
    let structures = Structures::load(library);
    let def = structures.get(name).unwrap();
    let Kind::Shaped(other) = &def.kind else {
        panic!("not a shaped structure");
    };
    let valid = valid_biomes(def, probe);
    let mut checked = 0;
    for (chunk, want) in expected(fixture) {
        let got = other.generate(library, ground, &valid, probe, 1, chunk);
        match want {
            None => assert!(got.is_none(), "{chunk:?} must not start"),
            Some(want) => {
                let got = got.unwrap_or_else(|| panic!("{chunk:?} must start"));
                let lines: Vec<String> = got
                    .iter()
                    .flat_map(|shape| {
                        let mut lines = vec![format!("PIECE {}", describe(shape))];
                        lines.extend(shape.template_line().map(|line| format!("TPL {line}")));
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
    let resources = Resources::resolve();
    let overworld = Overworld::with_library(
        1,
        ClimateVariant::Normal,
        &resources.blocks,
        Library::embedded(),
    );
    let dimension = Dimension::overworld();
    run(
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
