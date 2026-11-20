use std::collections::BTreeMap;

use rspigot_core::plugins::{
    BlockMetaView, MetaChange, MetaHolder, MetaOrder, MetaSnapshot, Question,
};
use rspigot_domain::meta::MetaValue;
use rspigot_domain::{BlockPos, EntityId};
use rspigot_plugin_protocol::v1::{self, meta_order, meta_query, meta_value};

use super::Malformed;
use super::shared::{self, required};

pub(super) fn value(wire: v1::MetaValue) -> Result<MetaValue, Malformed> {
    Ok(match required(wire.value)? {
        meta_value::Value::Flag(flag) => MetaValue::Bool(flag),
        meta_value::Value::Number(number) => MetaValue::Int(number),
        meta_value::Value::Decimal(number) => MetaValue::Double(number),
        meta_value::Value::Text(text) => MetaValue::Text(text),
        meta_value::Value::Data(bytes) => MetaValue::Bytes(bytes),
        meta_value::Value::List(list) => MetaValue::List(
            list.items
                .into_iter()
                .map(value)
                .collect::<Result<_, _>>()?,
        ),
        meta_value::Value::Map(map) => MetaValue::Map(
            map.entries
                .into_iter()
                .map(|(key, held)| Ok((key, value(held)?)))
                .collect::<Result<BTreeMap<_, _>, Malformed>>()?,
        ),
    })
}

pub(super) fn wire_value(value: MetaValue) -> v1::MetaValue {
    v1::MetaValue {
        value: Some(match value {
            MetaValue::Bool(flag) => meta_value::Value::Flag(flag),
            MetaValue::Int(number) => meta_value::Value::Number(number),
            MetaValue::Double(number) => meta_value::Value::Decimal(number),
            MetaValue::Text(text) => meta_value::Value::Text(text),
            MetaValue::Bytes(bytes) => meta_value::Value::Data(bytes),
            MetaValue::List(items) => meta_value::Value::List(v1::MetaList {
                items: items.into_iter().map(wire_value).collect(),
            }),
            MetaValue::Map(entries) => meta_value::Value::Map(v1::MetaMap {
                entries: entries
                    .into_iter()
                    .map(|(key, held)| (key, wire_value(held)))
                    .collect(),
            }),
        }),
    }
}

pub(super) fn wire_map(
    entries: &[(String, MetaValue)],
) -> std::collections::HashMap<String, v1::MetaValue> {
    entries
        .iter()
        .map(|(key, held)| (key.clone(), wire_value(held.clone())))
        .collect()
}

pub(super) fn wire_entries(entries: Vec<(String, MetaValue)>) -> v1::MetaEntries {
    v1::MetaEntries {
        entries: entries
            .into_iter()
            .map(|(key, held)| v1::MetaEntry {
                key,
                value: Some(wire_value(held)),
            })
            .collect(),
    }
}

pub(super) fn order(order: v1::MetaOrder) -> Result<MetaOrder, Malformed> {
    let change = match required(order.change)? {
        meta_order::Change::Set(entry) => MetaChange::Set {
            key: entry.key,
            value: value(required(entry.value)?)?,
        },
        meta_order::Change::Remove(key) => MetaChange::Remove(key),
        meta_order::Change::Clear(_) => MetaChange::Clear,
    };
    Ok(MetaOrder {
        volatile: order.volatile,
        change,
    })
}

pub(super) fn query(query: v1::MetaQuery) -> Result<Question, Malformed> {
    let holder = match required(query.holder)? {
        meta_query::Holder::Entity(id) => MetaHolder::Entity(EntityId(id)),
        meta_query::Holder::Player(id) => MetaHolder::Player(super::player::id(id)?),
        meta_query::Holder::Block(pos) => MetaHolder::Block(shared::block_pos(pos)),
    };
    Ok(Question::Meta {
        holder,
        key: query.key,
        volatile: query.volatile,
    })
}

pub(super) fn find_blocks(find: v1::FindBlocks) -> Result<Question, Malformed> {
    Ok(Question::FindBlocks {
        key: find.key,
        value: find.value.map(value).transpose()?,
        volatile: find.volatile,
    })
}

pub(super) fn snapshot(
    meta: std::collections::HashMap<String, v1::MetaValue>,
    volatile: std::collections::HashMap<String, v1::MetaValue>,
) -> Result<MetaSnapshot, Malformed> {
    let convert = |map: std::collections::HashMap<String, v1::MetaValue>| {
        map.into_iter()
            .map(|(key, held)| Ok((key, value(held)?)))
            .collect::<Result<Vec<_>, Malformed>>()
    };
    Ok(MetaSnapshot::new(convert(meta)?, convert(volatile)?))
}

pub(super) fn wire_block(view: BlockMetaView) -> v1::BlockMeta {
    v1::BlockMeta {
        pos: Some(shared::wire_block_pos(view.pos)),
        meta: wire_map(view.meta.persistent()),
        volatile_meta: wire_map(view.meta.volatile()),
    }
}

pub(super) fn wire_positions(positions: Vec<BlockPos>) -> v1::BlockPositions {
    v1::BlockPositions {
        positions: positions.into_iter().map(shared::wire_block_pos).collect(),
    }
}

#[cfg(test)]
mod tests {
    use rspigot_core::plugins::PlayerSnapshot;
    use rspigot_core::plugins::{
        Answer, BlockMetaView, BlockOrder, EntityOrder, EventPayload, FromPlugin, MetaChange,
        MetaOrder, MetaSnapshot, Order, ToPlugin,
    };
    use rspigot_domain::player::{GameMode, PlayerId};
    use rspigot_domain::{BlockPos, Rotation, Vec3};
    use rspigot_plugin_protocol::v1::{
        self, answer, block_order, entity_order, from_plugin, to_plugin,
    };
    use uuid::Uuid;

    use super::*;
    use crate::convert::{from_plugin as decode, to_plugin as encode};

    fn nested() -> MetaValue {
        MetaValue::Map(BTreeMap::from([(
            "k".to_owned(),
            MetaValue::List(vec![
                MetaValue::Bool(true),
                MetaValue::Int(2),
                MetaValue::Double(0.5),
                MetaValue::Text("t".to_owned()),
                MetaValue::Bytes(vec![9]),
            ]),
        )]))
    }

    #[test]
    fn a_value_round_trips_through_the_wire() {
        assert_eq!(value(wire_value(nested())).unwrap(), nested());
        assert!(value(v1::MetaValue { value: None }).is_err());
    }

    #[test]
    fn entity_and_block_meta_orders_decode() {
        let order = v1::MetaOrder {
            volatile: true,
            change: Some(meta_order::Change::Set(v1::MetaEntry {
                key: "a:b".to_owned(),
                value: Some(wire_value(MetaValue::Int(1))),
            })),
        };
        let entity = decode(
            v1::FromPlugin {
                body: Some(from_plugin::Body::Entity(v1::EntityOrder {
                    entity: 5,
                    order: Some(entity_order::Order::Meta(order.clone())),
                })),
                origin: None,
                request: None,
                in_world: None,
            },
            &mut Vec::new(),
        )
        .unwrap();
        let expected = MetaOrder {
            volatile: true,
            change: MetaChange::Set {
                key: "a:b".to_owned(),
                value: MetaValue::Int(1),
            },
        };
        assert_eq!(
            entity,
            FromPlugin::Order(Order::Entity {
                entity: EntityId(5),
                order: EntityOrder::Meta(expected.clone()),
            })
        );
        let block = decode(
            v1::FromPlugin {
                body: Some(from_plugin::Body::Block(v1::BlockOrder {
                    pos: Some(v1::BlockPos { x: 1, y: 2, z: 3 }),
                    order: Some(block_order::Order::Meta(order)),
                })),
                origin: None,
                request: None,
                in_world: None,
            },
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(
            block,
            FromPlugin::Order(Order::Block {
                pos: BlockPos::new(1, 2, 3),
                order: BlockOrder::Meta(expected),
            })
        );
    }

    #[test]
    fn an_event_carries_the_meta_of_its_blocks_and_answers_encode() {
        let message = encode(ToPlugin::Event {
            id: 1,
            handlers: vec![1],
            phase: rspigot_core::plugins::Phase::Settled,
            cancelled: false,
            payload: EventPayload::BlockBreak(rspigot_core::plugins::events::BlockBreak {
                block: BlockPos::new(1, 2, 3),
                drop_items: true,
                exp_to_drop: 0,
                player: PlayerSnapshot {
                    meta: Default::default(),
                    permissions: Vec::new(),
                    id: PlayerId(Uuid::from_u128(1)),
                    name: "Steve".to_owned(),
                    entity_id: 12,
                    world: "minecraft:overworld".to_owned(),
                    position: Vec3::new(1.0, 64.0, -3.5),
                    rotation: Rotation::new(0.0, 0.0),
                    game_mode: GameMode::Survival,
                },
            }),
            blocks: vec![BlockMetaView {
                pos: BlockPos::new(1, 2, 3),
                meta: MetaSnapshot::new(vec![("a:b".to_owned(), MetaValue::Int(1))], Vec::new()),
            }],
        });
        let Some(to_plugin::Body::Event(event)) = message.body else {
            panic!("expected an event");
        };
        assert_eq!(event.block_meta.len(), 1);
        assert_eq!(
            event.block_meta[0].pos,
            Some(v1::BlockPos { x: 1, y: 2, z: 3 })
        );
        assert!(event.block_meta[0].meta.contains_key("a:b"));

        let answered = encode(ToPlugin::Answer {
            request: 4,
            answer: Answer::MetaEntries(vec![("a:b".to_owned(), MetaValue::Int(1))]),
        });
        let Some(to_plugin::Body::Answer(v1::Answer {
            body: Some(answer::Body::MetaEntries(entries)),
            ..
        })) = answered.body
        else {
            panic!("expected entries");
        };
        assert_eq!(entries.entries.len(), 1);
    }

    #[test]
    fn a_player_meta_order_and_query_decode() {
        let uuid = Uuid::from_u128(1).hyphenated().to_string();
        let decoded = decode(
            v1::FromPlugin {
                body: Some(from_plugin::Body::Player(v1::PlayerOrder {
                    player: uuid.clone(),
                    order: Some(v1::player_order::Order::Meta(v1::MetaOrder {
                        volatile: false,
                        change: Some(meta_order::Change::Clear(v1::Empty {})),
                    })),
                })),
                origin: None,
                request: None,
                in_world: None,
            },
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(
            decoded,
            FromPlugin::Order(Order::Player {
                player: PlayerId(Uuid::from_u128(1)),
                order: rspigot_core::plugins::PlayerOrder::Meta(MetaOrder {
                    volatile: false,
                    change: MetaChange::Clear,
                }),
            })
        );
        let asked = query(v1::MetaQuery {
            holder: Some(meta_query::Holder::Player(uuid)),
            key: None,
            volatile: true,
        })
        .unwrap();
        assert_eq!(
            asked,
            Question::Meta {
                holder: MetaHolder::Player(PlayerId(Uuid::from_u128(1))),
                key: None,
                volatile: true,
            }
        );
    }
}
