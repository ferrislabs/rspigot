mod common;

use std::sync::atomic::{AtomicU64, Ordering};

use common::*;
use rspigot_core::events::ClientEvent;
use rspigot_core::plugins::{
    ActionError, ActionErrorKind, Answer, FromPlugin, Order, PlayerOrder, ToPlugin,
};

static NEXT: AtomicU64 = AtomicU64::new(5_000);

fn transfer(host: &str, port: u32) -> PlayerOrder {
    PlayerOrder::Transfer {
        host: host.to_owned(),
        port,
    }
}

fn perform(s: &mut Scene, order: Order) -> Option<Answer> {
    let request = NEXT.fetch_add(1, Ordering::Relaxed);
    common::order(&mut s.server, 1, FromPlugin::Action { request, order });
    s.plugin
        .received()
        .into_iter()
        .find_map(|message| match message {
            ToPlugin::Answer {
                request: answered,
                answer,
            } if answered == request => Some(answer),
            _ => None,
        })
}

fn refused(answer: Option<Answer>) -> ActionError {
    match answer {
        Some(Answer::Failed(error)) => error,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn transfer_sends_the_transfer_packet() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    alice.link.take();

    let answer = perform(&mut s, to(&alice, transfer("play.example.org", 25570)));

    assert_eq!(answer, Some(Answer::Done));
    assert!(alice.link.take().iter().any(|event| matches!(
        event,
        ClientEvent::Transfer { host, port } if host == "play.example.org" && *port == 25570
    )));
}

#[test]
fn transfer_without_a_host_or_a_port_is_an_invalid_argument() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    alice.link.take();

    for order in [transfer("", 25565), transfer("play.example.org", 0)] {
        let answer = perform(&mut s, to(&alice, order));
        assert_eq!(refused(answer).kind, ActionErrorKind::InvalidArgument);
    }
    assert!(
        !alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::Transfer { .. }))
    );
}

#[test]
fn transfer_of_a_player_who_left_reports_the_target_gone() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.server.handle(leave_of(&alice));
    s.server.deliver();

    let answer = perform(&mut s, to(&alice, transfer("play.example.org", 25565)));

    assert_eq!(refused(answer).kind, ActionErrorKind::TargetGone);
}
