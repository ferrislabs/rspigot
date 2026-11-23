use std::num::NonZeroU8;

use rspigot_domain::item::ItemStack;
use rspigot_domain::plugin_recipes::{self, Body, Category, PluginRecipe};
use rspigot_items::{CookingKind, ItemId, ItemKind, ItemSet};
use serde_json::{Map, Value};

use super::DataError;
use crate::ports::PackFile;

pub const OWNER: &str = "datapack";

const GRID: usize = 3;
const ITEM_REGISTRY: &str = "minecraft:item";

fn invalid<T>(reason: impl Into<String>) -> Result<T, DataError> {
    Err(DataError::Invalid(reason.into()))
}

fn item_ids(entry: &str) -> Result<Vec<ItemId>, DataError> {
    if let Some(tag) = entry.strip_prefix('#') {
        let key = if tag.contains(':') {
            tag.to_owned()
        } else {
            format!("minecraft:{tag}")
        };
        let Some(ids) = rspigot_data::tags::tag(ITEM_REGISTRY, &key) else {
            return invalid(format!("unknown item tag {key}"));
        };
        return Ok(ids
            .iter()
            .filter_map(|id| u16::try_from(*id).ok().map(ItemId))
            .collect());
    }
    match ItemKind::from_name(entry) {
        Some(kind) if kind != ItemKind::Air => Ok(vec![kind.id()]),
        _ => invalid(format!("unknown item {entry}")),
    }
}

fn ingredient(value: &Value) -> Result<ItemSet, DataError> {
    let ids = match value {
        Value::String(entry) => item_ids(entry)?,
        Value::Array(entries) => {
            let mut ids = Vec::new();
            for entry in entries {
                match entry.as_str() {
                    Some(entry) => ids.extend(item_ids(entry)?),
                    None => return invalid("an ingredient entry is not a string"),
                }
            }
            ids
        }
        _ => return invalid("an ingredient is not a string or a list"),
    };
    if ids.is_empty() {
        return invalid("an ingredient matches no item");
    }
    Ok(plugin_recipes::intern_items(&ids))
}

fn field<'a>(map: &'a Map<String, Value>, key: &str) -> Result<&'a Value, DataError> {
    match map.get(key) {
        Some(value) => Ok(value),
        None => invalid(format!("missing \"{key}\"")),
    }
}

fn result_of(map: &Map<String, Value>) -> Result<ItemStack, DataError> {
    let result = field(map, "result")?;
    let (name, count) = match result {
        Value::String(name) => (name.as_str(), 1),
        Value::Object(stack) => (
            field(stack, "id")?.as_str().unwrap_or_default(),
            stack.get("count").and_then(Value::as_i64).unwrap_or(1),
        ),
        _ => return invalid("the result is not an item"),
    };
    let kind = match ItemKind::from_name(name) {
        Some(kind) if kind != ItemKind::Air => kind,
        _ => return invalid(format!("unknown result item {name}")),
    };
    let count = u8::try_from(count)
        .ok()
        .and_then(NonZeroU8::new)
        .ok_or_else(|| DataError::Invalid(format!("bad result count {count}")))?;
    Ok(ItemStack::new(kind.id(), count))
}

fn shaped(map: &Map<String, Value>) -> Result<Body, DataError> {
    let rows: Vec<Vec<char>> = field(map, "pattern")?
        .as_array()
        .ok_or_else(|| DataError::Invalid("the pattern is not a list".to_owned()))?
        .iter()
        .map(|row| row.as_str().unwrap_or_default().chars().collect())
        .collect();
    let width = rows.first().map_or(0, Vec::len);
    if rows.is_empty() || rows.len() > GRID || width == 0 || width > GRID {
        return invalid("the pattern is not 1 to 3 rows of 1 to 3 cells");
    }
    if rows.iter().any(|row| row.len() != width) {
        return invalid("the pattern rows differ in width");
    }
    let keys = field(map, "key")?
        .as_object()
        .ok_or_else(|| DataError::Invalid("the key is not an object".to_owned()))?;
    let mut cells: Vec<Vec<Option<ItemSet>>> = Vec::new();
    for row in &rows {
        let mut cell_row = Vec::new();
        for symbol in row {
            if *symbol == ' ' {
                cell_row.push(None);
                continue;
            }
            let Some(entry) = keys.get(&symbol.to_string()) else {
                return invalid(format!("the pattern uses '{symbol}' with no key"));
            };
            cell_row.push(Some(ingredient(entry)?));
        }
        cells.push(cell_row);
    }
    let filled = |row: &Vec<Option<ItemSet>>| row.iter().any(Option::is_some);
    let (Some(top), Some(bottom)) = (
        cells.iter().position(filled),
        cells.iter().rposition(filled),
    ) else {
        return invalid("the pattern has no ingredient");
    };
    let column = |x: usize| cells.iter().any(|row| row[x].is_some());
    let (Some(left), Some(right)) = (
        (0..width).find(|x| column(*x)),
        (0..width).rfind(|x| column(*x)),
    ) else {
        return invalid("the pattern has no ingredient");
    };
    let trimmed = cells[top..=bottom]
        .iter()
        .flat_map(|row| row[left..=right].iter().copied())
        .collect();
    Ok(Body::Shaped {
        width: right - left + 1,
        height: bottom - top + 1,
        mirror: true,
        cells: trimmed,
    })
}

fn cooking(map: &Map<String, Value>, kind: CookingKind) -> Result<Body, DataError> {
    let default_time = match kind {
        CookingKind::Smelting => 200,
        CookingKind::Blasting | CookingKind::Smoking => 100,
        CookingKind::Campfire => 600,
    };
    let time = map
        .get("cookingtime")
        .and_then(Value::as_i64)
        .unwrap_or(default_time);
    Ok(Body::Cooking {
        kind,
        ingredient: ingredient(field(map, "ingredient")?)?,
        experience: map.get("experience").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        time: u16::try_from(time)
            .ok()
            .filter(|time| *time > 0)
            .ok_or_else(|| DataError::Invalid("the cooking time is not 1 to 65535".to_owned()))?,
    })
}

fn category_of(map: &Map<String, Value>, body: &Body) -> Category {
    match map.get("category").and_then(Value::as_str) {
        Some("building") => Category::Building,
        Some("redstone") => Category::Redstone,
        Some("equipment") => Category::Equipment,
        Some("food") => Category::Food,
        Some(_) => Category::Misc,
        None if matches!(body, Body::Cooking { .. }) => Category::Food,
        None => Category::Misc,
    }
}

pub fn compile(file: &PackFile) -> Result<PluginRecipe, DataError> {
    let root: Value =
        serde_json::from_str(&file.text).map_err(|error| DataError::Invalid(error.to_string()))?;
    let Some(map) = root.as_object() else {
        return invalid("a recipe is not an object");
    };
    let kind = field(map, "type")?.as_str().unwrap_or_default();
    let kind = kind.strip_prefix("minecraft:").unwrap_or(kind);
    let body = match kind {
        "crafting_shaped" => shaped(map)?,
        "crafting_shapeless" => {
            let list = field(map, "ingredients")?
                .as_array()
                .ok_or_else(|| DataError::Invalid("ingredients is not a list".to_owned()))?;
            if list.is_empty() || list.len() > GRID * GRID {
                return invalid("a shapeless recipe takes 1 to 9 ingredients");
            }
            Body::Shapeless {
                ingredients: list.iter().map(ingredient).collect::<Result<_, _>>()?,
            }
        }
        "smelting" => cooking(map, CookingKind::Smelting)?,
        "blasting" => cooking(map, CookingKind::Blasting)?,
        "smoking" => cooking(map, CookingKind::Smoking)?,
        "campfire_cooking" => cooking(map, CookingKind::Campfire)?,
        "stonecutting" => Body::Stonecutting {
            ingredient: ingredient(field(map, "ingredient")?)?,
        },
        "smithing_transform" => Body::Smithing {
            template: ingredient(field(map, "template")?)?,
            base: ingredient(field(map, "base")?)?,
            addition: ingredient(field(map, "addition")?)?,
        },
        other => return Err(DataError::Unsupported(format!("recipe type {other}"))),
    };
    let (id, display_id) = plugin_recipes::intern_id(file.id.as_str());
    Ok(PluginRecipe {
        id,
        display_id,
        owner: OWNER.to_owned(),
        category: category_of(map, &body),
        group: map
            .get("group")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        result: result_of(map)?,
        body,
        override_vanilla: true,
        auto_discover: false,
        exact: Vec::new(),
    })
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
    fn a_shaped_recipe_is_trimmed_and_keeps_its_result() {
        let recipe = compile(&file(
            "t:r",
            r##"{"type":"minecraft:crafting_shaped","key":{"#":"minecraft:stick"},"pattern":["   "," # ","   "],"result":{"id":"minecraft:diamond","count":3}}"##,
        ))
        .unwrap();
        assert!(matches!(
            recipe.body,
            Body::Shaped {
                width: 1,
                height: 1,
                ..
            }
        ));
        assert_eq!(recipe.result.count.get(), 3);
        assert!(recipe.override_vanilla);
    }

    #[test]
    fn cooking_defaults_and_unsupported_types() {
        let recipe = compile(&file(
            "t:b",
            r#"{"type":"minecraft:blasting","ingredient":"minecraft:stick","result":{"id":"minecraft:diamond"}}"#,
        ))
        .unwrap();
        assert!(matches!(recipe.body, Body::Cooking { time: 100, .. }));
        assert!(matches!(
            compile(&file(
                "t:x",
                r#"{"type":"minecraft:crafting_special_firework_star"}"#
            )),
            Err(DataError::Unsupported(_))
        ));
        assert!(compile(&file("t:y", "{")).is_err());
    }
}
