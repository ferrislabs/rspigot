use super::super::probe::{fingerprint, lines};
use super::super::tests::Bench;
use super::NOISE;
use crate::climate::NoiseStack;

const STONE: &str = "60:stone";

fn scene(name: &str) -> (String, (i32, i32, i32)) {
    let with = |extra: &str| format!("{STONE};{extra}");
    match name {
        "solid" => (STONE.to_owned(), (8, 10, 8)),
        "cave" => (with("-12..28/2..9/-12..28=air"), (8, 10, 8)),
        "pocket" => (with("4..12/8..14/4..12=air"), (8, 10, 8)),
        "wet" => (with("-12..28/8..13/-12..28=water"), (8, 10, 8)),
        "lavapart" => (
            with("14/14..16/12..14=lava;6..9/9..12/6..9=lava"),
            (8, 10, 8),
        ),
        "deep" => ("-60:bedrock,60:stone".to_owned(), (8, -52, 8)),
        "mixed" => (
            "20:deepslate,60:stone;-12..28/22..30/-12..28=dirt;0..10/10..40/0..16=tuff;12..14/14..18/4..8=calcite"
                .to_owned(),
            (8, 10, 8),
        ),
        "surface" => ("20:stone".to_owned(), (8, 14, 8)),
        "edge" => (with("-24..39/2..5/-24..39=air"), (0, 10, 0)),
        _ => ("9:stone".to_owned(), (8, 12, 8)),
    }
}

const REFERENCE: [(&str, &str, i64, usize, u64); 56] = [
    ("solid", "amethyst_geode", 1, 1557, 0x4d6ce8e4927e747b),
    ("solid", "amethyst_geode", 2, 1422, 0x9efbcbacb039c920),
    ("solid", "amethyst_geode", 3, 3105, 0xf6dcd751b6d44dc9),
    ("solid", "amethyst_geode", 4, 585, 0x5579e9cbd2871860),
    ("solid", "amethyst_geode", 5, 2737, 0xc8072c2a9135b023),
    ("solid", "amethyst_geode", 6, 907, 0x30ec6109adc99395),
    ("cave", "amethyst_geode", 1, 1557, 0x4d6ce8e4927e747b),
    ("cave", "amethyst_geode", 2, 1417, 0x2cf05e77fe0aab77),
    ("cave", "amethyst_geode", 3, 3099, 0xaba7e63e2cce0816),
    ("cave", "amethyst_geode", 4, 585, 0x5579e9cbd2871860),
    ("cave", "amethyst_geode", 5, 2736, 0x285a4d077654b314),
    ("cave", "amethyst_geode", 6, 907, 0x30ec6109adc99395),
    ("pocket", "amethyst_geode", 1, 1526, 0xd8c715bdd2cdb8ff),
    ("pocket", "amethyst_geode", 2, 1418, 0x3517f542f0e5c425),
    ("pocket", "amethyst_geode", 3, 3060, 0xdc37839cb43d8f3),
    ("pocket", "amethyst_geode", 4, 582, 0xbdb5e4cb35de3735),
    ("pocket", "amethyst_geode", 5, 2683, 0xbd8d4c86191e2a67),
    ("pocket", "amethyst_geode", 6, 898, 0x7362496b8783a4cd),
    ("wet", "amethyst_geode", 1, 1557, 0x4d6ce8e4927e747b),
    ("wet", "amethyst_geode", 2, 1422, 0x9efbcbacb039c920),
    ("wet", "amethyst_geode", 3, 3105, 0xf6dcd751b6d44dc9),
    ("wet", "amethyst_geode", 4, 585, 0x5579e9cbd2871860),
    ("wet", "amethyst_geode", 5, 2737, 0xc8072c2a9135b023),
    ("wet", "amethyst_geode", 6, 907, 0x30ec6109adc99395),
    ("lavapart", "amethyst_geode", 1, 1557, 0x4d6ce8e4927e747b),
    ("lavapart", "amethyst_geode", 2, 0, 0xcbf29ce484222325),
    ("lavapart", "amethyst_geode", 3, 3105, 0xf6dcd751b6d44dc9),
    ("lavapart", "amethyst_geode", 4, 585, 0x5579e9cbd2871860),
    ("lavapart", "amethyst_geode", 5, 2737, 0xc8072c2a9135b023),
    ("lavapart", "amethyst_geode", 6, 907, 0x30ec6109adc99395),
    ("lavapart", "amethyst_geode", 7, 0, 0xcbf29ce484222325),
    ("lavapart", "amethyst_geode", 8, 1035, 0xa30c0bee4566e51f),
    ("deep", "amethyst_geode", 1, 1430, 0x9ea78784b9709cd7),
    ("deep", "amethyst_geode", 2, 1143, 0xfaf0bb3bdc742f1c),
    ("deep", "amethyst_geode", 3, 3542, 0xc29aa5aabd6759ef),
    ("deep", "amethyst_geode", 4, 900, 0xf6e17f642d1f440c),
    ("deep", "amethyst_geode", 5, 2045, 0x13578d4cbe1bcb99),
    ("deep", "amethyst_geode", 6, 1315, 0x1b9ec1d7417a91dd),
    ("mixed", "amethyst_geode", 1, 1532, 0x332b97619d7a4d7b),
    ("mixed", "amethyst_geode", 2, 1408, 0x5bf6d177fec82e83),
    ("mixed", "amethyst_geode", 3, 3090, 0xfe8fafce04d3b7d8),
    ("mixed", "amethyst_geode", 4, 585, 0x5579e9cbd2871860),
    ("mixed", "amethyst_geode", 5, 2721, 0x496235e587919394),
    ("mixed", "amethyst_geode", 6, 900, 0xbc9d9048b7d80346),
    ("surface", "amethyst_geode", 1, 1542, 0xc79a5c3509b9d0ff),
    ("surface", "amethyst_geode", 2, 1342, 0x60153d9903486810),
    ("surface", "amethyst_geode", 3, 2692, 0xa44b2d6f361b398a),
    ("surface", "amethyst_geode", 4, 640, 0x8bab5be226ac073b),
    ("surface", "amethyst_geode", 5, 2675, 0x33db424c6383d705),
    ("surface", "amethyst_geode", 6, 873, 0x2979921d3acd645a),
    ("edge", "amethyst_geode", 1, 1176, 0xfd5872bc842a5e9d),
    ("edge", "amethyst_geode", 2, 1100, 0x656534437ba73dc6),
    ("edge", "amethyst_geode", 3, 2590, 0x3503695902ae81f2),
    ("edge", "amethyst_geode", 4, 1023, 0xce073dad31705918),
    ("air", "amethyst_geode", 1, 0, 0xcbf29ce484222325),
    ("air", "amethyst_geode", 2, 0, 0xcbf29ce484222325),
];

#[test]
fn amethyst_geodes_match_the_vanilla_feature_cell_for_cell() {
    let bench = Bench::new();
    for (name, feature, seed, count, hash) in REFERENCE {
        let (spec, origin) = scene(name);
        assert_eq!(
            fingerprint(&bench, &spec, feature, origin, seed, (-64, 48)),
            (count, hash),
            "{name} {feature} seed {seed}"
        );
    }
}

#[test]
fn geode_noise_matches_the_vanilla_parity_noise_seeded_by_the_world() {
    let expected = [
        -0.319_719_88_f32,
        -0.192_567_87,
        0.256_909_4,
        -0.063_774_14,
        -0.493_481_4,
        -0.616_444_8,
    ];
    let noise = NoiseStack::create_legacy(&NOISE, 12345);
    for (i, want) in expected.iter().enumerate() {
        let i = i as f64;
        assert_eq!(noise.get(i * 3.0, i * 5.0 - 7.0, i * 2.0 + 1.0), *want);
    }
}

#[test]
fn amethyst_geode_is_configured_and_stays_within_its_reach() {
    let bench = Bench::new();
    assert_eq!(
        bench.configured("amethyst_geode")["type"],
        "minecraft:geode"
    );
    let reach = 17;
    for seed in 1..=6 {
        for cell in lines(&bench, STONE, "amethyst_geode", (8, 10, 8), seed, (-64, 48)) {
            let coords: Vec<i32> = cell
                .split(' ')
                .next()
                .unwrap_or("")
                .split(',')
                .filter_map(|n| n.parse().ok())
                .collect();
            assert!(coords[0].abs_diff(8) <= reach, "{cell}");
            assert!(coords[1].abs_diff(10) <= reach, "{cell}");
            assert!(coords[2].abs_diff(8) <= reach, "{cell}");
        }
    }
}
