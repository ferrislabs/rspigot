mod common;

use common::*;
use rspigot_core::events::{Inbound, PlayerInput};

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn run(s: &mut Scene, who: usize, line: &str) {
    let player = s.players[who].clone();
    s.server.handle(Inbound::Input {
        player: player.id,
        input: PlayerInput::Command(line.to_owned()),
    });
    s.server.deliver();
}

fn ready() -> Scene {
    let mut s = scene(&["Alice", "Bob", "Carol"]);
    console(&mut s, "op Alice");
    console(&mut s, "op Bob");
    for player in &s.players {
        player.link.take();
    }
    s
}

fn admin_lines(s: &Scene, who: usize) -> Vec<String> {
    system_texts(&s.players[who])
        .into_iter()
        .filter(|line| line.contains("chat.type.admin"))
        .collect()
}

#[test]
fn a_logged_command_is_told_to_the_other_operators_only() {
    let mut s = ready();
    run(&mut s, 0, "time set day");
    assert!(admin_lines(&s, 0).is_empty());
    let bob = admin_lines(&s, 1);
    assert_eq!(bob.len(), 1, "{bob:?}");
    assert!(bob[0].contains("Alice"), "{bob:?}");
    assert!(admin_lines(&s, 2).is_empty());
}

#[test]
fn a_query_command_is_not_broadcast() {
    let mut s = ready();
    run(&mut s, 0, "seed");
    assert!(admin_lines(&s, 1).is_empty());
}

#[test]
fn send_command_feedback_off_silences_the_broadcast() {
    let mut s = ready();
    console(&mut s, "gamerule send_command_feedback false");
    for player in &s.players {
        player.link.take();
    }
    run(&mut s, 0, "time set day");
    assert!(admin_lines(&s, 1).is_empty());
}

#[test]
fn a_console_command_is_told_to_every_operator() {
    let mut s = ready();
    console(&mut s, "time set day");
    assert_eq!(admin_lines(&s, 0).len(), 1);
    assert_eq!(admin_lines(&s, 1).len(), 1);
    assert!(admin_lines(&s, 2).is_empty());
}
