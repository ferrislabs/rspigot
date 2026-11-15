use super::super::probe::fingerprint;
use super::super::tests::Bench;

const LOW: &str = "30:stone;-12..28/10..16/-12..28=air";
const TALL: &str = "30:stone;-12..28/10..24/-12..28=air";
const SULFUR: &str = "30:sulfur;-12..28/10..16/-12..28=air";
const SULFUR_TALL: &str = "30:sulfur;-12..28/10..24/-12..28=air";

fn scene(name: &str, feature: &str) -> (String, (i32, i32, i32)) {
    let sulfur = feature.starts_with("sulfur");
    let (low, tall) = if sulfur {
        (SULFUR, SULFUR_TALL)
    } else {
        (LOW, TALL)
    };
    let with = |base: &str, extra: &str| format!("{base};{extra}");
    let floor = (8, 13, 8);
    match name {
        "cave" => (low.to_owned(), floor),
        "tall" => (tall.to_owned(), (8, 16, 8)),
        "rough" => (
            with(
                low,
                "0..3/10..13/0..16=stone;12..16/14..16/4..10=stone;4..6/10..11/4..12=stone;10..11/10..12/10..11=dirt;2..5/16/6..9=air",
            ),
            floor,
        ),
        "mixed" => (
            with(
                low,
                "0..6/10/0..16=dirt;7..12/10/0..16=calcite;13..18/10/0..16=tuff;0..6/16/0..16=granite;7..12/16/0..16=oak_planks;13..18/16/0..16=dripstone_block;-4..2/10/-4..2=glass",
            ),
            floor,
        ),
        "wet" => (with(low, "-12..28/10..11/-12..28=water"), floor),
        "flood" => (with(low, "-12..28/10..16/-12..28=water"), floor),
        "lava" => (with(low, "-12..28/9/-12..28=lava"), floor),
        "lavaroof" => (with(low, "-12..28/17/-12..28=lava"), floor),
        "floorless" => ("30:stone;-12..28/-64..16/-12..28=air".to_owned(), floor),
        "roofless" => ("20:stone".to_owned(), (8, 24, 8)),
        "open" => ("12:stone".to_owned(), floor),
        _ => ("30:stone;-12..28/10..13/-12..28=air".to_owned(), (8, 11, 8)),
    }
}

const REFERENCE: [(&str, &str, i64, usize, u64); 63] = [
    ("cave", "large_dripstone", 1, 53, 0xc992164e346691bf),
    ("cave", "large_dripstone", 2, 20, 0x21b05b1ba5f59bf7),
    ("cave", "large_dripstone", 3, 33, 0x4e8f9502e7a5a16c),
    ("cave", "large_dripstone", 4, 60, 0x30b82deb0cf41e02),
    ("cave", "large_dripstone", 5, 63, 0xcf40eb63ca9abc06),
    ("cave", "large_dripstone", 6, 64, 0xbe772c95f092b60c),
    ("cave", "large_dripstone", 7, 37, 0xa8d263e8380051df),
    ("cave", "large_dripstone", 8, 0, 0xcbf29ce484222325),
    ("cave", "large_dripstone", 9, 21, 0xda3e8e5bf4f5d806),
    ("cave", "large_dripstone", 10, 38, 0x263de703bee6a0b1),
    ("cave", "large_dripstone", 11, 37, 0x7bd4be2a0d3ddd59),
    ("cave", "large_dripstone", 12, 40, 0xd94968983b8e42ab),
    ("tall", "large_dripstone", 1, 65, 0x271a5e2d3219398b),
    ("tall", "large_dripstone", 2, 20, 0x939733d0a4164d1a),
    ("tall", "large_dripstone", 3, 112, 0x585aa11683daaf55),
    ("tall", "large_dripstone", 4, 73, 0x1f55aad9769ff789),
    ("tall", "large_dripstone", 5, 215, 0xf67606ce28d89662),
    ("tall", "large_dripstone", 6, 84, 0x8a852739aa92e596),
    ("tall", "large_dripstone", 7, 126, 0xc0f8ea6e442b75f2),
    ("tall", "large_dripstone", 8, 0, 0xcbf29ce484222325),
    ("tall", "large_dripstone", 9, 21, 0x2f46beca4af1542e),
    ("tall", "large_dripstone", 10, 122, 0x61991e88876426),
    ("tall", "large_dripstone", 11, 37, 0x501178f9e541b2a4),
    ("tall", "large_dripstone", 12, 149, 0xe2b1b7149beeb3b5),
    ("rough", "large_dripstone", 1, 53, 0xc992164e346691bf),
    ("rough", "large_dripstone", 2, 20, 0x21b05b1ba5f59bf7),
    ("rough", "large_dripstone", 3, 33, 0x4e8f9502e7a5a16c),
    ("rough", "large_dripstone", 4, 59, 0x3527d8f7b05f9199),
    ("rough", "large_dripstone", 5, 63, 0xcf40eb63ca9abc06),
    ("rough", "large_dripstone", 6, 63, 0xbc7eb4eb201b2e09),
    ("rough", "large_dripstone", 7, 36, 0x62eda2716961e7be),
    ("rough", "large_dripstone", 8, 0, 0xcbf29ce484222325),
    ("mixed", "large_dripstone", 1, 0, 0xcbf29ce484222325),
    ("mixed", "large_dripstone", 2, 0, 0xcbf29ce484222325),
    ("mixed", "large_dripstone", 3, 0, 0xcbf29ce484222325),
    ("mixed", "large_dripstone", 4, 0, 0xcbf29ce484222325),
    ("mixed", "large_dripstone", 5, 0, 0xcbf29ce484222325),
    ("mixed", "large_dripstone", 6, 0, 0xcbf29ce484222325),
    ("mixed", "large_dripstone", 7, 0, 0xcbf29ce484222325),
    ("mixed", "large_dripstone", 8, 0, 0xcbf29ce484222325),
    ("wet", "large_dripstone", 1, 53, 0xc992164e346691bf),
    ("wet", "large_dripstone", 2, 20, 0x21b05b1ba5f59bf7),
    ("wet", "large_dripstone", 3, 33, 0x4e8f9502e7a5a16c),
    ("wet", "large_dripstone", 4, 60, 0x30b82deb0cf41e02),
    ("wet", "large_dripstone", 5, 63, 0xcf40eb63ca9abc06),
    ("wet", "large_dripstone", 6, 64, 0xbe772c95f092b60c),
    ("flood", "large_dripstone", 1, 53, 0xc992164e346691bf),
    ("flood", "large_dripstone", 2, 20, 0x21b05b1ba5f59bf7),
    ("flood", "large_dripstone", 3, 33, 0x4e8f9502e7a5a16c),
    ("flood", "large_dripstone", 4, 60, 0x30b82deb0cf41e02),
    ("flood", "large_dripstone", 5, 63, 0xcf40eb63ca9abc06),
    ("flood", "large_dripstone", 6, 64, 0xbe772c95f092b60c),
    ("lava", "large_dripstone", 1, 43, 0x9020a787f1853891),
    ("lava", "large_dripstone", 2, 15, 0x8c6a869a17f2c998),
    ("lava", "large_dripstone", 3, 29, 0x20bbdf4d4d065410),
    ("lava", "large_dripstone", 4, 44, 0x5bbf02973a9c32de),
    ("lavaroof", "large_dripstone", 1, 22, 0xb83abd4d3873f4be),
    ("lavaroof", "large_dripstone", 2, 6, 0x27427de4fd69cba6),
    ("lavaroof", "large_dripstone", 3, 5, 0x3b7c89d16fae9b10),
    ("lavaroof", "large_dripstone", 4, 28, 0x70ddfa109182b343),
    ("thin", "large_dripstone", 1, 38, 0x820cd9ac9f3eaa16),
    ("thin", "large_dripstone", 2, 19, 0x3fe231a35c26c4f),
    ("thin", "large_dripstone", 3, 27, 0xbdfb994a9111d196),
];

#[test]
fn large_dripstones_match_the_vanilla_feature_cell_for_cell() {
    let bench = Bench::new();
    for (name, feature, seed, count, hash) in REFERENCE {
        let (spec, origin) = scene(name, feature);
        assert_eq!(
            fingerprint(&bench, &spec, feature, origin, seed, (0, 40)),
            (count, hash),
            "{name} {feature} seed {seed}"
        );
    }
}
