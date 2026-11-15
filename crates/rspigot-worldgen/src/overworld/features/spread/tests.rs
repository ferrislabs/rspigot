use super::super::probe::{GRASS, fingerprint, world};
use super::super::tests::Bench;

fn scene(name: &str) -> String {
    match name {
        "blocks" => "20:stone;4..12/78..80/4..12=stone;8/79..81/8=air".to_owned(),
        "ground" => "75:stone".to_owned(),
        _ => "20:stone".to_owned(),
    }
}

const REFERENCE: [(&str, &str, i64, usize, u64); 17] = [
    ("air", "glowstone_extra", 1, 35, 0xab0a67749fb56471),
    ("air", "glowstone_extra", 2, 36, 0x65cb26b47b1f38),
    ("air", "glowstone_extra", 3, 43, 0x70007d2f95417d68),
    ("air", "glowstone_extra", 4, 34, 0x357954aadefa0922),
    ("air", "glowstone_extra", 5, 39, 0x23c0f3f602a69886),
    ("air", "glowstone_extra", 6, 23, 0xe2055bab2110ce6a),
    ("air", "glowstone_extra", 7, 28, 0x8d1fd76e997cf0a0),
    ("air", "glowstone_extra", 8, 27, 0x808424c461e83fc4),
    ("blocks", "glowstone_extra", 1, 1, 0xf04a0e1d284aa7bf),
    ("blocks", "glowstone_extra", 2, 2, 0xf356ff1bb1de07a9),
    ("blocks", "glowstone_extra", 3, 2, 0xf356ff1bb1de07a9),
    ("blocks", "glowstone_extra", 4, 2, 0xf356ff1bb1de07a9),
    ("blocks", "glowstone_extra", 5, 2, 0xf356ff1bb1de07a9),
    ("blocks", "glowstone_extra", 6, 2, 0xf356ff1bb1de07a9),
    ("ground", "glowstone_extra", 1, 35, 0xab0a67749fb56471),
    ("ground", "glowstone_extra", 2, 36, 0x65cb26b47b1f38),
    ("ground", "glowstone_extra", 3, 43, 0x70007d2f95417d68),
];

#[test]
fn random_neighbor_spread_matches_the_vanilla_feature_cell_for_cell() {
    let bench = Bench::new();
    for (name, feature, seed, count, hash) in REFERENCE {
        assert_eq!(
            fingerprint(&bench, &scene(name), feature, (8, 80, 8), seed, (60, 100)),
            (count, hash),
            "{name} seed {seed}"
        );
    }
}

#[test]
fn the_freeze_top_layer_feature_leaves_the_pass_to_the_chunk_freeze() {
    let bench = Bench::new();
    for seed in 0..3 {
        let mut region = world(&bench, GRASS);
        assert!(!bench.place(&mut region, "freeze_top_layer", (0, 70, 0), seed));
        assert_eq!(
            fingerprint(
                &bench,
                GRASS,
                "freeze_top_layer",
                (0, 70, 0),
                seed,
                (40, 130)
            ),
            (0, 0xcbf2_9ce4_8422_2325)
        );
    }
}
