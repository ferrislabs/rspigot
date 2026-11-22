mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{Hand, PlayerInput};
use rspigot_core::plugins::{
    EventKind, EventPayload, HandlerSpec, InventoryOrder, ItemSpec, Order,
};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::{BlockEntity, ChunkColumn};
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, Identifier, Rotation, Vec3};
use rspigot_nbt::Compound;

const DUNE: BlockPos = BlockPos::new(8, 64, 11);
const DOWN_AT_DUNE: Rotation = Rotation::new(0.0, 20.5);

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

fn dig(handlers: Vec<HandlerSpec>) -> Scene {
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
    let mut s = scene_on(server, &["Alice"], handlers);
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

#[test]
fn a_brush_stroke_asks_block_brush_with_the_state_it_will_become() {
    let mut s = dig(vec![observe(1, EventKind::BlockBrush)]);
    let alice = s.players[0].id;
    face(&mut s, DOWN_AT_DUNE);
    assert!(s.server.use_brush(alice, Hand::Main));
    s.tick(5);
    let asked = settled_for(&s.plugin, EventKind::BlockBrush).remove(0);
    let EventPayload::BlockBrush(brush) = asked.payload else {
        panic!("expected a BlockBrush");
    };
    assert_eq!(brush.block, DUNE);
    assert_eq!(
        brush
            .new_state
            .state
            .properties
            .iter()
            .find(|(name, _)| name == "dusted")
            .map(|(_, value)| value.as_str()),
        Some("1")
    );
    assert_eq!(s.server.brush_progress(DUNE), 1);
}

#[test]
fn a_cancelled_brush_stroke_makes_no_progress() {
    let mut s = dig(vec![deciding(1, EventKind::BlockBrush)]);
    let alice = s.players[0].id;
    face(&mut s, DOWN_AT_DUNE);
    assert!(s.server.use_brush(alice, Hand::Main));
    s.tick(5);
    let asked = last_asked(&s.plugin, EventKind::BlockBrush);
    judge(&mut s.server, 1, asked.id, 1, true, None);
    assert_eq!(s.server.brush_progress(DUNE), 0);
}

#[test]
fn an_allowed_brush_stroke_makes_its_progress() {
    let mut s = dig(vec![deciding(1, EventKind::BlockBrush)]);
    let alice = s.players[0].id;
    face(&mut s, DOWN_AT_DUNE);
    assert!(s.server.use_brush(alice, Hand::Main));
    s.tick(5);
    assert_eq!(s.server.brush_progress(DUNE), 0);
    let asked = last_asked(&s.plugin, EventKind::BlockBrush);
    judge(&mut s.server, 1, asked.id, 1, false, None);
    assert_eq!(s.server.brush_progress(DUNE), 1);
}
