mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_core::config::{GameConfig, LevelData, PluginSettings};
use rspigot_core::events::{ClientEvent, EntityKind as WireKind, Inbound};
use rspigot_core::plugins::HandlerSpec;
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::damage::DamageType;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Plains;

impl ChunkGenerator for Plains {
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
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

fn plains_server(adjust: impl FnOnce(&mut GameConfig)) -> Server<Link> {
    let mut config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        plugins: PluginSettings {
            decision_ticks: 1_000,
            ..PluginSettings::default()
        },
        ..GameConfig::default()
    };
    adjust(&mut config);
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
        Arc::new(Plains),
        StatusBoard::new(20),
    )
    .with_rng_seed(7);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn ready(handlers: Vec<HandlerSpec>) -> Scene {
    ready_with(handlers, |_| {})
}

fn ready_with(handlers: Vec<HandlerSpec>, adjust: impl FnOnce(&mut GameConfig)) -> Scene {
    let mut s = scene_on(plains_server(adjust), &["Alice"], handlers);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn summon(s: &mut Scene, kind: Kind, nbt: &str, at: Vec3) -> EntityId {
    let before = s.server.entities_of(kind).len();
    let name = kind.name();
    console(
        s,
        &format!("summon {name} {} {} {} {nbt}", at.x, at.y, at.z),
    );
    let found = s.server.entities_of(kind);
    assert_eq!(found.len(), before + 1, "{name} was not summoned");
    found.last().unwrap().id
}

fn health(s: &Scene, kind: Kind, id: EntityId) -> f32 {
    s.server
        .entities_of(kind)
        .into_iter()
        .find(|entity| entity.id == id)
        .and_then(|entity| entity.health)
        .expect("a living entity")
}

fn exists(s: &Scene, kind: Kind, id: EntityId) -> bool {
    s.server
        .entities_of(kind)
        .into_iter()
        .any(|entity| entity.id == id)
}

fn snapshot(s: &Scene, kind: Kind, id: EntityId) -> rspigot_core::entities::EntitySnapshot {
    s.server
        .entities_of(kind)
        .into_iter()
        .find(|entity| entity.id == id)
        .expect("the entity exists")
}

fn spawned(events: &[ClientEvent], kind: Kind) -> Vec<(EntityId, Vec3, i32)> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::SpawnEntity {
                entity,
                kind: WireKind::Other(id),
                position,
                data,
                ..
            } if *id == kind.id().0 => Some((*entity, *position, *data)),
            _ => None,
        })
        .collect()
}

fn damage_of(events: &[ClientEvent], id: EntityId, kind: DamageType) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(event, ClientEvent::DamageEvent { entity, kind: found, .. }
                if *entity == id && *found == kind)
        })
        .count()
}

fn gust_bursts(events: &[ClientEvent]) -> Vec<(Vec3, f32, bool, &'static str)> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Explode {
                center,
                radius,
                large,
                gust: Some(sound),
                ..
            } => Some((*center, *radius, *large, *sound)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_wind_charge_flies_straight_at_a_constant_speed() {
    let mut s = ready(Vec::new());
    let charge = summon(
        &mut s,
        Kind::BreezeWindCharge,
        "{Motion:[0.0d,0.0d,0.5d]}",
        Vec3::new(30.5, 70.0, 20.5),
    );
    for tick in 1..=10 {
        s.tick(1);
        let flying = snapshot(&s, Kind::BreezeWindCharge, charge);
        assert_eq!(flying.velocity, Vec3::new(0.0, 0.0, 0.5), "tick {tick}");
        assert!(
            (flying.position.z - (20.5 + 0.5 * f64::from(tick))).abs() < 1.0e-9,
            "tick {tick}"
        );
        assert_eq!(flying.position.y, 70.0);
    }
}

#[test]
fn a_breeze_wind_charge_hitting_the_ground_bursts_with_the_big_gust_and_pushes_a_cow() {
    let mut s = ready(Vec::new());
    let cow = summon(&mut s, Kind::Cow, "{NoAI:1b}", Vec3::new(30.5, 64.0, 30.5));
    let charge = summon(
        &mut s,
        Kind::BreezeWindCharge,
        "{Motion:[0.0d,-1.0d,0.0d]}",
        Vec3::new(31.5, 66.0, 30.5),
    );
    s.players[0].link.take();
    s.tick(3);
    assert!(!exists(&s, Kind::BreezeWindCharge, charge));
    let events = s.players[0].link.take();
    let bursts = gust_bursts(&events);
    assert_eq!(bursts.len(), 1, "{events:?}");
    let (center, radius, large, sound) = bursts[0];
    assert_eq!(radius, 3.0);
    assert!(large);
    assert_eq!(sound, "minecraft:entity.breeze.wind_burst");
    assert_eq!(center.x, 31.5);
    assert!((center.y - 64.25).abs() < 1.0e-2, "{}", center.y);
    let pushed = snapshot(&s, Kind::Cow, cow).velocity;
    assert!(pushed.x < 0.0, "{pushed:?}");
    assert!(pushed.y > 0.0, "{pushed:?}");
    assert_eq!(health(&s, Kind::Cow, cow), 10.0);
}

#[test]
fn a_player_wind_charge_bursts_small_with_its_own_sound_and_a_larger_knockback_multiplier() {
    let mut s = ready(Vec::new());
    let cow = summon(&mut s, Kind::Cow, "{NoAI:1b}", Vec3::new(30.5, 64.0, 30.5));
    summon(
        &mut s,
        Kind::WindCharge,
        "{Motion:[0.0d,-1.0d,0.0d]}",
        Vec3::new(31.5, 66.0, 30.5),
    );
    s.players[0].link.take();
    s.tick(3);
    let events = s.players[0].link.take();
    let bursts = gust_bursts(&events);
    assert_eq!(bursts.len(), 1, "{events:?}");
    assert_eq!(bursts[0].1, 1.2);
    assert!(!bursts[0].2);
    assert_eq!(bursts[0].3, "minecraft:entity.wind_charge.wind_burst");
    assert!(snapshot(&s, Kind::Cow, cow).velocity.length() > 0.0);
}

#[test]
fn a_wind_charge_hitting_a_zombie_hurts_it_for_one_before_armor_then_bursts() {
    let mut s = ready(Vec::new());
    let zombie = summon(
        &mut s,
        Kind::Zombie,
        "{NoAI:1b}",
        Vec3::new(30.5, 64.0, 30.5),
    );
    let charge = summon(
        &mut s,
        Kind::BreezeWindCharge,
        "{Motion:[0.0d,0.0d,0.5d]}",
        Vec3::new(30.5, 65.0, 26.5),
    );
    let before = health(&s, Kind::Zombie, zombie);
    s.players[0].link.take();
    s.tick(8);
    assert!(!exists(&s, Kind::BreezeWindCharge, charge));
    let events = s.players[0].link.take();
    assert_eq!(damage_of(&events, zombie, DamageType::WindCharge), 1);
    assert_eq!(gust_bursts(&events).len(), 1);
    let lost = before - health(&s, Kind::Zombie, zombie);
    assert!((lost - 0.94).abs() < 1.0e-3, "{lost}");
}

#[test]
fn a_wind_charge_leaves_the_world_with_a_burst_far_above_the_build_limit() {
    let mut s = ready(Vec::new());
    let charge = summon(
        &mut s,
        Kind::BreezeWindCharge,
        "{Motion:[0.0d,0.0d,0.0d]}",
        Vec3::new(30.5, 400.0, 30.5),
    );
    s.players[0].link.take();
    s.tick(2);
    assert!(!exists(&s, Kind::BreezeWindCharge, charge));
}

#[test]
fn a_wind_charge_round_trips_with_no_acceleration() {
    let mut s = ready(Vec::new());
    let charge = summon(
        &mut s,
        Kind::BreezeWindCharge,
        "{Motion:[0.0d,0.0d,0.5d]}",
        Vec3::new(30.5, 70.0, 20.5),
    );
    let saved = s.server.saved_entity(charge).expect("a saved charge");
    assert_eq!(saved.get_f64("acceleration_power"), Some(0.0));
    assert!(!saved.contains_key("ExplosionPower"));
}

#[test]
fn a_summoned_breeze_has_a_persisted_brain_and_shoots_a_wind_charge_at_a_player() {
    let mut s = ready(Vec::new());
    let breeze = summon(&mut s, Kind::Breeze, "{}", Vec3::new(8.5, 64.0, 8.5));
    s.tick(1);
    assert!(s.server.saved_entity(breeze).unwrap().contains_key("Brain"));
    let mut shots = 0;
    for _ in 0..400 {
        s.tick(1);
        shots += spawned(&s.players[0].link.take(), Kind::BreezeWindCharge).len();
        if shots > 0 {
            break;
        }
    }
    assert!(shots > 0, "the breeze never shot");
}
