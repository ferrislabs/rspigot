use rspigot_data::multi_noise::ClimateVariant;

use super::*;

const COLUMNS: &str = include_str!("columns.txt");

fn decode(rle: &str) -> Vec<char> {
    let mut out = Vec::new();
    let mut chars = rle.chars().peekable();
    while let Some(kind) = chars.next() {
        let mut count = 0usize;
        while let Some(d) = chars.peek().and_then(|c| c.to_digit(10)) {
            count = count * 10 + d as usize;
            chars.next();
        }
        out.extend(std::iter::repeat_n(kind, count));
    }
    out
}

fn letter(cell: Cell) -> char {
    match cell {
        Cell::Default => 's',
        Cell::Air => 'a',
        Cell::Water => 'w',
        Cell::Lava => 'l',
    }
}

#[test]
fn base_columns_match_the_vanilla_generator() {
    let mut mismatches = Vec::new();
    for line in COLUMNS.lines() {
        let mut parts = line.split(' ');
        let seed: i64 = parts.next().unwrap().parse().unwrap();
        let x: i32 = parts.next().unwrap().parse().unwrap();
        let z: i32 = parts.next().unwrap().parse().unwrap();
        let expected = decode(parts.next().unwrap());
        let terrain = Terrain::new(seed, ClimateVariant::Normal);
        let column = terrain.column(x, z);
        let got: Vec<char> = column.iter().map(|c| letter(*c)).collect();
        let diff = got.iter().zip(&expected).filter(|(a, b)| a != b).count();
        if diff != 0 || got.len() != expected.len() {
            mismatches.push((seed, x, z, diff));
        }
    }
    assert!(mismatches.is_empty(), "{mismatches:?}");
}

#[test]
fn amplified_and_large_biomes_columns_match_the_vanilla_generator() {
    const VARIANTS: &str = include_str!("variant_columns.txt");
    for line in VARIANTS.lines() {
        let mut parts = line.split(' ');
        let variant = match parts.next().unwrap() {
            "a" => ClimateVariant::Amplified,
            _ => ClimateVariant::LargeBiomes,
        };
        let seed: i64 = parts.next().unwrap().parse().unwrap();
        let x: i32 = parts.next().unwrap().parse().unwrap();
        let z: i32 = parts.next().unwrap().parse().unwrap();
        let expected = decode(parts.next().unwrap());
        let got: Vec<char> = Terrain::new(seed, variant)
            .column(x, z)
            .iter()
            .map(|c| letter(*c))
            .collect();
        assert_eq!(got, expected, "{variant:?} {seed} {x} {z}");
    }
}

#[test]
fn ore_veins_match_the_vanilla_material_rule() {
    const VEINS: &str = include_str!("vein_columns.txt");
    let named = |name: &str| crate::datapack::state_named(name, &[]);
    let kinds = [
        ('g', named("minecraft:granite")),
        ('t', named("minecraft:tuff")),
        ('c', named("minecraft:copper_ore")),
        ('C', named("minecraft:raw_copper_block")),
        ('i', named("minecraft:deepslate_iron_ore")),
        ('I', named("minecraft:raw_iron_block")),
    ];
    for line in VEINS.lines() {
        let mut parts = line.split(' ');
        let seed: i64 = parts.next().unwrap().parse().unwrap();
        let x: i32 = parts.next().unwrap().parse().unwrap();
        let z: i32 = parts.next().unwrap().parse().unwrap();
        let expected = decode(parts.next().unwrap());
        let terrain = Terrain::new(seed, ClimateVariant::Normal);
        let mut sampler = terrain.sampler();
        terrain.fill(&mut sampler, x & !15, z & !15);
        for (index, want) in expected.iter().enumerate() {
            let y = -64 + index as i32;
            if y < -59 {
                continue;
            }
            let got = terrain.vein(&mut sampler, x, y, z).map_or('.', |state| {
                kinds
                    .iter()
                    .find(|(_, s)| *s == Some(state))
                    .map_or('?', |(c, _)| *c)
            });
            assert_eq!(got, *want, "seed {seed} at {x} {y} {z}");
        }
    }
}
