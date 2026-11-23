use rspigot_data::loot::{Entry, EntryKind, FloatProvider, Leaf, LootTable, Output, Pool};
use rspigot_data::tags::HolderSet;
use serde_json::{Map, Value};

use super::functions::functions;
use super::json::{
    DataError, Parsed, conditions, float_provider, holder_set, int_provider, invalid, item_id,
    leak, leak_slice, leak_text, name, object, only_keys, optional_flag, required, unsupported,
};
use super::slots::slot_source;

pub struct TableOutcome {
    pub table: LootTable,
    pub skipped: Vec<String>,
    pub inline: Vec<(String, LootTable)>,
}

struct Collected<'a> {
    name: &'a str,
    skipped: Vec<String>,
    inline: Vec<(String, LootTable)>,
}

fn whole(value: &Value, what: &str) -> Parsed<i32> {
    match value.as_i64().and_then(|n| i32::try_from(n).ok()) {
        Some(number) => Ok(number),
        None => invalid(format!("{what} must be an integer")),
    }
}

fn children(map: &Map<String, Value>, found: &mut Collected) -> Parsed<&'static [Entry]> {
    let Some(Value::Array(list)) = map.get("children") else {
        return invalid("missing \"children\"");
    };
    Ok(entries(list, found))
}

fn entry(value: &Value, found: &mut Collected) -> Parsed<Entry> {
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
            let items = match map.get("items") {
                Some(items) => holder_set("minecraft:item", items)?,
                None => HolderSet::Tag(leak_text(name(required(map, "name")?)?)),
            };
            leaf(Output::Items {
                items,
                expand: optional_flag(map, "expand")?.unwrap_or(false),
            })?
        }
        "minecraft:loot_table" => match required(map, "value")? {
            Value::String(_) => leaf(Output::Table(leak_text(name(&map["value"])?)))?,
            inline => {
                let inline_name = format!("{}/inline{}", found.name, found.inline.len());
                let outcome = parse_named(inline, &inline_name)?;
                found.skipped.extend(outcome.skipped);
                found.inline.extend(outcome.inline);
                found.inline.push((inline_name.clone(), outcome.table));
                leaf(Output::Table(leak_text(inline_name)))?
            }
        },
        "minecraft:empty" => leaf(Output::Empty)?,
        "minecraft:dynamic" => leaf(Output::Dynamic(leak_text(name(required(map, "name")?)?)))?,
        "minecraft:slots" => leaf(Output::Slots(leak(slot_source(required(
            map,
            "slot_source",
        )?)?)))?,
        "minecraft:alternatives" => EntryKind::Alternatives(children(map, found)?),
        "minecraft:sequence" => EntryKind::Sequence(children(map, found)?),
        "minecraft:group" => EntryKind::Group(children(map, found)?),
        other => return unsupported(format!("entry type {other}")),
    };
    Ok(Entry {
        condition,
        modifier,
        kind,
    })
}

fn entries(list: &[Value], found: &mut Collected) -> &'static [Entry] {
    let mut parsed = Vec::with_capacity(list.len());
    for (index, value) in list.iter().enumerate() {
        match entry(value, found) {
            Ok(entry) => parsed.push(entry),
            Err(error) => found.skipped.push(format!("entry {index}: {error}")),
        }
    }
    leak_slice(parsed)
}

fn pool(value: &Value, found: &mut Collected) -> Parsed<Pool> {
    let map = object(value)?;
    only_keys(
        map,
        &[
            "rolls",
            "bonus_rolls",
            "condition",
            "conditions",
            "modifier",
            "functions",
            "entries",
        ],
        "pool",
    )?;
    let Some(Value::Array(list)) = map.get("entries") else {
        return invalid("missing \"entries\"");
    };
    let bonus_rolls = match map.get("bonus_rolls") {
        None => FloatProvider::Constant(0.0),
        Some(value) => float_provider(value)?,
    };
    Ok(Pool {
        rolls: int_provider(required(map, "rolls")?)?,
        bonus_rolls,
        condition: conditions(map)?,
        modifier: functions(map)?,
        entries: entries(list, found),
    })
}

pub fn parse_named(value: &Value, table_name: &str) -> Result<TableOutcome, DataError> {
    let map = object(value)?;
    let mut found = Collected {
        name: table_name,
        skipped: Vec::new(),
        inline: Vec::new(),
    };
    let mut pools = Vec::new();
    if let Some(Value::Array(list)) = map.get("pools") {
        for (index, item) in list.iter().enumerate() {
            match pool(item, &mut found) {
                Ok(pool) => pools.push(pool),
                Err(error) => found.skipped.push(format!("pool {index}: {error}")),
            }
        }
    }
    Ok(TableOutcome {
        table: LootTable {
            pools: leak_slice(pools),
            modifier: functions(map)?,
        },
        skipped: found.skipped,
        inline: found.inline,
    })
}

pub(super) fn entry_list(list: &[Value], table_name: &str) -> Parsed<&'static [Entry]> {
    let mut found = Collected {
        name: table_name,
        skipped: Vec::new(),
        inline: Vec::new(),
    };
    let parsed = entries(list, &mut found);
    match found.skipped.first() {
        Some(reason) => invalid(reason.clone()),
        None => Ok(parsed),
    }
}
