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
use rspigot_domain::dragon::phase::DragonPhase;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Island;

impl ChunkGenerator for Island {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                let at = |y: i32| BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z);
                for y in 40..=62 {
                    chunk.set_block(at(y), BlockStateId(1));
                }
                chunk.set_block(at(63), BlockKind::EndStone.default_state());
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
        view_distance: 8,
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
        Arc::new(Island),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    let plugin = connect_with(&mut server, 1, "tester", Vec::new());
    let alice = join_with(
        &mut server,
        "Alice",
        rspigot_domain::player::ClientSettings {
            view_distance: 8,
            ..Default::default()
        },
    );
    server.deliver();
    alice.link.take();
    let mut s = Scene {
        server,
        plugin,
        players: vec![alice],
    };
    stream_chunk_of(&mut s, BlockPos::new(8, 64, 8));
    s.server.wait_for_spawn(Duration::from_secs(60));
    s.tick(20);
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

fn position_of(s: &Scene, kind: Kind, id: EntityId) -> Vec3 {
    s.server
        .entities_of(kind)
        .into_iter()
        .find(|entity| entity.id == id)
        .map(|entity| entity.position)
        .expect("the entity")
}

#[test]
fn a_summoned_dragon_hovers_where_it_was_summoned_in_the_overworld() {
    let mut s = ready(1);
    let dragon = summon(&mut s, Kind::EnderDragon, "8.5 100 8.5", "{}");
    s.tick(40);
    assert_eq!(
        s.server.dragon_phase_of(dragon),
        Some(DragonPhase::Hovering)
    );
    let at = position_of(&s, Kind::EnderDragon, dragon);
    assert!(
        (at.x - 8.5).abs() < 0.5 && (at.y - 100.0).abs() < 0.5,
        "{at:?}"
    );
}

#[test]
fn the_parts_take_the_ids_after_the_dragon_and_the_next_entity_skips_them() {
    let mut s = ready(1);
    let dragon = summon(&mut s, Kind::EnderDragon, "8.5 100 8.5", "{}");
    let other = summon(&mut s, Kind::Cow, "8.5 64 8.5", "{NoAI:1b}");
    assert!(other.0 > dragon.0 + 8, "{dragon:?} {other:?}");
    let parts = s.server.dragon_parts(dragon).unwrap();
    assert_eq!(parts.len(), 8);
}

#[test]
fn a_holding_dragon_flies_around_its_pillars_without_leaving_the_arena() {
    let mut s = ready(2);
    let dragon = summon(&mut s, Kind::EnderDragon, "8.5 100 8.5", "{DragonPhase:0}");
    let mut seen = Vec::new();
    for _ in 0..30 {
        s.tick(20);
        seen.push(position_of(&s, Kind::EnderDragon, dragon));
    }
    let moved = seen
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).length())
        .sum::<f64>();
    assert!(moved > 20.0, "{moved}");
    assert!(seen.iter().all(|at| at.length() < 400.0), "{seen:?}");
}
