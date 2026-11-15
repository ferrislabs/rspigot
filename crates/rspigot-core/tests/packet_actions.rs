mod common;

use common::*;
use rspigot_core::admin::{AdminLists, ListEntry};
use rspigot_core::events::{ClientEvent, PlayerInput};
use rspigot_domain::world::Difficulty;

fn operators(names: &[&str]) -> impl FnOnce(&mut rspigot_core::config::GameConfig) {
    let entries: Vec<ListEntry> = names.iter().map(|name| ListEntry::named(name)).collect();
    move |config| {
        config.admin = AdminLists {
            operators: entries,
            ..AdminLists::default()
        }
    }
}

fn difficulties(player: &Player) -> Vec<(Difficulty, bool)> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Difficulty { difficulty, locked } => Some((difficulty, locked)),
            _ => None,
        })
        .collect()
}

#[test]
fn an_operator_changing_the_difficulty_tells_every_player() {
    let mut server = configured(operators(&["Alice"]));
    let alice = arrive(&mut server, "Alice");
    let bob = arrive(&mut server, "Bob");
    alice.link.take();
    bob.link.take();
    send_input(
        &mut server,
        &alice,
        PlayerInput::ChangeDifficulty(Difficulty::Hard),
    );
    assert_eq!(difficulties(&alice), [(Difficulty::Hard, false)]);
    assert_eq!(difficulties(&bob), [(Difficulty::Hard, false)]);
}

#[test]
fn a_player_without_permission_cannot_change_or_lock_the_difficulty() {
    let mut server = configured(operators(&["Alice"]));
    let alice = arrive(&mut server, "Alice");
    let bob = arrive(&mut server, "Bob");
    alice.link.take();
    bob.link.take();
    send_input(
        &mut server,
        &bob,
        PlayerInput::ChangeDifficulty(Difficulty::Hard),
    );
    send_input(&mut server, &bob, PlayerInput::LockDifficulty(true));
    assert!(difficulties(&alice).is_empty());
    assert!(difficulties(&bob).is_empty());
}

#[test]
fn a_locked_difficulty_refuses_packet_changes_and_stays_locked_for_new_players() {
    let mut server = configured(operators(&["Alice"]));
    let alice = arrive(&mut server, "Alice");
    alice.link.take();
    send_input(&mut server, &alice, PlayerInput::LockDifficulty(true));
    assert_eq!(difficulties(&alice), [(Difficulty::Easy, true)]);
    send_input(
        &mut server,
        &alice,
        PlayerInput::ChangeDifficulty(Difficulty::Hard),
    );
    assert!(difficulties(&alice).is_empty());
    let late = arrive(&mut server, "Carol");
    assert_eq!(difficulties(&late), [(Difficulty::Easy, true)]);
    send_input(&mut server, &alice, PlayerInput::LockDifficulty(false));
    send_input(
        &mut server,
        &alice,
        PlayerInput::ChangeDifficulty(Difficulty::Normal),
    );
    assert_eq!(
        difficulties(&alice),
        [(Difficulty::Easy, false), (Difficulty::Normal, false)]
    );
}

#[test]
fn a_hardcore_world_keeps_the_hard_difficulty() {
    let mut server = configured(|config| {
        operators(&["Alice"])(config);
        config.hardcore = true;
    });
    let alice = arrive(&mut server, "Alice");
    alice.link.take();
    send_input(
        &mut server,
        &alice,
        PlayerInput::ChangeDifficulty(Difficulty::Peaceful),
    );
    assert_eq!(difficulties(&alice), [(Difficulty::Hard, false)]);
}
