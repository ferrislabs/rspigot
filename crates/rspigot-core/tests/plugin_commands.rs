mod common;

use common::*;
use rspigot_core::events::{ClientEvent, Inbound, PlayerInput};
use rspigot_core::plugins::events::{EntityDamageCause, PlayerSpawnChangeCause};
use rspigot_core::plugins::{EventKind, EventPayload, PlayerOrder};
use rspigot_domain::Vec3;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn physics_blocks(s: &Scene) -> Vec<(i32, i32, i32)> {
    settled_for(&s.plugin, EventKind::BlockPhysics)
        .into_iter()
        .filter_map(|fired| match fired.payload {
            EventPayload::BlockPhysics(event) => {
                Some((event.block.x, event.block.y, event.block.z))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn setblock_asks_physics_at_the_changed_block() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::BlockPhysics)]);
    console(&mut s, "setblock 2 64 0 minecraft:stone");
    assert!(physics_blocks(&s).contains(&(2, 64, 0)));
}

#[test]
fn fill_asks_physics_once_per_changed_block() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::BlockPhysics)]);
    console(&mut s, "fill 2 64 0 3 64 0 minecraft:stone");
    let heard = physics_blocks(&s);
    assert_eq!(heard.iter().filter(|b| **b == (2, 64, 0)).count(), 1);
    assert_eq!(heard.iter().filter(|b| **b == (3, 64, 0)).count(), 1);
}

#[test]
fn clone_asks_physics_at_the_destination() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::BlockPhysics)]);
    console(&mut s, "setblock 2 64 0 minecraft:stone");
    console(&mut s, "clone 2 64 0 2 64 0 6 64 0");
    assert!(physics_blocks(&s).contains(&(6, 64, 0)));
}

#[test]
fn spawnpoint_fires_a_command_spawn_change_with_the_new_spawn() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerSpawnChange)]);
    console(&mut s, "spawnpoint Alice 5 64 7");
    let events = settled_for(&s.plugin, EventKind::PlayerSpawnChange);
    assert_eq!(events.len(), 1);
    let EventPayload::PlayerSpawnChange(event) = &events[0].payload else {
        panic!("expected a spawn change");
    };
    assert_eq!(event.cause, PlayerSpawnChangeCause::Command);
    assert!(event.forced);
    let spawn = event.new_spawn.as_ref().expect("a new spawn");
    assert_eq!(spawn.position, Vec3::new(5.0, 64.0, 7.0));
}

fn respawn(s: &mut Scene) -> Vec3 {
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::Kill));
    s.tick(2);
    alice.link.take();
    send_input(&mut s.server, &alice, PlayerInput::Respawn);
    s.server.deliver();
    alice
        .link
        .take()
        .iter()
        .rev()
        .find_map(|event| match event {
            ClientEvent::Teleport { position, .. } => Some(*position),
            _ => None,
        })
        .expect("a respawn teleport")
}

#[test]
fn a_cancelled_spawn_change_keeps_the_world_spawn_for_the_respawn() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerSpawnChange)]);
    console(&mut s, "spawnpoint Alice 5 64 7");
    let asked = last_asked(&s.plugin, EventKind::PlayerSpawnChange);
    rule_on(&mut s.server, 1, asked.id, true, None);
    let spot = respawn(&mut s);
    assert!(spot.distance_squared(Vec3::new(0.5, 64.0, 0.5)) < 1.0e-6);
}

#[test]
fn an_allowed_spawnpoint_sets_the_respawn_position() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerSpawnChange)]);
    console(&mut s, "spawnpoint Alice 5 64 7");
    let spot = respawn(&mut s);
    assert!(spot.distance_squared(Vec3::new(5.5, 64.1, 7.5)) < 1.0e-6);
}

#[test]
fn damage_hurts_a_player_through_the_damage_event() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::EntityDamage)]);
    console(&mut s, "damage Alice 3");
    let heard = settled_for(&s.plugin, EventKind::EntityDamage);
    assert_eq!(heard.len(), 1);
    let EventPayload::EntityDamage(event) = &heard[0].payload else {
        panic!("expected a damage event");
    };
    assert_eq!(event.cause, EntityDamageCause::Custom);
    assert_eq!(event.damage, 3.0);
}

fn ride_scene() -> Scene {
    let mut s = scene_with(
        &["Alice"],
        vec![
            observe(1, EventKind::EntityMount),
            observe(1, EventKind::EntityDismount),
        ],
    );
    console(&mut s, "summon minecraft:cow 5 70 5");
    console(&mut s, "summon minecraft:pig 6 70 5");
    s
}

#[test]
fn ride_mount_and_dismount_fire_the_mount_events() {
    let mut s = ride_scene();
    console(&mut s, "ride @e[type=cow] mount @e[type=pig]");
    assert_eq!(settled_for(&s.plugin, EventKind::EntityMount).len(), 1);
    assert!(settled_for(&s.plugin, EventKind::EntityDismount).is_empty());
    console(&mut s, "ride @e[type=cow] dismount");
    assert_eq!(settled_for(&s.plugin, EventKind::EntityDismount).len(), 1);
}

#[test]
fn a_cancelled_mount_keeps_the_rider_off() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::EntityMount)]);
    console(&mut s, "summon minecraft:cow 5 70 5");
    console(&mut s, "summon minecraft:pig 6 70 5");
    console(&mut s, "ride @e[type=cow] mount @e[type=pig]");
    let asked = last_asked(&s.plugin, EventKind::EntityMount);
    rule_on(&mut s.server, 1, asked.id, true, None);
    console(&mut s, "ride @e[type=cow] dismount");
    assert!(settled_for(&s.plugin, EventKind::EntityDismount).is_empty());
}
