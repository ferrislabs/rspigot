mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData, PluginSettings};
use rspigot_core::events::{ClientEvent, EntityStatus, Inbound, InputState, PlayerInput};
use rspigot_core::plugins::{EventKind, EventPayload};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::{BlockEntity, ChunkColumn};
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Identifier, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;

const SENSOR: BlockPos = BlockPos::new(6, 64, 0);
const CALIBRATED: BlockPos = BlockPos::new(6, 64, 4);
const SHRIEKER: BlockPos = BlockPos::new(0, 64, 8);
const FAR_SENSOR: BlockPos = BlockPos::new(-14, 64, 0);

struct Sculk;

fn state_with(kind: BlockKind, properties: &[(&str, &str)]) -> BlockStateId {
    kind.state_with(properties).expect("a state")
}

impl ChunkGenerator for Sculk {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                for y in 60..=63 {
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        BlockStateId(1),
                    );
                }
            }
        }
        let placed = [
            (SENSOR, BlockKind::SculkSensor, "sculk_sensor", vec![]),
            (
                CALIBRATED,
                BlockKind::CalibratedSculkSensor,
                "calibrated_sculk_sensor",
                vec![],
            ),
            (
                SHRIEKER,
                BlockKind::SculkShrieker,
                "sculk_shrieker",
                vec![("can_summon", "true")],
            ),
            (FAR_SENSOR, BlockKind::SculkSensor, "sculk_sensor", vec![]),
        ];
        for (at, kind, name, properties) in placed {
            if at.chunk() == pos {
                chunk.set_block(at, state_with(kind, &properties));
                chunk.set_block_entity(BlockEntity {
                    pos: at,
                    kind: Identifier::minecraft(name),
                    data: Compound::new(),
                });
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

fn sculk_server() -> Server<Link> {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        plugins: PluginSettings {
            decision_ticks: 1_000,
            ..PluginSettings::default()
        },
        ..GameConfig::default()
    };
    let data = LevelData {
        spawn_mobs: false,
        day_time: 18_000,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(Sculk),
        StatusBoard::new(20),
    )
    .with_rng_seed(7);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn ready() -> Scene {
    ready_with(Vec::new())
}

fn ready_with(handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> Scene {
    let mut s = scene_on(sculk_server(), &["Alice"], handlers);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s.tick(2);
    s.players[0].link.take();
    s
}

fn walk(s: &mut Scene, from: Vec3, steps: i32, dx: f64) -> Vec3 {
    let mut at = from;
    for _ in 0..steps {
        at = Vec3::new(at.x + dx, at.y, at.z);
        send_input(
            &mut s.server,
            &s.players[0],
            PlayerInput::Move {
                position: Some(at),
                rotation: None,
                on_ground: true,
            },
        );
        s.tick(1);
    }
    at
}

fn step_on_shrieker(s: &mut Scene) {
    walk(s, Vec3::new(1.5, 64.0, 8.5), 1, 0.0);
    walk(s, Vec3::new(1.5, 64.0, 8.5), 1, 1.0);
    walk(s, Vec3::new(2.5, 65.0, 8.5), 1, -2.0);
}

fn updates(events: &[ClientEvent], pos: BlockPos) -> Vec<BlockStateId> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::BlockUpdate { pos: at, state } if *at == pos => Some(*state),
            _ => None,
        })
        .collect()
}

fn phases(events: &[ClientEvent], pos: BlockPos) -> Vec<String> {
    updates(events, pos)
        .into_iter()
        .filter_map(|state| state.property("sculk_sensor_phase").map(str::to_owned))
        .collect()
}

fn sounds(events: &[ClientEvent]) -> Vec<&'static str> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Sound { sound, .. } => Some(*sound),
            _ => None,
        })
        .collect()
}

#[test]
fn a_footstep_near_a_sculk_sensor_activates_it_after_the_travel_time_then_cools_it_down() {
    let mut s = ready();
    walk(&mut s, Vec3::new(0.5, 64.0, 0.5), 12, 0.3);
    let mut events = s.players[0].link.take();
    s.tick(40);
    events.extend(s.players[0].link.take());
    let seen = phases(&events, SENSOR);
    assert_eq!(seen, vec!["active", "cooldown", "inactive"]);
    assert!(sounds(&events).contains(&"minecraft:block.sculk_sensor.clicking"));
    assert!(sounds(&events).contains(&"minecraft:block.sculk_sensor.clicking_stop"));
}

#[test]
fn a_sensor_beyond_its_listener_radius_stays_silent() {
    let mut s = ready();
    walk(&mut s, Vec3::new(0.5, 64.0, 0.5), 12, 0.3);
    s.tick(40);
    let events = s.players[0].link.take();
    assert!(phases(&events, FAR_SENSOR).is_empty());
}

#[test]
fn the_calibrated_sensor_listens_further_and_clicks_for_a_shorter_time() {
    let mut s = ready();
    walk(&mut s, Vec3::new(-8.5, 64.0, 0.5), 14, 0.3);
    let mut events = s.players[0].link.take();
    s.tick(40);
    events.extend(s.players[0].link.take());
    assert_eq!(
        phases(&events, CALIBRATED),
        vec!["active", "cooldown", "inactive"]
    );
}

#[test]
fn a_sneaking_footstep_does_not_trigger_a_sensor() {
    let mut s = ready();
    send_input(
        &mut s.server,
        &s.players[0],
        PlayerInput::Input(InputState {
            shift: true,
            ..InputState::default()
        }),
    );
    walk(&mut s, Vec3::new(0.5, 64.0, 0.5), 12, 0.3);
    s.tick(40);
    let events = s.players[0].link.take();
    assert!(phases(&events, SENSOR).is_empty());
}

#[test]
fn standing_on_a_shrieker_makes_it_shriek_for_ninety_ticks() {
    let mut s = ready();
    step_on_shrieker(&mut s);
    let mut events = s.players[0].link.take();
    s.tick(100);
    events.extend(s.players[0].link.take());
    let shrieking: Vec<Option<String>> = updates(&events, SHRIEKER)
        .into_iter()
        .map(|state| state.property("shrieking").map(str::to_owned))
        .collect();
    assert_eq!(
        shrieking.first().and_then(Clone::clone).as_deref(),
        Some("true")
    );
    assert!(shrieking.iter().flatten().any(|value| value == "false"));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ClientEvent::LevelEvent { event: 3007, .. }))
    );
}

#[test]
fn a_warden_near_a_footstep_ticks_its_tendrils() {
    let mut s = ready();
    let warden = {
        s.server.handle(Inbound::Console(
            "summon minecraft:warden 3.5 64 0.5".to_owned(),
        ));
        s.server.deliver();
        s.server
            .entities_of(Kind::Warden)
            .first()
            .expect("a warden")
            .id
    };
    walk(&mut s, Vec3::new(0.5, 64.0, 0.5), 12, 0.0);
    walk(&mut s, Vec3::new(0.5, 64.0, 0.5), 12, 0.3);
    s.tick(20);
    let events = s.players[0].link.take();
    let tendrils = events.iter().any(|event| {
        matches!(
            event,
            ClientEvent::EntityStatus { entity, status: EntityStatus::Event(61) } if *entity == warden
        )
    });
    assert!(tendrils, "the warden did not hear the footsteps");
}

#[test]
fn a_listening_block_tells_plugins_about_the_game_event_it_receives() {
    let mut s = ready_with(vec![observe(1, EventKind::BlockReceiveGame)]);
    walk(&mut s, Vec3::new(0.5, 64.0, 0.5), 12, 0.3);
    let heard = settled_for(&s.plugin, EventKind::BlockReceiveGame);
    assert!(!heard.is_empty());
    let EventPayload::BlockReceiveGame(event) = &heard[0].payload else {
        panic!("a block receive game event");
    };
    assert_eq!(event.event, "minecraft:step");
}

#[test]
fn the_fourth_warning_of_a_summoning_shrieker_calls_a_warden() {
    let mut s = ready();
    for warning in 1..=4 {
        step_on_shrieker(&mut s);
        s.tick(220);
        let wardens = s.server.entities_of(Kind::Warden).len();
        assert_eq!(
            wardens,
            usize::from(warning == 4),
            "after warning {warning}"
        );
    }
    let events = s.players[0].link.take();
    assert!(sounds(&events).contains(&"minecraft:entity.warden.nearby_close"));
    let warden = s.server.entities_of(Kind::Warden);
    assert_eq!(warden.len(), 1);
}

#[test]
fn a_summoned_warden_agitates_as_it_emerges() {
    let mut s = ready();
    let mut heard = Vec::new();
    for _ in 1..=4 {
        step_on_shrieker(&mut s);
        s.tick(220);
        heard.extend(s.players[0].link.take());
    }
    assert!(sounds(&heard).contains(&"minecraft:entity.warden.agitated"));
    assert_eq!(s.server.entities_of(Kind::Warden).len(), 1);
}
