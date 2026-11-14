mod common;

use std::sync::Arc;

use common::*;
use rspigot_core::events::ClientEvent;
use rspigot_core::plugins::events::{PlayerChangedWorld, PlayerPortalPatch, PlayerTeleportCause};
use rspigot_core::plugins::{EventKind, EventPatch, EventPayload, Location};
use rspigot_domain::world::Dimension;
use rspigot_domain::{Identifier, Rotation, Vec3};

fn nether() -> Identifier {
    Identifier::minecraft("the_nether")
}

fn end() -> Identifier {
    Identifier::minecraft("the_end")
}

fn scene_with_dimensions(handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> Scene {
    let mut server = configured(|_| {});
    for (key, dimension) in [(nether(), Dimension::nether()), (end(), Dimension::end())] {
        server.load_dimension(key, dimension, Arc::new(Floor), Arc::new(NoStore));
    }
    scene_on(server, &["Alice"], handlers)
}

fn travel(s: &mut Scene, to: Identifier) {
    let alice = s.players[0].clone();
    s.server.change_dimension(
        alice.id,
        to,
        Vec3::new(8.5, 70.0, 8.5),
        None,
        PlayerTeleportCause::NetherPortal,
    );
    s.server.deliver();
}

#[test]
fn a_portal_cancelled_by_a_plugin_keeps_the_player_home() {
    let mut s = scene_with_dimensions(vec![deciding(1, EventKind::PlayerPortal)]);
    let alice = s.players[0].clone();
    travel(&mut s, nether());
    let asked = last_asked(&s.plugin, EventKind::PlayerPortal);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(
        s.server.player_dimension(alice.id),
        Some(Identifier::minecraft("overworld"))
    );
    assert!(
        !alice
            .link
            .take()
            .iter()
            .any(|e| matches!(e, ClientEvent::Respawn { .. }))
    );
}

#[test]
fn a_portal_destination_changed_by_a_plugin_applies_and_announces_the_new_world() {
    let mut s = scene_with_dimensions(vec![
        deciding(1, EventKind::PlayerPortal),
        observe(2, EventKind::PlayerChangedWorld),
    ]);
    let alice = s.players[0].clone();
    travel(&mut s, nether());
    let asked = last_asked(&s.plugin, EventKind::PlayerPortal);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::PlayerPortal(PlayerPortalPatch {
            to: Some(Location {
                world: end().to_string(),
                position: Vec3::new(100.5, 49.0, 0.5),
                rotation: Rotation::default(),
            }),
            ..PlayerPortalPatch::default()
        })),
    );
    assert_eq!(s.server.player_dimension(alice.id), Some(end()));
    assert_eq!(
        s.server.player_location(alice.id),
        Some(Vec3::new(100.5, 49.0, 0.5))
    );
    let changed = settled_for(&s.plugin, EventKind::PlayerChangedWorld);
    let EventPayload::PlayerChangedWorld(PlayerChangedWorld { from, player }) =
        &changed.last().expect("announced").payload
    else {
        panic!("expected a world change");
    };
    assert_eq!(from, "minecraft:overworld");
    assert_eq!(player.world, end().to_string());
}
