mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_core::config::{GameConfig, LevelData, PluginSettings};
use rspigot_core::events::{DigAction, Direction, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::{
    Answer, EventKind, EventPayload, HandlerSpec, InventoryOrder, ItemSpec, Order, Question,
};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Lake;

impl ChunkGenerator for Lake {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                for y in 60..=61 {
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        BlockStateId(1),
                    );
                }
                let lake = pos.x == 0 && pos.z == 0 && z >= 4;
                for y in 62..=63 {
                    let state = if lake {
                        rspigot_blocks::BlockKind::Water.default_state()
                    } else {
                        BlockStateId(1)
                    };
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        state,
                    );
                }
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

fn plains_server(adjust: impl FnOnce(&mut GameConfig)) -> Server<Link> {
    let mut config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        plugins: PluginSettings {
            decision_ticks: 1_000,
            ..PluginSettings::default()
        },
        ..GameConfig::default()
    };
    adjust(&mut config);
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
        Arc::new(Lake),
        StatusBoard::new(20),
    )
    .with_rng_seed(7);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn ready(handlers: Vec<HandlerSpec>) -> Scene {
    ready_with(handlers, |_| {})
}

fn ready_with(handlers: Vec<HandlerSpec>, adjust: impl FnOnce(&mut GameConfig)) -> Scene {
    let mut s = scene_on(plains_server(adjust), &["Alice"], handlers);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn use_item(s: &mut Scene, pitch: f32) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: Rotation::new(0.0, pitch),
        },
    );
}

fn release(s: &mut Scene) {
    let alice = s.players[0].clone();
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
}

fn enchanted_trident(s: &mut Scene, enchantment: &str, level: i32) {
    let alice = s.players[0].id;
    s.act(Order::Inventory {
        player: alice,
        order: InventoryOrder::Set {
            slot: 0,
            item: Some(ItemSpec {
                enchantments: vec![(enchantment.to_owned(), level)],
                ..ItemSpec::of("minecraft:trident", 1)
            }),
        },
    });
}

fn first_slot(s: &mut Scene) -> Option<ItemSpec> {
    let alice = s.players[0].id;
    match s.ask(Question::Inventory(alice)) {
        Answer::Inventory(inventory) => inventory.slots[0].clone(),
        other => panic!("expected the inventory, got {other:?}"),
    }
}

fn throw(s: &mut Scene, charge: u32) {
    use_item(s, 80.0);
    s.tick(charge);
    release(s);
    s.tick(1);
}

#[test]
fn a_charged_trident_is_thrown_and_leaves_the_hand() {
    let mut s = ready(Vec::new());
    console(&mut s, "give Alice trident");
    throw(&mut s, 12);
    let tridents = s.server.entities_of(Kind::Trident);
    assert_eq!(tridents.len(), 1);
    assert!(tridents[0].velocity.length_sqr() > 1.0);
    assert_eq!(first_slot(&mut s), None);
}

#[test]
fn a_trident_released_too_early_stays_in_the_hand() {
    let mut s = ready(Vec::new());
    console(&mut s, "give Alice trident");
    throw(&mut s, 5);
    assert!(s.server.entities_of(Kind::Trident).is_empty());
    assert_eq!(
        first_slot(&mut s).map(|item| item.key).as_deref(),
        Some("minecraft:trident")
    );
}

#[test]
fn a_launch_a_plugin_cancels_keeps_the_trident() {
    let mut s = ready(vec![deciding(1, EventKind::ProjectileLaunch)]);
    console(&mut s, "give Alice trident");
    throw(&mut s, 12);
    let asked = last_asked(&s.plugin, EventKind::ProjectileLaunch);
    let EventPayload::ProjectileLaunch(event) = &asked.payload else {
        panic!("expected a launch");
    };
    assert_eq!(event.entity_type, "minecraft:trident");
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(1);
    assert!(s.server.entities_of(Kind::Trident).is_empty());
    assert_eq!(
        first_slot(&mut s).map(|item| item.key).as_deref(),
        Some("minecraft:trident")
    );
}

#[test]
fn a_loyal_trident_comes_back_to_the_thrower() {
    let mut s = ready(Vec::new());
    enchanted_trident(&mut s, "minecraft:loyalty", 3);
    throw(&mut s, 12);
    assert_eq!(s.server.entities_of(Kind::Trident).len(), 1);
    assert_eq!(first_slot(&mut s), None);
    s.tick(300);
    assert!(s.server.entities_of(Kind::Trident).is_empty());
    assert_eq!(
        first_slot(&mut s).map(|item| item.key).as_deref(),
        Some("minecraft:trident")
    );
}

#[test]
fn riptide_launches_the_player_in_water_and_asks_nothing_on_dry_land() {
    let mut s = ready(vec![observe(1, EventKind::PlayerRiptide)]);
    enchanted_trident(&mut s, "minecraft:riptide", 2);
    throw(&mut s, 12);
    assert!(settled_for(&s.plugin, EventKind::PlayerRiptide).is_empty());
    release(&mut s);
    console(&mut s, "tp Alice 0.5 63 8.5");
    s.tick(2);
    throw(&mut s, 12);
    let fired = settled_for(&s.plugin, EventKind::PlayerRiptide);
    assert_eq!(fired.len(), 1);
    let EventPayload::PlayerRiptide(event) = &fired[0].payload else {
        panic!("expected a riptide");
    };
    assert_eq!(event.item.key, "minecraft:trident");
    assert_eq!(event.player.name, "Alice");
    assert!(
        (event.velocity.length() - 2.25).abs() < 1e-3,
        "{:?}",
        event.velocity
    );
    assert!(s.server.entities_of(Kind::Trident).is_empty());
}

#[test]
fn a_spin_attack_hurts_the_mob_the_player_touches() {
    let mut s = ready(Vec::new());
    enchanted_trident(&mut s, "minecraft:riptide", 1);
    console(&mut s, "tp Alice 0.5 63 8.5");
    console(
        &mut s,
        "summon minecraft:cow 0.5 63 8.7 {NoAI:1b,Silent:1b}",
    );
    let before = s.server.entities_of(Kind::Cow)[0].health.unwrap();
    s.tick(2);
    throw(&mut s, 12);
    s.tick(3);
    let after = s.server.entities_of(Kind::Cow)[0].health;
    assert!(
        after.is_none_or(|health| health < before),
        "{before} {after:?}"
    );
}
