mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::Inbound;
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Meadow;

impl ChunkGenerator for Meadow {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        chunk.inhabited_time = 72_000_000;
        for x in 0..16 {
            for z in 0..16 {
                let at = |y: i32| BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z);
                for y in 60..=62 {
                    chunk.set_block(at(y), BlockStateId(1));
                }
                chunk.set_block(at(63), BlockKind::GrassBlock.default_state());
                if pos == ChunkPos::new(0, 0) && x < 4 {
                    chunk.set_block(at(71), BlockKind::Stone.default_state());
                }
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn meadow_server(seed: u64, day_time: i64, spawn_mobs: bool) -> Server<Link> {
    let config = GameConfig {
        view_distance: 5,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    let data = LevelData {
        spawn_mobs,
        day_time,
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
    server
}

fn ready(seed: u64) -> Scene {
    ready_at(seed, 6_000)
}

fn ready_at(seed: u64, day_time: i64) -> Scene {
    ready_with(seed, day_time, false)
}

fn ready_with(seed: u64, day_time: i64, spawn_mobs: bool) -> Scene {
    let mut s = scene_on(
        meadow_server(seed, day_time, spawn_mobs),
        &["Alice"],
        Vec::new(),
    );
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

fn exists(s: &Scene, kind: Kind, id: EntityId) -> bool {
    s.server
        .entities_of(kind)
        .iter()
        .any(|entity| entity.id == id)
}

fn position_of(s: &Scene, kind: Kind, id: EntityId) -> Vec3 {
    s.server
        .entities_of(kind)
        .into_iter()
        .find(|entity| entity.id == id)
        .expect("the entity")
        .position
}

#[test]
fn a_vex_floats_without_gravity_and_holds_an_iron_sword() {
    let mut s = ready(3);
    let vex = summon(&mut s, Kind::Vex, "8.5 80 8.5", "");
    s.tick(60);
    let at = position_of(&s, Kind::Vex, vex);
    assert!(at.y > 74.0 && at.y < 90.0, "{at:?}");
    assert!((at.x - 8.5).abs() < 20.0 && (at.z - 8.5).abs() < 20.0);
    let nbt = saved(&s, vex);
    let main = nbt
        .get_compound("equipment")
        .and_then(|equipment| equipment.get_compound("mainhand"));
    assert_eq!(
        main.and_then(|stack| stack.get_str("id")),
        Some("minecraft:iron_sword")
    );
}

#[test]
fn a_vex_with_a_limited_life_starves_when_it_runs_out() {
    let mut s = ready(3);
    let vex = summon(
        &mut s,
        Kind::Vex,
        "8.5 80 8.5",
        "{NoAI:1b,Health:3f,life_ticks:5}",
    );
    assert!(exists(&s, Kind::Vex, vex));
    s.tick(25);
    let health = s
        .server
        .entities_of(Kind::Vex)
        .into_iter()
        .find(|entity| entity.id == vex)
        .and_then(|entity| entity.health);
    assert!(health.is_some_and(|health| health < 3.0), "{health:?}");
    s.tick(60);
    assert!(!exists(&s, Kind::Vex, vex));
}

#[test]
fn a_vex_keeps_its_bound_position_life_and_no_gravity_across_a_save() {
    let mut s = ready(3);
    let vex = summon(
        &mut s,
        Kind::Vex,
        "8.5 80 8.5",
        "{NoAI:1b,bound_pos:[I;4,70,4],life_ticks:900}",
    );
    let nbt = saved(&s, vex);
    assert_eq!(nbt.get_int_array("bound_pos"), Some(&[4, 70, 4][..]));
    assert_eq!(nbt.get_i32("life_ticks"), Some(900));
}

#[test]
fn a_bat_summoned_with_a_command_flies_and_one_built_by_a_spawn_rests() {
    let mut s = ready(3);
    let bat = summon(&mut s, Kind::Bat, "8.5 70 8.5", "");
    assert_eq!(saved(&s, bat).get_i64("BatFlags"), Some(0));
    s.tick(40);
    let at = position_of(&s, Kind::Bat, bat);
    assert!(at.y > 60.0, "{at:?}");
}

#[test]
fn a_resting_bat_hangs_from_the_block_above_without_moving() {
    let mut s = ready(3);
    let bat = summon(&mut s, Kind::Bat, "2.5 70 2.5", "{BatFlags:1b}");
    s.tick(40);
    let at = position_of(&s, Kind::Bat, bat);
    let height = Kind::Bat.dimensions().height;
    assert!((at.y - (71.0 - f64::from(height))).abs() < 1.0e-6, "{at:?}");
    assert_eq!((at.x, at.z), (2.5, 2.5));
    assert_eq!(saved(&s, bat).get_i64("BatFlags"), Some(1));
}

#[test]
fn a_resting_bat_without_a_ceiling_wakes_up_at_once() {
    let mut s = ready(3);
    let bat = summon(&mut s, Kind::Bat, "12.5 70 2.5", "{BatFlags:1b}");
    s.tick(5);
    assert_eq!(saved(&s, bat).get_i64("BatFlags"), Some(0));
}

#[test]
fn a_phantom_keeps_its_size_and_anchor_and_grows_with_its_size() {
    let mut s = ready(3);
    let phantom = summon(
        &mut s,
        Kind::Phantom,
        "8.5 200 8.5",
        "{NoAI:1b,size:6,anchor_pos:[I;8,205,8]}",
    );
    let nbt = saved(&s, phantom);
    assert_eq!(nbt.get_i32("size"), Some(6));
    assert_eq!(nbt.get_int_array("anchor_pos"), Some(&[8, 205, 8][..]));
}

#[test]
fn a_fresh_phantom_anchors_five_blocks_above_its_spawn() {
    let mut s = ready(3);
    let phantom = summon(&mut s, Kind::Phantom, "8.5 200 8.5", "");
    let nbt = saved(&s, phantom);
    assert_eq!(nbt.get_int_array("anchor_pos"), Some(&[8, 205, 8][..]));
    assert_eq!(nbt.get_i32("size"), Some(0));
}

#[test]
fn a_phantom_out_of_reach_of_every_player_circles_around_its_anchor_without_falling() {
    let mut s = ready_at(3, 18_000);
    let phantom = summon(&mut s, Kind::Phantom, "8.5 150 8.5", "");
    s.tick(200);
    let at = position_of(&s, Kind::Phantom, phantom);
    assert!(at.y > 140.0 && at.y < 165.0, "{at:?}");
    let moved = (at.x - 8.5).abs() + (at.z - 8.5).abs();
    assert!(moved > 1.0, "{at:?}");
    assert!(moved < 40.0, "{at:?}");
    assert_eq!(s.server.mob_target(phantom), None);
}

#[test]
fn a_phantom_within_reach_of_a_player_picks_the_player_as_its_target() {
    let mut s = ready_at(3, 18_000);
    let phantom = summon(&mut s, Kind::Phantom, "8.5 90 8.5", "");
    s.tick(60);
    assert!(s.server.mob_target(phantom).is_some());
}

#[test]
fn a_phantom_burns_in_the_daylight() {
    let mut s = ready(3);
    let phantom = summon(&mut s, Kind::Phantom, "8.5 200 8.5", "");
    s.tick(400);
    assert!(!exists(&s, Kind::Phantom, phantom));
}

fn open_air_night(seed: u64) -> Scene {
    let mut s = ready_with(seed, 18_000, true);
    let alice = s.players[0].id;
    s.server.set_ticks_since_rest(alice, 10_000_000);
    s
}

fn force_difficulty(s: &mut Scene, line: &str) {
    console(s, line);
}

#[test]
fn a_player_who_never_rested_draws_phantoms_from_above_at_night() {
    let mut s = open_air_night(5);
    force_difficulty(&mut s, "difficulty hard");
    s.tick(2_400);
    let phantoms = s.server.entities_of(Kind::Phantom);
    assert!(!phantoms.is_empty());
    for phantom in phantoms {
        assert!(phantom.position.y > 64.0 + 19.0, "{phantom:?}");
        assert!((phantom.position.x - 8.5).abs() <= 11.0 + 20.0);
        assert!((phantom.position.z - 8.5).abs() <= 11.0 + 20.0);
    }
}

#[test]
fn a_rested_player_draws_no_phantoms() {
    let mut s = ready_with(5, 18_000, true);
    force_difficulty(&mut s, "difficulty hard");
    s.tick(2_400);
    assert!(s.server.entities_of(Kind::Phantom).is_empty());
}

#[test]
fn nothing_comes_by_day_and_the_timer_counts_the_ticks_since_rest() {
    let mut s = ready(5);
    let alice = s.players[0].id;
    s.server.set_ticks_since_rest(alice, 10_000_000);
    force_difficulty(&mut s, "difficulty hard");
    s.tick(2_400);
    assert!(s.server.entities_of(Kind::Phantom).is_empty());
    assert_eq!(s.server.ticks_since_rest(alice), 10_002_400);
}
