mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_core::config::{GameConfig, LevelData, PluginSettings};
use rspigot_core::events::{ClientEvent, Inbound, PlayerInput};
use rspigot_core::plugins::{EventKind, EventPayload, HandlerSpec};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::damage::DamageType;
use rspigot_domain::effect::{EffectDuration, EffectId};
use rspigot_domain::world::{Difficulty, Dimension};
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
        difficulty: Difficulty::Normal,
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

fn saved(s: &Scene, id: EntityId) -> rspigot_nbt::Compound {
    s.server.saved_entity(id).expect("a saved entity")
}

fn uuid_ints(uuid: uuid::Uuid) -> [i32; 4] {
    let (high, low) = uuid.as_u64_pair();
    [
        (high >> 32) as i32,
        high as i32,
        (low >> 32) as i32,
        low as i32,
    ]
}

fn uuid_text(uuid: uuid::Uuid) -> String {
    let [a, b, c, d] = uuid_ints(uuid);
    format!("[I;{a},{b},{c},{d}]")
}

fn sounds(events: &[ClientEvent], wanted: &str) -> Vec<(f32, f32)> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Sound {
                sound,
                volume,
                pitch,
                ..
            } if *sound == wanted => Some((*volume, *pitch)),
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

fn alice(s: &Scene) -> EntityId {
    s.server.player_entity_id(s.players[0].id).unwrap()
}

fn effect_names(s: &Scene, id: EntityId) -> Vec<(String, i32)> {
    saved(s, id)
        .get_list("active_effects")
        .map(|effects| {
            effects
                .iter()
                .filter_map(|effect| {
                    let effect = effect.as_compound()?;
                    Some((
                        effect.get_str("id")?.to_owned(),
                        effect.get_i32("duration")?,
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn levitation_applied(events: &[ClientEvent]) -> Vec<(u8, EffectDuration)> {
    let wanted = EffectId::from_name("minecraft:levitation").unwrap();
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::EffectApplied { effect, .. } if effect.effect == wanted => {
                Some((effect.amplifier, effect.duration))
            }
            _ => None,
        })
        .collect()
}

fn bullet_at(s: &mut Scene, nbt: &str, at: Vec3) -> EntityId {
    summon(s, Kind::ShulkerBullet, nbt, at)
}

fn hit_scene() -> (Scene, EntityId, EntityId) {
    let mut s = ready(vec![deciding(1, EventKind::ProjectileHit)]);
    let zombie = summon(&mut s, Kind::Zombie, "{NoAI:1b}", AWAY);
    let bullet = bullet_at(
        &mut s,
        "{Motion:[0.0d,0.0d,0.6d]}",
        Vec3::new(AWAY.x, AWAY.y + 0.9, AWAY.z - 4.0),
    );
    for _ in 0..12 {
        s.tick(1);
        if !asked_for(&s.plugin, EventKind::ProjectileHit).is_empty() {
            break;
        }
    }
    (s, zombie, bullet)
}

const AWAY: Vec3 = Vec3::new(30.5, 64.0, 30.5);
const FAR: Vec3 = Vec3::new(20.5, 120.0, 20.5);

#[test]
fn a_bullet_without_a_target_falls_at_four_hundredths_per_tick_and_never_slows() {
    let mut s = ready(Vec::new());
    let id = bullet_at(&mut s, "{Motion:[0.3d,0.0d,0.0d]}", FAR);
    for tick in 1..=12 {
        s.tick(1);
        let bullet = snapshot(&s, Kind::ShulkerBullet, id);
        assert!(
            (bullet.velocity.y + 0.04 * f64::from(tick)).abs() < 1.0e-9,
            "tick {tick}: {:?}",
            bullet.velocity
        );
        assert_eq!(bullet.velocity.x, 0.3, "tick {tick}");
        assert_eq!(bullet.velocity.z, 0.0);
    }
}

#[test]
fn a_homing_bullet_steers_a_fifth_of_the_way_to_its_growing_target_delta_each_tick() {
    let mut s = ready(Vec::new());
    let target = uuid_text(s.players[0].id.0);
    let start = Vec3::new(10.5, 70.5, 10.5);
    let id = bullet_at(
        &mut s,
        &format!("{{Target:{target},TXD:0.15d,TYD:0.0d,TZD:0.0d,Steps:50,Dir:5b}}"),
        start,
    );
    let mut delta = 0.15;
    let mut velocity = 0.0;
    let mut position = start.x;
    for tick in 1..=6 {
        s.tick(1);
        delta = (delta * 1.025f64).clamp(-1.0, 1.0);
        velocity += (delta - velocity) * 0.2;
        position += velocity;
        let bullet = snapshot(&s, Kind::ShulkerBullet, id);
        assert!(
            (bullet.velocity.x - velocity).abs() < 1.0e-12,
            "tick {tick}"
        );
        assert!(
            (bullet.position.x - position).abs() < 1.0e-12,
            "tick {tick}"
        );
        assert_eq!(bullet.velocity.y, 0.0);
        assert_eq!(saved(&s, id).get_i32("Steps"), Some(50 - tick));
    }
}

#[test]
fn a_bullet_hurts_a_player_for_four_and_levitates_them_for_ten_seconds_then_disappears() {
    let mut s = ready(Vec::new());
    let target = uuid_text(s.players[0].id.0);
    let bullet = bullet_at(
        &mut s,
        &format!("{{Target:{target},TXD:-0.15d,TYD:0.0d,TZD:0.0d}}"),
        Vec3::new(4.5, 64.9, 0.5),
    );
    let alice = alice(&s);
    s.players[0].link.take();
    let mut events = Vec::new();
    for _ in 0..60 {
        s.tick(1);
        events.extend(s.players[0].link.take());
        if !exists(&s, Kind::ShulkerBullet, bullet) {
            break;
        }
    }
    assert!(!exists(&s, Kind::ShulkerBullet, bullet));
    assert_eq!(damage_of(&events, alice, DamageType::MobProjectile), 1);
    assert_eq!(s.server.player_health(s.players[0].id), Some(16.0));
    assert_eq!(
        levitation_applied(&events),
        vec![(0, EffectDuration::Ticks(200))]
    );
}

#[test]
fn a_bullet_owned_by_a_mob_is_softened_by_easy_difficulty_for_a_player() {
    let mut s = ready_with(Vec::new(), |config| config.difficulty = Difficulty::Easy);
    let owner = summon(&mut s, Kind::Zombie, "{NoAI:1b}", AWAY);
    let owner_uuid = uuid_text(snapshot(&s, Kind::Zombie, owner).uuid);
    let target = uuid_text(s.players[0].id.0);
    let bullet = bullet_at(
        &mut s,
        &format!("{{Owner:{owner_uuid},Target:{target},TXD:-0.15d,TYD:0.0d,TZD:0.0d}}"),
        Vec3::new(4.5, 64.9, 0.5),
    );
    for _ in 0..60 {
        s.tick(1);
        if !exists(&s, Kind::ShulkerBullet, bullet) {
            break;
        }
    }
    assert!(!exists(&s, Kind::ShulkerBullet, bullet));
    assert_eq!(s.server.player_health(s.players[0].id), Some(17.0));
}

#[test]
fn a_bullet_hurts_and_levitates_a_mob_in_its_way_whoever_it_was_aimed_at() {
    let mut s = ready(Vec::new());
    let zombie = summon(&mut s, Kind::Zombie, "{NoAI:1b}", AWAY);
    let bullet = bullet_at(
        &mut s,
        "{Motion:[0.0d,0.0d,0.6d]}",
        Vec3::new(AWAY.x, AWAY.y + 0.9, AWAY.z - 4.0),
    );
    s.tick(10);
    assert!(!exists(&s, Kind::ShulkerBullet, bullet));
    let hurt = snapshot(&s, Kind::Zombie, zombie).health.unwrap();
    assert!(
        (hurt - (20.0 - 4.0 * (1.0 - 0.4 / 25.0))).abs() < 1.0e-4,
        "{hurt}"
    );
    let effects = effect_names(&s, zombie);
    assert_eq!(effects.len(), 1, "{effects:?}");
    assert_eq!(effects[0].0, "minecraft:levitation");
    assert!(effects[0].1 > 180 && effects[0].1 <= 200, "{effects:?}");
}

#[test]
fn a_bullet_that_hits_a_block_pops_in_an_explosion_particle_with_a_sound() {
    let mut s = ready(Vec::new());
    let bullet = bullet_at(
        &mut s,
        "{Motion:[0.0d,-0.5d,0.0d]}",
        Vec3::new(6.5, 66.0, 6.5),
    );
    s.players[0].link.take();
    let mut events = Vec::new();
    for _ in 0..12 {
        s.tick(1);
        events.extend(s.players[0].link.take());
        if !exists(&s, Kind::ShulkerBullet, bullet) {
            break;
        }
    }
    assert!(!exists(&s, Kind::ShulkerBullet, bullet));
    let particles: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Particle(particle) if particle.kind == "minecraft:explosion" => {
                Some((particle.count, particle.speed))
            }
            _ => None,
        })
        .collect();
    assert_eq!(particles, vec![(2, 0.0)]);
    assert_eq!(
        sounds(&events, "minecraft:entity.shulker_bullet.hit"),
        vec![(1.0, 1.0)]
    );
}

#[test]
fn hitting_a_bullet_shoots_it_down_with_crit_particles_and_the_hurt_sound() {
    let mut s = ready(Vec::new());
    let bullet = bullet_at(
        &mut s,
        "{Motion:[0.0d,0.0d,0.0d]}",
        Vec3::new(0.5, 64.6, 2.2),
    );
    let alice_player = s.players[0].clone();
    s.players[0].link.take();
    send_input(&mut s.server, &alice_player, PlayerInput::Attack(bullet));
    assert!(!exists(&s, Kind::ShulkerBullet, bullet));
    let events = s.players[0].link.take();
    assert_eq!(
        sounds(&events, "minecraft:entity.shulker_bullet.hurt"),
        vec![(1.0, 1.0)]
    );
    let crits: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Particle(particle) if particle.kind == "minecraft:crit" => {
                Some(particle.count)
            }
            _ => None,
        })
        .collect();
    assert_eq!(crits, vec![15]);
    assert_eq!(s.server.player_health(s.players[0].id), Some(20.0));
}

#[test]
fn an_entity_hit_asks_the_plugins_and_a_cancel_lets_the_bullet_fly_on() {
    let (mut s, zombie, bullet) = hit_scene();
    let asked = last_asked(&s.plugin, EventKind::ProjectileHit);
    let EventPayload::ProjectileHit(hit) = &asked.payload else {
        panic!("a projectile hit");
    };
    assert_eq!(hit.entity.id, bullet);
    assert_eq!(hit.entity_type, "minecraft:shulker_bullet");
    assert_eq!(hit.hit_entity.as_ref().map(|view| view.id), Some(zombie));
    s.tick(3);
    assert!(exists(&s, Kind::ShulkerBullet, bullet));
    assert_eq!(snapshot(&s, Kind::Zombie, zombie).health, Some(20.0));
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(exists(&s, Kind::ShulkerBullet, bullet));
    assert_eq!(snapshot(&s, Kind::Zombie, zombie).health, Some(20.0));
}

#[test]
fn an_allowed_entity_hit_lands_the_damage_and_removes_the_bullet() {
    let (mut s, zombie, bullet) = hit_scene();
    let asked = last_asked(&s.plugin, EventKind::ProjectileHit);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert!(!exists(&s, Kind::ShulkerBullet, bullet));
    let hurt = snapshot(&s, Kind::Zombie, zombie).health.unwrap();
    assert!(
        (hurt - (20.0 - 4.0 * (1.0 - 0.4 / 25.0))).abs() < 1.0e-4,
        "{hurt}"
    );
}

#[test]
fn a_block_hit_is_announced_with_the_block_and_the_face() {
    let mut s = ready(vec![observe(2, EventKind::ProjectileHit)]);
    let bullet = bullet_at(
        &mut s,
        "{Motion:[0.0d,-0.5d,0.0d]}",
        Vec3::new(20.5, 66.0, 20.5),
    );
    s.tick(12);
    assert!(!exists(&s, Kind::ShulkerBullet, bullet));
    let settled = settled_for(&s.plugin, EventKind::ProjectileHit);
    let EventPayload::ProjectileHit(hit) = &settled.last().expect("a hit").payload else {
        panic!("a projectile hit");
    };
    assert_eq!(hit.hit_block, Some(BlockPos::new(20, 63, 20)));
    assert!(hit.hit_entity.is_none());
}

#[test]
fn the_bullet_keeps_its_flight_state_across_a_save() {
    let mut s = ready(Vec::new());
    let target = uuid_text(s.players[0].id.0);
    let id = bullet_at(
        &mut s,
        &format!("{{Target:{target},TXD:0.1d,TYD:0.0d,TZD:-0.1d,Steps:7,Dir:3b}}"),
        FAR,
    );
    let data = saved(&s, id);
    assert_eq!(data.get_i32("Steps"), Some(7));
    assert_eq!(data.get_f64("TXD"), Some(0.1));
    assert_eq!(data.get_f64("TZD"), Some(-0.1));
    assert_eq!(data.get_i64("Dir"), Some(3));
    assert_eq!(
        data.get("Target").cloned(),
        Some(rspigot_nbt::Tag::IntArray(
            uuid_ints(s.players[0].id.0).to_vec()
        ))
    );
}
