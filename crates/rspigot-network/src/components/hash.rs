use rspigot_nbt::{Compound, Tag};

use super::{Def, Node, Rule, qualified, schema};

const CRC32C_POLY: u32 = 0x82F6_3B78;

const TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut crc = n as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ CRC32C_POLY
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[n] = crc;
        n += 1;
    }
    table
};

const TAG_MAP_START: u8 = 2;
const TAG_MAP_END: u8 = 3;
const TAG_LIST_START: u8 = 4;
const TAG_LIST_END: u8 = 5;
const TAG_BYTE: u8 = 6;
const TAG_SHORT: u8 = 7;
const TAG_INT: u8 = 8;
const TAG_LONG: u8 = 9;
const TAG_FLOAT: u8 = 10;
const TAG_DOUBLE: u8 = 11;
const TAG_STRING: u8 = 12;
const TAG_BOOLEAN: u8 = 13;
const TAG_BYTE_ARRAY_START: u8 = 14;
const TAG_BYTE_ARRAY_END: u8 = 15;
const TAG_INT_ARRAY_START: u8 = 16;
const TAG_INT_ARRAY_END: u8 = 17;
const TAG_LONG_ARRAY_START: u8 = 18;
const TAG_LONG_ARRAY_END: u8 = 19;

pub(crate) fn crc32c(bytes: &[u8]) -> u32 {
    !bytes.iter().fold(!0u32, |crc, byte| {
        TABLE[usize::from((crc as u8) ^ byte)] ^ (crc >> 8)
    })
}

fn leaf(tag: u8, payload: &[u8]) -> u32 {
    let mut bytes = Vec::with_capacity(1 + payload.len());
    bytes.push(tag);
    bytes.extend_from_slice(payload);
    crc32c(&bytes)
}

fn byte(value: i8) -> u32 {
    leaf(TAG_BYTE, &value.to_le_bytes())
}

fn short(value: i16) -> u32 {
    leaf(TAG_SHORT, &value.to_le_bytes())
}

fn int(value: i32) -> u32 {
    leaf(TAG_INT, &value.to_le_bytes())
}

fn long(value: i64) -> u32 {
    leaf(TAG_LONG, &value.to_le_bytes())
}

fn float(value: f32) -> u32 {
    leaf(TAG_FLOAT, &value.to_le_bytes())
}

fn double(value: f64) -> u32 {
    leaf(TAG_DOUBLE, &value.to_le_bytes())
}

fn boolean(value: bool) -> u32 {
    crc32c(&[TAG_BOOLEAN, u8::from(value)])
}

fn string(value: &str) -> u32 {
    let units: Vec<u16> = value.encode_utf16().collect();
    let mut bytes = Vec::with_capacity(5 + units.len() * 2);
    bytes.push(TAG_STRING);
    bytes.extend((units.len() as i32).to_le_bytes());
    for unit in units {
        bytes.extend(unit.to_le_bytes());
    }
    crc32c(&bytes)
}

fn list(items: &[u32]) -> u32 {
    let mut bytes = vec![TAG_LIST_START];
    for item in items {
        bytes.extend(item.to_le_bytes());
    }
    bytes.push(TAG_LIST_END);
    crc32c(&bytes)
}

fn map(mut entries: Vec<(u32, u32)>) -> u32 {
    entries.sort_unstable();
    let mut bytes = vec![TAG_MAP_START];
    for (key, value) in entries {
        bytes.extend(key.to_le_bytes());
        bytes.extend(value.to_le_bytes());
    }
    bytes.push(TAG_MAP_END);
    crc32c(&bytes)
}

fn array(start: u8, end: u8, body: &[u8]) -> u32 {
    let mut bytes = vec![start];
    bytes.extend_from_slice(body);
    bytes.push(end);
    crc32c(&bytes)
}

fn structural(tag: &Tag) -> u32 {
    match tag {
        Tag::Byte(value) => byte(*value),
        Tag::Short(value) => short(*value),
        Tag::Int(value) => int(*value),
        Tag::Long(value) => long(*value),
        Tag::Float(value) => float(*value),
        Tag::Double(value) => double(*value),
        Tag::String(value) => string(value),
        Tag::List(items) => list(&items.iter().map(structural).collect::<Vec<_>>()),
        Tag::Compound(compound) => structural_compound(compound),
        Tag::ByteArray(values) => array(TAG_BYTE_ARRAY_START, TAG_BYTE_ARRAY_END, values),
        Tag::IntArray(values) => array(
            TAG_INT_ARRAY_START,
            TAG_INT_ARRAY_END,
            &values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>(),
        ),
        Tag::LongArray(values) => array(
            TAG_LONG_ARRAY_START,
            TAG_LONG_ARRAY_END,
            &values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>(),
        ),
    }
}

fn structural_compound(compound: &Compound) -> u32 {
    map(compound
        .iter()
        .map(|(key, value)| (string(key), structural(value)))
        .collect())
}

fn qualified_string(name: &str) -> u32 {
    string(&qualified(name))
}

fn field_default(default: Def) -> Tag {
    default.tag()
}

fn record(fields: &[super::Field], compound: &Compound, depth: u8) -> Option<u32> {
    let mut entries = Vec::with_capacity(fields.len());
    for field in fields {
        let value = compound.get(field.name);
        let hashed = match (field.rule, value) {
            (Rule::Req, Some(value)) => field.node.hash(value, depth)?,
            (Rule::Req, None) => return None,
            (Rule::Opt, Some(value)) | (Rule::Def(_), Some(value)) => {
                let hashed = field.node.hash(value, depth)?;
                if let Rule::Def(default) = field.rule
                    && field.node.hash(&field_default(default), depth) == Some(hashed)
                {
                    continue;
                }
                hashed
            }
            (_, None) => continue,
        };
        entries.push((string(field.name), hashed));
    }
    Some(map(entries))
}

fn stack(tag: &Tag, depth: u8) -> Option<u32> {
    let depth = depth.checked_sub(1)?;
    let (item, count, components) = match tag {
        Tag::String(item) => (item.as_str(), 1, None),
        Tag::Compound(stack) => (
            stack.get_str("id")?,
            stack.get_i32("count").unwrap_or(1),
            stack.get_compound("components"),
        ),
        _ => return None,
    };
    let mut entries = vec![(string("id"), qualified_string(item))];
    if count != 1 {
        entries.push((string("count"), int(count)));
    }
    if let Some(components) = components.filter(|components| !components.is_empty()) {
        entries.push((string("components"), patch(components, depth)?));
    }
    Some(map(entries))
}

fn patch(components: &Compound, depth: u8) -> Option<u32> {
    let mut entries = Vec::with_capacity(components.len());
    for (name, value) in components.iter() {
        let hashed = match name.strip_prefix('!') {
            Some(_) => map(Vec::new()),
            None => schema::node(name)?.hash(value, depth)?,
        };
        entries.push((string(&qualified(name)), hashed));
    }
    Some(map(entries))
}

impl Node {
    fn hash(self, tag: &Tag, depth: u8) -> Option<u32> {
        Some(match self {
            Self::Unit => map(Vec::new()),
            Self::Bool => boolean(tag.as_i64()? != 0),
            Self::Int | Self::Var => int(i32::try_from(tag.as_i64()?).ok()?),
            Self::Float => float(tag.as_f64()? as f32),
            Self::Double => double(tag.as_f64()?),
            Self::Str | Self::Enum(_) | Self::EnumIds(_) => string(tag.as_str()?),
            Self::Ident | Self::Reg(_) => qualified_string(tag.as_str()?),
            Self::Text | Self::Nbt => structural(tag),
            Self::Custom(custom) => match custom.hash {
                Some(hash) => hash(tag, depth)?,
                None => structural(tag),
            },
            Self::Holder(_, direct) => match tag {
                Tag::String(name) => qualified_string(name),
                other => direct.hash(other, depth)?,
            },
            Self::Set(_) => match tag {
                Tag::String(name) => match name.strip_prefix('#') {
                    Some(key) => string(&format!("#{}", qualified(key))),
                    None => qualified_string(name),
                },
                Tag::List(entries) if entries.len() == 1 => qualified_string(entries[0].as_str()?),
                Tag::List(entries) => list(
                    &entries
                        .iter()
                        .map(|entry| entry.as_str().map(qualified_string))
                        .collect::<Option<Vec<_>>>()?,
                ),
                _ => return None,
            },
            Self::Fixed(element, _) | Self::List(element, _) => list(
                &tag.as_list()?
                    .iter()
                    .map(|item| element.hash(item, depth))
                    .collect::<Option<Vec<_>>>()?,
            ),
            Self::Wrap(key, inner) => match tag {
                Tag::Compound(_) => inner.hash(tag, depth)?,
                other => inner.hash(
                    &Tag::Compound(Compound::new().with(key, other.clone())),
                    depth,
                )?,
            },
            Self::Filterable(inner) => {
                let (raw, filtered) = match tag.as_compound().filter(|c| c.contains_key("raw")) {
                    Some(filterable) => (filterable.get("raw")?, filterable.get("filtered")),
                    None => (tag, None),
                };
                let mut entries = vec![(string("raw"), inner.hash(raw, depth)?)];
                if let Some(filtered) = filtered {
                    entries.push((string("filtered"), inner.hash(filtered, depth)?));
                }
                map(entries)
            }
            Self::Constant(inner) => inner.hash(tag, depth)?,
            Self::Compact(inner, _) => match tag {
                Tag::List(items) if items.len() == 1 => inner.hash(&items[0], depth)?,
                Tag::List(items) => list(
                    &items
                        .iter()
                        .map(|item| inner.hash(item, depth))
                        .collect::<Option<Vec<_>>>()?,
                ),
                single => inner.hash(single, depth)?,
            },
            Self::Rec(fields) => record(fields, tag.as_compound()?, depth)?,
            Self::Stack => stack(tag, depth)?,
            Self::Never => return None,
        })
    }
}

pub(crate) fn effect_hash(tag: &Tag, depth: u8) -> Option<u32> {
    let depth = depth.checked_sub(1)?;
    let effect = tag.as_compound()?;
    let mut entries = vec![(string("id"), qualified_string(effect.get_str("id")?))];
    let amplifier = effect.get_i32("amplifier").unwrap_or(0);
    if amplifier != 0 {
        entries.push((string("amplifier"), byte(i8::try_from(amplifier).ok()?)));
    }
    let duration = effect.get_i32("duration").unwrap_or(0);
    if duration != 0 {
        entries.push((string("duration"), int(duration)));
    }
    if effect.get_bool("ambient").unwrap_or(false) {
        entries.push((string("ambient"), boolean(true)));
    }
    let particles = effect.get_bool("show_particles").unwrap_or(true);
    if !particles {
        entries.push((string("show_particles"), boolean(false)));
    }
    let icon = effect.get_bool("show_icon").unwrap_or(particles);
    entries.push((string("show_icon"), boolean(icon)));
    if let Some(hidden) = effect.get("hidden_effect") {
        entries.push((string("hidden_effect"), effect_hash(hidden, depth)?));
    }
    Some(map(entries))
}

pub(crate) fn component_hash(name: &str, value: &Tag) -> Option<i32> {
    let node = schema::node(&qualified(name))?;
    node.hash(value, super::MAX_NESTING).map(|hash| hash as i32)
}

#[cfg(test)]
mod tests;
