use super::super::probe::{fingerprint, lines};
use super::super::tests::Bench;

fn scene(name: &str) -> (String, (i32, i32, i32)) {
    match name {
        "coal" | "diamonds" => ("60:stone".to_owned(), (8, 60, 8)),
        "coalcave" => ("60:stone;-12..28/30..50/-12..28=air".to_owned(), (8, 60, 8)),
        "coalpit" => ("60:stone;4..12/35..50/4..12=air".to_owned(), (8, 60, 8)),
        "coalwater" => (
            "60:stone;-12..28/36..44/-12..28=water".to_owned(),
            (8, 60, 8),
        ),
        _ => ("60:stone;0..8/61..66/0..16=dirt".to_owned(), (8, 62, 8)),
    }
}

const REFERENCE: [(&str, &str, i64, usize, u64); 38] = [
    ("coal", "fossil_coal", 1, 57, 0x5483c77cd1fddfe3),
    ("coal", "fossil_coal", 2, 54, 0x412f5cd98e74dcac),
    ("coal", "fossil_coal", 3, 29, 0xee4d9b3a78a45f32),
    ("coal", "fossil_coal", 4, 93, 0x9bd68ecb756b800c),
    ("coal", "fossil_coal", 5, 52, 0x5398c781b0863b98),
    ("coal", "fossil_coal", 6, 55, 0xb5ac26d59c9b5367),
    ("coal", "fossil_coal", 7, 87, 0x145a61c798591456),
    ("coal", "fossil_coal", 8, 72, 0xa2c39652be540667),
    ("coalcave", "fossil_coal", 1, 0, 0xcbf29ce484222325),
    ("coalcave", "fossil_coal", 2, 0, 0xcbf29ce484222325),
    ("coalcave", "fossil_coal", 3, 0, 0xcbf29ce484222325),
    ("coalcave", "fossil_coal", 4, 0, 0xcbf29ce484222325),
    ("coalpit", "fossil_coal", 1, 57, 0x5483c77cd1fddfe3),
    ("coalpit", "fossil_coal", 2, 54, 0x412f5cd98e74dcac),
    ("coalpit", "fossil_coal", 3, 0, 0xcbf29ce484222325),
    ("coalpit", "fossil_coal", 4, 93, 0x9bd68ecb756b800c),
    ("coalpit", "fossil_coal", 5, 0, 0xcbf29ce484222325),
    ("coalpit", "fossil_coal", 6, 55, 0xb5ac26d59c9b5367),
    ("coalpit", "fossil_coal", 7, 87, 0x145a61c798591456),
    ("coalpit", "fossil_coal", 8, 0, 0xcbf29ce484222325),
    ("coalwater", "fossil_coal", 1, 57, 0x5483c77cd1fddfe3),
    ("coalwater", "fossil_coal", 2, 54, 0x412f5cd98e74dcac),
    ("coalwater", "fossil_coal", 3, 29, 0xee4d9b3a78a45f32),
    ("coalwater", "fossil_coal", 4, 93, 0x9bd68ecb756b800c),
    ("diamonds", "fossil_diamonds", 1, 57, 0xb148a6ed68e07d87),
    ("diamonds", "fossil_diamonds", 2, 54, 0x5e083196d52a1934),
    ("diamonds", "fossil_diamonds", 3, 29, 0x2d01e9209755e2d8),
    ("diamonds", "fossil_diamonds", 4, 93, 0x62589d17caddfd49),
    ("diamonds", "fossil_diamonds", 5, 52, 0xf3eca70802b85845),
    ("diamonds", "fossil_diamonds", 6, 55, 0x1ffa42049660b7e2),
    ("diamonds", "fossil_diamonds", 7, 87, 0x5fdd9c85c6cfe272),
    ("diamonds", "fossil_diamonds", 8, 72, 0x49967ea99ccd4ea7),
    ("diaslope", "fossil_diamonds", 1, 57, 0x22099f5fbceb54a),
    ("diaslope", "fossil_diamonds", 2, 54, 0x1b707fb7cffb63a2),
    ("diaslope", "fossil_diamonds", 3, 29, 0x47379b88722a1e1),
    ("diaslope", "fossil_diamonds", 4, 93, 0x698e6bfa14499786),
    ("diaslope", "fossil_diamonds", 5, 52, 0x18f57771d89d411b),
    ("diaslope", "fossil_diamonds", 6, 55, 0x44c59642e795e928),
];

#[test]
fn fossils_match_the_vanilla_feature_cell_for_cell() {
    let bench = Bench::new();
    for (name, feature, seed, count, hash) in REFERENCE {
        let (spec, origin) = scene(name);
        assert_eq!(
            fingerprint(&bench, &spec, feature, origin, seed, (-64, 80)),
            (count, hash),
            "{name} {feature} seed {seed}"
        );
    }
}

#[test]
fn coal_fossils_hold_coal_ore_and_diamond_fossils_replace_it() {
    let bench = Bench::new();
    let (spec, origin) = scene("coal");
    for seed in 1..=8 {
        let coal = lines(&bench, &spec, "fossil_coal", origin, seed, (-64, 80));
        let diamonds = lines(&bench, &spec, "fossil_diamonds", origin, seed, (-64, 80));
        assert!(coal.iter().any(|c| c.contains(" coal_ore")), "{seed}");
        assert!(coal.iter().any(|c| c.contains(" bone_block")), "{seed}");
        assert!(!coal.iter().any(|c| c.contains("diamond")), "{seed}");
        assert!(
            diamonds
                .iter()
                .any(|c| c.contains(" deepslate_diamond_ore")),
            "{seed}"
        );
        assert!(!diamonds.iter().any(|c| c.contains(" coal_ore")), "{seed}");
    }
}

#[test]
fn fossils_stay_inside_the_chunk_and_its_sixteen_block_margin() {
    let bench = Bench::new();
    let (spec, origin) = scene("coal");
    for seed in 1..=8 {
        for cell in lines(&bench, &spec, "fossil_coal", origin, seed, (-64, 80)) {
            let coords: Vec<i32> = cell
                .split(' ')
                .next()
                .unwrap_or("")
                .split(',')
                .filter_map(|n| n.parse().ok())
                .collect();
            assert!((-16..=31).contains(&coords[0]), "{cell}");
            assert!((-16..=31).contains(&coords[2]), "{cell}");
            assert!(coords[1] < 60, "{cell}");
        }
    }
}

#[test]
fn a_fossil_inside_an_open_cave_is_rejected() {
    let bench = Bench::new();
    let (spec, origin) = scene("coalcave");
    for seed in 1..=4 {
        assert!(lines(&bench, &spec, "fossil_coal", origin, seed, (-64, 80)).is_empty());
    }
}
