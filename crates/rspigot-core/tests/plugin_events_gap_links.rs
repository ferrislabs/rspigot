mod common;

use common::*;
use rspigot_core::client_extras::ServerLink;
use rspigot_core::events::ClientEvent;
use rspigot_core::plugins::{EventKind, EventPayload};

#[test]
fn sending_server_links_fires_player_links_send_with_the_player() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerLinksSend)]);
    let alice = s.players[0].clone();
    s.server
        .send_server_links(
            alice.id,
            vec![ServerLink::labelled("website", "https://example.org")],
        )
        .expect("Alice is online");
    s.tick(1);
    let events = settled_for(&s.plugin, EventKind::PlayerLinksSend);
    assert_eq!(events.len(), 1);
    let EventPayload::PlayerLinksSend(event) = &events[0].payload else {
        panic!("expected a PlayerLinksSend");
    };
    assert_eq!(event.player.name, "Alice");
    assert_eq!(event.links.links.len(), 1);
    assert_eq!(event.links.links[0].label, "website");
    assert_eq!(event.links.links[0].url, "https://example.org");
    assert!(
        alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::ServerLinks(links) if links.len() == 1))
    );
}
