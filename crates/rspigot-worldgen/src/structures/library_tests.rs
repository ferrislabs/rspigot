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
    let (library, issues) = Library::build(&[dir.clone()]);
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
fn a_corrupt_template_is_reported() {
    let scratch = Scratch::new("badnbt");
    let dir = pack(&scratch.0, "mine");
    write(&dir.join("data/mine/structure/slab.nbt"), "garbage");
    let (library, issues) = Library::build(&[dir]);
    assert!(library.blueprint("mine:slab").is_none());
    assert!(issues.iter().any(|issue| issue.path.ends_with("slab.nbt")));
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
