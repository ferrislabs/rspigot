mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::Inbound;
use rspigot_core::plugins::events::{
    CreatureSpawnReason, CubeMobSplitPatch, EntityTransformReason,
};
use rspigot_core::plugins::{EventKind, EventPatch, EventPayload};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Marsh;

fn biome_of(_: ChunkPos) -> &'static str {
    "minecraft:plains"
}

impl ChunkGenerator for Marsh {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id(biome_of(pos)).unwrap();
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
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn marsh_server(seed: u64) -> Server<Link> {
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
        Arc::new(Marsh),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn ready(seed: u64) -> Scene {
    ready_with(seed, Vec::new())
}

fn ready_with(seed: u64, handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> Scene {
    let mut s = scene_on(marsh_server(seed), &["Alice"], handlers);
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

fn slimes(s: &Scene, kind: Kind) -> Vec<EntityId> {
    s.server
        .entities_of(kind)
        .into_iter()
        .map(|entity| entity.id)
        .collect()
}

fn size_of(s: &Scene, id: EntityId) -> i32 {
    saved(s, id).get_i32("Size").expect("a stored size") + 1
}

fn position_of(s: &Scene, kind: Kind, id: EntityId) -> Vec3 {
    s.server
        .entities_of(kind)
        .into_iter()
        .find(|entity| entity.id == id)
        .map(|entity| entity.position)
        .expect("a position")
}

#[test]
fn a_summoned_slime_keeps_its_stored_size_and_scales_its_health() {
    let mut s = ready(1);
    let slime = summon(&mut s, Kind::Slime, "20.5 64 20.5", "{Size:3,NoAI:1b}");
    let nbt = saved(&s, slime);
    assert_eq!(nbt.get_i32("Size"), Some(3));
    assert_eq!(nbt.get_f64("Health"), Some(16.0));
    let magma = summon(&mut s, Kind::MagmaCube, "22.5 64 20.5", "{Size:1,NoAI:1b}");
    let nbt = saved(&s, magma);
    assert_eq!(nbt.get_i32("Size"), Some(1));
    assert_eq!(nbt.get_f64("Health"), Some(4.0));
    let attributes = nbt.get_list("attributes").expect("attributes");
    let armor = attributes
        .iter()
        .filter_map(rspigot_nbt::Tag::as_compound)
        .find(|entry| entry.get_str("id") == Some("minecraft:armor"))
        .and_then(|entry| entry.get_f64("base"));
    assert_eq!(armor, Some(6.0));
}

#[test]
fn a_slime_without_a_player_nearby_hops_around() {
    let mut s = ready(2);
    let slime = summon(&mut s, Kind::Slime, "8.5 64 12.5", "{Size:1}");
    let start = position_of(&s, Kind::Slime, slime);
    let mut left_the_ground = false;
    for _ in 0..400 {
        s.tick(1);
        left_the_ground |= !s
            .server
            .entities_of(Kind::Slime)
            .iter()
            .find(|entity| entity.id == slime)
            .is_none_or(|entity| entity.on_ground);
    }
    let end = position_of(&s, Kind::Slime, slime);
    assert!(left_the_ground, "the slime never jumped");
    assert!(
        (end.x - start.x).abs() + (end.z - start.z).abs() > 0.5,
        "{start:?} -> {end:?}"
    );
}

#[test]
fn a_big_slime_hurts_the_player_it_touches_and_a_tiny_one_does_not() {
    let mut s = ready(3);
    let player = s.players[0].id;
    let before = s.server.player_health(player).unwrap();
    summon(&mut s, Kind::Slime, "8.5 64 8.5", "{Size:0,NoAI:1b}");
    s.tick(20);
    assert_eq!(s.server.player_health(player), Some(before));
    summon(&mut s, Kind::Slime, "8.5 64 8.5", "{Size:2}");
    s.tick(1);
    let after = s.server.player_health(player).unwrap();
    assert!(after < before, "{before} -> {after}");
}

#[test]
fn a_slime_without_ai_never_deals_contact_damage() {
    let mut s = ready(4);
    let player = s.players[0].id;
    let before = s.server.player_health(player).unwrap();
    let slime = summon(&mut s, Kind::Slime, "8.5 64 8.5", "{Size:2,NoAI:1b}");
    s.tick(30);
    assert_eq!(s.server.player_health(player), Some(before));
    assert!(exists(&s, Kind::Slime, slime));
}

#[test]
fn a_magma_cube_hurts_even_at_the_smallest_size() {
    let mut s = ready(5);
    let player = s.players[0].id;
    let before = s.server.player_health(player).unwrap();
    summon(&mut s, Kind::MagmaCube, "8.5 64 8.5", "{Size:0}");
    s.tick(2);
    assert!(s.server.player_health(player).unwrap() < before);
}

#[test]
fn a_dead_big_slime_splits_into_two_to_four_smaller_ones() {
    let mut s = ready(6);
    let slime = summon(&mut s, Kind::Slime, "20.5 64 20.5", "{Size:3,NoAI:1b}");
    console(&mut s, "kill @e[type=minecraft:slime]");
    for _ in 0..40 {
        s.tick(1);
    }
    assert!(!exists(&s, Kind::Slime, slime));
    let children = slimes(&s, Kind::Slime);
    assert!((2..=4).contains(&children.len()), "{}", children.len());
    for child in children {
        assert_eq!(size_of(&s, child), 2);
        let at = position_of(&s, Kind::Slime, child);
        assert!((at.x - 20.5).abs() <= 2.0 && (at.z - 20.5).abs() <= 2.0);
    }
}

#[test]
fn a_dead_tiny_slime_leaves_nothing_behind() {
    let mut s = ready(7);
    summon(&mut s, Kind::Slime, "20.5 64 20.5", "{Size:0,NoAI:1b}");
    console(&mut s, "kill @e[type=minecraft:slime]");
    for _ in 0..40 {
        s.tick(1);
    }
    assert!(slimes(&s, Kind::Slime).is_empty());
}

#[test]
fn split_children_inherit_the_name_the_persistence_and_the_missing_ai() {
    let mut s = ready(8);
    summon(
        &mut s,
        Kind::MagmaCube,
        "20.5 64 20.5",
        "{Size:3,NoAI:1b,PersistenceRequired:1b,CustomName:'\"Blob\"',Silent:1b}",
    );
    console(&mut s, "kill @e[type=minecraft:magma_cube]");
    for _ in 0..40 {
        s.tick(1);
    }
    let children = slimes(&s, Kind::MagmaCube);
    assert!(!children.is_empty());
    for child in children {
        let nbt = saved(&s, child);
        assert_eq!(size_of(&s, child), 2);
        assert_eq!(nbt.get_bool("NoAI"), Some(true));
        assert_eq!(nbt.get_bool("PersistenceRequired"), Some(true));
        assert_eq!(nbt.get_bool("Silent"), Some(true));
        assert!(nbt.get_str("CustomName").is_some());
        assert_eq!(nbt.get_f64("Health"), Some(4.0));
    }
}

#[test]
fn a_slime_remembers_whether_it_stood_on_the_ground() {
    let mut s = ready(9);
    let remembered = summon(
        &mut s,
        Kind::Slime,
        "20.5 64 20.5",
        "{Size:1,NoAI:1b,wasOnGround:1b}",
    );
    assert_eq!(saved(&s, remembered).get_bool("wasOnGround"), Some(true));
    let slime = summon(&mut s, Kind::Slime, "8.5 64 12.5", "{Size:1}");
    let mut grounded = 0;
    for _ in 0..60 {
        s.tick(1);
        let on_ground = s
            .server
            .entities_of(Kind::Slime)
            .into_iter()
            .find(|entity| entity.id == slime)
            .is_some_and(|entity| entity.on_ground);
        assert_eq!(saved(&s, slime).get_bool("wasOnGround"), Some(on_ground));
        grounded += usize::from(on_ground);
    }
    assert!(grounded > 0);
}

fn kill_big_slime(s: &mut Scene) {
    summon(s, Kind::Slime, "20.5 64 20.5", "{Size:3,NoAI:1b}");
    console(s, "kill @e[type=minecraft:slime]");
}

#[test]
fn the_split_event_carries_the_count_and_a_cancel_leaves_no_children() {
    let mut s = ready_with(10, vec![deciding(1, EventKind::CubeMobSplit)]);
    kill_big_slime(&mut s);
    let mut asked = None;
    for _ in 0..40 {
        s.tick(1);
        if asked.is_none() && !asked_for(&s.plugin, EventKind::CubeMobSplit).is_empty() {
            asked = Some(last_asked(&s.plugin, EventKind::CubeMobSplit));
            break;
        }
    }
    let asked = asked.expect("the split was asked");
    let EventPayload::CubeMobSplit(event) = &asked.payload else {
        panic!("not a split event");
    };
    assert!((2..=4).contains(&event.count));
    assert_eq!(event.entity.kind, "minecraft:slime");
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(5);
    assert!(slimes(&s, Kind::Slime).is_empty());
}

#[test]
fn a_handler_can_change_the_split_count() {
    let mut s = ready_with(11, vec![deciding(1, EventKind::CubeMobSplit)]);
    kill_big_slime(&mut s);
    for _ in 0..40 {
        s.tick(1);
        if !asked_for(&s.plugin, EventKind::CubeMobSplit).is_empty() {
            break;
        }
    }
    let asked = last_asked(&s.plugin, EventKind::CubeMobSplit);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::CubeMobSplit(CubeMobSplitPatch {
            count: Some(1),
        })),
    );
    s.tick(5);
    assert_eq!(slimes(&s, Kind::Slime).len(), 1);
}

#[test]
fn a_zero_count_splits_nothing_and_the_slime_still_goes_away() {
    let mut s = ready_with(12, vec![deciding(1, EventKind::CubeMobSplit)]);
    kill_big_slime(&mut s);
    for _ in 0..40 {
        s.tick(1);
        if !asked_for(&s.plugin, EventKind::CubeMobSplit).is_empty() {
            break;
        }
    }
    let asked = last_asked(&s.plugin, EventKind::CubeMobSplit);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::CubeMobSplit(CubeMobSplitPatch {
            count: Some(0),
        })),
    );
    s.tick(5);
    assert!(slimes(&s, Kind::Slime).is_empty());
}

#[test]
fn the_children_are_announced_as_a_split_transform_then_as_slime_split_spawns() {
    let mut s = ready_with(
        13,
        vec![
            observe(1, EventKind::EntityTransform),
            observe(2, EventKind::CreatureSpawn),
        ],
    );
    kill_big_slime(&mut s);
    s.tick(40);
    let fired = fired(&s.plugin);
    let transforms: Vec<_> = fired
        .iter()
        .filter_map(|event| match &event.payload {
            EventPayload::EntityTransform(transform) => Some(transform),
            _ => None,
        })
        .collect();
    assert_eq!(transforms.len(), 1);
    assert_eq!(transforms[0].transform_reason, EntityTransformReason::Split);
    assert!((2..=4).contains(&transforms[0].transformed_entities.len()));
    let splits = fired
        .iter()
        .filter_map(|event| match &event.payload {
            EventPayload::CreatureSpawn(spawn)
                if spawn.spawn_reason == CreatureSpawnReason::SlimeSplit =>
            {
                Some(spawn)
            }
            _ => None,
        })
        .count();
    assert_eq!(splits, transforms[0].transformed_entities.len());
}

#[test]
fn a_cancelled_transform_keeps_the_children_away() {
    let mut s = ready_with(14, vec![deciding(1, EventKind::EntityTransform)]);
    kill_big_slime(&mut s);
    for _ in 0..40 {
        s.tick(1);
        if !asked_for(&s.plugin, EventKind::EntityTransform).is_empty() {
            break;
        }
    }
    let asked = last_asked(&s.plugin, EventKind::EntityTransform);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(5);
    assert!(slimes(&s, Kind::Slime).is_empty());
}
