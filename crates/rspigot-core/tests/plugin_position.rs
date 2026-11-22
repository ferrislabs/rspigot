mod common;

use common::*;
use rspigot_core::events::{ClientEvent, Inbound, PlayerInput};
use rspigot_core::plugins::events::{
    EntityDeathPatch, PlayerDeath, PlayerDeathPatch, PlayerMovePatch, PlayerRespawn,
    PlayerRespawnPatch, PlayerRespawnReason, PlayerTeleport, PlayerTeleportCause,
    PlayerTeleportPatch, PlayerVelocity, PlayerVelocityPatch,
};
use rspigot_core::plugins::{
    Answer, EventKind, EventPatch, EventPayload, HandlerSpec, ItemSpec, ListDiff, Location,
    PlayerOrder, Priority, Question,
};
use rspigot_domain::{EntityId, Rotation, Vec3};

const SPAWN: Vec3 = Vec3::new(0.5, 64.0, 0.5);

fn at(x: f64, z: f64) -> Vec3 {
    Vec3::new(x, 64.0, z)
}

fn location(position: Vec3) -> Location {
    Location {
        world: "minecraft:overworld".to_owned(),
        position,
        rotation: Rotation::default(),
    }
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn teleports(player: &Player) -> Vec<(Vec3, Rotation)> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Teleport {
                position, rotation, ..
            } => Some((position, rotation)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_command_teleport_fires_with_its_cause_and_can_be_cancelled() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerTeleport)]);
    let alice = s.players[0].clone();
    console(&mut s, "tp Alice 10.5 64 0.5");
    assert_eq!(s.server.player_location(alice.id), Some(SPAWN));
    let asked = last_asked(&s.plugin, EventKind::PlayerTeleport);
    let EventPayload::PlayerTeleport(PlayerTeleport {
        cause, from, to, ..
    }) = &asked.payload
    else {
        panic!("expected a teleport");
    };
    assert_eq!(*cause, PlayerTeleportCause::Command);
    assert_eq!(from.position, SPAWN);
    assert_eq!(to.as_ref().map(|to| to.position), Some(at(10.5, 0.5)));
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(s.server.player_location(alice.id), Some(SPAWN));
    assert!(teleports(&alice).is_empty());
}

#[test]
fn a_changed_teleport_destination_and_rotation_apply() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerTeleport)]);
    let alice = s.players[0].clone();
    console(&mut s, "tp Alice 10.5 64 0.5");
    let asked = last_asked(&s.plugin, EventKind::PlayerTeleport);
    let mut elsewhere = location(at(3.5, 7.5));
    elsewhere.rotation = Rotation::new(90.0, 10.0);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::PlayerTeleport(PlayerTeleportPatch {
            from: None,
            to: Some(elsewhere),
        })),
    );
    assert_eq!(s.server.player_location(alice.id), Some(at(3.5, 7.5)));
    assert_eq!(
        teleports(&alice),
        [(at(3.5, 7.5), Rotation::new(90.0, 10.0))]
    );
}

#[test]
fn a_plugin_teleport_reaches_the_calling_plugin_as_a_plugin_cause() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerTeleport)]);
    let alice = s.players[0].clone();
    s.act(to(
        &alice,
        PlayerOrder::TeleportFacing(at(5.5, 0.5), Rotation::new(45.0, 0.0)),
    ));
    let asked = last_asked(&s.plugin, EventKind::PlayerTeleport);
    let EventPayload::PlayerTeleport(PlayerTeleport { cause, to, .. }) = &asked.payload else {
        panic!("expected a teleport");
    };
    assert_eq!(*cause, PlayerTeleportCause::Plugin);
    assert_eq!(
        to.as_ref().map(|to| to.rotation),
        Some(Rotation::new(45.0, 0.0))
    );
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert_eq!(s.server.player_location(alice.id), Some(at(5.5, 0.5)));
    assert_eq!(
        teleports(&alice),
        [(at(5.5, 0.5), Rotation::new(45.0, 0.0))]
    );
}

#[test]
fn a_respawn_does_not_fire_a_teleport() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerTeleport)]);
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::SetHealth(0.0)));
    send_input(&mut s.server, &alice, PlayerInput::Respawn);
    assert_eq!(s.server.player_is_dead(alice.id), Some(false));
    assert!(settled_for(&s.plugin, EventKind::PlayerTeleport).is_empty());
}

#[test]
fn a_changed_move_destination_fires_a_plugin_teleport_whose_cancel_keeps_the_move() {
    let mut s = scene_with(
        &["Alice"],
        vec![decide(1), deciding(2, EventKind::PlayerTeleport)],
    );
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: Some(at(1.5, 0.5)),
            rotation: None,
            on_ground: true,
        },
    );
    let moved = last_asked(&s.plugin, EventKind::PlayerMove);
    rule_on(
        &mut s.server,
        1,
        moved.id,
        false,
        Some(EventPatch::PlayerMove(PlayerMovePatch {
            to: Some(location(at(8.5, 0.5))),
        })),
    );
    let teleport = last_asked(&s.plugin, EventKind::PlayerTeleport);
    let EventPayload::PlayerTeleport(PlayerTeleport { cause, to, .. }) = &teleport.payload else {
        panic!("expected a teleport");
    };
    assert_eq!(*cause, PlayerTeleportCause::Plugin);
    assert_eq!(to.as_ref().map(|to| to.position), Some(at(8.5, 0.5)));
    assert_eq!(s.server.player_location(alice.id), Some(SPAWN));
    judge(&mut s.server, 1, teleport.id, 2, true, None);
    assert_eq!(s.server.player_location(alice.id), Some(at(1.5, 0.5)));
}

fn velocities(player: &Player, entity: EntityId) -> Vec<Vec3> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::EntityVelocity {
                entity: moved,
                velocity,
            } if moved == entity => Some(velocity),
            _ => None,
        })
        .collect()
}

fn push(s: &mut Scene, player: &Player, y: f64) {
    s.act(to(player, PlayerOrder::SetVelocity(Vec3::new(0.0, y, 0.0))));
}

#[test]
fn a_velocity_without_a_decider_is_sent_at_once_and_only_observed() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerVelocity)]);
    let alice = s.players[0].clone();
    let entity = s.server.player_entity_id(alice.id).expect("an entity");
    push(&mut s, &alice, 0.5);
    s.tick(1);
    assert_eq!(velocities(&alice, entity), [Vec3::new(0.0, 0.5, 0.0)]);
    let heard = settled_for(&s.plugin, EventKind::PlayerVelocity);
    assert_eq!(heard.len(), 1);
    assert_eq!(
        heard[0].payload,
        EventPayload::PlayerVelocity(PlayerVelocity {
            player: velocity_player(&heard[0].payload),
            velocity: Vec3::new(0.0, 0.5, 0.0),
        })
    );
    s.tick(3);
    assert_eq!(settled_for(&s.plugin, EventKind::PlayerVelocity).len(), 1);
}

fn velocity_player(payload: &EventPayload) -> rspigot_core::plugins::PlayerSnapshot {
    match payload {
        EventPayload::PlayerVelocity(PlayerVelocity { player, .. }) => player.clone(),
        other => panic!("expected a velocity, got {other:?}"),
    }
}

#[test]
fn a_cancelled_velocity_is_asked_again_but_never_twice_at_once() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerVelocity)]);
    let alice = s.players[0].clone();
    let entity = s.server.player_entity_id(alice.id).expect("an entity");
    push(&mut s, &alice, 0.5);
    s.server.tick();
    s.server.tick();
    s.server.deliver();
    let asked = asked_for(&s.plugin, EventKind::PlayerVelocity);
    assert_eq!(asked.len(), 1);
    rule_on(&mut s.server, 1, asked[0].id, true, None);
    assert!(velocities(&alice, entity).is_empty());
    s.server.tick();
    s.server.deliver();
    let asked = asked_for(&s.plugin, EventKind::PlayerVelocity);
    assert_eq!(asked.len(), 2);
    rule_on(
        &mut s.server,
        1,
        asked[1].id,
        false,
        Some(EventPatch::PlayerVelocity(PlayerVelocityPatch {
            velocity: Some(Vec3::new(0.0, 1.5, 0.0)),
        })),
    );
    assert_eq!(velocities(&alice, entity), [Vec3::new(0.0, 1.5, 0.0)]);
    s.tick(2);
    assert_eq!(asked_for(&s.plugin, EventKind::PlayerVelocity).len(), 2);
}

#[test]
fn a_hit_player_is_asked_with_the_velocity_before_the_hit() {
    let mut s = scene_with(
        &["Alice", "Bob"],
        vec![deciding(1, EventKind::PlayerVelocity)],
    );
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    let entity = s.server.player_entity_id(bob.id).expect("an entity");
    send_input(
        &mut s.server,
        &bob,
        PlayerInput::Move {
            position: Some(at(1.5, 0.5)),
            rotation: None,
            on_ground: true,
        },
    );
    s.tick(30);
    bob.link.take();
    send_input(&mut s.server, &alice, PlayerInput::Attack(entity));
    let asked = last_asked(&s.plugin, EventKind::PlayerVelocity);
    let EventPayload::PlayerVelocity(PlayerVelocity { velocity, player }) = &asked.payload else {
        panic!("expected a velocity");
    };
    assert_eq!(player.id, bob.id);
    assert_eq!(*velocity, Vec3::ZERO);
    assert!(velocities(&bob, entity).is_empty());
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::PlayerVelocity(PlayerVelocityPatch {
            velocity: Some(Vec3::new(0.0, 2.0, 0.0)),
        })),
    );
    assert_eq!(velocities(&bob, entity), [Vec3::new(0.0, 2.0, 0.0)]);
}

fn kill(s: &mut Scene, player: &Player) {
    s.act(to(player, PlayerOrder::SetHealth(0.0)));
    assert_eq!(s.server.player_is_dead(player.id), Some(true));
}

#[test]
fn a_respawn_decider_moves_the_respawn_location() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerRespawn)]);
    console(&mut s, "gamerule respawn_radius 0");
    let alice = s.players[0].clone();
    kill(&mut s, &alice);
    send_input(&mut s.server, &alice, PlayerInput::Respawn);
    assert_eq!(s.server.player_is_dead(alice.id), Some(true));
    let asked = last_asked(&s.plugin, EventKind::PlayerRespawn);
    let EventPayload::PlayerRespawn(PlayerRespawn {
        respawn_location,
        respawn_reason,
        bed_spawn,
        anchor_spawn,
        ..
    }) = &asked.payload
    else {
        panic!("expected a respawn");
    };
    assert_eq!(respawn_location.position, SPAWN);
    assert_eq!(*respawn_reason, PlayerRespawnReason::Death);
    assert!(!bed_spawn && !anchor_spawn);
    let mut there = location(at(20.5, 4.5));
    there.rotation = Rotation::new(180.0, 0.0);
    send_input(&mut s.server, &alice, PlayerInput::Respawn);
    alice.link.take();
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::PlayerRespawn(PlayerRespawnPatch {
            respawn_location: Some(there),
        })),
    );
    assert_eq!(s.server.player_is_dead(alice.id), Some(false));
    assert_eq!(s.server.player_location(alice.id), Some(at(20.5, 4.5)));
    let second = last_asked(&s.plugin, EventKind::PlayerRespawn);
    rule_on(&mut s.server, 1, second.id, false, None);
    let respawns = alice
        .link
        .take()
        .into_iter()
        .filter(|event| matches!(event, ClientEvent::Respawn { .. }))
        .count();
    assert_eq!(respawns, 1);
    assert_eq!(s.server.player_location(alice.id), Some(at(20.5, 4.5)));
}

#[test]
fn a_plugin_respawn_carries_the_plugin_reason() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerRespawn)]);
    let alice = s.players[0].clone();
    kill(&mut s, &alice);
    s.act(to(&alice, PlayerOrder::Respawn));
    let heard = settled_for(&s.plugin, EventKind::PlayerRespawn);
    let EventPayload::PlayerRespawn(PlayerRespawn { respawn_reason, .. }) = &heard[0].payload
    else {
        panic!("expected a respawn");
    };
    assert_eq!(*respawn_reason, PlayerRespawnReason::Plugin);
}

fn give(s: &mut Scene, line: &str) {
    console(s, line);
}

fn death_of(s: &Scene) -> (u64, PlayerDeath) {
    let asked = last_asked(&s.plugin, EventKind::PlayerDeath);
    match asked.payload {
        EventPayload::PlayerDeath(death) => (asked.id, death),
        other => panic!("expected a death, got {other:?}"),
    }
}

#[test]
fn a_death_describes_its_drops_and_waits_for_its_deciders() {
    let mut s = scene_with(&["Alice", "Bob"], vec![deciding(1, EventKind::PlayerDeath)]);
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    give(&mut s, "give Alice diamond 5");
    s.act(to(&alice, PlayerOrder::GiveLevels(30)));
    kill(&mut s, &alice);
    assert_eq!(s.server.items_on_ground(), 0);
    let (event, death) = death_of(&s);
    assert_eq!(death.drops, [ItemSpec::of("minecraft:diamond", 5)]);
    assert_eq!(death.entity.id, alice.id);
    assert_eq!(death.entity_type, "minecraft:player");
    assert!(!death.keep_inventory && !death.keep_level);
    assert_eq!(
        (death.new_exp, death.new_level, death.new_total_exp),
        (0, 0, 0)
    );
    assert!(
        death
            .death_message
            .as_deref()
            .is_some_and(|m| m.contains("Alice")),
        "{:?}",
        death.death_message
    );
    bob.link.take();
    rule_on(
        &mut s.server,
        1,
        event,
        false,
        Some(EventPatch::PlayerDeath(PlayerDeathPatch {
            death_message: Some(Some("§cAlice is no more".to_owned())),
            ..PlayerDeathPatch::default()
        })),
    );
    assert_eq!(s.server.items_on_ground(), 1);
    assert_eq!(system_texts(&bob), ["Alice is no more"]);
}

#[test]
fn a_death_with_cleared_drops_and_no_message_drops_nothing_and_says_nothing() {
    let mut s = scene_with(&["Alice", "Bob"], vec![deciding(1, EventKind::PlayerDeath)]);
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    give(&mut s, "give Alice diamond 5");
    kill(&mut s, &alice);
    let (event, death) = death_of(&s);
    bob.link.take();
    alice.link.take();
    rule_on(
        &mut s.server,
        1,
        event,
        false,
        Some(EventPatch::PlayerDeath(PlayerDeathPatch {
            death_message: Some(None),
            drops: replacing(&death.drops, Vec::new()),
            dropped_exp: Some(0),
            ..PlayerDeathPatch::default()
        })),
    );
    assert_eq!(s.server.items_on_ground(), 0);
    assert!(system_texts(&bob).is_empty());
    let kill_message = alice.link.take().into_iter().find_map(|event| match event {
        ClientEvent::CombatKill { message, .. } => Some(message),
        _ => None,
    });
    assert_eq!(
        kill_message.map(|m| rspigot_core::console::console_text(&m)),
        Some(String::new())
    );
}

#[test]
fn keeping_the_inventory_without_clearing_the_drops_duplicates_them_as_in_spigot() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerDeath)]);
    let alice = s.players[0].clone();
    give(&mut s, "give Alice diamond 5");
    kill(&mut s, &alice);
    let (event, _) = death_of(&s);
    rule_on(
        &mut s.server,
        1,
        event,
        false,
        Some(EventPatch::PlayerDeath(PlayerDeathPatch {
            keep_inventory: Some(true),
            ..PlayerDeathPatch::default()
        })),
    );
    assert_eq!(s.server.items_on_ground(), 1);
    let rspigot_core::plugins::Answer::Inventory(inventory) =
        s.ask(rspigot_core::plugins::Question::Inventory(alice.id))
    else {
        panic!("expected the inventory");
    };
    assert!(
        inventory
            .slots
            .iter()
            .flatten()
            .any(|item| item.key == "minecraft:diamond")
    );
}

#[test]
fn the_new_level_waits_for_the_respawn_and_keep_level_ignores_it() {
    for (keep_level, expected) in [(false, 7), (true, 30)] {
        let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerDeath)]);
        let alice = s.players[0].clone();
        s.act(to(&alice, PlayerOrder::GiveLevels(30)));
        kill(&mut s, &alice);
        let (event, _) = death_of(&s);
        rule_on(
            &mut s.server,
            1,
            event,
            false,
            Some(EventPatch::PlayerDeath(PlayerDeathPatch {
                keep_level: Some(keep_level),
                new_level: Some(7),
                new_total_exp: Some(91),
                ..PlayerDeathPatch::default()
            })),
        );
        assert_eq!(
            s.server.player_experience(alice.id).map(|xp| xp.0),
            Some(30)
        );
        send_input(&mut s.server, &alice, PlayerInput::Respawn);
        assert_eq!(
            s.server.player_experience(alice.id).map(|xp| xp.0),
            Some(expected),
            "keep_level {keep_level}"
        );
    }
}

#[test]
fn an_entity_death_handler_also_decides_the_death_of_a_player() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::EntityDeath)]);
    let alice = s.players[0].clone();
    give(&mut s, "give Alice diamond 5");
    kill(&mut s, &alice);
    let asked = last_asked(&s.plugin, EventKind::PlayerDeath);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::EntityDeath(EntityDeathPatch {
            dropped_exp: None,
            drops: ListDiff {
                removed: Vec::new(),
                replaced: Vec::new(),
                added: vec![ItemSpec::of("minecraft:stick", 2)],
            },
        })),
    );
    assert_eq!(s.server.items_on_ground(), 2);
}

#[test]
fn a_respawn_waits_behind_the_death_of_its_player() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            deciding(1, EventKind::PlayerDeath),
            observe(2, EventKind::PlayerRespawn),
        ],
    );
    let alice = s.players[0].clone();
    kill(&mut s, &alice);
    send_input(&mut s.server, &alice, PlayerInput::Respawn);
    assert_eq!(s.server.player_is_dead(alice.id), Some(true));
    assert!(settled_for(&s.plugin, EventKind::PlayerRespawn).is_empty());
    let (event, _) = death_of(&s);
    rule_on(&mut s.server, 1, event, false, None);
    assert_eq!(s.server.player_is_dead(alice.id), Some(false));
    assert_eq!(settled_for(&s.plugin, EventKind::PlayerRespawn).len(), 1);
}

#[test]
fn moves_sent_while_a_changed_destination_waits_are_dropped_when_the_teleport_applies() {
    let mut s = scene_with(
        &["Alice"],
        vec![decide(1), deciding(2, EventKind::PlayerTeleport)],
    );
    let alice = s.players[0].clone();
    let step = |s: &mut Scene, x: f64| {
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::Move {
                position: Some(at(x, 0.5)),
                rotation: None,
                on_ground: true,
            },
        );
    };
    step(&mut s, 1.5);
    let moved = last_asked(&s.plugin, EventKind::PlayerMove);
    rule_on(
        &mut s.server,
        1,
        moved.id,
        false,
        Some(EventPatch::PlayerMove(PlayerMovePatch {
            to: Some(location(at(8.5, 0.5))),
        })),
    );
    step(&mut s, 2.5);
    let later = last_asked(&s.plugin, EventKind::PlayerMove);
    assert_ne!(later.id, moved.id);
    let teleport = last_asked(&s.plugin, EventKind::PlayerTeleport);
    judge(&mut s.server, 1, teleport.id, 2, false, None);
    rule_on(&mut s.server, 1, later.id, false, None);
    s.tick(3);
    assert_eq!(s.server.player_location(alice.id), Some(at(8.5, 0.5)));
}

fn held_items(s: &mut Scene, player: &Player) -> Vec<ItemSpec> {
    match s.ask(rspigot_core::plugins::Question::Inventory(player.id)) {
        rspigot_core::plugins::Answer::Inventory(inventory) => {
            inventory.slots.into_iter().flatten().collect()
        }
        other => panic!("expected the inventory, got {other:?}"),
    }
}

#[test]
fn the_items_of_a_dying_player_stay_in_the_inventory_until_the_death_settles() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerDeath)]);
    let alice = s.players[0].clone();
    give(&mut s, "give Alice diamond 5");
    kill(&mut s, &alice);
    assert_eq!(
        held_items(&mut s, &alice),
        [ItemSpec::of("minecraft:diamond", 5)]
    );
    let (event, _) = death_of(&s);
    rule_on(&mut s.server, 1, event, false, None);
    assert!(held_items(&mut s, &alice).is_empty());
    assert_eq!(s.server.items_on_ground(), 1);
}

fn ground_heights(s: &mut Scene) -> Vec<(String, f64)> {
    let mut heights: Vec<(String, f64)> = s
        .server
        .ground_items()
        .into_iter()
        .filter_map(|(id, stack)| {
            let Answer::Entity(details) = s.ask(Question::Entity(id)) else {
                return None;
            };
            Some((
                stack.item.kind()?.name().to_owned(),
                details.entity.position.y,
            ))
        })
        .collect();
    heights.sort_by(|a, b| a.0.cmp(&b.0));
    heights
}

#[test]
fn a_replaced_drop_is_still_thrown_from_the_inventory() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerDeath)]);
    let alice = s.players[0].clone();
    give(&mut s, "give Alice diamond 5");
    kill(&mut s, &alice);
    let (event, death) = death_of(&s);
    assert_eq!(death.drops, [ItemSpec::of("minecraft:diamond", 5)]);
    rule_on(
        &mut s.server,
        1,
        event,
        false,
        Some(EventPatch::PlayerDeath(PlayerDeathPatch {
            drops: ListDiff {
                removed: Vec::new(),
                replaced: vec![(0, ItemSpec::of("minecraft:diamond", 9))],
                added: vec![ItemSpec::of("minecraft:stick", 2)],
            },
            ..PlayerDeathPatch::default()
        })),
    );
    let heights = ground_heights(&mut s);
    let [(diamond, thrown), (stick, spawned)] = heights.as_slice() else {
        panic!("two drops, got {heights:?}");
    };
    assert_eq!(
        (diamond.as_str(), stick.as_str()),
        ("minecraft:diamond", "minecraft:stick")
    );
    assert_eq!(
        s.server
            .ground_items()
            .iter()
            .map(|(_, stack)| stack.count.get())
            .max(),
        Some(9)
    );
    assert!(thrown - spawned > 1.0, "{heights:?}");
}

#[test]
fn an_entity_death_plugin_and_a_player_death_plugin_both_remove_a_drop() {
    for later_answers_first in [false, true] {
        let mut s = scene_with(
            &["Alice"],
            vec![HandlerSpec {
                priority: Priority::Low,
                ..deciding(1, EventKind::EntityDeath)
            }],
        );
        connect_with(
            &mut s.server,
            2,
            "second",
            vec![HandlerSpec {
                priority: Priority::High,
                ..deciding(1, EventKind::PlayerDeath)
            }],
        );
        let alice = s.players[0].clone();
        give(&mut s, "give Alice diamond 5");
        give(&mut s, "give Alice stick 2");
        kill(&mut s, &alice);
        let (event, death) = death_of(&s);
        assert_eq!(death.drops.len(), 2);
        let gone = |key: &'static str| removing(&death.drops, move |drop| drop.key == key);
        let mut answers = vec![
            (
                1,
                EventPatch::EntityDeath(EntityDeathPatch {
                    drops: gone("minecraft:diamond"),
                    ..EntityDeathPatch::default()
                }),
            ),
            (
                2,
                EventPatch::PlayerDeath(PlayerDeathPatch {
                    drops: gone("minecraft:stick"),
                    ..PlayerDeathPatch::default()
                }),
            ),
        ];
        if later_answers_first {
            answers.reverse();
        }
        for (connection, patch) in answers {
            rule_on(&mut s.server, connection, event, false, Some(patch));
        }
        assert_eq!(s.server.items_on_ground(), 0, "{later_answers_first}");
    }
}

#[test]
fn a_slot_filled_while_the_death_waits_loses_nothing() {
    for keep in [true, false] {
        let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerDeath)]);
        let alice = s.players[0].clone();
        give(&mut s, "give Alice diamond 5");
        kill(&mut s, &alice);
        give(&mut s, "give Alice gold_ingot 2");
        let (event, death) = death_of(&s);
        let patch = keep.then(|| {
            EventPatch::PlayerDeath(PlayerDeathPatch {
                keep_inventory: Some(true),
                drops: replacing(&death.drops, Vec::new()),
                ..PlayerDeathPatch::default()
            })
        });
        rule_on(&mut s.server, 1, event, false, patch);
        let expected = if keep {
            vec![
                ItemSpec::of("minecraft:diamond", 5),
                ItemSpec::of("minecraft:gold_ingot", 2),
            ]
        } else {
            vec![ItemSpec::of("minecraft:gold_ingot", 2)]
        };
        assert_eq!(held_items(&mut s, &alice), expected, "keep {keep}");
        assert_eq!(
            s.server.items_on_ground(),
            usize::from(!keep),
            "keep {keep}"
        );
    }
}

#[test]
fn a_dying_player_cannot_drop_while_the_death_waits() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            deciding(1, EventKind::PlayerDeath),
            observe(2, EventKind::PlayerDropItem),
        ],
    );
    let alice = s.players[0].clone();
    give(&mut s, "give Alice diamond 5");
    kill(&mut s, &alice);
    s.act(to(&alice, PlayerOrder::SetHealth(20.0)));
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Dig {
            action: rspigot_core::events::DigAction::DropStack,
            pos: rspigot_domain::BlockPos::new(0, 0, 0),
            face: rspigot_core::events::Direction::Up,
            sequence: 0,
        },
    );
    assert!(settled_for(&s.plugin, EventKind::PlayerDropItem).is_empty());
    assert_eq!(s.server.items_on_ground(), 0);
    let (event, _) = death_of(&s);
    rule_on(&mut s.server, 1, event, false, None);
    assert_eq!(s.server.items_on_ground(), 1);
}

#[test]
fn a_plugin_inventory_closes_after_the_death_of_its_viewer_settles() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            deciding(1, EventKind::PlayerDeath),
            observe(2, EventKind::PlayerDeath),
            observe(3, EventKind::InventoryClose),
        ],
    );
    let alice = s.players[0].clone();
    s.act(rspigot_core::plugins::Order::Server(
        rspigot_core::plugins::ServerOrder::CreateInventory {
            id: 7,
            rows: 1,
            title: "Menu".to_owned(),
            locked: true,
        },
    ));
    s.act(to(&alice, PlayerOrder::OpenInventory(7)));
    kill(&mut s, &alice);
    assert!(settled_for(&s.plugin, EventKind::InventoryClose).is_empty());
    let (event, _) = death_of(&s);
    rule_on(&mut s.server, 1, event, false, None);
    let settled: Vec<EventKind> = fired(&s.plugin)
        .into_iter()
        .filter(|fired| fired.phase == rspigot_core::plugins::Phase::Settled)
        .map(|fired| fired.payload.kind())
        .collect();
    assert_eq!(settled, [EventKind::PlayerDeath, EventKind::InventoryClose]);
}

#[test]
fn a_respawn_nobody_listens_to_does_not_revive_a_player_whose_death_waits() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerDeath)]);
    let alice = s.players[0].clone();
    kill(&mut s, &alice);
    send_input(&mut s.server, &alice, PlayerInput::Respawn);
    assert_eq!(s.server.player_is_dead(alice.id), Some(true));
    let (event, _) = death_of(&s);
    rule_on(&mut s.server, 1, event, false, None);
    assert_eq!(s.server.player_is_dead(alice.id), Some(true));
    send_input(&mut s.server, &alice, PlayerInput::Respawn);
    assert_eq!(s.server.player_is_dead(alice.id), Some(false));
}

#[test]
fn the_new_experience_of_a_death_is_given_at_respawn_and_its_dropped_experience_spawns_orbs() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerDeath)]);
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::GiveLevels(30)));
    kill(&mut s, &alice);
    let (event, death) = death_of(&s);
    assert!(death.dropped_exp > 0);
    rule_on(
        &mut s.server,
        1,
        event,
        false,
        Some(EventPatch::PlayerDeath(PlayerDeathPatch {
            new_exp: Some(50),
            ..PlayerDeathPatch::default()
        })),
    );
    assert!(
        !s.server
            .entities_of(rspigot_entities::EntityKind::ExperienceOrb)
            .is_empty()
    );
    send_input(&mut s.server, &alice, PlayerInput::Respawn);
    let (level, _, total) = s
        .server
        .player_experience(alice.id)
        .expect("Alice is online");
    assert_eq!((level, total), (4, 50));
}

#[test]
fn a_plugin_teleport_settled_after_its_player_died_does_not_move_them() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerTeleport)]);
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::Teleport(at(9.5, 0.5))));
    let asked = last_asked(&s.plugin, EventKind::PlayerTeleport);
    kill(&mut s, &alice);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert_eq!(s.server.player_location(alice.id), Some(SPAWN));
}
