mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_core::config::{GameConfig, LevelData, PluginSettings};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Vec3};
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

fn plains_server(seed: u64, day_time: i64, adjust: impl FnOnce(&mut GameConfig)) -> Server<Link> {
    let mut config = GameConfig {
        view_distance: 7,
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
        spawn_mobs: true,
        spawn_monsters: true,
        day_time,
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

fn ready(seed: u64, day_time: i64, adjust: impl FnOnce(&mut GameConfig)) -> Scene {
    let mut s = scene_on(
        plains_server(seed, day_time, adjust),
        &["Alice"],
        Vec::new(),
    );
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

const LATE: i64 = 126_000;
const EARLY: i64 = 6_000;

fn patrols(s: &Scene) -> Vec<rspigot_core::entities::EntitySnapshot> {
    s.server.entities_of(Kind::Pillager)
}

fn first_tick_patrol(seed: u64, adjust: impl FnOnce(&mut GameConfig)) -> Option<Scene> {
    let mut s = ready(seed, LATE, adjust);
    s.tick(2);
    (!patrols(&s).is_empty()).then_some(s)
}

#[test]
fn a_daytime_patrol_spawns_around_a_player_with_a_leader_wearing_the_banner() {
    let found = (0..200).find_map(|seed| first_tick_patrol(seed, |_| {}));
    let s = found.expect("a seed rolls a patrol on the first attempt");
    let members = patrols(&s);
    assert!(!members.is_empty());
    let leaders: Vec<_> = members
        .iter()
        .filter(|member| {
            s.server
                .saved_entity(member.id)
                .and_then(|saved| saved.get_bool("PatrolLeader"))
                == Some(true)
        })
        .collect();
    assert_eq!(leaders.len(), 1);
    for member in &members {
        let saved = s.server.saved_entity(member.id).unwrap();
        assert_eq!(saved.get_bool("Patrolling"), Some(true));
        assert_eq!(member.position.x.fract(), 0.0);
    }
    let leader = s.server.saved_entity(leaders[0].id).unwrap();
    assert!(leader.get("patrol_target").is_some());
    let head = leader
        .get_compound("equipment")
        .and_then(|equipment| equipment.get_compound("head"))
        .expect("a banner");
    assert_eq!(head.get_str("id"), Some("minecraft:white_banner"));
}

#[test]
fn no_patrol_spawns_when_the_game_rule_is_off() {
    for seed in 0..60 {
        let mut s = ready(seed, LATE, |config| config.rules.spawn_patrols = false);
        s.tick(2);
        assert!(patrols(&s).is_empty(), "{seed}");
    }
}

#[test]
fn no_patrol_spawns_in_the_early_game() {
    for seed in 0..60 {
        let mut s = ready(seed, EARLY, |_| {});
        s.tick(2);
        assert!(patrols(&s).is_empty(), "{seed}");
    }
}
