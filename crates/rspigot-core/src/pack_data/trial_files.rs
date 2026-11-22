use rspigot_data::trial_spawner::{Config, ConfigOverlay, WeightedSpawn};
use serde_json::Value;

use super::DataError;
use super::registry_overlay::RegistryOverlay;

const REGISTRY: &str = "minecraft:trial_spawner";

fn invalid<T>(reason: impl Into<String>) -> Result<T, DataError> {
    Err(DataError::Invalid(reason.into()))
}

fn leak(text: &str) -> &'static str {
    Box::leak(text.to_owned().into_boxed_str())
}

fn qualified(name: &str) -> String {
    if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    }
}

pub fn snbt(value: &Value) -> Result<String, DataError> {
    Ok(match value {
        Value::Bool(flag) => format!("{}b", u8::from(*flag)),
        Value::Number(number) => match number.as_i64() {
            Some(whole) => whole.to_string(),
            None => format!("{}d", number.as_f64().unwrap_or_default()),
        },
        Value::String(text) => format!("{text:?}"),
        Value::Array(items) => format!(
            "[{}]",
            items
                .iter()
                .map(snbt)
                .collect::<Result<Vec<_>, _>>()?
                .join(",")
        ),
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut parts = Vec::new();
            for key in keys {
                parts.push(format!("{key}:{}", snbt(&map[key])?));
            }
            format!("{{{}}}", parts.join(","))
        }
        Value::Null => return invalid("null in spawn data"),
    })
}

fn spawn(entry: &Value) -> Result<WeightedSpawn, DataError> {
    let Some(data) = entry.get("data") else {
        return invalid("a spawn potential has no data");
    };
    let Some(entity) = data.get("entity") else {
        return invalid("a spawn potential has no entity");
    };
    let equipment = data.get("equipment");
    Ok(WeightedSpawn {
        entity: leak(&snbt(entity)?),
        equipment: equipment
            .and_then(|equipment| equipment.get("loot_table"))
            .and_then(Value::as_str)
            .map(|table| leak(&qualified(table))),
        equipment_drop_chance: equipment
            .and_then(|equipment| equipment.get("slot_drop_chances"))
            .and_then(Value::as_f64)
            .map(|chance| chance as f32),
        weight: i32::try_from(entry.get("weight").and_then(Value::as_i64).unwrap_or(1))
            .unwrap_or(1),
    })
}

pub fn parse(name: &str, text: &str) -> Result<Config, DataError> {
    let value: Value =
        serde_json::from_str(text).map_err(|error| DataError::Invalid(error.to_string()))?;
    let Some(map) = value.as_object() else {
        return invalid("a trial spawner config is not an object");
    };
    let int = |key: &str| {
        map.get(key)
            .and_then(Value::as_i64)
            .and_then(|number| i32::try_from(number).ok())
    };
    let float = |key: &str| {
        map.get(key)
            .and_then(Value::as_f64)
            .map(|number| number as f32)
    };
    let potentials = map
        .get("spawn_potentials")
        .and_then(Value::as_array)
        .map_or(Ok(Vec::new()), |entries| {
            entries.iter().map(spawn).collect::<Result<Vec<_>, _>>()
        })?;
    let ejects: Option<Vec<(&'static str, i32)>> = map
        .get("loot_tables_to_eject")
        .and_then(Value::as_array)
        .filter(|entries| !entries.is_empty())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    Some((
                        leak(&qualified(entry.get("data")?.as_str()?)),
                        i32::try_from(entry.get("weight")?.as_i64()?).ok()?,
                    ))
                })
                .collect()
        });
    Ok(Config {
        name: leak(name),
        spawn_range: int("spawn_range"),
        total_mobs: float("total_mobs"),
        simultaneous_mobs: float("simultaneous_mobs"),
        total_mobs_added_per_player: float("total_mobs_added_per_player"),
        simultaneous_mobs_added_per_player: float("simultaneous_mobs_added_per_player"),
        ticks_between_spawn: int("ticks_between_spawn"),
        spawn_potentials: Box::leak(potentials.into_boxed_slice()),
        loot_tables_to_eject: ejects.map(|tables| &*Box::leak(tables.into_boxed_slice())),
        items_to_drop_when_ominous: map
            .get("items_to_drop_when_ominous")
            .and_then(Value::as_str)
            .map(|table| leak(&qualified(table))),
    })
}

pub fn build(registry: &RegistryOverlay) -> ConfigOverlay {
    let mut overlay = ConfigOverlay::default();
    for entry in registry.entries(REGISTRY) {
        match parse(&entry.id, &entry.json) {
            Ok(config) => overlay.insert(config),
            Err(error) => tracing::warn!("Failed to load trial spawner {}: {error}", entry.id),
        }
    }
    overlay
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_config_keeps_its_overrides_entities_and_ejected_tables() {
        let config = parse(
            "t:z",
            r#"{"total_mobs":9.0,"ticks_between_spawn":5,"spawn_potentials":[{"data":{"entity":{"id":"minecraft:slime","Size":2},"equipment":{"loot_table":"x","slot_drop_chances":0.5}},"weight":4}],"loot_tables_to_eject":[{"data":"t:l","weight":2}]}"#,
        )
        .unwrap();
        assert_eq!(config.total_mobs, Some(9.0));
        assert_eq!(config.simultaneous_mobs, None);
        assert_eq!(config.ticks_between_spawn, Some(5));
        let first = &config.spawn_potentials[0];
        assert_eq!(first.entity, r#"{Size:2,id:"minecraft:slime"}"#);
        assert_eq!(first.equipment, Some("minecraft:x"));
        assert_eq!(first.weight, 4);
        assert_eq!(config.loot_tables_to_eject, Some(&[("t:l", 2)][..]));
        assert!(rspigot_nbt::parse_compound(first.entity).is_ok());
    }
}
