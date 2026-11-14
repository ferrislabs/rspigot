mod common;

use common::*;
use rspigot_core::events::PlayerInput;
use rspigot_core::plugins::events::{
    PlayerCustomClick, PlayerResourcePackStatus, ResourcePackStatus,
};
use rspigot_core::plugins::{EventKind, EventPayload};
use uuid::Uuid;

#[test]
fn a_resource_pack_answer_reaches_the_plugins_with_its_pack_and_status() {
    let mut s = scene_with(
        &["Alice"],
        vec![observe(1, EventKind::PlayerResourcePackStatus)],
    );
    let alice = s.players[0].clone();
    let pack = Uuid::from_u128(7);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::ResourcePack {
            id: pack,
            status: ResourcePackStatus::Declined,
        },
    );
    let events = fired(&s.plugin);
    assert_eq!(events.len(), 1);
    let EventPayload::PlayerResourcePackStatus(PlayerResourcePackStatus {
        pack_id,
        player,
        status,
    }) = &events[0].payload
    else {
        panic!("expected a pack status, got {:?}", events[0]);
    };
    assert_eq!(*pack_id, pack);
    assert_eq!(player.id, alice.id);
    assert_eq!(*status, ResourcePackStatus::Declined);
}

#[test]
fn a_custom_click_reaches_the_plugins_with_its_id_and_data() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerCustomClick)]);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::CustomClick {
            id: "shop:buy".to_owned(),
            data: Some("{\"n\":1}".to_owned()),
        },
    );
    let events = fired(&s.plugin);
    assert_eq!(events.len(), 1);
    let EventPayload::PlayerCustomClick(PlayerCustomClick {
        data,
        click_id,
        player,
    }) = &events[0].payload
    else {
        panic!("expected a custom click, got {:?}", events[0]);
    };
    assert_eq!(click_id, "shop:buy");
    assert_eq!(data.as_deref(), Some("{\"n\":1}"));
    assert_eq!(player.id, alice.id);
}
