mod fake_host;

use fake_host::*;
use rspigot_plugin::{Dimension, Generator, Layer, World, WorldOptions};
use rspigot_plugin_protocol::v1::{self, answer, from_plugin, question, server_order, world_order};

fn options() -> WorldOptions {
    WorldOptions {
        name: "arena".to_owned(),
        dimension: Dimension::Nether,
        seed: 7,
        generator: Generator::Flat(vec![Layer::new("minecraft:stone", 3)]),
        structures: false,
    }
}

fn arena_state() -> v1::WorldState {
    v1::WorldState {
        name: "arena".to_owned(),
        uuid: None,
        seed: 7,
        day_time: 0,
        game_time: 0,
        raining: false,
        thundering: false,
        difficulty: v1::Difficulty::Easy as i32,
        spawn: Some(v1::BlockPos { x: 0, y: 4, z: 0 }),
        pvp: true,
        rules: Vec::new(),
    }
}

#[tokio::test]
async fn a_created_world_carries_its_id_on_every_call() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let arena = player.world.create(options()).await.unwrap();
            assert_eq!(arena.id(), "rspigot:arena");
            arena.pvp.set(false);
        })
    })
    .await;
    let message = called.session.hear_message().await;
    assert_eq!(message.in_world, None);
    let Some(from_plugin::Body::Server(v1::ServerOrder {
        order: Some(server_order::Order::CreateWorld(create)),
    })) = message.body
    else {
        panic!("expected a world creation");
    };
    assert_eq!(create.name, "arena");
    assert_eq!(create.dimension, v1::WorldDimension::Nether as i32);
    assert_eq!(create.seed, 7);
    assert!(!create.structures);
    assert_eq!(
        create.generator,
        Some(v1::WorldGenerator {
            kind: Some(v1::world_generator::Kind::Flat(v1::FlatLayers {
                layers: vec![v1::WorldLayer {
                    block: "minecraft:stone".to_owned(),
                    height: 3
                }]
            }))
        })
    );
    called.session.say(answer(
        message.request.expect("a result is expected"),
        answer::Body::WorldCreated("rspigot:arena".to_owned()),
    ));
    let pvp = called.session.hear_message().await;
    assert_eq!(pvp.in_world.as_deref(), Some("rspigot:arena"));
    assert_eq!(
        pvp.body,
        Some(from_plugin::Body::World(v1::WorldOrder {
            order: Some(world_order::Order::SetPvp(false)),
        }))
    );
    called.finished().await;
}

#[tokio::test]
async fn the_default_world_names_no_world() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let world: World = player.world.clone();
            assert_eq!(world.id(), "minecraft:overworld");
            world.pvp.set(true);
            let _ = world.get_state().await;
        })
    })
    .await;
    let order = called.session.hear_message().await;
    assert_eq!(order.in_world, None);
    let asked = called.session.hear_message().await;
    assert_eq!(asked.in_world, None);
    assert!(matches!(
        asked.body,
        Some(from_plugin::Body::Question(v1::Question {
            question: Some(question::Question::World(_)),
            ..
        }))
    ));
}

#[tokio::test]
async fn deleting_a_world_reports_the_result() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let arena = player.world.create(options()).await.unwrap();
            arena.delete(true).await.unwrap();
        })
    })
    .await;
    let (request, _) = called.session.acted().await;
    called.session.say(answer(
        request,
        answer::Body::WorldCreated("rspigot:arena".to_owned()),
    ));
    let message = called.session.hear_message().await;
    assert_eq!(message.in_world.as_deref(), Some("rspigot:arena"));
    let request = message.request.expect("a result is expected");
    assert_eq!(
        message.body,
        Some(from_plugin::Body::World(v1::WorldOrder {
            order: Some(world_order::Order::DeleteWorld(v1::DeleteWorld {
                files: true
            })),
        }))
    );
    called
        .session
        .say(answer(request, answer::Body::Done(v1::Empty {})));
    called.finished().await;
}

#[tokio::test]
async fn teleporting_to_a_world_goes_to_its_spawn() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let arena = player.world.create(options()).await.unwrap();
            player.location.teleport_to_world(&arena).await.unwrap();
        })
    })
    .await;
    let (request, _) = called.session.acted().await;
    called.session.say(answer(
        request,
        answer::Body::WorldCreated("rspigot:arena".to_owned()),
    ));
    let (request, question) = called.session.asked().await;
    assert!(matches!(question, question::Question::World(_)));
    called
        .session
        .say(answer(request, answer::Body::World(arena_state())));
    let (request, body) = called.session.acted().await;
    let from_plugin::Body::Player(order) = body else {
        panic!("expected a player order");
    };
    let Some(v1::player_order::Order::TeleportTo(location)) = order.order else {
        panic!("expected a teleport to a location");
    };
    assert_eq!(location.world, "rspigot:arena");
    assert_eq!(
        location.position,
        Some(v1::Vec3 {
            x: 0.5,
            y: 4.0,
            z: 0.5
        })
    );
    called
        .session
        .say(answer(request, answer::Body::Done(v1::Empty {})));
    called.finished().await;
}
