mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{ClientEvent, GameEvent, Inbound};
use rspigot_core::plugins::events::{
    SpawnChange, ThunderChange, TimeSkip, TimeSkipPatch, TimeSkipReason, WeatherChange, WorldLoad,
    WorldSave,
};
use rspigot_core::plugins::{
    Answer, EventKind, EventPatch, EventPayload, Location, Order, Question, WorldOrder, WorldState,
};
use rspigot_domain::{Rotation, Vec3};

const WORLD: &str = "minecraft:overworld";

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn world_state(s: &mut Scene) -> WorldState {
    match s.ask(Question::World) {
        Answer::World(state) => state,
        other => panic!("expected the world, got {other:?}"),
    }
}

fn day_time(s: &mut Scene) -> i64 {
    world_state(s).day_time
}

fn time_packets(player: &Player) -> Vec<i64> {
    player
        .link
        .take()
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Time { day_time, .. } => Some(*day_time),
            _ => None,
        })
        .collect()
}

fn weather_packets(player: &Player) -> Vec<GameEvent> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::GameEvent(
                event @ (GameEvent::StartRaining
                | GameEvent::StopRaining
                | GameEvent::RainLevelChange(_)
                | GameEvent::ThunderLevelChange(_)),
            ) => Some(event),
            _ => None,
        })
        .collect()
}

fn skip(amount: i64, reason: TimeSkipReason) -> EventPayload {
    EventPayload::TimeSkip(TimeSkip {
        skip_amount: amount,
        skip_reason: reason,
        world: WORLD.to_owned(),
    })
}

#[test]
fn time_add_with_a_decider_moves_the_time_once_by_the_settled_amount() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::TimeSkip)]);
    let alice = s.players[0].clone();
    let before = day_time(&mut s);
    alice.link.take();
    console(&mut s, "time add 100");
    let asked = last_asked(&s.plugin, EventKind::TimeSkip);
    assert_eq!(asked.payload, skip(100, TimeSkipReason::Command));
    assert_eq!(day_time(&mut s), before);
    assert!(time_packets(&alice).is_empty());
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::TimeSkip(TimeSkipPatch {
            skip_amount: Some(40),
        })),
    );
    assert_eq!(day_time(&mut s), before + 40);
    assert_eq!(time_packets(&alice), [before + 40]);
}

#[test]
fn a_cancelled_time_skip_keeps_the_time() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::TimeSkip)]);
    let alice = s.players[0].clone();
    let before = day_time(&mut s);
    alice.link.take();
    console(&mut s, "time add 500");
    let asked = last_asked(&s.plugin, EventKind::TimeSkip);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(day_time(&mut s), before);
    assert!(time_packets(&alice).is_empty());
}

#[test]
fn time_add_without_a_decider_moves_the_time_at_once() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::TimeSkip)]);
    let before = day_time(&mut s);
    console(&mut s, "time add 100");
    assert_eq!(day_time(&mut s), before + 100);
    assert_eq!(settled_for(&s.plugin, EventKind::TimeSkip).len(), 1);
}

#[test]
fn time_set_fires_nothing_and_a_plugin_time_fires_a_custom_skip() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::TimeSkip)]);
    console(&mut s, "time set 1000");
    console(&mut s, "time add 0");
    assert!(fired(&s.plugin).is_empty());
    s.act(Order::World(WorldOrder::SetTime(3000)));
    let heard = settled_for(&s.plugin, EventKind::TimeSkip);
    assert_eq!(heard.len(), 1);
    assert_eq!(heard[0].payload, skip(2000, TimeSkipReason::Custom));
}

#[test]
fn every_save_fires_world_save() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::WorldSave)]);
    console(&mut s, "save-all");
    s.act(Order::World(WorldOrder::Save));
    let heard = settled_for(&s.plugin, EventKind::WorldSave);
    assert_eq!(heard.len(), 2);
    assert_eq!(
        heard[0].payload,
        EventPayload::WorldSave(WorldSave {
            world: WORLD.to_owned(),
        })
    );
}

#[test]
fn the_autosave_fires_world_save() {
    let mut server = server(PluginSettings::default());
    let plugin = connect_with(
        &mut server,
        1,
        "tester",
        vec![observe(1, EventKind::WorldSave)],
    );
    for _ in 0..6000 {
        server.tick();
    }
    assert_eq!(settled_for(&plugin, EventKind::WorldSave).len(), 1);
}

#[test]
fn the_shutdown_save_reaches_a_connected_plugin_even_behind_a_waiting_event() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            observe(1, EventKind::WorldSave),
            deciding(2, EventKind::TimeSkip),
        ],
    );
    console(&mut s, "time add 10");
    assert_eq!(asked_for(&s.plugin, EventKind::TimeSkip).len(), 1);
    let (sender, inbound) = crossbeam_channel::unbounded();
    sender.send(Inbound::Shutdown).expect("open");
    let Scene { server, plugin, .. } = s;
    server.run(&inbound);
    assert_eq!(settled_for(&plugin, EventKind::WorldSave).len(), 1);
}

#[test]
fn the_world_load_reaches_the_plugins_connected_when_it_fires() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::WorldLoad)]);
    assert!(fired(&s.plugin).is_empty());
    s.server.world_loaded();
    assert_eq!(
        settled_for(&s.plugin, EventKind::WorldLoad)[0].payload,
        EventPayload::WorldLoad(WorldLoad {
            world: WORLD.to_owned(),
        })
    );
}

#[test]
fn moving_the_spawn_fires_spawn_change_with_the_previous_corner_once() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::SpawnChange)]);
    let alice = s.players[0].clone();
    console(&mut s, "setworldspawn 5 64 5");
    let heard = settled_for(&s.plugin, EventKind::SpawnChange);
    assert_eq!(
        heard[0].payload,
        EventPayload::SpawnChange(SpawnChange {
            previous_location: Location {
                world: WORLD.to_owned(),
                position: Vec3::new(0.0, 64.0, 0.0),
                rotation: Rotation::new(0.0, 0.0),
            },
            world: WORLD.to_owned(),
        })
    );
    alice.link.take();
    console(&mut s, "setworldspawn 5 64 5");
    assert_eq!(settled_for(&s.plugin, EventKind::SpawnChange).len(), 1);
    assert!(
        !alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::SpawnPosition { .. }))
    );
}

fn weather_kinds(s: &Scene) -> Vec<(EventKind, bool)> {
    fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::WeatherChange(WeatherChange {
                to_weather_state, ..
            }) => Some((EventKind::WeatherChange, to_weather_state)),
            EventPayload::ThunderChange(ThunderChange {
                to_thunder_state, ..
            }) => Some((EventKind::ThunderChange, to_thunder_state)),
            _ => None,
        })
        .collect()
}

#[test]
fn weather_commands_fire_rain_then_thunder_for_what_changes() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            observe(1, EventKind::WeatherChange),
            observe(2, EventKind::ThunderChange),
        ],
    );
    console(&mut s, "weather rain");
    console(&mut s, "weather thunder");
    console(&mut s, "weather thunder");
    console(&mut s, "weather clear");
    assert_eq!(
        weather_kinds(&s),
        [
            (EventKind::WeatherChange, true),
            (EventKind::ThunderChange, true),
            (EventKind::WeatherChange, false),
            (EventKind::ThunderChange, false),
        ]
    );
}

#[test]
fn a_cancelled_rain_keeps_the_sky_and_the_thunder_is_still_asked() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            deciding(1, EventKind::WeatherChange),
            deciding(2, EventKind::ThunderChange),
        ],
    );
    let alice = s.players[0].clone();
    s.act(Order::World(WorldOrder::SetWeather(
        rspigot_core::commands::Weather::Thunder,
    )));
    assert!(weather_packets(&alice).is_empty());
    let rain = last_asked(&s.plugin, EventKind::WeatherChange);
    judge(&mut s.server, 1, rain.id, 1, true, None);
    assert!(weather_packets(&alice).is_empty());
    let thunder = last_asked(&s.plugin, EventKind::ThunderChange);
    judge(&mut s.server, 1, thunder.id, 2, false, None);
    assert_eq!(
        weather_packets(&alice),
        [GameEvent::ThunderLevelChange(1.0)]
    );
    let state = world_state(&mut s);
    assert!(!state.raining);
    assert!(state.thundering);
}

#[test]
fn a_weather_command_sent_while_another_waits_reads_the_sky_when_its_turn_comes() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::WeatherChange)]);
    console(&mut s, "weather thunder");
    let rain = last_asked(&s.plugin, EventKind::WeatherChange);
    console(&mut s, "weather clear");
    assert_eq!(asked_for(&s.plugin, EventKind::WeatherChange).len(), 1);
    rule_on(&mut s.server, 1, rain.id, false, None);
    let state = world_state(&mut s);
    assert!(state.raining && state.thundering);
    let clear = last_asked(&s.plugin, EventKind::WeatherChange);
    assert_ne!(clear.id, rain.id);
    rule_on(&mut s.server, 1, clear.id, false, None);
    let state = world_state(&mut s);
    assert!(!state.raining && !state.thundering);
}

#[test]
fn a_plugin_connected_before_startup_hears_the_startup_events() {
    let (sender, inbox) = crossbeam_channel::unbounded::<Inbound<Link>>();
    let mailbox = Mailbox::default();
    sender
        .send(Inbound::PluginConnected {
            connection: 1,
            intro: rspigot_core::plugins::PluginIntro {
                commands: Vec::new(),
                name: "early".to_owned(),
                version: "1.0.0".to_owned(),
                handlers: vec![
                    observe(1, EventKind::WorldInit),
                    observe(2, EventKind::ChunkLoad),
                    observe(3, EventKind::WorldLoad),
                ],
            },
            outbox: Box::new(mailbox.clone()),
        })
        .expect("open");
    let mut server = rspigot_core::server::Server::new(
        rspigot_core::config::GameConfig {
            view_distance: 3,
            chunk_workers: 2,
            spawn_protection: 0,
            ..rspigot_core::config::GameConfig::default()
        },
        rspigot_core::config::LevelData::default(),
        std::sync::Arc::new(NoStore),
        std::sync::Arc::new(Floor),
        rspigot_core::status::StatusBoard::new(20),
    )
    .with_startup_inbox(inbox);
    server.start(std::time::Duration::from_secs(10));
    let kinds: Vec<EventKind> = fired(&mailbox)
        .into_iter()
        .map(|fired| fired.payload.kind())
        .collect();
    assert_eq!(kinds.first(), Some(&EventKind::WorldInit));
    assert_eq!(kinds.last(), Some(&EventKind::WorldLoad));
    assert!(kinds.contains(&EventKind::ChunkLoad));
}

#[test]
fn a_shutdown_settles_the_pending_events_of_the_keyed_lanes_before_saving() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::TimeSkip)]);
    let before = day_time(&mut s);
    console(&mut s, "time add 10");
    assert_eq!(asked_for(&s.plugin, EventKind::TimeSkip).len(), 1);
    assert_eq!(day_time(&mut s), before);
    let (sender, inbound) = crossbeam_channel::unbounded();
    sender.send(Inbound::Shutdown).expect("open");
    let Scene { server, .. } = s;
    let level = server.run(&inbound);
    assert_eq!(level.day_time, before + 10);
}

#[test]
fn a_cancelled_thunder_change_alone_keeps_the_rain_and_the_clear_sky_above_it() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::ThunderChange)]);
    console(&mut s, "weather rain");
    let state = world_state(&mut s);
    assert!(state.raining && !state.thundering);
    console(&mut s, "weather thunder");
    assert!(asked_for(&s.plugin, EventKind::WeatherChange).is_empty());
    let asked = last_asked(&s.plugin, EventKind::ThunderChange);
    rule_on(&mut s.server, 1, asked.id, true, None);
    let state = world_state(&mut s);
    assert!(state.raining && !state.thundering);
}
