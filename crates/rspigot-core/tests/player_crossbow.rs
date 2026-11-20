mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{
    ClientEvent, DigAction, Direction, Hand, Inbound, MetadataValue, PlayerInput,
};
use rspigot_core::plugins::{Answer, InventoryOrder, ItemSpec, Order, Question};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;

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

fn crossbow(enchantments: &[(&str, i32)]) -> ItemSpec {
    let mut spec = ItemSpec::of("minecraft:crossbow", 1);
    spec.enchantments = enchantments
        .iter()
        .map(|(name, level)| ((*name).to_owned(), *level))
        .collect();
    spec
}

fn press_use(s: &mut Scene) {
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

fn count_of(s: &mut Scene, key: &str) -> i32 {
    let player = s.players[0].id;
    match s.ask(Question::Inventory(player)) {
        Answer::Inventory(state) => state
            .slots
            .iter()
            .flatten()
            .filter(|stack| stack.key == key)
            .map(|stack| stack.count)
            .sum(),
        _ => 0,
    }
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

fn load(s: &mut Scene, hold_ticks: u32) {
    press_use(s);
    s.tick(hold_ticks);
    release(s);
}

#[test]
fn a_crossbow_loads_after_its_charge_and_spends_one_arrow() {
    let mut s = meadow();
    give(&mut s, 0, crossbow(&[]));
    give(&mut s, 9, ItemSpec::of("minecraft:arrow", 3));
    s.players[0].link.take();

    load(&mut s, 26);

    assert_eq!(count_of(&mut s, "minecraft:arrow"), 2);
    assert!(s.server.arrows().is_empty());
    let heard = sounds(&s.players[0].link.take());
    assert!(heard.contains(&"minecraft:item.crossbow.loading_start"));
    assert!(heard.contains(&"minecraft:item.crossbow.loading_middle"));
    assert!(heard.contains(&"minecraft:item.crossbow.loading_end"));
}

#[test]
fn an_early_release_loads_nothing() {
    let mut s = meadow();
    give(&mut s, 0, crossbow(&[]));
    give(&mut s, 9, ItemSpec::of("minecraft:arrow", 3));

    load(&mut s, 10);
    press_use(&mut s);

    assert_eq!(count_of(&mut s, "minecraft:arrow"), 3);
    assert!(s.server.arrows().is_empty());
}

#[test]
fn quick_charge_shortens_the_load() {
    let mut s = meadow();
    give(&mut s, 0, crossbow(&[("minecraft:quick_charge", 3)]));
    give(&mut s, 9, ItemSpec::of("minecraft:arrow", 3));

    load(&mut s, 11);

    assert_eq!(count_of(&mut s, "minecraft:arrow"), 2);
}

#[test]
fn a_loaded_crossbow_shoots_on_the_next_use_and_completes_ol_betsy() {
    let mut s = meadow();
    give(&mut s, 0, crossbow(&[("minecraft:quick_charge", 3)]));
    give(&mut s, 9, ItemSpec::of("minecraft:arrow", 3));
    load(&mut s, 11);

    press_use(&mut s);

    assert_eq!(s.server.arrows().len(), 1);
    assert_eq!(count_of(&mut s, "minecraft:arrow"), 2);
    assert!(
        s.server
            .done_advancements(s.players[0].id)
            .contains(&"minecraft:adventure/ol_betsy")
    );
    press_use(&mut s);
    assert_eq!(s.server.arrows().len(), 1);
}

#[test]
fn the_crossbow_keeps_its_arrow_in_the_saved_inventory() {
    let mut s = meadow();
    give(&mut s, 0, crossbow(&[("minecraft:quick_charge", 3)]));
    give(&mut s, 9, ItemSpec::of("minecraft:arrow", 3));
    load(&mut s, 11);

    let player = s.players[0].id;
    let held = match s.ask(Question::Inventory(player)) {
        Answer::Inventory(state) => state.slots[0].clone().unwrap(),
        _ => panic!("inventory"),
    };
    assert_eq!(held.key, "minecraft:crossbow");
    assert!(!held.components.is_empty());
}

#[test]
fn multishot_fires_three_arrows_for_one() {
    let mut s = meadow();
    give(
        &mut s,
        0,
        crossbow(&[("minecraft:quick_charge", 3), ("minecraft:multishot", 1)]),
    );
    give(&mut s, 9, ItemSpec::of("minecraft:arrow", 3));
    load(&mut s, 11);
    assert_eq!(count_of(&mut s, "minecraft:arrow"), 2);

    press_use(&mut s);

    assert_eq!(s.server.arrows().len(), 3);
    assert_eq!(count_of(&mut s, "minecraft:arrow"), 2);
}

#[test]
fn a_firework_loaded_in_the_crossbow_flies_as_a_rocket() {
    let mut s = meadow();
    give(&mut s, 0, crossbow(&[("minecraft:quick_charge", 3)]));
    give(&mut s, 9, ItemSpec::of("minecraft:firework_rocket", 2));
    load(&mut s, 11);
    assert_eq!(count_of(&mut s, "minecraft:firework_rocket"), 1);

    press_use(&mut s);

    assert_eq!(s.server.entities_of(Kind::FireworkRocket).len(), 1);
    assert!(s.server.arrows().is_empty());
}

#[test]
fn a_piercing_crossbow_wears_one_point_per_arrow_shot() {
    let mut s = meadow();
    give(
        &mut s,
        0,
        crossbow(&[("minecraft:quick_charge", 3), ("minecraft:piercing", 2)]),
    );
    give(&mut s, 9, ItemSpec::of("minecraft:arrow", 3));
    load(&mut s, 11);
    press_use(&mut s);
    assert_eq!(s.server.arrows().len(), 1);
    assert_eq!(s.server.held_damage(s.players[0].id), Some(1));
}

#[test]
fn charging_shows_the_using_flag_to_everyone_and_releasing_clears_it() {
    let mut s = meadow();
    give(&mut s, 0, crossbow(&[]));
    give(&mut s, 9, ItemSpec::of("minecraft:arrow", 3));
    s.players[0].link.take();

    press_use(&mut s);
    s.tick(2);
    let events = s.players[0].link.take();
    let flags: Vec<i8> = events
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
        .collect();
    assert_eq!(flags, vec![1]);

    release(&mut s);
    let cleared = s.players[0].link.take().into_iter().any(|event| {
        matches!(&event, ClientEvent::EntityMetadata { entries, .. }
            if entries.iter().any(|e| e.index == USING_FLAGS && matches!(e.value, MetadataValue::Byte(0))))
    });
    assert!(cleared);
}

#[test]
fn a_crossbow_without_ammunition_does_not_start_charging() {
    let mut s = meadow();
    give(&mut s, 0, crossbow(&[]));
    s.players[0].link.take();

    press_use(&mut s);
    s.tick(2);

    let shown = s.players[0].link.take().iter().any(|event| {
        matches!(event, ClientEvent::EntityMetadata { entries, .. }
            if entries.iter().any(|e| e.index == USING_FLAGS))
    });
    assert!(!shown);
}

#[test]
fn creative_loads_for_free_and_the_arrow_cannot_be_picked_up_in_survival() {
    let mut s = meadow();
    s.server
        .handle(Inbound::Console("gamemode creative Alice".to_owned()));
    s.server.deliver();
    give(&mut s, 0, crossbow(&[("minecraft:quick_charge", 3)]));

    load(&mut s, 11);
    press_use(&mut s);

    assert_eq!(s.server.arrows().len(), 1);
}
