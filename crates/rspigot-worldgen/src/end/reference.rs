use super::End;
use crate::nether::density::{Corners, END_LATTICE};
use crate::resources::Resources;

const SEED: i64 = 1;

const EROSION: [(i32, i32, f32); 40] = [
    (-2011, 3596, 0.22156554),
    (748, -3465, -0.44134712),
    (759, -3343, -0.014959812),
    (3273, -1224, -0.010118961),
    (3113, 893, -0.16513348),
    (3446, -3514, -0.7421875),
    (-3174, -2142, -0.4700085),
    (1295, -702, -0.84375),
    (2011, -2832, 0.40889794),
    (-561, -156, -0.84375),
    (-409, -2354, -0.32940686),
    (-3694, -388, -0.04179597),
    (3201, -506, -0.84375),
    (-2938, 788, -0.63190913),
    (-2007, 1337, 0.34375),
    (-3474, -270, -0.4160005),
    (-3334, 3252, 0.159733),
    (-3950, 2106, -0.67691326),
    (-797, -2085, -0.22199959),
    (1415, 431, 0.5265879),
    (-3648, 2242, -0.84375),
    (-2090, 2330, -0.84375),
    (3604, -549, 0.15625),
    (-3316, -3453, -0.37279356),
    (-2395, -2167, -0.43446076),
    (2398, 1628, -0.70376885),
    (-2252, 3219, -0.1015625),
    (118, 126, -0.62989664),
    (-3240, 3572, -0.18168938),
    (3742, 3978, 0.18841994),
    (246, 2703, 0.5625),
    (776, 1970, -0.2157504),
    (3999, 3583, -0.7728033),
    (-2441, 1232, -0.2157504),
    (-3130, 2509, -0.8040664),
    (-3762, -2892, 0.17181921),
    (-3300, -645, -0.265625),
    (-461, 379, -0.84375),
    (-1654, -3011, -0.29096842),
    (-1824, -998, -0.5),
];

const BIOMES: [(i32, i32, &str); 40] = [
    (2989, 2596, "end_midlands"),
    (-2252, -2465, "small_end_islands"),
    (-241, -2343, "end_midlands"),
    (2273, 1776, "small_end_islands"),
    (2113, -107, "end_midlands"),
    (446, 1486, "small_end_islands"),
    (-2174, -1142, "end_highlands"),
    (-1705, -1702, "end_midlands"),
    (-2989, 2168, "end_midlands"),
    (-1561, 2844, "end_midlands"),
    (-1409, 646, "end_midlands"),
    (-694, -1388, "end_highlands"),
    (2201, -1506, "small_end_islands"),
    (-1938, -2212, "end_barrens"),
    (993, 337, "small_end_islands"),
    (-474, 2730, "end_highlands"),
    (-2334, -1748, "small_end_islands"),
    (-2950, 1106, "small_end_islands"),
    (203, 915, "small_end_islands"),
    (2415, -2569, "end_highlands"),
    (-2648, -2758, "small_end_islands"),
    (2910, -670, "small_end_islands"),
    (-1396, -1549, "small_end_islands"),
    (-316, 1547, "small_end_islands"),
    (605, 2833, "small_end_islands"),
    (-602, 628, "end_midlands"),
    (2748, -1781, "small_end_islands"),
    (-882, -2874, "small_end_islands"),
    (1760, 572, "small_end_islands"),
    (-1258, 2978, "end_midlands"),
    (-2754, -297, "small_end_islands"),
    (-224, -1030, "small_end_islands"),
    (2999, 2583, "end_midlands"),
    (-1441, 232, "small_end_islands"),
    (-2130, -2491, "small_end_islands"),
    (-762, 2108, "small_end_islands"),
    (-300, 355, "small_end_islands"),
    (539, -621, "small_end_islands"),
    (1346, -11, "end_highlands"),
    (1176, -1998, "small_end_islands"),
];

const COLUMNS: [(i32, i32, &str); 16] = [
    (
        19,
        46,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaeaaaaaaaaeeeeeeeeeeeeaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ),
    (
        -2,
        85,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ),
    (
        89,
        27,
        "aaaaaaaaaaaaaaaaaaaaaaaaaeeeeeeeeeeeeeeeaeeeeeeeeeeeeeeeeeaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ),
    (
        83,
        6,
        "aaaaaaaaaaaaaaaaaeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ),
    (
        43,
        -77,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaeeeeeeeaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ),
    (
        -64,
        16,
        "aaaaaaaaaaaaaaaaaeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ),
    (
        2489,
        -1904,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaeeeeeeeeeeeeeeeeeeeeeeeeeeeeaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ),
    (
        1248,
        1035,
        "aaaaaaaaaaaaaaaaaaaaaaaeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ),
    (
        2259,
        -843,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaeeeeeeeeeeeeeaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ),
    (
        -1227,
        -724,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ),
    (
        1613,
        393,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ),
    (
        -1054,
        986,
        "aaaaaaaaaaaaaaaaeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ),
    (
        2326,
        -642,
        "aaaaaaaaaaaaaaaaaaaaaaeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ),
    (
        -1205,
        -1202,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ),
    (
        -1489,
        1668,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaeeeeeeeeeaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ),
    (
        1939,
        1344,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaeeaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ),
];

#[test]
fn the_islands_function_matches_the_vanilla_erosion_router() {
    let end = End::new(SEED as u64, &Resources::resolve().blocks);
    for (x, z, expected) in EROSION {
        let got = end.islands_erosion(x, z);
        assert!(
            (got - expected).abs() < 1e-5,
            "{x} {z}: {got} vs {expected}"
        );
    }
}

#[test]
fn biomes_follow_the_vanilla_end_biome_source() {
    let end = End::new(SEED as u64, &Resources::resolve().blocks);
    for (qx, qz, name) in BIOMES {
        let got = super::END_BIOMES[usize::from(end.biome_index(qx, qz))];
        assert_eq!(got, format!("minecraft:{name}"), "{qx} {qz}");
    }
}

#[test]
fn terrain_columns_match_the_vanilla_noise_chunk() {
    let end = End::new(SEED as u64, &Resources::resolve().blocks);
    let (mut wrong, mut total) = (0, 0);
    for (x, z, expected) in COLUMNS {
        let (x0, z0) = (x.div_euclid(16) * 16, z.div_euclid(16) * 16);
        let corners = Corners::build(
            |x, y, z| end.raw_density(x, y, z),
            END_LATTICE,
            x0,
            z0,
            0,
            128,
        );
        let mut column = [b'a'; 128];
        corners.fill(|cx, y, cz, value| {
            if cx as i32 == x - x0 && cz as i32 == z - z0 && value > 0.0 {
                column[y] = b'e';
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

#[test]
fn outer_highlands_grow_chorus_and_small_islands_float_in_the_void() {
    use rspigot_domain::world::Dimension;
    use rspigot_domain::{BlockPos, ChunkPos};

    let resources = Resources::resolve();
    let end = End::new(SEED as u64, &resources.blocks);
    let dimension = Dimension::end();
    let (mut chorus, mut islands, mut checked) = (0, 0, 0);
    'search: for cx in (-160..160).step_by(3) {
        for cz in (-160..160).step_by(3) {
            if cx * cx + cz * cz < 4200 {
                continue;
            }
            let index = end.biome_index(cx * 4, cz * 4);
            if index != 1 && index != 3 {
                continue;
            }
            let column = end.generate(&resources, &dimension, ChunkPos::new(cx, cz));
            for y in 0..128 {
                for z in 0..16 {
                    for x in 0..16 {
                        let at = BlockPos::new(cx * 16 + x, y, cz * 16 + z);
                        let Some(state) = column.block(at) else {
                            continue;
                        };
                        let name = state.kind().map_or("", |kind| kind.name());
                        if name == "minecraft:chorus_plant" || name == "minecraft:chorus_flower" {
                            chorus += 1;
                        }
                        if index == 3 && name == "minecraft:end_stone" {
                            islands += 1;
                        }
                    }
                }
            }
            checked += 1;
            if checked >= 60 {
                break 'search;
            }
        }
    }
    assert!(chorus > 0, "chorus {chorus}");
    assert!(islands > 0, "islands {islands}");
}
