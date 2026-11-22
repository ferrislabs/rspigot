mod common;

use common::*;
use rspigot_core::commands::{CommandOutcome, CommandSender, execute};
use rspigot_core::events::{ChatKind, ClientEvent, Inbound, PlayerInput};

type Heard = (ChatKind, String, Option<String>, String);

fn scene_with_operator() -> Scene {
    let mut s = scene(&["Alice", "Bob"]);
    s.server.handle(Inbound::Console("op Alice".to_owned()));
    s.server.deliver();
    for player in &s.players {
        player.link.take();
    }
    s
}

fn heard(player: &Player) -> Vec<Heard> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::ChatMessage {
                chat_type,
                sender_name,
                target_name,
                message,
            } => Some((
                chat_type,
                sender_name.plain_text(),
                target_name.map(|t| t.plain_text()),
                message.plain_text(),
            )),
            _ => None,
        })
        .collect()
}

fn run(s: &mut Scene, line: &str) -> CommandOutcome {
    let alice = s.players[0].id;
    let outcome = execute(&mut s.server, CommandSender::Player(alice), line);
    s.server.deliver();
    outcome
}

#[test]
fn msg_sends_the_msg_command_chat_types_to_both_sides() {
    let mut s = scene_with_operator();
    assert_eq!(run(&mut s, "msg Bob hello there"), CommandOutcome::Success);
    assert_eq!(
        heard(&s.players[0]),
        vec![(
            ChatKind::MsgOutgoing,
            "Alice".to_owned(),
            Some("Bob".to_owned()),
            "hello there".to_owned()
        )]
    );
    assert_eq!(
        heard(&s.players[1]),
        vec![(
            ChatKind::MsgIncoming,
            "Alice".to_owned(),
            Some("Bob".to_owned()),
            "hello there".to_owned()
        )]
    );
}

#[test]
fn msg_reaches_every_selected_player_and_refuses_no_target() {
    let mut s = scene_with_operator();
    assert_eq!(run(&mut s, "tell @a hi"), CommandOutcome::Success);
    assert_eq!(heard(&s.players[0]).len(), 3);
    assert_eq!(heard(&s.players[1]).len(), 1);
    assert_eq!(run(&mut s, "msg Nobody hi"), CommandOutcome::Failure);
}

#[test]
fn say_and_me_use_the_say_and_emote_chat_types() {
    let mut s = scene_with_operator();
    assert_eq!(run(&mut s, "say good morning"), CommandOutcome::Success);
    assert_eq!(
        heard(&s.players[1]),
        vec![(
            ChatKind::SayCommand,
            "Alice".to_owned(),
            None,
            "good morning".to_owned()
        )]
    );
    assert_eq!(run(&mut s, "me waves"), CommandOutcome::Success);
    assert_eq!(
        heard(&s.players[1]),
        vec![(
            ChatKind::EmoteCommand,
            "Alice".to_owned(),
            None,
            "waves".to_owned()
        )]
    );
}

#[test]
fn teammsg_uses_the_team_chat_types_with_the_team_as_target() {
    let mut s = scene_with_operator();
    for line in [
        "team add red_team",
        "team join red_team Alice",
        "team join red_team Bob",
    ] {
        assert_eq!(run(&mut s, line), CommandOutcome::Success, "{line}");
    }
    for player in &s.players {
        player.link.take();
    }
    assert_eq!(run(&mut s, "teammsg hello team"), CommandOutcome::Success);
    let alice = heard(&s.players[0]);
    let bob = heard(&s.players[1]);
    assert_eq!(alice[0].0, ChatKind::TeamOutgoing);
    assert_eq!(bob[0].0, ChatKind::TeamIncoming);
    assert_eq!(bob[0].2.as_deref(), Some("[red_team]"));
    assert_eq!(run(&mut s, "tm hi"), CommandOutcome::Success);
}

#[test]
fn chat_spam_kicks_a_non_operator_after_the_threshold() {
    let mut s = scene_on(
        configured(|config| config.chat.spam_threshold_ticks = 60),
        &["Alice", "Bob"],
        Vec::new(),
    );
    let bob = s.players[1].clone();
    for _ in 0..3 {
        send_input(&mut s.server, &bob, PlayerInput::Chat("hi".to_owned()));
    }
    assert!(bob.link.disconnected_with().is_none());
    send_input(&mut s.server, &bob, PlayerInput::Chat("hi".to_owned()));
    let reason = bob.link.disconnected_with().expect("kicked for spam");
    assert_eq!(reason.plain_text(), "disconnect.spam");
}
