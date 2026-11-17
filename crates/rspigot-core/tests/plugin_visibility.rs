mod common;

use common::*;
use rspigot_core::events::{ClientEvent, Inbound};
use rspigot_core::plugins::{Answer, FromPlugin, PlayerOrder, PlayerState, Question};
use rspigot_domain::EntityId;
use rspigot_domain::player::PlayerId;

fn state(s: &mut Scene, player: &Player) -> PlayerState {
    let Answer::Player(state) = s.ask(Question::Player(player.id)) else {
        panic!("expected the state of the player");
    };
    state
}

fn spawned(viewer: &Player, target: &Player) -> Option<EntityId> {
    viewer
        .link
        .take()
        .into_iter()
        .find_map(|event| match event {
            ClientEvent::SpawnEntity { entity, uuid, .. } if uuid == target.id.0 => Some(entity),
            _ => None,
        })
}

fn meet() -> (Scene, Player, Player, EntityId) {
    let mut s = scene(&["Alice", "Bob"]);
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    let mut bob_entity = None;
    let mut bob_sees_alice = false;
    for _ in 0..40 {
        if bob_entity.is_some() && bob_sees_alice {
            break;
        }
        s.tick(1);
        bob_entity = bob_entity.or_else(|| spawned(&alice, &bob));
        bob_sees_alice |= spawned(&bob, &alice).is_some();
    }
    assert!(bob_sees_alice, "Bob sees Alice");
    let bob_entity = bob_entity.expect("Alice sees Bob");
    (s, alice, bob, bob_entity)
}

fn hide(s: &mut Scene, connection: u64, viewer: &Player, target: PlayerId) {
    order(
        &mut s.server,
        connection,
        FromPlugin::Order(to(viewer, PlayerOrder::HidePlayer(target))),
    );
}

fn show(s: &mut Scene, connection: u64, viewer: &Player, target: PlayerId) {
    order(
        &mut s.server,
        connection,
        FromPlugin::Order(to(viewer, PlayerOrder::ShowPlayer(target))),
    );
}

fn listed(viewer: &Player) -> Vec<PlayerId> {
    viewer
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::PlayerInfo(entries) => Some(entries),
            _ => None,
        })
        .flatten()
        .map(|entry| entry.profile.id)
        .collect()
}

#[test]
fn a_hidden_player_leaves_the_tab_list_and_is_despawned() {
    let (mut s, alice, bob, bob_entity) = meet();
    bob.link.take();
    s.act(to(&alice, PlayerOrder::HidePlayer(bob.id)));
    assert_eq!(
        alice.link.take(),
        [
            ClientEvent::RemoveEntities(vec![bob_entity]),
            ClientEvent::PlayerInfoRemove(vec![bob.id]),
        ]
    );
    s.tick(5);
    assert_eq!(spawned(&alice, &bob), None);
    assert!(!bob.link.take().iter().any(|event| matches!(
        event,
        ClientEvent::RemoveEntities(_) | ClientEvent::PlayerInfoRemove(_)
    )));
    assert!(s.warnings().is_empty());
}

#[test]
fn show_brings_the_player_back() {
    let (mut s, alice, bob, _) = meet();
    s.act(to(&alice, PlayerOrder::HidePlayer(bob.id)));
    alice.link.take();
    s.act(to(&alice, PlayerOrder::ShowPlayer(bob.id)));
    let events = alice.link.take();
    let [ClientEvent::PlayerInfo(entries)] = events.as_slice() else {
        panic!("expected Bob's tab-list entry, got {events:?}");
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].profile.id, bob.id);
    assert_eq!(entries[0].profile.name, "Bob");
    assert!(entries[0].listed);
    s.tick(1);
    assert!(spawned(&alice, &bob).is_some());
    assert!(s.warnings().is_empty());
}

#[test]
fn two_plugins_hiding_need_two_shows() {
    let (mut s, alice, bob, _) = meet();
    let other = connect(&mut s.server, 2, "other");
    hide(&mut s, 1, &alice, bob.id);
    hide(&mut s, 2, &alice, bob.id);
    let removals = alice
        .link
        .take()
        .into_iter()
        .filter(|event| *event == ClientEvent::PlayerInfoRemove(vec![bob.id]))
        .count();
    assert_eq!(removals, 1);

    show(&mut s, 1, &alice, bob.id);
    assert!(alice.link.take().is_empty());
    s.tick(1);
    assert_eq!(spawned(&alice, &bob), None);

    show(&mut s, 2, &alice, bob.id);
    assert_eq!(listed(&alice), [bob.id]);
    s.tick(1);
    assert!(spawned(&alice, &bob).is_some());
    assert!(s.warnings().is_empty());
    assert!(heard(&other).is_empty());
}

#[test]
fn a_hide_survives_a_reconnect_and_ends_when_a_player_leaves() {
    let (mut s, alice, bob, _) = meet();
    s.act(to(&alice, PlayerOrder::HidePlayer(bob.id)));
    assert!(
        alice
            .link
            .take()
            .contains(&ClientEvent::PlayerInfoRemove(vec![bob.id]))
    );
    s.server
        .handle(Inbound::PluginDisconnected { connection: 1 });
    connect(&mut s.server, 3, "tester");
    s.tick(3);
    assert_eq!(spawned(&alice, &bob), None);
    hide(&mut s, 3, &alice, bob.id);
    assert!(alice.link.take().is_empty());

    s.server.handle(leave_of(&bob));
    s.server.deliver();
    let bob = join(&mut s.server, "Bob");
    s.tick(1);
    assert!(spawned(&alice, &bob).is_some());
}

#[test]
fn resolve_lists_the_hidden_players() {
    let (mut s, alice, bob, _) = meet();
    assert!(state(&mut s, &alice).hidden.is_empty());
    s.act(to(&alice, PlayerOrder::HidePlayer(bob.id)));
    assert_eq!(state(&mut s, &alice).hidden, [bob.id]);
    assert!(state(&mut s, &bob).hidden.is_empty());

    alice.link.take();
    s.act(to(&alice, PlayerOrder::HidePlayer(alice.id)));
    s.act(to(
        &alice,
        PlayerOrder::HidePlayer(PlayerId::offline("Carol")),
    ));
    assert!(alice.link.take().is_empty());
    assert_eq!(state(&mut s, &alice).hidden, [bob.id]);
    let warnings = s.warnings();
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(warnings[0].contains("cannot be hidden from themselves"));
    assert!(warnings[1].contains("the player to hide is not online"));
}

fn hears_sound(listener: &Player) -> bool {
    listener
        .link
        .take()
        .iter()
        .any(|event| matches!(event, ClientEvent::Sound { .. }))
}

#[test]
fn a_hidden_player_is_not_heard() {
    let mut s = scene(&["Alice", "Bob", "Carol"]);
    let (alice, bob, carol) = (
        s.players[0].clone(),
        s.players[1].clone(),
        s.players[2].clone(),
    );
    s.act(to(&alice, PlayerOrder::HidePlayer(bob.id)));
    alice.link.take();
    carol.link.take();
    s.act(to(&bob, PlayerOrder::Damage(1.0)));
    assert!(!hears_sound(&alice));
    assert!(hears_sound(&carol));

    s.act(to(&alice, PlayerOrder::ShowPlayer(bob.id)));
    s.tick(20);
    alice.link.take();
    carol.link.take();
    s.act(to(&bob, PlayerOrder::Damage(1.0)));
    assert!(hears_sound(&alice));
    assert!(hears_sound(&carol));
    assert!(s.warnings().is_empty());
}
