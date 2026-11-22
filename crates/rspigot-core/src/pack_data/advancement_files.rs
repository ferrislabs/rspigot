use std::collections::BTreeMap;

use rspigot_data::advancements::{
    Advancement, Criterion, Display, ItemMatch, Rewards, Trigger, built_in,
};
use rspigot_items::ItemKind;
use serde_json::{Map, Value};

use super::DataError;
use crate::ports::PackFile;

fn leak_str(text: &str) -> &'static str {
    Box::leak(text.to_owned().into_boxed_str())
}

fn leak_slice<T>(items: Vec<T>) -> &'static [T] {
    Box::leak(items.into_boxed_slice())
}

fn invalid<T>(reason: impl Into<String>) -> Result<T, DataError> {
    Err(DataError::Invalid(reason.into()))
}

fn item_id(name: &str) -> Result<u16, DataError> {
    match ItemKind::from_name(name) {
        Some(kind) => Ok(kind.id().0),
        None => invalid(format!("unknown item {name}")),
    }
}

fn item_match(name: &str) -> Result<ItemMatch, DataError> {
    match name.strip_prefix('#') {
        Some(tag) => Ok(ItemMatch::Tag(leak_str(tag))),
        None => item_id(name).map(ItemMatch::Item),
    }
}

fn text_of(value: &Value) -> &str {
    match value {
        Value::String(text) => text,
        Value::Object(map) => map
            .get("translate")
            .or_else(|| map.get("text"))
            .and_then(Value::as_str)
            .unwrap_or_default(),
        _ => "",
    }
}

fn single_item(entry: &Value) -> Option<&str> {
    let map = entry.as_object()?;
    if map.len() != 1 {
        return None;
    }
    map.get("items")?.as_str()
}

fn trigger_of(body: &Map<String, Value>) -> Result<Trigger, DataError> {
    let Some(name) = body.get("trigger").and_then(Value::as_str) else {
        return invalid("a criterion has no trigger");
    };
    let conditions = body.get("conditions").and_then(Value::as_object);
    match (name, conditions) {
        ("minecraft:impossible", _) => return Ok(Trigger::Impossible),
        ("minecraft:tick", _) => return Ok(Trigger::Tick),
        ("minecraft:slept_in_bed", None) => return Ok(Trigger::SleptInBed),
        ("minecraft:entity_killed_player", None) => return Ok(Trigger::EntityKilledPlayer),
        ("minecraft:recipe_unlocked", Some(conditions)) => {
            if let Some(recipe) = conditions.get("recipes").and_then(Value::as_str) {
                return Ok(Trigger::RecipeUnlocked(leak_str(recipe)));
            }
        }
        ("minecraft:inventory_changed", conditions) => {
            let empty = Map::new();
            let conditions = conditions.unwrap_or(&empty);
            let slots = conditions.get("slots").and_then(Value::as_object);
            let occupied = slots
                .and_then(|slots| slots.get("occupied"))
                .and_then(|occupied| occupied.get("min"))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let listed: &[Value] = conditions
                .get("items")
                .and_then(Value::as_array)
                .map_or(&[], Vec::as_slice);
            let items: Vec<ItemMatch> = listed
                .iter()
                .filter_map(single_item)
                .map(item_match)
                .collect::<Result<_, _>>()?;
            let simple = conditions
                .keys()
                .all(|key| key == "items" || key == "slots");
            if simple && items.len() == listed.len() {
                return Ok(Trigger::InventoryChanged {
                    items: leak_slice(items),
                    occupied: i32::try_from(occupied).unwrap_or(0),
                });
            }
        }
        ("minecraft:consume_item", None) => return Ok(Trigger::ConsumeItem(None)),
        ("minecraft:consume_item", Some(conditions)) if conditions.len() == 1 => {
            if let Some(item) = conditions.get("item").and_then(single_item) {
                return Ok(Trigger::ConsumeItem(Some(item_match(item)?)));
            }
        }
        _ => {}
    }
    let conditions = conditions.map(|map| {
        let sorted: BTreeMap<&String, &Value> = map.iter().collect();
        leak_str(&serde_json::to_string(&sorted).unwrap_or_default())
    });
    Ok(Trigger::Event {
        name: leak_str(name),
        conditions,
    })
}

fn display_of(value: &Value, depth: f32, row: f32) -> Result<Display, DataError> {
    let Some(map) = value.as_object() else {
        return invalid("display is not an object");
    };
    let icon = match map.get("icon") {
        Some(Value::String(name)) => item_id(name)?,
        Some(Value::Object(stack)) => match stack.get("id").and_then(Value::as_str) {
            Some(name) => item_id(name)?,
            None => return invalid("display icon has no id"),
        },
        _ => return invalid("display has no icon"),
    };
    let flag = |key: &str, default: bool| map.get(key).and_then(Value::as_bool).unwrap_or(default);
    let frame = match map.get("frame").and_then(Value::as_str) {
        Some("challenge") => 1,
        Some("goal") => 2,
        _ => 0,
    };
    Ok(Display {
        icon,
        title: leak_str(text_of(map.get("title").unwrap_or(&Value::Null))),
        description: leak_str(text_of(map.get("description").unwrap_or(&Value::Null))),
        frame,
        background: map.get("background").and_then(Value::as_str).map(leak_str),
        show_toast: flag("show_toast", true),
        announce_to_chat: flag("announce_to_chat", true),
        hidden: flag("hidden", false),
        x: depth,
        y: row,
    })
}

pub fn parse(file: &PackFile, depth: f32, row: f32) -> Result<Advancement, DataError> {
    let root: Value =
        serde_json::from_str(&file.text).map_err(|error| DataError::Invalid(error.to_string()))?;
    let Some(root) = root.as_object() else {
        return invalid("an advancement is not an object");
    };
    let Some(criteria) = root.get("criteria").and_then(Value::as_object) else {
        return invalid("an advancement has no criteria");
    };
    let mut parsed = Vec::new();
    for (name, body) in criteria {
        let Some(body) = body.as_object() else {
            return invalid(format!("criterion {name} is not an object"));
        };
        parsed.push(Criterion {
            name: leak_str(name),
            trigger: trigger_of(body)?,
        });
    }
    let requirements: Vec<&'static [&'static str]> =
        match root.get("requirements").and_then(Value::as_array) {
            Some(groups) => groups
                .iter()
                .map(|group| {
                    let names: Vec<&'static str> = group
                        .as_array()
                        .map(|names| {
                            names
                                .iter()
                                .filter_map(Value::as_str)
                                .map(leak_str)
                                .collect()
                        })
                        .unwrap_or_default();
                    leak_slice(names)
                })
                .collect(),
            None => parsed
                .iter()
                .map(|criterion| leak_slice(vec![criterion.name]))
                .collect(),
        };
    let rewards = root.get("rewards").and_then(Value::as_object);
    let recipes: Vec<&'static str> = rewards
        .and_then(|rewards| rewards.get("recipes"))
        .and_then(Value::as_array)
        .map(|recipes| {
            recipes
                .iter()
                .filter_map(Value::as_str)
                .map(leak_str)
                .collect()
        })
        .unwrap_or_default();
    let experience = rewards
        .and_then(|rewards| rewards.get("experience"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    Ok(Advancement {
        id: leak_str(file.id.as_str()),
        parent: root.get("parent").and_then(Value::as_str).map(leak_str),
        display: root
            .get("display")
            .map(|display| display_of(display, depth, row))
            .transpose()?,
        criteria: leak_slice(parsed),
        requirements: leak_slice(requirements),
        rewards: Rewards {
            recipes: leak_slice(recipes),
            experience: i32::try_from(experience).unwrap_or(0),
        },
        telemetry: false,
    })
}

pub fn merge(files: &[&PackFile]) -> Option<&'static [Advancement]> {
    let mut by_id: BTreeMap<&str, Advancement> = BTreeMap::new();
    let mut rows: BTreeMap<Option<String>, f32> = BTreeMap::new();
    let mut loaded = Vec::new();
    for file in files {
        let parent = serde_json::from_str::<Value>(&file.text)
            .ok()
            .and_then(|value| {
                value
                    .get("parent")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            });
        let row = rows.entry(parent.clone()).or_insert(0.0);
        let depth = if parent.is_some() { 1.0 } else { 0.0 };
        match parse(file, depth, *row) {
            Ok(advancement) => {
                if advancement.display.is_some() {
                    *row += 1.0;
                }
                loaded.push(advancement);
            }
            Err(error) => tracing::warn!("Failed to load advancement {}: {error}", file.id),
        }
    }
    if loaded.is_empty() {
        return None;
    }
    for advancement in built_in() {
        by_id.insert(advancement.id, *advancement);
    }
    for advancement in loaded {
        by_id.insert(advancement.id, advancement);
    }
    Some(leak_slice(by_id.into_values().collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(id: &str, text: &str) -> PackFile {
        PackFile {
            id: rspigot_domain::Identifier::parse(id).unwrap(),
            text: text.to_owned(),
        }
    }

    #[test]
    fn a_datapack_advancement_keeps_its_triggers_rewards_and_requirements() {
        let advancement = parse(
            &file(
                "t:root",
                r#"{"display":{"icon":{"id":"minecraft:stone"},"title":{"text":"Hi"},"description":"d","frame":"goal","background":"b"},"criteria":{"a":{"trigger":"minecraft:tick"},"b":{"trigger":"minecraft:consume_item"},"c":{"trigger":"minecraft:location","conditions":{"x":1}}},"requirements":[["a"],["b","c"]],"rewards":{"recipes":["t:r"],"experience":7}}"#,
            ),
            0.0,
            0.0,
        )
        .unwrap();
        assert_eq!(advancement.id, "t:root");
        let display = advancement.display.unwrap();
        assert_eq!((display.title, display.frame), ("Hi", 2));
        assert_eq!(advancement.criteria[0].trigger, Trigger::Tick);
        assert_eq!(advancement.criteria[1].trigger, Trigger::ConsumeItem(None));
        assert!(matches!(
            advancement.criteria[2].trigger,
            Trigger::Event {
                name: "minecraft:location",
                conditions: Some("{\"x\":1}")
            }
        ));
        assert_eq!(advancement.requirements.len(), 2);
        assert_eq!(advancement.rewards.experience, 7);
        assert_eq!(advancement.rewards.recipes, ["t:r"]);
    }

    #[test]
    fn merging_overrides_a_built_in_advancement_and_keeps_the_order() {
        let merged = merge(&[
            &file(
                "minecraft:story/root",
                r#"{"criteria":{"z":{"trigger":"minecraft:impossible"}}}"#,
            ),
            &file(
                "t:new",
                r#"{"criteria":{"z":{"trigger":"minecraft:impossible"}}}"#,
            ),
        ])
        .unwrap();
        assert!(merged.windows(2).all(|pair| pair[0].id < pair[1].id));
        let root = merged
            .iter()
            .find(|a| a.id == "minecraft:story/root")
            .unwrap();
        assert_eq!(root.criteria[0].name, "z");
        assert!(merged.iter().any(|a| a.id == "t:new"));
        assert_eq!(merged.len(), built_in().len() + 1);
    }
}
