mod common;

use common::*;
use rspigot_core::events::{ClientEvent, InputState, PlayerInput};
use rspigot_core::plugins::events::{PlayerStatisticIncrement, Statistic};
use rspigot_core::plugins::{EventKind, EventPayload};
use rspigot_core::ports::ProgressFile;

const CUSTOM: i32 = 8;
const JUMP: i32 = 23;
const PLAY_TIME: i32 = 1;

fn jump(s: &mut Scene, player: &Player) {
    send_input(
        &mut s.server,
        player,
        PlayerInput::Move {
            position: None,
            rotation: None,
            on_ground: true,
        },
    );
    send_input(
        &mut s.server,
        player,
        PlayerInput::Input(InputState {
            jump: true,
            ..InputState::default()
        }),
    );
}

fn counted(s: &mut Scene, player: &Player) -> Vec<(i32, i32, i32)> {
    player.link.take();
    send_input(&mut s.server, player, PlayerInput::RequestStats);
    player
        .link
        .take()
        .into_iter()
        .find_map(|event| match event {
            ClientEvent::Stats(entries) => Some(entries),
            _ => None,
        })
        .expect("the stats were sent")
}

#[test]
fn a_stats_request_sends_what_was_counted_with_registry_ids() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.tick(5);
    jump(&mut s, &alice);
    let entries = counted(&mut s, &alice);
    assert!(entries.contains(&(CUSTOM, JUMP, 1)));
    let play_time = entries
        .iter()
        .find(|(stat_type, value, _)| (*stat_type, *value) == (CUSTOM, PLAY_TIME))
        .expect("play time");
    assert!(play_time.2 >= 5);
}

#[test]
fn player_statistic_increment_carries_the_statistic_and_the_values() {
    let mut s = scene_with(
        &["Alice"],
        vec![observe(1, EventKind::PlayerStatisticIncrement)],
    );
    let alice = s.players[0].clone();
    jump(&mut s, &alice);
    let events = fired(&s.plugin);
    let EventPayload::PlayerStatisticIncrement(PlayerStatisticIncrement {
        statistic,
        previous_value,
        new_value,
        material,
        entity_type,
        ..
    }) = &events[0].payload
    else {
        panic!("a statistic increment");
    };
    assert_eq!(*statistic, Statistic::Jump);
    assert_eq!((*previous_value, *new_value), (0, 1));
    assert_eq!((material.as_deref(), entity_type.as_deref()), (None, None));
}

#[test]
fn a_cancelled_increment_leaves_the_counter_alone() {
    let mut s = scene_with(
        &["Alice"],
        vec![deciding(1, EventKind::PlayerStatisticIncrement)],
    );
    let alice = s.players[0].clone();
    jump(&mut s, &alice);
    let asked = last_asked(&s.plugin, EventKind::PlayerStatisticIncrement);
    rule_on(&mut s.server, 1, asked.id, true, None);
    let entries = counted(&mut s, &alice);
    assert!(!entries.iter().any(|(_, value, _)| *value == JUMP));
}

#[test]
fn an_allowed_increment_is_counted_once_the_plugin_answers() {
    let mut s = scene_with(
        &["Alice"],
        vec![deciding(1, EventKind::PlayerStatisticIncrement)],
    );
    let alice = s.players[0].clone();
    jump(&mut s, &alice);
    let asked = last_asked(&s.plugin, EventKind::PlayerStatisticIncrement);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert!(counted(&mut s, &alice).contains(&(CUSTOM, JUMP, 1)));
}

#[test]
fn the_clocks_and_distances_never_fire_the_event() {
    let mut s = scene_with(
        &["Alice"],
        vec![observe(1, EventKind::PlayerStatisticIncrement)],
    );
    s.tick(10);
    assert!(fired(&s.plugin).is_empty());
}

#[test]
fn statistics_are_saved_when_a_player_leaves_and_loaded_when_they_return() {
    let (mut s, saves) = saving_scene(&["Alice"], Vec::new());
    let alice = s.players[0].clone();
    jump(&mut s, &alice);
    s.server.handle(leave_of(&alice));
    s.server.deliver();
    let text = saves
        .progress(alice.id, ProgressFile::Stats)
        .expect("a stats file");
    assert!(text.contains("minecraft:jump"));
    assert!(text.contains("minecraft:leave_game"));
    let back = join(&mut s.server, "Alice");
    s.server.deliver();
    assert!(counted(&mut s, &back).contains(&(CUSTOM, JUMP, 1)));
}

const DEATHS: i32 = 32;
const TIME_SINCE_DEATH: i32 = 3;

#[test]
fn a_death_counts_and_restarts_the_time_since_death() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.tick(5);
    s.server.handle(rspigot_core::events::Inbound::Console(
        "kill Alice".to_owned(),
    ));
    s.server.deliver();
    let entries = counted(&mut s, &alice);
    assert!(entries.contains(&(CUSTOM, DEATHS, 1)));
    assert!(
        !entries
            .iter()
            .any(|(_, value, amount)| *value == TIME_SINCE_DEATH && *amount > 1)
    );
}
