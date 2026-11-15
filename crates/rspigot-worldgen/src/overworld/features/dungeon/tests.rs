use super::super::probe::{fingerprint, lines};
use super::super::tests::Bench;

const STONE: &str = "60:stone";
const TUNNEL: &str = "60:stone;-12..28/10..11/8=air";

fn scene(name: &str) -> (String, (i32, i32, i32)) {
    let with = |extra: &str| format!("{TUNNEL};{extra}");
    let origin = (8, 10, 8);
    match name {
        "tunnel" => (TUNNEL.to_owned(), origin),
        "one" => ("60:stone;8..28/10..11/8=air".to_owned(), origin),
        "solid" => (STONE.to_owned(), origin),
        "many" => ("60:stone;-12..28/10..11/4..12=air".to_owned(), origin),
        "floorless" => (with("8/9/8=air"), origin),
        "roofless" => (with("8/14/8=air"), origin),
        "undercut" => (with("5..11/8/5..11=air"), origin),
        "mixed" => (
            with(
                "4..7/9/5..11=gravel;9..12/9/5..11=dirt;4..12/14/5..11=deepslate;7/12/10=glass;12/12..13/6=water",
            ),
            origin,
        ),
        "cross" => (with("8/10..11/-12..28=air"), origin),
        "deep" => (
            "-60:bedrock,60:stone;-12..28/-57..-56/8=air".to_owned(),
            (8, -57, 8),
        ),
        _ => (with("6..10/10/6=cobblestone;6..10/12/6=air"), origin),
    }
}

const REFERENCE: [(&str, &str, i64, usize, u64); 78] = [
    ("tunnel", "monster_room", 1, 243, 0x2034b52630aa7695),
    ("tunnel", "monster_room", 2, 243, 0xd7ccb8e7481ff7c7),
    ("tunnel", "monster_room", 3, 403, 0x513a9a70276016fa),
    ("tunnel", "monster_room", 4, 243, 0x9230e8e84868702f),
    ("tunnel", "monster_room", 5, 403, 0x53df8c74d5dd07dd),
    ("tunnel", "monster_room", 6, 243, 0x6a6c84a58e3feb21),
    ("tunnel", "monster_room", 7, 313, 0xf257e8aaf6647a6f),
    ("tunnel", "monster_room", 8, 313, 0xdf989c6a80fe3d36),
    ("tunnel", "monster_room", 9, 313, 0xbc9f4d0f63c0eeb4),
    ("tunnel", "monster_room", 10, 313, 0xffb6c397f697e414),
    ("one", "monster_room", 1, 244, 0x4711f8f82f14077f),
    ("one", "monster_room", 2, 244, 0xf9c3db98fb2bb3d9),
    ("one", "monster_room", 3, 404, 0x6899e41db0bbd2eb),
    ("one", "monster_room", 4, 244, 0x89d5880e5dc334d7),
    ("one", "monster_room", 5, 404, 0xa31be7b642475f22),
    ("one", "monster_room", 6, 244, 0x61f00e0ac995f1e3),
    ("one", "monster_room", 7, 314, 0x2636f3ba94e79182),
    ("one", "monster_room", 8, 314, 0xe184a45e30f96462),
    ("one", "monster_room", 9, 314, 0xcfdeae361b609858),
    ("one", "monster_room", 10, 314, 0xf0f04d818569d8b),
    ("solid", "monster_room", 1, 0, 0xcbf29ce484222325),
    ("solid", "monster_room", 2, 0, 0xcbf29ce484222325),
    ("solid", "monster_room", 3, 0, 0xcbf29ce484222325),
    ("many", "monster_room", 1, 0, 0xcbf29ce484222325),
    ("many", "monster_room", 2, 0, 0xcbf29ce484222325),
    ("many", "monster_room", 3, 0, 0xcbf29ce484222325),
    ("floorless", "monster_room", 1, 0, 0xcbf29ce484222325),
    ("floorless", "monster_room", 2, 0, 0xcbf29ce484222325),
    ("floorless", "monster_room", 3, 0, 0xcbf29ce484222325),
    ("roofless", "monster_room", 1, 0, 0xcbf29ce484222325),
    ("roofless", "monster_room", 2, 0, 0xcbf29ce484222325),
    ("roofless", "monster_room", 3, 0, 0xcbf29ce484222325),
    ("undercut", "monster_room", 1, 243, 0xc1bea192d61d9dd7),
    ("undercut", "monster_room", 2, 243, 0x374cba3c5bdca69d),
    ("undercut", "monster_room", 3, 403, 0xe8e58ec92fea69b0),
    ("undercut", "monster_room", 4, 243, 0xc1bea192d61d9dd7),
    ("undercut", "monster_room", 5, 403, 0xb8d03f4b2ab56de3),
    ("undercut", "monster_room", 6, 243, 0x4c66c36482899b4f),
    ("undercut", "monster_room", 7, 313, 0xabffb303a2750af8),
    ("undercut", "monster_room", 8, 313, 0x9ef65707b27cd8),
    ("mixed", "monster_room", 1, 243, 0x2034b52630aa7695),
    ("mixed", "monster_room", 2, 243, 0xd7ccb8e7481ff7c7),
    ("mixed", "monster_room", 3, 402, 0x88dc428bcda57d54),
    ("mixed", "monster_room", 4, 243, 0x9230e8e84868702f),
    ("mixed", "monster_room", 5, 402, 0xbe5e1ff8225077c5),
    ("mixed", "monster_room", 6, 243, 0x6a6c84a58e3feb21),
    ("mixed", "monster_room", 7, 312, 0x66fc88dedc57010d),
    ("mixed", "monster_room", 8, 313, 0xdf989c6a80fe3d36),
    ("mixed", "monster_room", 9, 313, 0xbc9f4d0f63c0eeb4),
    ("mixed", "monster_room", 10, 312, 0xf72eff0f6e46dbf4),
    ("cross", "monster_room", 1, 241, 0x4f1d9fb81737204c),
    ("cross", "monster_room", 2, 241, 0xff3ba82cc0403280),
    ("cross", "monster_room", 3, 401, 0x2870116afc8d782c),
    ("cross", "monster_room", 4, 241, 0x3afa2d14ecb8b690),
    ("cross", "monster_room", 5, 401, 0x5927399b4bd547d2),
    ("cross", "monster_room", 6, 241, 0x413730c43dd9288f),
    ("cross", "monster_room", 7, 311, 0xaa57a07589a60c84),
    ("cross", "monster_room", 8, 311, 0xc49d4404bea94d09),
    ("cross", "monster_room", 9, 311, 0x9b1b413cc4390db7),
    ("cross", "monster_room", 10, 311, 0x6ff01e4cf730a667),
    ("deep", "monster_room", 1, 243, 0xa421705dae3b2282),
    ("deep", "monster_room", 2, 243, 0x32fe6f77d343a852),
    ("deep", "monster_room", 3, 403, 0x3cf8982997af56ff),
    ("deep", "monster_room", 4, 243, 0xa5eebc7cf8e1e074),
    ("deep", "monster_room", 5, 403, 0x786a18831e0f93d0),
    ("deep", "monster_room", 6, 243, 0x61162edbe21c01c6),
    ("deep", "monster_room", 7, 313, 0xd7c86113246e041a),
    ("deep", "monster_room", 8, 313, 0xb695c5415df1b61),
    ("chests", "monster_room", 1, 243, 0x2034b52630aa7695),
    ("chests", "monster_room", 2, 243, 0xd7ccb8e7481ff7c7),
    ("chests", "monster_room", 3, 403, 0x513a9a70276016fa),
    ("chests", "monster_room", 4, 243, 0x9230e8e84868702f),
    ("chests", "monster_room", 5, 403, 0x53df8c74d5dd07dd),
    ("chests", "monster_room", 6, 243, 0x6a6c84a58e3feb21),
    ("chests", "monster_room", 7, 313, 0xf257e8aaf6647a6f),
    ("chests", "monster_room", 8, 313, 0xdf989c6a80fe3d36),
    ("chests", "monster_room", 9, 313, 0xbc9f4d0f63c0eeb4),
    ("chests", "monster_room", 10, 313, 0xffb6c397f697e414),
];

#[test]
fn monster_rooms_match_the_vanilla_feature_cell_for_cell() {
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
fn monster_room_is_configured_and_stays_within_its_reach() {
    let bench = Bench::new();
    assert_eq!(
        bench.configured("monster_room")["type"],
        "minecraft:monster_room"
    );
    for seed in 1..=10 {
        for cell in lines(&bench, TUNNEL, "monster_room", (8, 10, 8), seed, (-64, 48)) {
            let coords: Vec<i32> = cell
                .split(' ')
                .next()
                .unwrap_or("")
                .split(',')
                .filter_map(|n| n.parse().ok())
                .collect();
            assert!(coords[0].abs_diff(8) <= 4, "{cell}");
            assert!(coords[2].abs_diff(8) <= 4, "{cell}");
            assert!((9..=14).contains(&coords[1]), "{cell}");
        }
    }
}

#[test]
fn monster_room_places_a_spawner_at_the_origin_and_chests_beside_walls() {
    let bench = Bench::new();
    let cells = lines(&bench, TUNNEL, "monster_room", (8, 10, 8), 3, (-64, 48));
    assert!(cells.iter().any(|c| c == "8,10,8 spawner"));
    let chests = cells.iter().filter(|c| c.contains(" chest ")).count();
    assert!((1..=2).contains(&chests), "{chests}");
}
