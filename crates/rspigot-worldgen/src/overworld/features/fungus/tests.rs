use super::super::probe::fingerprint;
use super::super::tests::Bench;

const REFERENCE: [(&str, &str, i64, usize, u64); 26] = [
    (
        "crimson_nylium",
        "crimson_fungus",
        1,
        91,
        0xf88db48b1410eaac,
    ),
    (
        "crimson_nylium",
        "crimson_fungus",
        2,
        98,
        0xa15693ac1a4b07a3,
    ),
    (
        "crimson_nylium",
        "crimson_fungus",
        3,
        96,
        0x197a242a895a0e0c,
    ),
    (
        "crimson_nylium",
        "crimson_fungus",
        4,
        48,
        0x786acebade65fb71,
    ),
    ("crimson_nylium", "crimson_fungus", 5, 63, 0xfd6d63f45394324),
    (
        "crimson_nylium",
        "crimson_fungus",
        6,
        40,
        0xd9d683dc708c653c,
    ),
    (
        "crimson_nylium",
        "crimson_fungus",
        7,
        88,
        0x4eff3e6aaeead5fb,
    ),
    (
        "crimson_nylium",
        "crimson_fungus",
        8,
        168,
        0x48a6f77d3a49d20d,
    ),
    (
        "crimson_nylium",
        "crimson_fungus",
        9,
        72,
        0x63187c9445237032,
    ),
    (
        "crimson_nylium",
        "crimson_fungus",
        10,
        323,
        0xd4d25f41e7a6cda9,
    ),
    (
        "crimson_nylium",
        "crimson_fungus",
        11,
        55,
        0xd8bcda904acc7d76,
    ),
    (
        "crimson_nylium",
        "crimson_fungus",
        12,
        108,
        0xcdced77fc0660386,
    ),
    (
        "crimson_nylium",
        "crimson_fungus",
        13,
        88,
        0xe35a3b43d4408f8d,
    ),
    (
        "crimson_nylium",
        "crimson_fungus",
        14,
        112,
        0x9757ef2100811d7b,
    ),
    ("warped_nylium", "warped_fungus", 1, 78, 0x3e03873bf71820ed),
    ("warped_nylium", "warped_fungus", 2, 85, 0xd7a2e50540029e05),
    ("warped_nylium", "warped_fungus", 3, 90, 0xf50eab9904a5c0a5),
    ("warped_nylium", "warped_fungus", 4, 51, 0x13098a56f6e37549),
    ("warped_nylium", "warped_fungus", 5, 78, 0x15f4fce98f7f591),
    ("warped_nylium", "warped_fungus", 6, 52, 0xc74ffc2528a53380),
    ("warped_nylium", "warped_fungus", 7, 116, 0xe5be79fa4f058bd7),
    ("warped_nylium", "warped_fungus", 8, 130, 0xc32624b7f1f1601c),
    (
        "crimson_nylium",
        "crimson_fungus_planted",
        1,
        61,
        0x97e95af44d330961,
    ),
    (
        "crimson_nylium",
        "crimson_fungus_planted",
        2,
        67,
        0x2104b758473e6c95,
    ),
    (
        "crimson_nylium",
        "crimson_fungus_planted",
        3,
        83,
        0x904126852282efd7,
    ),
    (
        "crimson_nylium",
        "crimson_fungus_planted",
        4,
        40,
        0x7d83b1dd968a35f7,
    ),
];

#[test]
fn huge_fungi_match_the_vanilla_feature_cell_for_cell() {
    let bench = Bench::new();
    for (base, feature, seed, count, hash) in REFERENCE {
        let scene = format!("40:{base}");
        assert_eq!(
            fingerprint(&bench, &scene, feature, (8, 41, 8), seed, (30, 100)),
            (count, hash),
            "{feature} seed {seed}"
        );
    }
}
