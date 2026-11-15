use super::super::probe::{fingerprint, lines};
use super::super::tests::Bench;

fn scene(name: &str) -> (String, (i32, i32, i32)) {
    match name {
        "sand" => ("60:sand".to_owned(), (8, 60, 8)),
        "wet" => ("60:sand;0..16/58..62/0..16=water".to_owned(), (8, 60, 8)),
        "hole" => ("60:sand;7..9/55..59/7..9=air".to_owned(), (8, 60, 8)),
        "flowing" => ("60:sand;0..16/59..63/6=water".to_owned(), (8, 60, 8)),
        "stone" => ("60:stone".to_owned(), (8, 50, 8)),
        "pool" => ("60:stone;0..16/40..48/0..16=water".to_owned(), (8, 50, 8)),
        "cave" => ("60:stone;0..16/42..46/0..16=air".to_owned(), (8, 50, 8)),
        _ => ("-60:bedrock,60:deepslate".to_owned(), (8, 20, 8)),
    }
}

const REFERENCE: [(&str, &str, i64, usize, u64); 54] = [
    ("sand", "desert_well", 1, 108, 0x25eb0e5c4f6ca8da),
    ("sand", "desert_well", 2, 108, 0xdabb9ec324bb7ed2),
    ("sand", "desert_well", 3, 108, 0x54470f20515986c),
    ("sand", "desert_well", 4, 108, 0xa5f1605f5d20d4eb),
    ("sand", "desert_well", 5, 108, 0x3bacaabab91e4798),
    ("sand", "desert_well", 6, 108, 0x25eb0e5c4f6ca8da),
    ("wet", "desert_well", 1, 117, 0xcdc5086c5f57ef0c),
    ("wet", "desert_well", 2, 117, 0xac315cff208da9a0),
    ("wet", "desert_well", 3, 117, 0xb12f4d54a8af55a8),
    ("wet", "desert_well", 4, 117, 0x33e1ce27cd324d90),
    ("hole", "desert_well", 1, 112, 0x8a10d7b7384daea5),
    ("hole", "desert_well", 2, 112, 0xb5ac952ff19ae777),
    ("flowing", "desert_well", 1, 108, 0x583401c8d47066af),
    ("flowing", "desert_well", 2, 108, 0xf8c1baa6358a675),
    ("stone", "sulfur_spring", 1, 613, 0x99eeaca98b687e8),
    ("stone", "sulfur_spring", 2, 655, 0xc36ba5168cd9debb),
    ("stone", "sulfur_spring", 3, 1278, 0x21447806669ab610),
    ("stone", "sulfur_spring", 4, 548, 0xc015ffd318e2da5),
    ("stone", "sulfur_spring", 5, 544, 0xed0392beea212983),
    ("stone", "sulfur_spring", 6, 613, 0xc2b31ac8b3e90ef0),
    ("stone", "sulfur_spring", 7, 1025, 0xb3aaeeeb101103f9),
    ("stone", "sulfur_spring", 8, 560, 0x673700a90035e3d3),
    ("stone", "sulfur_spring", 9, 663, 0xef5d72b86262887c),
    ("stone", "sulfur_spring", 10, 1023, 0x79ddf90303c89d7b),
    ("stone", "sulfur_spring", 11, 608, 0x815583ab4c4ce045),
    ("stone", "sulfur_spring", 12, 1027, 0xf55d45b6a504efdf),
    ("pool", "sulfur_spring", 1, 683, 0xb642741fa49466f8),
    ("pool", "sulfur_spring", 2, 737, 0xbfaf9f794dc7fabb),
    ("pool", "sulfur_spring", 3, 1388, 0x3164d0b1288313f4),
    ("pool", "sulfur_spring", 4, 616, 0x7efe4d2021ddb304),
    ("pool", "sulfur_spring", 5, 611, 0xf681b6ebc8eff0bd),
    ("pool", "sulfur_spring", 6, 685, 0x44e440daaeba5e27),
    ("pool", "sulfur_spring", 7, 1153, 0xfa773620950cfce1),
    ("pool", "sulfur_spring", 8, 608, 0xe12ba09c758941fb),
    ("pool", "sulfur_spring", 9, 746, 0xd9c74d738ad4110a),
    ("pool", "sulfur_spring", 10, 1148, 0x2c7d45acc8d80c79),
    ("pool", "sulfur_spring", 11, 676, 0x6c12786985b330b),
    ("pool", "sulfur_spring", 12, 1183, 0xacf18a22b345ce6d),
    ("cave", "sulfur_spring", 1, 672, 0xbbf456801208fc1a),
    ("cave", "sulfur_spring", 2, 732, 0x146d7e7da12aafec),
    ("cave", "sulfur_spring", 3, 1365, 0x5ca6bed5766c509),
    ("cave", "sulfur_spring", 4, 609, 0x80cc2b11352d82c),
    ("cave", "sulfur_spring", 5, 605, 0x7a16baf136dcbcd5),
    ("cave", "sulfur_spring", 6, 672, 0x52503d4ce6418fb2),
    ("cave", "sulfur_spring", 7, 1116, 0x581069639743f52d),
    ("cave", "sulfur_spring", 8, 607, 0xab3a40f453b02ac8),
    ("deep", "sulfur_spring", 1, 689, 0x3fc90f5421be9246),
    ("deep", "sulfur_spring", 2, 749, 0xf9d4125c9d5e4b4b),
    ("deep", "sulfur_spring", 3, 1406, 0x7849c1d03c36bc53),
    ("deep", "sulfur_spring", 4, 619, 0x69f5dfb713d12cae),
    ("deep", "sulfur_spring", 5, 615, 0x9a02a7c71c079484),
    ("deep", "sulfur_spring", 6, 689, 0x3501acd07cdc84dc),
    ("deep", "sulfur_spring", 7, 1162, 0x60358f344f7c37af),
    ("deep", "sulfur_spring", 8, 616, 0xcbf1f46e71fb1c2e),
];

#[test]
fn templates_match_the_vanilla_feature_cell_for_cell() {
    let bench = Bench::new();
    for (name, feature, seed, count, hash) in REFERENCE {
        let (spec, origin) = scene(name);
        assert_eq!(
            fingerprint(&bench, &spec, feature, origin, seed, (-64, 80)),
            (count, hash),
            "{name} {feature} seed {seed}"
        );
    }
}

#[test]
fn the_desert_well_is_a_sandstone_basin_with_water_and_buried_suspicious_sand() {
    let bench = Bench::new();
    let (spec, origin) = scene("sand");
    let cells = lines(&bench, &spec, "desert_well", origin, 1, (-64, 80));
    let count = |needle: &str| cells.iter().filter(|c| c.contains(needle)).count();
    assert!(count(" sandstone") > 90, "{}", count(" sandstone"));
    assert!(count(" water level=0") >= 4);
    assert_eq!(count(" sandstone_slab"), 12);
    assert!((1..=10).contains(&count(" suspicious_sand")));
    for cell in &cells {
        let coords: Vec<i32> = cell
            .split(' ')
            .next()
            .unwrap_or("")
            .split(',')
            .filter_map(|n| n.parse().ok())
            .collect();
        assert!(
            coords[0].abs_diff(8) <= 2 && coords[2].abs_diff(8) <= 2,
            "{cell}"
        );
        assert!((58..=64).contains(&coords[1]), "{cell}");
    }
}

#[test]
fn slabs_placed_in_source_water_are_waterlogged_and_the_others_are_not() {
    let bench = Bench::new();
    let (spec, origin) = scene("wet");
    let cells = lines(&bench, &spec, "desert_well", origin, 1, (-64, 80));
    let slabs = |flag: &str| {
        cells
            .iter()
            .filter(|c| c.contains("sandstone_slab") && c.contains(flag))
            .count()
    };
    assert_eq!(slabs("waterlogged=true"), 4);
    assert_eq!(slabs("waterlogged=false"), 8);
}

#[test]
fn the_same_seed_always_places_the_same_spring() {
    let bench = Bench::new();
    let (spec, origin) = scene("stone");
    let first = lines(&bench, &spec, "sulfur_spring", origin, 3, (-64, 80));
    assert_eq!(
        first,
        lines(&bench, &spec, "sulfur_spring", origin, 3, (-64, 80))
    );
    assert!(first.iter().any(|c| c.contains(" sulfur")));
    assert!(first.iter().any(|c| c.contains(" potent_sulfur")));
}

#[test]
fn template_features_are_configured_from_the_datapack() {
    let bench = Bench::new();
    assert_eq!(bench.configured("desert_well")["type"], "minecraft:overlay");
    let spring = bench.configured("sulfur_spring");
    assert!(spring.to_string().contains("minecraft:template"));
}
