use rspigot_domain::{Identifier, Vec3};

use crate::end_tests::arrive;
use crate::events::ClientEvent;
use crate::plugins::PlayerOrder;

fn respawns(events: &[ClientEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, ClientEvent::Respawn { .. }))
        .count()
}

#[test]
fn a_plugin_teleport_to_the_end_changes_world() {
    let (mut server, id, link) = arrive();
    server.apply_player_order(
        1,
        id,
        PlayerOrder::TeleportTo {
            world: "minecraft:the_end".to_owned(),
            position: Vec3::new(0.5, 80.0, 0.5),
            rotation: None,
        },
    );
    server.deliver();
    assert_eq!(
        server.player_dimension(id),
        Some(Identifier::minecraft("the_end"))
    );
    assert_eq!(respawns(&link.take()), 1);
}

#[test]
fn an_empty_world_keeps_the_current_one() {
    let (mut server, id, link) = arrive();
    server.apply_player_order(
        1,
        id,
        PlayerOrder::TeleportTo {
            world: String::new(),
            position: Vec3::new(5.5, 80.0, 5.5),
            rotation: None,
        },
    );
    server.deliver();
    assert_eq!(
        server.player_dimension(id),
        Some(Identifier::minecraft("overworld"))
    );
    assert_eq!(respawns(&link.take()), 0);
}

#[test]
fn an_unknown_world_leaves_the_player_where_they_are() {
    let (mut server, id, link) = arrive();
    server.apply_player_order(
        1,
        id,
        PlayerOrder::TeleportTo {
            world: "minecraft:nowhere".to_owned(),
            position: Vec3::new(0.5, 80.0, 0.5),
            rotation: None,
        },
    );
    server.deliver();
    assert_eq!(
        server.player_dimension(id),
        Some(Identifier::minecraft("overworld"))
    );
    assert_eq!(respawns(&link.take()), 0);
}
