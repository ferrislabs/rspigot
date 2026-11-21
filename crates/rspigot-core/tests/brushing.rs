mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, Hand, PlayerInput};
use rspigot_core::plugins::{InventoryOrder, ItemSpec, Order};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::{BlockEntity, ChunkColumn};
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, Identifier, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;

const DUNE: BlockPos = BlockPos::new(8, 64, 11);
const DOWN_AT_DUNE: Rotation = Rotation::new(0.0, 20.5);
const SKY: Rotation = Rotation::new(0.0, -45.0);
const USING_FLAGS: u8 = 8;

struct Dig;

impl ChunkGenerator for Dig {
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
        if DUNE.chunk() == pos {
            chunk.set_block(DUNE, BlockKind::SuspiciousSand.default_state());
            chunk.set_block_entity(BlockEntity {
                pos: DUNE,
                kind: Identifier::minecraft("brushable_block"),
                data: Compound::new()
                    .with("LootTable", "minecraft:archaeology/desert_pyramid")
                    .with("LootTableSeed", 5_i64),
            });
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn dig() -> Scene {
    let config = GameConfig {
        view_distance: 3,
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
        Arc::new(Dig),
        StatusBoard::new(20),
    )
    .with_rng_seed(7);
    server.wait_for_spawn(Duration::from_secs(10));
    let mut s = scene_on(server, &["Alice"], Vec::new());
    give_brush(&mut s);
    s
}

fn give_brush(s: &mut Scene) {
    let player = s.players[0].id;
    s.act(Order::Inventory {
        player,
        order: InventoryOrder::Set {
            slot: 0,
            item: Some(ItemSpec::of("minecraft:brush", 1)),
        },
    });
}

fn face(s: &mut Scene, rotation: Rotation) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: None,
            rotation: Some(rotation),
            on_ground: true,
        },
    );
}

fn sounds(events: &[ClientEvent]) -> Vec<&'static str> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Sound { sound, .. } => Some(*sound),
            _ => None,
        })
        .collect()
}

#[test]
fn a_brush_starts_only_when_a_block_is_in_reach() {
    let mut s = dig();
    let alice = s.players[0].id;
    face(&mut s, SKY);
    assert!(!s.server.use_brush(alice, Hand::Main));
    face(&mut s, DOWN_AT_DUNE);
    assert!(s.server.use_brush(alice, Hand::Main));
    assert!(!s.server.use_brush(alice, Hand::Main));
}

#[test]
fn brushing_raises_the_dust_every_twenty_ticks() {
    let mut s = dig();
    let alice = s.players[0].id;
    face(&mut s, DOWN_AT_DUNE);
    assert!(s.server.use_brush(alice, Hand::Main));

    s.tick(5);
    assert_eq!(s.server.brush_progress(DUNE), 1);
    s.tick(20);
    assert_eq!(s.server.brush_progress(DUNE), 2);
    s.tick(20);
    assert_eq!(s.server.brush_progress(DUNE), 3);
    let state = s.server.block_at(DUNE).unwrap();
    assert_eq!(state.property("dusted"), Some("2"));
}

#[test]
fn the_brusher_is_not_sent_their_own_brushing_sound() {
    let mut s = dig();
    let alice = s.players[0].id;
    face(&mut s, DOWN_AT_DUNE);
    s.players[0].link.take();
    assert!(s.server.use_brush(alice, Hand::Main));

    s.tick(6);

    assert!(sounds(&s.players[0].link.take()).is_empty());
}

#[test]
fn the_tenth_brush_breaks_the_block_and_drops_the_archaeology_loot() {
    let mut s = dig();
    let alice = s.players[0].id;
    face(&mut s, DOWN_AT_DUNE);
    assert!(s.server.use_brush(alice, Hand::Main));

    s.tick(190);

    let state = s.server.block_at(DUNE).unwrap();
    assert_eq!(state.kind(), Some(BlockKind::Sand));
    let dropped = s.server.entities_of(Kind::Item);
    assert_eq!(dropped.len(), 1);
    assert!((dropped[0].position.x - 8.5).abs() < 0.01);
    assert!((dropped[0].position.z - 10.5).abs() < 0.01);
    assert!(dropped[0].position.y < 65.0);
    assert_eq!(s.server.brush_progress(DUNE), 0);
}

#[test]
fn looking_away_stops_the_brushing() {
    let mut s = dig();
    let alice = s.players[0].id;
    face(&mut s, DOWN_AT_DUNE);
    assert!(s.server.use_brush(alice, Hand::Main));
    s.tick(6);
    assert_eq!(s.server.brush_progress(DUNE), 1);
    s.players[0].link.take();

    face(&mut s, SKY);
    s.tick(30);

    assert_eq!(s.server.brush_progress(DUNE), 1);
    let cleared = s.players[0].link.take().into_iter().any(|event| {
        matches!(&event, ClientEvent::EntityMetadata { entries, .. }
            if entries.iter().any(|entry| entry.index == USING_FLAGS))
    });
    assert!(cleared);
}

#[test]
fn the_dust_recedes_when_the_brushing_stops() {
    let mut s = dig();
    let alice = s.players[0].id;
    face(&mut s, DOWN_AT_DUNE);
    assert!(s.server.use_brush(alice, Hand::Main));
    s.tick(26);
    assert_eq!(s.server.brush_progress(DUNE), 2);
    face(&mut s, SKY);
    s.tick(70);
    face(&mut s, DOWN_AT_DUNE);
    assert!(s.server.use_brush(alice, Hand::Main));

    s.tick(6);

    assert_eq!(s.server.brush_progress(DUNE), 1);
}

#[test]
fn completing_the_block_wears_the_brush() {
    let mut s = dig();
    let alice = s.players[0].id;
    face(&mut s, DOWN_AT_DUNE);
    assert!(s.server.use_brush(alice, Hand::Main));

    s.tick(190);

    assert_eq!(s.server.held_damage(alice), Some(1));
}

#[test]
fn the_loot_table_is_unpacked_into_the_block_on_the_first_brush() {
    let mut s = dig();
    let alice = s.players[0].id;
    face(&mut s, DOWN_AT_DUNE);
    assert!(s.server.use_brush(alice, Hand::Main));

    s.tick(6);

    let stack = s.server.brushed_stack(DUNE);
    assert!(stack.is_some(), "{stack:?}");
}
