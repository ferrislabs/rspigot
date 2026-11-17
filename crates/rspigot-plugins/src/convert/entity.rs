use rspigot_core::plugins::{EntityDetails, EntityOrder, EntityView, Order, Question};
use rspigot_domain::EntityId;
use rspigot_plugin_protocol::v1::{self, entity_order::Order as Wire};
use uuid::Uuid;

use super::Malformed;
use super::shared::{self, required, wire_vec};

pub(super) fn order(order: v1::EntityOrder) -> Result<Order, Malformed> {
    let entity = EntityId(order.entity);
    let order = match required(order.order)? {
        Wire::Teleport(position) => EntityOrder::Teleport(shared::vec3(position)),
        Wire::Remove(_) => EntityOrder::Remove,
        Wire::Damage(amount) => EntityOrder::Damage(amount),
        Wire::Kill(_) => EntityOrder::Kill,
        Wire::AddEffect(effect) => EntityOrder::AddEffect(shared::effect(effect)),
        Wire::RemoveEffect(key) => EntityOrder::RemoveEffect(key),
        Wire::ClearEffects(_) => EntityOrder::ClearEffects,
        Wire::SetVelocity(velocity) => EntityOrder::SetVelocity(shared::vec3(velocity)),
    };
    Ok(Order::Entity { entity, order })
}

pub(super) fn query(query: v1::EntityQuery) -> Result<Question, Malformed> {
    let near = match query.near {
        Some(near) => Some((shared::vec3(required(near.position)?), near.radius)),
        None => None,
    };
    Ok(Question::Entities {
        near,
        kind: query.kind,
    })
}

pub(super) fn spawn(spawn: v1::Spawn) -> Result<Question, Malformed> {
    Ok(Question::Spawn {
        kind: spawn.kind,
        position: shared::vec3(required(spawn.position)?),
    })
}

pub(super) fn drop_item(drop: v1::DropItem) -> Result<Question, Malformed> {
    Ok(Question::DropItem {
        position: shared::vec3(required(drop.position)?),
        item: shared::item(required(drop.item)?),
    })
}

pub(super) fn view(snapshot: v1::EntitySnapshot) -> Result<EntityView, Malformed> {
    Ok(EntityView {
        id: EntityId(snapshot.entity_id),
        uuid: Uuid::parse_str(&snapshot.uuid).map_err(|_| Malformed::Entity(snapshot.uuid))?,
        kind: snapshot.kind,
        position: shared::vec3(required(snapshot.position)?),
        player: snapshot.player.map(super::player::snapshot).transpose()?,
    })
}

pub(super) fn wire_view(view: EntityView) -> v1::EntitySnapshot {
    v1::EntitySnapshot {
        entity_id: view.id.0,
        uuid: view.uuid.hyphenated().to_string(),
        kind: view.kind,
        position: Some(wire_vec(view.position)),
        player: view.player.map(super::player::wire_player),
    }
}

pub(super) fn wire_details(details: EntityDetails) -> v1::EntityState {
    v1::EntityState {
        entity: Some(wire_view(details.entity)),
        health: details.health,
        velocity: Some(wire_vec(details.velocity)),
        on_ground: details.on_ground,
        custom_name: details.custom_name,
        fire_ticks: details.fire_ticks,
    }
}

#[cfg(test)]
mod tests {
    use rspigot_core::plugins::{
        Answer, EntityDetails, EntityOrder, EntityView, FromPlugin, ItemSpec, Order, Question,
        ToPlugin,
    };
    use rspigot_domain::{EntityId, Vec3};
    use rspigot_plugin_protocol::v1::{self, answer, entity_order};
    use uuid::Uuid;

    use crate::convert::{Malformed, from_plugin, question, to_plugin};

    const ZOMBIE: EntityId = EntityId(42);

    fn body(body: from_plugin::Body) -> v1::FromPlugin {
        v1::FromPlugin {
            body: Some(body),
            origin: None,
            request: None,
        }
    }

    fn entity(order: Option<entity_order::Order>) -> Result<FromPlugin, Malformed> {
        from_plugin(
            body(from_plugin::Body::Entity(v1::EntityOrder {
                entity: ZOMBIE.0,
                order,
            })),
            &mut Vec::new(),
        )
    }

    fn ask(asked: question::Question) -> Result<FromPlugin, Malformed> {
        from_plugin(
            body(from_plugin::Body::Question(v1::Question {
                request: 9,
                question: Some(asked),
            })),
            &mut Vec::new(),
        )
    }

    fn asked(question: Question) -> FromPlugin {
        FromPlugin::Question {
            request: 9,
            question,
        }
    }

    fn answered(answer: Answer) -> Option<answer::Body> {
        let message = to_plugin(ToPlugin::Answer { request: 9, answer });
        match message.body {
            Some(to_plugin::Body::Answer(v1::Answer { request: 9, body })) => body,
            other => panic!("expected an answer, got {other:?}"),
        }
    }

    const fn wire_vec3(x: f64, y: f64, z: f64) -> v1::Vec3 {
        v1::Vec3 { x, y, z }
    }

    fn view() -> EntityView {
        EntityView {
            id: ZOMBIE,
            uuid: Uuid::from_u128(7),
            kind: "minecraft:zombie".into(),
            position: Vec3::new(4.5, 64.0, -2.5),
            player: None,
        }
    }

    fn snapshot() -> v1::EntitySnapshot {
        v1::EntitySnapshot {
            entity_id: 42,
            uuid: "00000000-0000-0000-0000-000000000007".into(),
            kind: "minecraft:zombie".into(),
            position: Some(wire_vec3(4.5, 64.0, -2.5)),
            player: None,
        }
    }

    #[test]
    fn a_player_behind_an_entity_survives_both_conversions() {
        let wire = v1::EntitySnapshot {
            kind: "minecraft:player".into(),
            player: Some(v1::PlayerSnapshot {
                permissions: Vec::new(),
                uuid: "00000000-0000-0000-0000-000000000007".into(),
                name: "Steve".into(),
                entity_id: 42,
                world: "minecraft:overworld".into(),
                position: Some(wire_vec3(4.5, 64.0, -2.5)),
                game_mode: v1::GameMode::Survival as i32,
                rotation: Some(v1::Rotation {
                    yaw: 0.0,
                    pitch: 0.0,
                }),
            }),
            ..snapshot()
        };
        let view = super::view(wire.clone()).unwrap();
        assert_eq!(
            view.player.as_ref().map(|player| player.name.as_str()),
            Some("Steve")
        );
        assert_eq!(super::wire_view(view), wire);
    }

    #[test]
    fn entity_orders_questions_and_answers_convert() {
        assert_eq!(
            entity(Some(entity_order::Order::Teleport(wire_vec3(
                1.0, 65.0, 2.0
            ))))
            .unwrap(),
            FromPlugin::Order(Order::Entity {
                entity: ZOMBIE,
                order: EntityOrder::Teleport(Vec3::new(1.0, 65.0, 2.0)),
            })
        );
        assert_eq!(
            entity(Some(entity_order::Order::Damage(3.5))).unwrap(),
            FromPlugin::Order(Order::Entity {
                entity: ZOMBIE,
                order: EntityOrder::Damage(3.5),
            })
        );
        assert!(matches!(entity(None), Err(Malformed::Empty)));

        assert_eq!(
            ask(question::Question::Entities(v1::EntityQuery {
                near: Some(v1::Near {
                    position: Some(wire_vec3(4.5, 64.0, 4.5)),
                    radius: 2.0,
                }),
                kind: None,
            }))
            .unwrap(),
            asked(Question::Entities {
                near: Some((Vec3::new(4.5, 64.0, 4.5), 2.0)),
                kind: None,
            })
        );
        assert_eq!(
            ask(question::Question::Entities(v1::EntityQuery {
                near: None,
                kind: Some("minecraft:cow".into()),
            }))
            .unwrap(),
            asked(Question::Entities {
                near: None,
                kind: Some("minecraft:cow".into()),
            })
        );
        assert!(matches!(
            ask(question::Question::Entities(v1::EntityQuery {
                near: Some(v1::Near {
                    position: None,
                    radius: 2.0,
                }),
                kind: None,
            })),
            Err(Malformed::Empty)
        ));
        assert_eq!(
            ask(question::Question::Entity(42)).unwrap(),
            asked(Question::Entity(ZOMBIE))
        );
        assert_eq!(
            ask(question::Question::Spawn(v1::Spawn {
                kind: "minecraft:zombie".into(),
                position: Some(wire_vec3(4.5, 64.0, 4.5)),
            }))
            .unwrap(),
            asked(Question::Spawn {
                kind: "minecraft:zombie".into(),
                position: Vec3::new(4.5, 64.0, 4.5),
            })
        );
        assert_eq!(
            ask(question::Question::DropItem(v1::DropItem {
                position: Some(wire_vec3(2.5, 65.0, 2.5)),
                item: Some(v1::Item {
                    key: "minecraft:diamond".into(),
                    count: 3,
                    ..v1::Item::default()
                }),
            }))
            .unwrap(),
            asked(Question::DropItem {
                position: Vec3::new(2.5, 65.0, 2.5),
                item: ItemSpec::of("minecraft:diamond", 3),
            })
        );
        assert!(matches!(
            ask(question::Question::DropItem(v1::DropItem {
                position: Some(wire_vec3(2.5, 65.0, 2.5)),
                item: None,
            })),
            Err(Malformed::Empty)
        ));

        assert_eq!(
            answered(Answer::Spawned(view())),
            Some(answer::Body::Spawned(snapshot()))
        );
        assert_eq!(
            answered(Answer::Entities(vec![view()])),
            Some(answer::Body::Entities(v1::EntityList {
                entities: vec![snapshot()],
            }))
        );
        assert_eq!(
            answered(Answer::Refused("Unable to summon entity".into())),
            Some(answer::Body::Refused(v1::Refused {
                reason: "Unable to summon entity".into(),
            }))
        );
        assert_eq!(
            answered(Answer::Entity(EntityDetails {
                entity: view(),
                health: None,
                velocity: Vec3::new(0.0, -0.08, 0.0),
                on_ground: true,
                custom_name: Some("§aBob".into()),
                fire_ticks: -1,
            })),
            Some(answer::Body::Entity(v1::EntityState {
                entity: Some(snapshot()),
                health: None,
                velocity: Some(wire_vec3(0.0, -0.08, 0.0)),
                on_ground: true,
                custom_name: Some("§aBob".into()),
                fire_ticks: -1,
            }))
        );
    }
}
