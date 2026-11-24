use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::write::GzEncoder;
use rspigot_nbt::{Compound, Tag, write_named};
use serde_json::json;

use super::library::Library;

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("rspigot-packs-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn pack(root: &Path, name: &str) -> PathBuf {
    let dir = root.join(name);
    write(
        &dir.join("pack.mcmeta"),
        r#"{"pack":{"pack_format":99,"description":"test"}}"#,
    );
    dir
}

fn stone_template() -> Vec<u8> {
    let state = |name: &str| Compound::new().with("Name", name);
    let block = |x: i32, state: i32| {
        Compound::new()
            .with(
                "pos",
                Tag::List(vec![Tag::Int(x), Tag::Int(0), Tag::Int(0)]),
            )
            .with("state", state)
    };
    let root = Compound::new()
        .with(
            "size",
            Tag::List(vec![Tag::Int(2), Tag::Int(1), Tag::Int(1)]),
        )
        .with("DataVersion", 1)
        .with(
            "palette",
            Tag::List(vec![Tag::Compound(state("minecraft:stone"))]),
        )
        .with(
            "blocks",
            Tag::List(vec![Tag::Compound(block(0, 0)), Tag::Compound(block(1, 0))]),
        )
        .with("entities", Tag::List(Vec::new()));
    let mut raw = Vec::new();
    write_named(&mut raw, "", &root);
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&raw).unwrap();
    encoder.finish().unwrap()
}

fn single_pool(template: &str) -> String {
    json!({
        "fallback": "minecraft:empty",
        "elements": [{
            "weight": 1,
            "element": {
                "element_type": "minecraft:single_pool_element",
                "location": template,
                "processors": "minecraft:empty",
                "projection": "rigid"
            }
        }]
    })
    .to_string()
}

#[test]
fn a_pack_replaces_a_vanilla_pool_by_identifier() {
    let scratch = Scratch::new("replace");
    let dir = pack(&scratch.0, "mine");
    let before = Library::embedded()
        .pools()
        .get("minecraft:village/plains/town_centers")
        .unwrap()
        .size();
    write(
        &dir.join("data/minecraft/worldgen/template_pool/village/plains/town_centers.json"),
        &single_pool("minecraft:village/plains/town_centers/plains_fountain_01"),
    );
    let (library, issues) = Library::build(&[dir]);
    let library = Box::leak(Box::new(library));
    assert!(issues.is_empty(), "{issues:?}");
    let after = library
        .pools()
        .get("minecraft:village/plains/town_centers")
        .unwrap()
        .size();
    assert_eq!(after, 1);
    assert!(before > 1);
}

#[test]
fn the_later_pack_wins_over_the_earlier_one() {
    let scratch = Scratch::new("order");
    for (name, weight) in [("a", 1), ("b", 3)] {
        let dir = pack(&scratch.0, name);
        let pool = json!({
            "fallback": "minecraft:empty",
            "elements": [{"weight": weight, "element": {"element_type": "minecraft:empty_pool_element"}}]
        });
        write(
            &dir.join("data/mine/worldgen/template_pool/shared.json"),
            &pool.to_string(),
        );
    }
    let (packs, found) = Library::discover(&scratch.0);
    assert!(found.is_empty());
    let (library, issues) = Library::build(&packs);
    let library = Box::leak(Box::new(library));
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(library.pools().get("mine:shared").unwrap().size(), 3);
}

#[test]
fn an_invalid_file_is_reported_and_the_rest_still_loads() {
    let scratch = Scratch::new("invalid");
    let dir = pack(&scratch.0, "mine");
    write(
        &dir.join("data/mine/worldgen/template_pool/broken.json"),
        "{not json",
    );
    write(
        &dir.join("data/mine/worldgen/template_pool/fine.json"),
        &single_pool("mine:missing"),
    );
    let (library, issues) = Library::build(std::slice::from_ref(&dir));
    let library = Box::leak(Box::new(library));
    assert!(library.pools().get("mine:fine").is_some());
    assert!(library.pools().get("mine:broken").is_none());
    assert!(
        issues
            .iter()
            .any(|issue| issue.path.ends_with("broken.json")),
        "{issues:?}"
    );
}

#[test]
fn a_folder_without_pack_mcmeta_is_skipped_with_an_issue() {
    let scratch = Scratch::new("nometa");
    fs::create_dir_all(scratch.0.join("loose/data")).unwrap();
    let (packs, issues) = Library::discover(&scratch.0);
    assert!(packs.is_empty());
    assert_eq!(issues.len(), 1);
    assert!(issues[0].path.ends_with("loose"));
}

#[test]
fn a_missing_template_is_reported() {
    let scratch = Scratch::new("missing");
    let dir = pack(&scratch.0, "mine");
    write(
        &dir.join("data/mine/worldgen/template_pool/lonely.json"),
        &single_pool("mine:nowhere"),
    );
    let (_, issues) = Library::build(&[dir]);
    assert!(
        issues
            .iter()
            .any(|issue| issue.reason.contains("mine:nowhere")),
        "{issues:?}"
    );
}

#[test]
fn a_template_nbt_becomes_a_blueprint() {
    let scratch = Scratch::new("nbt");
    let dir = pack(&scratch.0, "mine");
    let path = dir.join("data/mine/structure/slab.nbt");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, stone_template()).unwrap();
    let (library, issues) = Library::build(&[dir]);
    assert!(issues.is_empty(), "{issues:?}");
    let blueprint = library.blueprint("mine:slab").unwrap();
    assert_eq!(blueprint.size, (2, 1, 1));
    assert_eq!(blueprint.blocks.len(), 2);
}

#[test]
fn a_corrupt_template_is_not_a_boot_issue_and_has_no_blueprint() {
    let scratch = Scratch::new("badnbt");
    let dir = pack(&scratch.0, "mine");
    write(&dir.join("data/mine/structure/slab.nbt"), "garbage");
    let (library, issues) = Library::build(&[dir]);
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(library.parsed(), 0);
    assert!(library.blueprint("mine:slab").is_none());
    assert!(library.blueprint("mine:slab").is_none());
    assert_eq!(library.parsed(), 1);
}

#[test]
fn a_corrupt_template_over_an_embedded_one_falls_back_to_the_embedded_one() {
    let scratch = Scratch::new("badoverride");
    let dir = pack(&scratch.0, "minecraft");
    write(
        &dir.join("data/minecraft/structure/end_city/base_floor.nbt"),
        "garbage",
    );
    let (library, issues) = Library::build(&[dir]);
    assert!(issues.is_empty(), "{issues:?}");
    let (base, _) = Library::build(&[]);
    let expected = base
        .blueprint(EMBEDDED_TEMPLATE)
        .map(|blueprint| blueprint.size);
    assert!(expected.is_some());
    assert_eq!(
        library
            .blueprint(EMBEDDED_TEMPLATE)
            .map(|blueprint| blueprint.size),
        expected
    );
}

fn slab_pack(scratch: &Scratch) -> PathBuf {
    let dir = pack(&scratch.0, "mine");
    let path = dir.join("data/mine/structure/slab.nbt");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, stone_template()).unwrap();
    dir
}

const EMBEDDED_TEMPLATE: &str = "minecraft:end_city/base_floor";

#[test]
fn building_a_base_library_parses_no_template() {
    let (library, issues) = Library::build(&[]);
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(library.parsed(), 0);
    assert!(
        library
            .blueprint_names()
            .iter()
            .any(|name| name == EMBEDDED_TEMPLATE)
    );
}

#[test]
fn building_a_library_with_a_datapack_parses_no_template() {
    let scratch = Scratch::new("lazy-none");
    let (library, issues) = Library::build(&[slab_pack(&scratch)]);
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(library.parsed(), 0);
    let names = library.blueprint_names();
    assert!(names.iter().any(|name| name == "mine:slab"));
    assert!(names.iter().any(|name| name == EMBEDDED_TEMPLATE));
    assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn the_first_lookup_parses_one_datapack_template_and_the_next_reuses_it() {
    let scratch = Scratch::new("lazy-file");
    let (library, _) = Library::build(&[slab_pack(&scratch)]);
    let first = library.blueprint("mine:slab").unwrap();
    assert_eq!(library.parsed(), 1);
    let second = library.blueprint("mine:slab").unwrap();
    assert!(std::ptr::eq(first, second));
    assert_eq!(library.parsed(), 1);
}

#[test]
fn the_first_lookup_parses_one_embedded_template_and_the_next_reuses_it() {
    let (library, _) = Library::build(&[]);
    let first = library.blueprint(EMBEDDED_TEMPLATE).unwrap();
    assert_eq!(library.parsed(), 1);
    let second = library.blueprint(EMBEDDED_TEMPLATE).unwrap();
    assert!(std::ptr::eq(first, second));
    assert_eq!(library.parsed(), 1);
    assert!(!first.blocks.is_empty());
}

#[test]
fn an_unknown_template_name_returns_none_and_parses_nothing() {
    let scratch = Scratch::new("lazy-unknown");
    let (library, _) = Library::build(&[slab_pack(&scratch)]);
    assert!(library.blueprint("mine:nowhere").is_none());
    assert!(library.blueprint("").is_none());
    assert_eq!(library.parsed(), 0);
}

#[test]
fn a_datapack_template_replaces_an_embedded_one_of_the_same_name() {
    let scratch = Scratch::new("lazy-override");
    let dir = pack(&scratch.0, "mine");
    let path = dir.join("data/minecraft/structure/end_city/base_floor.nbt");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, stone_template()).unwrap();
    let (library, issues) = Library::build(&[dir]);
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(
        library.blueprint(EMBEDDED_TEMPLATE).unwrap().size,
        (2, 1, 1)
    );
    assert_eq!(library.parsed(), 1);
}

#[test]
fn a_parsed_blueprint_keeps_no_spare_capacity() {
    let scratch = Scratch::new("lazy-tight");
    let (library, _) = Library::build(&[slab_pack(&scratch)]);
    assert!(library.blueprint("mine:slab").unwrap().is_tight());
    let shipwreck = library.blueprint("minecraft:shipwreck/with_mast").unwrap();
    assert!(shipwreck.palette_count() > 1);
    assert!(shipwreck.is_tight());
}

#[test]
fn structure_biomes_resolve_builtin_and_pack_tags() {
    let scratch = Scratch::new("tags");
    let dir = pack(&scratch.0, "mine");
    write(
        &dir.join("data/mine/tags/worldgen/biome/cosy.json"),
        r##"{"values":["minecraft:plains","#minecraft:is_forest"]}"##,
    );
    write(
        &dir.join("data/mine/worldgen/structure/hut.json"),
        &json!({
            "type": "minecraft:jigsaw",
            "biomes": "#mine:cosy",
            "step": "surface_structures",
            "terrain_adaptation": "none",
            "spawn_overrides": {},
            "start_pool": "mine:hut",
            "size": 1,
            "max_distance_from_center": 80,
            "start_height": {"absolute": 0},
            "use_expansion_hack": false
        })
        .to_string(),
    );
    let (library, issues) = Library::build(&[dir]);
    assert!(issues.is_empty(), "{issues:?}");
    let defs = super::structure::Structures::load(&library);
    let hut = defs.get("mine:hut").unwrap();
    assert!(hut.biomes.contains(&"plains".to_owned()));
    assert!(hut.biomes.contains(&"forest".to_owned()));
    assert!(hut.biomes.contains(&"birch_forest".to_owned()));
}

#[test]
fn an_invalid_structure_set_is_reported() {
    let scratch = Scratch::new("badset");
    let dir = pack(&scratch.0, "mine");
    write(
        &dir.join("data/mine/worldgen/structure_set/odd.json"),
        r#"{"placement":{"type":"minecraft:nonsense"},"structures":[]}"#,
    );
    let (_, issues) = Library::build(&[dir]);
    assert!(issues.iter().any(|issue| issue.path.ends_with("odd.json")));
}

fn structure_file(dir: &Path, name: &str, body: &serde_json::Value) {
    write(
        &dir.join(format!("data/mine/worldgen/structure/{name}.json")),
        &body.to_string(),
    );
}

#[test]
fn an_unknown_structure_type_is_reported_and_not_installed() {
    let scratch = Scratch::new("unknown-structure");
    let dir = pack(&scratch.0, "mine");
    structure_file(
        &dir,
        "odd",
        &json!({"type": "mine:castle", "biomes": [], "step": "surface_structures", "spawn_overrides": {}}),
    );
    let (library, issues) = Library::build(&[dir]);
    assert!(
        issues
            .iter()
            .any(|issue| issue.reason.contains("unknown structure type mine:castle")),
        "{issues:?}"
    );
    assert!(library.structures().get("mine:odd").is_none());
}

#[test]
fn a_known_structure_type_loads_without_an_issue() {
    let scratch = Scratch::new("known-structure");
    let dir = pack(&scratch.0, "mine");
    structure_file(
        &dir,
        "hut",
        &json!({"type": "minecraft:swamp_hut", "biomes": [], "step": "surface_structures", "spawn_overrides": {}}),
    );
    let (library, issues) = Library::build(&[dir]);
    assert!(issues.is_empty(), "{issues:?}");
    assert!(library.structures().get("mine:hut").is_some());
}

#[test]
fn a_nether_fossil_without_its_height_is_reported() {
    let scratch = Scratch::new("fossil-structure");
    let dir = pack(&scratch.0, "mine");
    structure_file(
        &dir,
        "fossil",
        &json!({"type": "minecraft:nether_fossil", "biomes": [], "step": "underground_decoration", "spawn_overrides": {}}),
    );
    let (_, issues) = Library::build(&[dir]);
    assert!(
        issues
            .iter()
            .any(|issue| issue.reason.contains("nether_fossil")),
        "{issues:?}"
    );
}

#[test]
fn loading_the_pools_parses_no_template_until_an_element_is_used() {
    let library: &'static Library = Box::leak(Box::new(Library::build(&[]).0));
    let pools = library.pools();
    assert_eq!(library.parsed(), 0);
    let element = pools
        .get("minecraft:village/plains/town_centers")
        .and_then(|pool| pool.elements.first())
        .map(|(element, _)| element)
        .expect("a vanilla pool");
    assert!(element.blueprint().is_some());
    assert_eq!(library.parsed(), 1);
}
