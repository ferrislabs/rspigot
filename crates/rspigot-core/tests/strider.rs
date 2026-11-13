mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData, PluginSettings};
use rspigot_core::events::{ClientEvent, Inbound, MetadataValue};
use rspigot_core::plugins::{EventKind, EventPayload, HandlerSpec};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Lake;

impl ChunkGenerator for Lake {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(0));
        for x in 0..16 {
            for z in 0..16 {
                let (bx, bz) = (pos.min_block_x() + x, pos.min_block_z() + z);
                let at = |y: i32| BlockPos::new(bx, y, bz);
                for y in 55..=62 {
                    chunk.set_block(at(y), BlockKind::Netherrack.default_state());
                }
                if bx >= 4 {
                    chunk.set_block(at(63), BlockKind::Lava.default_state());
                } else {
                    chunk.set_block(at(63), BlockKind::Netherrack.default_state());
                }
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

fn lake(seed: u64) -> Scene {
    lake_with(seed, Vec::new())
}

fn lake_with(seed: u64, handlers: Vec<HandlerSpec>) -> Scene {
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
    let level = LevelData {
        spawn_mobs: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        level,
        Arc::new(NoStore),
        Arc::new(Lake),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    let mut s = scene_on(server, &["Alice"], handlers);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn summon(s: &mut Scene, at: &str, nbt: &str) -> EntityId {
    console(s, &format!("summon minecraft:strider {at} {nbt}"));
    s.server.entities_of(Kind::Strider)[0].id
}

fn position(s: &Scene, id: EntityId) -> Vec3 {
    s.server
        .entities_of(Kind::Strider)
        .into_iter()
        .find(|e| e.id == id)
        .map(|e| e.position)
        .expect("the strider")
}

#[test]
fn a_strider_stands_on_the_surface_of_a_lava_lake_instead_of_sinking() {
    let mut s = lake(1);
    let strider = summon(&mut s, "8.5 66 8.5", "{}");
    s.tick(80);
    let at = position(&s, strider);
    assert!((at.y - 63.5).abs() < 0.05, "{at:?}");
}

#[test]
fn a_strider_on_netherrack_shivers_and_warms_up_in_lava() {
    let mut s = lake(1);
    s.players[0].link.take();
    let strider = summon(&mut s, "2.5 64 8.5", "{}");
    s.tick(3);
    let cold = shivering_updates(&s.players[0].link.take(), strider);
    assert_eq!(cold.last(), Some(&true), "{cold:?}");
    console(&mut s, "tp @e[type=strider] 8.5 66 8.5");
    s.tick(40);
    let warm = shivering_updates(&s.players[0].link.take(), strider);
    assert_eq!(warm.last(), Some(&false), "{warm:?}");
}

fn shivering_updates(events: &[ClientEvent], strider: EntityId) -> Vec<bool> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::EntityMetadata { entity, entries } if *entity == strider => entries
                .iter()
                .find(|entry| entry.index == 18)
                .and_then(|entry| match entry.value {
                    MetadataValue::Boolean(value) => Some(value),
                    _ => None,
                }),
            _ => None,
        })
        .collect()
}

#[test]
fn a_strider_walks_to_the_lava_and_stays_out_of_water_and_off_the_shore() {
    let mut s = lake(1);
    let strider = summon(&mut s, "1.5 64 8.5", "{}");
    s.tick(600);
    let at = position(&s, strider);
    assert!(at.x > 3.0, "{at:?}");
}

#[test]
fn some_striders_spawn_carrying_a_zombified_piglin_or_a_baby_strider() {
    let mut s = lake(9);
    for _ in 0..240 {
        console(&mut s, "summon minecraft:strider 8.5 66 8.5");
    }
    s.tick(2);
    let piglins = s.server.entities_of(Kind::ZombifiedPiglin);
    assert!(!piglins.is_empty(), "a zombified piglin jockey spawned");
    let saddled = s
        .server
        .entities_of(Kind::Strider)
        .into_iter()
        .filter(|strider| {
            s.server.saved_entity(strider.id).is_some_and(|nbt| {
                nbt.get("Passengers").is_some_and(|riders| {
                    format!("{riders:?}").contains("zombified_piglin")
                        && format!("{riders:?}").contains("warped_fungus_on_a_stick")
                }) && format!("{:?}", nbt.get("equipment")).contains("saddle")
            })
        })
        .count();
    assert!(saddled > 0, "the carrier wears a saddle");
    let stacked = s
        .server
        .entities_of(Kind::Strider)
        .into_iter()
        .filter(|strider| {
            s.server
                .saved_entity(strider.id)
                .is_some_and(|nbt| format!("{:?}", nbt.get("Passengers")).contains("strider"))
        })
        .count();
    assert!(stacked > 0, "a baby strider rides an adult");
}

fn ride_scene() -> (Scene, EntityId) {
    ride_scene_with(Vec::new())
}

fn ride_scene_with(handlers: Vec<HandlerSpec>) -> (Scene, EntityId) {
    let mut s = lake_with(1, handlers);
    console(&mut s, "give Alice minecraft:warped_fungus_on_a_stick 1");
    let strider = summon(
        &mut s,
        "1.5 64 0.5",
        "{NoAI:1b,equipment:{saddle:{id:\"minecraft:saddle\",count:1}}}",
    );
    s.tick(2);
    (s, strider)
}

fn interact(s: &mut Scene, target: EntityId, sneaking: bool) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        rspigot_core::events::PlayerInput::Interact {
            target,
            hand: rspigot_core::events::Hand::Main,
            sneaking,
            location: Vec3::new(0.0, 0.5, 0.0),
        },
    );
}

#[test]
fn a_player_mounts_a_saddled_strider_and_a_sneak_lets_them_down() {
    let (mut s, strider) = ride_scene();
    s.players[0].link.take();
    interact(&mut s, strider, false);
    s.tick(2);
    let events = s.players[0].link.take();
    assert!(
        events.iter().any(|event| matches!(event, ClientEvent::Passengers { vehicle, passengers } if *vehicle == strider && passengers.len() == 1)),
        "{events:?}"
    );
    send_input(
        &mut s.server,
        &s.players[0].clone(),
        rspigot_core::events::PlayerInput::Input(rspigot_core::events::InputState {
            shift: true,
            ..Default::default()
        }),
    );
    s.tick(2);
    let events = s.players[0].link.take();
    assert!(
        events.iter().any(|event| matches!(event, ClientEvent::Passengers { vehicle, passengers } if *vehicle == strider && passengers.is_empty())),
        "{events:?}"
    );
}

#[test]
fn the_saddled_rider_with_a_fungus_stick_steers_with_client_moves_and_boosts() {
    let (mut s, strider) = ride_scene();
    interact(&mut s, strider, false);
    s.tick(2);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        rspigot_core::events::PlayerInput::MoveVehicle {
            position: Vec3::new(2.5, 64.0, 0.5),
            rotation: rspigot_domain::Rotation::new(90.0, 0.0),
            on_ground: true,
        },
    );
    s.tick(1);
    let at = position(&s, strider);
    assert!((at.x - 2.5).abs() < 0.2, "{at:?}");
    s.players[0].link.take();
    send_input(
        &mut s.server,
        &alice,
        rspigot_core::events::PlayerInput::UseItem {
            hand: rspigot_core::events::Hand::Main,
            sequence: 1,
            rotation: rspigot_domain::Rotation::new(0.0, 0.0),
        },
    );
    s.tick(2);
    let boosts: Vec<i32> = s.players[0]
        .link
        .take()
        .iter()
        .filter_map(|event| match event {
            ClientEvent::EntityMetadata { entity, entries } if *entity == strider => entries
                .iter()
                .find(|entry| entry.index == 17)
                .and_then(|entry| match entry.value {
                    MetadataValue::VarInt(total) => Some(total),
                    _ => None,
                }),
            _ => None,
        })
        .collect();
    assert!(
        boosts.iter().any(|total| (140..=980).contains(total)),
        "{boosts:?}"
    );
}

#[test]
fn a_plugin_sees_the_vehicle_enter_then_the_mount_and_can_cancel_the_boarding() {
    let (mut s, strider) = ride_scene_with(vec![
        deciding(1, EventKind::VehicleEnter),
        observe(2, EventKind::EntityMount),
    ]);
    s.players[0].link.take();
    interact(&mut s, strider, false);
    let asked = last_asked(&s.plugin, EventKind::VehicleEnter);
    let EventPayload::VehicleEnter(enter) = &asked.payload else {
        panic!("a vehicle enter");
    };
    assert_eq!(enter.vehicle.id, strider);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(2);
    assert!(settled_for(&s.plugin, EventKind::EntityMount).is_empty());
    assert!(
        !s.players[0].link.take().iter().any(|event| matches!(
            event,
            ClientEvent::Passengers { vehicle, .. } if *vehicle == strider
        )),
        "the boarding was cancelled"
    );
    s.tick(70);
    interact(&mut s, strider, false);
    let asked = last_asked(&s.plugin, EventKind::VehicleEnter);
    rule_on(&mut s.server, 1, asked.id, false, None);
    s.tick(2);
    assert!(!settled_for(&s.plugin, EventKind::EntityMount).is_empty());
}
