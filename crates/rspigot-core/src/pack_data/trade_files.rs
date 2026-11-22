use rspigot_data::trading::{self, Modifier, Require, Trade, TradeCost, TradeOverlay, TradeSet};
use rspigot_items::ItemKind;
use serde_json::{Map, Value};

use super::DataError;
use super::registry_overlay::RegistryOverlay;

const TRADE: &str = "minecraft:villager_trade";
const SET: &str = "minecraft:trade_set";
const TAGS: &str = "minecraft:tags/villager_trade";
const MAX_TAG_DEPTH: usize = 8;

fn invalid<T>(reason: impl Into<String>) -> Result<T, DataError> {
    Err(DataError::Invalid(reason.into()))
}

fn leak(text: &str) -> &'static str {
    Box::leak(text.to_owned().into_boxed_str())
}

fn item_name(value: &Value) -> Result<&'static str, DataError> {
    match value.as_str().and_then(ItemKind::from_name) {
        Some(kind) => Ok(kind.name()),
        None => invalid(format!("unknown item {value}")),
    }
}

fn cost(value: &Value, default_count: i64) -> Result<TradeCost, DataError> {
    let Some(map) = value.as_object() else {
        return invalid("a trade cost is not an object");
    };
    let potion = map
        .get("components")
        .and_then(|components| components.get("minecraft:potion_contents"))
        .and_then(|contents| contents.get("potion"))
        .and_then(Value::as_str)
        .map(|potion| leak(&qualified(potion)));
    Ok(TradeCost {
        item: item_name(map.get("id").unwrap_or(&Value::Null))?,
        count: i32::try_from(
            map.get("count")
                .and_then(Value::as_i64)
                .unwrap_or(default_count),
        )
        .unwrap_or(1),
        potion,
    })
}

fn qualified(name: &str) -> String {
    if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    }
}

fn variants(map: &Map<String, Value>) -> Vec<&'static str> {
    let listed = map
        .get("merchant_predicate")
        .and_then(|predicate| predicate.get("predicate"))
        .and_then(|predicate| predicate.get("minecraft:predicates"))
        .and_then(|predicates| predicates.get("minecraft:villager/variant"));
    match listed {
        Some(Value::String(one)) => vec![leak(&qualified(one))],
        Some(Value::Array(many)) => many
            .iter()
            .filter_map(Value::as_str)
            .map(|one| leak(&qualified(one)))
            .collect(),
        _ => Vec::new(),
    }
}

pub fn parse_trade(name: &str, text: &str) -> Result<Trade, DataError> {
    let value: Value =
        serde_json::from_str(text).map_err(|error| DataError::Invalid(error.to_string()))?;
    let Some(map) = value.as_object() else {
        return invalid("a trade is not an object");
    };
    let modifiers = match map.get("given_item_modifier") {
        Some(Value::Array(items)) => items
            .iter()
            .map(parse_modifier)
            .collect::<Result<Vec<_>, _>>()?,
        Some(Value::Null) | None => Vec::new(),
        Some(single) => vec![parse_modifier(single)?],
    };
    let gives = map.get("gives").and_then(Value::as_object);
    let Some(gives) = gives else {
        return invalid("a trade has no gives");
    };
    let number = |key: &str, default: i64| {
        i32::try_from(map.get(key).and_then(Value::as_i64).unwrap_or(default)).unwrap_or(1)
    };
    Ok(Trade {
        name: leak(name),
        wants: cost(
            map.get("wants")
                .ok_or_else(|| DataError::Invalid("a trade has no wants".to_owned()))?,
            1,
        )?,
        additional_wants: map
            .get("additional_wants")
            .map(|extra| cost(extra, 1))
            .transpose()?,
        gives: item_name(gives.get("id").unwrap_or(&Value::Null))?,
        gives_count: i32::try_from(gives.get("count").and_then(Value::as_i64).unwrap_or(1))
            .unwrap_or(1),
        max_uses: number("max_uses", 4),
        xp: number("xp", 1),
        reputation_discount: map
            .get("reputation_discount")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        variants: Box::leak(variants(map).into_boxed_slice()),
        modifiers: Box::leak(modifiers.into_boxed_slice()),
        double_price: map
            .get("double_trade_price_enchantments")
            .and_then(Value::as_str)
            .map(|tag| leak(&qualified(tag.trim_start_matches('#')))),
    })
}

fn tag_name(value: Option<&Value>) -> Result<&'static str, DataError> {
    match value
        .and_then(Value::as_str)
        .and_then(|tag| tag.strip_prefix('#'))
    {
        Some(tag) => Ok(leak(&qualified(tag))),
        None => invalid("an options field is not a tag"),
    }
}

fn int(value: Option<&Value>, what: &str) -> Result<i32, DataError> {
    value
        .and_then(Value::as_i64)
        .and_then(|number| i32::try_from(number).ok())
        .ok_or_else(|| DataError::Invalid(format!("{what} is not an integer")))
}

pub fn parse_modifier(value: &Value) -> Result<Modifier, DataError> {
    let kind = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let extra = value
        .get("include_additional_cost_component")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok(match kind {
        "minecraft:enchant_randomly" => Modifier::EnchantRandomly {
            options: tag_name(value.get("options"))?,
            only_compatible: value
                .get("only_compatible")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            additional_cost: extra,
        },
        "minecraft:enchant_with_levels" => {
            let levels = value.get("levels");
            Modifier::EnchantWithLevels {
                options: tag_name(value.get("options"))?,
                min: int(levels.and_then(|levels| levels.get("min")), "levels.min")?,
                max: int(levels.and_then(|levels| levels.get("max")), "levels.max")?,
                additional_cost: extra,
            }
        }
        "minecraft:set_stew_effect" => {
            let mut effects = Vec::new();
            for effect in value
                .get("effects")
                .and_then(Value::as_array)
                .map_or(&[][..], Vec::as_slice)
            {
                let name = effect
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                effects.push((
                    leak(&qualified(name)),
                    int(effect.get("duration"), "duration")?,
                ));
            }
            Modifier::SetStewEffects(Box::leak(effects.into_boxed_slice()))
        }
        "minecraft:set_random_dyes" => {
            let inputs = value
                .get("number_of_dyes")
                .and_then(|number| number.get("inputs"))
                .and_then(Value::as_array);
            let (Some(base), Some(binomial)) = (
                inputs.and_then(|i| i.first()),
                inputs.and_then(|i| i.get(1)),
            ) else {
                return invalid("set_random_dyes needs a base and a binomial");
            };
            Modifier::SetRandomDyes {
                base: int(Some(base), "base")?,
                trials: int(binomial.get("n"), "n")?,
                chance: binomial.get("p").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            }
        }
        "minecraft:exploration_map" => Modifier::ExplorationMap {
            decoration: leak(&qualified(
                value
                    .get("decoration")
                    .and_then(Value::as_str)
                    .unwrap_or("minecraft:mansion"),
            )),
            destination: tag_name(value.get("destination"))?,
            search_radius: int(value.get("search_radius"), "search_radius")?,
        },
        "minecraft:set_random_potion" => Modifier::SetRandomPotion,
        "minecraft:set_potion" => Modifier::SetPotion(leak(&qualified(
            value.get("id").and_then(Value::as_str).unwrap_or_default(),
        ))),
        "minecraft:filtered" => {
            let predicates = value
                .get("item_filter")
                .and_then(|filter| filter.get("predicates"))
                .and_then(Value::as_object);
            let key = predicates
                .and_then(|map| map.keys().next())
                .map(String::as_str);
            Modifier::Require(match key {
                Some("minecraft:enchantments") => Require::Enchantments,
                Some("minecraft:stored_enchantments") => Require::StoredEnchantments,
                Some("minecraft:dyed_color") => Require::DyedColor,
                Some("minecraft:map_id") => Require::MapId,
                other => {
                    return Err(DataError::Unsupported(format!("filter {other:?}")));
                }
            })
        }
        other => return Err(DataError::Unsupported(format!("trade modifier {other}"))),
    })
}

fn tag_members(
    overlay: &TradeOverlay,
    registry: &RegistryOverlay,
    tag: &str,
    depth: usize,
    out: &mut Vec<u16>,
) {
    if depth > MAX_TAG_DEPTH {
        tracing::warn!("Villager trade tag {tag} nests too deeply");
        return;
    }
    let file = registry
        .get(TAGS, tag)
        .and_then(|entry| serde_json::from_str::<Value>(&entry.json).ok());
    let replaced = file
        .as_ref()
        .and_then(|file| file.get("replace"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !replaced && let Some(base) = trading::built_in_trade_set(tag) {
        for index in base.trades {
            if !out.contains(index) {
                out.push(*index);
            }
        }
    }
    let values = file
        .as_ref()
        .and_then(|file| file.get("values"))
        .and_then(Value::as_array);
    for entry in values.map_or(&[][..], Vec::as_slice) {
        let Some(entry) = entry.as_str() else {
            continue;
        };
        match entry.strip_prefix('#') {
            Some(inner) => tag_members(overlay, registry, &qualified(inner), depth + 1, out),
            None => match overlay.index_of(&qualified(entry)) {
                Some(index) if !out.contains(&index) => out.push(index),
                Some(_) => {}
                None => tracing::warn!("Villager trade tag {tag} names an unknown trade {entry}"),
            },
        }
    }
}

fn parse_set(
    name: &str,
    text: &str,
    overlay: &TradeOverlay,
    registry: &RegistryOverlay,
) -> Result<TradeSet, DataError> {
    let value: Value =
        serde_json::from_str(text).map_err(|error| DataError::Invalid(error.to_string()))?;
    let mut members = Vec::new();
    match value.get("trades") {
        Some(Value::String(entry)) => match entry.strip_prefix('#') {
            Some(tag) => tag_members(overlay, registry, &qualified(tag), 0, &mut members),
            None => members.extend(overlay.index_of(&qualified(entry))),
        },
        Some(Value::Array(entries)) => {
            for entry in entries.iter().filter_map(Value::as_str) {
                match overlay.index_of(&qualified(entry)) {
                    Some(index) => members.push(index),
                    None => return invalid(format!("unknown trade {entry}")),
                }
            }
        }
        _ => return invalid("a trade set has no trades"),
    }
    Ok(TradeSet {
        name: leak(name),
        trades: Box::leak(members.into_boxed_slice()),
        amount: i32::try_from(value.get("amount").and_then(Value::as_i64).unwrap_or(1))
            .unwrap_or(1),
        allow_duplicates: value
            .get("allow_duplicates")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

pub fn build(registry: &RegistryOverlay) -> TradeOverlay {
    let mut overlay = TradeOverlay::default();
    for entry in registry.entries(TRADE) {
        match parse_trade(&entry.id, &entry.json) {
            Ok(trade) => {
                overlay.add_trade(trade);
            }
            Err(error) => tracing::warn!("Failed to load villager trade {}: {error}", entry.id),
        }
    }
    for entry in registry.entries(SET) {
        match parse_set(&entry.id, &entry.json, &overlay, registry) {
            Ok(set) => overlay.add_set(set),
            Err(error) => tracing::warn!("Failed to load trade set {}: {error}", entry.id),
        }
    }
    overlay
}

#[cfg(test)]
mod tests {
    use super::super::registry_overlay::OverlayEntry;
    use super::*;

    fn entry(id: &str, json: &str) -> OverlayEntry {
        OverlayEntry {
            id: id.to_owned(),
            json: json.to_owned(),
            data: None,
        }
    }

    const TRADE_JSON: &str = r#"{"wants":{"id":"minecraft:emerald","count":3},"gives":{"id":"minecraft:diamond","count":2},"max_uses":7}"#;

    #[test]
    fn a_trade_and_a_set_over_a_tag_load_with_the_built_in_members() {
        let mut registry = RegistryOverlay::default();
        registry.insert(TRADE, entry("t:gem", TRADE_JSON));
        registry.insert(
            TAGS,
            entry("minecraft:armorer/level_1", r#"{"values":["t:gem"]}"#),
        );
        registry.insert(
            SET,
            entry(
                "minecraft:armorer/level_1",
                r##"{"amount":6,"trades":"#minecraft:armorer/level_1"}"##,
            ),
        );
        let overlay = build(&registry);
        let set = overlay.set("minecraft:armorer/level_1").unwrap();
        assert_eq!(set.amount, 6);
        let base = trading::built_in_trade_set("minecraft:armorer/level_1").unwrap();
        assert_eq!(set.trades.len(), base.trades.len() + 1);
        let gem = overlay.index_of("t:gem").unwrap();
        assert!(set.trades.contains(&gem));
        assert!(usize::from(gem) >= trading::trade_count());
    }

    #[test]
    fn modifiers_are_parsed_and_unknown_kinds_or_items_are_rejected() {
        let trade = parse_trade(
            "t:m",
            r##"{"wants":{"id":"minecraft:emerald"},"gives":{"id":"minecraft:enchanted_book"},"given_item_modifier":[{"type":"minecraft:enchant_with_levels","options":"#minecraft:tradeable","levels":{"type":"minecraft:uniform","min":5,"max":9},"include_additional_cost_component":true},{"type":"minecraft:set_potion","id":"swiftness"},{"type":"minecraft:filtered","item_filter":{"predicates":{"minecraft:stored_enchantments":[{}]}},"on_fail":{"type":"minecraft:discard"}}]}"##,
        )
        .unwrap();
        assert_eq!(trade.modifiers.len(), 3);
        assert!(matches!(
            trade.modifiers[0],
            Modifier::EnchantWithLevels {
                min: 5,
                max: 9,
                additional_cost: true,
                ..
            }
        ));
        assert_eq!(
            trade.modifiers[1],
            Modifier::SetPotion("minecraft:swiftness")
        );
        assert_eq!(
            trade.modifiers[2],
            Modifier::Require(Require::StoredEnchantments)
        );
        assert!(matches!(
            parse_trade(
                "t:m",
                r#"{"wants":{"id":"minecraft:emerald"},"gives":{"id":"minecraft:book"},"given_item_modifier":[{"type":"minecraft:set_components"}]}"#
            ),
            Err(DataError::Unsupported(_))
        ));
        assert!(
            parse_trade(
                "t:u",
                r#"{"wants":{"id":"minecraft:nope"},"gives":{"id":"minecraft:book"}}"#
            )
            .is_err()
        );
    }
}
