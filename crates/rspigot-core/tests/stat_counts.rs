mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, Direction, Hand, Inbound, PlayerInput};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, Vec3};

const CAKE: BlockPos = BlockPos::new(9, 64, 8);
const POT: BlockPos = BlockPos::new(9, 64, 10);
const CUSTOM: i32 = 8;

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
        if CAKE.chunk() == pos {
            chunk.set_block(CAKE, BlockKind::Cake.default_state());
            chunk.set_block(POT, BlockKind::FlowerPot.default_state());
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 9.5)
    }
}

fn meadow() -> Scene {
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
    let mut s = scene_on(server, &["Alice"], Vec::new());
    s.tick(30);
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn stat(s: &mut Scene, name: &str) -> i32 {
    let id = rspigot_data::registries::static_index("minecraft:custom_stat", name)
        .expect("a registered statistic") as i32;
    let alice = s.players[0].clone();
    alice.link.take();
    send_input(&mut s.server, &alice, PlayerInput::RequestStats);
    alice
        .link
        .take()
        .into_iter()
        .find_map(|event| match event {
            ClientEvent::Stats(entries) => Some(entries),
            _ => None,
        })
        .expect("the stats were sent")
        .iter()
        .find(|(kind, value, _)| (*kind, *value) == (CUSTOM, id))
        .map_or(0, |entry| entry.2)
}

fn use_on(s: &mut Scene, pos: BlockPos) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 1,
        },
    );
    s.tick(1);
}

fn bites(s: &Scene) -> Option<String> {
    s.server
        .block_state_at(CAKE)
        .filter(|state| state.kind() == Some(BlockKind::Cake))
        .and_then(|state| state.property("bites").map(str::to_owned))
}

#[test]
fn eating_a_slice_of_cake_counts_eat_cake_slice_and_takes_a_bite() {
    let mut s = meadow();
    console(&mut s, "gamemode creative Alice");
    s.tick(1);
    use_on(&mut s, CAKE);
    assert_eq!(stat(&mut s, "minecraft:eat_cake_slice"), 1);
    assert_eq!(bites(&s).as_deref(), Some("1"));
    use_on(&mut s, CAKE);
    assert_eq!(stat(&mut s, "minecraft:eat_cake_slice"), 2);
}

#[test]
fn the_seventh_slice_finishes_the_cake() {
    let mut s = meadow();
    console(&mut s, "gamemode creative Alice");
    s.tick(1);
    for _ in 0..7 {
        use_on(&mut s, CAKE);
    }
    assert_eq!(stat(&mut s, "minecraft:eat_cake_slice"), 7);
    assert_eq!(bites(&s), None);
}

#[test]
fn a_full_player_in_survival_does_not_eat_the_cake() {
    let mut s = meadow();
    use_on(&mut s, CAKE);
    assert_eq!(stat(&mut s, "minecraft:eat_cake_slice"), 0);
    assert_eq!(bites(&s).as_deref(), Some("0"));
}

#[test]
fn planting_a_flower_in_a_pot_counts_pot_flower_and_fills_the_pot() {
    let mut s = meadow();
    console(&mut s, "give Alice minecraft:dandelion 2");
    s.tick(1);
    use_on(&mut s, POT);
    assert_eq!(stat(&mut s, "minecraft:pot_flower"), 1);
    assert_eq!(
        s.server.block_state_at(POT).and_then(|state| state.kind()),
        Some(BlockKind::PottedDandelion)
    );
}

#[test]
fn a_stone_in_the_hand_does_not_fill_the_pot() {
    let mut s = meadow();
    console(&mut s, "give Alice minecraft:cobblestone 1");
    s.tick(1);
    use_on(&mut s, POT);
    assert_eq!(stat(&mut s, "minecraft:pot_flower"), 0);
    assert_eq!(
        s.server.block_state_at(POT).and_then(|state| state.kind()),
        Some(BlockKind::FlowerPot)
    );
}
