use super::super::probe::fingerprint;
use super::super::tests::Bench;

const SCENES: [&str; 2] = [
    "40:netherrack;3..6/41..41/3..6=netherrack;10..11/41..42/9..10=netherrack",
    "40:netherrack;5..12/40..40/5..12=lava",
];

const REFERENCE: [(usize, i64, usize, u64); 11] = [
    (0, 1, 24, 0x8dc3d1a71084d36f),
    (0, 2, 70, 0xf102d55973f57bc1),
    (0, 3, 82, 0xd9d324da30d8233f),
    (0, 4, 34, 0xf13c574607c302a4),
    (0, 5, 23, 0x350c391adef8c544),
    (0, 6, 57, 0xb724b3ddeca78a1d),
    (0, 7, 67, 0x12b805d22297b866),
    (0, 8, 36, 0x2dad38d4a58f89d4),
    (1, 1, 0, 0xcbf29ce484222325),
    (1, 2, 25, 0xa82f66a45ce088a9),
    (1, 3, 32, 0x83703e9a14a153f6),
];

#[test]
fn delta_matches_the_vanilla_feature_cell_for_cell() {
    let bench = Bench::new();
    for (scene, seed, count, hash) in REFERENCE {
        assert_eq!(
            fingerprint(&bench, SCENES[scene], "delta", (8, 40, 8), seed, (20, 100)),
            (count, hash),
            "scene {scene} seed {seed}"
        );
    }
}
