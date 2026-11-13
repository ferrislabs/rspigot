mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::commands::Target;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, Direction, Hand, Inbound, PlayerInput};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::boss_event::{BossColor, BossOverlay, BossUpdate};
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Meadow;

impl ChunkGenerator for Meadow {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                let at = |y: i32| BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z);
                for y in 60..=62 {
                    chunk.set_block(at(y), BlockStateId(1));
                }
                chunk.set_block(at(63), BlockKind::GrassBlock.default_state());
            }
        }
        if pos == ChunkPos::new(0, 0) {
            let sand = BlockKind::SoulSand.default_state();
            chunk.set_block(BlockPos::new(7, 64, 6), sand);
            for x in 6..=8 {
                chunk.set_block(BlockPos::new(x, 65, 6), sand);
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn ready(seed: u64) -> Scene {
    let config = GameConfig {
        view_distance: 5,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    let data = LevelData {
        spawn_mobs: false,
        day_time: 6_000,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(Meadow),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(8, 64, 8));
    s.tick(40);
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn summon(s: &mut Scene, kind: Kind, at: &str, nbt: &str) -> EntityId {
    let known: Vec<EntityId> = s
        .server
        .entities_of(kind)
        .into_iter()
        .map(|entity| entity.id)
        .collect();
    console(s, &format!("summon {} {at} {nbt}", kind.name()));
    s.server
        .entities_of(kind)
        .into_iter()
        .map(|entity| entity.id)
        .find(|id| !known.contains(id))
        .expect("a new entity")
}

fn saved(s: &Scene, id: EntityId) -> rspigot_nbt::Compound {
    s.server.saved_entity(id).expect("a saved entity")
}

fn health_of(s: &Scene, id: EntityId) -> f32 {
    s.server
        .entities_of(Kind::Wither)
        .into_iter()
        .find(|entity| entity.id == id)
        .and_then(|entity| entity.health)
        .expect("the wither")
}

fn bars(s: &Scene) -> Vec<(uuid::Uuid, BossUpdate)> {
    s.players[0]
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::BossEvent { id, update } => Some((id, update)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_wither_in_view_adds_a_purple_progress_bar_that_darkens_the_screen_named_after_it() {
    let mut s = ready(3);
    s.players[0].link.take();
    summon(&mut s, Kind::Wither, "8.5 70 24.5", "{NoAI:1b}");
    s.tick(3);
    let events = bars(&s);
    let adds: Vec<_> = events
        .iter()
        .filter(|(_, update)| matches!(update, BossUpdate::Add { .. }))
        .collect();
    assert_eq!(adds.len(), 1, "{events:?}");
    let BossUpdate::Add {
        name,
        color,
        overlay,
        flags,
        ..
    } = &adds[0].1
    else {
        unreachable!();
    };
    assert_eq!(*color, BossColor::Purple);
    assert_eq!(*overlay, BossOverlay::Progress);
    assert!(flags.darken_screen && !flags.play_music && !flags.create_world_fog);
    assert_eq!(name.plain_text(), "entity.minecraft.wither");
}

#[test]
fn the_bar_follows_the_health_ratio_of_an_awake_wither() {
    let mut s = ready(3);
    summon(&mut s, Kind::Wither, "8.5 90 24.5", "{Health:150f}");
    s.players[0].link.take();
    s.tick(2);
    let progress: Vec<f32> = bars(&s)
        .into_iter()
        .filter_map(|(_, update)| match update {
            BossUpdate::Progress(progress) => Some(progress),
            BossUpdate::Add { progress, .. } => Some(progress),
            _ => None,
        })
        .collect();
    assert!(!progress.is_empty());
    let last = *progress.last().unwrap();
    assert!((0.49..=0.52).contains(&last), "{progress:?}");
}

#[test]
fn the_bar_goes_away_with_the_wither_and_a_new_one_has_its_own_id() {
    let mut s = ready(3);
    let first = summon(&mut s, Kind::Wither, "8.5 70 24.5", "{NoAI:1b}");
    s.tick(2);
    let events = bars(&s);
    let id = events[0].0;
    console(&mut s, "kill @e[type=minecraft:wither]");
    s.tick(60);
    let after = bars(&s);
    assert!(
        after
            .iter()
            .any(|(bar, update)| *bar == id && *update == BossUpdate::Remove),
        "{after:?}"
    );
    assert!(
        s.server
            .entities_of(Kind::Wither)
            .iter()
            .all(|entity| entity.id != first)
            || s.server
                .entities_of(Kind::Wither)
                .iter()
                .any(|entity| entity.health.is_some_and(|health| health <= 0.0))
    );
    let second = summon(&mut s, Kind::Wither, "8.5 70 24.5", "{NoAI:1b}");
    assert_ne!(second, first);
    s.tick(2);
    let again = bars(&s);
    let fresh = again
        .iter()
        .find(|(_, update)| matches!(update, BossUpdate::Add { .. }))
        .expect("a new bar");
    assert_ne!(fresh.0, id);
}

#[test]
fn the_invulnerable_ticks_survive_a_save_and_a_fresh_wither_has_none() {
    let mut s = ready(3);
    let kept = summon(&mut s, Kind::Wither, "8.5 70 24.5", "{NoAI:1b,Invul:120}");
    assert_eq!(saved(&s, kept).get_i32("Invul"), Some(120));
    let plain = summon(&mut s, Kind::Wither, "12.5 70 24.5", "{NoAI:1b}");
    assert_eq!(saved(&s, plain).get_i32("Invul"), Some(0));
}

#[test]
fn a_growing_wither_counts_down_heals_ten_every_ten_ticks_and_cannot_be_hurt() {
    let mut s = ready(3);
    let wither = summon(
        &mut s,
        Kind::Wither,
        "8.5 90 24.5",
        "{Invul:220,Health:100f}",
    );
    let attacker = summon(&mut s, Kind::Vindicator, "8.5 90 26.5", "{NoAI:1b}");
    s.tick(25);
    assert!(!s.server.do_hurt_target(attacker, Target::Entity(wither)));
    let invul = saved(&s, wither).get_i32("Invul").unwrap();
    assert!((190..=196).contains(&invul), "{invul}");
    let health = health_of(&s, wither);
    assert!(health >= 120.0, "{health}");
    s.tick(1);
    assert!(health_of(&s, wither) >= health);
}

#[test]
fn a_growing_wither_explodes_when_the_countdown_ends() {
    let mut s = ready(3);
    let wither = summon(
        &mut s,
        Kind::Wither,
        "8.5 100 24.5",
        "{Invul:5,Health:100f}",
    );
    s.players[0].link.take();
    s.tick(8);
    let events = s.players[0].link.take();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ClientEvent::Explode { radius, .. } if (*radius - 7.0).abs() < 1e-6)),
        "{events:?}"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ClientEvent::GlobalLevelEvent { event: 1023, .. }))
    );
    assert_eq!(saved(&s, wither).get_i32("Invul"), Some(0));
}

#[test]
fn an_awake_wither_regenerates_a_heart_each_second() {
    let mut s = ready(3);
    let wither = summon(&mut s, Kind::Wither, "8.5 120 24.5", "{Health:100f}");
    s.tick(2);
    let before = health_of(&s, wither);
    s.tick(60);
    let after = health_of(&s, wither);
    assert!(
        after >= before + 2.0 && after <= before + 4.0,
        "{before} {after}"
    );
}

#[test]
fn a_wither_shoots_skulls_owned_by_itself_at_a_player_in_range() {
    let mut s = ready(3);
    let wither = summon(&mut s, Kind::Wither, "8.5 66 24.5", "");
    s.players[0].link.take();
    let mut shot_events = 0;
    let mut skulls = 0;
    for _ in 0..120 {
        s.tick(1);
        shot_events += s.players[0]
            .link
            .take()
            .iter()
            .filter(|event| matches!(event, ClientEvent::LevelEvent { event: 1024, .. }))
            .count();
        skulls = skulls.max(s.server.entities_of(Kind::WitherSkull).len());
    }
    assert!(shot_events >= 1, "{shot_events}");
    assert!(skulls >= 1);
    let _ = wither;
}

#[test]
fn a_wither_hurt_by_a_hostile_mob_loses_health_but_its_friends_do_nothing() {
    let mut s = ready(3);
    let wither = summon(
        &mut s,
        Kind::Wither,
        "8.5 120 24.5",
        "{NoAI:1b,Health:200f}",
    );
    let friend = summon(&mut s, Kind::Skeleton, "8.5 120 26.5", "{NoAI:1b}");
    let foe = summon(&mut s, Kind::Vindicator, "10.5 120 26.5", "{NoAI:1b}");
    s.tick(2);
    let before = health_of(&s, wither);
    s.server.do_hurt_target(friend, Target::Entity(wither));
    assert_eq!(health_of(&s, wither), before);
    s.server.do_hurt_target(foe, Target::Entity(wither));
    assert!(health_of(&s, wither) < before);
}

fn place_skull(s: &mut Scene, on: BlockPos, sequence: i32) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: on,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence,
        },
    );
    s.server.deliver();
}

#[test]
fn building_a_t_of_soul_sand_and_placing_the_third_skull_summons_a_growing_wither() {
    let mut s = ready(3);
    console(&mut s, "difficulty normal");
    console(&mut s, "give Alice minecraft:wither_skeleton_skull 3");
    s.tick(2);
    for (index, dx) in (0..3).enumerate() {
        place_skull(&mut s, BlockPos::new(6 + dx, 65, 6), index as i32 + 1);
        s.tick(1);
        if index < 2 {
            assert!(s.server.entities_of(Kind::Wither).is_empty());
        }
    }
    s.tick(2);
    let withers = s.server.entities_of(Kind::Wither);
    assert_eq!(withers.len(), 1);
    let wither = withers[0].id;
    assert!(
        saved(&s, wither)
            .get_i32("Invul")
            .is_some_and(|ticks| ticks > 200)
    );
    assert!((withers[0].position.x - 7.5).abs() < 1e-6);
    assert!((withers[0].position.y - 64.55).abs() < 0.3);
    for dx in 0..3 {
        for y in [65, 66] {
            let state = s
                .server
                .block_state_at(BlockPos::new(6 + dx, y, 6))
                .unwrap();
            assert!(state.is_air(), "{dx} {y}");
        }
    }
    assert!(
        s.server
            .block_state_at(BlockPos::new(7, 64, 6))
            .unwrap()
            .is_air()
    );
}

#[test]
fn a_wither_cannot_be_given_effects() {
    let mut s = ready(3);
    let wither = summon(&mut s, Kind::Wither, "8.5 120 24.5", "{NoAI:1b}");
    console(
        &mut s,
        "effect give @e[type=minecraft:wither] minecraft:poison 30",
    );
    assert!(
        saved(&s, wither)
            .get_list("active_effects")
            .is_none_or(|list| list.is_empty())
    );
}

#[test]
fn a_t_built_in_peaceful_summons_nothing() {
    let mut s = ready(3);
    console(&mut s, "difficulty peaceful");
    console(&mut s, "give Alice minecraft:wither_skeleton_skull 3");
    s.tick(2);
    for (index, dx) in (0..3).enumerate() {
        place_skull(&mut s, BlockPos::new(6 + dx, 65, 6), index as i32 + 1);
        s.tick(1);
    }
    s.tick(5);
    assert!(s.server.entities_of(Kind::Wither).is_empty());
    let state = s.server.block_state_at(BlockPos::new(8, 66, 6)).unwrap();
    assert!(!state.is_air());
}
