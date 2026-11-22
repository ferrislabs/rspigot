mod common;

use std::collections::BTreeMap;

use common::*;
use rspigot_core::plugins::{
    Answer, BlockOrder, EntityOrder, ItemSpec, ObjectAnswer, ObjectOrder, ObjectQuestion, Order,
    PdcChange, PdcHolder, PdcValue, PlayerObjectOrder, PlayerOrder, Question,
};
use rspigot_domain::{BlockPos, EntityId, Vec3};

fn settled() -> Scene {
    let mut s = scene(&["Alice"]);
    s.tick(60);
    s
}

fn cow(s: &mut Scene) -> EntityId {
    let Answer::Spawned(view) = s.ask(Question::Spawn {
        kind: "minecraft:cow".into(),
        position: Vec3::new(3.5, 64.0, 3.5),
    }) else {
        panic!("expected the spawned cow");
    };
    view.id
}

fn set(key: &str, value: PdcValue) -> PdcChange {
    PdcChange::Set {
        key: key.to_owned(),
        value,
    }
}

fn entries(s: &mut Scene, holder: PdcHolder) -> BTreeMap<String, PdcValue> {
    match s.ask(Question::Object(ObjectQuestion::Pdc(holder))) {
        Answer::Object(ObjectAnswer::Pdc(entries)) => entries,
        other => panic!("expected a container, got {other:?}"),
    }
}

fn on_entity(s: &mut Scene, entity: EntityId, change: PdcChange) {
    s.act(Order::Entity {
        entity,
        order: EntityOrder::Object(ObjectOrder::Pdc(change)),
    });
}

fn nested() -> PdcValue {
    PdcValue::Container(BTreeMap::from([
        ("name".to_owned(), PdcValue::Text("x".to_owned())),
        (
            "scores".to_owned(),
            PdcValue::List(vec![PdcValue::Long(1), PdcValue::Long(2)]),
        ),
        ("flag".to_owned(), PdcValue::Byte(1)),
    ]))
}

#[test]
fn an_entity_keeps_every_typed_value_under_namespaced_keys() {
    let mut s = settled();
    let cow = cow(&mut s);
    let values = [
        ("plugin:text", PdcValue::Text("moo".to_owned())),
        ("plugin:byte", PdcValue::Byte(-4)),
        ("plugin:int", PdcValue::Int(123_456)),
        ("plugin:long", PdcValue::Long(i64::MAX - 1)),
        ("plugin:double", PdcValue::Double(2.5)),
        ("plugin:bytes", PdcValue::Bytes(vec![9, 8, 7])),
        ("plugin:nested", nested()),
    ];

    for (key, value) in &values {
        on_entity(&mut s, cow, set(key, value.clone()));
    }

    assert!(s.warnings().is_empty());
    let held = entries(&mut s, PdcHolder::Entity(cow));
    assert_eq!(held.len(), values.len());
    for (key, value) in values {
        assert_eq!(held.get(key), Some(&value), "{key}");
    }
}

#[test]
fn removing_a_key_and_clearing_a_container_empties_it() {
    let mut s = settled();
    let cow = cow(&mut s);
    on_entity(&mut s, cow, set("plugin:a", PdcValue::Int(1)));
    on_entity(&mut s, cow, set("plugin:b", PdcValue::Int(2)));

    on_entity(&mut s, cow, PdcChange::Remove("plugin:a".to_owned()));
    let held = entries(&mut s, PdcHolder::Entity(cow));
    assert_eq!(held.keys().collect::<Vec<_>>(), ["plugin:b"]);

    on_entity(&mut s, cow, PdcChange::Clear);
    assert!(entries(&mut s, PdcHolder::Entity(cow)).is_empty());
}

#[test]
fn an_unnamespaced_key_a_mixed_list_or_a_gone_entity_is_refused() {
    let mut s = settled();
    let cow = cow(&mut s);

    on_entity(&mut s, cow, set("level", PdcValue::Int(1)));
    on_entity(
        &mut s,
        cow,
        set(
            "plugin:mixed",
            PdcValue::List(vec![PdcValue::Int(1), PdcValue::Text("a".to_owned())]),
        ),
    );
    on_entity(&mut s, EntityId(999_999), set("plugin:a", PdcValue::Int(1)));

    assert_eq!(s.warnings().len(), 3);
    assert!(entries(&mut s, PdcHolder::Entity(cow)).is_empty());
    assert!(matches!(
        s.ask(Question::Object(ObjectQuestion::Pdc(PdcHolder::Entity(
            EntityId(999_999)
        )))),
        Answer::Failed(_)
    ));
}

#[test]
fn a_block_keeps_typed_values() {
    let mut s = settled();
    let pos = BlockPos::new(2, 63, 2);

    s.act(Order::Block {
        pos,
        order: BlockOrder::Pdc(set("plugin:owner", PdcValue::Text("alice".to_owned()))),
    });
    s.act(Order::Block {
        pos,
        order: BlockOrder::Pdc(set("plugin:uses", PdcValue::Long(3))),
    });

    assert!(s.warnings().is_empty());
    let held = entries(&mut s, PdcHolder::Block(pos));
    assert_eq!(
        held.get("plugin:owner"),
        Some(&PdcValue::Text("alice".to_owned()))
    );
    assert_eq!(held.get("plugin:uses"), Some(&PdcValue::Long(3)));
    s.act(Order::Block {
        pos,
        order: BlockOrder::Pdc(PdcChange::Clear),
    });
    assert!(entries(&mut s, PdcHolder::Block(pos)).is_empty());
}

#[test]
fn an_item_in_a_slot_keeps_typed_values_in_its_custom_data() {
    let mut s = settled();
    let alice = s.players[0].clone();
    s.act(to(
        &alice,
        PlayerOrder::Object(PlayerObjectOrder::ItemPdc {
            slot: 0,
            change: set("plugin:tag", PdcValue::Int(1)),
        }),
    ));
    assert_eq!(s.warnings().len(), 1);

    s.act(Order::Inventory {
        player: alice.id,
        order: rspigot_core::plugins::InventoryOrder::Set {
            slot: 0,
            item: Some(ItemSpec::of("minecraft:stick", 1)),
        },
    });
    for (key, value) in [
        ("plugin:level", PdcValue::Int(7)),
        ("plugin:nested", nested()),
    ] {
        s.act(to(
            &alice,
            PlayerOrder::Object(PlayerObjectOrder::ItemPdc {
                slot: 0,
                change: set(key, value),
            }),
        ));
    }

    assert_eq!(s.warnings().len(), 1);
    let held = entries(
        &mut s,
        PdcHolder::Item {
            player: alice.id,
            slot: 0,
        },
    );
    assert_eq!(held.get("plugin:level"), Some(&PdcValue::Int(7)));
    assert_eq!(held.get("plugin:nested"), Some(&nested()));
}

#[test]
fn an_item_value_survives_leaving_and_joining_again() {
    let (mut s, _saves) = saving_scene(&["Alice"], Vec::new());
    let alice = s.players[0].clone();
    s.act(Order::Inventory {
        player: alice.id,
        order: rspigot_core::plugins::InventoryOrder::Set {
            slot: 0,
            item: Some(ItemSpec::of("minecraft:stick", 1)),
        },
    });
    s.act(to(
        &alice,
        PlayerOrder::Object(PlayerObjectOrder::ItemPdc {
            slot: 0,
            change: set("plugin:kept", PdcValue::Long(42)),
        }),
    ));

    s.server.handle(leave_of(&alice));
    s.server.deliver();
    let again = arrive(&mut s.server, "Alice");
    settle_arrival(&mut s.server, &again);

    let held = entries(
        &mut s,
        PdcHolder::Item {
            player: again.id,
            slot: 0,
        },
    );
    assert_eq!(held.get("plugin:kept"), Some(&PdcValue::Long(42)));
}
