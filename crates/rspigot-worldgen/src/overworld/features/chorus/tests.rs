use super::super::probe::fingerprint;
use super::super::tests::Bench;

const REFERENCE: [(i64, usize, u64); 12] = [
    (1, 8, 0x32777b08fff88af7),
    (2, 9, 0x139fe18dc8b12342),
    (3, 40, 0x12971ba123d01e6a),
    (4, 35, 0x787c5a9f28e4650b),
    (5, 10, 0xf2ade562720e6f89),
    (6, 16, 0x9f01d9d1d882123b),
    (7, 25, 0xdec3d7ed459cc2a4),
    (8, 36, 0x14e92f85b59d392f),
    (9, 24, 0xfe54feb49a856926),
    (10, 19, 0x76df1cec35d04c54),
    (11, 18, 0x6fd26ceeec54a74f),
    (12, 40, 0xaa2ed118348f92f3),
];

#[test]
fn chorus_plants_match_the_vanilla_feature_cell_for_cell() {
    let bench = Bench::new();
    for (seed, count, hash) in REFERENCE {
        assert_eq!(
            fingerprint(
                &bench,
                "50:end_stone",
                "chorus_plant",
                (8, 51, 8),
                seed,
                (40, 100)
            ),
            (count, hash),
            "seed {seed}"
        );
    }
}
