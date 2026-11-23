use rspigot_data::loot::{
    BonusFormula, Condition, CopyOperation, CustomDataSource, EntityTarget, Function, FunctionKind,
    IntProvider, ListMode, MergeStrategy, NameTarget,
};
use serde_json::{Map, Value};

use super::functions_extra::extra;
use super::json::{
    Parsed, conditions, float_provider, int_provider, invalid, item_id, kind_of, leak_slice,
    leak_text, name, object, optional_flag, required, unsupported, whole,
};
use super::predicates::target;

pub(super) fn snbt(value: &Value) -> Parsed<String> {
    match value {
        Value::Bool(true) => Ok("1b".to_owned()),
        Value::Bool(false) => Ok("0b".to_owned()),
        Value::Number(number) => match (number.as_i64(), number.as_f64()) {
            (Some(whole), _) if i32::try_from(whole).is_ok() => Ok(whole.to_string()),
            (Some(whole), _) => Ok(format!("{whole}l")),
            (None, Some(float)) if float.is_finite() => Ok(format!("{float:?}d")),
            _ => invalid(format!("cannot encode {number} as snbt")),
        },
        Value::String(text) => Ok(quote(text)),
        Value::Array(items) => {
            let parts: Parsed<Vec<String>> = items.iter().map(snbt).collect();
            Ok(format!("[{}]", parts?.join(",")))
        }
        Value::Object(map) => {
            let mut parts = Vec::with_capacity(map.len());
            for (key, item) in map {
                parts.push(format!("{}:{}", quote(key), snbt(item)?));
            }
            Ok(format!("{{{}}}", parts.join(",")))
        }
        Value::Null => invalid("cannot encode null as snbt"),
    }
}

fn quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

fn limit(value: &Value) -> Parsed<(Option<IntProvider>, Option<IntProvider>)> {
    match value {
        Value::Number(_) => {
            let exact = int_provider(value)?;
            Ok((Some(exact), Some(exact)))
        }
        Value::Object(map) => Ok((
            map.get("min").map(int_provider).transpose()?,
            map.get("max").map(int_provider).transpose()?,
        )),
        other => invalid(format!("bad limit {other}")),
    }
}

fn formula(map: &Map<String, Value>) -> Parsed<BonusFormula> {
    let parameters = map.get("parameters").and_then(Value::as_object);
    let parameter = |keys: &[&str]| {
        parameters.and_then(|parameters| keys.iter().find_map(|key| parameters.get(*key)))
    };
    match name(required(map, "formula")?)?.as_str() {
        "minecraft:ore_drops" => Ok(BonusFormula::OreDrops),
        "minecraft:uniform_bonus_count" => Ok(BonusFormula::UniformBonusCount {
            bonus_multiplier: whole(
                parameter(&["bonusMultiplier", "bonus_multiplier"]).unwrap_or(&Value::Null),
            )?,
        }),
        "minecraft:binomial_with_bonus_count" => Ok(BonusFormula::BinomialWithBonusCount {
            extra: whole(parameter(&["extra"]).unwrap_or(&Value::Null))?,
            probability: parameter(&["probability"])
                .and_then(Value::as_f64)
                .map_or_else(|| invalid("missing probability"), |p| Ok(p as f32))?,
        }),
        other => unsupported(format!("bonus formula {other}")),
    }
}

fn tag_name(value: &Value, what: &str) -> Parsed<&'static str> {
    match value.as_str().and_then(|text| text.strip_prefix('#')) {
        Some(tag) => Ok(leak_text(name(&Value::from(tag))?)),
        None => unsupported(format!("{what} that is not a tag")),
    }
}

fn enchant_randomly(map: &Map<String, Value>) -> Parsed<FunctionKind> {
    let compatible = optional_flag(map, "only_compatible")?.unwrap_or(true);
    match map.get("options") {
        Some(Value::Array(items)) => {
            let mut names = Vec::with_capacity(items.len());
            for item in items {
                names.push(leak_text(name(item)?));
            }
            Ok(FunctionKind::EnchantRandomlyAmong {
                options: leak_slice(names),
                only_compatible: compatible,
            })
        }
        Some(Value::String(text)) if text.starts_with('#') => {
            Ok(FunctionKind::EnchantRandomlyFrom {
                options: tag_name(&map["options"], "enchant_randomly options")?,
                only_compatible: compatible,
            })
        }
        Some(single @ Value::String(_)) => {
            let enchantment = name(single)?;
            let max_level = rspigot_data::enchantments::by_name(&enchantment)
                .map_or(1, |found| i32::from(found.max_level));
            Ok(FunctionKind::EnchantRandomly {
                enchantment: leak_text(enchantment),
                max_level,
            })
        }
        None => Ok(FunctionKind::EnchantRandomlyAmong {
            options: rspigot_data::registries::synced_entries("minecraft:enchantment")
                .unwrap_or(&[]),
            only_compatible: compatible,
        }),
        _ => unsupported("enchant_randomly options"),
    }
}

pub(super) fn list_mode(map: &Map<String, Value>) -> Parsed<ListMode> {
    let offset = map.get("offset").map_or(Ok(0), whole)?;
    match map
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("replace_all")
    {
        "replace_all" => Ok(ListMode::ReplaceAll),
        "append" => Ok(ListMode::Append),
        "insert" => Ok(ListMode::Insert { offset }),
        "replace_section" => Ok(ListMode::ReplaceSection {
            offset,
            size: map.get("size").map(whole).transpose()?,
        }),
        other => unsupported(format!("list mode {other}")),
    }
}

fn copy_custom_data(map: &Map<String, Value>) -> Parsed<FunctionKind> {
    let source = object(required(map, "source")?)?;
    let source = match source.get("type").and_then(Value::as_str) {
        Some("minecraft:context") => {
            let wanted = required(source, "target")?;
            match wanted.as_str() {
                Some("block_entity") => CustomDataSource::BlockEntity,
                _ => CustomDataSource::Entity(target(wanted)?),
            }
        }
        Some("minecraft:storage") => {
            CustomDataSource::Storage(leak_text(name(required(source, "source")?)?))
        }
        _ => return unsupported("copy_custom_data source"),
    };
    let Some(Value::Array(ops)) = map.get("ops") else {
        return invalid("missing \"ops\"");
    };
    let mut operations = Vec::with_capacity(ops.len());
    for op in ops {
        let op = object(op)?;
        let text = |key: &str| match required(op, key)?.as_str() {
            Some(found) => Ok(leak_text(found.to_owned())),
            None => invalid(format!("\"{key}\" must be a string")),
        };
        let strategy = match required(op, "op")?.as_str() {
            Some("replace") => MergeStrategy::Replace,
            Some("append") => MergeStrategy::Append,
            Some("merge") => MergeStrategy::Merge,
            other => return unsupported(format!("copy op {other:?}")),
        };
        operations.push(CopyOperation {
            source: text("source")?,
            target: text("target")?,
            strategy,
        });
    }
    Ok(FunctionKind::CopyCustomData {
        source,
        operations: leak_slice(operations),
    })
}

fn optional_target(map: &Map<String, Value>) -> Parsed<Option<EntityTarget>> {
    map.get("entity").map(target).transpose()
}

fn kind_and_rest(kind: &str, map: &Map<String, Value>) -> Parsed<FunctionKind> {
    Ok(match kind {
        "minecraft:set_count" => FunctionKind::SetCount {
            count: int_provider(required(map, "count")?)?,
            add: optional_flag(map, "add")?.unwrap_or(false),
        },
        "minecraft:apply_bonus" => FunctionKind::ApplyBonus {
            enchantment: leak_text(name(required(map, "enchantment")?)?),
            formula: formula(map)?,
        },
        "minecraft:limit_count" => {
            let (min, max) = limit(required(map, "limit")?)?;
            FunctionKind::LimitCount { min, max }
        }
        "minecraft:explosion_decay" => FunctionKind::ExplosionDecay,
        "minecraft:furnace_smelt" => FunctionKind::FurnaceSmelt {
            use_input_count: optional_flag(map, "use_input_count")?.unwrap_or(true),
        },
        "minecraft:enchanted_count_increase" => FunctionKind::EnchantedCountIncrease {
            enchantment: leak_text(name(required(map, "enchantment")?)?),
            count: float_provider(required(map, "count")?)?,
            limit: map.get("limit").map_or(Ok(0), whole)?,
        },
        "minecraft:set_damage" => {
            if optional_flag(map, "add")?.unwrap_or(false) {
                return unsupported("set_damage with add");
            }
            FunctionKind::SetDamage(float_provider(required(map, "damage")?)?)
        }
        "minecraft:set_potion" => FunctionKind::SetPotion(leak_text(name(required(map, "id")?)?)),
        "minecraft:enchant_randomly" => enchant_randomly(map)?,
        "minecraft:set_ominous_bottle_amplifier" => {
            FunctionKind::SetOminousBottleAmplifier(int_provider(required(map, "amplifier")?)?)
        }
        "minecraft:enchant_with_levels" => FunctionKind::EnchantWithLevels {
            options: tag_name(required(map, "options")?, "enchant_with_levels options")?,
            levels: int_provider(required(map, "levels")?)?,
        },
        "minecraft:set_stew_effect" => {
            let Some(Value::Array(effects)) = map.get("effects") else {
                return invalid("missing \"effects\"");
            };
            let mut entries = Vec::with_capacity(effects.len());
            for effect in effects {
                let effect = object(effect)?;
                entries.push((
                    leak_text(name(required(effect, "type")?)?),
                    int_provider(required(effect, "duration")?)?,
                ));
            }
            FunctionKind::SetStewEffect(leak_slice(entries))
        }
        "minecraft:set_instrument" => {
            FunctionKind::SetInstrument(tag_name(required(map, "options")?, "instrument options")?)
        }
        "minecraft:set_enchantments" => {
            let mut levels = Vec::new();
            for (enchantment, level) in object(required(map, "enchantments")?)? {
                levels.push((
                    leak_text(name(&Value::from(enchantment.as_str()))?),
                    int_provider(level)?,
                ));
            }
            FunctionKind::SetEnchantments {
                levels: leak_slice(levels),
                add: optional_flag(map, "add")?.unwrap_or(false),
            }
        }
        "minecraft:exploration_map" => FunctionKind::ExplorationMap {
            destination: tag_name(required(map, "destination")?, "map destination")?,
            decoration: leak_text(name(
                map.get("decoration")
                    .unwrap_or(&Value::from("minecraft:mansion")),
            )?),
            zoom: map.get("zoom").map_or(Ok(2), whole)?,
            search_radius: map.get("search_radius").map_or(Ok(50), whole)?,
            skip_existing_chunks: optional_flag(map, "skip_existing_chunks")?.unwrap_or(true),
        },
        "minecraft:filtered" => {
            let discard = map.get("on_fail").and_then(|fail| fail.get("type"))
                == Some(&Value::from("minecraft:discard"));
            let filter = serde_json::json!({ "predicates": { "minecraft:map_id": {} } });
            if discard && map.get("item_filter") == Some(&filter) {
                FunctionKind::RequireMapId
            } else {
                return extra(kind, map);
            }
        }
        "minecraft:copy_components" => {
            let from_block = map.get("source").and_then(Value::as_str) == Some("block_entity");
            if !from_block || map.contains_key("exclude") || !map.contains_key("include") {
                return extra(kind, map);
            }
            let mut include = Vec::new();
            if let Some(Value::Array(list)) = map.get("include") {
                for item in list {
                    include.push(leak_text(name(item)?));
                }
            }
            FunctionKind::CopyComponents {
                include: leak_slice(include),
            }
        }
        "minecraft:copy_name" => {
            if map.get("source").and_then(Value::as_str) != Some("block_entity") {
                return extra(kind, map);
            }
            FunctionKind::CopyComponents {
                include: &["minecraft:custom_name"],
            }
        }
        "minecraft:copy_state" => {
            let mut properties = Vec::new();
            if let Some(Value::Array(list)) = map.get("properties") {
                for item in list {
                    match item.as_str() {
                        Some(property) => properties.push(leak_text(property.to_owned())),
                        None => return invalid("property names are strings"),
                    }
                }
            }
            FunctionKind::CopyState {
                block: leak_text(name(required(map, "block")?)?),
                properties: leak_slice(properties),
            }
        }
        "minecraft:set_name" => FunctionKind::SetName {
            entity: optional_target(map)?,
            name: leak_text(snbt(required(map, "name")?)?),
            target: match map.get("target").and_then(Value::as_str) {
                None | Some("custom_name") => NameTarget::CustomName,
                Some("item_name") => NameTarget::ItemName,
                Some(other) => return unsupported(format!("set_name target {other}")),
            },
        },
        "minecraft:set_lore" => {
            let Some(Value::Array(lore)) = map.get("lore") else {
                return invalid("missing \"lore\"");
            };
            let mut lines = Vec::with_capacity(lore.len());
            for line in lore {
                lines.push(leak_text(snbt(line)?));
            }
            FunctionKind::SetLore {
                entity: optional_target(map)?,
                lines: leak_slice(lines),
                mode: list_mode(map)?,
            }
        }
        "minecraft:set_custom_data" => FunctionKind::SetCustomData {
            tag: match required(map, "tag")? {
                Value::String(text) => leak_text(text.clone()),
                other => leak_text(snbt(other)?),
            },
        },
        "minecraft:set_item" => FunctionKind::SetItem(item_id(required(map, "item")?)?),
        "minecraft:set_loot_table" => FunctionKind::SetLootTable {
            table: leak_text(name(required(map, "loot_table_id")?)?),
            seed: map.get("seed").and_then(Value::as_i64).unwrap_or(0),
        },
        "minecraft:copy_custom_data" => copy_custom_data(map)?,
        "minecraft:toggle_tooltips" => {
            let mut toggles = Vec::new();
            for (component, shown) in object(required(map, "toggles")?)? {
                let Some(shown) = shown.as_bool() else {
                    return invalid(format!("toggle {component} is not a boolean"));
                };
                toggles.push((leak_text(name(&Value::from(component.as_str()))?), shown));
            }
            FunctionKind::ToggleTooltips(leak_slice(toggles))
        }
        "minecraft:set_components" => {
            let mut values = Vec::new();
            for (component, value) in object(required(map, "components")?)? {
                values.push((
                    leak_text(name(&Value::from(component.as_str()))?),
                    leak_text(snbt(value)?),
                ));
            }
            FunctionKind::SetComponents(leak_slice(values))
        }
        other => return extra(other, map),
    })
}

fn both(outer: Option<Condition>, inner: Option<Condition>) -> Option<Condition> {
    match (outer, inner) {
        (Some(outer), Some(inner)) => Some(Condition::AllOf(leak_slice(vec![outer, inner]))),
        (outer, None) => outer,
        (None, inner) => inner,
    }
}

fn function_into(value: &Value, outer: Option<Condition>, out: &mut Vec<Function>) -> Parsed<()> {
    let map = object(value)?;
    let own = both(outer, conditions(map)?);
    let kind = kind_of(map, "function")?;
    if kind == "minecraft:sequence" {
        let Some(Value::Array(list)) = map.get("functions") else {
            return invalid("missing \"functions\"");
        };
        for item in list {
            function_into(item, own, out)?;
        }
        return Ok(());
    }
    out.push(Function {
        condition: own,
        kind: kind_and_rest(&kind, map)?,
    });
    Ok(())
}

pub(super) fn function_list(value: &Value) -> Parsed<&'static [Function]> {
    let mut parsed = Vec::new();
    match value {
        Value::Array(items) => {
            for item in items {
                function_into(item, None, &mut parsed)?;
            }
        }
        single => function_into(single, None, &mut parsed)?,
    }
    Ok(leak_slice(parsed))
}

pub(super) fn functions(map: &Map<String, Value>) -> Parsed<&'static [Function]> {
    match map.get("modifier").or_else(|| map.get("functions")) {
        Some(value) => function_list(value),
        None => Ok(&[]),
    }
}
