mod common;

use std::ops::{Range, RangeInclusive};
use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::{Block, Stone, Water};
use rspigot_core::config::{GameConfig, LevelData, SpawningConfig};
use rspigot_core::events::{ClientEvent, Inbound, PlayerInput};
use rspigot_core::plugins::events::CreatureSpawnReason;
use rspigot_core::plugins::{EventKind, EventPayload, HandlerSpec};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::spawn::state::{CategorySettings, SpawnSettings};
use rspigot_domain::world::{Difficulty, Dimension};
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, Vec3};
use rspigot_entities::{EntityKind as Kind, MobCategory};
use rspigot_nbt::{Compound, Tag};

const POCKET: RangeInclusive<i32> = 30..=32;
const OPEN: Range<i32> = -48..64;
const EAST: i32 = 8;
const MARGIN: i32 = 16;

#[derive(Clone, Copy)]
struct Placed {
    name: &'static str,
    bounds: [i32; 6],
}

struct Cave {
    structure: Option<Placed>,
    flooded: bool,
}

fn pack(chunk: ChunkPos) -> i64 {
    (i64::from(chunk.x) & 0xFFFF_FFFF) | ((i64::from(chunk.z) & 0xFFFF_FFFF) << 32)
}

fn structures(placed: Placed, pos: ChunkPos) -> Compound {
    let origin = ChunkPos::new(0, 0);
    let mut tag = Compound::new().with(
        "References",
        Compound::new().with(placed.name, Tag::LongArray(vec![pack(origin)])),
    );
    if pos == origin {
        let piece = Compound::new().with("BB", Tag::IntArray(placed.bounds.to_vec()));
        tag.insert(
            "starts",
            Compound::new().with(
                placed.name,
                Compound::new()
                    .with("id", placed.name)
                    .with("ChunkX", 0)
                    .with("ChunkZ", 0)
                    .with("Children", Tag::List(vec![Tag::Compound(piece)])),
            ),
        );
    }
    tag
}

impl ChunkGenerator for Cave {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(1));
        for x in 0..16 {
            for z in 0..16 {
                let (bx, bz) = (pos.min_block_x() + x, pos.min_block_z() + z);
                let open = OPEN.contains(&bx) && OPEN.contains(&bz);
                for y in 20..=63 {
                    let state = if open && POCKET.contains(&y) {
                        if !self.flooded {
                            continue;
                        }
                        Water::DEFAULT_STATE
                    } else {
                        Stone::DEFAULT_STATE
                    };
                    chunk.set_block(BlockPos::new(bx, y, bz), state);
                }
            }
        }
        if let Some(placed) = self.structure {
            chunk.extra.insert("structures", structures(placed, pos));
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn every_tick(limit: i32) -> CategorySettings {
    CategorySettings {
        limit,
        ticks_per: 1,
    }
}

fn monsters_only() -> SpawningConfig {
    SpawningConfig {
        categories: SpawnSettings {
            monster: every_tick(70),
            creature: every_tick(0),
            ambient: every_tick(0),
            axolotls: every_tick(0),
            underground_water_creature: every_tick(0),
            water_creature: every_tick(0),
            water_ambient: every_tick(0),
        },
        ..SpawningConfig::default()
    }
}

fn cave(structure: Option<Placed>, flooded: bool, handlers: Vec<HandlerSpec>) -> Scene {
    let config = GameConfig {
        view_distance: 4,
        simulation_distance: 4,
        chunk_workers: 2,
        spawn_protection: 0,
        spawning: monsters_only(),
        ..GameConfig::default()
    };
    let level = LevelData {
        day_time: 18_000,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let server = Server::new(
        config,
        level,
        Arc::new(NoStore),
        Arc::new(Cave { structure, flooded }),
        StatusBoard::new(20),
    )
    .with_rng_seed(7);
    let mut server = server;
    server.wait_for_spawn(Duration::from_secs(10));
    scene_on(server, &["Alice"], handlers)
}

fn run(s: &mut Scene, ticks: usize) {
    for _ in 0..ticks {
        s.server.wait_for_spawn(Duration::from_millis(5));
        s.tick(1);
        let player = s.players[0].clone();
        for event in player.link.take() {
            if let ClientEvent::ChunkBatchFinished { .. } = event {
                send_input(
                    &mut s.server,
                    &player,
                    PlayerInput::ChunkBatchReceived {
                        chunks_per_tick: 64.0,
                    },
                );
            }
        }
    }
}

fn monsters(s: &Scene) -> Vec<(Kind, Vec3)> {
    Kind::ALL
        .iter()
        .filter(|kind| kind.category() == MobCategory::Monster)
        .flat_map(|kind| {
            s.server
                .entities_of(*kind)
                .into_iter()
                .map(|snapshot| (*kind, snapshot.position))
        })
        .collect()
}

fn split(found: Vec<(Kind, Vec3)>) -> (Vec<Kind>, Vec<Kind>) {
    let (inside, outside): (Vec<_>, Vec<_>) = found
        .into_iter()
        .partition(|(_, position)| position.block().x >= EAST);
    (
        inside
            .into_iter()
            .filter(|(_, position)| position.block().x >= EAST + MARGIN)
            .map(|(kind, _)| kind)
            .collect(),
        outside
            .into_iter()
            .filter(|(_, position)| position.block().x <= EAST - 2 * MARGIN)
            .map(|(kind, _)| kind)
            .collect(),
    )
}

fn east(name: &'static str) -> Placed {
    Placed {
        name,
        bounds: [EAST, 20, -1000, 1000, 70, 1000],
    }
}

#[test]
fn a_nether_fortress_piece_spawns_its_own_monsters_and_the_rest_spawns_the_biome_list() {
    let mut s = cave(Some(east("minecraft:fortress")), false, Vec::new());
    run(&mut s, 120);
    let (inside, outside) = split(monsters(&s));
    let fortress = [
        Kind::Blaze,
        Kind::ZombifiedPiglin,
        Kind::WitherSkeleton,
        Kind::Skeleton,
        Kind::MagmaCube,
    ];
    assert!(!inside.is_empty(), "nothing spawned in the fortress");
    assert!(
        inside.iter().all(|kind| fortress.contains(kind)),
        "{inside:?}"
    );
    assert!(
        inside
            .iter()
            .any(|kind| matches!(kind, Kind::Blaze | Kind::WitherSkeleton)),
        "{inside:?}"
    );
    assert!(!outside.is_empty(), "nothing spawned outside the fortress");
    assert!(
        outside.iter().all(|kind| !matches!(
            kind,
            Kind::Blaze | Kind::WitherSkeleton | Kind::ZombifiedPiglin | Kind::MagmaCube
        )),
        "{outside:?}"
    );
}

#[test]
fn a_swamp_hut_box_spawns_witches_only() {
    let mut s = cave(Some(east("minecraft:swamp_hut")), false, Vec::new());
    run(&mut s, 120);
    let (inside, outside) = split(monsters(&s));
    assert!(!inside.is_empty());
    assert!(inside.iter().all(|kind| *kind == Kind::Witch), "{inside:?}");
    assert!(outside.iter().any(|kind| *kind != Kind::Witch));
}

#[test]
fn an_outpost_box_spawns_pillagers_only() {
    let mut s = cave(Some(east("minecraft:pillager_outpost")), false, Vec::new());
    run(&mut s, 120);
    let (inside, outside) = split(monsters(&s));
    assert!(!inside.is_empty());
    assert!(
        inside.iter().all(|kind| *kind == Kind::Pillager),
        "{inside:?}"
    );
    assert!(outside.iter().all(|kind| *kind != Kind::Pillager));
}

#[test]
fn an_ocean_monument_box_spawns_guardians() {
    let mut s = cave(Some(east("minecraft:monument")), true, Vec::new());
    run(&mut s, 120);
    let (inside, _) = split(monsters(&s));
    assert!(!inside.is_empty(), "nothing spawned in the monument");
    assert!(
        inside.iter().all(|kind| *kind == Kind::Guardian),
        "{inside:?}"
    );
}

#[test]
fn natural_spawns_inside_a_structure_still_report_the_natural_reason() {
    let mut s = cave(
        Some(east("minecraft:swamp_hut")),
        false,
        vec![observe(1, EventKind::CreatureSpawn)],
    );
    run(&mut s, 60);
    let reasons: Vec<CreatureSpawnReason> = fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::CreatureSpawn(spawn) => Some(spawn.spawn_reason),
            _ => None,
        })
        .collect();
    assert!(!reasons.is_empty());
    assert!(
        reasons
            .iter()
            .all(|reason| *reason == CreatureSpawnReason::Natural)
    );
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn no_natural_spawns() -> SpawningConfig {
    SpawningConfig {
        categories: SpawnSettings {
            monster: every_tick(0),
            creature: every_tick(0),
            ambient: every_tick(0),
            axolotls: every_tick(0),
            underground_water_creature: every_tick(0),
            water_creature: every_tick(0),
            water_ambient: every_tick(0),
        },
        ..SpawningConfig::default()
    }
}

fn village_at_night(seed: u64, difficulty: Difficulty, handlers: Vec<HandlerSpec>) -> Scene {
    let server = configured_level(
        |config| {
            config.difficulty = difficulty;
            config.spawning = no_natural_spawns();
        },
        |level| {
            level.day_time = 17_000;
            level.daylight_cycle = true;
            level.spawn_mobs = true;
        },
    )
    .with_rng_seed(seed);
    let mut s = scene_on(server, &["Alice"], handlers);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    console(&mut s, "setblock 8 64 8 minecraft:lectern");
    console(
        &mut s,
        "summon minecraft:villager 7.5 64 8.5 {VillagerData:{type:\"minecraft:plains\",profession:\"minecraft:librarian\",level:2},Xp:20}",
    );
    s.tick(100);
    console(&mut s, "time set 17990");
    s
}

fn siege_zombies(s: &Scene) -> usize {
    s.server.entities_of(Kind::Zombie).len()
}

fn first_siege(difficulty: Difficulty) -> Option<Scene> {
    (0..80).find_map(|seed| {
        let mut s = village_at_night(seed, difficulty, Vec::new());
        s.tick(200);
        (siege_zombies(&s) > 0).then_some(s)
    })
}

#[test]
fn a_zombie_siege_rings_the_village_with_twenty_zombies_once_per_night() {
    let mut s = first_siege(Difficulty::Normal).expect("no seed started a siege");
    s.tick(200);
    let spawned = siege_zombies(&s);
    assert!((15..=20).contains(&spawned), "{spawned}");
    let alice = s.server.entities_of(Kind::Zombie);
    for zombie in &alice {
        assert_eq!(zombie.position.y, 64.0);
        let (dx, dz) = (zombie.position.x - 8.5, zombie.position.z - 8.5);
        let distance = (dx * dx + dz * dz).sqrt();
        assert!(distance < 60.0, "{distance}");
    }
    s.tick(400);
    assert_eq!(siege_zombies(&s), spawned);
}

#[test]
fn a_zombie_siege_reports_the_village_invasion_reason() {
    let found = (0..80).find_map(|seed| {
        let mut s = village_at_night(
            seed,
            Difficulty::Normal,
            vec![observe(1, EventKind::CreatureSpawn)],
        );
        s.tick(200);
        let reasons: Vec<CreatureSpawnReason> = fired(&s.plugin)
            .into_iter()
            .filter_map(|event| match event.payload {
                EventPayload::CreatureSpawn(spawn) => Some(spawn.spawn_reason),
                _ => None,
            })
            .filter(|reason| *reason != CreatureSpawnReason::Command)
            .collect();
        (!reasons.is_empty()).then_some(reasons)
    });
    let reasons = found.expect("no seed started a siege");
    assert!(
        reasons
            .iter()
            .all(|reason| *reason == CreatureSpawnReason::VillageInvasion)
    );
}

#[test]
fn no_siege_in_peaceful_or_without_a_village() {
    for seed in 0..40 {
        let mut s = village_at_night(seed, Difficulty::Peaceful, Vec::new());
        s.tick(200);
        assert_eq!(siege_zombies(&s), 0);
    }
    for seed in 0..40 {
        let server = configured_level(
            |config| config.spawning = no_natural_spawns(),
            |level| {
                level.day_time = 17_000;
                level.daylight_cycle = true;
                level.spawn_mobs = true;
            },
        )
        .with_rng_seed(seed);
        let mut s = scene_on(server, &["Alice"], Vec::new());
        stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
        console(&mut s, "time set 17990");
        s.tick(200);
        assert_eq!(siege_zombies(&s), 0);
    }
}

#[test]
fn no_siege_starts_while_a_raid_is_active() {
    for seed in 0..25 {
        let mut s = village_at_night(seed, Difficulty::Normal, Vec::new());
        console(&mut s, "time set 12000");
        console(&mut s, "effect give Alice minecraft:bad_omen 120 0");
        s.tick(700);
        assert_eq!(s.server.raids().ids().len(), 1, "seed {seed}");
        console(&mut s, "time set 17990");
        s.tick(200);
        assert_eq!(siege_zombies(&s), 0, "seed {seed}");
    }
}
