use super::super::probe::fingerprint;
use super::super::tests::Bench;

const REFERENCE: [(i64, usize, u64); 10] = [
    (1, 345, 0x61c7ea3874993efa),
    (2, 345, 0x61c7ea3874993efa),
    (3, 365, 0xf9792f9a07990420),
    (4, 418, 0x9b225b0e6a4daf15),
    (5, 315, 0xb91d26d05f988a21),
    (6, 171, 0x6bbbc48323b37115),
    (7, 405, 0x8f67ce8a651fe66a),
    (8, 123, 0x42fb9d52e8f2e775),
    (9, 203, 0x9be7765a83425519),
    (10, 405, 0x8f67ce8a651fe66a),
];

#[test]
fn end_islands_match_the_vanilla_feature_cell_for_cell() {
    let bench = Bench::new();
    for (seed, count, hash) in REFERENCE {
        assert_eq!(
            fingerprint(&bench, "10:air", "end_island", (8, 60, 8), seed, (40, 100)),
            (count, hash),
            "seed {seed}"
        );
    }
}
