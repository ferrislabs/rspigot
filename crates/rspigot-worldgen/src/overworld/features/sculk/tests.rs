use super::super::probe::fingerprint;
use super::super::tests::Bench;

const CAVE: &str = "40:deepslate;-12..28/10..18/-12..28=air";

fn scene(name: &str) -> (String, (i32, i32, i32)) {
    let cave = |extra: &str| format!("{CAVE};{extra}");
    match name {
        "floor" => ("9:stone".to_owned(), (8, 10, 8)),
        "cave" => (CAVE.to_owned(), (8, 10, 8)),
        "pillars" => (
            cave(
                "4..6/10..18/4..6=stone;10..12/10..14/10..12=cobbled_deepslate;0..1/10..13/9..10=tuff",
            ),
            (8, 10, 8),
        ),
        "pool" => (
            "9:stone;-12..28/10..11/-12..28=water".to_owned(),
            (8, 10, 8),
        ),
        "sensors" => (
            "9:stone;6/10/8=sculk_sensor;10/10/6=sculk_shrieker;5..12/11/5..12=sculk_sensor"
                .to_owned(),
            (8, 10, 8),
        ),
        "ground" => (
            "9:stone;-12..4/9/-12..28=oak_planks;4..28/9/-12..2=dirt".to_owned(),
            (8, 10, 8),
        ),
        "blocked" => ("9:stone".to_owned(), (8, 9, 8)),
        "floating" => ("9:stone".to_owned(), (8, 30, 8)),
        _ => ("9:stone;8/10/8=sculk".to_owned(), (8, 10, 8)),
    }
}

const REFERENCE: [(&str, &str, i64, usize, u64); 56] = [
    ("floor", "sculk_patch_deep_dark", 1, 302, 0x48c7902606113a8),
    ("floor", "sculk_patch_deep_dark", 2, 251, 0x9c5a351c0225ac73),
    ("floor", "sculk_patch_deep_dark", 3, 294, 0xfce6f05113c08be0),
    ("floor", "sculk_patch_deep_dark", 4, 314, 0xa7262ed0304d8d50),
    ("floor", "sculk_patch_deep_dark", 5, 295, 0xc9fc299afea00515),
    ("floor", "sculk_patch_deep_dark", 6, 328, 0x7f18874db2862443),
    ("floor", "sculk_patch_deep_dark", 7, 303, 0x6b97f9dd75ba2bea),
    ("floor", "sculk_patch_deep_dark", 8, 267, 0x5d39f817ee5bfbff),
    ("cave", "sculk_patch_deep_dark", 1, 302, 0x48c7902606113a8),
    ("cave", "sculk_patch_deep_dark", 2, 251, 0x9c5a351c0225ac73),
    ("cave", "sculk_patch_deep_dark", 3, 294, 0xfce6f05113c08be0),
    ("cave", "sculk_patch_deep_dark", 4, 314, 0xa7262ed0304d8d50),
    ("cave", "sculk_patch_deep_dark", 5, 295, 0xc9fc299afea00515),
    ("cave", "sculk_patch_deep_dark", 6, 328, 0x7f18874db2862443),
    ("cave", "sculk_patch_deep_dark", 7, 303, 0x6b97f9dd75ba2bea),
    ("cave", "sculk_patch_deep_dark", 8, 267, 0x5d39f817ee5bfbff),
    (
        "pillars",
        "sculk_patch_deep_dark",
        1,
        355,
        0x8a2b133e393559ba,
    ),
    (
        "pillars",
        "sculk_patch_deep_dark",
        2,
        311,
        0x8de3d0b56ca8b0a2,
    ),
    (
        "pillars",
        "sculk_patch_deep_dark",
        3,
        334,
        0x2ffc93f692a5858b,
    ),
    (
        "pillars",
        "sculk_patch_deep_dark",
        4,
        341,
        0x55c36202b2f827a6,
    ),
    (
        "pillars",
        "sculk_patch_deep_dark",
        5,
        346,
        0xfad96cd67258bdc6,
    ),
    (
        "pillars",
        "sculk_patch_deep_dark",
        6,
        250,
        0x4ce31e45f62655cb,
    ),
    (
        "pillars",
        "sculk_patch_deep_dark",
        7,
        355,
        0x8e74e5f2d6fdd5a4,
    ),
    (
        "pillars",
        "sculk_patch_deep_dark",
        8,
        307,
        0xdadd66c792c9295c,
    ),
    ("pool", "sculk_patch_deep_dark", 1, 302, 0x9d3c394219a6ffd1),
    ("pool", "sculk_patch_deep_dark", 2, 251, 0xaf0b4eb7b4df23d9),
    ("pool", "sculk_patch_deep_dark", 3, 294, 0x72df1bd9a9135b),
    ("pool", "sculk_patch_deep_dark", 4, 314, 0x1cf43cbb14cdc961),
    ("pool", "sculk_patch_deep_dark", 5, 295, 0xae12d2028bffa0df),
    ("pool", "sculk_patch_deep_dark", 6, 328, 0x30e729940d87b058),
    (
        "sensors",
        "sculk_patch_deep_dark",
        1,
        300,
        0x2662deb76a00f03b,
    ),
    (
        "sensors",
        "sculk_patch_deep_dark",
        2,
        339,
        0x4869e06fa6f175a8,
    ),
    (
        "sensors",
        "sculk_patch_deep_dark",
        3,
        308,
        0x9a3049fe71723aaa,
    ),
    (
        "sensors",
        "sculk_patch_deep_dark",
        4,
        351,
        0x52918fc77f301cbe,
    ),
    (
        "sensors",
        "sculk_patch_deep_dark",
        5,
        330,
        0x61517557afb497ee,
    ),
    (
        "sensors",
        "sculk_patch_deep_dark",
        6,
        286,
        0xf965990f23fb7103,
    ),
    (
        "ground",
        "sculk_patch_ancient_city",
        1,
        233,
        0x61d46c13b5e99784,
    ),
    (
        "ground",
        "sculk_patch_ancient_city",
        2,
        221,
        0x23327eb3aa9b88e7,
    ),
    (
        "ground",
        "sculk_patch_ancient_city",
        3,
        204,
        0x5b96f56be2b91dfe,
    ),
    (
        "ground",
        "sculk_patch_ancient_city",
        4,
        269,
        0xf6abc11660fc397c,
    ),
    (
        "ground",
        "sculk_patch_ancient_city",
        5,
        241,
        0x3b00f59204823ee7,
    ),
    (
        "ground",
        "sculk_patch_ancient_city",
        6,
        263,
        0xa1f17c287767a846,
    ),
    (
        "cave",
        "sculk_patch_ancient_city",
        1,
        302,
        0x48c7902606113a8,
    ),
    (
        "cave",
        "sculk_patch_ancient_city",
        2,
        251,
        0x9c5a351c0225ac73,
    ),
    (
        "cave",
        "sculk_patch_ancient_city",
        3,
        294,
        0xfce6f05113c08be0,
    ),
    (
        "cave",
        "sculk_patch_ancient_city",
        4,
        315,
        0x2f8a8a6940882380,
    ),
    (
        "cave",
        "sculk_patch_ancient_city",
        5,
        296,
        0x1d3851ca51acb67,
    ),
    (
        "cave",
        "sculk_patch_ancient_city",
        6,
        329,
        0x908a8c36ecee6cd8,
    ),
    ("blocked", "sculk_patch_deep_dark", 1, 0, 0xcbf29ce484222325),
    ("blocked", "sculk_patch_deep_dark", 2, 0, 0xcbf29ce484222325),
    (
        "floating",
        "sculk_patch_deep_dark",
        1,
        0,
        0xcbf29ce484222325,
    ),
    (
        "floating",
        "sculk_patch_deep_dark",
        2,
        0,
        0xcbf29ce484222325,
    ),
    ("sculk", "sculk_patch_deep_dark", 1, 300, 0x66268427bd2ac0a8),
    ("sculk", "sculk_patch_deep_dark", 2, 261, 0xbe329f8edaa9e709),
    ("sculk", "sculk_patch_deep_dark", 3, 281, 0xdffd8dd60fd34483),
    ("sculk", "sculk_patch_deep_dark", 4, 304, 0xc3312534e5979f74),
];

#[test]
fn sculk_patches_match_the_vanilla_feature_cell_for_cell() {
    let bench = Bench::new();
    for (name, feature, seed, count, hash) in REFERENCE {
        let (spec, origin) = scene(name);
        assert_eq!(
            fingerprint(&bench, &spec, feature, origin, seed, (0, 40)),
            (count, hash),
            "{name} seed {seed}"
        );
    }
}
