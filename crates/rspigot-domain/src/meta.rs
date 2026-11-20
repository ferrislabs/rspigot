use std::collections::BTreeMap;

use rspigot_nbt::{Compound, Tag};

pub const MAX_KEYS: usize = 64;
pub const MAX_BYTES: usize = 64 * 1024;
pub const MAX_DEPTH: usize = 8;
pub const MAX_KEY_LENGTH: usize = 128;

pub const FALLING_KEY: &str = "rspigot:falling_block_data";

const TYPE: &str = "t";
const VALUE: &str = "v";

#[derive(Debug, Clone, PartialEq)]
pub enum MetaValue {
    Bool(bool),
    Int(i64),
    Double(f64),
    Text(String),
    Bytes(Vec<u8>),
    List(Vec<MetaValue>),
    Map(BTreeMap<String, MetaValue>),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MetaError {
    #[error("`{0}` is not a namespaced key of lowercase letters, digits and `_-./`")]
    InvalidKey(String),
    #[error("a holder takes at most {MAX_KEYS} keys")]
    TooManyKeys,
    #[error("a holder takes at most {MAX_BYTES} encoded bytes")]
    TooLarge,
    #[error("a value nests at most {MAX_DEPTH} levels")]
    TooDeep,
}

impl MetaError {
    pub const fn is_limit(&self) -> bool {
        !matches!(self, Self::InvalidKey(_))
    }
}

pub fn validate_key(key: &str) -> Result<(), MetaError> {
    let invalid = || MetaError::InvalidKey(key.to_owned());
    if key.len() > MAX_KEY_LENGTH {
        return Err(invalid());
    }
    let (namespace, path) = key.split_once(':').ok_or_else(invalid)?;
    let namespace_ok = !namespace.is_empty()
        && namespace
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.'));
    let path_ok = !path.is_empty()
        && path
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b'/'));
    if namespace_ok && path_ok {
        Ok(())
    } else {
        Err(invalid())
    }
}

impl MetaValue {
    pub fn depth(&self) -> usize {
        match self {
            Self::List(items) => 1 + items.iter().map(Self::depth).max().unwrap_or(0),
            Self::Map(entries) => 1 + entries.values().map(Self::depth).max().unwrap_or(0),
            _ => 0,
        }
    }

    pub fn encoded_size(&self) -> usize {
        match self {
            Self::Bool(_) => 2,
            Self::Int(_) | Self::Double(_) => 9,
            Self::Text(text) => 5 + text.len(),
            Self::Bytes(bytes) => 5 + bytes.len(),
            Self::List(items) => 5 + items.iter().map(Self::encoded_size).sum::<usize>(),
            Self::Map(entries) => {
                5 + entries
                    .iter()
                    .map(|(key, value)| 2 + key.len() + value.encoded_size())
                    .sum::<usize>()
            }
        }
    }

    fn check_nested_keys(&self) -> Result<(), MetaError> {
        match self {
            Self::List(items) => items.iter().try_for_each(Self::check_nested_keys),
            Self::Map(entries) => entries.iter().try_for_each(|(key, value)| {
                if key.len() > MAX_KEY_LENGTH {
                    Err(MetaError::InvalidKey(key.clone()))
                } else {
                    value.check_nested_keys()
                }
            }),
            _ => Ok(()),
        }
    }

    pub fn to_nbt(&self) -> Tag {
        let (kind, value) = match self {
            Self::Bool(flag) => ("bool", Tag::Byte(i8::from(*flag))),
            Self::Int(number) => ("int", Tag::Long(*number)),
            Self::Double(number) => ("double", Tag::Double(*number)),
            Self::Text(text) => ("string", Tag::String(text.clone())),
            Self::Bytes(bytes) => ("bytes", Tag::ByteArray(bytes.clone())),
            Self::List(items) => ("list", Tag::List(items.iter().map(Self::to_nbt).collect())),
            Self::Map(entries) => (
                "map",
                Tag::Compound(
                    entries
                        .iter()
                        .map(|(key, value)| (key.clone(), value.to_nbt()))
                        .collect(),
                ),
            ),
        };
        Tag::Compound(Compound::new().with(TYPE, kind).with(VALUE, value))
    }

    pub fn from_nbt(tag: &Tag) -> Option<Self> {
        let compound = tag.as_compound()?;
        if compound.len() != 2 {
            return None;
        }
        let kind = compound.get_str(TYPE)?;
        let value = compound.get(VALUE)?;
        Some(match (kind, value) {
            ("bool", Tag::Byte(flag)) => Self::Bool(*flag != 0),
            ("int", Tag::Long(number)) => Self::Int(*number),
            ("double", Tag::Double(number)) => Self::Double(*number),
            ("string", Tag::String(text)) => Self::Text(text.clone()),
            ("bytes", Tag::ByteArray(bytes)) => Self::Bytes(bytes.clone()),
            ("list", Tag::List(items)) => {
                Self::List(items.iter().map(Self::from_nbt).collect::<Option<_>>()?)
            }
            ("map", Tag::Compound(entries)) => Self::Map(
                entries
                    .iter()
                    .map(|(key, value)| Some((key.to_owned(), Self::from_nbt(value)?)))
                    .collect::<Option<_>>()?,
            ),
            _ => return None,
        })
    }
}

fn foreign_value(tag: &Tag) -> Option<MetaValue> {
    match tag {
        Tag::Byte(_) | Tag::Short(_) | Tag::Int(_) | Tag::Long(_) => {
            tag.as_i64().map(MetaValue::Int)
        }
        Tag::Float(number) => Some(MetaValue::Double(f64::from(*number))),
        Tag::Double(number) => Some(MetaValue::Double(*number)),
        Tag::String(text) => Some(MetaValue::Text(text.clone())),
        Tag::ByteArray(bytes) => Some(MetaValue::Bytes(bytes.clone())),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Metadata {
    entries: BTreeMap<String, MetaValue>,
    foreign: Compound,
}

impl Metadata {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.foreign.is_empty()
    }

    pub fn has_visible(&self) -> bool {
        !self.entries.is_empty()
            || self
                .foreign
                .iter()
                .any(|(_, tag)| foreign_value(tag).is_some())
    }

    pub fn get(&self, key: &str) -> Option<MetaValue> {
        self.entries
            .get(key)
            .cloned()
            .or_else(|| self.foreign.get(key).and_then(foreign_value))
    }

    pub fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(key) || self.foreign.get(key).and_then(foreign_value).is_some()
    }

    pub fn entries(&self) -> Vec<(String, MetaValue)> {
        let mut all: BTreeMap<String, MetaValue> = self
            .foreign
            .iter()
            .filter_map(|(key, tag)| Some((key.to_owned(), foreign_value(tag)?)))
            .collect();
        all.extend(self.entries.iter().map(|(k, v)| (k.clone(), v.clone())));
        all.into_iter().collect()
    }

    pub fn own(&self) -> impl Iterator<Item = (&str, &MetaValue)> {
        self.entries
            .iter()
            .map(|(key, value)| (key.as_str(), value))
    }

    pub fn set(&mut self, key: &str, value: MetaValue) -> Result<(), MetaError> {
        validate_key(key)?;
        if value.depth() >= MAX_DEPTH {
            return Err(MetaError::TooDeep);
        }
        value.check_nested_keys()?;
        if !self.entries.contains_key(key) && self.entries.len() >= MAX_KEYS {
            return Err(MetaError::TooManyKeys);
        }
        let others: usize = self
            .entries
            .iter()
            .filter(|(existing, _)| existing.as_str() != key)
            .map(|(existing, held)| 2 + existing.len() + held.encoded_size())
            .sum();
        if others + 2 + key.len() + value.encoded_size() > MAX_BYTES {
            return Err(MetaError::TooLarge);
        }
        self.foreign.remove(key);
        self.entries.insert(key.to_owned(), value);
        Ok(())
    }

    pub fn remove(&mut self, key: &str) -> bool {
        let own = self.entries.remove(key).is_some();
        let foreign = self.foreign.get(key).and_then(foreign_value).is_some()
            && self.foreign.remove(key).is_some();
        own || foreign
    }

    pub fn clear(&mut self) -> bool {
        let had = self.has_visible();
        self.entries.clear();
        self.foreign = self
            .foreign
            .iter()
            .filter(|(_, tag)| foreign_value(tag).is_none())
            .map(|(key, tag)| (key.to_owned(), tag.clone()))
            .collect();
        had
    }

    pub fn as_value(&self) -> MetaValue {
        MetaValue::Map(
            self.entries
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        )
    }

    pub fn extend_from(&mut self, value: &MetaValue) {
        if let MetaValue::Map(entries) = value {
            for (key, value) in entries {
                let _ = self.set(key, value.clone());
            }
        }
    }

    pub fn to_nbt(&self) -> Compound {
        let mut out = self.foreign.clone();
        for (key, value) in &self.entries {
            out.insert(key.clone(), value.to_nbt());
        }
        out
    }

    pub fn from_nbt(nbt: &Compound) -> Self {
        let mut meta = Self::new();
        for (key, tag) in nbt.iter() {
            match MetaValue::from_nbt(tag) {
                Some(value) => {
                    meta.entries.insert(key.to_owned(), value);
                }
                None => {
                    meta.foreign.insert(key, tag.clone());
                }
            }
        }
        meta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_value() -> Vec<MetaValue> {
        let mut inner = BTreeMap::new();
        inner.insert("a".to_owned(), MetaValue::Int(-3));
        inner.insert("b".to_owned(), MetaValue::List(vec![MetaValue::Bool(true)]));
        vec![
            MetaValue::Bool(true),
            MetaValue::Bool(false),
            MetaValue::Int(i64::MIN),
            MetaValue::Double(1.5),
            MetaValue::Text("hello".to_owned()),
            MetaValue::Bytes(vec![0, 1, 255]),
            MetaValue::List(vec![MetaValue::Int(1), MetaValue::Text("x".to_owned())]),
            MetaValue::Map(inner),
        ]
    }

    #[test]
    fn every_value_type_round_trips_through_nbt_and_stays_distinguishable() {
        for value in every_value() {
            let tag = value.to_nbt();
            assert_eq!(MetaValue::from_nbt(&tag), Some(value));
        }
        assert_ne!(MetaValue::Bool(true).to_nbt(), MetaValue::Int(1).to_nbt());
    }

    #[test]
    fn keys_are_namespaced_lowercase_identifiers() {
        assert!(validate_key("marker:kind").is_ok());
        assert!(validate_key("my-plugin.x:deep/path_1").is_ok());
        for bad in [
            "kind",
            "Marker:kind",
            "marker:",
            ":kind",
            "marker:Kind",
            "a b:c",
        ] {
            assert!(validate_key(bad).is_err(), "{bad}");
        }
        let long = format!("marker:{}", "a".repeat(MAX_KEY_LENGTH));
        assert!(validate_key(&long).is_err());
    }

    #[test]
    fn set_get_remove_clear() {
        let mut meta = Metadata::new();
        for (index, value) in every_value().into_iter().enumerate() {
            let key = format!("t:k{index}");
            meta.set(&key, value.clone()).unwrap();
            assert_eq!(meta.get(&key), Some(value));
        }
        assert!(meta.remove("t:k0"));
        assert!(!meta.remove("t:k0"));
        assert_eq!(meta.get("t:k0"), None);
        assert!(meta.clear());
        assert!(meta.is_empty());
    }

    #[test]
    fn limits_refuse_the_excess() {
        let mut meta = Metadata::new();
        for index in 0..MAX_KEYS {
            meta.set(&format!("t:k{index}"), MetaValue::Int(1)).unwrap();
        }
        assert_eq!(
            meta.set("t:extra", MetaValue::Int(1)),
            Err(MetaError::TooManyKeys)
        );
        assert!(meta.set("t:k0", MetaValue::Int(2)).is_ok());

        let mut big = Metadata::new();
        assert_eq!(
            big.set("t:big", MetaValue::Bytes(vec![0; MAX_BYTES])),
            Err(MetaError::TooLarge)
        );

        let mut deep = MetaValue::Int(0);
        for _ in 0..MAX_DEPTH {
            deep = MetaValue::List(vec![deep]);
        }
        assert_eq!(Metadata::new().set("t:deep", deep), Err(MetaError::TooDeep));
        let mut ok = MetaValue::Int(0);
        for _ in 0..MAX_DEPTH - 1 {
            ok = MetaValue::List(vec![ok]);
        }
        assert!(Metadata::new().set("t:deep", ok).is_ok());
    }

    #[test]
    fn spigot_values_are_kept_untouched_and_readable() {
        let nbt = Compound::new()
            .with("plugin:count", Tag::Int(7))
            .with("plugin:name", "steve")
            .with("plugin:ints", Tag::IntArray(vec![1, 2]))
            .with(
                "plugin:nested",
                Compound::new().with("plugin:inner", Tag::Byte(1)),
            )
            .with("marker:kind", MetaValue::Text("a".to_owned()).to_nbt());
        let mut meta = Metadata::from_nbt(&nbt);
        assert_eq!(meta.get("plugin:count"), Some(MetaValue::Int(7)));
        assert_eq!(
            meta.get("plugin:name"),
            Some(MetaValue::Text("steve".to_owned()))
        );
        assert_eq!(meta.get("plugin:ints"), None);
        assert_eq!(meta.to_nbt(), nbt);
        meta.set("marker:other", MetaValue::Bool(true)).unwrap();
        let written = meta.to_nbt();
        assert_eq!(written.get("plugin:ints"), nbt.get("plugin:ints"));
        assert_eq!(written.get("plugin:nested"), nbt.get("plugin:nested"));
        assert_eq!(written.get("plugin:count"), Some(&Tag::Int(7)));
        meta.clear();
        let cleared = meta.to_nbt();
        assert_eq!(cleared.get("plugin:count"), None);
        assert_eq!(cleared.get("plugin:ints"), nbt.get("plugin:ints"));
    }
}
