mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_core::config::{GameConfig, LevelData, PluginSettings};
use rspigot_core::events::{ClientEvent, EntityKind as WireKind, Inbound};
use rspigot_core::plugins::{BlockOrder, EventKind, EventPayload, HandlerSpec, Order};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
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

fn plains_server(seed: u64, adjust: impl FnOnce(&mut GameConfig)) -> Server<Link> {
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
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn ready(handlers: Vec<HandlerSpec>) -> Scene {
    ready_with(handlers, |_| {})
}

fn ready_with(handlers: Vec<HandlerSpec>, adjust: impl FnOnce(&mut GameConfig)) -> Scene {
    ready_seeded(7, handlers, adjust)
}

fn ready_seeded(
    seed: u64,
    handlers: Vec<HandlerSpec>,
    adjust: impl FnOnce(&mut GameConfig),
) -> Scene {
    let mut s = scene_on(plains_server(seed, adjust), &["Alice"], handlers);
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

fn health(s: &Scene, id: EntityId) -> f32 {
    snapshot(s, Kind::Shulker, id)
        .health
        .expect("a living entity")
}

fn saved(s: &Scene, id: EntityId) -> rspigot_nbt::Compound {
    s.server.saved_entity(id).expect("a saved entity")
}

fn peek(s: &Scene, id: EntityId) -> i64 {
    saved(s, id).get_i64("Peek").expect("a peek amount")
}

fn face(s: &Scene, id: EntityId) -> i64 {
    saved(s, id).get_i64("AttachFace").expect("an attach face")
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

fn level_events(events: &[ClientEvent], wanted: i32) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, ClientEvent::LevelEvent { event, .. } if *event == wanted))
        .count()
}

fn set_block(s: &mut Scene, pos: BlockPos, name: &str) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::Set(name.to_owned()),
    });
}

const SPOT: Vec3 = Vec3::new(10.5, 64.0, 10.5);
const AWAY: Vec3 = Vec3::new(30.5, 64.0, 30.5);
const FAR: Vec3 = Vec3::new(20.5, 120.0, 20.5);

fn modifiers_of_armor(s: &Scene, id: EntityId) -> Vec<String> {
    saved(s, id)
        .get_list("attributes")
        .map(|list| {
            list.iter()
                .filter_map(|entry| entry.as_compound())
                .filter(|entry| entry.get_str("id") == Some("minecraft:armor"))
                .filter_map(|entry| entry.get_list("modifiers"))
                .flat_map(|modifiers| {
                    modifiers
                        .iter()
                        .filter_map(|modifier| modifier.as_compound())
                        .filter_map(|modifier| modifier.get_str("id").map(str::to_owned))
                        .collect::<Vec<_>>()
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

#[test]
fn a_summoned_shulker_is_closed_on_the_floor_with_thirty_health_and_no_colour() {
    let mut s = ready(Vec::new());
    let id = summon(&mut s, Kind::Shulker, "{}", Vec3::new(10.9, 64.0, 10.1));
    let shulker = snapshot(&s, Kind::Shulker, id);
    assert_eq!(shulker.position, SPOT);
    assert_eq!(health(&s, id), 30.0);
    assert_eq!(face(&s, id), 0);
    assert_eq!(peek(&s, id), 0);
    assert_eq!(saved(&s, id).get_i64("Color"), Some(16));
}

#[test]
fn the_attach_face_peek_and_colour_survive_a_reload_of_the_save_data() {
    let mut s = ready(Vec::new());
    set_block(&mut s, BlockPos::new(9, 64, 10), "minecraft:stone");
    let id = summon(
        &mut s,
        Kind::Shulker,
        "{AttachFace:4b,Peek:30b,Color:11b}",
        SPOT,
    );
    s.tick(1);
    let data = saved(&s, id);
    assert_eq!(data.get_i64("AttachFace"), Some(4));
    assert_eq!(data.get_i64("Peek"), Some(30));
    assert_eq!(data.get_i64("Color"), Some(11));
}

#[test]
fn a_shulker_whose_floor_disappears_attaches_to_a_neighbouring_wall() {
    let mut s = ready(Vec::new());
    set_block(&mut s, BlockPos::new(11, 64, 10), "minecraft:stone");
    let id = summon(&mut s, Kind::Shulker, "{NoAI:1b}", SPOT);
    s.tick(2);
    assert_eq!(face(&s, id), 0);
    set_block(&mut s, BlockPos::new(10, 63, 10), "minecraft:air");
    s.tick(2);
    assert_eq!(face(&s, id), 5);
    assert_eq!(snapshot(&s, Kind::Shulker, id).position, SPOT);
}

#[test]
fn a_shulker_with_nothing_to_hold_on_to_stays_put_when_no_spot_is_found() {
    let mut s = ready(Vec::new());
    let id = summon(&mut s, Kind::Shulker, "{NoAI:1b}", FAR);
    s.tick(5);
    assert_eq!(snapshot(&s, Kind::Shulker, id).position, FAR);
}

#[test]
fn a_shulker_never_moves_or_falls_whatever_pushes_it() {
    let mut s = ready(Vec::new());
    let id = summon(&mut s, Kind::Shulker, "{NoAI:1b}", FAR);
    s.tick(20);
    let shulker = snapshot(&s, Kind::Shulker, id);
    assert_eq!(shulker.position, FAR);
    assert_eq!(shulker.velocity, Vec3::ZERO);
}

#[test]
fn an_idle_shulker_peeks_for_a_while_then_closes_with_sounds_and_covers_itself_in_armor() {
    let mut s = ready_with(Vec::new(), |config| {
        config.difficulty = Difficulty::Peaceful
    });
    let id = summon(&mut s, Kind::Shulker, "{}", Vec3::new(6.5, 64.0, 6.5));
    assert!(modifiers_of_armor(&s, id).is_empty());
    s.players[0].link.take();
    let (mut opened, mut closed) = (0, 0);
    let mut seen = Vec::new();
    for _ in 0..900 {
        s.tick(1);
        let events = s.players[0].link.take();
        opened += sounds(&events, "minecraft:entity.shulker.open").len();
        closed += sounds(&events, "minecraft:entity.shulker.close").len();
        let raw = peek(&s, id);
        if seen.last() != Some(&raw) {
            seen.push(raw);
        }
        if raw == 30 {
            assert!(modifiers_of_armor(&s, id).is_empty());
        }
        if opened > 0 && closed > 0 {
            break;
        }
    }
    assert_eq!(seen, vec![0, 30, 0]);
    assert_eq!((opened, closed), (1, 1));
    assert_eq!(modifiers_of_armor(&s, id), vec!["minecraft:covered"]);
}

#[test]
fn a_closed_shulker_takes_no_arrow_damage_but_an_open_one_does() {
    let mut s = ready(Vec::new());
    let closed = summon(&mut s, Kind::Shulker, "{NoAI:1b}", SPOT);
    let open = summon(
        &mut s,
        Kind::Shulker,
        "{NoAI:1b,Peek:30b}",
        Vec3::new(14.5, 64.0, 10.5),
    );
    for x in [10.5, 14.5] {
        summon(
            &mut s,
            Kind::Arrow,
            "{Motion:[0.0d,0.0d,1.0d]}",
            Vec3::new(x, 64.5, 6.5),
        );
    }
    s.tick(8);
    assert_eq!(health(&s, closed), 30.0);
    assert!(health(&s, open) < 30.0, "{}", health(&s, open));
}

#[test]
fn opening_the_lid_lifts_the_entities_resting_in_its_way() {
    let mut s = ready(Vec::new());
    summon(&mut s, Kind::Shulker, "{NoAI:1b,Peek:100b}", AWAY);
    let zombie = summon(
        &mut s,
        Kind::Zombie,
        "{NoAI:1b}",
        Vec3::new(30.5, 65.0, 30.5),
    );
    s.tick(60);
    let lifted = snapshot(&s, Kind::Zombie, zombie).position.y;
    assert!((lifted - 66.0).abs() < 0.05, "{lifted}");
}

const SEEDS: u64 = 60;

fn volley(s: &mut Scene) {
    summon(
        s,
        Kind::Arrow,
        "{Motion:[0.0d,0.0d,1.0d]}",
        Vec3::new(AWAY.x, AWAY.y + 0.5, AWAY.z - 4.0),
    );
    s.tick(5);
}

const NEAR: Vec3 = Vec3::new(6.5, 64.0, 6.5);

fn volley_at(s: &mut Scene, at: Vec3) {
    summon(
        s,
        Kind::Arrow,
        "{Motion:[0.0d,0.0d,1.0d]}",
        Vec3::new(at.x, at.y + 0.5, at.z - 4.0),
    );
}

struct Teleported {
    scene: Scene,
    id: EntityId,
    events: Vec<ClientEvent>,
    to: Vec3,
    had_target: bool,
}

fn first_teleport(difficulty: Difficulty) -> Teleported {
    for seed in 0..SEEDS {
        let mut s = ready_seeded(seed, Vec::new(), |config| config.difficulty = difficulty);
        let id = summon(&mut s, Kind::Shulker, "{Health:16f,Peek:30b}", NEAR);
        if difficulty != Difficulty::Peaceful {
            for _ in 0..80 {
                s.tick(1);
                if s.server.mob_target(id).is_some() {
                    break;
                }
            }
        }
        s.players[0].link.take();
        let mut events = Vec::new();
        for _ in 0..12 {
            if !exists(&s, Kind::Shulker, id) {
                break;
            }
            volley_at(&mut s, NEAR);
            for _ in 0..5 {
                let had_target = s.server.mob_target(id).is_some();
                s.tick(1);
                events.extend(s.players[0].link.take());
                let at = snapshot(&s, Kind::Shulker, id).position;
                if at != NEAR {
                    return Teleported {
                        scene: s,
                        id,
                        events,
                        to: at,
                        had_target,
                    };
                }
            }
        }
    }
    panic!("no seed made the shulker teleport");
}

#[test]
fn a_shulker_below_half_health_teleports_to_a_free_spot_closes_its_lid_and_signals_it() {
    let Teleported {
        scene,
        id,
        events,
        to,
        ..
    } = first_teleport(Difficulty::Peaceful);
    assert_eq!(level_events(&events, 2016), 1);
    assert_eq!(
        sounds(&events, "minecraft:entity.shulker.teleport"),
        vec![(1.0, 1.0)]
    );
    assert!((to.x - NEAR.x).abs() <= 8.5 && (to.y - NEAR.y).abs() <= 8.5);
    assert_eq!(to.x.fract(), 0.5);
    assert_eq!(to.z.fract(), 0.5);
    assert_eq!(to.y.fract(), 0.0);
    assert!(health(&scene, id) < 15.0);
    assert!((0..=5).contains(&face(&scene, id)));
}

#[test]
fn a_teleporting_shulker_forgets_its_target_at_once() {
    let Teleported {
        scene,
        id,
        had_target,
        ..
    } = first_teleport(Difficulty::Normal);
    assert!(had_target);
    assert!(scene.server.mob_target(id).is_none());
}

#[test]
fn a_healthy_shulker_does_not_teleport_when_hurt_by_an_arrow() {
    let mut s = ready(Vec::new());
    let id = summon(&mut s, Kind::Shulker, "{NoAI:1b,Peek:30b}", AWAY);
    for _ in 0..3 {
        volley(&mut s);
    }
    assert!(health(&s, id) > 15.0);
    assert_eq!(snapshot(&s, Kind::Shulker, id).position, AWAY);
}

#[test]
fn a_teleport_is_a_cancellable_entity_teleport_event() {
    let mut proven = false;
    for seed in 0..SEEDS {
        let mut s = ready_seeded(seed, vec![deciding(1, EventKind::EntityTeleport)], |_| {});
        let id = summon(&mut s, Kind::Shulker, "{Health:16f,Peek:30b}", AWAY);
        for _ in 0..12 {
            if !exists(&s, Kind::Shulker, id)
                || !asked_for(&s.plugin, EventKind::EntityTeleport).is_empty()
            {
                break;
            }
            volley(&mut s);
        }
        if asked_for(&s.plugin, EventKind::EntityTeleport).is_empty() {
            continue;
        }
        let asked = last_asked(&s.plugin, EventKind::EntityTeleport);
        let EventPayload::EntityTeleport(teleport) = &asked.payload else {
            panic!("an entity teleport");
        };
        assert_eq!(teleport.entity_type, "minecraft:shulker");
        assert_eq!(teleport.entity.id, id);
        assert_eq!(
            teleport.to.as_ref().map(|to| to.position.x.fract()),
            Some(0.0)
        );
        assert_eq!(snapshot(&s, Kind::Shulker, id).position, AWAY);
        rule_on(&mut s.server, 1, asked.id, true, None);
        assert_eq!(snapshot(&s, Kind::Shulker, id).position, AWAY);
        assert_ne!(peek(&s, id), 0);
        proven = true;
        break;
    }
    assert!(proven, "no seed made the shulker ask to teleport");
}

#[test]
fn a_bullet_hit_teleports_an_open_shulker_and_breeds_a_copy_of_it_where_it_stood() {
    let mut proven = false;
    for seed in 0..SEEDS {
        let mut s = ready_seeded(seed, Vec::new(), |_| {});
        let id = summon(&mut s, Kind::Shulker, "{Peek:30b,Color:14b}", AWAY);
        bullet_at(
            &mut s,
            "{Motion:[0.0d,0.0d,0.5d]}",
            Vec3::new(AWAY.x, AWAY.y + 0.5, AWAY.z - 2.0),
        );
        s.tick(5);
        let shulkers = s.server.entities_of(Kind::Shulker);
        let moved = shulkers.iter().find(|shulker| shulker.id == id).unwrap();
        assert_eq!(health(&s, id), 26.0, "seed {seed}");
        if moved.position == AWAY {
            assert_eq!(shulkers.len(), 1, "seed {seed}");
            continue;
        }
        assert_eq!(shulkers.len(), 2, "seed {seed}: {shulkers:?}");
        let child = shulkers.iter().find(|shulker| shulker.id != id).unwrap();
        assert_eq!(child.position, AWAY);
        assert_eq!(saved(&s, child.id).get_i64("Color"), Some(14));
        assert_eq!(peek(&s, id), 0);
        proven = true;
        break;
    }
    assert!(proven, "no seed made the bullet teleport the shulker");
}

#[test]
fn a_bullet_hit_on_a_closed_shulker_only_hurts_it() {
    let mut s = ready(Vec::new());
    let id = summon(&mut s, Kind::Shulker, "{NoAI:1b}", AWAY);
    bullet_at(
        &mut s,
        "{Motion:[0.0d,0.0d,0.5d]}",
        Vec3::new(AWAY.x, AWAY.y + 0.5, AWAY.z - 2.0),
    );
    s.tick(5);
    assert_eq!(s.server.entities_of(Kind::Shulker).len(), 1);
    assert_eq!(snapshot(&s, Kind::Shulker, id).position, AWAY);
    assert_eq!(health(&s, id), 30.0 - 4.0);
}

fn shoot_scene(handlers: Vec<HandlerSpec>, difficulty: Difficulty) -> (Scene, EntityId) {
    let mut s = ready_with(handlers, |config| config.difficulty = difficulty);
    let id = summon(&mut s, Kind::Shulker, "{}", Vec3::new(6.5, 64.0, 6.5));
    (s, id)
}

#[test]
fn a_shulker_in_range_opens_wide_and_shoots_homing_bullets_with_a_pitched_sound() {
    let (mut s, id) = shoot_scene(Vec::new(), Difficulty::Normal);
    s.players[0].link.take();
    let mut bullets = Vec::new();
    let mut shots = Vec::new();
    let mut widest = 0;
    for _ in 0..200 {
        s.tick(1);
        let events = s.players[0].link.take();
        bullets.extend(spawned(&events, Kind::ShulkerBullet));
        shots.extend(sounds(&events, "minecraft:entity.shulker.shoot"));
        widest = widest.max(peek(&s, id));
        if bullets.len() >= 2 {
            break;
        }
    }
    assert_eq!(widest, 100);
    assert!(bullets.len() >= 2, "{bullets:?}");
    for (_, position, owner) in &bullets {
        assert_eq!(*owner, id.0);
        assert!((position.x - 6.5).abs() < 0.5 && (position.z - 6.5).abs() < 0.5);
        assert!(position.y > 64.0 && position.y < 65.5, "{position:?}");
    }
    assert!(shots.len() >= 2);
    for (volume, pitch) in shots {
        assert_eq!(volume, 2.0);
        assert!((0.8..=1.2).contains(&pitch), "{pitch}");
    }
    assert!(s.server.mob_target(id).is_some());
}

#[test]
fn a_shulker_shoots_its_victim_who_ends_up_levitating() {
    let (mut s, _) = shoot_scene(Vec::new(), Difficulty::Normal);
    s.players[0].link.take();
    let mut events = Vec::new();
    for _ in 0..400 {
        s.tick(1);
        events.extend(s.players[0].link.take());
        if !levitation_applied(&events).is_empty() {
            break;
        }
    }
    assert_eq!(
        levitation_applied(&events).first(),
        Some(&(0, EffectDuration::Ticks(200)))
    );
    assert!(s.server.player_health(s.players[0].id).unwrap() < 20.0);
}

#[test]
fn a_peaceful_world_keeps_a_shulker_from_targeting_or_shooting() {
    let (mut s, id) = shoot_scene(Vec::new(), Difficulty::Peaceful);
    s.players[0].link.take();
    let mut bullets = 0;
    for _ in 0..200 {
        s.tick(1);
        bullets += spawned(&s.players[0].link.take(), Kind::ShulkerBullet).len();
    }
    assert_eq!(bullets, 0);
    assert!(s.server.mob_target(id).is_none());
}

#[test]
fn a_shot_is_a_cancellable_projectile_launch_of_a_shulker_bullet() {
    let (mut s, _) = shoot_scene(
        vec![deciding(1, EventKind::ProjectileLaunch)],
        Difficulty::Normal,
    );
    for _ in 0..200 {
        s.tick(1);
        if !asked_for(&s.plugin, EventKind::ProjectileLaunch).is_empty() {
            break;
        }
    }
    let asked = last_asked(&s.plugin, EventKind::ProjectileLaunch);
    let EventPayload::ProjectileLaunch(launch) = &asked.payload else {
        panic!("a projectile launch");
    };
    assert_eq!(launch.entity_type, "minecraft:shulker_bullet");
    assert!(s.server.entities_of(Kind::ShulkerBullet).is_empty());
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(s.server.entities_of(Kind::ShulkerBullet).is_empty());
}

#[test]
fn an_allowed_launch_puts_the_bullet_in_the_world() {
    let (mut s, _) = shoot_scene(
        vec![deciding(1, EventKind::ProjectileLaunch)],
        Difficulty::Normal,
    );
    for _ in 0..200 {
        s.tick(1);
        if !asked_for(&s.plugin, EventKind::ProjectileLaunch).is_empty() {
            break;
        }
    }
    let asked = last_asked(&s.plugin, EventKind::ProjectileLaunch);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert_eq!(s.server.entities_of(Kind::ShulkerBullet).len(), 1);
}
