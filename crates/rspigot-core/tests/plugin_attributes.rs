mod common;

use common::*;
use rspigot_core::plugins::{Answer, EntityOrder, Order, Question};
use rspigot_domain::{EntityId, Vec3};

const MAX_HEALTH: &str = "minecraft:max_health";
const MOVEMENT_SPEED: &str = "minecraft:movement_speed";

fn entity_of(s: &mut Scene) -> EntityId {
    let alice = s.players[0].clone();
    let Answer::Player(state) = s.ask(Question::Player(alice.id)) else {
        panic!("expected the player state");
    };
    EntityId(state.player.entity_id)
}

fn max_health(s: &mut Scene) -> f32 {
    let alice = s.players[0].clone();
    let Answer::Player(state) = s.ask(Question::Player(alice.id)) else {
        panic!("expected the player state");
    };
    state.max_health
}

fn set(s: &mut Scene, entity: EntityId, attribute: &str, value: f64) {
    s.act(Order::Entity {
        entity,
        order: EntityOrder::SetAttribute {
            attribute: attribute.to_owned(),
            value,
        },
    });
}

fn value(s: &mut Scene, entity: EntityId, attribute: &str) -> Answer {
    s.ask(Question::AttributeValue {
        entity,
        attribute: attribute.to_owned(),
    })
}

fn spawn(s: &mut Scene, kind: &str) -> EntityId {
    let at = Vec3::new(4.5, 64.0, 4.5);
    stream_chunk_of(s, at.block());
    let Answer::Spawned(view) = s.ask(Question::Spawn {
        kind: kind.into(),
        position: at,
    }) else {
        panic!("expected the spawned {kind}");
    };
    view.id
}

#[test]
fn max_health_attribute_changes_the_limit() {
    let mut s = scene(&["Alice"]);
    let alice = entity_of(&mut s);
    assert_eq!(max_health(&mut s), 20.0);

    set(&mut s, alice, MAX_HEALTH, 40.0);

    assert_eq!(max_health(&mut s), 40.0);
    assert_eq!(
        value(&mut s, alice, MAX_HEALTH),
        Answer::AttributeValue(40.0)
    );
}

#[test]
fn an_attribute_of_a_mob_changes_its_base_value() {
    let mut s = scene(&["Alice"]);
    let zombie = spawn(&mut s, "minecraft:zombie");

    set(&mut s, zombie, MAX_HEALTH, 100.0);
    set(&mut s, zombie, MOVEMENT_SPEED, 0.5);

    assert_eq!(
        value(&mut s, zombie, MAX_HEALTH),
        Answer::AttributeValue(100.0)
    );
    assert_eq!(
        value(&mut s, zombie, MOVEMENT_SPEED),
        Answer::AttributeValue(0.5)
    );
}

#[test]
fn an_attribute_the_entity_does_not_have_is_refused() {
    let mut s = scene(&["Alice"]);
    let cow = spawn(&mut s, "minecraft:cow");

    assert!(matches!(
        value(&mut s, cow, "minecraft:attack_damage"),
        Answer::Refused(_)
    ));
    set(&mut s, cow, "minecraft:attack_damage", 4.0);
    assert!(
        s.warnings()
            .iter()
            .any(|warning| warning.contains("entity.set_attribute"))
    );
}

#[test]
fn the_attribute_of_a_gone_entity_is_not_found() {
    let mut s = scene(&["Alice"]);

    assert_eq!(
        value(&mut s, EntityId(987_654), MAX_HEALTH),
        Answer::NotFound
    );
}
