mod common;

use common::*;
use rspigot_core::events::{
    ClientEvent, Direction, Hand, Inbound, MetadataValue, MovementAction, PlayerInput, Pose,
};
use rspigot_core::plugins::{Answer, BlockOrder, Order, PlayerOrder, Question};
use rspigot_domain::{BlockPos, Vec3};

const FLOOR: BlockPos = BlockPos::new(2, 63, 0);
const FOOT: BlockPos = BlockPos::new(2, 64, 0);
const HEAD: BlockPos = BlockPos::new(2, 64, 1);

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn state_at(s: &mut Scene, pos: BlockPos) -> (String, Vec<(String, String)>) {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => (state.kind, state.properties),
        other => panic!("expected a block, got {other:?}"),
    }
}

fn kind_at(s: &mut Scene, pos: BlockPos) -> String {
    state_at(s, pos).0
}

fn property_at(s: &mut Scene, pos: BlockPos, name: &str) -> Option<String> {
    state_at(s, pos)
        .1
        .into_iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
}

fn set(s: &mut Scene, pos: BlockPos, kind: &str, properties: &[(&str, &str)]) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::SetWith {
            kind: kind.to_owned(),
            properties: properties
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
        },
    });
}

fn click(s: &mut Scene, pos: BlockPos) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 1,
        },
    );
}

fn bed_scene() -> Scene {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, FLOOR);
    s
}

#[test]
fn placing_a_bed_places_the_foot_and_the_head_in_front_of_it() {
    let mut s = bed_scene();
    console(&mut s, "give Alice red_bed");
    click(&mut s, FLOOR);
    assert_eq!(kind_at(&mut s, FOOT), "minecraft:red_bed");
    assert_eq!(kind_at(&mut s, HEAD), "minecraft:red_bed");
    assert_eq!(property_at(&mut s, FOOT, "part").as_deref(), Some("foot"));
    assert_eq!(property_at(&mut s, HEAD, "part").as_deref(), Some("head"));
    assert_eq!(
        property_at(&mut s, HEAD, "facing").as_deref(),
        Some("south")
    );
}

#[test]
fn a_bed_is_not_placed_when_its_head_has_no_room() {
    let mut s = bed_scene();
    console(&mut s, "give Alice red_bed");
    set(&mut s, HEAD, "minecraft:stone", &[]);
    click(&mut s, FLOOR);
    assert_eq!(kind_at(&mut s, FOOT), "minecraft:air");
}

#[test]
fn breaking_one_half_of_a_bed_removes_the_other() {
    let mut s = bed_scene();
    set(
        &mut s,
        FOOT,
        "minecraft:red_bed",
        &[("part", "foot"), ("facing", "south")],
    );
    set(
        &mut s,
        HEAD,
        "minecraft:red_bed",
        &[("part", "head"), ("facing", "south")],
    );
    assert_eq!(kind_at(&mut s, HEAD), "minecraft:red_bed");
    set(&mut s, FOOT, "minecraft:air", &[]);
    assert_eq!(kind_at(&mut s, HEAD), "minecraft:air");
}

fn night_scene(names: &[&str]) -> Scene {
    let server = configured_level(|_| {}, |level| level.day_time = 18_000);
    let mut s = scene_on(server, names, Vec::new());
    stream_chunk_of(&mut s, FLOOR);
    set(
        &mut s,
        FOOT,
        "minecraft:red_bed",
        &[("part", "foot"), ("facing", "south")],
    );
    set(
        &mut s,
        HEAD,
        "minecraft:red_bed",
        &[("part", "head"), ("facing", "south")],
    );
    s
}

fn use_bed(s: &mut Scene, player: &Player, pos: BlockPos) {
    send_input(
        &mut s.server,
        player,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 0.5, 0.5),
            inside_block: false,
            sequence: 3,
        },
    );
    s.server.deliver();
}

fn overlays(events: &[ClientEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::SystemChat {
                message,
                overlay: true,
            } => Some(format!("{message:?}")),
            _ => None,
        })
        .collect()
}

fn says(events: &[ClientEvent], key: &str) -> bool {
    events.iter().any(|event| match event {
        ClientEvent::SystemChat { message, .. } => format!("{message:?}").contains(key),
        _ => false,
    })
}

fn lying(events: &[ClientEvent]) -> bool {
    events.iter().any(|event| match event {
        ClientEvent::EntityMetadata { entries, .. } => entries
            .iter()
            .any(|entry| entry.value == MetadataValue::Pose(Pose::Sleeping)),
        _ => false,
    })
}

#[test]
fn right_clicking_a_bed_at_night_puts_the_player_to_sleep_on_it() {
    let mut s = night_scene(&["Alice"]);
    let alice = s.players[0].clone();
    alice.link.take();
    use_bed(&mut s, &alice, FOOT);
    let events = alice.link.take();
    assert!(lying(&events));
    assert_eq!(
        property_at(&mut s, HEAD, "occupied").as_deref(),
        Some("true")
    );
    assert!(events.iter().any(|event| matches!(
        event,
        ClientEvent::Teleport { position, .. } if (position.y - 64.6875).abs() < 1.0e-9
            && (position.x - 2.5).abs() < 1.0e-9
            && (position.z - 1.5).abs() < 1.0e-9
    )));
    assert!(says(&events, "block.minecraft.set_spawn"));
}

#[test]
fn a_bed_cannot_be_slept_in_by_day_but_still_sets_the_spawn() {
    let mut s = bed_scene();
    let alice = s.players[0].clone();
    set(
        &mut s,
        FOOT,
        "minecraft:red_bed",
        &[("part", "foot"), ("facing", "south")],
    );
    set(
        &mut s,
        HEAD,
        "minecraft:red_bed",
        &[("part", "head"), ("facing", "south")],
    );
    alice.link.take();
    use_bed(&mut s, &alice, HEAD);
    let events = alice.link.take();
    assert!(!lying(&events));
    assert!(says(&events, "block.minecraft.bed.no_sleep"));
    assert!(says(&events, "block.minecraft.set_spawn"));
    assert_eq!(overlays(&events).len(), 1);
    assert_eq!(
        property_at(&mut s, HEAD, "occupied").as_deref(),
        Some("false")
    );
}

#[test]
fn a_bed_out_of_reach_refuses_the_player() {
    let mut s = night_scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.act(to(
        &alice,
        PlayerOrder::Teleport(Vec3::new(-1.0, 64.0, 0.5)),
    ));
    s.tick(2);
    alice.link.take();
    use_bed(&mut s, &alice, FOOT);
    let events = alice.link.take();
    assert!(says(&events, "block.minecraft.bed.too_far_away"));
    assert!(!lying(&events));
}

#[test]
fn an_occupied_bed_refuses_the_second_player() {
    let mut s = night_scene(&["Alice", "Bob"]);
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    use_bed(&mut s, &alice, FOOT);
    bob.link.take();
    use_bed(&mut s, &bob, HEAD);
    let events = bob.link.take();
    assert!(says(&events, "block.minecraft.bed.occupied"));
    assert!(!lying(&events));
}

#[test]
fn a_hostile_mob_near_the_bed_prevents_sleep() {
    let mut s = night_scene(&["Alice"]);
    let alice = s.players[0].clone();
    console(&mut s, "summon zombie 4.5 64 1.5");
    alice.link.take();
    use_bed(&mut s, &alice, FOOT);
    let events = alice.link.take();
    assert!(says(&events, "block.minecraft.bed.not_safe"));
    assert!(!lying(&events));
}

#[test]
fn leaving_the_bed_stands_the_player_beside_it_and_frees_the_bed() {
    let mut s = night_scene(&["Alice"]);
    let alice = s.players[0].clone();
    use_bed(&mut s, &alice, FOOT);
    alice.link.take();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Movement(MovementAction::LeaveBed),
    );
    s.server.deliver();
    let events = alice.link.take();
    assert!(!lying(&events));
    assert!(events.iter().any(|event| matches!(
        event,
        ClientEvent::EntityMetadata { entries, .. }
            if entries.iter().any(|entry| entry.value == MetadataValue::Pose(Pose::Standing))
    )));
    assert_eq!(
        property_at(&mut s, HEAD, "occupied").as_deref(),
        Some("false")
    );
    let teleport = events.iter().find_map(|event| match event {
        ClientEvent::Teleport { position, .. } => Some(*position),
        _ => None,
    });
    let spot = teleport.expect("the player is put back on its feet");
    assert!((spot.y - 64.0).abs() < 1.0e-9);
    assert!((spot.x - 2.5).abs() > 0.9 || (spot.z - 1.5).abs() > 0.9);
}

#[test]
fn a_sleeping_player_does_not_move() {
    let mut s = night_scene(&["Alice"]);
    let alice = s.players[0].clone();
    use_bed(&mut s, &alice, FOOT);
    settle_arrival(&mut s.server, &alice);
    let before = s.server.player_location(alice.id);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: Some(Vec3::new(9.0, 64.0, 9.0)),
            rotation: None,
            on_ground: true,
        },
    );
    assert_eq!(s.server.player_location(alice.id), before);
}

#[test]
fn a_hurt_sleeper_wakes_up() {
    let mut s = night_scene(&["Alice"]);
    let alice = s.players[0].clone();
    use_bed(&mut s, &alice, FOOT);
    alice.link.take();
    s.act(to(&alice, PlayerOrder::Damage(1.0)));
    let events = alice.link.take();
    assert!(!lying(&events));
    assert_eq!(
        property_at(&mut s, HEAD, "occupied").as_deref(),
        Some("false")
    );
}

#[test]
fn breaking_the_bed_under_a_sleeper_wakes_it() {
    let mut s = night_scene(&["Alice"]);
    let alice = s.players[0].clone();
    use_bed(&mut s, &alice, FOOT);
    alice.link.take();
    set(&mut s, FOOT, "minecraft:air", &[]);
    s.tick(2);
    assert!(!lying(&alice.link.take()));
}

fn sleeping_scene(names: &[&str], percentage: i32) -> Scene {
    let server = configured_level(
        |config| config.rules.players_sleeping_percentage = percentage,
        |level| {
            level.day_time = 18_000;
            level.raining = true;
        },
    );
    let mut s = scene_on(server, names, Vec::new());
    stream_chunk_of(&mut s, FLOOR);
    set(
        &mut s,
        FOOT,
        "minecraft:red_bed",
        &[("part", "foot"), ("facing", "south")],
    );
    set(
        &mut s,
        HEAD,
        "minecraft:red_bed",
        &[("part", "head"), ("facing", "south")],
    );
    s
}

#[test]
fn a_lone_player_who_sleeps_a_hundred_ticks_skips_the_night_and_the_rain() {
    let mut s = sleeping_scene(&["Alice"], 100);
    let alice = s.players[0].clone();
    use_bed(&mut s, &alice, FOOT);
    s.tick(60);
    assert!(s.server.level_snapshot().day_time < 24_000);
    alice.link.take();
    s.tick(60);
    let level = s.server.level_snapshot();
    assert_eq!(level.day_time / 24_000, 1);
    assert!(!level.raining);
    assert!(!lying(&alice.link.take()));
    assert_eq!(
        property_at(&mut s, HEAD, "occupied").as_deref(),
        Some("false")
    );
}

#[test]
fn the_night_is_not_skipped_while_too_few_players_sleep() {
    let mut s = sleeping_scene(&["Alice", "Bob"], 100);
    let alice = s.players[0].clone();
    use_bed(&mut s, &alice, FOOT);
    s.tick(150);
    assert!(s.server.level_snapshot().day_time < 24_000);
    assert!(s.server.level_snapshot().raining);
}

#[test]
fn the_sleeping_percentage_rule_lets_half_the_players_skip_the_night() {
    let mut s = sleeping_scene(&["Alice", "Bob"], 50);
    let alice = s.players[0].clone();
    use_bed(&mut s, &alice, FOOT);
    s.tick(110);
    assert!(s.server.level_snapshot().day_time >= 24_000);
}

#[test]
fn the_players_are_told_how_many_sleep_and_how_many_are_needed() {
    let mut s = sleeping_scene(&["Alice", "Bob"], 100);
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    bob.link.take();
    use_bed(&mut s, &alice, FOOT);
    let events = bob.link.take();
    assert!(says(&events, "sleep.players_sleeping"));
}

fn bed_spawn_scene() -> Scene {
    let mut s = bed_scene();
    set(
        &mut s,
        FOOT,
        "minecraft:red_bed",
        &[("part", "foot"), ("facing", "south")],
    );
    set(
        &mut s,
        HEAD,
        "minecraft:red_bed",
        &[("part", "head"), ("facing", "south")],
    );
    let alice = s.players[0].clone();
    use_bed(&mut s, &alice, HEAD);
    s
}

fn die_and_respawn(s: &mut Scene) -> Vec<ClientEvent> {
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::Kill));
    s.tick(2);
    alice.link.take();
    send_input(&mut s.server, &alice, PlayerInput::Respawn);
    s.server.deliver();
    alice.link.take()
}

fn teleport_of(events: &[ClientEvent]) -> Vec3 {
    events
        .iter()
        .rev()
        .find_map(|event| match event {
            ClientEvent::Teleport { position, .. } => Some(*position),
            _ => None,
        })
        .expect("the player is teleported")
}

#[test]
fn a_player_respawns_beside_the_bed_it_clicked() {
    let mut s = bed_spawn_scene();
    let events = die_and_respawn(&mut s);
    let spot = teleport_of(&events);
    assert!((spot.x - 0.5).abs() > 0.4 || (spot.z - 0.5).abs() > 0.4);
    assert!(spot.distance_squared(Vec3::new(2.5, 64.0, 1.0)) < 9.0);
    assert!(!events.iter().any(|event| matches!(
        event,
        ClientEvent::GameEvent(rspigot_core::events::GameEvent::NoRespawnBlockAvailable)
    )));
}

#[test]
fn a_player_whose_bed_is_gone_respawns_at_the_world_spawn_and_is_told() {
    let mut s = bed_spawn_scene();
    set(&mut s, FOOT, "minecraft:air", &[]);
    let events = die_and_respawn(&mut s);
    let spot = teleport_of(&events);
    assert!(spot.distance_squared(Vec3::new(0.5, 64.0, 0.5)) < 1.0e-6);
    assert!(events.iter().any(|event| matches!(
        event,
        ClientEvent::GameEvent(rspigot_core::events::GameEvent::NoRespawnBlockAvailable)
    )));
    let again = die_and_respawn(&mut s);
    assert!(!again.iter().any(|event| matches!(
        event,
        ClientEvent::GameEvent(rspigot_core::events::GameEvent::NoRespawnBlockAvailable)
    )));
}
