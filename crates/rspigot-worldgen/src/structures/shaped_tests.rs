use super::library::Library;
use super::shaped::Shaped;
use super::structure::{Kind, Structures, valid_biomes};
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

pub(super) fn check(name: &str, fixture: &str) {
    let resources = Resources::resolve();
    let library = Library::embedded();
    let overworld = Overworld::with_library(1, ClimateVariant::Normal, &resources.blocks, library);
    let dimension = Dimension::overworld();
    let probe = overworld.biome_probe();
    let ground = overworld.ground(&dimension);
    let structures = Structures::load(library);
    let def = structures.get(name).unwrap();
    let Kind::Shaped(other) = &def.kind else {
        panic!("not a shaped structure");
    };
    let valid = valid_biomes(def, &probe);
    let mut checked = 0;
    for (chunk, want) in expected(fixture) {
        let got = other.generate(library, &ground, &valid, &probe, 1, chunk);
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
