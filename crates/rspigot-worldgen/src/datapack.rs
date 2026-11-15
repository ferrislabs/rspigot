use std::sync::OnceLock;

use rspigot_data::multi_noise::NoiseParameters;
use rspigot_domain::BlockStateId;
use serde_json::Value;

const SOURCE: &str = include_str!("../data/worldgen.json");

pub(crate) fn worldgen() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| serde_json::from_str(SOURCE).unwrap_or(Value::Null))
}

pub(crate) fn state_named(name: &str, properties: &[(&str, &str)]) -> Option<BlockStateId> {
    rspigot_blocks::BlockKind::from_name(name).and_then(|kind| kind.state_with(properties))
}

pub(crate) fn block_state(value: &Value) -> Option<BlockStateId> {
    match value {
        Value::String(name) => state_named(name, &[]),
        Value::Object(map) => {
            let name = map
                .get("id")
                .or_else(|| map.get("Name"))
                .and_then(Value::as_str)?;
            let properties = map
                .get("properties")
                .or_else(|| map.get("Properties"))
                .and_then(Value::as_object);
            let mut pairs: Vec<(&str, &str)> = properties
                .map(|map| {
                    map.iter()
                        .filter_map(|(key, value)| Some((key.as_str(), value.as_str()?)))
                        .collect()
                })
                .unwrap_or_default();
            if matches!(name, "minecraft:water" | "minecraft:lava")
                && let Some(index) = pairs.iter().position(|(key, _)| *key == "falling")
            {
                let (_, falling) = pairs.remove(index);
                if falling == "true" {
                    pairs.push(("level", "8"));
                }
            }
            state_named(name, &pairs)
        }
        _ => None,
    }
}

pub(crate) fn noise_parameters(name: &str) -> Option<NoiseParameters> {
    let entry = worldgen().get("noises")?.get(name)?;
    let modifiers: Vec<f64> = entry
        .get("amplitude_modifiers")?
        .as_array()?
        .iter()
        .filter_map(Value::as_f64)
        .collect();
    Some(NoiseParameters {
        id: Box::leak(name.to_owned().into_boxed_str()),
        base_octave: entry.get("base_octave")?.as_i64()? as i32,
        octave_count: entry.get("octave_count")?.as_u64()? as u32,
        base_amplitude: entry.get("base_amplitude")?.as_f64()?,
        amplitude_modifiers: Box::leak(modifiers.into_boxed_slice()),
    })
}

pub(crate) fn inline_noise_parameters(value: &Value) -> NoiseParameters {
    let modifiers: Vec<f64> = value
        .get("amplitude_modifiers")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default();
    NoiseParameters {
        id: "inline",
        base_octave: value
            .get("base_octave")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        octave_count: value
            .get("octave_count")
            .and_then(Value::as_u64)
            .unwrap_or(1) as u32,
        base_amplitude: value
            .get("base_amplitude")
            .and_then(Value::as_f64)
            .unwrap_or(1.0),
        amplitude_modifiers: Box::leak(modifiers.into_boxed_slice()),
    }
}
