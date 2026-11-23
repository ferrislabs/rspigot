use rspigot_core::plugins::{EntityMarks, EntityOrder, EntityTrait, EntityView, Order, Question};
use rspigot_domain::EntityId;
use rspigot_plugin_protocol::v1::{self, entity_order::Order as Wire, typed_change::Change};
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
        Wire::SetAttribute(set) => EntityOrder::SetAttribute {
            attribute: attribute_key(set.attribute)?,
            value: set.value,
        },
        Wire::Change(change) => EntityOrder::Change(typed(required(change.change)?)),
        Wire::Meta(order) => EntityOrder::Meta(super::meta::order(order)?),
        Wire::Edit(edit) => EntityOrder::Edit(super::edits::edit(edit)?),
        Wire::Object(object) => EntityOrder::Object(super::objects::entity_order(object)?),
        Wire::SetLootTable(table) => EntityOrder::SetLootTable {
            table: table.table,
            seed: table.seed,
        },
    };
    Ok(Order::Entity { entity, order })
}

fn attribute_key(attribute: i32) -> Result<String, Malformed> {
    match v1::Attribute::try_from(attribute) {
        Ok(v1::Attribute::Unspecified) | Err(_) => Err(Malformed::Enum {
            field: "attribute",
            value: attribute,
        }),
        Ok(known) => {
            let name = known.as_str_name();
            Ok(format!(
                "minecraft:{}",
                name.strip_prefix("ATTRIBUTE_")
                    .unwrap_or(name)
                    .to_lowercase()
            ))
        }
    }
}

pub(super) fn attribute_query(query: v1::AttributeQuery) -> Result<Question, Malformed> {
    Ok(Question::AttributeValue {
        entity: EntityId(query.entity),
        attribute: attribute_key(query.attribute)?,
    })
}

const fn typed(change: Change) -> EntityTrait {
    match change {
        Change::ZombieBaby(baby) => EntityTrait::ZombieBaby(baby),
        Change::CreeperPowered(powered) => EntityTrait::CreeperPowered(powered),
        Change::SheepColor(color) => EntityTrait::SheepColor(color),
        Change::SheepSheared(sheared) => EntityTrait::SheepSheared(sheared),
        Change::SlimeSize(size) => EntityTrait::SlimeSize(size),
        Change::VillagerProfession(profession) => EntityTrait::VillagerProfession(profession),
        Change::WolfCollar(color) => EntityTrait::WolfCollar(color),
        Change::CatCollar(color) => EntityTrait::CatCollar(color),
        Change::RabbitVariant(variant) => EntityTrait::RabbitVariant(variant),
    }
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
    let position = shared::vec3(required(spawn.position)?);
    if spawn.block.is_none() && spawn.velocity.is_none() {
        return Ok(Question::Spawn {
            kind: spawn.kind,
            position,
        });
    }
    Ok(Question::SpawnWith {
        kind: spawn.kind,
        position,
        block: spawn.block,
        velocity: spawn.velocity.map(shared::vec3),
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
        meta: super::meta::snapshot(snapshot.meta, snapshot.volatile_meta)?,
        marks: EntityMarks::new(snapshot.custom_name, snapshot.tags),
    })
}

pub(super) fn wire_view(view: EntityView) -> v1::EntitySnapshot {
    v1::EntitySnapshot {
        entity_id: view.id.0,
        uuid: view.uuid.hyphenated().to_string(),
        kind: view.kind,
        position: Some(wire_vec(view.position)),
        player: view.player.map(super::player::wire_player),
        meta: super::meta::wire_map(view.meta.persistent()),
        volatile_meta: super::meta::wire_map(view.meta.volatile()),
        custom_name: view.marks.custom_name().map(str::to_owned),
        tags: view.marks.tags().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use rspigot_core::plugins::{
        Answer, EntityDetails, EntityFlags, EntityOrder, EntityTrait, EntityView, FromPlugin,
        ItemSpec, Order, Question, ToPlugin,
    };
    use rspigot_domain::{EntityId, Rotation, Vec3};
    use rspigot_plugin_protocol::v1::{self, answer, entity_order};
    use uuid::Uuid;

    use crate::convert::{Malformed, from_plugin, question, to_plugin};

    const ZOMBIE: EntityId = EntityId(42);

    fn body(body: from_plugin::Body) -> v1::FromPlugin {
        v1::FromPlugin {
            body: Some(body),
            origin: None,
            request: None,
            in_world: None,
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
            meta: Default::default(),
            id: ZOMBIE,
            uuid: Uuid::from_u128(7),
            kind: "minecraft:zombie".into(),
            position: Vec3::new(4.5, 64.0, -2.5),
            player: None,
            marks: Default::default(),
        }
    }

    fn details_rest() -> EntityDetails {
        EntityDetails {
            entity: view(),
            health: None,
            velocity: Vec3::ZERO,
            on_ground: false,
            custom_name: None,
            fire_ticks: 0,
            flags: EntityFlags {
                gravity: true,
                ..Default::default()
            },
            freeze_ticks: 0,
            air: 0,
            fall_distance: 0.0,
            rotation: Rotation::default(),
            ticks_lived: 0,
            tags: Vec::new(),
            passengers: Vec::new(),
            vehicle: None,
            living: None,
        }
    }

    fn snapshot() -> v1::EntitySnapshot {
        v1::EntitySnapshot {
            entity_id: 42,
            uuid: "00000000-0000-0000-0000-000000000007".into(),
            kind: "minecraft:zombie".into(),
            position: Some(wire_vec3(4.5, 64.0, -2.5)),
            player: None,
            ..Default::default()
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
                ..Default::default()
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
                ..v1::Spawn::default()
            }))
            .unwrap(),
            asked(Question::Spawn {
                kind: "minecraft:zombie".into(),
                position: Vec3::new(4.5, 64.0, 4.5),
            })
        );
        assert_eq!(
            ask(question::Question::Spawn(v1::Spawn {
                kind: "minecraft:falling_block".into(),
                position: Some(wire_vec3(1.5, 70.0, 2.5)),
                block: Some("minecraft:stone".into()),
                velocity: Some(wire_vec3(0.5, 1.0, -0.5)),
            }))
            .unwrap(),
            asked(Question::SpawnWith {
                kind: "minecraft:falling_block".into(),
                position: Vec3::new(1.5, 70.0, 2.5),
                block: Some("minecraft:stone".into()),
                velocity: Some(Vec3::new(0.5, 1.0, -0.5)),
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
                ..details_rest()
            })),
            Some(answer::Body::Entity(v1::EntityState {
                entity: Some(snapshot()),
                health: None,
                velocity: Some(wire_vec3(0.0, -0.08, 0.0)),
                on_ground: true,
                custom_name: Some("§aBob".into()),
                fire_ticks: -1,
                gravity: true,
                rotation: Some(v1::Rotation {
                    yaw: 0.0,
                    pitch: 0.0,
                }),
                ..Default::default()
            }))
        );
    }

    #[test]
    fn attributes_and_typed_changes_convert() {
        assert_eq!(
            entity(Some(entity_order::Order::SetAttribute(v1::SetAttribute {
                attribute: v1::Attribute::MaxHealth as i32,
                value: 40.0,
            })))
            .unwrap(),
            FromPlugin::Order(Order::Entity {
                entity: ZOMBIE,
                order: EntityOrder::SetAttribute {
                    attribute: "minecraft:max_health".into(),
                    value: 40.0,
                },
            })
        );
        assert!(matches!(
            entity(Some(entity_order::Order::SetAttribute(v1::SetAttribute {
                attribute: 0,
                value: 1.0,
            }))),
            Err(Malformed::Enum { .. })
        ));
        assert_eq!(
            entity(Some(entity_order::Order::Change(v1::TypedChange {
                change: Some(v1::typed_change::Change::ZombieBaby(true)),
            })))
            .unwrap(),
            FromPlugin::Order(Order::Entity {
                entity: ZOMBIE,
                order: EntityOrder::Change(EntityTrait::ZombieBaby(true)),
            })
        );
        assert!(matches!(
            entity(Some(entity_order::Order::Change(v1::TypedChange {
                change: None,
            }))),
            Err(Malformed::Empty)
        ));
        assert_eq!(
            ask(question::Question::AttributeValue(v1::AttributeQuery {
                entity: 42,
                attribute: v1::Attribute::MovementSpeed as i32,
            }))
            .unwrap(),
            asked(Question::AttributeValue {
                entity: ZOMBIE,
                attribute: "minecraft:movement_speed".into(),
            })
        );
        assert_eq!(
            answered(Answer::AttributeValue(0.25)),
            Some(answer::Body::AttributeValue(0.25))
        );
    }

    #[test]
    fn entity_edits_convert_and_the_living_part_survives_the_answer() {
        use rspigot_core::plugins::{EntityEdit, EquipmentView, LivingDetails};

        let edit = |edit| {
            entity(Some(entity_order::Order::Edit(v1::EntityEdit {
                edit: Some(edit),
            })))
        };
        let expected = |edit| {
            FromPlugin::Order(Order::Entity {
                entity: ZOMBIE,
                order: EntityOrder::Edit(edit),
            })
        };
        assert_eq!(
            edit(v1::entity_edit::Edit::SetCustomName("Bob".into())).unwrap(),
            expected(EntityEdit::SetCustomName("Bob".into()))
        );
        assert_eq!(
            edit(v1::entity_edit::Edit::SetEquipment(v1::EquipmentEdit {
                slot: "head".into(),
                item: Some(v1::Item {
                    key: "minecraft:iron_helmet".into(),
                    count: 1,
                    ..Default::default()
                }),
            }))
            .unwrap(),
            expected(EntityEdit::SetEquipment {
                slot: "head".into(),
                item: Some(ItemSpec::of("minecraft:iron_helmet", 1)),
            })
        );
        assert_eq!(
            edit(v1::entity_edit::Edit::Mount(7)).unwrap(),
            expected(EntityEdit::Mount(EntityId(7)))
        );
        assert_eq!(
            edit(v1::entity_edit::Edit::SetRotation(v1::Rotation {
                yaw: 90.0,
                pitch: 5.0,
            }))
            .unwrap(),
            expected(EntityEdit::SetRotation {
                yaw: 90.0,
                pitch: 5.0,
            })
        );
        assert!(matches!(
            edit(v1::entity_edit::Edit::SetOwner("nope".into())),
            Err(Malformed::Uuid(_))
        ));
        assert!(matches!(
            entity(Some(entity_order::Order::Edit(v1::EntityEdit {
                edit: None
            }))),
            Err(Malformed::Empty)
        ));

        let owner = Uuid::from_u128(9);
        let Some(answer::Body::Entity(state)) = answered(Answer::Entity(EntityDetails {
            tags: vec!["boss".into()],
            passengers: vec![EntityId(3)],
            vehicle: Some(EntityId(4)),
            living: Some(Box::new(LivingDetails {
                max_health: 20.0,
                absorption: 1.0,
                ai: false,
                can_pick_up_loot: true,
                left_handed: false,
                equipment: vec![EquipmentView {
                    slot: "head",
                    item: Some(ItemSpec::of("minecraft:iron_helmet", 1)),
                    drop_chance: 0.5,
                }],
                leash_holder: Some(EntityId(5)),
                owner: Some(owner),
                tamed: true,
                sitting: true,
                target: Some(EntityId(6)),
                age: -5,
                age_locked: true,
                baby: true,
                in_love: false,
            })),
            ..details_rest()
        })) else {
            panic!("expected an entity state");
        };
        assert_eq!(state.passengers, vec![3]);
        assert_eq!(state.vehicle, Some(4));
        let living = state.living.expect("a living part");
        assert!(!living.ai && living.tamed && living.sitting && living.baby);
        assert_eq!(
            living.owner.as_deref(),
            Some("00000000-0000-0000-0000-000000000009")
        );
        assert_eq!(living.leash_holder, Some(5));
        assert_eq!(living.target, Some(6));
        assert_eq!(living.equipment[0].slot, "head");
        assert_eq!(living.equipment[0].drop_chance, 0.5);
    }

    #[test]
    fn a_snapshot_keeps_its_name_and_tags_both_ways() {
        let wire = v1::EntitySnapshot {
            custom_name: Some("Bob".into()),
            tags: vec!["boss".into()],
            ..snapshot()
        };
        let view = super::view(wire.clone()).unwrap();
        assert_eq!(view.marks.custom_name(), Some("Bob"));
        assert_eq!(super::wire_view(view), wire);
    }
}
