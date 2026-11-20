use std::collections::BTreeMap;

use rspigot_nbt::{Compound, Tag};

#[derive(Debug, Clone, PartialEq)]
pub enum ComponentValue {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    String(String),
    Bytes(Vec<u8>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
    List(Vec<ComponentValue>),
    Compound(BTreeMap<String, ComponentValue>),
}

impl ComponentValue {
    pub const fn bool(flag: bool) -> Self {
        Self::Byte(flag as i8)
    }

    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Byte(number) => Some(*number != 0),
            _ => None,
        }
    }

    pub const fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Byte(number) => Some(*number as i64),
            Self::Short(number) => Some(*number as i64),
            Self::Int(number) => Some(*number as i64),
            Self::Long(number) => Some(*number),
            _ => None,
        }
    }

    pub const fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float(number) => Some(*number as f64),
            Self::Double(number) => Some(*number),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[ComponentValue]> {
        match self {
            Self::List(items) => Some(items),
            _ => None,
        }
    }

    pub const fn as_compound(&self) -> Option<&BTreeMap<String, ComponentValue>> {
        match self {
            Self::Compound(entries) => Some(entries),
            _ => None,
        }
    }

    pub fn empty_compound() -> Self {
        Self::Compound(BTreeMap::new())
    }

    pub(crate) fn to_tag(&self) -> Tag {
        match self {
            Self::Byte(number) => Tag::Byte(*number),
            Self::Short(number) => Tag::Short(*number),
            Self::Int(number) => Tag::Int(*number),
            Self::Long(number) => Tag::Long(*number),
            Self::Float(number) => Tag::Float(*number),
            Self::Double(number) => Tag::Double(*number),
            Self::String(text) => Tag::String(text.clone()),
            Self::Bytes(bytes) => Tag::ByteArray(bytes.clone()),
            Self::IntArray(numbers) => Tag::IntArray(numbers.clone()),
            Self::LongArray(numbers) => Tag::LongArray(numbers.clone()),
            Self::List(items) => Tag::List(items.iter().map(Self::to_tag).collect()),
            Self::Compound(entries) => Tag::Compound(
                entries
                    .iter()
                    .map(|(key, value)| (key.clone(), value.to_tag()))
                    .collect(),
            ),
        }
    }

    pub(crate) fn from_tag(tag: &Tag) -> Self {
        match tag {
            Tag::Byte(number) => Self::Byte(*number),
            Tag::Short(number) => Self::Short(*number),
            Tag::Int(number) => Self::Int(*number),
            Tag::Long(number) => Self::Long(*number),
            Tag::Float(number) => Self::Float(*number),
            Tag::Double(number) => Self::Double(*number),
            Tag::String(text) => Self::String(text.clone()),
            Tag::ByteArray(bytes) => Self::Bytes(bytes.clone()),
            Tag::IntArray(numbers) => Self::IntArray(numbers.clone()),
            Tag::LongArray(numbers) => Self::LongArray(numbers.clone()),
            Tag::List(items) => Self::List(items.iter().map(Self::from_tag).collect()),
            Tag::Compound(entries) => Self::Compound(
                entries
                    .iter()
                    .map(|(key, value)| (key.to_owned(), Self::from_tag(value)))
                    .collect(),
            ),
        }
    }
}

impl From<bool> for ComponentValue {
    fn from(value: bool) -> Self {
        Self::bool(value)
    }
}

impl From<i32> for ComponentValue {
    fn from(value: i32) -> Self {
        Self::Int(value)
    }
}

impl From<i64> for ComponentValue {
    fn from(value: i64) -> Self {
        Self::Long(value)
    }
}

impl From<f32> for ComponentValue {
    fn from(value: f32) -> Self {
        Self::Float(value)
    }
}

impl From<f64> for ComponentValue {
    fn from(value: f64) -> Self {
        Self::Double(value)
    }
}

impl From<&str> for ComponentValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<String> for ComponentValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<Vec<ComponentValue>> for ComponentValue {
    fn from(value: Vec<ComponentValue>) -> Self {
        Self::List(value)
    }
}

impl From<BTreeMap<String, ComponentValue>> for ComponentValue {
    fn from(value: BTreeMap<String, ComponentValue>) -> Self {
        Self::Compound(value)
    }
}

pub(crate) fn decode(bytes: &[u8]) -> Option<Compound> {
    if bytes.is_empty() {
        return Some(Compound::new());
    }
    let mut rest = bytes;
    match rspigot_nbt::read_network_compound(&mut rest) {
        Ok(Some(components)) if rest.is_empty() => Some(components),
        _ => None,
    }
}

pub(crate) fn encode(components: &Compound) -> Vec<u8> {
    let mut out = Vec::new();
    if !components.is_empty() {
        rspigot_nbt::write_network(&mut out, &Tag::Compound(components.clone()));
    }
    out
}
