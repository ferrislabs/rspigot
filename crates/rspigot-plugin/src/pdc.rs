use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use rspigot_plugin_protocol::v1::{
    self, answer, block_order, entity_order, from_plugin, object_answer, object_entity_order,
    object_player_order, pdc_change, pdc_query, pdc_value, player_order, question,
};

use crate::link::Link;
use crate::{BlockPos, Error};

const MAX_KEY_LENGTH: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NamespacedKey(String);

fn namespace_ok(part: &str, slash: bool) -> bool {
    !part.is_empty()
        && part.bytes().all(|byte| {
            matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.')
                || (slash && byte == b'/')
        })
}

impl NamespacedKey {
    pub fn new(namespace: &str, key: &str) -> Result<Self, Error> {
        let joined = format!("{namespace}:{key}");
        if joined.len() > MAX_KEY_LENGTH
            || !namespace_ok(namespace, false)
            || !namespace_ok(key, true)
        {
            return Err(Error::Rejected(format!(
                "`{joined}` is not a namespaced key of lowercase letters, digits and `_-./`"
            )));
        }
        Ok(Self(joined))
    }

    pub fn parse(key: &str) -> Result<Self, Error> {
        let (namespace, path) = key
            .split_once(':')
            .ok_or_else(|| Error::Rejected(format!("`{key}` has no namespace")))?;
        Self::new(namespace, path)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NamespacedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PdcValue {
    Text(String),
    Byte(i8),
    Int(i32),
    Long(i64),
    Double(f64),
    Bytes(Vec<u8>),
    Container(BTreeMap<String, PdcValue>),
    List(Vec<PdcValue>),
}

impl PdcValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }

    pub const fn as_i8(&self) -> Option<i8> {
        match self {
            Self::Byte(value) => Some(*value),
            _ => None,
        }
    }

    pub const fn as_i32(&self) -> Option<i32> {
        match self {
            Self::Int(value) => Some(*value),
            _ => None,
        }
    }

    pub const fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Long(value) => Some(*value),
            _ => None,
        }
    }

    pub const fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Double(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Bytes(bytes) => Some(bytes),
            _ => None,
        }
    }

    pub const fn as_container(&self) -> Option<&BTreeMap<String, PdcValue>> {
        match self {
            Self::Container(entries) => Some(entries),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[PdcValue]> {
        match self {
            Self::List(items) => Some(items),
            _ => None,
        }
    }

    pub(crate) fn to_wire(&self) -> v1::PdcValue {
        use pdc_value::Value;
        v1::PdcValue {
            value: Some(match self {
                Self::Text(text) => Value::Text(text.clone()),
                Self::Byte(value) => Value::Byte(i32::from(*value)),
                Self::Int(value) => Value::Int(*value),
                Self::Long(value) => Value::Long(*value),
                Self::Double(value) => Value::Double(*value),
                Self::Bytes(bytes) => Value::ByteArray(bytes.clone()),
                Self::Container(entries) => Value::Container(container_to_wire(entries)),
                Self::List(items) => Value::List(v1::PdcList {
                    items: items.iter().map(Self::to_wire).collect(),
                }),
            }),
        }
    }

    pub(crate) fn from_wire(value: v1::PdcValue) -> Result<Self, Error> {
        use pdc_value::Value;
        let value = value
            .value
            .ok_or_else(|| Error::Malformed("an empty value".to_owned()))?;
        Ok(match value {
            Value::Text(text) => Self::Text(text),
            Value::Byte(byte) => Self::Byte(
                i8::try_from(byte).map_err(|_| Error::Malformed(format!("a byte of {byte}")))?,
            ),
            Value::Int(value) => Self::Int(value),
            Value::Long(value) => Self::Long(value),
            Value::Double(value) => Self::Double(value),
            Value::ByteArray(bytes) => Self::Bytes(bytes),
            Value::Container(container) => Self::Container(container_from_wire(container)?),
            Value::List(list) => Self::List(
                list.items
                    .into_iter()
                    .map(Self::from_wire)
                    .collect::<Result<_, _>>()?,
            ),
        })
    }
}

fn container_to_wire(entries: &BTreeMap<String, PdcValue>) -> v1::PdcContainer {
    v1::PdcContainer {
        entries: entries
            .iter()
            .map(|(key, value)| (key.clone(), value.to_wire()))
            .collect(),
    }
}

fn container_from_wire(container: v1::PdcContainer) -> Result<BTreeMap<String, PdcValue>, Error> {
    container
        .entries
        .into_iter()
        .map(|(key, value)| Ok((key, PdcValue::from_wire(value)?)))
        .collect()
}

impl From<&str> for PdcValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

impl From<String> for PdcValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<i8> for PdcValue {
    fn from(value: i8) -> Self {
        Self::Byte(value)
    }
}

impl From<bool> for PdcValue {
    fn from(value: bool) -> Self {
        Self::Byte(i8::from(value))
    }
}

impl From<i32> for PdcValue {
    fn from(value: i32) -> Self {
        Self::Int(value)
    }
}

impl From<i64> for PdcValue {
    fn from(value: i64) -> Self {
        Self::Long(value)
    }
}

impl From<f64> for PdcValue {
    fn from(value: f64) -> Self {
        Self::Double(value)
    }
}

impl From<Vec<u8>> for PdcValue {
    fn from(value: Vec<u8>) -> Self {
        Self::Bytes(value)
    }
}

impl From<BTreeMap<String, PdcValue>> for PdcValue {
    fn from(value: BTreeMap<String, PdcValue>) -> Self {
        Self::Container(value)
    }
}

#[derive(Debug, Clone)]
enum Holder {
    Entity(i32),
    Block(BlockPos),
    Item { player: Arc<str>, slot: i32 },
}

#[derive(Debug, Clone)]
pub struct PdcHandle {
    holder: Holder,
    link: Link,
}

impl PdcHandle {
    pub(crate) fn of_entity(entity: i32, link: Link) -> Self {
        Self {
            holder: Holder::Entity(entity),
            link,
        }
    }

    pub(crate) fn of_block(pos: BlockPos, link: Link) -> Self {
        Self {
            holder: Holder::Block(pos),
            link,
        }
    }

    pub(crate) fn of_item(player: Arc<str>, slot: i32, link: Link) -> Self {
        Self {
            holder: Holder::Item { player, slot },
            link,
        }
    }

    fn change(&self, change: pdc_change::Change) {
        let change = v1::PdcChange {
            change: Some(change),
        };
        let body = match &self.holder {
            Holder::Entity(entity) => from_plugin::Body::Entity(v1::EntityOrder {
                entity: *entity,
                order: Some(entity_order::Order::Object(v1::ObjectEntityOrder {
                    order: Some(object_entity_order::Order::Pdc(change)),
                })),
            }),
            Holder::Block(pos) => from_plugin::Body::Block(v1::BlockOrder {
                pos: Some(pos.to_wire()),
                order: Some(block_order::Order::Pdc(change)),
            }),
            Holder::Item { player, slot } => from_plugin::Body::Player(v1::PlayerOrder {
                player: player.to_string(),
                order: Some(player_order::Order::Object(v1::ObjectPlayerOrder {
                    order: Some(object_player_order::Order::ItemPdc(v1::ItemPdc {
                        slot: *slot,
                        change: Some(change),
                    })),
                })),
            }),
        };
        self.link.order(body);
    }

    pub fn set(&self, key: &NamespacedKey, value: impl Into<PdcValue>) {
        self.change(pdc_change::Change::Set(v1::PdcSet {
            key: key.as_str().to_owned(),
            value: Some(value.into().to_wire()),
        }));
    }

    pub fn remove(&self, key: &NamespacedKey) {
        self.change(pdc_change::Change::Remove(key.as_str().to_owned()));
    }

    pub fn clear(&self) {
        self.change(pdc_change::Change::Clear(v1::Empty {}));
    }

    pub async fn entries(&self) -> Result<BTreeMap<String, PdcValue>, Error> {
        let holder = match &self.holder {
            Holder::Entity(entity) => pdc_query::Holder::Entity(*entity),
            Holder::Block(pos) => pdc_query::Holder::Block(pos.to_wire()),
            Holder::Item { player, slot } => pdc_query::Holder::Item(v1::ItemSlotRef {
                player: player.to_string(),
                slot: *slot,
            }),
        };
        let container = self
            .link
            .ask(
                question::Question::Object(v1::ObjectQuestion {
                    query: Some(v1::object_question::Query::Pdc(v1::PdcQuery {
                        holder: Some(holder),
                    })),
                }),
                "a container",
                |body| match body {
                    answer::Body::Object(v1::ObjectAnswer {
                        answer: Some(object_answer::Answer::Pdc(container)),
                    }) => Some(container),
                    _ => None,
                },
            )
            .await?;
        container_from_wire(container)
    }

    pub async fn get(&self, key: &NamespacedKey) -> Result<Option<PdcValue>, Error> {
        Ok(self.entries().await?.remove(key.as_str()))
    }

    pub async fn has(&self, key: &NamespacedKey) -> Result<bool, Error> {
        Ok(self.entries().await?.contains_key(key.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_checked_when_they_are_made() {
        assert_eq!(
            NamespacedKey::new("plugin", "level/max").unwrap().as_str(),
            "plugin:level/max"
        );
        assert_eq!(NamespacedKey::parse("a:b").unwrap().to_string(), "a:b");
        assert!(NamespacedKey::new("Plugin", "level").is_err());
        assert!(NamespacedKey::new("plugin", "").is_err());
        assert!(NamespacedKey::new("plugin", "a b").is_err());
        assert!(NamespacedKey::parse("nonamespace").is_err());
    }

    #[test]
    fn values_survive_the_wire() {
        let nested = PdcValue::Container(BTreeMap::from([
            ("a".to_owned(), PdcValue::Byte(-1)),
            (
                "b".to_owned(),
                PdcValue::List(vec![PdcValue::Long(1), PdcValue::Long(2)]),
            ),
        ]));
        assert_eq!(PdcValue::from_wire(nested.to_wire()).unwrap(), nested);
        assert_eq!(PdcValue::from(true), PdcValue::Byte(1));
        assert!(PdcValue::from_wire(v1::PdcValue { value: None }).is_err());
    }
}
