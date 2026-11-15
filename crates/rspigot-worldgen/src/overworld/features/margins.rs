use super::probe::world;
use super::tests::Bench;

const WORLDS: [&str; 3] = [
    "68:stone,69:grass_block",
    "40:stone,41:sand,66:water",
    "70:stone;0..15/60..66/0..15=air",
];
const SEEDS: i64 = 8;
const ORIGIN: (i32, i32, i32) = (8, 70, 8);

fn extent(bench: &Bench, name: &str, spec: &str, seed: i64) -> i32 {
    let mut region = world(bench, spec);
    region.changed.clear();
    bench.place(&mut region, name, ORIGIN, seed);
    region
        .changed
        .iter()
        .map(|(x, z)| (x - ORIGIN.0).abs().max((z - ORIGIN.2).abs()))
        .max()
        .unwrap_or(0)
}

#[test]
fn reach_covers_every_write_of_every_configured_feature() {
    let bench = Bench::new();
    let mut failures = Vec::new();
    for full in bench.configured_names() {
        let name = full.strip_prefix("minecraft:").unwrap_or(&full).to_owned();
        let reach = bench.reach_of(&full);
        for spec in WORLDS {
            for seed in 0..SEEDS {
                let wrote = extent(&bench, &name, spec, seed);
                if wrote > reach {
                    failures.push(format!("{name} seed {seed} wrote {wrote} > reach {reach}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_overworld_biome_feature_is_supported() {
    assert_eq!(
        unsupported(rspigot_data::multi_noise::overworld_biomes()),
        Vec::<String>::new()
    );
}

#[test]
fn every_nether_biome_feature_is_supported() {
    assert_eq!(
        unsupported(&crate::nether::NETHER_BIOMES),
        Vec::<String>::new()
    );
}

#[test]
fn every_end_biome_feature_is_supported_but_the_ones_the_end_places_itself() {
    let handled = [
        "minecraft:end_spike",
        "minecraft:end_platform",
        "minecraft:end_gateway",
        "minecraft:end_gateway_return",
        "minecraft:end_gateway_delayed",
        "minecraft:end_podium_active",
        "minecraft:end_podium_inactive",
    ];
    let missing: Vec<String> = unsupported(&crate::end::END_BIOMES)
        .into_iter()
        .filter(|name| !handled.contains(&name.as_str()))
        .collect();
    assert_eq!(missing, Vec::<String>::new());
}

fn unsupported(biomes: &[&str]) -> Vec<String> {
    use super::bodies::Feature;
    use serde_json::Value;

    let data = crate::datapack::worldgen();
    let configured = data.get("configured").and_then(Value::as_object);
    let placed = data.get("placed").and_then(Value::as_object);
    let mut missing = std::collections::BTreeSet::new();
    let mut stack: Vec<String> = Vec::new();
    for name in biomes {
        let Some(steps) = data
            .get("biomes")
            .and_then(|all| all.get(name))
            .and_then(|biome| biome.get("features"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        for step in steps {
            for feature in step
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                stack.push(feature.to_owned());
            }
        }
    }
    let bench = Bench::new();
    let mut seen = std::collections::BTreeSet::new();
    while let Some(name) = stack.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        let target = placed
            .and_then(|all| all.get(&name))
            .and_then(|entry| entry.get("feature"))
            .and_then(Value::as_str)
            .map_or(name.clone(), str::to_owned);
        let Some(definition) = configured.and_then(|all| all.get(&target)) else {
            continue;
        };
        if matches!(*bench.feature_of(&target), Feature::Unsupported) {
            missing.insert(target.clone());
        }
        let mut found = Vec::new();
        strings(definition, &mut found);
        stack.extend(found);
    }
    missing.into_iter().collect()
}

fn strings(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => out.push(text.clone()),
        serde_json::Value::Array(items) => items.iter().for_each(|item| strings(item, out)),
        serde_json::Value::Object(map) => map.values().for_each(|item| strings(item, out)),
        _ => {}
    }
}
