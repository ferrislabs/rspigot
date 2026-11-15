use super::super::probe::fingerprint;
use super::super::tests::Bench;

const OCEAN: &str = "30:stone,31:sand,62:water";

fn scene(name: &str) -> (String, (i32, i32, i32)) {
    let with = |extra: &str| format!("{OCEAN};{extra}");
    match name {
        "flat" => (OCEAN.to_owned(), (8, 34, 8)),
        "deep" => (OCEAN.to_owned(), (8, 33, 8)),
        "far" => (OCEAN.to_owned(), (8, 40, 8)),
        "ground" => (OCEAN.to_owned(), (8, 28, 8)),
        "bumps" => (
            with("7..9/32/7..8=stone;10/32/9=stone;8/31/10=water;7/31/9=water"),
            (8, 35, 8),
        ),
        "pits" => (
            with("9/31/9..10=water;7..8/31/7=air;8/30/9=water"),
            (8, 32, 8),
        ),
        _ => (with("7..9/32/7..9=stone_slab"), (8, 33, 8)),
    }
}

const REFERENCE: [(&str, &str, i64, usize, u64); 38] = [
    ("flat", "underwater_magma", 1, 6, 0xccd904aa727fa67d),
    ("flat", "underwater_magma", 2, 9, 0x521c88c3fa9e707c),
    ("flat", "underwater_magma", 3, 9, 0x8eb7a56c1f5c0fd5),
    ("flat", "underwater_magma", 4, 7, 0x5306b7aa06d48193),
    ("flat", "underwater_magma", 5, 7, 0x4078ede31e6ad20c),
    ("flat", "underwater_magma", 6, 10, 0x7769febbd8b2bf5a),
    ("flat", "underwater_magma", 7, 3, 0x4075262bd2df00ee),
    ("flat", "underwater_magma", 8, 6, 0x4bd5fdd490733a0b),
    ("deep", "underwater_magma", 1, 6, 0xccd904aa727fa67d),
    ("deep", "underwater_magma", 2, 9, 0x521c88c3fa9e707c),
    ("far", "underwater_magma", 1, 0, 0xcbf29ce484222325),
    ("far", "underwater_magma", 2, 0, 0xcbf29ce484222325),
    ("ground", "underwater_magma", 1, 0, 0xcbf29ce484222325),
    ("ground", "underwater_magma", 2, 0, 0xcbf29ce484222325),
    ("bumps", "underwater_magma", 1, 2, 0x509b9f611f92932),
    ("bumps", "underwater_magma", 2, 3, 0xd184e776215dc14a),
    ("bumps", "underwater_magma", 3, 2, 0x808cdc86cd80fca5),
    ("bumps", "underwater_magma", 4, 3, 0xab35656e6ac23d46),
    ("bumps", "underwater_magma", 5, 3, 0xab35656e6ac23d46),
    ("bumps", "underwater_magma", 6, 3, 0xab35656e6ac23d46),
    ("bumps", "underwater_magma", 7, 2, 0x8952e27f3f5e6ed1),
    ("bumps", "underwater_magma", 8, 1, 0x6a9c187d395062c),
    ("bumps", "underwater_magma", 9, 4, 0x21c0aa981c6916c8),
    ("bumps", "underwater_magma", 10, 4, 0xcf8536ef4287f4ea),
    ("pits", "underwater_magma", 1, 1, 0x211af763f05b0782),
    ("pits", "underwater_magma", 2, 3, 0x8b81e87cf3286a0c),
    ("pits", "underwater_magma", 3, 3, 0xfdd3ee8e48e54546),
    ("pits", "underwater_magma", 4, 4, 0x5c5acd7c86376d4),
    ("pits", "underwater_magma", 5, 3, 0xccd7ef7004a95349),
    ("pits", "underwater_magma", 6, 5, 0x6966599e133d5ccb),
    ("pits", "underwater_magma", 7, 2, 0x5359a9c64531e83a),
    ("pits", "underwater_magma", 8, 2, 0x5359a9c64531e83a),
    ("slab", "underwater_magma", 1, 3, 0x692c60c5413db324),
    ("slab", "underwater_magma", 2, 4, 0xca0055ab5ed37678),
    ("slab", "underwater_magma", 3, 3, 0xbfc676accbe41f45),
    ("slab", "underwater_magma", 4, 6, 0x8114d1f61c09223),
    ("slab", "underwater_magma", 5, 4, 0xde4ac46bb6e9cb24),
    ("slab", "underwater_magma", 6, 6, 0x8114d1f61c09223),
];

#[test]
fn underwater_magma_matches_the_vanilla_feature_cell_for_cell() {
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
