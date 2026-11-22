mod atlas;
mod common;

use std::sync::Arc;

use atlas::*;
use common::*;
use rspigot_core::events::{Hand, PlayerInput};
use rspigot_core::plugins::{EventKind, EventPayload};
use rspigot_domain::Rotation;

fn scene_observing_maps() -> Scene {
    let atlas = Arc::new(Atlas::default());
    let server = meadow_server(&atlas, Meadow::default());
    let mut s = scene_on(
        server,
        &["Alice"],
        vec![observe(1, EventKind::MapInitialize)],
    );
    console(&mut s, "gamemode survival Alice");
    s
}

fn use_empty_map(s: &mut Scene) {
    console(s, "give Alice minecraft:map 2");
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: Rotation::new(0.0, 0.0),
        },
    );
    s.tick(1);
}

fn initialised(s: &Scene) -> Vec<i32> {
    settled_for(&s.plugin, EventKind::MapInitialize)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::MapInitialize(map) => Some(map.map_id),
            _ => None,
        })
        .collect()
}

#[test]
fn a_new_map_fires_map_initialize_with_its_id() {
    let mut s = scene_observing_maps();
    assert!(initialised(&s).is_empty());
    use_empty_map(&mut s);
    assert_eq!(initialised(&s).len(), 1);
}

#[test]
fn a_second_new_map_fires_again_with_another_id() {
    let mut s = scene_observing_maps();
    use_empty_map(&mut s);
    use_empty_map(&mut s);
    let ids = initialised(&s);
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
}
