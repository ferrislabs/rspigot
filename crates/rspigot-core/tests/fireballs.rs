mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_core::config::{GameConfig, LevelData, PluginSettings};
use rspigot_core::events::{ClientEvent, EntityKind as WireKind, Inbound, PlayerInput};
use rspigot_core::plugins::events::BlockIgniteCause;
use rspigot_core::plugins::{
    Answer, BlockOrder, EventKind, EventPayload, HandlerSpec, Order, Question,
};
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

fn owner_of(s: &Scene, id: EntityId) -> String {
    let saved = s.server.saved_entity(id).expect("a saved entity");
    match saved.get("UUID").cloned() {
        Some(rspigot_nbt::Tag::IntArray(parts)) => {
            format!("[I;{},{},{},{}]", parts[0], parts[1], parts[2], parts[3])
        }
        other => panic!("an int array uuid, got {other:?}"),
    }
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

fn player_owner(s: &Scene) -> String {
    let [a, b, c, d] = uuid_ints(s.players[0].id.0);
    format!("[I;{a},{b},{c},{d}]")
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

fn level_events(events: &[ClientEvent], wanted: i32) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, ClientEvent::LevelEvent { event, .. } if *event == wanted))
        .count()
}

fn explosions(events: &[ClientEvent]) -> Vec<(Vec3, f32, i32)> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Explode {
                center,
                radius,
                block_count,
                ..
            } => Some((*center, *radius, *block_count)),
            _ => None,
        })
        .collect()
}

fn fire_at(events: &[ClientEvent], pos: BlockPos) -> bool {
    events.iter().any(|event| {
        matches!(event, ClientEvent::BlockUpdate { pos: at, state } if *at == pos
            && state.kind().is_some_and(|kind| kind.name() == "minecraft:fire"))
    })
}

fn block_at(s: &mut Scene, pos: BlockPos) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state.kind,
        other => panic!("expected a block, got {other:?}"),
    }
}

const FAR: Vec3 = Vec3::new(20.5, 120.0, 20.5);

#[test]
fn a_small_fireball_accelerates_along_its_motion_with_the_vanilla_inertia() {
    let mut s = ready(Vec::new());
    let id = summon(
        &mut s,
        Kind::SmallFireball,
        "{Motion:[0.0d,0.0d,0.1d]}",
        FAR,
    );
    let mut velocity = Vec3::new(0.0, 0.0, 0.1);
    let mut position = FAR;
    let inertia = f64::from(0.95f32);
    for tick in 1..=15 {
        s.tick(1);
        velocity = (velocity + velocity.normalize().scale(0.1)).scale(inertia);
        position = position + velocity;
        let ball = snapshot(&s, Kind::SmallFireball, id);
        assert!(
            (ball.velocity.z - velocity.z).abs() < 1.0e-9 && ball.velocity.y == 0.0,
            "tick {tick}: {:?} is not {velocity:?}",
            ball.velocity
        );
        assert!(
            (ball.position.z - position.z).abs() < 1.0e-9 && ball.position.y == FAR.y,
            "tick {tick}: {:?} is not {position:?}",
            ball.position
        );
    }
}

#[test]
fn a_fireball_in_water_keeps_four_fifths_of_its_speed_and_a_dangerous_skull_less_in_air() {
    let mut s = ready(Vec::new());
    for y in 119..=122 {
        s.act(Order::Block {
            pos: BlockPos::new(20, y, 20),
            order: BlockOrder::Set("minecraft:water".to_owned()),
        });
    }
    let wet = summon(
        &mut s,
        Kind::Fireball,
        "{Motion:[0.0d,0.0d,0.0d]}",
        Vec3::new(20.5, 120.5, 20.5),
    );
    let skull = summon(
        &mut s,
        Kind::WitherSkull,
        "{dangerous:1b,Motion:[0.0d,0.0d,1.0d]}",
        Vec3::new(30.5, 120.0, 30.5),
    );
    s.tick(1);
    assert_eq!(snapshot(&s, Kind::Fireball, wet).velocity, Vec3::ZERO);
    let at = snapshot(&s, Kind::WitherSkull, skull).velocity.z;
    assert!(
        (at - (1.0 + 0.1) * f64::from(0.73f32)).abs() < 1.0e-9,
        "{at}"
    );
}

#[test]
fn a_small_fireball_hitting_the_ground_sets_the_air_above_it_on_fire_and_is_discarded() {
    let mut s = ready(Vec::new());
    let id = summon(
        &mut s,
        Kind::SmallFireball,
        "{Motion:[0.0d,-1.0d,0.0d]}",
        Vec3::new(3.5, 66.0, 3.5),
    );
    s.players[0].link.take();
    s.tick(3);
    assert!(!exists(&s, Kind::SmallFireball, id));
    let events = s.players[0].link.take();
    assert!(fire_at(&events, BlockPos::new(3, 64, 3)), "{events:?}");
    assert_eq!(block_at(&mut s, BlockPos::new(3, 64, 3)), "minecraft:fire");
}

#[test]
fn a_small_fireball_on_soul_sand_lights_soul_fire() {
    let mut s = ready(Vec::new());
    s.act(Order::Block {
        pos: BlockPos::new(3, 63, 3),
        order: BlockOrder::Set("minecraft:soul_sand".to_owned()),
    });
    summon(
        &mut s,
        Kind::SmallFireball,
        "{Motion:[0.0d,-1.0d,0.0d]}",
        Vec3::new(3.5, 66.0, 3.5),
    );
    s.tick(3);
    assert_eq!(
        block_at(&mut s, BlockPos::new(3, 64, 3)),
        "minecraft:soul_fire"
    );
}

#[test]
fn a_blaze_fireball_leaves_the_ground_alone_when_mob_griefing_is_off() {
    let mut s = ready_with(Vec::new(), |config| config.rules.mob_griefing = false);
    summon(&mut s, Kind::Blaze, "{}", Vec3::new(6.5, 64.0, 6.5));
    s.players[0].link.take();
    let mut shots = 0;
    let mut burning = false;
    for _ in 0..300 {
        s.tick(1);
        let events = s.players[0].link.take();
        shots += spawned(&events, Kind::SmallFireball).len();
        burning |= events.iter().any(|event| {
            matches!(event, ClientEvent::BlockUpdate { state, .. }
                if state.kind().is_some_and(|kind| kind.name() == "minecraft:fire"))
        });
        if shots >= 6 {
            break;
        }
    }
    assert!(shots >= 3);
    assert!(!burning);
}

#[test]
fn a_small_fireball_lights_the_ground_when_mob_griefing_is_on() {
    let mut s = ready(Vec::new());
    summon(
        &mut s,
        Kind::SmallFireball,
        "{Motion:[0.0d,-1.0d,0.0d]}",
        Vec3::new(3.5, 66.0, 3.5),
    );
    s.tick(3);
    assert!(fire_at(&s.players[0].link.take(), BlockPos::new(3, 64, 3)));
}

#[test]
fn a_small_fireball_hits_a_zombie_for_five_and_sets_it_burning_for_five_seconds() {
    let mut s = ready(Vec::new());
    let zombie = summon(
        &mut s,
        Kind::Zombie,
        "{NoAI:1b,Fire:-20s}",
        Vec3::new(30.5, 64.0, 30.5),
    );
    let ball = summon(
        &mut s,
        Kind::SmallFireball,
        "{Motion:[0.0d,0.0d,0.5d]}",
        Vec3::new(30.5, 64.5, 26.5),
    );
    s.players[0].link.take();
    s.tick(6);
    assert!(!exists(&s, Kind::SmallFireball, ball));
    let events = s.players[0].link.take();
    assert_eq!(
        damage_of(&events, zombie, DamageType::UnattributedFireball),
        1
    );
    assert!(health(&s, Kind::Zombie, zombie) < 16.0);
    let burning = s.server.entity_fire_ticks(zombie).expect("a fire counter");
    assert!(burning > 80 && burning <= 100, "{burning}");
}

#[test]
fn a_small_fireball_hurts_a_player_by_five_and_burns_them() {
    let mut s = ready(Vec::new());
    let alice = s.server.player_entity_id(s.players[0].id).unwrap();
    let ball = summon(
        &mut s,
        Kind::SmallFireball,
        "{Motion:[0.0d,0.0d,0.5d]}",
        Vec3::new(0.5, 64.8, -3.5),
    );
    s.players[0].link.take();
    s.tick(6);
    assert!(!exists(&s, Kind::SmallFireball, ball));
    let events = s.players[0].link.take();
    assert_eq!(
        damage_of(&events, alice, DamageType::UnattributedFireball),
        1
    );
    assert_eq!(s.server.player_health(s.players[0].id), Some(15.0));
    assert!(s.server.player_fire_ticks(s.players[0].id).unwrap() > 80);
}

#[test]
fn a_small_fireball_does_not_hit_its_owner_until_it_has_left_it() {
    let mut s = ready(Vec::new());
    let blaze = summon(
        &mut s,
        Kind::Blaze,
        "{NoAI:1b}",
        Vec3::new(30.5, 64.0, 30.5),
    );
    let owner = owner_of(&s, blaze);
    let ball = summon(
        &mut s,
        Kind::SmallFireball,
        &format!("{{Motion:[0.0d,0.0d,0.2d],Owner:{owner}}}"),
        Vec3::new(30.5, 64.9, 30.5),
    );
    s.players[0].link.take();
    s.tick(2);
    assert!(exists(&s, Kind::SmallFireball, ball));
    assert_eq!(
        damage_of(&s.players[0].link.take(), blaze, DamageType::Fireball),
        0
    );
}

#[test]
fn a_fireball_between_an_owner_and_a_target_blames_the_owner() {
    let mut s = ready(Vec::new());
    let blaze = summon(
        &mut s,
        Kind::Blaze,
        "{NoAI:1b}",
        Vec3::new(30.5, 64.0, 20.5),
    );
    let owner = owner_of(&s, blaze);
    let zombie = summon(
        &mut s,
        Kind::Zombie,
        "{NoAI:1b}",
        Vec3::new(30.5, 64.0, 30.5),
    );
    summon(
        &mut s,
        Kind::SmallFireball,
        &format!("{{Motion:[0.0d,0.0d,0.8d],Owner:{owner},LeftOwner:1b}}"),
        Vec3::new(30.5, 64.5, 26.5),
    );
    s.players[0].link.take();
    s.tick(6);
    let events = s.players[0].link.take();
    assert!(events.iter().any(|event| matches!(event,
        ClientEvent::DamageEvent { entity, kind: DamageType::Fireball, cause: Some(cause), .. }
            if *entity == zombie && *cause == blaze)));
}

#[test]
fn a_large_fireball_explodes_on_the_ground_with_its_power_and_is_discarded() {
    let mut s = ready(Vec::new());
    let id = summon(
        &mut s,
        Kind::Fireball,
        "{Motion:[0.0d,-1.0d,0.0d],ExplosionPower:3b}",
        Vec3::new(5.5, 66.0, 5.5),
    );
    s.players[0].link.take();
    s.tick(3);
    assert!(!exists(&s, Kind::Fireball, id));
    let events = s.players[0].link.take();
    let blasts = explosions(&events);
    assert_eq!(blasts.len(), 1, "{blasts:?}");
    assert_eq!(blasts[0].1, 3.0);
    assert!(blasts[0].2 > 0);
    assert_eq!(block_at(&mut s, BlockPos::new(5, 63, 5)), "minecraft:air");
}

#[test]
fn a_large_fireball_blasts_without_digging_or_fire_when_mob_griefing_is_off() {
    let mut s = ready_with(Vec::new(), |config| config.rules.mob_griefing = false);
    summon(
        &mut s,
        Kind::Fireball,
        "{Motion:[0.0d,-1.0d,0.0d],ExplosionPower:3b}",
        Vec3::new(5.5, 66.0, 5.5),
    );
    s.players[0].link.take();
    s.tick(3);
    assert_eq!(explosions(&s.players[0].link.take()).len(), 1);
    assert_ne!(block_at(&mut s, BlockPos::new(5, 63, 5)), "minecraft:air");
    assert_eq!(block_at(&mut s, BlockPos::new(5, 64, 5)), "minecraft:air");
}

#[test]
fn a_large_fireball_hits_for_six_and_blasts_the_target_as_well() {
    let mut s = ready(Vec::new());
    let zombie = summon(
        &mut s,
        Kind::Zombie,
        "{NoAI:1b}",
        Vec3::new(30.5, 64.0, 30.5),
    );
    summon(
        &mut s,
        Kind::Fireball,
        "{Motion:[0.0d,0.0d,0.8d]}",
        Vec3::new(30.5, 64.5, 26.5),
    );
    s.players[0].link.take();
    s.tick(6);
    let events = s.players[0].link.take();
    assert_eq!(
        damage_of(&events, zombie, DamageType::UnattributedFireball),
        1
    );
    assert_eq!(explosions(&events).len(), 1);
    assert!(!exists(&s, Kind::Zombie, zombie) || health(&s, Kind::Zombie, zombie) < 14.0);
}

#[test]
fn a_fireball_blast_of_an_owned_fireball_is_a_player_explosion_by_the_owner() {
    let mut s = ready(Vec::new());
    let ghast = summon(
        &mut s,
        Kind::Ghast,
        "{NoAI:1b}",
        Vec3::new(30.5, 90.0, 20.5),
    );
    let owner = owner_of(&s, ghast);
    let zombie = summon(
        &mut s,
        Kind::Zombie,
        "{NoAI:1b}",
        Vec3::new(28.5, 64.0, 30.5),
    );
    summon(
        &mut s,
        Kind::Fireball,
        &format!("{{Motion:[0.0d,-1.0d,0.0d],Owner:{owner},LeftOwner:1b,ExplosionPower:2b}}"),
        Vec3::new(26.5, 66.0, 30.5),
    );
    s.players[0].link.take();
    s.tick(4);
    let events = s.players[0].link.take();
    assert!(events.iter().any(|event| matches!(event,
        ClientEvent::DamageEvent { entity, kind: DamageType::PlayerExplosion, cause: Some(cause), .. }
            if *entity == zombie && *cause == ghast)), "{events:?}");
}

#[test]
fn a_wither_skull_explodes_with_a_radius_of_one_and_never_sets_fire() {
    let mut s = ready(Vec::new());
    let id = summon(
        &mut s,
        Kind::WitherSkull,
        "{Motion:[0.0d,-1.0d,0.0d]}",
        Vec3::new(5.5, 66.0, 5.5),
    );
    s.players[0].link.take();
    s.tick(3);
    assert!(!exists(&s, Kind::WitherSkull, id));
    let events = s.players[0].link.take();
    let blasts = explosions(&events);
    assert_eq!(blasts.len(), 1);
    assert_eq!(blasts[0].1, 1.0);
    assert_eq!(block_at(&mut s, BlockPos::new(5, 64, 5)), "minecraft:air");
}

#[test]
fn a_dangerous_skull_digs_obsidian_that_a_plain_one_cannot() {
    let mut dug = false;
    for _ in 0..6 {
        let mut s = ready(Vec::new());
        s.act(Order::Block {
            pos: BlockPos::new(5, 63, 5),
            order: BlockOrder::Set("minecraft:obsidian".to_owned()),
        });
        summon(
            &mut s,
            Kind::WitherSkull,
            "{Motion:[0.0d,-1.0d,0.0d]}",
            Vec3::new(5.5, 66.0, 5.5),
        );
        s.tick(3);
        assert_eq!(
            block_at(&mut s, BlockPos::new(5, 63, 5)),
            "minecraft:obsidian"
        );
        let mut s = ready(Vec::new());
        s.act(Order::Block {
            pos: BlockPos::new(5, 63, 5),
            order: BlockOrder::Set("minecraft:obsidian".to_owned()),
        });
        summon(
            &mut s,
            Kind::WitherSkull,
            "{dangerous:1b,Motion:[0.0d,-1.0d,0.0d]}",
            Vec3::new(5.5, 66.0, 5.5),
        );
        s.tick(3);
        dug |= block_at(&mut s, BlockPos::new(5, 63, 5)) == "minecraft:air";
        if dug {
            break;
        }
    }
    assert!(dug);
}

#[test]
fn a_wither_skull_hits_for_eight_and_withers_the_target_on_hard() {
    let mut s = ready_with(Vec::new(), |config| {
        config.difficulty = rspigot_domain::world::Difficulty::Hard;
    });
    let alice = s.server.player_entity_id(s.players[0].id).unwrap();
    let wither = summon(
        &mut s,
        Kind::Wither,
        "{NoAI:1b,Invul:0}",
        Vec3::new(30.5, 64.0, 30.5),
    );
    let owner = owner_of(&s, wither);
    summon(
        &mut s,
        Kind::WitherSkull,
        &format!("{{Motion:[0.0d,0.0d,0.5d],Owner:{owner}}}"),
        Vec3::new(0.5, 64.8, -3.5),
    );
    s.players[0].link.take();
    s.tick(6);
    let events = s.players[0].link.take();
    assert_eq!(damage_of(&events, alice, DamageType::WitherSkull), 1);
    assert!(s.server.player_health(s.players[0].id).unwrap() < 20.0);
    let Answer::Player(state) = s.ask(Question::Player(s.players[0].id)) else {
        panic!("expected Alice");
    };
    assert!(
        state
            .effects
            .iter()
            .any(|effect| effect.key == "minecraft:wither" && effect.amplifier == 1),
        "{:?}",
        state.effects
    );
}

#[test]
fn a_launch_is_a_cancellable_projectile_launch_of_a_small_fireball() {
    let mut s = ready(vec![deciding(1, EventKind::ProjectileLaunch)]);
    summon(&mut s, Kind::Blaze, "{}", Vec3::new(6.5, 64.0, 6.5));
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
    assert_eq!(launch.entity_type, "minecraft:small_fireball");
    assert!(s.server.entities_of(Kind::SmallFireball).is_empty());
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(s.server.entities_of(Kind::SmallFireball).is_empty());
}

#[test]
fn an_allowed_launch_puts_the_small_fireball_in_the_world() {
    let mut s = ready(vec![deciding(1, EventKind::ProjectileLaunch)]);
    summon(&mut s, Kind::Blaze, "{}", Vec3::new(6.5, 64.0, 6.5));
    for _ in 0..200 {
        s.tick(1);
        if !asked_for(&s.plugin, EventKind::ProjectileLaunch).is_empty() {
            break;
        }
    }
    let asked = last_asked(&s.plugin, EventKind::ProjectileLaunch);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert_eq!(s.server.entities_of(Kind::SmallFireball).len(), 1);
}

fn hit_scene(kind: Kind) -> (Scene, EntityId, EntityId) {
    let mut s = ready(vec![deciding(1, EventKind::ProjectileHit)]);
    let zombie = summon(
        &mut s,
        Kind::Zombie,
        "{NoAI:1b}",
        Vec3::new(30.5, 64.0, 30.5),
    );
    let ball = summon(
        &mut s,
        kind,
        "{Motion:[0.0d,0.0d,0.8d]}",
        Vec3::new(30.5, 64.5, 26.5),
    );
    for _ in 0..10 {
        s.tick(1);
        if !asked_for(&s.plugin, EventKind::ProjectileHit).is_empty() {
            break;
        }
    }
    (s, zombie, ball)
}

#[test]
fn an_entity_hit_asks_the_plugins_and_waits_for_the_answer() {
    let (mut s, zombie, ball) = hit_scene(Kind::SmallFireball);
    let asked = last_asked(&s.plugin, EventKind::ProjectileHit);
    let EventPayload::ProjectileHit(hit) = &asked.payload else {
        panic!("a projectile hit");
    };
    assert_eq!(hit.entity.id, ball);
    assert_eq!(hit.entity_type, "minecraft:small_fireball");
    assert_eq!(hit.hit_entity.as_ref().map(|view| view.id), Some(zombie));
    s.tick(3);
    assert!(exists(&s, Kind::SmallFireball, ball));
    assert_eq!(health(&s, Kind::Zombie, zombie), 20.0);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert!(!exists(&s, Kind::SmallFireball, ball));
    assert!(health(&s, Kind::Zombie, zombie) < 20.0);
}

#[test]
fn a_cancelled_entity_hit_lets_the_fireball_fly_on() {
    for kind in [Kind::SmallFireball, Kind::Fireball, Kind::WitherSkull] {
        let (mut s, zombie, ball) = hit_scene(kind);
        let asked = last_asked(&s.plugin, EventKind::ProjectileHit);
        s.players[0].link.take();
        rule_on(&mut s.server, 1, asked.id, true, None);
        assert!(exists(&s, kind, ball), "{kind:?}");
        assert_eq!(health(&s, Kind::Zombie, zombie), 20.0, "{kind:?}");
        assert!(explosions(&s.players[0].link.take()).is_empty(), "{kind:?}");
    }
}

#[test]
fn a_block_hit_is_announced_with_the_block_and_the_face() {
    let mut s = ready(vec![observe(2, EventKind::ProjectileHit)]);
    let ball = summon(
        &mut s,
        Kind::SmallFireball,
        "{Motion:[0.0d,-1.0d,0.0d]}",
        Vec3::new(3.5, 66.0, 3.5),
    );
    s.tick(3);
    assert!(!exists(&s, Kind::SmallFireball, ball));
    let settled = settled_for(&s.plugin, EventKind::ProjectileHit);
    let EventPayload::ProjectileHit(hit) = &settled.last().expect("a hit").payload else {
        panic!("a projectile hit");
    };
    assert_eq!(hit.hit_block, Some(BlockPos::new(3, 63, 3)));
    assert!(hit.hit_entity.is_none());
}

#[test]
fn igniting_a_block_asks_the_plugins_and_a_cancel_keeps_the_air() {
    let mut s = ready(vec![deciding(1, EventKind::BlockIgnite)]);
    let ball = summon(
        &mut s,
        Kind::SmallFireball,
        "{Motion:[0.0d,-1.0d,0.0d]}",
        Vec3::new(3.5, 66.0, 3.5),
    );
    s.tick(3);
    let asked = last_asked(&s.plugin, EventKind::BlockIgnite);
    let EventPayload::BlockIgnite(ignite) = &asked.payload else {
        panic!("a block ignite");
    };
    assert_eq!(ignite.block, BlockPos::new(3, 64, 3));
    assert_eq!(ignite.cause, BlockIgniteCause::Fireball);
    assert_eq!(
        ignite.igniting_entity.as_ref().map(|entity| entity.id),
        Some(ball)
    );
    assert!(exists(&s, Kind::SmallFireball, ball));
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(!exists(&s, Kind::SmallFireball, ball));
    assert_eq!(block_at(&mut s, BlockPos::new(3, 64, 3)), "minecraft:air");
}

#[test]
fn a_cancelled_explosion_prime_discards_the_fireball_without_a_blast() {
    let mut s = ready(vec![deciding(1, EventKind::ExplosionPrime)]);
    let ball = summon(
        &mut s,
        Kind::Fireball,
        "{Motion:[0.0d,-1.0d,0.0d],ExplosionPower:3b}",
        Vec3::new(5.5, 66.0, 5.5),
    );
    s.players[0].link.take();
    s.tick(3);
    let asked = last_asked(&s.plugin, EventKind::ExplosionPrime);
    let EventPayload::ExplosionPrime(prime) = &asked.payload else {
        panic!("an explosion prime");
    };
    assert_eq!(prime.radius, 3.0);
    assert!(prime.fire);
    assert_eq!(prime.entity_type, "minecraft:fireball");
    assert!(exists(&s, Kind::Fireball, ball));
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(!exists(&s, Kind::Fireball, ball));
    assert!(explosions(&s.players[0].link.take()).is_empty());
    assert_ne!(block_at(&mut s, BlockPos::new(5, 63, 5)), "minecraft:air");
}

fn reflect_scene() -> (Scene, EntityId) {
    let mut s = ready(Vec::new());
    let ball = summon(
        &mut s,
        Kind::Fireball,
        "{Motion:[0.0d,0.0d,0.0d]}",
        Vec3::new(0.5, 64.6, 2.2),
    );
    (s, ball)
}

#[test]
fn hitting_a_large_fireball_sends_it_back_along_the_look_and_makes_the_player_its_owner() {
    let (mut s, ball) = reflect_scene();
    let alice = s.players[0].clone();
    s.players[0].link.take();
    send_input(&mut s.server, &alice, PlayerInput::Attack(ball));
    let deflected = snapshot(&s, Kind::Fireball, ball);
    assert!(deflected.velocity.z > 0.9, "{:?}", deflected.velocity);
    let saved = s.server.saved_entity(ball).expect("saved");
    assert_eq!(saved.get_f64("acceleration_power"), Some(0.1));
    assert_eq!(
        saved.get("Owner").cloned(),
        Some(rspigot_nbt::Tag::IntArray(uuid_ints(alice.id.0).to_vec()))
    );
}

#[test]
fn a_ghast_dies_to_a_fireball_a_player_reflected_but_shrugs_off_a_plain_one() {
    let mut s = ready(Vec::new());
    let alice_owner = player_owner(&s);
    let ghast = summon(
        &mut s,
        Kind::Ghast,
        "{NoAI:1b}",
        Vec3::new(30.5, 70.0, 30.5),
    );
    summon(
        &mut s,
        Kind::Fireball,
        "{Motion:[0.0d,0.0d,0.8d],ExplosionPower:0b}",
        Vec3::new(30.5, 71.5, 25.5),
    );
    s.players[0].link.take();
    s.tick(8);
    let events = s.players[0].link.take();
    assert_eq!(
        damage_of(&events, ghast, DamageType::UnattributedFireball),
        0
    );
    assert_eq!(health(&s, Kind::Ghast, ghast), 10.0);
    summon(
        &mut s,
        Kind::Fireball,
        &format!("{{Motion:[0.0d,0.0d,0.8d],Owner:{alice_owner},LeftOwner:1b,ExplosionPower:0b}}"),
        Vec3::new(30.5, 71.5, 25.5),
    );
    s.tick(30);
    assert!(!exists(&s, Kind::Ghast, ghast));
}

#[test]
fn a_blaze_fires_three_small_fireballs_at_a_player_it_sees() {
    let mut s = ready(Vec::new());
    let blaze = summon(&mut s, Kind::Blaze, "{}", Vec3::new(6.5, 64.0, 6.5));
    s.players[0].link.take();
    let mut shots = Vec::new();
    let mut events = Vec::new();
    for _ in 0..400 {
        s.tick(1);
        let batch = s.players[0].link.take();
        shots.extend(spawned(&batch, Kind::SmallFireball));
        events.extend(batch);
        if shots.len() >= 3 {
            break;
        }
    }
    assert!(shots.len() >= 3, "{} fireballs", shots.len());
    assert!(level_events(&events, 1018) >= 3);
    let owner = snapshot(&s, Kind::Blaze, blaze);
    for (_, position, data) in shots.iter().take(3) {
        assert_eq!(*data, blaze.0);
        assert!(position.y > owner.position.y + 1.0, "{position:?}");
    }
}

#[test]
fn a_blaze_next_to_its_target_bites_instead_of_shooting() {
    let mut s = ready(Vec::new());
    let alice = s.server.player_entity_id(s.players[0].id).unwrap();
    let blaze = summon(&mut s, Kind::Blaze, "{}", Vec3::new(0.5, 64.0, 1.5));
    s.players[0].link.take();
    let mut bitten = false;
    for _ in 0..60 {
        s.tick(1);
        let events = s.players[0].link.take();
        bitten |= events.iter().any(|event| {
            matches!(event,
            ClientEvent::DamageEvent { entity, kind: DamageType::MobAttack, cause: Some(cause), .. }
                if *entity == alice && *cause == blaze)
        });
        if bitten {
            break;
        }
    }
    assert!(bitten);
    assert!(s.server.entities_of(Kind::SmallFireball).is_empty());
}

#[test]
fn a_blaze_standing_in_water_takes_drowning_damage() {
    let mut s = ready(Vec::new());
    s.act(Order::Block {
        pos: BlockPos::new(30, 64, 30),
        order: BlockOrder::Set("minecraft:water".to_owned()),
    });
    let id = summon(
        &mut s,
        Kind::Blaze,
        "{NoAI:1b}",
        Vec3::new(30.5, 64.0, 30.5),
    );
    s.tick(2);
    assert_eq!(health(&s, Kind::Blaze, id), 19.0);
}

#[test]
fn a_blaze_falling_loses_two_fifths_of_its_vertical_speed_each_tick() {
    let mut s = ready(Vec::new());
    let id = summon(&mut s, Kind::Blaze, "{}", Vec3::new(10.5, 90.0, 10.5));
    let drag = f64::from(0.98f32);
    s.tick(1);
    let first = snapshot(&s, Kind::Blaze, id).velocity.y;
    assert!((first - (-0.08 * drag)).abs() < 1.0e-9, "{first}");
    s.tick(1);
    let second = snapshot(&s, Kind::Blaze, id).velocity.y;
    assert!(
        (second - ((first * 0.6 - 0.08) * drag)).abs() < 1.0e-9,
        "{second}"
    );
}

#[test]
fn a_ghast_wanders_in_the_air_and_never_takes_fall_damage() {
    let mut s = ready(Vec::new());
    let id = summon(&mut s, Kind::Ghast, "{}", Vec3::new(30.5, 80.0, 30.5));
    let start = snapshot(&s, Kind::Ghast, id).position;
    s.tick(200);
    let end = snapshot(&s, Kind::Ghast, id);
    assert!(
        end.position.distance_squared(start) > 1.0,
        "{:?}",
        end.position
    );
    assert_eq!(health(&s, Kind::Ghast, id), 10.0);
}

#[test]
fn a_ghast_warns_then_fires_a_large_fireball_at_a_player_in_sight() {
    let mut s = ready(Vec::new());
    let ghast = summon(&mut s, Kind::Ghast, "{}", Vec3::new(0.5, 66.0, 20.5));
    s.players[0].link.take();
    let mut shots = Vec::new();
    let mut events = Vec::new();
    for _ in 0..200 {
        s.tick(1);
        let batch = s.players[0].link.take();
        shots.extend(spawned(&batch, Kind::Fireball));
        events.extend(batch);
        if !shots.is_empty() {
            break;
        }
    }
    assert_eq!(shots.len(), 1);
    assert!(level_events(&events, 1015) >= 1);
    assert_eq!(level_events(&events, 1016), 1);
    let (id, position, owner) = shots[0];
    assert_eq!(owner, ghast.0);
    let shooter = snapshot(&s, Kind::Ghast, ghast).position;
    assert!(
        (position.y - (shooter.y + 2.5)).abs() < 1.5,
        "{position:?} {shooter:?}"
    );
    let ball = snapshot(&s, Kind::Fireball, id);
    assert!(ball.velocity.z < 0.0, "{:?}", ball.velocity);
}

#[test]
fn a_ghast_ignores_players_more_than_four_blocks_above_or_below_it() {
    let mut s = ready(Vec::new());
    summon(&mut s, Kind::Ghast, "{}", Vec3::new(0.5, 90.0, 20.5));
    for _ in 0..200 {
        s.tick(1);
    }
    assert!(s.server.entities_of(Kind::Fireball).is_empty());
}

#[test]
fn a_summoned_ghast_keeps_its_explosion_power_and_saves_it() {
    let mut s = ready(Vec::new());
    let id = summon(
        &mut s,
        Kind::Ghast,
        "{ExplosionPower:4b,NoAI:1b}",
        Vec3::new(30.5, 80.0, 30.5),
    );
    let saved = s.server.saved_entity(id).expect("saved");
    assert_eq!(saved.get_i64("ExplosionPower"), Some(4));
}
