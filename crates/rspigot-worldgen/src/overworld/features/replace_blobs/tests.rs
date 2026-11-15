use super::super::probe::fingerprint;
use super::super::tests::Bench;

const REFERENCE: [(&str, &str, i64, usize, u64); 7] = [
    ("30:netherrack", "basalt_blobs", 1, 130, 0x580cbdcfac443ea9),
    ("30:netherrack", "basalt_blobs", 2, 138, 0x6cae1b8b75ab0191),
    ("30:netherrack", "basalt_blobs", 3, 316, 0xbebb8fc54f2c217a),
    ("30:netherrack", "basalt_blobs", 4, 135, 0x6ff6b2d15c311cb5),
    (
        "20:netherrack;5..9/21..25/5..9=air",
        "blackstone_blobs",
        1,
        130,
        0x1a2074dbe2c24a8f,
    ),
    (
        "20:netherrack;5..9/21..25/5..9=air",
        "blackstone_blobs",
        2,
        138,
        0xe15dcbffb13cefef,
    ),
    (
        "20:netherrack;5..9/21..25/5..9=air",
        "blackstone_blobs",
        3,
        316,
        0xb2ce473a65e3d084,
    ),
];

#[test]
fn netherrack_replace_blobs_match_the_vanilla_feature_cell_for_cell() {
    let bench = Bench::new();
    for (scene, feature, seed, count, hash) in REFERENCE {
        assert_eq!(
            fingerprint(&bench, scene, feature, (8, 40, 8), seed, (10, 100)),
            (count, hash),
            "{feature} seed {seed}"
        );
    }
}
