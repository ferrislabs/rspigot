mod common;

use common::*;
use rspigot_core::events::{DigAction, Direction, Inbound, PlayerInput};
use rspigot_core::plugins::{EventKind, EventPayload};
use rspigot_domain::BlockPos;
use rspigot_entities::EntityKind as Kind;

fn drop_one(s: &mut Scene, player: &Player) {
    send_input(
        &mut s.server,
        player,
        PlayerInput::Dig {
            action: DigAction::DropItem,
            pos: BlockPos::new(0, 0, 0),
            face: Direction::Up,
            sequence: 3,
        },
    );
}

fn item_drop_id(s: &Scene, from: &rspigot_core::plugins::EventPayload) -> rspigot_domain::EntityId {
    let _ = s;
    match from {
        EventPayload::PlayerDropItem(drop) => drop.item_drop.id,
        other => panic!("expected a drop, got {other:?}"),
    }
}

#[test]
fn the_item_drop_handle_is_the_entity_that_spawns_once_the_drop_is_allowed() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerDropItem)]);
    let alice = s.players[0].clone();
    s.server
        .handle(Inbound::Console("give Alice diamond 3".to_owned()));
    s.server.deliver();
    drop_one(&mut s, &alice);
    let asked = last_asked(&s.plugin, EventKind::PlayerDropItem);
    let handle = item_drop_id(&s, &asked.payload);
    assert!(s.server.entities_of(Kind::Item).is_empty());
    rule_on(&mut s.server, 1, asked.id, false, None);
    let spawned = s.server.entities_of(Kind::Item);
    assert_eq!(spawned.len(), 1);
    assert_eq!(spawned[0].id, handle);
}

#[test]
fn a_cancelled_drop_never_spawns_the_handled_entity() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerDropItem)]);
    let alice = s.players[0].clone();
    s.server
        .handle(Inbound::Console("give Alice diamond 3".to_owned()));
    s.server.deliver();
    drop_one(&mut s, &alice);
    let asked = last_asked(&s.plugin, EventKind::PlayerDropItem);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(s.server.entities_of(Kind::Item).is_empty());
}
