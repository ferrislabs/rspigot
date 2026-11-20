use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;

use rspigot_plugin_protocol::v1::{
    self, answer, block_order, entity_order, from_plugin, meta_order, meta_query, meta_value,
    player_order, question,
};

use crate::Error;
use crate::block::BlockPos;
use crate::link::Link;

#[derive(Debug, Clone, PartialEq)]
pub enum MetaValue {
    Bool(bool),
    Int(i64),
    Double(f64),
    String(String),
    Bytes(Vec<u8>),
    List(Vec<MetaValue>),
    Map(BTreeMap<String, MetaValue>),
}

impl MetaValue {
    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(flag) => Some(*flag),
            _ => None,
        }
    }

    pub const fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int(number) => Some(*number),
            _ => None,
        }
    }

    pub const fn as_f64(&self) -> Option<f64> {
        match self {
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

    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Bytes(bytes) => Some(bytes),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[MetaValue]> {
        match self {
            Self::List(items) => Some(items),
            _ => None,
        }
    }

    pub const fn as_map(&self) -> Option<&BTreeMap<String, MetaValue>> {
        match self {
            Self::Map(entries) => Some(entries),
            _ => None,
        }
    }

    pub(crate) fn from_wire(wire: v1::MetaValue) -> Result<Self, Error> {
        let value = wire
            .value
            .ok_or_else(|| Error::Malformed("a metadata value without a type".to_owned()))?;
        Ok(match value {
            meta_value::Value::Flag(flag) => Self::Bool(flag),
            meta_value::Value::Number(number) => Self::Int(number),
            meta_value::Value::Decimal(number) => Self::Double(number),
            meta_value::Value::Text(text) => Self::String(text),
            meta_value::Value::Data(bytes) => Self::Bytes(bytes),
            meta_value::Value::List(list) => Self::List(
                list.items
                    .into_iter()
                    .map(Self::from_wire)
                    .collect::<Result<_, _>>()?,
            ),
            meta_value::Value::Map(map) => Self::Map(
                map.entries
                    .into_iter()
                    .map(|(key, held)| Ok((key, Self::from_wire(held)?)))
                    .collect::<Result<_, Error>>()?,
            ),
        })
    }

    pub(crate) fn to_wire(&self) -> v1::MetaValue {
        v1::MetaValue {
            value: Some(match self {
                Self::Bool(flag) => meta_value::Value::Flag(*flag),
                Self::Int(number) => meta_value::Value::Number(*number),
                Self::Double(number) => meta_value::Value::Decimal(*number),
                Self::String(text) => meta_value::Value::Text(text.clone()),
                Self::Bytes(bytes) => meta_value::Value::Data(bytes.clone()),
                Self::List(items) => meta_value::Value::List(v1::MetaList {
                    items: items.iter().map(Self::to_wire).collect(),
                }),
                Self::Map(entries) => meta_value::Value::Map(v1::MetaMap {
                    entries: entries
                        .iter()
                        .map(|(key, held)| (key.clone(), held.to_wire()))
                        .collect(),
                }),
            }),
        }
    }
}

impl From<bool> for MetaValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i64> for MetaValue {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<i32> for MetaValue {
    fn from(value: i32) -> Self {
        Self::Int(i64::from(value))
    }
}

impl From<f64> for MetaValue {
    fn from(value: f64) -> Self {
        Self::Double(value)
    }
}

impl From<&str> for MetaValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<String> for MetaValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<Vec<u8>> for MetaValue {
    fn from(value: Vec<u8>) -> Self {
        Self::Bytes(value)
    }
}

impl From<Vec<MetaValue>> for MetaValue {
    fn from(value: Vec<MetaValue>) -> Self {
        Self::List(value)
    }
}

impl From<BTreeMap<String, MetaValue>> for MetaValue {
    fn from(value: BTreeMap<String, MetaValue>) -> Self {
        Self::Map(value)
    }
}

type Entries = Arc<BTreeMap<String, MetaValue>>;

pub(crate) fn snapshot(
    wire: std::collections::HashMap<String, v1::MetaValue>,
) -> Result<Entries, Error> {
    wire.into_iter()
        .map(|(key, held)| Ok((key, MetaValue::from_wire(held)?)))
        .collect::<Result<BTreeMap<_, _>, Error>>()
        .map(Arc::new)
}

#[derive(Debug, Clone, PartialEq)]
enum Holder {
    Player(Arc<str>),
    Entity(i32),
    Block(BlockPos),
}

#[derive(Debug, Clone)]
pub struct MetaHandle {
    holder: Holder,
    volatile: bool,
    entries: Entries,
    link: Link,
}

impl MetaHandle {
    pub(crate) fn of_player(
        player: Arc<str>,
        volatile: bool,
        entries: Entries,
        link: Link,
    ) -> Self {
        Self {
            holder: Holder::Player(player),
            volatile,
            entries,
            link,
        }
    }

    pub(crate) fn of_entity(entity: i32, volatile: bool, entries: Entries, link: Link) -> Self {
        Self {
            holder: Holder::Entity(entity),
            volatile,
            entries,
            link,
        }
    }

    pub(crate) fn of_block(pos: BlockPos, volatile: bool, entries: Entries, link: Link) -> Self {
        Self {
            holder: Holder::Block(pos),
            volatile,
            entries,
            link,
        }
    }

    pub fn get(&self, key: &str) -> Option<&MetaValue> {
        self.entries.get(key)
    }

    pub fn has(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &MetaValue)> {
        self.entries
            .iter()
            .map(|(key, value)| (key.as_str(), value))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(MetaValue::as_bool)
    }

    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(MetaValue::as_i64)
    }

    pub fn get_f64(&self, key: &str) -> Option<f64> {
        self.get(key).and_then(MetaValue::as_f64)
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(MetaValue::as_str)
    }

    pub fn get_bytes(&self, key: &str) -> Option<&[u8]> {
        self.get(key).and_then(MetaValue::as_bytes)
    }

    pub fn set(&self, key: &str, value: impl Into<MetaValue>) {
        self.order(meta_order::Change::Set(v1::MetaEntry {
            key: key.to_owned(),
            value: Some(value.into().to_wire()),
        }));
    }

    pub fn remove(&self, key: &str) {
        self.order(meta_order::Change::Remove(key.to_owned()));
    }

    pub fn clear(&self) {
        self.order(meta_order::Change::Clear(v1::Empty {}));
    }

    pub async fn try_set(&self, key: &str, value: impl Into<MetaValue>) -> Result<(), Error> {
        self.link
            .act(self.body(meta_order::Change::Set(v1::MetaEntry {
                key: key.to_owned(),
                value: Some(value.into().to_wire()),
            })))
            .await
    }

    pub async fn get_value(&self, key: &str) -> Result<Option<MetaValue>, Error> {
        Ok(self
            .ask(Some(key.to_owned()))
            .await?
            .into_iter()
            .next()
            .map(|(_, value)| value))
    }

    pub async fn get_all(&self) -> Result<Vec<(String, MetaValue)>, Error> {
        self.ask(None).await
    }

    async fn ask(&self, key: Option<String>) -> Result<Vec<(String, MetaValue)>, Error> {
        let holder = match &self.holder {
            Holder::Player(player) => meta_query::Holder::Player(player.to_string()),
            Holder::Entity(id) => meta_query::Holder::Entity(*id),
            Holder::Block(pos) => meta_query::Holder::Block(pos.to_wire()),
        };
        let entries = self
            .link
            .ask(
                question::Question::Meta(v1::MetaQuery {
                    holder: Some(holder),
                    key,
                    volatile: self.volatile,
                }),
                "metadata entries",
                |body| match body {
                    answer::Body::MetaEntries(entries) => Some(entries),
                    _ => None,
                },
            )
            .await?;
        entries
            .entries
            .into_iter()
            .map(|entry| {
                let value = entry.value.ok_or_else(|| {
                    Error::Malformed("a metadata entry without a value".to_owned())
                })?;
                Ok((entry.key, MetaValue::from_wire(value)?))
            })
            .collect()
    }

    fn change(&self, change: meta_order::Change) -> v1::MetaOrder {
        v1::MetaOrder {
            volatile: self.volatile,
            change: Some(change),
        }
    }

    fn body(&self, change: meta_order::Change) -> from_plugin::Body {
        let order = self.change(change);
        match &self.holder {
            Holder::Player(player) => from_plugin::Body::Player(v1::PlayerOrder {
                player: player.to_string(),
                order: Some(player_order::Order::Meta(order)),
            }),
            Holder::Entity(entity) => from_plugin::Body::Entity(v1::EntityOrder {
                entity: *entity,
                order: Some(entity_order::Order::Meta(order)),
            }),
            Holder::Block(pos) => from_plugin::Body::Block(v1::BlockOrder {
                pos: Some(pos.to_wire()),
                order: Some(block_order::Order::Meta(order)),
            }),
        }
    }

    fn order(&self, change: meta_order::Change) {
        self.link.order(self.body(change));
    }
}

#[derive(Debug, Clone)]
pub struct WorldMetaHandle {
    volatile: bool,
    link: Link,
}

impl WorldMetaHandle {
    pub(crate) const fn new(volatile: bool, link: Link) -> Self {
        Self { volatile, link }
    }

    pub async fn find_blocks(
        &self,
        key: &str,
        value: Option<MetaValue>,
    ) -> Result<Vec<BlockPos>, Error> {
        let found = self
            .link
            .ask(
                question::Question::FindBlocks(v1::FindBlocks {
                    key: key.to_owned(),
                    value: value.as_ref().map(MetaValue::to_wire),
                    volatile: self.volatile,
                }),
                "block positions",
                |body| match body {
                    answer::Body::BlockPositions(found) => Some(found),
                    _ => None,
                },
            )
            .await?;
        found
            .positions
            .into_iter()
            .map(|pos| BlockPos::from_wire(Some(pos), "positions"))
            .collect()
    }
}

thread_local! {
    static BLOCKS: RefCell<Option<Arc<Vec<v1::BlockMeta>>>> = const { RefCell::new(None) };
}

pub(crate) fn scoped<R>(blocks: &Arc<Vec<v1::BlockMeta>>, run: impl FnOnce() -> R) -> R {
    let previous = BLOCKS.with(|cell| cell.replace(Some(Arc::clone(blocks))));
    let result = run();
    BLOCKS.with(|cell| cell.replace(previous));
    result
}

pub(crate) fn block_entries(pos: BlockPos) -> (Entries, Entries) {
    BLOCKS.with(|cell| {
        let held = cell.borrow();
        let Some(blocks) = held.as_ref() else {
            return (Entries::default(), Entries::default());
        };
        let Some(found) = blocks.iter().find(|candidate| {
            candidate
                .pos
                .is_some_and(|at| at.x == pos.x && at.y == pos.y && at.z == pos.z)
        }) else {
            return (Entries::default(), Entries::default());
        };
        (
            snapshot(found.meta.clone()).unwrap_or_default(),
            snapshot(found.volatile_meta.clone()).unwrap_or_default(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Arc<Vec<v1::BlockMeta>> {
        Arc::new(vec![v1::BlockMeta {
            pos: Some(v1::BlockPos { x: 1, y: 2, z: 3 }),
            meta: std::collections::HashMap::from([(
                "a:b".to_owned(),
                v1::MetaValue {
                    value: Some(meta_value::Value::Flag(true)),
                },
            )]),
            volatile_meta: std::collections::HashMap::new(),
        }])
    }

    #[test]
    fn a_block_in_the_event_table_carries_its_meta_and_others_none() {
        let link = Link::default();
        let (tagged, plain) = scoped(&table(), || {
            (
                crate::wire::block(v1::BlockPos { x: 1, y: 2, z: 3 }, &link),
                crate::wire::block(v1::BlockPos { x: 9, y: 2, z: 3 }, &link),
            )
        });
        assert_eq!(tagged.meta.get_bool("a:b"), Some(true));
        assert!(plain.meta.is_empty());
        let outside = crate::wire::block(v1::BlockPos { x: 1, y: 2, z: 3 }, &link);
        assert!(outside.meta.is_empty());
    }

    #[test]
    fn every_value_type_survives_the_wire() {
        let nested = MetaValue::Map(BTreeMap::from([(
            "k".to_owned(),
            MetaValue::List(vec![MetaValue::Bytes(vec![1]), MetaValue::Double(0.5)]),
        )]));
        for value in [
            MetaValue::Bool(false),
            MetaValue::Int(-1),
            MetaValue::Double(2.0),
            MetaValue::String("s".to_owned()),
            nested,
        ] {
            assert_eq!(MetaValue::from_wire(value.to_wire()).unwrap(), value);
        }
        assert!(MetaValue::from_wire(v1::MetaValue { value: None }).is_err());
    }
}
