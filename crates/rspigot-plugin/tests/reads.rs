mod fake_host;

use fake_host::*;
use rspigot_plugin::{BlockPos, BlockType, EntityType, Position, Uuid};
use rspigot_plugin_protocol::v1::{self, answer, question};

const AT: BlockPos = BlockPos { x: 2, y: 64, z: -3 };

#[tokio::test]
async fn get_block_returns_the_block_from_the_server() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let block = player.world.get_block(AT).await.unwrap();
            assert_eq!(block.kind, BlockType::Stone);
        })
    })
    .await;

    let (request, asked) = called.session.asked().await;
    assert_eq!(
        asked,
        question::Question::Block(v1::BlockPos { x: 2, y: 64, z: -3 })
    );
    called.session.say(answer(
        request,
        answer::Body::Block(v1::BlockState {
            block: "minecraft:stone".to_owned(),
            properties: Vec::new(),
        }),
    ));
    called.finished().await;
}

#[tokio::test]
async fn get_players_returns_a_list() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let online = player.server().get_players().await.unwrap();
            assert_eq!(online.len(), 2);
            assert_eq!(online[1].name, "Alex");
            let named = player.server().get_players_named("alex").await.unwrap();
            assert_eq!(named.len(), 1);
        })
    })
    .await;

    let (request, asked) = called.session.asked().await;
    assert_eq!(asked, question::Question::Players(v1::Empty {}));
    called.session.say(answer(
        request,
        answer::Body::Players(v1::PlayerList {
            players: vec![steve(), alex()],
        }),
    ));
    let (request, asked) = called.session.asked().await;
    assert_eq!(asked, question::Question::PlayerNamed("alex".to_owned()));
    called.session.say(answer(
        request,
        answer::Body::Players(v1::PlayerList {
            players: vec![alex()],
        }),
    ));
    called.finished().await;
}

#[tokio::test]
async fn get_state_reads_the_server() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let state = player.server().get_state().await.unwrap();
            assert_eq!(state.online, 2);
        })
    })
    .await;

    let (request, asked) = called.session.asked().await;
    assert_eq!(asked, question::Question::Server(v1::Empty {}));
    called.session.say(answer(
        request,
        answer::Body::Server(v1::ServerState {
            max_players: 20,
            online: 2,
            tps_1m: 20.0,
            tps_5m: 20.0,
            tps_15m: 20.0,
            whitelist_enabled: false,
        }),
    ));
    called.finished().await;
}

#[tokio::test]
async fn domain_handles_read_their_own_lists() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let server = player.server();
            let names = server.whitelist.get_names().await.unwrap();
            assert_eq!(names[0].name, "Carol");
            let ops = server.operators.get_names().await.unwrap();
            assert!(ops.is_empty());
            let bans = server.bans.get_entries().await.unwrap();
            assert_eq!(bans[0].target, "Steve");
            let ips = server.ip_bans.get_entries().await.unwrap();
            assert_eq!(ips[0].address.to_string(), "203.0.113.7");
        })
    })
    .await;

    let listed = answer::Body::Listed(v1::ListedPlayers {
        players: vec![v1::ListedPlayer {
            name: "Carol".to_owned(),
            uuid: None,
        }],
    });
    let entry = |target: &str| {
        answer::Body::Bans(v1::BanList {
            entries: vec![v1::BanEntry {
                target: target.to_owned(),
                uuid: None,
                created_ms: 1000,
                source: "Alice".to_owned(),
                expires_ms: None,
                reason: None,
            }],
        })
    };
    for (expected, body) in [
        (question::Question::Whitelist(v1::Empty {}), listed),
        (
            question::Question::Operators(v1::Empty {}),
            answer::Body::Listed(v1::ListedPlayers::default()),
        ),
        (question::Question::Bans(v1::Empty {}), entry("Steve")),
        (
            question::Question::IpBans(v1::Empty {}),
            entry("203.0.113.7"),
        ),
    ] {
        let (request, asked) = called.session.asked().await;
        assert_eq!(asked, expected);
        called.session.say(answer(request, body));
    }
    called.finished().await;
}

#[tokio::test]
async fn get_entities_lists_the_matching_entities() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let cows = player
                .world
                .entities
                .of(EntityType::Cow)
                .get_entities()
                .await
                .unwrap();
            assert_eq!(cows[0].kind, EntityType::Cow);
            let state = cows[0].get_state().await.unwrap();
            assert_eq!(state.fire_ticks, 5);
        })
    })
    .await;

    let snapshot = v1::EntitySnapshot {
        entity_id: 30,
        uuid: Uuid::from_u128(30).to_string(),
        kind: "minecraft:cow".to_owned(),
        position: Some(v1::Vec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }),
        player: None,
    };
    let (request, _) = called.session.asked().await;
    called.session.say(answer(
        request,
        answer::Body::Entities(v1::EntityList {
            entities: vec![snapshot.clone()],
        }),
    ));
    let (request, asked) = called.session.asked().await;
    assert_eq!(asked, question::Question::Entity(30));
    called.session.say(answer(
        request,
        answer::Body::Entity(v1::EntityState {
            entity: Some(snapshot),
            fire_ticks: 5,
            velocity: Some(v1::Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            }),
            ..Default::default()
        }),
    ));
    called.finished().await;
}

#[tokio::test]
async fn get_position_reads_the_current_position_of_a_player() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let position = player.get_position().await.unwrap();
            assert_eq!(
                position,
                Position {
                    x: 8.0,
                    y: 70.0,
                    z: 9.0
                }
            );
            let state = player.get_state().await.unwrap();
            assert_eq!(state.player.name, "Steve");
            let items = player.inventory.get_items().await.unwrap();
            assert_eq!(items.held_slot, 2);
        })
    })
    .await;

    let mut moved = steve();
    moved.position = Some(v1::Vec3 {
        x: 8.0,
        y: 70.0,
        z: 9.0,
    });
    let state = |player| v1::PlayerState {
        player: Some(player),
        address: "203.0.113.7".to_owned(),
        ..Default::default()
    };
    let (request, asked) = called.session.asked().await;
    assert_eq!(asked, question::Question::Player(STEVE.to_owned()));
    called
        .session
        .say(answer(request, answer::Body::Player(state(moved))));
    let (request, _) = called.session.asked().await;
    called
        .session
        .say(answer(request, answer::Body::Player(state(steve()))));
    let (request, asked) = called.session.asked().await;
    assert_eq!(asked, question::Question::Inventory(STEVE.to_owned()));
    called.session.say(answer(
        request,
        answer::Body::Inventory(v1::InventoryState {
            slots: Vec::new(),
            held_slot: 2,
        }),
    ));
    called.finished().await;
}
