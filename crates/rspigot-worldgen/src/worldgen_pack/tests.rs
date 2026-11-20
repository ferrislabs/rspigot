use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};

use super::*;

fn pack(name: &str, files: &[(&str, String)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "rspigot-worldgen-pack-{}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    for (path, content) in files {
        let target = root.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|e| panic!("{e}"));
        }
        fs::write(target, content).unwrap_or_else(|e| panic!("{e}"));
    }
    root
}

fn load(roots: &[PathBuf]) -> (Overlay, Vec<LoadIssue>) {
    let mut issues = Vec::new();
    let overlay = Overlay::load(roots, &mut issues);
    (overlay, issues)
}

fn text(value: Value) -> String {
    value.to_string()
}

fn mentions(issues: &[LoadIssue], needle: &str) -> bool {
    issues.iter().any(|issue| issue.reason.contains(needle))
}

fn embedded_settings() -> Value {
    let data = datapack::embedded();
    let variant = &data["terrain"]["variants"]["normal"];
    json!({
        "noise_router": variant["router"],
        "aquifers": variant["aquifers"],
        "sea_level": 40,
        "noise": {"min_y": variant["min_y"], "height": variant["height"]},
    })
}

#[test]
fn no_pack_data_leaves_the_embedded_data_untouched() {
    let (overlay, issues) = load(&[]);
    assert!(issues.is_empty());
    assert_eq!(overlay.merged(), datapack::embedded());
}

#[test]
fn noise_files_override_and_extend_the_noise_table() {
    let root = pack(
        "noise",
        &[
            (
                "data/demo/worldgen/noise/wobble.json",
                text(
                    json!({"base_octave": -3, "octave_count": 2, "base_amplitude": 1.0, "amplitude_modifiers": [0.5, 1.0]}),
                ),
            ),
            (
                "data/minecraft/worldgen/noise/surface.json",
                text(json!({"base_octave": -7, "octave_count": 3})),
            ),
        ],
    );
    let (overlay, issues) = load(&[root]);
    assert!(issues.is_empty(), "{issues:?}");
    let merged = overlay.merged();
    assert_eq!(merged["noises"]["demo:wobble"]["base_octave"], -3);
    assert_eq!(
        merged["noises"]["demo:wobble"]["amplitude_modifiers"],
        json!([0.5, 1.0])
    );
    assert_eq!(merged["noises"]["minecraft:surface"]["base_octave"], -7);
    assert_eq!(merged["noises"]["minecraft:surface"]["octave_count"], 3);
}

#[test]
fn later_packs_win_in_alphabetical_order() {
    let file = |octave: i32| {
        (
            "data/demo/worldgen/noise/wobble.json",
            text(json!({"base_octave": octave})),
        )
    };
    let first = pack("order-a", &[file(-1)]);
    let second = pack("order-b", &[file(-2)]);
    let (overlay, issues) = load(&[first, second]);
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(overlay.merged()["noises"]["demo:wobble"]["base_octave"], -2);
}

#[test]
fn malformed_files_are_reported_and_skipped() {
    let root = pack(
        "malformed",
        &[
            (
                "data/demo/worldgen/noise/broken.json",
                "{not json".to_owned(),
            ),
            (
                "data/demo/worldgen/noise/missing.json",
                text(json!({"octave_count": 1})),
            ),
            (
                "data/demo/worldgen/noise/fine.json",
                text(json!({"base_octave": -4})),
            ),
        ],
    );
    let (overlay, issues) = load(&[root]);
    assert_eq!(issues.len(), 2, "{issues:?}");
    let merged = overlay.merged();
    assert!(merged["noises"].get("demo:broken").is_none());
    assert!(merged["noises"].get("demo:missing").is_none());
    assert_eq!(merged["noises"]["demo:fine"]["base_octave"], -4);
}

#[test]
fn density_functions_are_added_and_unsupported_types_are_rejected_by_name() {
    let root = pack(
        "density",
        &[
            (
                "data/demo/worldgen/density_function/flat.json",
                text(json!({"type": "minecraft:add", "argument1": 0.0, "argument2": 1.0})),
            ),
            (
                "data/demo/worldgen/density_function/sum.json",
                text(json!({"type": "minecraft:add", "left": 0.5, "right": "demo:flat2"})),
            ),
            (
                "data/demo/worldgen/density_function/flat2.json",
                text(json!({"type": "minecraft:constant", "argument": 2.0})),
            ),
            (
                "data/demo/worldgen/density_function/island.json",
                text(json!({"type": "minecraft:end_islands"})),
            ),
            (
                "data/demo/worldgen/density_function/uses_island.json",
                text(json!({"type": "minecraft:abs", "input": "demo:island"})),
            ),
        ],
    );
    let (overlay, issues) = load(&[root]);
    let functions = &overlay.merged()["terrain"]["functions"];
    assert!(functions.get("demo:sum").is_some());
    assert!(functions.get("demo:flat2").is_some());
    assert!(functions.get("demo:island").is_none());
    assert!(functions.get("demo:uses_island").is_none());
    assert!(functions.get("demo:flat").is_none());
    assert!(
        mentions(&issues, "density function demo:island"),
        "{issues:?}"
    );
    assert!(mentions(&issues, "end_islands"));
    assert!(mentions(&issues, "density function demo:uses_island"));
}

#[test]
fn functions_naming_an_unknown_noise_are_rejected() {
    let root = pack(
        "unknown-noise",
        &[(
            "data/demo/worldgen/density_function/hum.json",
            text(
                json!({"type": "minecraft:noise", "noise": "demo:nothing", "xz_scale": 1.0, "y_scale": 1.0}),
            ),
        )],
    );
    let (overlay, issues) = load(&[root]);
    assert!(
        overlay.merged()["terrain"]["functions"]
            .get("demo:hum")
            .is_none()
    );
    assert!(
        mentions(&issues, "unknown noise demo:nothing"),
        "{issues:?}"
    );
}

#[test]
fn noise_settings_override_the_matching_terrain_variant() {
    let root = pack(
        "settings",
        &[(
            "data/minecraft/worldgen/noise_settings/overworld.json",
            text(embedded_settings()),
        )],
    );
    let (overlay, issues) = load(&[root]);
    assert!(issues.is_empty(), "{issues:?}");
    let merged = overlay.merged();
    assert_eq!(merged["terrain"]["variants"]["normal"]["sea_level"], 40);
    assert_eq!(merged["terrain"]["variants"]["amplified"]["sea_level"], 63);
}

#[test]
fn noise_settings_with_a_broken_router_are_rejected() {
    let mut settings = embedded_settings();
    settings["noise_router"]["final_density"] = json!({"type": "minecraft:weird_scaled_sampler"});
    let root = pack(
        "settings-bad",
        &[(
            "data/minecraft/worldgen/noise_settings/overworld.json",
            text(settings),
        )],
    );
    let (overlay, issues) = load(&[root]);
    assert_eq!(
        overlay.merged()["terrain"]["variants"]["normal"]["sea_level"],
        63
    );
    assert!(mentions(&issues, "weird_scaled_sampler"), "{issues:?}");
}

#[test]
fn custom_noise_settings_ids_are_reported() {
    let root = pack(
        "settings-custom",
        &[(
            "data/demo/worldgen/noise_settings/wild.json",
            text(embedded_settings()),
        )],
    );
    let (_, issues) = load(&[root]);
    assert!(mentions(&issues, "demo:wild"), "{issues:?}");
}

#[test]
fn carver_files_override_and_invalid_ones_are_rejected() {
    let mut cave = datapack::embedded()["carvers"]["minecraft:cave"].clone();
    cave["probability"] = json!(0.5);
    let root = pack(
        "carver",
        &[
            ("data/minecraft/worldgen/carver/cave.json", text(cave)),
            (
                "data/demo/worldgen/configured_carver/odd.json",
                text(json!({"type": "minecraft:nether", "probability": 1.0})),
            ),
        ],
    );
    let (overlay, issues) = load(&[root]);
    let merged = overlay.merged();
    assert_eq!(merged["carvers"]["minecraft:cave"]["probability"], 0.5);
    assert!(merged["carvers"].get("demo:odd").is_none());
    assert!(mentions(&issues, "unsupported carver type"), "{issues:?}");
}

#[test]
fn biome_files_override_features_and_carver_lists() {
    let mut plains = json!({
        "temperature": 0.8,
        "has_precipitation": true,
        "features": [[], [], [], [], [], [], [], [], [], []],
        "carvers": ["minecraft:cave"],
    });
    plains["temperature_modifier"] = json!("none");
    let root = pack(
        "biome",
        &[("data/minecraft/worldgen/biome/plains.json", text(plains))],
    );
    let (overlay, issues) = load(&[root]);
    assert!(issues.is_empty(), "{issues:?}");
    let merged = overlay.merged();
    assert_eq!(
        merged["biomes"]["minecraft:plains"]["carvers"],
        json!(["minecraft:cave"])
    );
    assert_eq!(
        merged["biomes"]["minecraft:plains"]["features"][0],
        json!([])
    );
    let carvers = crate::carver::Carvers::for_biome(&merged, "minecraft:plains");
    assert_eq!(carvers.map(|c| c.len()), Ok(1));
}

#[test]
fn a_biome_naming_a_missing_carver_is_reverted() {
    let plains = json!({
        "temperature": 0.8,
        "features": [[]],
        "carvers": ["demo:missing"],
    });
    let root = pack(
        "biome-bad",
        &[("data/minecraft/worldgen/biome/plains.json", text(plains))],
    );
    let (overlay, issues) = load(&[root]);
    let merged = overlay.merged();
    assert_eq!(
        merged["biomes"]["minecraft:plains"],
        datapack::embedded()["biomes"]["minecraft:plains"]
    );
    assert!(mentions(&issues, "no carver demo:missing"), "{issues:?}");
}

#[test]
fn dimensions_map_to_the_generators_the_engine_can_run() {
    let generator = |settings: &str, preset: &str| {
        text(json!({
            "type": "minecraft:overworld",
            "generator": {
                "type": "minecraft:noise",
                "settings": settings,
                "biome_source": {"type": "minecraft:multi_noise", "preset": preset},
            },
        }))
    };
    let root = pack(
        "dimension",
        &[
            (
                "data/demo/dimension/big.json",
                generator("minecraft:large_biomes", "minecraft:overworld"),
            ),
            (
                "data/demo/dimension/nether.json",
                generator("minecraft:overworld", "minecraft:nether"),
            ),
            (
                "data/demo/dimension/custom.json",
                generator("demo:custom", "minecraft:overworld"),
            ),
            (
                "data/demo/dimension/flat.json",
                text(
                    json!({"type": "minecraft:overworld", "generator": {"type": "minecraft:flat"}}),
                ),
            ),
        ],
    );
    let (overlay, mut issues) = load(&[root]);
    let aliases = overlay.aliases(&mut issues);
    assert_eq!(
        aliases.get("demo:big").map(String::as_str),
        Some("minecraft:large_biomes")
    );
    assert_eq!(aliases.len(), 1);
    assert_eq!(issues.len(), 3, "{issues:?}");
}

#[test]
fn unsupported_worldgen_folders_are_reported() {
    let root = pack(
        "unsupported",
        &[
            (
                "data/demo/worldgen/multi_noise_biome_source_parameter_list/x.json",
                "{}".to_owned(),
            ),
            ("data/demo/dimension_type/y.json", "{}".to_owned()),
        ],
    );
    let (_, issues) = load(&[root]);
    assert!(
        mentions(&issues, "multi_noise_biome_source_parameter_list")
            || mentions(&issues, "biome climate parameters")
    );
    assert!(mentions(&issues, "dimension type demo:y"), "{issues:?}");
}
