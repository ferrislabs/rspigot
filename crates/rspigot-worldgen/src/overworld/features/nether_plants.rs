use super::probe::fingerprint;
use super::tests::Bench;

const REFERENCE: [(&str, &str, i64, usize, u64); 9] = [
    ("40:netherrack", "crimson_roots", 1, 0, 0xcbf29ce484222325),
    (
        "40:crimson_nylium",
        "crimson_roots",
        1,
        1,
        0x9449e23e4b6dedc1,
    ),
    (
        "40:warped_nylium",
        "nether_sprouts",
        1,
        1,
        0x360366dbb7bb47ad,
    ),
    ("40:netherrack", "nether_sprouts", 1, 0, 0xcbf29ce484222325),
    ("40:netherrack", "crimson_fungus", 1, 0, 0xcbf29ce484222325),
    ("40:netherrack", "patch_fire", 1, 1, 0x2826fab71751eb45),
    ("40:glass", "patch_fire", 1, 1, 0x2826fab71751eb45),
    ("40:soul_sand", "patch_soul_fire", 1, 1, 0x7c40b0574d061c18),
    ("40:netherrack", "patch_soul_fire", 1, 0, 0xcbf29ce484222325),
];

#[test]
fn nether_plants_and_fire_survive_exactly_where_vanilla_lets_them() {
    let bench = Bench::new();
    for (scene, feature, seed, count, hash) in REFERENCE {
        assert_eq!(
            fingerprint(&bench, scene, feature, (8, 41, 8), seed, (30, 100)),
            (count, hash),
            "{feature} on {scene}"
        );
    }
}
