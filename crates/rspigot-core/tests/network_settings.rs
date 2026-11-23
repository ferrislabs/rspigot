mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, Inbound, PlayerInput};
use rspigot_core::management::{ManagementCall, ManagementRequest, Setting, SettingValue};
use rspigot_core::server::Server;
use rspigot_core::status::{ANONYMOUS_PLAYER, StatusBoard};
use rspigot_domain::player::{ChatVisibility, ClientSettings};

fn settings(visibility: ChatVisibility, listing: bool) -> ClientSettings {
    ClientSettings {
        chat_visibility: visibility,
        allows_listing: listing,
        ..ClientSettings::default()
    }
}

fn is_chat(event: &ClientEvent) -> bool {
    matches!(
        event,
        ClientEvent::PlayerChat { .. }
            | ClientEvent::SignedChat(_)
            | ClientEvent::ChatMessage { .. }
    )
}

fn board_server(adjust: impl FnOnce(&mut GameConfig)) -> (Server<Link>, StatusBoard) {
    let mut config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    adjust(&mut config);
    let board = StatusBoard::new(20);
    let mut server = Server::new(
        config,
        LevelData {
            spawn_mobs: false,
            ..LevelData::default()
        },
        Arc::new(NoStore),
        Arc::new(Floor),
        board.clone(),
    );
    server.wait_for_spawn(Duration::from_secs(10));
    (server, board)
}

#[test]
fn a_hidden_chat_player_receives_no_chat_but_a_commands_only_player_still_gets_system_lines() {
    let mut server = calm_server();
    let alice = join(&mut server, "Alice");
    let hidden = join_with(
        &mut server,
        "Hidden",
        settings(ChatVisibility::Hidden, true),
    );
    let commands = join_with(&mut server, "Cmds", settings(ChatVisibility::System, true));
    let full = join(&mut server, "Full");
    for player in [&alice, &hidden, &commands, &full] {
        player.link.take();
    }
    send_input(&mut server, &alice, PlayerInput::Chat("hello".to_owned()));
    assert!(full.link.take().iter().any(is_chat));
    assert!(!hidden.link.take().iter().any(is_chat));
    assert!(!commands.link.take().iter().any(is_chat));
    server.handle(Inbound::Console("tellraw @a \"loud\"".to_owned()));
    server.deliver();
    assert!(
        !system_texts(&hidden)
            .iter()
            .any(|line| line.contains("loud"))
    );
    assert!(
        system_texts(&commands)
            .iter()
            .any(|line| line.contains("loud"))
    );
}

#[test]
fn a_player_with_chat_hidden_cannot_speak_and_is_told_why() {
    let mut server = calm_server();
    let alice = join_with(&mut server, "Alice", settings(ChatVisibility::Hidden, true));
    let bob = join(&mut server, "Bob");
    alice.link.take();
    bob.link.take();
    send_input(&mut server, &alice, PlayerInput::Chat("hi".to_owned()));
    assert!(!bob.link.take().iter().any(is_chat));
    let told = alice.link.take();
    assert!(told.iter().any(|event| matches!(
        event,
        ClientEvent::SystemChat { message, .. } if format!("{message:?}").contains("chat.disabled.options")
    )));
}

#[test]
fn the_status_sample_hides_players_who_refuse_listing() {
    let (mut server, board) = board_server(|_| {});
    join_with(&mut server, "Visible", settings(ChatVisibility::Full, true));
    join_with(
        &mut server,
        "Private",
        settings(ChatVisibility::Full, false),
    );
    let names: Vec<String> = board
        .snapshot()
        .sample
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&"Visible".to_owned()));
    assert!(names.contains(&ANONYMOUS_PLAYER.to_owned()));
    assert!(!names.contains(&"Private".to_owned()));
}

#[test]
fn a_settings_change_to_refuse_listing_updates_the_sample() {
    let (mut server, board) = board_server(|_| {});
    let alice = join(&mut server, "Alice");
    assert_eq!(board.snapshot().sample[0].name, "Alice");
    send_input(
        &mut server,
        &alice,
        PlayerInput::Settings(settings(ChatVisibility::Full, false)),
    );
    assert_eq!(board.snapshot().sample[0].name, ANONYMOUS_PLAYER);
}

#[test]
fn joining_sends_the_server_data_with_motd_and_icon() {
    let mut server = configured(|config| {
        config.motd = "Hello".to_owned();
        config.server_icon = Some(vec![9, 9]);
    });
    let link = Link::default();
    server.handle(Inbound::Join {
        profile: rspigot_domain::player::GameProfile {
            id: rspigot_domain::player::PlayerId::offline("Bob"),
            name: "Bob".to_owned(),
            properties: Vec::new(),
        },
        settings: ClientSettings::default(),
        link: link.clone(),
        address: [127, 0, 0, 1].into(),
        session: 77,
    });
    server.deliver();
    let data = link.take().into_iter().find_map(|event| match event {
        ClientEvent::ServerData { motd, icon } => Some((motd.plain_text(), icon)),
        _ => None,
    });
    assert_eq!(data, Some(("Hello".to_owned(), Some(vec![9, 9]))));
}

#[test]
fn latency_is_measured_listed_and_refreshed_every_600_ticks() {
    let mut server = calm_server();
    let alice = join(&mut server, "Alice");
    send_input(&mut server, &alice, PlayerInput::Latency(120));
    for _ in 0..600 {
        server.tick();
        server.deliver();
    }
    let refreshes: Vec<Vec<(rspigot_domain::player::PlayerId, i32)>> = alice
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::PlayerInfoLatency(latencies) => Some(latencies),
            _ => None,
        })
        .collect();
    assert_eq!(refreshes.len(), 1);
    assert_eq!(refreshes[0][0].0, alice.id);
    assert!(refreshes[0][0].1 > 0);
}

#[test]
fn a_hot_view_distance_change_is_announced_to_every_player() {
    let mut server = calm_server();
    let alice = join(&mut server, "Alice");
    alice.link.take();
    let (reply, answer) = crossbeam_channel::bounded(1);
    server.handle(Inbound::Management(ManagementCall {
        request: ManagementRequest::Setting {
            setting: Setting::ViewDistance,
            value: Some(SettingValue::Int(5)),
        },
        reply,
    }));
    answer.recv().expect("answered");
    server.deliver();
    let radius = alice.link.take().into_iter().find_map(|event| match event {
        ClientEvent::ViewDistance(radius) => Some(radius),
        _ => None,
    });
    assert_eq!(radius, Some(5));
}

#[test]
fn a_hot_simulation_distance_change_is_announced_to_every_player() {
    let mut server = calm_server();
    let alice = join(&mut server, "Alice");
    alice.link.take();
    let (reply, answer) = crossbeam_channel::bounded(1);
    server.handle(Inbound::Management(ManagementCall {
        request: ManagementRequest::Setting {
            setting: Setting::SimulationDistance,
            value: Some(SettingValue::Int(6)),
        },
        reply,
    }));
    answer.recv().expect("answered");
    server.deliver();
    let distance = alice.link.take().into_iter().find_map(|event| match event {
        ClientEvent::SimulationDistance(distance) => Some(distance),
        _ => None,
    });
    assert_eq!(distance, Some(6));
}
