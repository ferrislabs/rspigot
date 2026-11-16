mod common;

use common::*;
use rspigot_core::events::{ClientEvent, Inbound, PlayerInput};
use rspigot_core::plugins::events::{
    AsyncPlayerChat, AsyncPlayerChatPatch, PlayerCommandPreprocess, PlayerCommandPreprocessPatch,
};
use rspigot_core::plugins::{
    EventKind, EventPatch, EventPayload, HandlerSpec, ListDiff, PlayerOrder, PlayerSnapshot,
    Priority, SetDiff,
};

fn chat_lines(player: &Player) -> Vec<String> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::PlayerChat { message, .. } => Some(format!("chat:{message}")),
            ClientEvent::SystemChat { message, .. } => {
                Some(rspigot_core::console::console_text(&message))
            }
            _ => None,
        })
        .collect()
}

fn say(s: &mut Scene, player: &Player, message: &str) {
    send_input(&mut s.server, player, PlayerInput::Chat(message.to_owned()));
}

#[test]
fn an_unchanged_chat_reaches_everyone_as_player_chat() {
    let mut s = scene_with(
        &["Alice", "Bob"],
        vec![observe(1, EventKind::AsyncPlayerChat)],
    );
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    say(&mut s, &alice, "hello");
    assert_eq!(chat_lines(&bob), ["chat:hello"]);
    let heard = settled_for(&s.plugin, EventKind::AsyncPlayerChat);
    let EventPayload::AsyncPlayerChat(AsyncPlayerChat {
        format,
        message,
        player,
        recipients,
    }) = &heard[0].payload
    else {
        panic!("expected a chat");
    };
    assert_eq!(format, "<%1$s> %2$s");
    assert_eq!(message, "hello");
    assert_eq!(player.id, alice.id);
    let names: Vec<&str> = recipients.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Alice", "Bob"]);
}

#[test]
fn a_cancelled_chat_reaches_nobody() {
    let mut s = scene_with(
        &["Alice", "Bob"],
        vec![deciding(1, EventKind::AsyncPlayerChat)],
    );
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    say(&mut s, &alice, "hello");
    assert!(chat_lines(&bob).is_empty());
    let asked = last_asked(&s.plugin, EventKind::AsyncPlayerChat);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(chat_lines(&bob).is_empty());
    assert!(chat_lines(&alice).is_empty());
}

#[test]
fn a_changed_message_and_format_are_sent_as_system_text() {
    let mut s = scene_with(
        &["Alice", "Bob"],
        vec![deciding(1, EventKind::AsyncPlayerChat)],
    );
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    say(&mut s, &alice, "hello");
    let asked = last_asked(&s.plugin, EventKind::AsyncPlayerChat);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::AsyncPlayerChat(AsyncPlayerChatPatch {
            format: Some("§7[%s] %s!".to_owned()),
            message: Some("hi there".to_owned()),
            ..AsyncPlayerChatPatch::default()
        })),
    );
    assert_eq!(chat_lines(&bob), ["[Alice] hi there!"]);
    assert_eq!(chat_lines(&alice), ["[Alice] hi there!"]);
}

#[test]
fn changed_recipients_limit_who_reads_the_chat() {
    let mut s = scene_with(
        &["Alice", "Bob", "Carol"],
        vec![deciding(1, EventKind::AsyncPlayerChat)],
    );
    let (alice, bob, carol) = (
        s.players[0].clone(),
        s.players[1].clone(),
        s.players[2].clone(),
    );
    say(&mut s, &alice, "psst");
    let asked = last_asked(&s.plugin, EventKind::AsyncPlayerChat);
    let EventPayload::AsyncPlayerChat(AsyncPlayerChat { recipients, .. }) = &asked.payload else {
        panic!("expected a chat");
    };
    let only_bob = removing(recipients, |player| player.id != bob.id);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::AsyncPlayerChat(AsyncPlayerChatPatch {
            recipients: only_bob,
            ..AsyncPlayerChatPatch::default()
        })),
    );
    assert_eq!(chat_lines(&bob), ["<Alice> psst"]);
    assert!(chat_lines(&carol).is_empty());
    assert!(chat_lines(&alice).is_empty());
}

fn chatting(priority: Priority) -> HandlerSpec {
    HandlerSpec {
        priority,
        ..deciding(1, EventKind::AsyncPlayerChat)
    }
}

fn two_plugins_chat(names: &[&str]) -> (Scene, u64, Vec<PlayerSnapshot>) {
    let mut s = scene_with(names, vec![chatting(Priority::Low)]);
    connect_with(&mut s.server, 2, "second", vec![chatting(Priority::High)]);
    let alice = s.players[0].clone();
    say(&mut s, &alice, "psst");
    let asked = last_asked(&s.plugin, EventKind::AsyncPlayerChat);
    let EventPayload::AsyncPlayerChat(AsyncPlayerChat { recipients, .. }) = asked.payload else {
        panic!("expected a chat");
    };
    (s, asked.id, recipients)
}

fn recipients_patch(recipients: SetDiff<PlayerSnapshot>) -> Option<EventPatch> {
    Some(EventPatch::AsyncPlayerChat(AsyncPlayerChatPatch {
        recipients,
        ..AsyncPlayerChatPatch::default()
    }))
}

#[test]
fn two_plugins_that_each_remove_a_recipient_both_keep_the_chat_from_them() {
    for later_answers_first in [false, true] {
        let (mut s, event, offered) = two_plugins_chat(&["Alice", "Bob", "Carol", "Dave"]);
        let [alice, bob, carol, dave] = [0, 1, 2, 3].map(|index| s.players[index].clone());
        let mut answers = vec![
            (1, removing(&offered, |player| player.id == bob.id)),
            (2, removing(&offered, |player| player.id == carol.id)),
        ];
        if later_answers_first {
            answers.reverse();
        }
        for (connection, diff) in answers {
            rule_on(
                &mut s.server,
                connection,
                event,
                false,
                recipients_patch(diff),
            );
        }
        assert_eq!(chat_lines(&alice), ["<Alice> psst"]);
        assert_eq!(chat_lines(&dave), ["<Alice> psst"]);
        assert!(chat_lines(&bob).is_empty(), "{later_answers_first}");
        assert!(chat_lines(&carol).is_empty(), "{later_answers_first}");
    }
}

#[test]
fn a_recipient_removed_by_one_plugin_and_added_back_by_a_later_one_reads_the_chat_once() {
    for later_answers_first in [false, true] {
        let (mut s, event, offered) = two_plugins_chat(&["Alice", "Bob", "Carol"]);
        let [alice, bob, carol] = [0, 1, 2].map(|index| s.players[index].clone());
        let bob_again: Vec<PlayerSnapshot> = offered
            .iter()
            .filter(|player| player.id == bob.id)
            .cloned()
            .collect();
        let mut answers = vec![
            (1, removing(&offered, |player| player.id == bob.id)),
            (
                2,
                ListDiff {
                    removed: Vec::new(),
                    replaced: Vec::new(),
                    added: bob_again,
                },
            ),
        ];
        if later_answers_first {
            answers.reverse();
        }
        for (connection, diff) in answers {
            rule_on(
                &mut s.server,
                connection,
                event,
                false,
                recipients_patch(diff),
            );
        }
        for player in [&alice, &bob, &carol] {
            assert_eq!(
                chat_lines(player),
                ["<Alice> psst"],
                "{later_answers_first}"
            );
        }
    }
}

#[test]
fn a_chat_sent_just_before_leaving_is_broadcast_before_the_quit() {
    let mut s = scene_with(
        &["Alice", "Bob"],
        vec![deciding(1, EventKind::AsyncPlayerChat)],
    );
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    say(&mut s, &alice, "bye");
    let asked = last_asked(&s.plugin, EventKind::AsyncPlayerChat);
    assert!(chat_lines(&bob).is_empty());
    s.server.handle(leave_of(&alice));
    s.server.deliver();
    assert_eq!(chat_lines(&bob), ["chat:bye", "Alice left the game"]);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(chat_lines(&bob).is_empty());
}

#[test]
fn a_plugin_made_chat_fires_the_chat_event_too() {
    let mut s = scene_with(
        &["Alice", "Bob"],
        vec![observe(1, EventKind::AsyncPlayerChat)],
    );
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    s.act(to(&alice, PlayerOrder::Chat("from a plugin".to_owned())));
    assert_eq!(chat_lines(&bob), ["chat:from a plugin"]);
    assert_eq!(settled_for(&s.plugin, EventKind::AsyncPlayerChat).len(), 1);
}

#[test]
fn a_cancelled_command_does_not_run() {
    let mut s = scene_with(
        &["Alice"],
        vec![deciding(1, EventKind::PlayerCommandPreprocess)],
    );
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Command("seed".to_owned()),
    );
    let asked = last_asked(&s.plugin, EventKind::PlayerCommandPreprocess);
    assert_eq!(
        asked.payload,
        EventPayload::PlayerCommandPreprocess(PlayerCommandPreprocess {
            message: "/seed".to_owned(),
            player: command_player(&asked.payload),
        })
    );
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(system_texts(&alice).is_empty());
}

fn command_player(payload: &EventPayload) -> rspigot_core::plugins::PlayerSnapshot {
    match payload {
        EventPayload::PlayerCommandPreprocess(PlayerCommandPreprocess { player, .. }) => {
            player.clone()
        }
        other => panic!("expected a command, got {other:?}"),
    }
}

#[test]
fn a_changed_command_runs_instead_of_the_typed_one() {
    let mut s = scene_with(
        &["Alice"],
        vec![deciding(1, EventKind::PlayerCommandPreprocess)],
    );
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Command("seed".to_owned()),
    );
    let asked = last_asked(&s.plugin, EventKind::PlayerCommandPreprocess);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::PlayerCommandPreprocess(
            PlayerCommandPreprocessPatch {
                message: Some("/list".to_owned()),
            },
        )),
    );
    let said = system_texts(&alice);
    assert_eq!(said.len(), 1);
    assert!(said[0].contains("players online"), "{said:?}");
}

#[test]
fn a_command_sent_just_before_leaving_still_runs() {
    let mut s = scene_with(
        &["Alice", "Bob"],
        vec![deciding(1, EventKind::PlayerCommandPreprocess)],
    );
    s.server.handle(Inbound::Console("op Alice".to_owned()));
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Command("kill Bob".to_owned()),
    );
    let asked = last_asked(&s.plugin, EventKind::PlayerCommandPreprocess);
    assert_eq!(s.server.player_is_dead(s.players[1].id), Some(false));
    s.server.handle(leave_of(&alice));
    assert_eq!(s.server.player_is_dead(s.players[1].id), Some(true));
    rule_on(&mut s.server, 1, asked.id, true, None);
}

#[test]
fn perform_command_skips_the_preprocess_event() {
    let mut s = scene_with(
        &["Alice"],
        vec![observe(1, EventKind::PlayerCommandPreprocess)],
    );
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::PerformCommand("seed".to_owned())));
    assert!(settled_for(&s.plugin, EventKind::PlayerCommandPreprocess).is_empty());
    s.act(to(&alice, PlayerOrder::Chat("/seed".to_owned())));
    assert_eq!(
        settled_for(&s.plugin, EventKind::PlayerCommandPreprocess).len(),
        1
    );
}

#[test]
fn a_chat_and_a_command_settle_in_the_order_they_were_sent() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            deciding(1, EventKind::AsyncPlayerChat),
            deciding(2, EventKind::PlayerCommandPreprocess),
        ],
    );
    let alice = s.players[0].clone();
    say(&mut s, &alice, "first");
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Command("version".to_owned()),
    );
    let command = last_asked(&s.plugin, EventKind::PlayerCommandPreprocess);
    judge(&mut s.server, 1, command.id, 2, false, None);
    assert!(chat_lines(&alice).is_empty());
    let chat = last_asked(&s.plugin, EventKind::AsyncPlayerChat);
    rule_on(&mut s.server, 1, chat.id, false, None);
    let lines = chat_lines(&alice);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(lines[0], "chat:first");
    assert!(lines[1].starts_with("This server is running"), "{lines:?}");
}
