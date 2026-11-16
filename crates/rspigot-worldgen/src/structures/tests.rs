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
        self.source.biome_index(&mut self.scratch.borrow_mut(), qx, qy, qz)
    }

    fn id(&self, name: &str) -> Option<u8> {
        overworld_biomes()
            .iter()
            .position(|candidate| candidate.strip_prefix("minecraft:") == Some(name))
            .map(|index| index as u8)
    }
}

fn expected(fixture: &str) -> (Vec<(String, i32, i32)>, Vec<(i32, i32)>) {
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
    let sets = StructureSets::load();
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
            assert!(differing <= 8, "{differing} rings differ");
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
    use super::super::pool::{Element, Pools, Projection};
    use super::super::structure::{Kind, Structures, generate_jigsaw, valid_biomes};
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
        let overworld = Overworld::new(seed as u64, ClimateVariant::Normal, &resources.blocks);
        let dimension = Dimension::overworld();
        let probe = overworld.biome_probe();
        let ground = overworld.ground(&dimension);
        let structures = Structures::load();
        let def = structures.get(name).unwrap();
        let Kind::Jigsaw(jigsaw) = &def.kind else {
            panic!("not a jigsaw structure");
        };
        let pools = Pools::load();
        let valid = valid_biomes(def, &probe);
        let assembly = generate_jigsaw(jigsaw, &valid, &pools, &ground, &probe, seed, chunk)
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
    fn savanna_village_matches_vanilla_for_a_negative_seed() {
        check(
            -4172144997902289642,
            "village_savanna",
            (-32, -23),
            include_str!("fixtures/jigsaw_village_savanna_-4172144997902289642_-32_-23.txt"),
        );
    }
}
