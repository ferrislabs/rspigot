mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{DigAction, Direction, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::{InventoryOrder, ItemSpec, Order};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Meadow {
    roof: bool,
}

impl ChunkGenerator for Meadow {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                let (bx, bz) = (pos.min_block_x() + x, pos.min_block_z() + z);
                for y in 60..=63 {
                    chunk.set_block(BlockPos::new(bx, y, bz), BlockKind::Stone.default_state());
                }
                if self.roof {
                    chunk.set_block(BlockPos::new(bx, 70, bz), BlockKind::Stone.default_state());
                }
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn meadow(roof: bool) -> Scene {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        thunder_chance: 0,
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
        Arc::new(Meadow { roof }),
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

fn arm(s: &mut Scene, enchantments: Vec<(String, i32)>) {
    let alice = s.players[0].id;
    s.act(Order::Inventory {
        player: alice,
        order: InventoryOrder::Set {
            slot: 0,
            item: Some(ItemSpec {
                enchantments,
                ..ItemSpec::of("minecraft:trident", 1)
            }),
        },
    });
}

fn throw_at_a_villager(s: &mut Scene) -> bool {
    console(
        s,
        "summon minecraft:villager 8.5 64 11.5 {NoAI:1b,Silent:1b,PersistenceRequired:1b}",
    );
    s.tick(2);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: Rotation::new(0.0, 0.0),
        },
    );
    s.tick(12);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Dig {
            action: DigAction::ReleaseUseItem,
            pos: BlockPos::new(0, 0, 0),
            face: Direction::Down,
            sequence: 0,
        },
    );
    let mut struck = false;
    for _ in 0..12 {
        s.tick(1);
        struck |= !s.server.entities_of(Kind::LightningBolt).is_empty();
    }
    struck
}

fn channeling() -> Vec<(String, i32)> {
    vec![("minecraft:channeling".to_owned(), 1)]
}

#[test]
fn a_channeling_trident_calls_lightning_on_a_mob_in_a_thunderstorm() {
    let mut s = meadow(false);
    console(&mut s, "weather thunder");
    arm(&mut s, channeling());
    assert!(throw_at_a_villager(&mut s));
}

#[test]
fn the_lightning_it_calls_completes_very_very_frightening_for_the_thrower() {
    let mut s = meadow(false);
    console(&mut s, "weather thunder");
    arm(&mut s, channeling());
    throw_at_a_villager(&mut s);
    assert!(
        s.server
            .done_advancements(s.players[0].id)
            .contains(&"minecraft:adventure/very_very_frightening")
    );
}

#[test]
fn clear_weather_calls_no_lightning() {
    let mut s = meadow(false);
    arm(&mut s, channeling());
    assert!(!throw_at_a_villager(&mut s));
}

#[test]
fn a_trident_without_channeling_calls_no_lightning() {
    let mut s = meadow(false);
    console(&mut s, "weather thunder");
    arm(&mut s, Vec::new());
    assert!(!throw_at_a_villager(&mut s));
}

#[test]
fn a_mob_under_a_roof_is_not_struck() {
    let mut s = meadow(true);
    console(&mut s, "weather thunder");
    arm(&mut s, channeling());
    assert!(!throw_at_a_villager(&mut s));
}
