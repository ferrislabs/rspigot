mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, Hand};
use rspigot_core::plugins::{
    Answer, BedSpawn, ExtraAnswer, ExtraQuestion, ItemSpec, Order, PlayerAction, PlayerOrder,
    Question, ServerOrder, StatisticMode,
};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::{BlockPos, Vec3};
use rspigot_entities::SoundSource;

fn act(s: &mut Scene, action: PlayerAction) {
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::Action(action)));
}

fn count(s: &mut Scene, question: ExtraQuestion) -> i32 {
    match s.ask(Question::Extra(question)) {
        Answer::Extra(ExtraAnswer::Count(count)) => count,
        other => panic!("expected a count, got {other:?}"),
    }
}

fn statistic(s: &mut Scene, kind: &str, key: &str) -> i32 {
    let player = s.players[0].id;
    count(
        s,
        ExtraQuestion::Statistic {
            player,
            kind: kind.to_owned(),
            key: key.to_owned(),
        },
    )
}

fn change(s: &mut Scene, kind: &str, key: &str, amount: i32, mode: StatisticMode) {
    act(
        s,
        PlayerAction::Statistic {
            kind: kind.to_owned(),
            key: key.to_owned(),
            amount,
            mode,
        },
    );
}

#[test]
fn stop_sound_reaches_the_player_with_its_category_and_sound() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();

    act(
        &mut s,
        PlayerAction::StopSound {
            sound: Some("entity.cow.hurt".to_owned()),
            source: Some("hostile".to_owned()),
        },
    );

    assert!(s.warnings().is_empty());
    assert!(alice.link.take().iter().any(|event| matches!(
        event,
        ClientEvent::StopSound { source: Some(SoundSource::Hostile), sound: Some(sound) }
            if sound == "minecraft:entity.cow.hurt"
    )));
}

#[test]
fn stop_sound_without_filters_stops_everything_and_an_unknown_category_is_refused() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();

    act(
        &mut s,
        PlayerAction::StopSound {
            sound: None,
            source: None,
        },
    );
    assert!(alice.link.take().iter().any(|event| matches!(
        event,
        ClientEvent::StopSound {
            source: None,
            sound: None
        }
    )));

    act(
        &mut s,
        PlayerAction::StopSound {
            sound: None,
            source: Some("loudest".to_owned()),
        },
    );
    assert_eq!(s.warnings().len(), 1);
}

#[test]
fn a_cooldown_is_sent_counts_down_and_can_be_read() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let player = alice.id;
    let remaining = |s: &mut Scene| {
        count(
            s,
            ExtraQuestion::Cooldown {
                player,
                item: "minecraft:ender_pearl".to_owned(),
            },
        )
    };

    act(
        &mut s,
        PlayerAction::SetCooldown {
            item: "minecraft:ender_pearl".to_owned(),
            ticks: 100,
        },
    );

    assert!(alice.link.take().iter().any(|event| matches!(
        event,
        ClientEvent::Cooldown { group, ticks: 100 } if group == "minecraft:ender_pearl"
    )));
    assert_eq!(remaining(&mut s), 100);
    s.tick(40);
    assert_eq!(remaining(&mut s), 60);
}

#[test]
fn a_zero_cooldown_clears_it_and_an_unknown_item_is_refused() {
    let mut s = scene(&["Alice"]);
    let player = s.players[0].id;
    act(
        &mut s,
        PlayerAction::SetCooldown {
            item: "minecraft:ender_pearl".to_owned(),
            ticks: 100,
        },
    );

    act(
        &mut s,
        PlayerAction::SetCooldown {
            item: "ender_pearl".to_owned(),
            ticks: 0,
        },
    );
    assert_eq!(
        count(
            &mut s,
            ExtraQuestion::Cooldown {
                player,
                item: "ender_pearl".to_owned()
            }
        ),
        0
    );

    act(
        &mut s,
        PlayerAction::SetCooldown {
            item: "minecraft:nothing".to_owned(),
            ticks: 5,
        },
    );
    assert_eq!(s.warnings().len(), 1);
}

#[test]
fn incrementing_a_statistic_adds_to_what_the_player_has() {
    let mut s = scene(&["Alice"]);
    assert_eq!(statistic(&mut s, "minecraft:mined", "minecraft:stone"), 0);

    change(
        &mut s,
        "minecraft:mined",
        "minecraft:stone",
        3,
        StatisticMode::Increment,
    );
    change(
        &mut s,
        "minecraft:mined",
        "stone",
        2,
        StatisticMode::Increment,
    );

    assert!(s.warnings().is_empty());
    assert_eq!(statistic(&mut s, "minecraft:mined", "minecraft:stone"), 5);
    assert_eq!(statistic(&mut s, "minecraft:mined", "minecraft:dirt"), 0);
}

#[test]
fn decrementing_and_setting_a_statistic_never_goes_below_zero() {
    let mut s = scene(&["Alice"]);
    change(
        &mut s,
        "minecraft:custom",
        "minecraft:jump",
        10,
        StatisticMode::Set,
    );
    assert_eq!(statistic(&mut s, "minecraft:custom", "minecraft:jump"), 10);

    change(
        &mut s,
        "minecraft:custom",
        "minecraft:jump",
        4,
        StatisticMode::Decrement,
    );
    assert_eq!(statistic(&mut s, "minecraft:custom", "minecraft:jump"), 6);

    change(
        &mut s,
        "minecraft:custom",
        "minecraft:jump",
        100,
        StatisticMode::Decrement,
    );
    assert_eq!(statistic(&mut s, "minecraft:custom", "minecraft:jump"), 0);
}

#[test]
fn a_statistic_with_an_unknown_kind_or_qualifier_is_refused() {
    let mut s = scene(&["Alice"]);

    change(
        &mut s,
        "minecraft:flown",
        "minecraft:stone",
        1,
        StatisticMode::Increment,
    );
    change(
        &mut s,
        "minecraft:mined",
        "minecraft:not_a_block",
        1,
        StatisticMode::Increment,
    );
    change(
        &mut s,
        "minecraft:mined",
        "minecraft:stone",
        0,
        StatisticMode::Increment,
    );

    assert_eq!(s.warnings().len(), 3);
    assert_eq!(statistic(&mut s, "minecraft:mined", "minecraft:stone"), 0);
    let player = s.players[0].id;
    let answer = s.ask(Question::Extra(ExtraQuestion::Statistic {
        player,
        kind: "minecraft:mined".to_owned(),
        key: "minecraft:not_a_block".to_owned(),
    }));
    assert!(matches!(answer, Answer::Refused(_)));
}

#[test]
fn the_compass_target_is_sent_stored_and_defaults_to_the_world_spawn() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let read = |s: &mut Scene| match s.ask(Question::Extra(ExtraQuestion::CompassTarget(alice.id)))
    {
        Answer::Extra(ExtraAnswer::CompassTarget(target)) => target,
        other => panic!("expected a target, got {other:?}"),
    };
    let spawn = read(&mut s);

    act(
        &mut s,
        PlayerAction::SetCompassTarget(Vec3::new(100.5, 70.0, -40.5)),
    );

    assert!(alice.link.take().iter().any(|event| matches!(
        event,
        ClientEvent::SpawnPosition { pos, .. } if *pos == BlockPos::new(100, 70, -41)
    )));
    assert_eq!(read(&mut s), Vec3::new(100.5, 70.0, -40.5));
    assert_ne!(read(&mut s), spawn);
}

#[test]
fn a_bed_spawn_is_stored_read_and_cleared() {
    let mut s = scene(&["Alice"]);
    let player = s.players[0].id;
    let read = |s: &mut Scene| match s.ask(Question::Extra(ExtraQuestion::BedSpawn(player))) {
        Answer::Extra(ExtraAnswer::BedSpawn(spawn)) => spawn,
        other => panic!("expected a bed spawn, got {other:?}"),
    };
    assert_eq!(read(&mut s), None);

    act(
        &mut s,
        PlayerAction::SetBedSpawn(BedSpawn {
            pos: BlockPos::new(7, 64, 9),
            world: String::new(),
            yaw: 90.0,
            force: true,
        }),
    );

    let spawn = read(&mut s).expect("a bed spawn");
    assert_eq!(spawn.pos, BlockPos::new(7, 64, 9));
    assert_eq!(spawn.yaw, 90.0);
    assert!(spawn.force);
    assert_eq!(spawn.world, "minecraft:overworld");

    act(&mut s, PlayerAction::ClearBedSpawn);
    assert_eq!(read(&mut s), None);
}

#[test]
fn a_bed_spawn_in_an_invalid_world_is_refused() {
    let mut s = scene(&["Alice"]);

    act(
        &mut s,
        PlayerAction::SetBedSpawn(BedSpawn {
            pos: BlockPos::new(0, 64, 0),
            world: "NOT A WORLD".to_owned(),
            yaw: 0.0,
            force: false,
        }),
    );

    assert_eq!(s.warnings().len(), 1);
}

#[test]
fn open_book_swaps_the_held_slot_opens_the_book_and_restores_the_slot() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    alice.link.take();

    act(
        &mut s,
        PlayerAction::OpenBook(ItemSpec::of("minecraft:written_book", 1)),
    );

    assert!(s.warnings().is_empty());
    let events = alice.link.take();
    let slots: Vec<&ClientEvent> = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                ClientEvent::PlayerInventorySlot { .. } | ClientEvent::OpenBook(Hand::Main)
            )
        })
        .collect();
    assert_eq!(slots.len(), 3);
    assert!(matches!(
        slots[0],
        ClientEvent::PlayerInventorySlot { stack: Some(stack), .. }
            if stack.item.kind().map(|kind| kind.name()) == Some("minecraft:written_book")
    ));
    assert!(matches!(slots[1], ClientEvent::OpenBook(Hand::Main)));
    assert!(matches!(
        slots[2],
        ClientEvent::PlayerInventorySlot { stack: None, .. }
    ));
}

#[test]
fn open_book_refuses_an_item_that_is_not_a_written_book() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    alice.link.take();

    act(
        &mut s,
        PlayerAction::OpenBook(ItemSpec::of("minecraft:stick", 1)),
    );

    assert_eq!(s.warnings().len(), 1);
    assert!(
        !alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::OpenBook(_)))
    );
}

fn board_scene() -> (Scene, StatusBoard) {
    let board = StatusBoard::new(20);
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    let level = LevelData {
        spawn_mobs: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        level,
        Arc::new(NoStore),
        Arc::new(Floor),
        board.clone(),
    );
    server.wait_for_spawn(Duration::from_secs(10));
    (scene_on(server, &["Alice"], Vec::new()), board)
}

#[test]
fn the_motd_is_published_to_the_status_board() {
    let (mut s, board) = board_scene();
    assert_eq!(board.snapshot().motd, None);

    s.act(Order::Server(ServerOrder::SetMotd("§6Welcome".to_owned())));

    assert!(s.warnings().is_empty());
    assert_eq!(board.snapshot().motd.as_deref(), Some("§6Welcome"));
}

#[test]
fn max_players_changes_the_server_the_board_and_refuses_a_negative_count() {
    let (mut s, board) = board_scene();

    s.act(Order::Server(ServerOrder::SetMaxPlayers(7)));

    assert!(s.warnings().is_empty());
    assert_eq!(board.snapshot().max_players, 7);
    match s.ask(Question::Server) {
        Answer::Server(state) => assert_eq!(state.max_players, 7),
        other => panic!("expected the server state, got {other:?}"),
    }

    s.act(Order::Server(ServerOrder::SetMaxPlayers(-1)));
    assert_eq!(s.warnings().len(), 1);
    assert_eq!(board.snapshot().max_players, 7);
}
