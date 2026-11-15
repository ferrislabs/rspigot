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
