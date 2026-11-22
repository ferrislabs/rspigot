mod common;

use common::*;
use rspigot_core::commands::Weather;
use rspigot_core::events::{ClientEvent, GameEvent};
use rspigot_core::plugins::{
    Answer, BlockOrder, BorderOrder, ChunkOrder, ExtraAnswer, ExtraQuestion, Order, PlayerAction,
    PlayerOrder, Question, WorldAction, WorldOrder,
};
use rspigot_domain::{BlockPos, ChunkPos, Vec3};

fn world(s: &mut Scene, action: WorldAction) {
    s.act(Order::World(WorldOrder::Action(action)));
}

fn border(s: &mut Scene, order: BorderOrder) {
    world(s, WorldAction::Border(order));
}

fn chunk_info(s: &mut Scene, pos: ChunkPos, load: bool) -> (bool, bool) {
    match s.ask(Question::Extra(ExtraQuestion::Chunk { pos, load })) {
        Answer::Extra(ExtraAnswer::Chunk {
            loaded,
            force_loaded,
            ..
        }) => (loaded, force_loaded),
        other => panic!("expected a chunk answer, got {other:?}"),
    }
}

fn block_kind(s: &mut Scene, pos: BlockPos) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state.kind,
        other => panic!("expected a block, got {other:?}"),
    }
}

fn player_action(s: &mut Scene, action: PlayerAction) {
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::Action(action)));
}

#[test]
fn a_border_resize_without_time_is_broadcast_as_a_size() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();

    border(
        &mut s,
        BorderOrder::Resize {
            size: 100.0,
            ticks: 0,
        },
    );

    assert!(s.warnings().is_empty());
    assert!(
        alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::BorderSize(size) if *size == 100.0))
    );
}

#[test]
fn a_border_resize_with_time_lerps_from_the_current_size() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    border(
        &mut s,
        BorderOrder::Resize {
            size: 100.0,
            ticks: 0,
        },
    );
    alice.link.take();

    border(
        &mut s,
        BorderOrder::Resize {
            size: 40.0,
            ticks: 200,
        },
    );

    let events = alice.link.take();
    assert!(events.iter().any(|event| matches!(
        event,
        ClientEvent::BorderLerp { old_size, new_size, time }
            if *old_size == 100.0 && *new_size == 40.0 && *time == 200
    )));
}

#[test]
fn the_border_center_and_warnings_reach_the_clients() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();

    border(&mut s, BorderOrder::SetCenter { x: 10.5, z: -20.0 });
    border(&mut s, BorderOrder::SetWarningDistance(9));
    border(&mut s, BorderOrder::SetWarningTime(400));

    assert!(s.warnings().is_empty());
    let events = alice.link.take();
    assert!(events.iter().any(|event| matches!(
        event,
        ClientEvent::BorderCenter { x, z } if *x == 10.5 && *z == -20.0
    )));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ClientEvent::BorderWarningDistance(9)))
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ClientEvent::BorderWarningDelay(400)))
    );
}

#[test]
fn the_border_damage_settings_hurt_a_player_outside_it() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    border(
        &mut s,
        BorderOrder::Resize {
            size: 1.0,
            ticks: 0,
        },
    );
    border(
        &mut s,
        BorderOrder::SetCenter {
            x: 1000.0,
            z: 1000.0,
        },
    );
    border(&mut s, BorderOrder::SetDamageBuffer(0.0));
    border(&mut s, BorderOrder::SetDamageAmount(2.0));
    alice.link.take();

    s.tick(2);

    assert!(alice.link.take().iter().any(|event| matches!(
        event,
        ClientEvent::Health { health, .. } if *health < 20.0
    )));
}

#[test]
fn a_border_with_an_impossible_value_is_refused() {
    let mut s = scene(&["Alice"]);

    border(
        &mut s,
        BorderOrder::Resize {
            size: 0.5,
            ticks: 0,
        },
    );
    border(
        &mut s,
        BorderOrder::SetCenter {
            x: f64::NAN,
            z: 0.0,
        },
    );
    border(&mut s, BorderOrder::SetDamageAmount(-1.0));
    border(&mut s, BorderOrder::SetWarningDistance(-1));

    assert_eq!(s.warnings().len(), 4);
}

#[test]
fn a_lightning_strike_spawns_a_bolt_where_asked() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    alice.link.take();

    world(
        &mut s,
        WorldAction::StrikeLightning {
            position: Vec3::new(2.5, 64.0, 2.5),
            effect_only: false,
        },
    );

    assert!(s.warnings().is_empty());
    let bolts = match s.ask(Question::Entities {
        near: Some((Vec3::new(2.5, 64.0, 2.5), 4.0)),
        kind: Some("minecraft:lightning_bolt".to_owned()),
    }) {
        Answer::Entities(views) => views,
        other => panic!("expected entities, got {other:?}"),
    };
    assert_eq!(bolts.len(), 1);
    assert_eq!(bolts[0].position, Vec3::new(2.5, 64.0, 2.5));
}

#[test]
fn a_lightning_effect_spawns_a_bolt_and_a_nan_position_is_refused() {
    let mut s = scene(&["Alice"]);

    world(
        &mut s,
        WorldAction::StrikeLightning {
            position: Vec3::new(f64::NAN, 64.0, 0.0),
            effect_only: true,
        },
    );
    assert_eq!(s.warnings().len(), 1);

    world(
        &mut s,
        WorldAction::StrikeLightning {
            position: Vec3::new(0.5, 64.0, 0.5),
            effect_only: true,
        },
    );
    assert_eq!(s.warnings().len(), 1);
    match s.ask(Question::Entities {
        near: Some((Vec3::new(0.5, 64.0, 0.5), 4.0)),
        kind: Some("minecraft:lightning_bolt".to_owned()),
    }) {
        Answer::Entities(views) => assert_eq!(views.len(), 1),
        other => panic!("expected entities, got {other:?}"),
    }
}

#[test]
fn a_far_chunk_loads_on_demand_and_stays_reported_loaded() {
    let mut s = scene(&["Alice"]);
    let far = ChunkPos::new(40, 40);
    assert_eq!(chunk_info(&mut s, far, false), (false, false));

    world(&mut s, WorldAction::Chunk(ChunkOrder::Load(far)));

    assert!(s.warnings().is_empty());
    assert_eq!(chunk_info(&mut s, far, false), (true, false));
}

#[test]
fn getting_a_chunk_loads_it() {
    let mut s = scene(&["Alice"]);
    let far = ChunkPos::new(-40, 12);

    assert_eq!(chunk_info(&mut s, far, true), (true, false));
}

#[test]
fn a_force_loaded_chunk_is_marked_and_released() {
    let mut s = scene(&["Alice"]);
    let far = ChunkPos::new(30, -30);

    world(
        &mut s,
        WorldAction::Chunk(ChunkOrder::SetForceLoaded {
            pos: far,
            forced: true,
        }),
    );
    assert!(chunk_info(&mut s, far, false).1);
    s.server.wait_for_spawn(std::time::Duration::from_secs(10));
    assert_eq!(chunk_info(&mut s, far, false), (true, true));

    world(
        &mut s,
        WorldAction::Chunk(ChunkOrder::SetForceLoaded {
            pos: far,
            forced: false,
        }),
    );
    assert!(!chunk_info(&mut s, far, false).1);
}

#[test]
fn regenerating_a_chunk_restores_its_generated_terrain() {
    let mut s = scene(&["Alice"]);
    let pos = BlockPos::new(5, 62, 5);
    let original = block_kind(&mut s, pos);
    s.act(Order::Block {
        pos,
        order: BlockOrder::Set("minecraft:diamond_block".to_owned()),
    });
    assert_eq!(block_kind(&mut s, pos), "minecraft:diamond_block");

    world(
        &mut s,
        WorldAction::Chunk(ChunkOrder::Regenerate(pos.chunk())),
    );

    assert!(s.warnings().is_empty());
    assert_eq!(block_kind(&mut s, pos), original);
}

#[test]
fn regenerating_a_chunk_resends_it_to_the_players_who_have_it() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    stream_chunk_of(&mut s, BlockPos::new(0, 64, 0));
    alice.link.take();

    world(
        &mut s,
        WorldAction::Chunk(ChunkOrder::Regenerate(ChunkPos::new(0, 0))),
    );
    s.tick(3);

    assert!(alice.link.take().iter().any(
        |event| matches!(event, ClientEvent::Chunk { chunk, .. } if chunk.pos() == ChunkPos::new(0, 0))
    ));
}

#[test]
fn a_fixed_player_time_overrides_the_world_clock_for_that_player_only() {
    let mut s = scene(&["Alice", "Bob"]);
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());

    player_action(
        &mut s,
        PlayerAction::SetTime {
            time: 18_000,
            relative: false,
        },
    );

    assert!(alice.link.take().iter().any(|event| matches!(
        event,
        ClientEvent::Time {
            day_time: 18_000,
            advance_time: false,
            ..
        }
    )));
    assert!(
        !bob.link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::Time { .. }))
    );
}

#[test]
fn a_player_time_survives_the_world_clock_broadcast() {
    let mut s = scene(&["Alice", "Bob"]);
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    player_action(
        &mut s,
        PlayerAction::SetTime {
            time: 6_000,
            relative: false,
        },
    );
    alice.link.take();
    bob.link.take();

    s.tick(20);

    let last_time = |events: Vec<ClientEvent>| {
        events
            .into_iter()
            .filter_map(|event| match event {
                ClientEvent::Time { day_time, .. } => Some(day_time),
                _ => None,
            })
            .next_back()
    };
    assert_eq!(last_time(alice.link.take()), Some(6_000));
    assert_ne!(last_time(bob.link.take()), Some(6_000));
}

#[test]
fn a_relative_player_time_offsets_the_world_clock_and_keeps_ticking() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();

    player_action(
        &mut s,
        PlayerAction::SetTime {
            time: 1_000,
            relative: true,
        },
    );

    assert!(alice.link.take().iter().any(|event| matches!(
        event,
        ClientEvent::Time { day_time, advance_time: true, .. } if *day_time >= 1_000
    )));
    match s.ask(Question::Extra(ExtraQuestion::PlayerClock(alice.id))) {
        Answer::Extra(ExtraAnswer::Clock { time, weather }) => {
            assert_eq!(time, Some((1_000, true)));
            assert_eq!(weather, None);
        }
        other => panic!("expected a clock, got {other:?}"),
    }
}

#[test]
fn resetting_the_player_time_sends_the_world_time_again() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    player_action(
        &mut s,
        PlayerAction::SetTime {
            time: 18_000,
            relative: false,
        },
    );
    alice.link.take();

    player_action(&mut s, PlayerAction::ResetTime);

    assert!(alice.link.take().iter().any(|event| matches!(
        event,
        ClientEvent::Time {
            advance_time: true,
            ..
        }
    )));
    match s.ask(Question::Extra(ExtraQuestion::PlayerClock(alice.id))) {
        Answer::Extra(ExtraAnswer::Clock { time, .. }) => assert_eq!(time, None),
        other => panic!("expected a clock, got {other:?}"),
    }
}

#[test]
fn a_player_weather_changes_that_player_only() {
    let mut s = scene(&["Alice", "Bob"]);
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());

    player_action(&mut s, PlayerAction::SetWeather(Weather::Thunder));

    let events = alice.link.take();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ClientEvent::GameEvent(GameEvent::StartRaining)))
    );
    assert!(events.iter().any(|event| matches!(
        event,
        ClientEvent::GameEvent(GameEvent::ThunderLevelChange(level)) if *level == 1.0
    )));
    assert!(
        !bob.link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::GameEvent(_)))
    );
    match s.ask(Question::Extra(ExtraQuestion::PlayerClock(alice.id))) {
        Answer::Extra(ExtraAnswer::Clock { weather, .. }) => {
            assert_eq!(weather, Some(Weather::Thunder));
        }
        other => panic!("expected a clock, got {other:?}"),
    }
}

#[test]
fn resetting_the_player_weather_restores_the_world_weather() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    player_action(&mut s, PlayerAction::SetWeather(Weather::Rain));
    alice.link.take();

    player_action(&mut s, PlayerAction::ResetWeather);

    let events = alice.link.take();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ClientEvent::GameEvent(GameEvent::StopRaining)))
    );
    match s.ask(Question::Extra(ExtraQuestion::PlayerClock(alice.id))) {
        Answer::Extra(ExtraAnswer::Clock { weather, .. }) => assert_eq!(weather, None),
        other => panic!("expected a clock, got {other:?}"),
    }
}
