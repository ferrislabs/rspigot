use std::io::Read;

use flate2::read::GzDecoder;
use rspigot_nbt::{Compound, Tag};
use serde_json::{Map, Value, json};

use super::pack::PackError;

fn typed(kind: &str, value: Value) -> Value {
    let mut map = Map::new();
    map.insert(format!("${kind}"), value);
    Value::Object(map)
}

fn convert(tag: &Tag) -> Value {
    match tag {
        Tag::Byte(n) => typed("b", json!(n)),
        Tag::Short(n) => typed("s", json!(n)),
        Tag::Int(n) => json!(n),
        Tag::Long(n) => typed("l", json!(n)),
        Tag::Float(n) => typed("f", json!(f64::from(*n))),
        Tag::Double(n) => json!(n),
        Tag::ByteArray(items) => typed(
            "ba",
            Value::Array(items.iter().map(|n| json!(*n as i8)).collect()),
        ),
        Tag::String(text) => json!(text),
        Tag::List(items) => Value::Array(items.iter().map(convert).collect()),
        Tag::Compound(compound) => compound_json(compound),
        Tag::IntArray(items) => typed("ia", json!(items)),
        Tag::LongArray(items) => typed("la", json!(items)),
    }
}

fn compound_json(compound: &Compound) -> Value {
    Value::Object(
        compound
            .iter()
            .map(|(key, value)| (key.to_owned(), convert(value)))
            .collect(),
    )
}

fn invalid(what: &str) -> PackError {
    PackError::Invalid(format!("template has no valid {what}"))
}

fn triple(tag: Option<&[Tag]>, what: &str) -> Result<Vec<Value>, PackError> {
    let items = tag
        .filter(|items| items.len() == 3)
        .ok_or_else(|| invalid(what))?;
    Ok(items.iter().map(convert).collect())
}

fn palette(entries: &[Tag]) -> Value {
    Value::Array(
        entries
            .iter()
            .filter_map(Tag::as_compound)
            .map(|entry| {
                let name = entry
                    .get_str("Name")
                    .or_else(|| entry.get_str("id"))
                    .unwrap_or("minecraft:air");
                let mut row = vec![json!(name)];
                let properties = entry
                    .get_compound("Properties")
                    .or_else(|| entry.get_compound("properties"));
                if let Some(properties) = properties {
                    let mut pairs: Vec<(&str, &str)> = properties
                        .iter()
                        .filter_map(|(key, value)| Some((key, value.as_str()?)))
                        .collect();
                    pairs.sort_unstable();
                    row.extend(pairs.into_iter().map(|(key, value)| json!([key, value])));
                }
                Value::Array(row)
            })
            .collect(),
    )
}

pub(crate) fn decode(bytes: &[u8]) -> Result<Value, PackError> {
    let mut raw = Vec::new();
    if bytes.starts_with(&[0x1f, 0x8b]) {
        GzDecoder::new(bytes).read_to_end(&mut raw)?;
    } else {
        raw.extend_from_slice(bytes);
    }
    let mut input = raw.as_slice();
    let (_, root) = rspigot_nbt::read_named(&mut input)?;
    let size = triple(root.get_list("size"), "size")?;
    let palettes: Vec<Value> = match root.get_list("palettes") {
        Some(lists) => lists.iter().filter_map(Tag::as_list).map(palette).collect(),
        None => vec![palette(
            root.get_list("palette").ok_or_else(|| invalid("palette"))?,
        )],
    };
    let mut blocks = Vec::new();
    for block in root.get_list("blocks").ok_or_else(|| invalid("blocks"))? {
        let block = block.as_compound().ok_or_else(|| invalid("block"))?;
        let mut row = triple(block.get_list("pos"), "block position")?;
        row.push(json!(
            block
                .get_i32("state")
                .ok_or_else(|| invalid("block state"))?
        ));
        if let Some(nbt) = block.get_compound("nbt") {
            row.push(compound_json(nbt));
        }
        blocks.push(Value::Array(row));
    }
    let entities: Vec<Value> = root
        .get_list("entities")
        .unwrap_or_default()
        .iter()
        .filter_map(Tag::as_compound)
        .filter_map(|entity| {
            Some(json!({
                "pos": Value::Array(entity.get_list("pos")?.iter().map(convert).collect()),
                "block_pos": Value::Array(entity.get_list("blockPos")?.iter().map(convert).collect()),
                "nbt": compound_json(entity.get_compound("nbt")?),
            }))
        })
        .collect();
    Ok(json!({
        "size": size,
        "palettes": palettes,
        "blocks": blocks,
        "entities": entities,
    }))
}
