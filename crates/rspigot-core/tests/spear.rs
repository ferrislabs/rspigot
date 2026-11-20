mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::{Answer, InventoryOrder, ItemSpec, Order, Question};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, EntityId, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;

const AIM: Rotation = Rotation::new(0.0, 0.0);

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
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(9, 64, 20));
    s
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

fn spear(item: &str, enchantments: &[(&str, i32)]) -> ItemSpec {
    let mut spec = ItemSpec::of(item, 1);
    spec.enchantments = enchantments
        .iter()
        .map(|(name, level)| ((*name).to_owned(), *level))
        .collect();
    spec
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn husk(s: &mut Scene, x: f64, z: f64) -> EntityId {
    let before = s.server.entities_of(Kind::Husk).len();
    console(
        s,
        &format!("summon minecraft:husk {x} 64 {z} {{NoAI:1b,PersistenceRequired:1b}}"),
    );
    let all = s.server.entities_of(Kind::Husk);
    assert_eq!(all.len(), before + 1);
    all.last().unwrap().id
}

fn health(s: &mut Scene, id: EntityId) -> f32 {
    match s.ask(Question::Entity(id)) {
        Answer::Entity(details) => details.health.unwrap_or(0.0),
        _ => 0.0,
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

fn ready(s: &mut Scene) {
    s.tick(30);
}

#[test]
fn a_jab_hurts_a_mob_inside_the_reach_window() {
    let mut s = meadow();
    give(&mut s, 0, spear("minecraft:iron_spear", &[]));
    let target = husk(&mut s, 8.5, 11.5);
    ready(&mut s);
    s.players[0].link.take();

    assert!(s.server.spear_stab(s.players[0].id));

    let left = health(&mut s, target);
    assert!((left - 17.0).abs() < 0.1, "{left}");
    let heard = sounds(&s.players[0].link.take());
    assert!(heard.contains(&"minecraft:item.spear.attack"));
    assert!(heard.contains(&"minecraft:item.spear.hit"));
}

#[test]
fn a_wooden_spear_makes_the_wooden_sounds() {
    let mut s = meadow();
    give(&mut s, 0, spear("minecraft:wooden_spear", &[]));
    husk(&mut s, 8.5, 11.5);
    ready(&mut s);
    s.players[0].link.take();

    assert!(s.server.spear_stab(s.players[0].id));
    s.server.deliver();

    let heard = sounds(&s.players[0].link.take());
    assert!(
        heard.contains(&"minecraft:item.spear_wood.attack"),
        "{heard:?}"
    );
}

#[test]
fn a_jab_misses_a_mob_closer_than_the_minimum_reach() {
    let mut s = meadow();
    give(&mut s, 0, spear("minecraft:iron_spear", &[]));
    let target = husk(&mut s, 8.5, 9.5);
    ready(&mut s);

    assert!(s.server.spear_stab(s.players[0].id));

    assert_eq!(health(&mut s, target), 20.0);
}

#[test]
fn a_jab_misses_a_mob_beyond_the_maximum_reach() {
    let mut s = meadow();
    give(&mut s, 0, spear("minecraft:iron_spear", &[]));
    let target = husk(&mut s, 8.5, 15.5);
    ready(&mut s);

    assert!(s.server.spear_stab(s.players[0].id));

    assert_eq!(health(&mut s, target), 20.0);
}

#[test]
fn a_second_jab_needs_the_attack_charge_back() {
    let mut s = meadow();
    give(&mut s, 0, spear("minecraft:iron_spear", &[]));
    husk(&mut s, 8.5, 11.5);
    ready(&mut s);
    let alice = s.players[0].id;

    assert!(s.server.spear_stab(alice));
    assert!(!s.server.spear_stab(alice));
}

#[test]
fn only_a_spear_can_jab() {
    let mut s = meadow();
    give(&mut s, 0, ItemSpec::of("minecraft:iron_sword", 1));
    ready(&mut s);
    assert!(!s.server.spear_stab(s.players[0].id));
}

#[test]
fn lunge_pushes_the_jabber_and_sounds() {
    let mut s = meadow();
    give(
        &mut s,
        0,
        spear("minecraft:iron_spear", &[("minecraft:lunge", 2)]),
    );
    ready(&mut s);
    s.players[0].link.take();

    assert!(s.server.spear_stab(s.players[0].id));
    s.server.deliver();

    let heard = sounds(&s.players[0].link.take());
    assert!(
        heard
            .iter()
            .any(|sound| sound.starts_with("minecraft:item.spear.lunge_")),
        "{heard:?}"
    );
}

fn run_at(s: &mut Scene, ticks: u32, step: f64) {
    let alice = s.players[0].clone();
    for index in 0..ticks {
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::Move {
                position: Some(Vec3::new(8.5, 64.0, 8.5 + step * f64::from(index + 1))),
                rotation: Some(AIM),
                on_ground: true,
            },
        );
        s.tick(1);
    }
}

fn charge(s: &mut Scene) {
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

#[test]
fn a_charging_spear_hits_for_speed_scaled_damage() {
    let mut s = meadow();
    give(&mut s, 0, spear("minecraft:iron_spear", &[]));
    let target = husk(&mut s, 8.5, 15.0);
    ready(&mut s);
    charge(&mut s);

    run_at(&mut s, 16, 0.3);

    let left = health(&mut s, target);
    assert!(left < 19.0, "{left}");
}

#[test]
fn a_charging_spear_that_stands_still_hurts_nobody() {
    let mut s = meadow();
    give(&mut s, 0, spear("minecraft:iron_spear", &[]));
    let target = husk(&mut s, 8.5, 11.5);
    ready(&mut s);
    charge(&mut s);

    s.tick(30);

    assert_eq!(health(&mut s, target), 20.0);
}

#[test]
fn stabbing_five_mobs_in_one_charge_completes_spear_many_mobs() {
    let mut s = meadow();
    give(&mut s, 0, spear("minecraft:iron_spear", &[]));
    for offset in [-0.2, -0.1, 0.0, 0.1, 0.2] {
        husk(&mut s, 8.5 + offset, 15.0);
    }
    ready(&mut s);
    charge(&mut s);

    run_at(&mut s, 16, 0.3);

    assert!(
        s.server
            .done_advancements(s.players[0].id)
            .contains(&"minecraft:adventure/spear_many_mobs")
    );
}
