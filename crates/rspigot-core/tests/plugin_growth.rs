mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::plugins::events::BlockIgniteCause;
use rspigot_core::plugins::{
    Answer, BlockOrder, EventKind, EventPayload, Order, Question, WorldOrder,
};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Vec3};

const FIRE: BlockPos = BlockPos::new(3, 64, 3);
const PLANKS: BlockPos = BlockPos::new(4, 64, 3);

struct Meadow;

impl ChunkGenerator for Meadow {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let plains = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(plains));
        for x in 0..16 {
            for z in 0..16 {
                for y in 60..=63 {
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        BlockStateId(1),
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

fn growing_server(seed: u64, adjust: impl FnOnce(&mut GameConfig)) -> Server<Link> {
    let mut config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    adjust(&mut config);
    let data = LevelData {
        spawn_mobs: false,
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
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn growing(seed: u64, handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> Scene {
    scene_on(growing_server(seed, |_| {}), &["Alice"], handlers)
}

fn set(s: &mut Scene, pos: BlockPos, key: &str) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::Set(key.to_owned()),
    });
}

fn kind_at(s: &mut Scene, pos: BlockPos) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state.kind,
        other => panic!("expected a block, got {other:?}"),
    }
}

fn rain(s: &mut Scene) {
    s.act(Order::World(WorldOrder::SetWeather(
        rspigot_core::commands::Weather::Rain,
    )));
}

fn wait_for(s: &mut Scene, kind: EventKind, limit: u32) -> Vec<Fired> {
    for _ in 0..limit {
        let heard = settled_for(&s.plugin, kind);
        if !heard.is_empty() {
            return heard;
        }
        s.tick(1);
    }
    Vec::new()
}

fn decide_until(
    s: &mut Scene,
    kind: EventKind,
    limit: u32,
    cancel: bool,
    done: impl Fn(&Fired) -> bool,
) -> Option<Fired> {
    let mut answered: Vec<u64> = Vec::new();
    for _ in 0..limit {
        for asked in asked_for(&s.plugin, kind) {
            if answered.contains(&asked.id) {
                continue;
            }
            answered.push(asked.id);
            let wanted = done(&asked);
            rule_on(&mut s.server, 1, asked.id, cancel && wanted, None);
            if wanted {
                return Some(asked);
            }
        }
        s.tick(1);
    }
    None
}

fn burning_planks(s: &mut Scene) {
    set(s, PLANKS, "minecraft:oak_planks");
    set(s, FIRE, "minecraft:fire");
}

#[test]
fn a_fire_beside_planks_asks_block_burn_with_the_fire_as_igniter() {
    let mut s = growing(1, vec![observe(1, EventKind::BlockBurn)]);
    burning_planks(&mut s);
    let heard = wait_for(&mut s, EventKind::BlockBurn, 4_000);
    let burn = heard
        .iter()
        .find_map(|fired| match &fired.payload {
            EventPayload::BlockBurn(event) if event.block == PLANKS => Some(event.clone()),
            _ => None,
        })
        .expect("the planks burn");
    assert_eq!(burn.igniting_block, Some(FIRE));
}

#[test]
fn a_cancelled_block_burn_keeps_the_planks() {
    let mut s = growing(1, vec![deciding(1, EventKind::BlockBurn)]);
    burning_planks(&mut s);
    let asked = decide_until(
        &mut s,
        EventKind::BlockBurn,
        4_000,
        true,
        |fired| matches!(&fired.payload, EventPayload::BlockBurn(event) if event.block == PLANKS),
    );
    assert!(asked.is_some());
    s.tick(2);
    assert_eq!(kind_at(&mut s, PLANKS), "minecraft:oak_planks");
}

#[test]
fn burning_planks_without_a_handler_turn_to_fire_or_air() {
    let mut s = growing(1, Vec::new());
    burning_planks(&mut s);
    for _ in 0..4_000 {
        s.tick(1);
        if kind_at(&mut s, PLANKS) != "minecraft:oak_planks" {
            break;
        }
    }
    let after = kind_at(&mut s, PLANKS);
    assert!(
        after == "minecraft:fire" || after == "minecraft:air",
        "{after}"
    );
}

#[test]
fn a_fire_hanging_in_the_air_fades_and_a_cancel_keeps_it() {
    let hanging = BlockPos::new(8, 70, 8);
    let mut s = growing(2, vec![deciding(1, EventKind::BlockFade)]);
    set(&mut s, hanging, "minecraft:fire");
    let asked = decide_until(&mut s, EventKind::BlockFade, 200, true, |_| true)
        .expect("the fire is asked about");
    match asked.payload {
        EventPayload::BlockFade(event) => {
            assert_eq!(event.block, hanging);
            assert_eq!(event.new_state.state.kind, "minecraft:air");
        }
        other => panic!("expected a fade, got {other:?}"),
    }
    s.tick(2);
    assert_eq!(kind_at(&mut s, hanging), "minecraft:fire");
}

#[test]
fn a_fire_hanging_in_the_air_dies_without_a_handler() {
    let hanging = BlockPos::new(8, 70, 8);
    let mut s = growing(2, Vec::new());
    set(&mut s, hanging, "minecraft:fire");
    s.tick(60);
    assert_eq!(kind_at(&mut s, hanging), "minecraft:air");
}

#[test]
fn a_spreading_fire_asks_ignite_then_spread_from_the_fire() {
    let mut s = growing(
        3,
        vec![
            observe(1, EventKind::BlockIgnite),
            observe(1, EventKind::BlockSpread),
        ],
    );
    burning_planks(&mut s);
    let spread = wait_for(&mut s, EventKind::BlockSpread, 4_000);
    assert!(!spread.is_empty());
    let ignite = settled_for(&s.plugin, EventKind::BlockIgnite);
    let first = ignite
        .iter()
        .find_map(|fired| match &fired.payload {
            EventPayload::BlockIgnite(event) => Some(event.clone()),
            _ => None,
        })
        .expect("an ignite was asked");
    assert_eq!(first.cause, BlockIgniteCause::Spread);
    assert_eq!(first.igniting_block, Some(FIRE));
    match &spread[0].payload {
        EventPayload::BlockSpread(event) => {
            assert_eq!(event.source, FIRE);
            assert_eq!(event.new_state.state.kind, "minecraft:fire");
        }
        other => panic!("expected a spread, got {other:?}"),
    }
}

#[test]
fn a_fire_spread_radius_of_zero_keeps_fire_from_spreading() {
    let mut s = scene_on(
        growing_server(3, |config| config.rules.fire_spread_radius = 0),
        &["Alice"],
        vec![
            observe(1, EventKind::BlockBurn),
            observe(1, EventKind::BlockSpread),
        ],
    );
    burning_planks(&mut s);
    s.tick(1_500);
    assert!(settled_for(&s.plugin, EventKind::BlockBurn).is_empty());
    assert!(settled_for(&s.plugin, EventKind::BlockSpread).is_empty());
    assert_eq!(kind_at(&mut s, PLANKS), "minecraft:oak_planks");
}

fn fades_within(s: &mut Scene, ticks: u32) -> bool {
    set(s, PLANKS, "minecraft:oak_planks");
    set(s, FIRE, "minecraft:fire");
    s.tick(ticks);
    !settled_for(&s.plugin, EventKind::BlockFade).is_empty()
}

#[test]
fn rain_puts_a_fire_out_with_a_fade() {
    let mut dry = growing(4, vec![observe(1, EventKind::BlockFade)]);
    assert!(!fades_within(&mut dry, 250));
    let mut wet = growing(4, vec![observe(1, EventKind::BlockFade)]);
    rain(&mut wet);
    assert!(fades_within(&mut wet, 250));
}
