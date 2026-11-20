use rspigot_data::loot::{
    BonusFormula, Entry, EntryKind, Function, FunctionKind, IntProvider, Leaf, LootTable, Output,
    Pool,
};
use rspigot_data::tags::HolderSet;
use serde_json::{Map, Value};

use super::json::{
    DataError, Parsed, conditions, float_provider, int_provider, invalid, item_id, leak_slice,
    leak_text, name, number_provider, object, only_keys, optional_flag, required, unsupported,
};

pub struct TableOutcome {
    pub table: LootTable,
    pub skipped: Vec<String>,
}

fn whole(value: &Value, what: &str) -> Parsed<i32> {
    match value.as_i64().and_then(|n| i32::try_from(n).ok()) {
        Some(number) => Ok(number),
        None => invalid(format!("{what} must be an integer")),
    }
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
                "bonusMultiplier",
            )?,
        }),
        "minecraft:binomial_with_bonus_count" => Ok(BonusFormula::BinomialWithBonusCount {
            extra: whole(parameter(&["extra"]).unwrap_or(&Value::Null), "extra")?,
            probability: parameter(&["probability"])
                .and_then(Value::as_f64)
                .map_or_else(|| invalid("missing probability"), |p| Ok(p as f32))?,
        }),
        other => unsupported(format!("bonus formula {other}")),
    }
}

fn function(value: &Value) -> Parsed<Function> {
    let map = object(value)?;
    let condition = conditions(map)?;
    let kind = match name(required(map, "function")?)?.as_str() {
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
            use_input_count: true,
        },
        "minecraft:enchanted_count_increase" => FunctionKind::EnchantedCountIncrease {
            enchantment: leak_text(name(required(map, "enchantment")?)?),
            count: float_provider(required(map, "count")?)?,
            limit: map
                .get("limit")
                .map_or(Ok(0), |limit| whole(limit, "limit"))?,
        },
        "minecraft:set_damage" => {
            if optional_flag(map, "add")?.unwrap_or(false) {
                return unsupported("set_damage with add");
            }
            FunctionKind::SetDamage(float_provider(required(map, "damage")?)?)
        }
        "minecraft:set_potion" => FunctionKind::SetPotion(leak_text(name(required(map, "id")?)?)),
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
        other => return unsupported(format!("function {other}")),
    };
    Ok(Function { condition, kind })
}

fn functions(map: &Map<String, Value>) -> Parsed<&'static [Function]> {
    let Some(Value::Array(list)) = map.get("functions") else {
        return Ok(&[]);
    };
    let mut parsed = Vec::with_capacity(list.len());
    for item in list {
        parsed.push(function(item)?);
    }
    Ok(leak_slice(parsed))
}

fn children(map: &Map<String, Value>, skipped: &mut Vec<String>) -> Parsed<&'static [Entry]> {
    let Some(Value::Array(list)) = map.get("children") else {
        return invalid("missing \"children\"");
    };
    Ok(entries(list, skipped))
}

fn entry(value: &Value, skipped: &mut Vec<String>) -> Parsed<Entry> {
    let map = object(value)?;
    let condition = conditions(map)?;
    let modifier = functions(map)?;
    let leaf = |output: Output| -> Parsed<EntryKind> {
        Ok(EntryKind::Leaf(Leaf {
            weight: map.get("weight").map_or(Ok(1), |w| whole(w, "weight"))?,
            quality: map.get("quality").map_or(Ok(0), |q| whole(q, "quality"))?,
            output,
        }))
    };
    let kind = match name(required(map, "type")?)?.as_str() {
        "minecraft:item" => leaf(Output::Item(item_id(required(map, "name")?)?))?,
        "minecraft:tag" => {
            let tag = name(required(map, "name")?)?;
            leaf(Output::Items {
                items: HolderSet::Tag(leak_text(tag)),
                expand: optional_flag(map, "expand")?.unwrap_or(false),
            })?
        }
        "minecraft:loot_table" => match required(map, "value")? {
            Value::String(_) => leaf(Output::Table(leak_text(name(&map["value"])?)))?,
            _ => return unsupported("inline loot table entry"),
        },
        "minecraft:empty" => leaf(Output::Empty)?,
        "minecraft:dynamic" => leaf(Output::Dynamic(leak_text(name(required(map, "name")?)?)))?,
        "minecraft:alternatives" => EntryKind::Alternatives(children(map, skipped)?),
        "minecraft:sequence" => EntryKind::Sequence(children(map, skipped)?),
        "minecraft:group" => EntryKind::Group(children(map, skipped)?),
        other => return unsupported(format!("entry type {other}")),
    };
    Ok(Entry {
        condition,
        modifier,
        kind,
    })
}

fn entries(list: &[Value], skipped: &mut Vec<String>) -> &'static [Entry] {
    let mut parsed = Vec::with_capacity(list.len());
    for (index, value) in list.iter().enumerate() {
        match entry(value, skipped) {
            Ok(entry) => parsed.push(entry),
            Err(error) => skipped.push(format!("entry {index}: {error}")),
        }
    }
    leak_slice(parsed)
}

fn pool(value: &Value, skipped: &mut Vec<String>) -> Parsed<Pool> {
    let map = object(value)?;
    only_keys(
        map,
        &["rolls", "bonus_rolls", "conditions", "functions", "entries"],
        "pool",
    )?;
    let Some(Value::Array(list)) = map.get("entries") else {
        return invalid("missing \"entries\"");
    };
    let bonus_rolls = match map.get("bonus_rolls") {
        None => 0.0,
        Some(value) => match number_provider(value)? {
            rspigot_data::loot::NumberProvider::Constant(rolls) => rolls,
            _ => return unsupported("non constant bonus_rolls"),
        },
    };
    Ok(Pool {
        rolls: int_provider(required(map, "rolls")?)?,
        bonus_rolls,
        condition: conditions(map)?,
        modifier: functions(map)?,
        entries: entries(list, skipped),
    })
}

pub fn parse(value: &Value) -> Result<TableOutcome, DataError> {
    let map = object(value)?;
    let mut skipped = Vec::new();
    let mut pools = Vec::new();
    if let Some(Value::Array(list)) = map.get("pools") {
        for (index, item) in list.iter().enumerate() {
            match pool(item, &mut skipped) {
                Ok(pool) => pools.push(pool),
                Err(error) => skipped.push(format!("pool {index}: {error}")),
            }
        }
    }
    Ok(TableOutcome {
        table: LootTable {
            pools: leak_slice(pools),
            modifier: functions(map)?,
        },
        skipped,
    })
}
