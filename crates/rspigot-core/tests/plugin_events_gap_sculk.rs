mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{Inbound, PlayerInput};
use rspigot_core::plugins::events::SculkBloomPatch;
use rspigot_core::plugins::{EventKind, EventPatch, EventPayload, HandlerSpec};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, Vec3};
use rspigot_entities::EntityKind as Kind;

const CATALYST: BlockPos = BlockPos::new(8, 64, 8);

struct Meadow;

impl ChunkGenerator for Meadow {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                for y in 60..=63 {
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        BlockKind::Stone.default_state(),
                    );
                }
            }
        }
        if CATALYST.chunk() == pos {
            chunk.set_block(CATALYST, BlockKind::SculkCatalyst.default_state());
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 12.5)
    }
}

fn meadow(handlers: Vec<HandlerSpec>) -> Scene {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
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
        Arc::new(Meadow),
        StatusBoard::new(20),
    )
    .with_rng_seed(7);
    server.wait_for_spawn(Duration::from_secs(10));
    let mut s = scene_on(server, &["Alice"], handlers);
    s.tick(30);
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn sculk_blocks(s: &Scene) -> usize {
    let mut count = 0;
    for x in -4..=20 {
        for z in -4..=20 {
            for y in 60..=66 {
                let at = BlockPos::new(x, y, z);
                if s.server.block_state_at(at).and_then(|state| state.kind())
                    == Some(BlockKind::Sculk)
                {
                    count += 1;
                }
            }
        }
    }
    count
}

fn kill_a_zombie_by_hand(s: &mut Scene) {
    console(
        s,
        "summon minecraft:zombie 8.5 64 10.5 {NoAI:1b,Silent:1b,Health:0.5f,PersistenceRequired:1b}",
    );
    s.tick(2);
    let alice = s.players[0].clone();
    let target = s.server.entities_of(Kind::Zombie)[0].id;
    send_input(&mut s.server, &alice, PlayerInput::Attack(target));
    s.tick(1);
}

#[test]
fn a_death_near_a_catalyst_asks_sculk_bloom_with_the_experience_as_charge() {
    let mut s = meadow(vec![observe(1, EventKind::SculkBloom)]);
    kill_a_zombie_by_hand(&mut s);
    let asked = settled_for(&s.plugin, EventKind::SculkBloom).remove(0);
    let EventPayload::SculkBloom(bloom) = asked.payload else {
        panic!("expected a SculkBloom");
    };
    assert_eq!(bloom.block, CATALYST);
    assert_eq!(bloom.charge, 5);
}

#[test]
fn a_cancelled_sculk_bloom_spreads_nothing() {
    let mut s = meadow(vec![deciding(1, EventKind::SculkBloom)]);
    kill_a_zombie_by_hand(&mut s);
    let asked = last_asked(&s.plugin, EventKind::SculkBloom);
    judge(&mut s.server, 1, asked.id, 1, true, None);
    s.tick(120);
    assert_eq!(sculk_blocks(&s), 0);
}

#[test]
fn an_allowed_sculk_bloom_spreads_and_a_zero_charge_spreads_nothing() {
    let mut s = meadow(vec![deciding(1, EventKind::SculkBloom)]);
    kill_a_zombie_by_hand(&mut s);
    let asked = last_asked(&s.plugin, EventKind::SculkBloom);
    judge(&mut s.server, 1, asked.id, 1, false, None);
    s.tick(120);
    assert!(sculk_blocks(&s) > 0);

    let mut s = meadow(vec![deciding(1, EventKind::SculkBloom)]);
    kill_a_zombie_by_hand(&mut s);
    let asked = last_asked(&s.plugin, EventKind::SculkBloom);
    let patch = EventPatch::SculkBloom(SculkBloomPatch { charge: Some(0) });
    judge(&mut s.server, 1, asked.id, 1, false, Some(patch));
    s.tick(120);
    assert_eq!(sculk_blocks(&s), 0);
}
