use super::super::probe::{config_fingerprint, fingerprint};
use super::super::tests::Bench;

const GRASS: &str = "90:stone,91:grass_block;5..11/10..16/5..11=air";

fn scene(name: &str) -> String {
    let grass = |extra: &str| format!("{GRASS};{extra}");
    match name {
        "grass" => GRASS.to_owned(),
        "dirt" => "90:stone,91:dirt;5..11/10..16/5..11=air".to_owned(),
        "sand" => "90:stone,91:sand;5..11/10..16/5..11=air".to_owned(),
        "wide" => grass("2..14/13..16/2..14=air;3..13/12/3..13=air"),
        "flooded" => grass("4..12/92..93/4..12=water"),
        "deepwater" => grass("4..12/92..95/4..12=water"),
        "capped" => grass("4..12/94/4..12=stone"),
        "low" => grass("6..10/93/6..10=stone"),
        "bare" => "90:stone;5..11/10..16/5..11=air".to_owned(),
        "stepped" => grass(
            "0..16/92/0..10=grass_block;0..16/92/11..16=grass_block;0..16/93/12..16=grass_block",
        ),
        "lava" => "90:stone,91:lava;5..11/10..16/5..11=air".to_owned(),
        "stone" => "90:stone,91:stone;5..11/10..16/5..11=air".to_owned(),
        "filled" => "90:stone,91:grass_block".to_owned(),
        _ => grass("9/17/9=air"),
    }
}

const REFERENCE: [(&str, &str, i64, usize, u64); 53] = [
    ("grass", "rooted_azalea_tree", 1, 1069, 0x721660db80590fe3),
    ("grass", "rooted_azalea_tree", 2, 1070, 0x7302c4038c993301),
    ("grass", "rooted_azalea_tree", 3, 1064, 0xaef627776f1ff935),
    ("grass", "rooted_azalea_tree", 4, 1067, 0x1256ffd124c1e9e6),
    ("grass", "rooted_azalea_tree", 5, 1032, 0xef698e95834a8764),
    ("grass", "rooted_azalea_tree", 6, 1027, 0xf6598aef80fd627),
    ("grass", "rooted_azalea_tree", 7, 1030, 0x8d1125dac382ee77),
    ("grass", "rooted_azalea_tree", 8, 1048, 0x823835180996f219),
    ("dirt", "rooted_azalea_tree", 1, 1069, 0x721660db80590fe3),
    ("dirt", "rooted_azalea_tree", 2, 1070, 0x7302c4038c993301),
    ("dirt", "rooted_azalea_tree", 3, 1064, 0xaef627776f1ff935),
    ("dirt", "rooted_azalea_tree", 4, 1067, 0x1256ffd124c1e9e6),
    ("dirt", "rooted_azalea_tree", 5, 1032, 0xef698e95834a8764),
    ("sand", "rooted_azalea_tree", 1, 1069, 0x721660db80590fe3),
    ("sand", "rooted_azalea_tree", 2, 1070, 0x7302c4038c993301),
    ("sand", "rooted_azalea_tree", 3, 1064, 0xaef627776f1ff935),
    ("wide", "rooted_azalea_tree", 1, 1069, 0x721660db80590fe3),
    ("wide", "rooted_azalea_tree", 2, 1070, 0x7302c4038c993301),
    ("wide", "rooted_azalea_tree", 3, 1064, 0xaef627776f1ff935),
    ("wide", "rooted_azalea_tree", 4, 1067, 0x1256ffd124c1e9e6),
    ("wide", "rooted_azalea_tree", 5, 1032, 0xef698e95834a8764),
    ("wide", "rooted_azalea_tree", 6, 1027, 0xf6598aef80fd627),
    ("flooded", "rooted_azalea_tree", 1, 1069, 0x721660db80590fe3),
    ("flooded", "rooted_azalea_tree", 2, 1070, 0x7302c4038c993301),
    ("flooded", "rooted_azalea_tree", 3, 1064, 0xaef627776f1ff935),
    ("flooded", "rooted_azalea_tree", 4, 1067, 0x1256ffd124c1e9e6),
    ("flooded", "rooted_azalea_tree", 5, 1032, 0xef698e95834a8764),
    ("deepwater", "rooted_azalea_tree", 1, 0, 0xcbf29ce484222325),
    ("deepwater", "rooted_azalea_tree", 2, 0, 0xcbf29ce484222325),
    ("deepwater", "rooted_azalea_tree", 3, 0, 0xcbf29ce484222325),
    ("capped", "rooted_azalea_tree", 1, 0, 0xcbf29ce484222325),
    ("capped", "rooted_azalea_tree", 2, 0, 0xcbf29ce484222325),
    ("capped", "rooted_azalea_tree", 3, 0, 0xcbf29ce484222325),
    ("low", "rooted_azalea_tree", 1, 0, 0xcbf29ce484222325),
    ("low", "rooted_azalea_tree", 2, 0, 0xcbf29ce484222325),
    ("low", "rooted_azalea_tree", 3, 0, 0xcbf29ce484222325),
    ("bare", "rooted_azalea_tree", 1, 0, 0xcbf29ce484222325),
    ("bare", "rooted_azalea_tree", 2, 0, 0xcbf29ce484222325),
    ("bare", "rooted_azalea_tree", 3, 0, 0xcbf29ce484222325),
    ("stepped", "rooted_azalea_tree", 1, 1085, 0xf43ed26a8ffd449a),
    ("stepped", "rooted_azalea_tree", 2, 1083, 0x17da13b21a50b60b),
    ("stepped", "rooted_azalea_tree", 3, 1085, 0x40305574133bc312),
    ("stepped", "rooted_azalea_tree", 4, 1080, 0xc276fca52090cf2e),
    ("stepped", "rooted_azalea_tree", 5, 1048, 0x749f9e5d90c26543),
    ("lava", "rooted_azalea_tree", 1, 0, 0xcbf29ce484222325),
    ("lava", "rooted_azalea_tree", 2, 0, 0xcbf29ce484222325),
    ("stone", "rooted_azalea_tree", 1, 0, 0xcbf29ce484222325),
    ("stone", "rooted_azalea_tree", 2, 0, 0xcbf29ce484222325),
    ("filled", "rooted_azalea_tree", 1, 0, 0xcbf29ce484222325),
    ("filled", "rooted_azalea_tree", 2, 0, 0xcbf29ce484222325),
    ("ceiling", "rooted_azalea_tree", 1, 1068, 0xf857b5a8fbe748ac),
    ("ceiling", "rooted_azalea_tree", 2, 1070, 0x6e3d2d7fab62aa07),
    ("ceiling", "rooted_azalea_tree", 3, 1063, 0x1f661a3b34b62216),
];

#[test]
fn root_systems_match_the_vanilla_feature_cell_for_cell() {
    let bench = Bench::new();
    for (name, feature, seed, count, hash) in REFERENCE {
        assert_eq!(
            fingerprint(&bench, &scene(name), feature, (8, 16, 8), seed, (0, 130)),
            (count, hash),
            "{name} seed {seed}"
        );
    }
}

const LEVELLED: [(&str, i64, usize, u64); 7] = [
    ("flat", 1, 1069, 0x721660db80590fe3),
    ("flat", 2, 1070, 0x7302c4038c993301),
    ("flat", 3, 1064, 0xaef627776f1ff935),
    ("pit", 1, 0, 0xcbf29ce484222325),
    ("pit", 2, 0, 0xcbf29ce484222325),
    ("pillar", 1, 0, 0xcbf29ce484222325),
    ("pillar", 2, 0, 0xcbf29ce484222325),
];

#[test]
fn the_level_test_rejects_uneven_ground_around_the_tree() {
    let bench = Bench::new();
    let mut config = bench.configured("rooted_azalea_tree");
    config["level_test_distance"] = 3.into();
    config["max_level_deviation"] = 1.into();
    for (name, seed, count, hash) in LEVELLED {
        let spec = match name {
            "pit" => format!("{GRASS};11/91/8=air"),
            "pillar" => format!("{GRASS};11/92..93/8=stone"),
            _ => GRASS.to_owned(),
        };
        let found = config_fingerprint(&bench, &spec, &config, (8, 16, 8), seed, (0, 130));
        assert_eq!(found, (count, hash), "{name} seed {seed}");
    }
}
