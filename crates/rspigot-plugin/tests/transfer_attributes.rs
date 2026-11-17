mod fake_host;

use fake_host::*;
use rspigot_plugin::{
    ActionErrorKind, Attribute, DyeColor, EntityType, Error, Profession, RabbitVariant,
};
use rspigot_plugin_protocol::v1::{
    self, answer, entity_order, from_plugin, player_order, question, typed_change,
};
use uuid::Uuid;

fn transferred(host: &str, port: u32) -> from_plugin::Body {
    from_plugin::Body::Player(v1::PlayerOrder {
        player: STEVE.to_owned(),
        order: Some(player_order::Order::Transfer(v1::Transfer {
            host: host.to_owned(),
            port,
        })),
    })
}

fn snapshot(kind: &str) -> v1::EntitySnapshot {
    v1::EntitySnapshot {
        entity_id: 30,
        uuid: Uuid::from_u128(30).to_string(),
        kind: kind.to_owned(),
        position: Some(v1::Vec3 {
            x: 0.0,
            y: 64.0,
            z: 0.0,
        }),
        player: None,
    }
}

fn entity_order(order: entity_order::Order) -> from_plugin::Body {
    from_plugin::Body::Entity(v1::EntityOrder {
        entity: 30,
        order: Some(order),
    })
}

fn change(change: typed_change::Change) -> from_plugin::Body {
    entity_order(entity_order::Order::Change(v1::TypedChange {
        change: Some(change),
    }))
}

async fn listing(called: &mut Called, kind: &str) {
    let (request, _) = called.session.asked().await;
    called.session.say(answer(
        request,
        answer::Body::Entities(v1::EntityList {
            entities: vec![snapshot(kind)],
        }),
    ));
}

#[tokio::test]
async fn transfer_sends_the_host_and_the_port_and_reports_the_result() {
    let mut called = calling(|player| {
        Box::pin(async move {
            player
                .connection
                .transfer("play.example.org", 25570)
                .await
                .unwrap();
            let refused = player.connection.transfer("", 25565).await;
            let Err(Error::Action(error)) = refused else {
                panic!("expected a refused transfer, got {refused:?}");
            };
            assert_eq!(error.kind, ActionErrorKind::InvalidArgument);
        })
    })
    .await;

    called
        .session
        .performed(transferred("play.example.org", 25570))
        .await;
    let (request, body) = called.session.acted().await;
    assert_eq!(body, transferred("", 25565));
    called.session.say(answer(
        request,
        answer::Body::Failed(v1::ActionError {
            kind: v1::ActionErrorKind::InvalidArgument as i32,
            reason: "the host is empty".to_owned(),
        }),
    ));
    called.finished().await;
}

#[tokio::test]
async fn attributes_set_and_get_value_use_the_entity_and_the_attribute() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let zombie = player
                .world
                .entities
                .of(EntityType::Zombie)
                .get_entities()
                .await
                .unwrap()
                .remove(0);
            zombie.attributes.set(Attribute::MaxHealth, 40.0);
            let speed = zombie
                .attributes
                .get_value(Attribute::MovementSpeed)
                .await
                .unwrap();
            assert_eq!(speed, 0.23);
        })
    })
    .await;

    listing(&mut called, "minecraft:zombie").await;
    assert_eq!(
        called.session.hear().await,
        entity_order(entity_order::Order::SetAttribute(v1::SetAttribute {
            attribute: v1::Attribute::MaxHealth as i32,
            value: 40.0,
        }))
    );
    let (request, asked) = called.session.asked().await;
    assert_eq!(
        asked,
        question::Question::AttributeValue(v1::AttributeQuery {
            entity: 30,
            attribute: v1::Attribute::MovementSpeed as i32,
        })
    );
    called
        .session
        .say(answer(request, answer::Body::AttributeValue(0.23)));
    called.finished().await;
}

#[tokio::test]
async fn as_zombie_is_none_for_a_creeper() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let entities = player.world.entities.get_entities().await.unwrap();
            assert!(entities[0].as_zombie().is_none());
            assert!(entities[0].as_creeper().is_some());
            assert!(entities[1].as_creeper().is_none());
            let zombie = entities[1].as_zombie().unwrap();
            zombie.set_baby(true);
        })
    })
    .await;

    let (request, _) = called.session.asked().await;
    let mut zombie = snapshot("minecraft:zombie");
    zombie.entity_id = 31;
    called.session.say(answer(
        request,
        answer::Body::Entities(v1::EntityList {
            entities: vec![snapshot("minecraft:creeper"), zombie],
        }),
    ));
    assert_eq!(
        called.session.hear().await,
        from_plugin::Body::Entity(v1::EntityOrder {
            entity: 31,
            order: Some(entity_order::Order::Change(v1::TypedChange {
                change: Some(typed_change::Change::ZombieBaby(true)),
            })),
        })
    );
    called.finished().await;
}

#[tokio::test]
async fn typed_entities_send_their_own_changes() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let entities = player.world.entities.get_entities().await.unwrap();
            for entity in &entities {
                match entity.kind {
                    EntityType::Creeper => entity.as_creeper().unwrap().set_powered(true),
                    EntityType::Sheep => {
                        let sheep = entity.as_sheep().unwrap();
                        sheep.set_color(DyeColor::Red);
                        sheep.set_sheared(true);
                    }
                    EntityType::Slime => entity.as_slime().unwrap().set_size(3),
                    EntityType::Villager => entity
                        .as_villager()
                        .unwrap()
                        .set_profession(Profession::Farmer),
                    EntityType::Wolf => {
                        entity.as_wolf().unwrap().set_collar_color(DyeColor::Blue);
                    }
                    EntityType::Cat => entity.as_cat().unwrap().set_collar_color(DyeColor::Lime),
                    EntityType::Rabbit => {
                        entity.as_rabbit().unwrap().set_variant(RabbitVariant::Gold)
                    }
                    other => panic!("unexpected {other:?}"),
                }
            }
        })
    })
    .await;

    let kinds = [
        "creeper", "sheep", "slime", "villager", "wolf", "cat", "rabbit",
    ];
    let (request, _) = called.session.asked().await;
    called.session.say(answer(
        request,
        answer::Body::Entities(v1::EntityList {
            entities: kinds
                .iter()
                .map(|kind| snapshot(&format!("minecraft:{kind}")))
                .collect(),
        }),
    ));
    for expected in [
        typed_change::Change::CreeperPowered(true),
        typed_change::Change::SheepColor(14),
        typed_change::Change::SheepSheared(true),
        typed_change::Change::SlimeSize(3),
        typed_change::Change::VillagerProfession(5),
        typed_change::Change::WolfCollar(11),
        typed_change::Change::CatCollar(5),
        typed_change::Change::RabbitVariant(4),
    ] {
        assert_eq!(called.session.hear().await, change(expected));
    }
    called.finished().await;
}
