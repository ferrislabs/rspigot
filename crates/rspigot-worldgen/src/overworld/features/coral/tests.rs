use super::super::probe::fingerprint;
use super::super::tests::Bench;

const OCEAN: &str = "30:stone,31:sand,62:water";

fn scene(name: &str) -> (String, (i32, i32, i32)) {
    match name {
        "shallow" => ("30:stone,31:sand,36:water".to_owned(), (8, 32, 8)),
        "bumps" => (
            format!(
                "{OCEAN};6..7/32..33/8..9=stone;10/32/7=stone;9/32/11=sand;5/32/5..6=sand;12/32..34/9..10=stone"
            ),
            (8, 32, 8),
        ),
        "slope" => (
            format!("{OCEAN};0..40/32/-24..8=sand;0..40/33/-24..4=sand"),
            (8, 34, 8),
        ),
        _ => (OCEAN.to_owned(), (8, 32, 8)),
    }
}

const REFERENCE: [(&str, &str, i64, usize, u64); 78] = [
    ("flat", "warm_ocean_vegetation", 1, 17, 0x349dc5df4a17203),
    ("flat", "warm_ocean_vegetation", 2, 34, 0x8bfdf709bf3b7d5c),
    ("flat", "warm_ocean_vegetation", 3, 30, 0xd47326557a46db33),
    ("flat", "warm_ocean_vegetation", 4, 34, 0xf9732790182f777b),
    ("flat", "warm_ocean_vegetation", 5, 18, 0x180b371a74e3546e),
    ("flat", "warm_ocean_vegetation", 6, 27, 0xc536a403e3d1f055),
    ("flat", "warm_ocean_vegetation", 7, 24, 0x9e1edd7fffdfa3c),
    ("flat", "warm_ocean_vegetation", 8, 21, 0xd5a7201fb927face),
    ("flat", "warm_ocean_vegetation", 9, 28, 0x7593e8e2f46cf808),
    ("flat", "warm_ocean_vegetation", 10, 28, 0x9f393f9aedfa669f),
    ("flat", "warm_ocean_vegetation", 11, 23, 0x5a31a5ad9c30e8dc),
    ("flat", "warm_ocean_vegetation", 12, 39, 0x2530699906c21eb2),
    ("flat", "warm_ocean_vegetation", 13, 17, 0x68f850da1f843a93),
    ("flat", "warm_ocean_vegetation", 14, 26, 0x5e4724c3cd901964),
    ("flat", "warm_ocean_vegetation", 15, 29, 0xda5806d4eea13532),
    ("flat", "warm_ocean_vegetation", 16, 18, 0x2b2a2afe2c532f62),
    ("flat", "warm_ocean_vegetation", 17, 30, 0xda17c069a342b38e),
    ("flat", "warm_ocean_vegetation", 18, 23, 0xc1b7aa273f9fd8d2),
    ("flat", "warm_ocean_vegetation", 19, 10, 0xc9eb8ada53736e4c),
    ("flat", "warm_ocean_vegetation", 20, 27, 0xfd5e4b81294f194b),
    ("flat", "warm_ocean_vegetation", 21, 27, 0x7bc0ad8cd572e60e),
    ("flat", "warm_ocean_vegetation", 22, 12, 0x24d78f1388d0bd0a),
    ("flat", "warm_ocean_vegetation", 23, 27, 0xb1839d3ac5515b71),
    ("flat", "warm_ocean_vegetation", 24, 51, 0xcc159b26df9e2a7b),
    ("flat", "warm_ocean_vegetation", 25, 9, 0x998fd2607773500c),
    ("flat", "warm_ocean_vegetation", 26, 29, 0xea26c65ab038e778),
    ("flat", "warm_ocean_vegetation", 27, 37, 0x33e8905b9112aec5),
    ("flat", "warm_ocean_vegetation", 28, 24, 0x547532dce90a8da3),
    ("flat", "warm_ocean_vegetation", 29, 48, 0x935d18ff92d82567),
    ("flat", "warm_ocean_vegetation", 30, 38, 0x670b3c188dcf845c),
    ("flat", "warm_ocean_vegetation", 31, 24, 0x750229d994000f0b),
    ("flat", "warm_ocean_vegetation", 32, 12, 0xe9d4e02a74a98ec9),
    ("flat", "warm_ocean_vegetation", 33, 92, 0xa12f7e722fa3acd3),
    ("flat", "warm_ocean_vegetation", 34, 63, 0x3a5d6013418045b4),
    ("flat", "warm_ocean_vegetation", 35, 38, 0x4f826124711c07f5),
    ("flat", "warm_ocean_vegetation", 36, 33, 0xb36a94f52935a972),
    (
        "shallow",
        "warm_ocean_vegetation",
        1,
        12,
        0xacf84b0cbaa2035f,
    ),
    (
        "shallow",
        "warm_ocean_vegetation",
        2,
        24,
        0xc151ebfc290b9018,
    ),
    (
        "shallow",
        "warm_ocean_vegetation",
        3,
        30,
        0xd47326557a46db33,
    ),
    (
        "shallow",
        "warm_ocean_vegetation",
        4,
        16,
        0x594e4ca153b149e4,
    ),
    (
        "shallow",
        "warm_ocean_vegetation",
        5,
        10,
        0x8c0478752f42dc64,
    ),
    (
        "shallow",
        "warm_ocean_vegetation",
        6,
        27,
        0xc536a403e3d1f055,
    ),
    (
        "shallow",
        "warm_ocean_vegetation",
        7,
        23,
        0x957b2aabddac3b14,
    ),
    (
        "shallow",
        "warm_ocean_vegetation",
        8,
        21,
        0xd5a7201fb927face,
    ),
    (
        "shallow",
        "warm_ocean_vegetation",
        9,
        16,
        0x1190a7b052b30a85,
    ),
    (
        "shallow",
        "warm_ocean_vegetation",
        10,
        19,
        0x9a123f29c7f1c23d,
    ),
    (
        "shallow",
        "warm_ocean_vegetation",
        11,
        23,
        0x5a31a5ad9c30e8dc,
    ),
    (
        "shallow",
        "warm_ocean_vegetation",
        12,
        31,
        0x64e6a5d390ea3143,
    ),
    (
        "shallow",
        "warm_ocean_vegetation",
        13,
        17,
        0x68f850da1f843a93,
    ),
    (
        "shallow",
        "warm_ocean_vegetation",
        14,
        21,
        0x45443be6d366ed1d,
    ),
    ("bumps", "warm_ocean_vegetation", 1, 8, 0x1c473a5174838240),
    ("bumps", "warm_ocean_vegetation", 2, 34, 0x8bfdf709bf3b7d5c),
    ("bumps", "warm_ocean_vegetation", 3, 30, 0x1ae075e9938e30a),
    ("bumps", "warm_ocean_vegetation", 4, 34, 0xf9732790182f777b),
    ("bumps", "warm_ocean_vegetation", 5, 17, 0xad39e2003b9c61fb),
    ("bumps", "warm_ocean_vegetation", 6, 25, 0x3b38c251d2f681dc),
    ("bumps", "warm_ocean_vegetation", 7, 24, 0x64907520f181e37d),
    ("bumps", "warm_ocean_vegetation", 8, 19, 0x5667ffb98acfea74),
    ("bumps", "warm_ocean_vegetation", 9, 28, 0x7593e8e2f46cf808),
    ("bumps", "warm_ocean_vegetation", 10, 28, 0x91d6de83cd63eaf),
    ("bumps", "warm_ocean_vegetation", 11, 23, 0x5a31a5ad9c30e8dc),
    ("bumps", "warm_ocean_vegetation", 12, 31, 0x6679606bf0333035),
    ("bumps", "warm_ocean_vegetation", 13, 6, 0x283eb380ba12b15a),
    ("bumps", "warm_ocean_vegetation", 14, 27, 0x2f47f50c7e54deb7),
    ("bumps", "warm_ocean_vegetation", 15, 24, 0x3274534edf021323),
    ("bumps", "warm_ocean_vegetation", 16, 18, 0x2b2a2afe2c532f62),
    ("bumps", "warm_ocean_vegetation", 17, 39, 0xbf0b05e73decda90),
    ("bumps", "warm_ocean_vegetation", 18, 12, 0x5db442a5593d7ba1),
    ("slope", "warm_ocean_vegetation", 1, 17, 0x5a132cbe8b19ee22),
    ("slope", "warm_ocean_vegetation", 2, 34, 0x8e3ed2d5d8aa148),
    ("slope", "warm_ocean_vegetation", 3, 30, 0x8b7be0766995db77),
    ("slope", "warm_ocean_vegetation", 4, 34, 0x1f9efca8a5ed30f8),
    ("slope", "warm_ocean_vegetation", 5, 18, 0x5c5ae22fad607893),
    ("slope", "warm_ocean_vegetation", 6, 27, 0xbec6928973048d23),
    ("slope", "warm_ocean_vegetation", 7, 24, 0x4d20273ed6da7d00),
    ("slope", "warm_ocean_vegetation", 8, 21, 0xb7fc748227d324f0),
    ("slope", "warm_ocean_vegetation", 9, 28, 0xd43b20f470f17e29),
    ("slope", "warm_ocean_vegetation", 10, 28, 0xe1f79a6a04bb6547),
];

#[test]
fn warm_ocean_vegetation_matches_the_vanilla_feature_cell_for_cell() {
    let bench = Bench::new();
    for (name, feature, seed, count, hash) in REFERENCE {
        let (spec, origin) = scene(name);
        assert_eq!(
            fingerprint(&bench, &spec, feature, origin, seed, (20, 80)),
            (count, hash),
            "{name} seed {seed}"
        );
    }
}
