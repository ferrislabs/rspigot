use rspigot_data::loot::{
    AttributeModifier, BannerLayer, CustomModelEdit, Explosion, ExtraFunction, FunctionKind,
    ListEdit, ListMode, PageKind,
};
use serde_json::{Map, Value};

use super::functions::{function_list, list_mode, snbt};
use super::json::{
    Parsed, float_provider, holder_set, int_provider, invalid, leak_slice, leak_text, name, object,
    registry_id, required, unsupported, whole,
};
use super::predicates::{item_predicate, target};
use super::slots::container_component;
use super::table::entry_list;

const OPERATIONS: [&str; 3] = ["add_value", "add_multiplied_base", "add_multiplied_total"];
const SLOT_GROUPS: [&str; 10] = [
    "any", "mainhand", "offhand", "hand", "feet", "legs", "chest", "head", "armor", "body",
];
const SHAPES: [&str; 5] = ["small_ball", "large_ball", "star", "creeper", "burst"];

fn text_of(value: &Value, what: &str) -> Parsed<&'static str> {
    match value.as_str() {
        Some(text) => Ok(leak_text(text.to_owned())),
        None => invalid(format!("{what} must be a string")),
    }
}

fn filterable(value: &Value, what: &str) -> Parsed<&'static str> {
    match value {
        Value::Object(map) => text_of(required(map, "raw")?, what),
        other => text_of(other, what),
    }
}

fn list_edit<T: Copy>(value: &Value, item: impl Fn(&Value) -> Parsed<T>) -> Parsed<ListEdit<T>> {
    let map = object(value)?;
    let Value::Array(list) = required(map, "values")? else {
        return invalid("\"values\" must be a list");
    };
    let values: Parsed<Vec<T>> = list.iter().map(item).collect();
    Ok(ListEdit {
        values: leak_slice(values?),
        mode: list_mode(map)?,
    })
}

fn color(value: &Value) -> Parsed<i32> {
    match value {
        Value::Array(parts) if parts.len() == 3 => {
            let mut packed = 0;
            for part in parts {
                let channel = part
                    .as_f64()
                    .map_or_else(|| invalid("color channel must be a number"), Ok)?;
                packed = (packed << 8) | (channel.clamp(0.0, 1.0) * 255.0) as i32;
            }
            Ok(packed)
        }
        other => whole(other),
    }
}

fn colors(value: &Value) -> Parsed<&'static [i32]> {
    let Value::Array(list) = value else {
        return invalid("colors must be a list");
    };
    let parsed: Parsed<Vec<i32>> = list.iter().map(color).collect();
    Ok(leak_slice(parsed?))
}

fn optional<T>(
    map: &Map<String, Value>,
    key: &str,
    parse: impl Fn(&Value) -> Parsed<T>,
) -> Parsed<Option<T>> {
    map.get(key).map(parse).transpose()
}

fn explosion(value: &Value) -> Parsed<Explosion> {
    let map = object(value)?;
    let flag = |key: &str| match map.get(key) {
        None => Ok(None),
        Some(found) => found
            .as_bool()
            .map(Some)
            .map_or_else(|| invalid(format!("\"{key}\" must be a boolean")), Ok),
    };
    let shape = match map.get("shape") {
        None => None,
        Some(found) => {
            let text = text_of(found, "shape")?;
            if !SHAPES.contains(&text) {
                return invalid(format!("unknown firework shape {text}"));
            }
            Some(text)
        }
    };
    Ok(Explosion {
        shape,
        colors: optional(map, "colors", colors)?,
        fade_colors: optional(map, "fade_colors", colors)?,
        trail: flag("has_trail")?.or(flag("trail")?),
        twinkle: flag("has_twinkle")?.or(flag("twinkle")?),
    })
}

fn modifier(value: &Value) -> Parsed<AttributeModifier> {
    let map = object(value)?;
    let operation = text_of(required(map, "operation")?, "operation")?;
    if !OPERATIONS.contains(&operation) {
        return invalid(format!("unknown attribute operation {operation}"));
    }
    let slot_names = match required(map, "slot")? {
        Value::Array(list) => list.iter().map(|slot| text_of(slot, "slot")).collect(),
        single => text_of(single, "slot").map(|slot| vec![slot]),
    }?;
    if slot_names.is_empty() || slot_names.iter().any(|slot| !SLOT_GROUPS.contains(slot)) {
        return invalid("unknown attribute slot");
    }
    let attribute = name(required(map, "attribute")?)?;
    registry_id("minecraft:attribute", &attribute)?;
    Ok(AttributeModifier {
        attribute: leak_text(attribute),
        id: leak_text(name(required(map, "id")?)?),
        operation,
        amount: float_provider(required(map, "amount")?)?,
        slots: leak_slice(slot_names),
    })
}

fn page_lines(map: &Map<String, Value>, kind: PageKind) -> Parsed<&'static [&'static str]> {
    let Value::Array(list) = required(map, "pages")? else {
        return invalid("\"pages\" must be a list");
    };
    let mut pages = Vec::with_capacity(list.len());
    for page in list {
        pages.push(match kind {
            PageKind::Writable => filterable(page, "page")?,
            PageKind::Written => match page {
                Value::Object(wrapped) if wrapped.contains_key("raw") => {
                    leak_text(snbt(&wrapped["raw"])?)
                }
                other => leak_text(snbt(other)?),
            },
        });
    }
    Ok(leak_slice(pages))
}

pub(super) fn extra(kind: &str, map: &Map<String, Value>) -> Parsed<FunctionKind> {
    let function = match kind {
        "minecraft:discard" => ExtraFunction::Discard,
        "minecraft:copy_name" => ExtraFunction::CopyName(target(required(map, "source")?)?),
        "minecraft:copy_components" => {
            let names = |key: &str| -> Parsed<Option<&'static [&'static str]>> {
                match map.get(key) {
                    None => Ok(None),
                    Some(Value::Array(list)) => {
                        let parsed: Parsed<Vec<&'static str>> =
                            list.iter().map(|item| name(item).map(leak_text)).collect();
                        Ok(Some(leak_slice(parsed?)))
                    }
                    Some(_) => invalid(format!("\"{key}\" must be a list")),
                }
            };
            let from = match required(map, "source")?.as_str() {
                Some("block_entity") => None,
                _ => Some(target(required(map, "source")?)?),
            };
            ExtraFunction::CopyComponents {
                from,
                include: names("include")?,
                exclude: names("exclude")?.unwrap_or(&[]),
            }
        }
        "minecraft:fill_player_head" => {
            ExtraFunction::FillPlayerHead(target(required(map, "entity")?)?)
        }
        "minecraft:modify_contents" => ExtraFunction::ModifyContents {
            component: container_component(required(map, "component")?)?,
            modifier: function_list(required(map, "modifier")?)?,
        },
        "minecraft:set_contents" => {
            let Value::Array(entries) = required(map, "entries")? else {
                return invalid("\"entries\" must be a list");
            };
            ExtraFunction::SetContents {
                component: container_component(required(map, "component")?)?,
                entries: entry_list(entries, "minecraft:set_contents")?,
            }
        }
        "minecraft:set_attributes" => {
            let Value::Array(list) = required(map, "modifiers")? else {
                return invalid("\"modifiers\" must be a list");
            };
            let modifiers: Parsed<Vec<AttributeModifier>> = list.iter().map(modifier).collect();
            ExtraFunction::SetAttributes {
                modifiers: leak_slice(modifiers?),
                replace: map.get("replace").and_then(Value::as_bool).unwrap_or(true),
            }
        }
        "minecraft:set_banner_pattern" => {
            let Value::Array(list) = required(map, "patterns")? else {
                return invalid("\"patterns\" must be a list");
            };
            let mut patterns = Vec::with_capacity(list.len());
            for layer in list {
                let layer = object(layer)?;
                patterns.push(BannerLayer {
                    pattern: leak_text(name(required(layer, "pattern")?)?),
                    color: text_of(required(layer, "color")?, "color")?,
                });
            }
            ExtraFunction::SetBannerPattern {
                patterns: leak_slice(patterns),
                append: required(map, "append")?.as_bool().unwrap_or(false),
            }
        }
        "minecraft:set_book_cover" => {
            let generation = optional(map, "generation", whole)?;
            if generation.is_some_and(|generation| !(0..=3).contains(&generation)) {
                return invalid("generation must be between 0 and 3");
            }
            ExtraFunction::SetBookCover {
                title: optional(map, "title", |title| filterable(title, "title"))?,
                author: optional(map, "author", |author| text_of(author, "author"))?,
                generation,
            }
        }
        "minecraft:set_custom_model_data" => ExtraFunction::SetCustomModelData(CustomModelEdit {
            floats: optional(map, "floats", |floats| {
                list_edit(floats, |item| match item.as_f64() {
                    Some(number) => Ok(number as f32),
                    None => invalid("floats must be numbers"),
                })
            })?,
            flags: optional(map, "flags", |flags| {
                list_edit(flags, |item| match item.as_bool() {
                    Some(flag) => Ok(flag),
                    None => invalid("flags must be booleans"),
                })
            })?,
            strings: optional(map, "strings", |strings| {
                list_edit(strings, |item| text_of(item, "string"))
            })?,
            colors: optional(map, "colors", |list| list_edit(list, color))?,
        }),
        "minecraft:set_firework_explosion" => {
            ExtraFunction::SetFireworkExplosion(explosion(&Value::Object(map.clone()))?)
        }
        "minecraft:set_fireworks" => {
            let flight_duration = optional(map, "flight_duration", whole)?;
            if flight_duration.is_some_and(|duration| !(0..=255).contains(&duration)) {
                return invalid("flight_duration must be between 0 and 255");
            }
            ExtraFunction::SetFireworks {
                explosions: optional(map, "explosions", |list| list_edit(list, explosion))?,
                flight_duration,
            }
        }
        "minecraft:set_writable_book_pages" | "minecraft:set_written_book_pages" => {
            let page_kind = if kind == "minecraft:set_writable_book_pages" {
                PageKind::Writable
            } else {
                PageKind::Written
            };
            ExtraFunction::SetPages {
                kind: page_kind,
                pages: page_lines(map, page_kind)?,
                mode: list_mode(map).unwrap_or(ListMode::ReplaceAll),
            }
        }
        "minecraft:set_random_dyes" => {
            ExtraFunction::SetRandomDyes(int_provider(required(map, "number_of_dyes")?)?)
        }
        "minecraft:set_random_potion" => {
            ExtraFunction::SetRandomPotion(optional(map, "options", |options| {
                holder_set("minecraft:potion", options)
            })?)
        }
        "minecraft:filtered" => ExtraFunction::Filtered {
            filter: item_predicate(required(map, "item_filter")?)?,
            on_pass: match map.get("on_pass") {
                Some(pass) => function_list(pass)?,
                None => &[],
            },
            on_fail: match map.get("on_fail") {
                Some(fail) => function_list(fail)?,
                None => &[],
            },
        },
        other => return unsupported(format!("function {other}")),
    };
    Ok(FunctionKind::Extra(function))
}
