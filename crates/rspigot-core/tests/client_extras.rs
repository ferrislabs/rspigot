mod common;

use common::*;
use rspigot_core::client_extras::{
    ClientExtraError, CookieReply, KnownLink, LinkLabel, MAX_COOKIE_BYTES, ResourcePackOffer,
    ServerLink,
};
use rspigot_core::events::{ClientEvent, PlayerInput};
use rspigot_core::plugins::events::ResourcePackStatus;
use rspigot_domain::text::Component;
use rspigot_domain::{Rotation, Vec3};
use uuid::Uuid;

fn offer(required: bool) -> ResourcePackOffer {
    ResourcePackOffer {
        id: Uuid::from_u128(9),
        url: "https://example.org/pack.zip".to_owned(),
        hash: "0123456789abcdef0123456789abcdef01234567".to_owned(),
        required,
        prompt: Some(Component::text("Please")),
    }
}

fn answer(s: &mut Scene, status: ResourcePackStatus) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::ResourcePack {
            id: Uuid::from_u128(9),
            status,
        },
    );
}

#[test]
fn a_pushed_pack_reaches_the_player() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    alice.link.take();
    s.server.send_resource_pack(alice.id, offer(false)).unwrap();
    s.server.deliver();
    assert!(alice.link.take().iter().any(|event| matches!(
        event,
        ClientEvent::ResourcePackPush(pack) if pack.url == "https://example.org/pack.zip" && !pack.required
    )));
}

#[test]
fn a_pack_for_an_unknown_player_is_refused() {
    let mut s = scene(&["Alice"]);
    assert_eq!(
        s.server.send_resource_pack(
            rspigot_domain::player::PlayerId(Uuid::from_u128(77)),
            offer(false)
        ),
        Err(ClientExtraError::UnknownPlayer)
    );
}

#[test]
fn a_declined_required_pack_kicks_the_player() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.server.send_resource_pack(alice.id, offer(true)).unwrap();
    s.server.deliver();
    answer(&mut s, ResourcePackStatus::Accepted);
    assert!(alice.link.disconnected_with().is_none());
    answer(&mut s, ResourcePackStatus::Declined);
    assert_eq!(
        alice.link.disconnected_with(),
        Some(Component::translatable(
            "multiplayer.requiredTexturePrompt.disconnect",
            Vec::new()
        ))
    );
}

#[test]
fn a_failed_download_of_a_required_pack_kicks_the_player() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.server.send_resource_pack(alice.id, offer(true)).unwrap();
    answer(&mut s, ResourcePackStatus::FailedDownload);
    assert!(alice.link.disconnected_with().is_some());
}

#[test]
fn a_declined_optional_pack_keeps_the_player() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.server.send_resource_pack(alice.id, offer(false)).unwrap();
    answer(&mut s, ResourcePackStatus::Declined);
    assert!(alice.link.disconnected_with().is_none());
}

#[test]
fn a_loaded_required_pack_keeps_the_player() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.server.send_resource_pack(alice.id, offer(true)).unwrap();
    answer(&mut s, ResourcePackStatus::SuccessfullyLoaded);
    answer(&mut s, ResourcePackStatus::Discarded);
    assert!(alice.link.disconnected_with().is_none());
}

#[test]
fn a_popped_required_pack_is_no_longer_enforced() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.server.send_resource_pack(alice.id, offer(true)).unwrap();
    alice.link.take();
    s.server
        .clear_resource_packs(alice.id, Some(Uuid::from_u128(9)))
        .unwrap();
    s.server.deliver();
    assert!(alice.link.take().iter().any(
        |event| matches!(event, ClientEvent::ResourcePackPop(Some(id)) if *id == Uuid::from_u128(9))
    ));
    answer(&mut s, ResourcePackStatus::Declined);
    assert!(alice.link.disconnected_with().is_none());
}

#[test]
fn server_links_reach_the_player_with_the_bug_report_link() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    alice.link.take();
    let links = vec![
        ServerLink {
            label: LinkLabel::Known(KnownLink::BugReport),
            url: "https://example.org/bugs".to_owned(),
        },
        ServerLink {
            label: LinkLabel::Custom(Component::text("Shop")),
            url: "https://example.org/shop".to_owned(),
        },
    ];
    s.server.send_server_links(alice.id, links.clone()).unwrap();
    s.server.deliver();
    assert!(
        alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::ServerLinks(sent) if *sent == links))
    );
}

#[test]
fn a_stored_cookie_reaches_the_player() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    alice.link.take();
    s.server
        .store_cookie(alice.id, "shop:session", vec![1, 2, 3])
        .unwrap();
    s.server.deliver();
    assert!(alice.link.take().iter().any(|event| matches!(
        event,
        ClientEvent::StoreCookie { key, payload } if key == "shop:session" && payload == &[1, 2, 3]
    )));
}

#[test]
fn a_cookie_over_the_size_limit_is_refused() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    alice.link.take();
    assert_eq!(
        s.server
            .store_cookie(alice.id, "shop:session", vec![0; MAX_COOKIE_BYTES + 1]),
        Err(ClientExtraError::CookieTooLarge)
    );
    assert!(
        s.server
            .store_cookie(alice.id, "shop:session", vec![0; MAX_COOKIE_BYTES])
            .is_ok()
    );
    s.server.deliver();
    let sent = alice.link.take();
    assert_eq!(
        sent.iter()
            .filter(|event| matches!(event, ClientEvent::StoreCookie { .. }))
            .count(),
        1
    );
}

#[test]
fn a_cookie_key_must_be_an_identifier() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    for key in ["Bad Key", "", "a:b:c", "UPPER:case"] {
        assert_eq!(
            s.server.store_cookie(alice.id, key, vec![1]),
            Err(ClientExtraError::InvalidCookieKey),
            "{key}"
        );
        assert_eq!(
            s.server.request_cookie(alice.id, key),
            Err(ClientExtraError::InvalidCookieKey),
            "{key}"
        );
    }
}

#[test]
fn a_cookie_request_and_its_response_round_trip() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    alice.link.take();
    s.server.request_cookie(alice.id, "shop:session").unwrap();
    s.server.deliver();
    assert!(
        alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::CookieRequest(key) if key == "shop:session"))
    );
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::CookieResponse {
            key: "shop:session".to_owned(),
            payload: Some(vec![4, 5]),
        },
    );
    assert_eq!(
        s.server.take_cookie_replies(),
        vec![CookieReply {
            player: alice.id,
            key: "shop:session".to_owned(),
            payload: Some(vec![4, 5]),
        }]
    );
    assert!(s.server.take_cookie_replies().is_empty());
}

#[test]
fn an_unrequested_cookie_response_is_dropped() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::CookieResponse {
            key: "shop:session".to_owned(),
            payload: None,
        },
    );
    assert!(s.server.take_cookie_replies().is_empty());
}

#[test]
fn a_missing_cookie_answers_with_no_payload() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.server.request_cookie(alice.id, "shop:session").unwrap();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::CookieResponse {
            key: "shop:session".to_owned(),
            payload: None,
        },
    );
    let replies = s.server.take_cookie_replies();
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0].payload, None);
}

#[test]
fn report_details_reach_the_player_within_the_limits() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    alice.link.take();
    s.server
        .send_report_details(alice.id, vec![("server".to_owned(), "rspigot".to_owned())])
        .unwrap();
    s.server.deliver();
    assert!(alice.link.take().iter().any(|event| matches!(
        event,
        ClientEvent::ReportDetails(details) if details == &[("server".to_owned(), "rspigot".to_owned())]
    )));
    let too_many: Vec<(String, String)> = (0..33).map(|i| (i.to_string(), String::new())).collect();
    assert_eq!(
        s.server.send_report_details(alice.id, too_many),
        Err(ClientExtraError::ReportDetailsTooLarge)
    );
    assert_eq!(
        s.server
            .send_report_details(alice.id, vec![("k".repeat(129), String::new())]),
        Err(ClientExtraError::ReportDetailsTooLarge)
    );
}

#[test]
fn reconfiguring_goes_play_to_configuration_to_play() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    alice.link.take();
    s.server.reconfigure(alice.id).unwrap();
    s.server.deliver();
    assert!(
        alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::Reconfigure))
    );
    assert!(s.server.is_reconfiguring(alice.id));
    assert_eq!(
        s.server.reconfigure(alice.id),
        Err(ClientExtraError::AlreadyReconfiguring)
    );

    let before = s.server.player_position(alice.id).unwrap();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: Some(Vec3::new(before.x + 3.0, before.y, before.z)),
            rotation: Some(Rotation::new(0.0, 0.0)),
            on_ground: true,
        },
    );
    assert_eq!(s.server.player_position(alice.id), Some(before));

    send_input(&mut s.server, &alice, PlayerInput::Reconfigured);
    assert!(!s.server.is_reconfiguring(alice.id));
    assert!(
        alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::Join(_)))
    );
    assert!(s.server.player_position(alice.id).is_some());
}

#[test]
fn reconfiguring_an_unknown_player_is_refused() {
    let mut s = scene(&["Alice"]);
    assert_eq!(
        s.server
            .reconfigure(rspigot_domain::player::PlayerId(Uuid::from_u128(5))),
        Err(ClientExtraError::UnknownPlayer)
    );
}
