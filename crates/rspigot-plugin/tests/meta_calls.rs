mod fake_host;

use std::collections::{BTreeMap, HashMap};

use fake_host::*;
use rspigot_plugin::{ActionErrorKind, BlockPos, Error, MetaValue};
use rspigot_plugin_protocol::v1::{
    self, answer, block_order, entity_order, from_plugin, meta_order, meta_query, meta_value,
    question,
};
use uuid::Uuid;

fn text(value: &str) -> v1::MetaValue {
    v1::MetaValue {
        value: Some(meta_value::Value::Text(value.to_owned())),
    }
}

fn number(value: i64) -> v1::MetaValue {
    v1::MetaValue {
        value: Some(meta_value::Value::Number(value)),
    }
}

fn snapshot(meta: HashMap<String, v1::MetaValue>) -> v1::EntitySnapshot {
    v1::EntitySnapshot {
        entity_id: 30,
        uuid: Uuid::from_u128(30).to_string(),
        kind: "minecraft:zombie".to_owned(),
        position: Some(v1::Vec3 {
            x: 0.0,
            y: 64.0,
            z: 0.0,
        }),
        meta,
        volatile_meta: HashMap::from([("t:tmp".to_owned(), number(4))]),
        ..Default::default()
    }
}

fn entity_meta(volatile: bool, change: meta_order::Change) -> from_plugin::Body {
    from_plugin::Body::Entity(v1::EntityOrder {
        entity: 30,
        order: Some(entity_order::Order::Meta(v1::MetaOrder {
            volatile,
            change: Some(change),
        })),
    })
}

#[test]
fn a_value_converts_from_the_common_types_and_reads_back() {
    assert_eq!(MetaValue::from(true).as_bool(), Some(true));
    assert_eq!(MetaValue::from(7_i32).as_i64(), Some(7));
    assert_eq!(MetaValue::from(7_i64).as_i64(), Some(7));
    assert_eq!(MetaValue::from(1.5).as_f64(), Some(1.5));
    assert_eq!(MetaValue::from("a").as_str(), Some("a"));
    assert_eq!(MetaValue::from(String::from("b")).as_str(), Some("b"));
    assert_eq!(
        MetaValue::from(vec![1_u8, 2]).as_bytes(),
        Some(&[1_u8, 2][..])
    );
    let list = MetaValue::from(vec![MetaValue::from(1)]);
    assert_eq!(list.as_list().map(<[MetaValue]>::len), Some(1));
    let map = MetaValue::from(BTreeMap::from([("k".to_owned(), MetaValue::from(true))]));
    assert_eq!(map.as_map().map(BTreeMap::len), Some(1));
    assert_eq!(MetaValue::from(1).as_bool(), None);
}

#[tokio::test]
async fn an_entity_handle_reads_its_snapshot_and_sends_meta_orders() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let zombie = player
                .world
                .entities
                .get_entities()
                .await
                .unwrap()
                .remove(0);
            assert_eq!(zombie.meta.get_str("marker:kind"), Some("boss"));
            assert!(zombie.meta.has("marker:kind"));
            assert!(!zombie.meta.has("marker:other"));
            assert_eq!(zombie.meta.keys().collect::<Vec<_>>(), ["marker:kind"]);
            assert_eq!(zombie.meta.iter().count(), 1);
            assert_eq!(zombie.volatile_meta.get_i64("t:tmp"), Some(4));
            zombie.meta.set("marker:level", 3);
            zombie.meta.remove("marker:kind");
            zombie.volatile_meta.clear();
            let found = zombie.meta.get_value("marker:level").await.unwrap();
            assert_eq!(found, Some(MetaValue::Int(3)));
            let refused = zombie.meta.try_set("marker:x", 1).await;
            let Err(Error::Action(error)) = refused else {
                panic!("expected a refused set, got {refused:?}");
            };
            assert_eq!(error.kind, ActionErrorKind::LimitReached);
        })
    })
    .await;

    let (request, _) = called.session.asked().await;
    called.session.say(answer(
        request,
        answer::Body::Entities(v1::EntityList {
            entities: vec![snapshot(HashMap::from([(
                "marker:kind".to_owned(),
                text("boss"),
            )]))],
        }),
    ));
    assert_eq!(
        called.session.hear().await,
        entity_meta(
            false,
            meta_order::Change::Set(v1::MetaEntry {
                key: "marker:level".to_owned(),
                value: Some(number(3)),
            })
        )
    );
    assert_eq!(
        called.session.hear().await,
        entity_meta(false, meta_order::Change::Remove("marker:kind".to_owned()))
    );
    assert_eq!(
        called.session.hear().await,
        entity_meta(true, meta_order::Change::Clear(v1::Empty {}))
    );
    let (request, asked) = called.session.asked().await;
    assert_eq!(
        asked,
        question::Question::Meta(v1::MetaQuery {
            holder: Some(meta_query::Holder::Entity(30)),
            key: Some("marker:level".to_owned()),
            volatile: false,
        })
    );
    called.session.say(answer(
        request,
        answer::Body::MetaEntries(v1::MetaEntries {
            entries: vec![v1::MetaEntry {
                key: "marker:level".to_owned(),
                value: Some(number(3)),
            }],
        }),
    ));
    let (request, body) = called.session.acted().await;
    assert_eq!(
        body,
        entity_meta(
            false,
            meta_order::Change::Set(v1::MetaEntry {
                key: "marker:x".to_owned(),
                value: Some(number(1)),
            })
        )
    );
    called.session.say(answer(
        request,
        answer::Body::Failed(v1::ActionError {
            kind: v1::ActionErrorKind::LimitReached as i32,
            reason: "a holder takes at most 64 keys".to_owned(),
        }),
    ));
    called.finished().await;
}

#[tokio::test]
async fn a_block_handle_sends_meta_orders_to_its_position_and_finds_blocks() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let block = player.world.block(BlockPos { x: 1, y: 64, z: -2 });
            assert!(block.meta.is_empty());
            block.meta.set("marker:kind", "wall");
            block.volatile_meta.remove("t:tmp");
            let found = player
                .world
                .meta
                .find_blocks("marker:kind", Some(MetaValue::from("wall")))
                .await
                .unwrap();
            assert_eq!(found, [BlockPos { x: 1, y: 64, z: -2 }]);
        })
    })
    .await;

    let pos = v1::BlockPos { x: 1, y: 64, z: -2 };
    let at = Some(pos);
    assert_eq!(
        called.session.hear().await,
        from_plugin::Body::Block(v1::BlockOrder {
            pos: at,
            order: Some(block_order::Order::Meta(v1::MetaOrder {
                volatile: false,
                change: Some(meta_order::Change::Set(v1::MetaEntry {
                    key: "marker:kind".to_owned(),
                    value: Some(text("wall")),
                })),
            })),
        })
    );
    assert_eq!(
        called.session.hear().await,
        from_plugin::Body::Block(v1::BlockOrder {
            pos: at,
            order: Some(block_order::Order::Meta(v1::MetaOrder {
                volatile: true,
                change: Some(meta_order::Change::Remove("t:tmp".to_owned())),
            })),
        })
    );
    let (request, asked) = called.session.asked().await;
    assert_eq!(
        asked,
        question::Question::FindBlocks(v1::FindBlocks {
            key: "marker:kind".to_owned(),
            value: Some(text("wall")),
            volatile: false,
        })
    );
    called.session.say(answer(
        request,
        answer::Body::BlockPositions(v1::BlockPositions {
            positions: vec![pos],
        }),
    ));
    called.finished().await;
}

#[tokio::test]
async fn a_player_handle_reads_its_snapshot_and_sends_meta_orders_to_its_uuid() {
    let mut called = calling(|player| {
        Box::pin(async move {
            player.meta.set("marker:rank", "vip");
            player.volatile_meta.set("t:tmp", 1);
            let found = player.meta.get_value("marker:rank").await.unwrap();
            assert_eq!(found, Some(MetaValue::from("vip")));
            let listed = player
                .server()
                .get_players_with_meta("marker:rank")
                .await
                .unwrap();
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].meta.get_str("marker:rank"), Some("vip"));
        })
    })
    .await;

    let player_meta = |volatile, change| {
        from_plugin::Body::Player(v1::PlayerOrder {
            player: STEVE.to_owned(),
            order: Some(v1::player_order::Order::Meta(v1::MetaOrder {
                volatile,
                change: Some(change),
            })),
        })
    };
    assert_eq!(
        called.session.hear().await,
        player_meta(
            false,
            meta_order::Change::Set(v1::MetaEntry {
                key: "marker:rank".to_owned(),
                value: Some(text("vip")),
            })
        )
    );
    assert_eq!(
        called.session.hear().await,
        player_meta(
            true,
            meta_order::Change::Set(v1::MetaEntry {
                key: "t:tmp".to_owned(),
                value: Some(number(1)),
            })
        )
    );
    let (request, asked) = called.session.asked().await;
    assert_eq!(
        asked,
        question::Question::Meta(v1::MetaQuery {
            holder: Some(meta_query::Holder::Player(STEVE.to_owned())),
            key: Some("marker:rank".to_owned()),
            volatile: false,
        })
    );
    called.session.say(answer(
        request,
        answer::Body::MetaEntries(v1::MetaEntries {
            entries: vec![v1::MetaEntry {
                key: "marker:rank".to_owned(),
                value: Some(text("vip")),
            }],
        }),
    ));
    let (request, asked) = called.session.asked().await;
    assert_eq!(
        asked,
        question::Question::PlayersWithMeta(v1::EntitiesWithMeta {
            key: "marker:rank".to_owned(),
            volatile: false,
        })
    );
    let mut tagged = steve();
    tagged.meta = HashMap::from([("marker:rank".to_owned(), text("vip"))]);
    called.session.say(answer(
        request,
        answer::Body::Players(v1::PlayerList {
            players: vec![tagged],
        }),
    ));
    called.finished().await;
}
