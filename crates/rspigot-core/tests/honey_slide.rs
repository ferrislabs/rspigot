mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, EntityStatus, PlayerInput};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, Vec3};

const WALL_X: i32 = 9;
const WALL_Z: i32 = 8;
const TOP: i32 = 110;
const START: f64 = 100.0;
const SIDE: f64 = 8.75;

struct Wall {
    honey: bool,
}

impl ChunkGenerator for Wall {
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
        if self.honey {
            for y in 64..=TOP {
                let at = BlockPos::new(WALL_X, y, WALL_Z);
                if at.chunk() == pos {
                    chunk.set_block(at, BlockKind::HoneyBlock.default_state());
                }
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(SIDE, START, 8.5)
    }
}

fn wall(honey: bool) -> Scene {
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
        Arc::new(Wall { honey }),
        StatusBoard::new(20),
    )
    .with_rng_seed(7);
    server.wait_for_spawn(Duration::from_secs(10));
    scene_on(server, &["Alice"], Vec::new())
}

fn drop_beside_the_wall(s: &mut Scene) {
    let alice = s.players[0].clone();
    s.tick(30);
    s.players[0].link.take();
    let mut y = START;
    while y > 64.5 {
        y -= 0.5;
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::Move {
                position: Some(Vec3::new(SIDE, y, 8.5)),
                rotation: None,
                on_ground: false,
            },
        );
        s.tick(1);
    }
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: Some(Vec3::new(SIDE, 64.0, 8.5)),
            rotation: None,
            on_ground: true,
        },
    );
    s.tick(1);
}

#[test]
fn sliding_down_a_honey_wall_cancels_the_fall_damage() {
    let mut s = wall(true);
    drop_beside_the_wall(&mut s);
    assert_eq!(s.server.player_health(s.players[0].id), Some(20.0));
}

#[test]
fn the_same_drop_beside_plain_air_is_deadly() {
    let mut s = wall(false);
    drop_beside_the_wall(&mut s);
    assert!(
        s.server
            .player_health(s.players[0].id)
            .is_none_or(|h| h < 20.0)
    );
}

#[test]
fn sliding_down_honey_completes_honey_block_slide() {
    let mut s = wall(true);
    drop_beside_the_wall(&mut s);
    assert!(
        s.server
            .done_advancements(s.players[0].id)
            .contains(&"minecraft:adventure/honey_block_slide")
    );
}

#[test]
fn the_slide_shows_its_particles_to_the_player() {
    let mut s = wall(true);
    let alice = s.players[0].clone();
    s.tick(30);
    s.players[0].link.take();
    let mut y = START;
    let mut shown = false;
    while y > 70.0 && !shown {
        y -= 0.5;
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::Move {
                position: Some(Vec3::new(SIDE, y, 8.5)),
                rotation: None,
                on_ground: false,
            },
        );
        s.tick(1);
        shown = s.players[0].link.take().iter().any(|event| {
            matches!(
                event,
                ClientEvent::EntityStatus {
                    status: EntityStatus::Event(53),
                    ..
                }
            )
        });
    }
    assert!(shown);
}
