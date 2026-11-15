use super::biomes::NetherBiomes;
use super::density::{Corners, NetherDensity};

const SEED: i64 = 1;

const COLUMNS: [(i32, i32, &str); 8] = [
    (
        -13,
        -8,
        "nnnnnnnnnnnnnnnnnnnnnnnlllllllllaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaannnnnnnnnnnnnnnnnnnnnn",
    ),
    (
        -226,
        -76,
        "nnnnnnnnnnnnnnnnnnnnnnnnnnllllllaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaannnnnnnnnnnnnnnnnnnnn",
    ),
    (
        6,
        -295,
        "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnaaaaaaaaaaaaaaaaaannnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnn",
    ),
    (
        154,
        191,
        "nnnnnnnnnnnnnnnnnnnnnnnnllllllllaaannnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnaaaaaaaaaaannnnnnnnnnnnnnnnnnnnn",
    ),
    (
        -178,
        21,
        "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaannnnnnnnnnnnnnnnnn",
    ),
    (
        131,
        3,
        "nnnnnnnnnnnnnnnnnnnnnnnlllllllllnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnaaaaaaaaaaaaaaaaaaaannnnnnnnnnnnnnnnnnnnnn",
    ),
    (
        247,
        60,
        "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnaaaaaaaaaaaaannnnnnnnnnnnnnnnnnnnnnnnnnnnaaaaaaaaaaaaaaaaaaaaaaaaaaaannnnnnnnnnnnnnnnn",
    ),
    (
        128,
        190,
        "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnn",
    ),
];

const CLIMATE: [(i32, i32, i64, i64, &str); 24] = [
    (487, -908, -362, 719, "nether_wastes"),
    (424, -494, 1595, 3066, "nether_wastes"),
    (854, -309, 1628, -1890, "nether_wastes"),
    (321, 831, -1801, -4236, "soul_sand_valley"),
    (-253, -440, 262, 801, "nether_wastes"),
    (-910, 978, -1912, 800, "nether_wastes"),
    (-983, -983, -218, -632, "nether_wastes"),
    (165, -192, -8056, -510, "basalt_deltas"),
    (-239, 402, -3074, 2228, "basalt_deltas"),
    (-353, 715, 2476, -2595, "crimson_forest"),
    (-311, 292, -2272, -28, "nether_wastes"),
    (129, 707, -3926, 7615, "warped_forest"),
    (-54, -457, -2272, 606, "nether_wastes"),
    (633, -116, -957, 5941, "warped_forest"),
    (870, 357, -1091, -4051, "soul_sand_valley"),
    (-802, -548, 5014, -262, "crimson_forest"),
    (-947, 595, 1744, 3696, "nether_wastes"),
    (805, -288, -2464, 1931, "nether_wastes"),
    (503, 912, 4547, -5562, "soul_sand_valley"),
    (-100, 947, -1179, -1896, "nether_wastes"),
    (-901, 251, 1224, 5277, "warped_forest"),
    (326, -702, 4112, 3300, "crimson_forest"),
    (733, 697, -1096, 832, "nether_wastes"),
    (-482, 49, 923, 4940, "warped_forest"),
];

#[test]
fn climate_and_biomes_match_the_vanilla_nether_source() {
    let biomes = NetherBiomes::new(SEED);
    for (qx, qz, temperature, humidity, name) in CLIMATE {
        assert_eq!(biomes.target(qx, qz), (temperature, humidity), "{qx} {qz}");
        assert_eq!(
            super::biomes::NETHER_BIOMES[biomes.index(qx, qz)],
            format!("minecraft:{name}")
        );
    }
}

#[test]
fn terrain_columns_match_the_vanilla_noise_chunk() {
    let density = NetherDensity::new(SEED);
    let mut wrong = 0;
    let mut total = 0;
    for (x, z, expected) in COLUMNS {
        let (x0, z0) = (x.div_euclid(16) * 16, z.div_euclid(16) * 16);
        let corners = Corners::build(&density, x0, z0, 0, 128);
        let mut column = [b'a'; 128];
        corners.fill(|cx, y, cz, value| {
            if cx as i32 == x - x0 && cz as i32 == z - z0 {
                column[y] = if value > 0.0 {
                    b'n'
                } else if y < 32 {
                    b'l'
                } else {
                    b'a'
                };
            }
        });
        for (y, (got, want)) in column.iter().zip(expected.bytes()).enumerate() {
            total += 1;
            if *got != want {
                wrong += 1;
                eprintln!("{x} {z} y={y} got {} want {}", *got as char, want as char);
            }
        }
    }
    assert!(wrong * 500 <= total, "{wrong} of {total}");
}
