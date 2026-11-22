use std::fs;
use std::path::{Path, PathBuf};

use rspigot_core::ports::ChunkGenerator;
use rspigot_core::worlds::{GeneratorSpec, WorldKind, WorldSpec};
use rspigot_domain::{BlockPos, ChunkPos, Identifier};
use rspigot_worldgen::{WorldGenerator, install_datapacks};
use serde_json::{Value, json};

fn write(root: &Path, path: &str, content: &Value) {
    let target = root.join(path);
    fs::create_dir_all(target.parent().unwrap_or(root)).unwrap_or_else(|e| panic!("{e}"));
    fs::write(target, content.to_string()).unwrap_or_else(|e| panic!("{e}"));
}

fn pack() -> PathBuf {
    let base = std::env::temp_dir().join(format!("rspigot-dimensions-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let root = base.join("short");
    write(
        &root,
        "pack.mcmeta",
        &json!({"pack": {"pack_format": 99, "description": "dimensions"}}),
    );
    let embedded: Value = serde_json::from_str(include_str!("../data/worldgen.json"))
        .unwrap_or_else(|e| panic!("{e}"));
    let variant = &embedded["terrain"]["variants"]["normal"];
    let mut router = variant["router"].clone();
    router["final_density"] = json!({"type": "minecraft:constant", "argument": 1.0});
    write(
        &root,
        "data/demo/dimension_type/short.json",
        &json!({
            "has_skylight": true,
            "has_ceiling": false,
            "min_y": 0,
            "height": 128,
            "ambient_light": 0.0,
        }),
    );
    write(
        &root,
        "data/demo/dimension/short.json",
        &json!({
            "type": "demo:short",
            "generator": {
                "type": "minecraft:noise",
                "biome_source": {"type": "minecraft:multi_noise", "preset": "minecraft:overworld"},
                "settings": {
                    "noise_router": router,
                    "aquifers": variant["aquifers"],
                    "sea_level": 40,
                    "noise": {"min_y": 0, "height": 128},
                    "surface_rule": {"type": "minecraft:block", "result_state": {"Name": "minecraft:sand"}},
                },
            },
        }),
    );
    base
}

#[test]
fn a_datapack_dimension_with_its_own_type_and_inline_settings_generates() {
    let issues = install_datapacks(&pack());
    assert!(issues.is_empty(), "{issues:?}");
    let spec = WorldSpec {
        name: "short".to_owned(),
        kind: WorldKind::Overworld,
        seed: 9,
        generator: GeneratorSpec::Datapack(
            Identifier::parse("demo:short").unwrap_or_else(|e| panic!("{e}")),
        ),
        structures: false,
    };
    let dimension = spec.dimension();
    assert_eq!(dimension.height.min_y, 0);
    assert_eq!(dimension.height.height, 128);
    assert_eq!(dimension.dimension_type.as_str(), "demo:short");
    let generator = WorldGenerator::for_world(&spec).unwrap_or_else(|e| panic!("{e}"));
    let column = generator.generate(&dimension, ChunkPos::new(0, 0));
    let solid = |y: i32| {
        column
            .block(BlockPos::new(8, y, 8))
            .is_some_and(|state| !state.is_air())
    };
    assert!(solid(0), "the world floor");
    assert!(solid(60), "the middle of the world");
    assert!(solid(127), "the world top");
    assert_eq!(
        column.block(BlockPos::new(8, 60, 8)),
        Some(rspigot_blocks::BlockKind::Sand.default_state())
    );
    assert!(column.block(BlockPos::new(8, 128, 8)).is_none());
    assert!(column.block(BlockPos::new(8, -1, 8)).is_none());
}
