mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{ClientEvent, Direction, GameEvent, Hand, MovementAction, PlayerInput};
use rspigot_core::plugins::events::{
    EventResult, PlayerBedEnterPatch, PlayerBedEnterResult, PlayerSpawnChangeCause,
    PlayerSpawnChangePatch,
};
use rspigot_core::plugins::{
    Answer, BlockOrder, EventKind, EventPatch, EventPayload, HandlerSpec, Location, Order,
    PlayerOrder, Question,
};
use rspigot_domain::{BlockPos, Rotation, Vec3};

const FLOOR: BlockPos = BlockPos::new(2, 63, 0);
const FOOT: BlockPos = BlockPos::new(2, 64, 0);
const HEAD: BlockPos = BlockPos::new(2, 64, 1);

fn scene_at(day_time: i64, handlers: Vec<HandlerSpec>) -> Scene {
    let server = configured_level(
        |config| {
            config.rules.respawn_radius = 0;
            config.plugins = PluginSettings {
                decision_ticks: 1_000,
                ..PluginSettings::default()
            };
        },
        |level| level.day_time = day_time,
    );
    let mut s = scene_on(server, &["Alice"], handlers);
    stream_chunk_of(&mut s, FLOOR);
    for (pos, part) in [(FOOT, "foot"), (HEAD, "head")] {
        s.act(Order::Block {
            pos,
            order: BlockOrder::SetWith {
                kind: "minecraft:red_bed".to_owned(),
                properties: vec![
                    ("part".to_owned(), part.to_owned()),
                    ("facing".to_owned(), "south".to_owned()),
                ],
            },
        });
    }
    s.players[0].link.take();
    s
}

fn click_bed(s: &mut Scene) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: FOOT,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 0.5, 0.5),
            inside_block: false,
            sequence: 1,
        },
    );
    s.server.deliver();
}

fn occupied(s: &mut Scene) -> bool {
    match s.ask(Question::Block(HEAD)) {
        Answer::Block(state) => state
            .properties
            .iter()
            .any(|(name, value)| name == "occupied" && value == "true"),
        other => panic!("expected a block, got {other:?}"),
    }
}

fn leave(s: &mut Scene) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Movement(MovementAction::LeaveBed),
    );
    s.server.deliver();
}

#[test]
fn entering_a_bed_at_night_fires_the_bed_enter_with_an_ok_result() {
    let mut s = scene_at(18_000, vec![observe(1, EventKind::PlayerBedEnter)]);
    click_bed(&mut s);
    let events = settled_for(&s.plugin, EventKind::PlayerBedEnter);
    assert_eq!(events.len(), 1);
    let EventPayload::PlayerBedEnter(event) = &events[0].payload else {
        panic!("expected a bed enter");
    };
    assert_eq!(event.bed, HEAD);
    assert_eq!(event.bed_enter_result, PlayerBedEnterResult::Ok);
    assert_eq!(event.use_bed, EventResult::Default);
    assert_eq!(event.player.name, "Alice");
    assert!(occupied(&mut s));
}

#[test]
fn entering_a_bed_by_day_fires_with_the_reason_it_is_not_possible_now() {
    let mut s = scene_at(1_000, vec![observe(1, EventKind::PlayerBedEnter)]);
    click_bed(&mut s);
    let events = settled_for(&s.plugin, EventKind::PlayerBedEnter);
    let EventPayload::PlayerBedEnter(event) = &events[0].payload else {
        panic!("expected a bed enter");
    };
    assert_eq!(event.bed_enter_result, PlayerBedEnterResult::NotPossibleNow);
    assert!(!occupied(&mut s));
}

#[test]
fn a_cancelled_bed_enter_keeps_the_player_out_of_the_bed() {
    let mut s = scene_at(18_000, vec![deciding(1, EventKind::PlayerBedEnter)]);
    click_bed(&mut s);
    let asked = last_asked(&s.plugin, EventKind::PlayerBedEnter);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(!occupied(&mut s));
}

#[test]
fn allowing_a_bed_enter_lets_the_player_sleep_by_day() {
    let mut s = scene_at(1_000, vec![deciding(1, EventKind::PlayerBedEnter)]);
    click_bed(&mut s);
    let asked = last_asked(&s.plugin, EventKind::PlayerBedEnter);
    let patch = EventPatch::PlayerBedEnter(PlayerBedEnterPatch {
        use_bed: Some(EventResult::Allow),
    });
    rule_on(&mut s.server, 1, asked.id, false, Some(patch));
    assert!(occupied(&mut s));
}

#[test]
fn denying_a_bed_enter_keeps_the_player_out_at_night() {
    let mut s = scene_at(18_000, vec![deciding(1, EventKind::PlayerBedEnter)]);
    click_bed(&mut s);
    let asked = last_asked(&s.plugin, EventKind::PlayerBedEnter);
    let patch = EventPatch::PlayerBedEnter(PlayerBedEnterPatch {
        use_bed: Some(EventResult::Deny),
    });
    rule_on(&mut s.server, 1, asked.id, false, Some(patch));
    assert!(!occupied(&mut s));
}

#[test]
fn leaving_the_bed_fires_the_bed_leave_with_the_bed_it_slept_in() {
    let mut s = scene_at(18_000, vec![observe(1, EventKind::PlayerBedLeave)]);
    click_bed(&mut s);
    assert!(settled_for(&s.plugin, EventKind::PlayerBedLeave).is_empty());
    leave(&mut s);
    let events = settled_for(&s.plugin, EventKind::PlayerBedLeave);
    assert_eq!(events.len(), 1);
    let EventPayload::PlayerBedLeave(event) = &events[0].payload else {
        panic!("expected a bed leave");
    };
    assert_eq!(event.bed, HEAD);
    assert!(event.set_spawn_location);
    assert!(!occupied(&mut s));
}

#[test]
fn a_cancelled_bed_leave_keeps_the_player_in_bed() {
    let mut s = scene_at(18_000, vec![deciding(1, EventKind::PlayerBedLeave)]);
    click_bed(&mut s);
    leave(&mut s);
    let asked = last_asked(&s.plugin, EventKind::PlayerBedLeave);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(occupied(&mut s));
}

#[test]
fn an_allowed_bed_leave_gets_the_player_up() {
    let mut s = scene_at(18_000, vec![deciding(1, EventKind::PlayerBedLeave)]);
    click_bed(&mut s);
    leave(&mut s);
    let asked = last_asked(&s.plugin, EventKind::PlayerBedLeave);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert!(!occupied(&mut s));
}

#[test]
fn clicking_a_bed_fires_the_spawn_change_with_the_bed_as_the_new_spawn() {
    let mut s = scene_at(1_000, vec![observe(1, EventKind::PlayerSpawnChange)]);
    click_bed(&mut s);
    let events = settled_for(&s.plugin, EventKind::PlayerSpawnChange);
    assert_eq!(events.len(), 1);
    let EventPayload::PlayerSpawnChange(event) = &events[0].payload else {
        panic!("expected a spawn change");
    };
    assert_eq!(event.cause, PlayerSpawnChangeCause::Bed);
    assert!(!event.forced);
    let spawn = event.new_spawn.as_ref().expect("a new spawn");
    assert_eq!(spawn.world, "minecraft:overworld");
    assert_eq!(spawn.position, Vec3::new(2.0, 64.0, 1.0));
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
fn a_cancelled_spawn_change_keeps_the_old_spawn_so_the_player_respawns_at_the_world_spawn() {
    let mut s = scene_at(1_000, vec![deciding(1, EventKind::PlayerSpawnChange)]);
    click_bed(&mut s);
    let asked = last_asked(&s.plugin, EventKind::PlayerSpawnChange);
    rule_on(&mut s.server, 1, asked.id, true, None);
    let spot = respawn(&mut s);
    assert!(spot.distance_squared(Vec3::new(0.5, 64.0, 0.5)) < 1.0e-6);
}

#[test]
fn a_patched_spawn_change_sets_the_forced_spawn_the_plugin_chose() {
    let mut s = scene_at(1_000, vec![deciding(1, EventKind::PlayerSpawnChange)]);
    click_bed(&mut s);
    let asked = last_asked(&s.plugin, EventKind::PlayerSpawnChange);
    let patch = EventPatch::PlayerSpawnChange(PlayerSpawnChangePatch {
        forced: Some(true),
        new_spawn: Some(Some(Location {
            world: "minecraft:overworld".to_owned(),
            position: Vec3::new(5.0, 64.0, 5.0),
            rotation: Rotation::new(0.0, 0.0),
        })),
    });
    rule_on(&mut s.server, 1, asked.id, false, Some(patch));
    let spot = respawn(&mut s);
    assert!(spot.distance_squared(Vec3::new(5.5, 64.1, 5.5)) < 1.0e-6);
}

#[test]
fn losing_the_bed_resets_the_spawn_and_fires_a_reset_spawn_change() {
    let mut s = scene_at(1_000, vec![observe(1, EventKind::PlayerSpawnChange)]);
    click_bed(&mut s);
    s.act(Order::Block {
        pos: FOOT,
        order: BlockOrder::Set("minecraft:air".to_owned()),
    });
    respawn(&mut s);
    let events = settled_for(&s.plugin, EventKind::PlayerSpawnChange);
    let EventPayload::PlayerSpawnChange(last) = &events.last().expect("a reset").payload else {
        panic!("expected a spawn change");
    };
    assert_eq!(last.cause, PlayerSpawnChangeCause::Reset);
    assert!(last.new_spawn.is_none());
}

#[test]
fn the_reset_spawn_tells_the_client_there_is_no_respawn_block() {
    let mut s = scene_at(1_000, Vec::new());
    click_bed(&mut s);
    s.act(Order::Block {
        pos: FOOT,
        order: BlockOrder::Set("minecraft:air".to_owned()),
    });
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::Kill));
    s.tick(2);
    alice.link.take();
    send_input(&mut s.server, &alice, PlayerInput::Respawn);
    s.server.deliver();
    assert!(alice.link.take().iter().any(|event| matches!(
        event,
        ClientEvent::GameEvent(GameEvent::NoRespawnBlockAvailable)
    )));
}
