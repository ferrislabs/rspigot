use std::fs;
use std::path::{Path, PathBuf};

use rspigot_core::ports::ChunkGenerator;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BlockPos, ChunkPos};
use rspigot_worldgen::{NoiseGenerator, install_datapacks};
use serde_json::{Value, json};

fn write(root: &Path, path: &str, content: &Value) {
    let target = root.join(path);
    fs::create_dir_all(target.parent().unwrap_or(root)).unwrap_or_else(|e| panic!("{e}"));
    fs::write(target, content.to_string()).unwrap_or_else(|e| panic!("{e}"));
}

fn solid_world_pack(name: &str, carvers_off: bool) -> PathBuf {
    let base = std::env::temp_dir().join(format!("rspigot-e2e-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let root = base.join("solid");
    write(
        &root,
        "pack.mcmeta",
        &json!({"pack": {"pack_format": 99, "description": "e2e"}}),
    );
    let embedded: Value = serde_json::from_str(include_str!("../data/worldgen.json"))
        .unwrap_or_else(|e| panic!("{e}"));
    let variant = &embedded["terrain"]["variants"]["normal"];
    let mut router = variant["router"].clone();
    router["final_density"] = json!({"type": "minecraft:constant", "argument": 1.0});
    let settings = json!({
        "noise_router": router,
        "aquifers": variant["aquifers"],
        "sea_level": 63,
        "noise": {"min_y": -64, "height": 384},
    });
    write(
        &root,
        "data/minecraft/worldgen/noise_settings/overworld.json",
        &settings,
    );
    if carvers_off {
        for name in ["cave", "cave_extra_underground", "canyon"] {
            let mut carver = embedded["carvers"][format!("minecraft:{name}")].clone();
            carver["probability"] = json!(0.0);
            write(
                &root,
                &format!("data/minecraft/worldgen/carver/{name}.json"),
                &carver,
            );
        }
    }
    base
}

fn open_cells(column: &ChunkColumn, below: i32) -> usize {
    let mut open = 0;
    for z in 0..16 {
        for x in 0..16 {
            for y in -60..below {
                let pos = BlockPos::new(
                    column.pos().min_block_x() + x,
                    y,
                    column.pos().min_block_z() + z,
                );
                open += usize::from(column.block(pos).is_some_and(|state| state.is_air()));
            }
        }
    }
    open
}

fn generate_open_cells() -> usize {
    let generator = NoiseGenerator::new(5).without_structures();
    let dimension = Dimension::overworld();
    (0..4)
        .map(|cx| open_cells(&generator.generate(&dimension, ChunkPos::new(cx, 0)), 100))
        .sum()
}

#[test]
fn a_datapack_density_function_and_carvers_drive_the_generator() {
    let base = solid_world_pack("carvers-off", true);
    let issues = install_datapacks(&base);
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(generate_open_cells(), 0);
}
