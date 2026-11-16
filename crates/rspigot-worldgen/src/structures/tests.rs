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
