mod hash;
mod schema;

use rspigot_domain::entity::save::component_from_tag;
use rspigot_nbt::{Compound, Tag};
use rspigot_protocol::play::slot::{ComponentValue, DataComponentPatch};
use rspigot_protocol::play::wire::{read_var, write_var};

use crate::text::to_nbt;

const DATA_COMPONENT_TYPE: &str = "minecraft:data_component_type";
const ITEM_REGISTRY: &str = "minecraft:item";
const REMOVED_PREFIX: &str = "!";
const MAX_STRING_BYTES: usize = 32767 * 3;
const MAX_NESTING: u8 = 8;

#[derive(Clone, Copy)]
pub(crate) enum Reg {
    Static(&'static str),
    Synced(&'static str),
}

impl Reg {
    pub(crate) fn index(self, name: &str) -> Option<i32> {
        let name = qualified(name);
        let id = match self {
            Self::Static(registry) => rspigot_data::registries::static_index(registry, &name),
            Self::Synced(registry) => rspigot_data::registries::synced_index(registry, &name),
        }?;
        i32::try_from(id).ok()
    }

    pub(crate) fn name(self, id: i32) -> Option<&'static str> {
        let entries = match self {
            Self::Static(registry) => rspigot_data::registries::static_entries(registry),
            Self::Synced(registry) => rspigot_data::registries::synced_entries(registry),
        }?;
        entries.get(usize::try_from(id).ok()?).copied()
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Def {
    Bool(bool),
    Int(i32),
    Float(f32),
    Str(&'static str),
    Empty,
    With(fn() -> Tag),
}

impl Def {
    fn tag(self) -> Tag {
        match self {
            Self::Bool(value) => Tag::Byte(i8::from(value)),
            Self::Int(value) => Tag::Int(value),
            Self::Float(value) => Tag::Float(value),
            Self::Str(value) => Tag::String(value.to_owned()),
            Self::Empty => Tag::List(Vec::new()),
            Self::With(build) => build(),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Rule {
    Req,
    Opt,
    Def(Def),
}

#[derive(Clone, Copy)]
pub(crate) struct Field {
    pub(crate) name: &'static str,
    pub(crate) node: Node,
    pub(crate) rule: Rule,
}

#[derive(Clone, Copy)]
pub(crate) struct Custom {
    pub(crate) enc: fn(&Tag, &mut Vec<u8>) -> Option<()>,
    pub(crate) dec: fn(&mut &[u8]) -> Option<Tag>,
    pub(crate) hash: Option<fn(&Tag, u8) -> Option<u32>>,
}

#[derive(Clone, Copy)]
pub(crate) enum Node {
    Unit,
    Bool,
    Int,
    Var,
    Float,
    Double,
    Str,
    Ident,
    Text,
    Nbt,
    Enum(&'static [&'static str]),
    Reg(Reg),
    Holder(Reg, &'static Node),
    Set(Reg),
    Fixed(&'static Node, usize),
    EnumIds(&'static [(&'static str, i32)]),
    Wrap(&'static str, &'static Node),
    Filterable(&'static Node),
    Constant(&'static Node),
    Compact(&'static Node, usize),
    List(&'static Node, usize),
    Rec(&'static [Field]),
    Stack,
    Never,
    Custom(Custom),
}

pub(crate) const fn req(name: &'static str, node: Node) -> Field {
    Field {
        name,
        node,
        rule: Rule::Req,
    }
}

pub(crate) const fn opt(name: &'static str, node: Node) -> Field {
    Field {
        name,
        node,
        rule: Rule::Opt,
    }
}

pub(crate) const fn def(name: &'static str, node: Node, default: Def) -> Field {
    Field {
        name,
        node,
        rule: Rule::Def(default),
    }
}

pub(crate) fn qualified(name: &str) -> std::borrow::Cow<'_, str> {
    if name.contains(':') {
        std::borrow::Cow::Borrowed(name)
    } else {
        std::borrow::Cow::Owned(format!("minecraft:{name}"))
    }
}

pub(crate) fn write_string(value: &str, out: &mut Vec<u8>) -> Option<()> {
    if value.len() > MAX_STRING_BYTES {
        return None;
    }
    write_var(i32::try_from(value.len()).ok()?, out);
    out.extend_from_slice(value.as_bytes());
    Some(())
}

pub(crate) fn read_string(input: &mut &[u8]) -> Option<String> {
    let len = usize::try_from(read_var(input).ok()?).ok()?;
    if len > MAX_STRING_BYTES {
        return None;
    }
    String::from_utf8(take(input, len)?.to_vec()).ok()
}

pub(crate) fn take<'a>(input: &mut &'a [u8], len: usize) -> Option<&'a [u8]> {
    if input.len() < len {
        return None;
    }
    let (head, tail) = input.split_at(len);
    *input = tail;
    Some(head)
}

fn read_len(input: &mut &[u8], max: usize) -> Option<usize> {
    let len = usize::try_from(read_var(input).ok()?).ok()?;
    (len <= max).then_some(len)
}

fn number(tag: &Tag) -> Option<i64> {
    tag.as_i64()
}

fn float(tag: &Tag) -> Option<f64> {
    tag.as_f64()
}

fn read_array<const N: usize>(input: &mut &[u8]) -> Option<[u8; N]> {
    take(input, N)?.try_into().ok()
}

impl Node {
    pub(crate) fn encode(self, tag: &Tag, out: &mut Vec<u8>) -> Option<()> {
        match self {
            Self::Unit => {
                tag.as_compound()?;
            }
            Self::Bool => out.push(u8::from(number(tag)? != 0)),
            Self::Int => out.extend(i32::try_from(number(tag)?).ok()?.to_be_bytes()),
            Self::Var => write_var(i32::try_from(number(tag)?).ok()?, out),
            Self::Float => out.extend((float(tag)? as f32).to_be_bytes()),
            Self::Double => out.extend(float(tag)?.to_be_bytes()),
            Self::Str => write_string(tag.as_str()?, out)?,
            Self::Ident => write_string(&qualified(tag.as_str()?), out)?,
            Self::Text => rspigot_nbt::write_network(out, &to_nbt(&component_from_tag(tag)?)),
            Self::Nbt => {
                rspigot_nbt::write_network(out, &Tag::Compound(tag.as_compound()?.clone()))
            }
            Self::Enum(names) => {
                let name = tag.as_str()?;
                let index = names.iter().position(|candidate| *candidate == name)?;
                write_var(i32::try_from(index).ok()?, out);
            }
            Self::Reg(registry) => write_var(registry.index(tag.as_str()?)?, out),
            Self::Holder(registry, direct) => match tag {
                Tag::String(name) => write_var(registry.index(name)?.checked_add(1)?, out),
                other => {
                    write_var(0, out);
                    direct.encode(other, out)?;
                }
            },
            Self::Set(registry) => encode_set(registry, tag, out)?,
            Self::Fixed(element, len) => {
                let items = tag.as_list()?;
                if items.len() != len {
                    return None;
                }
                for item in items {
                    element.encode(item, out)?;
                }
            }
            Self::EnumIds(names) => {
                let name = tag.as_str()?;
                write_var(
                    names.iter().find(|(candidate, _)| *candidate == name)?.1,
                    out,
                );
            }
            Self::Wrap(key, inner) => match tag {
                Tag::Compound(_) => inner.encode(tag, out)?,
                other => inner.encode(
                    &Tag::Compound(Compound::new().with(key, other.clone())),
                    out,
                )?,
            },
            Self::Filterable(inner) => encode_filterable(inner, tag, out)?,
            Self::Compact(inner, max) => match tag {
                Tag::List(_) => Self::List(inner, max).encode(tag, out)?,
                single => {
                    write_var(1, out);
                    inner.encode(single, out)?;
                }
            },
            Self::Constant(inner) => {
                out.push(1);
                inner.encode(tag, out)?;
            }
            Self::List(element, max) => {
                let items = tag.as_list()?;
                if items.len() > max {
                    return None;
                }
                write_var(i32::try_from(items.len()).ok()?, out);
                for item in items {
                    element.encode(item, out)?;
                }
            }
            Self::Rec(fields) => {
                let compound = tag.as_compound()?;
                for field in fields {
                    field.encode(compound, out)?;
                }
            }
            Self::Stack => encode_stack(tag, out, MAX_NESTING)?,
            Self::Never => return None,
            Self::Custom(custom) => (custom.enc)(tag, out)?,
        }
        Some(())
    }

    pub(crate) fn decode(self, input: &mut &[u8]) -> Option<Tag> {
        Some(match self {
            Self::Unit => Tag::Compound(Compound::new()),
            Self::Bool => Tag::Byte(match take(input, 1)?[0] {
                0 => 0,
                1 => 1,
                _ => return None,
            }),
            Self::Int => Tag::Int(i32::from_be_bytes(read_array(input)?)),
            Self::Var => Tag::Int(read_var(input).ok()?),
            Self::Float => Tag::Float(f32::from_be_bytes(read_array(input)?)),
            Self::Double => Tag::Double(f64::from_be_bytes(read_array(input)?)),
            Self::Str | Self::Ident => Tag::String(read_string(input)?),
            Self::Text => rspigot_nbt::read_network(input).ok()??,
            Self::Nbt => Tag::Compound(rspigot_nbt::read_network_compound(input).ok()??),
            Self::Enum(names) => {
                let index = usize::try_from(read_var(input).ok()?).ok()?;
                Tag::String((*names.get(index)?).to_owned())
            }
            Self::Reg(registry) => Tag::String(registry.name(read_var(input).ok()?)?.to_owned()),
            Self::Holder(registry, direct) => match read_var(input).ok()? {
                0 => direct.decode(input)?,
                id => Tag::String(registry.name(id.checked_sub(1)?)?.to_owned()),
            },
            Self::Set(registry) => decode_set(registry, input)?,
            Self::Fixed(element, len) => {
                let mut items = Vec::with_capacity(len);
                for _ in 0..len {
                    items.push(element.decode(input)?);
                }
                Tag::List(items)
            }
            Self::EnumIds(names) => {
                let id = read_var(input).ok()?;
                Tag::String(
                    names
                        .iter()
                        .find(|(_, candidate)| *candidate == id)?
                        .0
                        .to_owned(),
                )
            }
            Self::Wrap(_, inner) => inner.decode(input)?,
            Self::Filterable(inner) => decode_filterable(inner, input)?,
            Self::Compact(inner, max) => Self::List(inner, max).decode(input)?,
            Self::Constant(inner) => {
                if take(input, 1)?[0] != 1 {
                    return None;
                }
                inner.decode(input)?
            }
            Self::List(element, max) => {
                let len = read_len(input, max)?;
                let mut items = Vec::with_capacity(len.min(input.len()));
                for _ in 0..len {
                    items.push(element.decode(input)?);
                }
                Tag::List(items)
            }
            Self::Rec(fields) => {
                let mut compound = Compound::new();
                for field in fields {
                    field.decode(&mut compound, input)?;
                }
                Tag::Compound(compound)
            }
            Self::Stack => decode_stack(input, MAX_NESTING)?,
            Self::Never => return None,
            Self::Custom(custom) => (custom.dec)(input)?,
        })
    }
}

impl Field {
    fn encode(&self, compound: &Compound, out: &mut Vec<u8>) -> Option<()> {
        let value = compound.get(self.name);
        match (self.rule, value) {
            (Rule::Req, Some(value)) => self.node.encode(value, out),
            (Rule::Req, None) => None,
            (Rule::Opt, Some(value)) => {
                out.push(1);
                self.node.encode(value, out)
            }
            (Rule::Opt, None) => {
                out.push(0);
                Some(())
            }
            (Rule::Def(_), Some(value)) => self.node.encode(value, out),
            (Rule::Def(default), None) => self.node.encode(&default.tag(), out),
        }
    }

    fn decode(&self, compound: &mut Compound, input: &mut &[u8]) -> Option<()> {
        if matches!(self.rule, Rule::Opt) {
            match take(input, 1)?[0] {
                0 => return Some(()),
                1 => {}
                _ => return None,
            }
        }
        compound.insert(self.name, self.node.decode(input)?);
        Some(())
    }
}

fn encode_filterable(inner: &Node, tag: &Tag, out: &mut Vec<u8>) -> Option<()> {
    match tag
        .as_compound()
        .filter(|compound| compound.contains_key("raw"))
    {
        Some(filterable) => {
            inner.encode(filterable.get("raw")?, out)?;
            match filterable.get("filtered") {
                Some(filtered) => {
                    out.push(1);
                    inner.encode(filtered, out)
                }
                None => {
                    out.push(0);
                    Some(())
                }
            }
        }
        None => {
            inner.encode(tag, out)?;
            out.push(0);
            Some(())
        }
    }
}

fn decode_filterable(inner: &Node, input: &mut &[u8]) -> Option<Tag> {
    let raw = inner.decode(input)?;
    match take(input, 1)?[0] {
        0 => Some(raw),
        1 => Some(Tag::Compound(
            Compound::new()
                .with("raw", raw)
                .with("filtered", inner.decode(input)?),
        )),
        _ => None,
    }
}

fn encode_set(registry: Reg, tag: &Tag, out: &mut Vec<u8>) -> Option<()> {
    match tag {
        Tag::String(name) => match name.strip_prefix('#') {
            Some(key) => {
                write_var(0, out);
                write_string(&qualified(key), out)
            }
            None => {
                write_var(2, out);
                write_var(registry.index(name)?, out);
                Some(())
            }
        },
        Tag::List(entries) => {
            write_var(i32::try_from(entries.len()).ok()?.checked_add(1)?, out);
            for entry in entries {
                write_var(registry.index(entry.as_str()?)?, out);
            }
            Some(())
        }
        _ => None,
    }
}

fn decode_set(registry: Reg, input: &mut &[u8]) -> Option<Tag> {
    match read_var(input).ok()? {
        0 => Some(Tag::String(format!("#{}", read_string(input)?))),
        count if count > 0 => {
            let count = usize::try_from(count - 1).ok()?;
            let mut entries = Vec::with_capacity(count.min(input.len()));
            for _ in 0..count {
                entries.push(Tag::String(
                    registry.name(read_var(input).ok()?)?.to_owned(),
                ));
            }
            Some(Tag::List(entries))
        }
        _ => None,
    }
}

fn encode_stack(tag: &Tag, out: &mut Vec<u8>, depth: u8) -> Option<()> {
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
    if count < 1 {
        return None;
    }
    write_var(Reg::Static(ITEM_REGISTRY).index(item)?, out);
    write_var(count, out);
    match components {
        Some(components) => encode_patch(components, out, depth),
        None => {
            out.extend([0, 0]);
            Some(())
        }
    }
}

fn decode_stack(input: &mut &[u8], depth: u8) -> Option<Tag> {
    let depth = depth.checked_sub(1)?;
    let item = Reg::Static(ITEM_REGISTRY).name(read_var(input).ok()?)?;
    let count = read_var(input).ok()?;
    if count < 1 {
        return None;
    }
    let components = decode_patch(input, depth)?;
    let mut stack = Compound::new().with("id", item).with("count", count);
    if !components.is_empty() {
        stack.insert("components", components);
    }
    Some(Tag::Compound(stack))
}

fn encode_patch(components: &Compound, out: &mut Vec<u8>, depth: u8) -> Option<()> {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    for (name, value) in components.iter() {
        match name.strip_prefix(REMOVED_PREFIX) {
            Some(name) => removed.extend(component_type(name)),
            None => {
                if let Some(node) = schema::node(name)
                    && let Some(type_id) = component_type(name)
                {
                    let mut data = Vec::new();
                    node.encode_nested(value, &mut data, depth)?;
                    added.push((type_id, data));
                }
            }
        }
    }
    added.sort_by_key(|(type_id, _)| *type_id);
    removed.sort_unstable();
    write_var(i32::try_from(added.len()).ok()?, out);
    write_var(i32::try_from(removed.len()).ok()?, out);
    for (type_id, data) in added {
        write_var(type_id, out);
        out.extend(data);
    }
    for type_id in removed {
        write_var(type_id, out);
    }
    Some(())
}

fn decode_patch(input: &mut &[u8], depth: u8) -> Option<Compound> {
    let added = read_len(input, 256)?;
    let removed = read_len(input, 256)?;
    let mut components = Compound::new();
    for _ in 0..added {
        let name = component_name(read_var(input).ok()?)?;
        let value = schema::node(name)?.decode_nested(input, depth)?;
        components.insert(name, value);
    }
    for _ in 0..removed {
        let name = component_name(read_var(input).ok()?)?;
        components.insert(format!("{REMOVED_PREFIX}{name}"), Compound::new());
    }
    Some(components)
}

impl Node {
    fn encode_nested(self, tag: &Tag, out: &mut Vec<u8>, depth: u8) -> Option<()> {
        match self {
            Self::Stack => encode_stack(tag, out, depth),
            other => other.encode(tag, out),
        }
    }

    fn decode_nested(self, input: &mut &[u8], depth: u8) -> Option<Tag> {
        match self {
            Self::Stack => decode_stack(input, depth),
            other => other.decode(input),
        }
    }
}

pub(crate) use hash::component_hash;
pub(crate) use schema::encode_resolvable_profile;

pub(crate) fn component_type(name: &str) -> Option<i32> {
    Reg::Static(DATA_COMPONENT_TYPE).index(name)
}

pub(crate) fn component_name(id: i32) -> Option<&'static str> {
    Reg::Static(DATA_COMPONENT_TYPE).name(id)
}

pub(crate) fn encode_value(name: &str, value: &Tag) -> Option<ComponentValue> {
    let mut data = Vec::new();
    schema::node(name)?.encode(value, &mut data)?;
    Some(ComponentValue {
        type_id: component_type(name)?,
        data,
    })
}

pub(crate) fn encode_components(components: &Compound) -> DataComponentPatch {
    let mut patch = DataComponentPatch::EMPTY;
    for (name, value) in components.iter() {
        match name.strip_prefix(REMOVED_PREFIX) {
            Some(name) => patch.removed.extend(component_type(name)),
            None => patch.added.extend(encode_value(name, value)),
        }
    }
    patch.added.sort_by_key(|value| value.type_id);
    patch.removed.sort_unstable();
    patch
}

pub(crate) fn decode_components(patch: &DataComponentPatch) -> Compound {
    let mut components = Compound::new();
    for value in &patch.added {
        let Some(name) = component_name(value.type_id) else {
            continue;
        };
        let Some(node) = schema::node(name) else {
            continue;
        };
        let mut data = value.data.as_slice();
        if let Some(decoded) = node.decode_nested(&mut data, MAX_NESTING)
            && data.is_empty()
        {
            components.insert(name, decoded);
        }
    }
    for type_id in &patch.removed {
        if let Some(name) = component_name(*type_id) {
            components.insert(format!("{REMOVED_PREFIX}{name}"), Compound::new());
        }
    }
    components
}

#[cfg(test)]
mod tests;
