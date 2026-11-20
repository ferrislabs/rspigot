mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{
    ClientEvent, DigAction, Direction, Hand, Inbound, MetadataValue, PlayerInput,
};
use rspigot_core::plugins::{InventoryOrder, ItemSpec, Order};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, Rotation, Vec3};

const AIM: Rotation = Rotation::new(0.0, 0.0);
const USING_FLAGS: u8 = 8;

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
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
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
        day_time: 6_000,
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
    scene_on(server, &["Alice"], Vec::new())
}

fn give(s: &mut Scene, slot: i32, spec: ItemSpec) {
    let player = s.players[0].id;
    s.act(Order::Inventory {
        player,
        order: InventoryOrder::Set {
            slot,
            item: Some(spec),
        },
    });
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
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

fn raise(s: &mut Scene) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: AIM,
        },
    );
}

fn lower(s: &mut Scene) {
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

fn sounds(events: &[ClientEvent]) -> Vec<&'static str> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Sound { sound, .. } => Some(*sound),
            _ => None,
        })
        .collect()
}

fn flags(events: &[ClientEvent]) -> Vec<i8> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::EntityMetadata { entries, .. } => entries
                .iter()
                .find(|entry| entry.index == USING_FLAGS)
                .and_then(|entry| match entry.value {
                    MetadataValue::Byte(flags) => Some(flags),
                    _ => None,
                }),
            _ => None,
        })
        .collect()
}

#[test]
fn raising_the_spyglass_sets_the_using_status_and_plays_the_use_sound() {
    let mut s = meadow();
    give(&mut s, 0, ItemSpec::of("minecraft:spyglass", 1));
    s.players[0].link.take();

    raise(&mut s);

    let events = s.players[0].link.take();
    assert_eq!(flags(&events), vec![1]);
    assert!(sounds(&events).contains(&"minecraft:item.spyglass.use"));
}

#[test]
fn lowering_the_spyglass_clears_the_status_and_plays_the_stop_sound() {
    let mut s = meadow();
    give(&mut s, 0, ItemSpec::of("minecraft:spyglass", 1));
    raise(&mut s);
    s.tick(3);
    s.players[0].link.take();

    lower(&mut s);

    let events = s.players[0].link.take();
    assert_eq!(flags(&events), vec![0]);
    assert!(sounds(&events).contains(&"minecraft:item.spyglass.stop_using"));
}

#[test]
fn the_offhand_spyglass_sets_the_offhand_flag() {
    let mut s = meadow();
    give(&mut s, 40, ItemSpec::of("minecraft:spyglass", 1));
    s.players[0].link.take();
    let alice = s.players[0].clone();

    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Off,
            sequence: 1,
            rotation: AIM,
        },
    );

    assert_eq!(flags(&s.players[0].link.take()), vec![3]);
}

#[test]
fn looking_at_a_parrot_through_the_spyglass_completes_the_advancement() {
    let mut s = meadow();
    give(&mut s, 0, ItemSpec::of("minecraft:spyglass", 1));
    console(
        &mut s,
        "summon minecraft:parrot 8.5 64 12.5 {NoAI:1b,PersistenceRequired:1b}",
    );
    face(&mut s, Rotation::new(0.0, 15.6));
    raise(&mut s);
    s.tick(3);

    assert!(
        s.server
            .done_advancements(s.players[0].id)
            .contains(&"minecraft:adventure/spyglass_at_parrot")
    );
}

#[test]
fn looking_elsewhere_through_the_spyglass_completes_nothing() {
    let mut s = meadow();
    give(&mut s, 0, ItemSpec::of("minecraft:spyglass", 1));
    console(
        &mut s,
        "summon minecraft:parrot 8.5 64 12.5 {NoAI:1b,PersistenceRequired:1b}",
    );
    face(&mut s, Rotation::new(90.0, 0.0));
    raise(&mut s);
    s.tick(3);

    assert!(
        !s.server
            .done_advancements(s.players[0].id)
            .contains(&"minecraft:adventure/spyglass_at_parrot")
    );
}

#[test]
fn the_spyglass_lowers_itself_after_a_minute() {
    let mut s = meadow();
    give(&mut s, 0, ItemSpec::of("minecraft:spyglass", 1));
    raise(&mut s);
    s.players[0].link.take();

    s.tick(1_201);

    assert_eq!(flags(&s.players[0].link.take()), vec![0]);
}
