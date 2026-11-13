mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::commands::Target;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, GameEvent, Inbound};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Sea {
    biome: &'static str,
}

impl ChunkGenerator for Sea {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id(self.biome).unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        chunk.inhabited_time = 72_000_000;
        let wet = pos.x <= 0;
        for x in 0..16 {
            for z in 0..16 {
                let at = |y: i32| BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z);
                for y in 50..=55 {
                    chunk.set_block(at(y), BlockStateId(1));
                }
                if wet {
                    for y in 56..=62 {
                        chunk.set_block(at(y), BlockKind::Water.default_state());
                    }
                } else {
                    for y in 56..=62 {
                        chunk.set_block(at(y), BlockStateId(1));
                    }
                }
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn sea_server(seed: u64, biome: &'static str, spawn_mobs: bool) -> Server<Link> {
    let config = GameConfig {
        view_distance: 8,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    let data = LevelData {
        spawn_mobs,
        day_time: 6_000,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(Sea { biome }),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn ready(seed: u64) -> Scene {
    ready_with(seed, "minecraft:warm_ocean", false)
}

fn ready_with(seed: u64, biome: &'static str, spawn_mobs: bool) -> Scene {
    let mut s = scene_on(sea_server(seed, biome, spawn_mobs), &["Alice"], Vec::new());
    s.tick(120);
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

fn position_of(s: &Scene, kind: Kind, id: EntityId) -> Vec3 {
    s.server
        .entities_of(kind)
        .into_iter()
        .find(|entity| entity.id == id)
        .expect("the entity")
        .position
}

fn health_of(s: &Scene, id: EntityId) -> f64 {
    saved(s, id).get_f64("Health").unwrap_or(0.0)
}

#[test]
fn guardians_swim_in_the_water_and_save_nothing_but_the_common_layout() {
    let mut s = ready(3);
    let guardian = summon(&mut s, Kind::Guardian, "-8.5 58 8.5", "");
    let start = position_of(&s, Kind::Guardian, guardian);
    let mut travelled = 0.0_f64;
    for _ in 0..8 {
        s.tick(25);
        let at = position_of(&s, Kind::Guardian, guardian);
        assert!(at.y > 55.5 && at.y < 63.2, "{at:?}");
        travelled = travelled.max(at.distance_squared(start));
    }
    assert!(travelled > 0.5, "{travelled}");
    assert_eq!(
        saved(&s, guardian).get_bool("PersistenceRequired"),
        Some(false)
    );
}

#[test]
fn an_elder_guardian_curses_players_within_fifty_blocks_with_mining_fatigue() {
    let mut s = ready(5);
    summon(
        &mut s,
        Kind::ElderGuardian,
        "-3.5 58 8.5",
        "{\"Spigot.ticksLived\":1190}",
    );
    s.players[0].link.take();
    let mut cursed = false;
    let mut told = false;
    for _ in 0..30 {
        s.tick(1);
        for event in s.players[0].link.take() {
            match event {
                ClientEvent::EffectApplied { effect, .. }
                    if effect.effect.name() == Some("minecraft:mining_fatigue") =>
                {
                    assert_eq!(effect.amplifier, 2);
                    cursed = true;
                }
                ClientEvent::GameEvent(GameEvent::GuardianElderEffect(value)) => {
                    assert_eq!(value, 1.0);
                    told = true;
                }
                _ => {}
            }
        }
    }
    assert!(cursed && told);
}

#[test]
fn a_still_guardian_returns_two_thorn_damage_to_whoever_strikes_it_in_melee() {
    let mut s = ready(3);
    let guardian = summon(&mut s, Kind::Guardian, "-8.5 58 8.5", "{NoAI:1b}");
    let foe = summon(&mut s, Kind::Vindicator, "-8.5 58 9.5", "{NoAI:1b}");
    let before = health_of(&s, foe);
    s.server.do_hurt_target(foe, Target::Entity(guardian));
    s.tick(2);
    let after = health_of(&s, foe);
    assert!(before - after >= 2.0 - 1.0e-6, "{before} {after}");
    assert!(health_of(&s, guardian) < 30.0);
}

#[test]
fn a_guardian_that_targets_a_player_hurts_them_with_its_beam() {
    let mut s = ready(7);
    summon(&mut s, Kind::Guardian, "-6.5 60 8.5", "");
    console(&mut s, "tp Alice -3.5 60 8.5");
    let alice = s.players[0].id;
    s.tick(200);
    let health = s.server.player_health(alice).expect("alice lives");
    assert!(health < 20.0, "{health}");
}
