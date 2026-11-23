mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, Inbound, PlayerInput};
use rspigot_core::ports::{PlayerStore, ProgressFile};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::Vec3;

fn with_saves(adjust: impl FnOnce(&mut GameConfig)) -> (Server<Link>, Arc<Saves>) {
    let saves = Arc::new(Saves::default());
    let server = configured(adjust).with_player_store(Arc::clone(&saves));
    (server, saves)
}

#[test]
fn disabled_saving_writes_nothing_and_enabled_saving_writes_everything() {
    let (mut server, saves) = with_saves(|config| {
        config.save_players = false;
        config.save_stats = false;
        config.save_advancements = false;
    });
    let alice = join(&mut server, "Alice");
    server.handle(leave_of(&alice));
    server.deliver();
    assert!(saves.progress(alice.id, ProgressFile::Stats).is_none());
    assert!(
        saves
            .progress(alice.id, ProgressFile::Advancements)
            .is_none()
    );
    assert!(saves.load_player(alice.id).unwrap().is_none());

    let (mut server, saves) = with_saves(|_| {});
    let bob = join(&mut server, "Bob");
    server.handle(leave_of(&bob));
    server.deliver();
    assert!(saves.progress(bob.id, ProgressFile::Stats).is_some());
    assert!(saves.progress(bob.id, ProgressFile::Advancements).is_some());
    assert!(saves.load_player(bob.id).unwrap().is_some());
}

#[test]
fn each_disable_saving_flag_only_stops_its_own_file() {
    let (mut server, saves) = with_saves(|config| config.save_stats = false);
    let alice = join(&mut server, "Alice");
    server.handle(leave_of(&alice));
    server.deliver();
    assert!(
        saves
            .progress(alice.id, ProgressFile::Advancements)
            .is_some()
    );
    assert!(saves.load_player(alice.id).unwrap().is_some());
    assert!(saves.progress(alice.id, ProgressFile::Stats).is_none());
}

#[test]
fn forced_stats_override_the_saved_custom_counters() {
    let (mut server, saves) = with_saves(|config| {
        config.forced_stats = vec![("minecraft:leave_game".to_owned(), 77)];
    });
    let id = rspigot_domain::player::PlayerId::offline("Alice");
    saves.seed_progress(
        id,
        ProgressFile::Stats,
        r#"{"stats":{"minecraft:custom":{"minecraft:leave_game":3}}}"#,
    );
    let alice = join(&mut server, "Alice");
    server.handle(leave_of(&alice));
    server.deliver();
    let mut saved = String::new();
    for _ in 0..200 {
        saved = saves.progress(id, ProgressFile::Stats).unwrap();
        if saved.contains("78") {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(saved.contains("\"minecraft:leave_game\":78"), "{saved}");
}

#[test]
fn a_disabled_advancement_is_never_awarded() {
    let mut server = configured(|config| {
        config.disabled_advancements = vec!["minecraft:story/*".to_owned()];
    });
    let alice = join(&mut server, "Alice");
    server.award_advancement(alice.id, "minecraft:story/root");
    assert!(
        server
            .obtained_criteria(alice.id, "minecraft:story/root")
            .is_empty()
    );
    let mut server = configured(|_| {});
    let alice = join(&mut server, "Alice");
    server.award_advancement(alice.id, "minecraft:story/root");
    assert!(
        !server
            .obtained_criteria(alice.id, "minecraft:story/root")
            .is_empty()
    );
}

#[test]
fn the_status_sample_is_capped_by_sample_count() {
    let status = StatusBoard::new(20);
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        status_sample_count: 2,
        ..GameConfig::default()
    };
    let mut server = Server::new(
        config,
        LevelData::default(),
        Arc::new(NoStore),
        Arc::new(Floor),
        status.clone(),
    );
    server.wait_for_spawn(Duration::from_secs(10));
    for name in ["A", "B", "C"] {
        join(&mut server, name);
    }
    assert_eq!(status.snapshot().sample.len(), 2);
}

fn teleports_after_a_far_move(multiplier: f64) -> usize {
    let mut server = configured(|config| config.moved_too_quickly_multiplier = multiplier);
    let alice = join(&mut server, "Alice");
    settle_arrival(&mut server, &alice);
    alice.link.take();
    server.handle(Inbound::Input {
        player: alice.id,
        input: PlayerInput::Move {
            position: Some(Vec3::new(15.5, 64.0, 0.5)),
            rotation: None,
            on_ground: true,
        },
    });
    server.deliver();
    alice
        .link
        .take()
        .iter()
        .filter(|event| matches!(event, ClientEvent::Teleport { .. }))
        .count()
}

#[test]
fn the_moved_too_quickly_multiplier_scales_the_movement_limit() {
    assert_eq!(teleports_after_a_far_move(10.0), 1);
    assert_eq!(teleports_after_a_far_move(20.0), 0);
}
