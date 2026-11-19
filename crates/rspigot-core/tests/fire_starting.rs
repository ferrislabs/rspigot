mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, Direction, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::events::BlockIgniteCause;
use rspigot_core::plugins::{Answer, BlockOrder, EventKind, EventPayload, Order, Question};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Rotation, Vec3};
use rspigot_nbt::read_network_compound;

const SPOT: BlockPos = BlockPos::new(10, 64, 10);
const GRASS: BlockPos = BlockPos::new(10, 63, 10);

struct Meadow;

impl ChunkGenerator for Meadow {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                let at = |y: i32| BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z);
                for y in 60..=62 {
                    chunk.set_block(at(y), BlockStateId(1));
                }
                chunk.set_block(at(63), BlockKind::GrassBlock.default_state());
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn meadow(handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> Scene {
    let config = GameConfig {
        plugins: rspigot_core::config::PluginSettings {
            decision_ticks: 1_000,
            ..rspigot_core::config::PluginSettings::default()
        },
        view_distance: 5,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
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
    .with_rng_seed(3);
    server.wait_for_spawn(Duration::from_secs(10));
    let mut s = scene_on(server, &["Alice"], handlers);
    stream_chunk_of(&mut s, BlockPos::new(8, 64, 8));
    s.tick(20);
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn set(s: &mut Scene, pos: BlockPos, kind: &str, properties: &[(&str, &str)]) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::SetWith {
            kind: kind.to_owned(),
            properties: properties
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
        },
    });
}

fn block(s: &mut Scene, pos: BlockPos) -> (String, Vec<(String, String)>) {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => (state.kind, state.properties),
        other => panic!("expected a block, got {other:?}"),
    }
}

fn kind_at(s: &mut Scene, pos: BlockPos) -> String {
    block(s, pos).0
}

fn property(s: &mut Scene, pos: BlockPos, name: &str) -> Option<String> {
    block(s, pos)
        .1
        .into_iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
}

fn use_on(s: &mut Scene, pos: BlockPos, face: Direction, y: f64) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos,
            face,
            cursor: Vec3::new(0.5, y, 0.5),
            inside_block: false,
            sequence: 9,
        },
    );
    s.tick(3);
}

fn inventory(s: &mut Scene) -> Vec<rspigot_core::plugins::ItemSpec> {
    let alice = s.players[0].clone();
    let Answer::Inventory(state) = s.ask(Question::Inventory(alice.id)) else {
        panic!("an inventory");
    };
    state.slots.into_iter().flatten().collect()
}

fn damage_of(s: &mut Scene, key: &str) -> i32 {
    let item = inventory(s)
        .into_iter()
        .find(|item| item.key == key)
        .unwrap_or_else(|| panic!("{key} in the inventory"));
    if item.components.is_empty() {
        return 0;
    }
    let mut rest = item.components.as_slice();
    read_network_compound(&mut rest)
        .expect("components decode")
        .and_then(|components| components.get_i32("minecraft:damage"))
        .unwrap_or(0)
}

fn count_of(s: &mut Scene, key: &str) -> i32 {
    inventory(s)
        .into_iter()
        .filter(|item| item.key == key)
        .map(|item| item.count)
        .sum()
}

fn heard(s: &Scene, sound: &str) -> bool {
    s.players[0]
        .link
        .take()
        .iter()
        .any(|event| matches!(event, ClientEvent::Sound { sound: heard, .. } if *heard == sound))
}

fn campfire(s: &mut Scene, pos: BlockPos, lit: &str, waterlogged: &str) {
    set(
        s,
        pos,
        "minecraft:campfire",
        &[("lit", lit), ("waterlogged", waterlogged)],
    );
}

#[test]
fn flint_and_steel_lights_an_unlit_campfire_and_loses_one_durability() {
    let mut s = meadow(Vec::new());
    campfire(&mut s, SPOT, "false", "false");
    console(&mut s, "give Alice minecraft:flint_and_steel");
    use_on(&mut s, SPOT, Direction::Up, 1.0);
    assert_eq!(property(&mut s, SPOT, "lit").as_deref(), Some("true"));
    assert_eq!(damage_of(&mut s, "minecraft:flint_and_steel"), 1);
}

#[test]
fn flint_and_steel_does_not_wear_in_creative() {
    let mut s = meadow(Vec::new());
    console(&mut s, "gamemode creative Alice");
    campfire(&mut s, SPOT, "false", "false");
    console(&mut s, "give Alice minecraft:flint_and_steel");
    use_on(&mut s, SPOT, Direction::Up, 1.0);
    assert_eq!(property(&mut s, SPOT, "lit").as_deref(), Some("true"));
    assert_eq!(damage_of(&mut s, "minecraft:flint_and_steel"), 0);
}

#[test]
fn flint_and_steel_cannot_light_a_waterlogged_campfire() {
    let mut s = meadow(Vec::new());
    campfire(&mut s, SPOT, "false", "true");
    console(&mut s, "give Alice minecraft:flint_and_steel");
    use_on(&mut s, SPOT, Direction::Up, 1.0);
    assert_eq!(property(&mut s, SPOT, "lit").as_deref(), Some("false"));
    assert_eq!(kind_at(&mut s, SPOT.offset(0, 1, 0)), "minecraft:air");
    assert_eq!(damage_of(&mut s, "minecraft:flint_and_steel"), 0);
}

#[test]
fn flint_and_steel_lights_a_candle() {
    let mut s = meadow(Vec::new());
    set(
        &mut s,
        SPOT,
        "minecraft:red_candle",
        &[("candles", "3"), ("lit", "false")],
    );
    console(&mut s, "give Alice minecraft:flint_and_steel");
    use_on(&mut s, SPOT, Direction::Up, 1.0);
    assert_eq!(property(&mut s, SPOT, "lit").as_deref(), Some("true"));
    assert_eq!(property(&mut s, SPOT, "candles").as_deref(), Some("3"));
    assert_eq!(damage_of(&mut s, "minecraft:flint_and_steel"), 1);
}

#[test]
fn flint_and_steel_lights_a_candle_cake() {
    let mut s = meadow(Vec::new());
    set(
        &mut s,
        SPOT,
        "minecraft:white_candle_cake",
        &[("lit", "false")],
    );
    console(&mut s, "give Alice minecraft:flint_and_steel");
    use_on(&mut s, SPOT, Direction::Up, 1.0);
    assert_eq!(property(&mut s, SPOT, "lit").as_deref(), Some("true"));
}

#[test]
fn flint_and_steel_places_fire_on_the_face_of_a_grass_block() {
    let mut s = meadow(Vec::new());
    console(&mut s, "give Alice minecraft:flint_and_steel");
    use_on(&mut s, GRASS, Direction::Up, 1.0);
    assert_eq!(kind_at(&mut s, SPOT), "minecraft:fire");
    assert_eq!(damage_of(&mut s, "minecraft:flint_and_steel"), 1);
}

#[test]
fn flint_and_steel_does_not_place_fire_on_a_face_without_support() {
    let mut s = meadow(Vec::new());
    console(&mut s, "give Alice minecraft:flint_and_steel");
    use_on(&mut s, GRASS, Direction::Down, 0.0);
    assert_ne!(kind_at(&mut s, GRASS.offset(0, -1, 0)), "minecraft:fire");
    assert_eq!(damage_of(&mut s, "minecraft:flint_and_steel"), 0);
}

#[test]
fn a_fire_charge_places_fire_and_is_consumed() {
    let mut s = meadow(Vec::new());
    console(&mut s, "give Alice minecraft:fire_charge 2");
    use_on(&mut s, GRASS, Direction::Up, 1.0);
    assert_eq!(kind_at(&mut s, SPOT), "minecraft:fire");
    assert_eq!(count_of(&mut s, "minecraft:fire_charge"), 1);
}

#[test]
fn a_fire_charge_lights_a_campfire() {
    let mut s = meadow(Vec::new());
    campfire(&mut s, SPOT, "false", "false");
    console(&mut s, "give Alice minecraft:fire_charge");
    use_on(&mut s, SPOT, Direction::Up, 1.0);
    assert_eq!(property(&mut s, SPOT, "lit").as_deref(), Some("true"));
    assert_eq!(count_of(&mut s, "minecraft:fire_charge"), 0);
}

#[test]
fn a_shovel_puts_out_a_lit_campfire_and_loses_one_durability() {
    let mut s = meadow(Vec::new());
    campfire(&mut s, SPOT, "true", "false");
    console(&mut s, "give Alice minecraft:iron_shovel");
    use_on(&mut s, SPOT, Direction::Up, 1.0);
    assert_eq!(property(&mut s, SPOT, "lit").as_deref(), Some("false"));
    assert_eq!(damage_of(&mut s, "minecraft:iron_shovel"), 1);
}

#[test]
fn an_empty_hand_snuffs_a_lit_candle() {
    let mut s = meadow(Vec::new());
    set(
        &mut s,
        SPOT,
        "minecraft:candle",
        &[("candles", "2"), ("lit", "true")],
    );
    use_on(&mut s, SPOT, Direction::Up, 1.0);
    assert_eq!(property(&mut s, SPOT, "lit").as_deref(), Some("false"));
}

#[test]
fn an_empty_hand_snuffs_a_lit_candle_cake_hit_on_the_candle() {
    let mut s = meadow(Vec::new());
    set(&mut s, SPOT, "minecraft:candle_cake", &[("lit", "true")]);
    use_on(&mut s, SPOT, Direction::Up, 0.9);
    assert_eq!(property(&mut s, SPOT, "lit").as_deref(), Some("false"));
}

#[test]
fn an_empty_hand_leaves_an_unlit_candle_alone() {
    let mut s = meadow(Vec::new());
    set(&mut s, SPOT, "minecraft:candle", &[("lit", "false")]);
    use_on(&mut s, SPOT, Direction::Up, 1.0);
    assert_eq!(property(&mut s, SPOT, "lit").as_deref(), Some("false"));
}

#[test]
fn a_campfire_placed_in_water_is_waterlogged_and_unlit() {
    let mut s = meadow(Vec::new());
    set(&mut s, SPOT, "minecraft:water", &[("level", "0")]);
    console(&mut s, "give Alice minecraft:campfire");
    use_on(&mut s, GRASS, Direction::Up, 1.0);
    assert_eq!(kind_at(&mut s, SPOT), "minecraft:campfire");
    assert_eq!(
        property(&mut s, SPOT, "waterlogged").as_deref(),
        Some("true")
    );
    assert_eq!(property(&mut s, SPOT, "lit").as_deref(), Some("false"));
}

#[test]
fn a_campfire_placed_on_dry_land_is_lit() {
    let mut s = meadow(Vec::new());
    console(&mut s, "give Alice minecraft:campfire");
    use_on(&mut s, GRASS, Direction::Up, 1.0);
    assert_eq!(kind_at(&mut s, SPOT), "minecraft:campfire");
    assert_eq!(property(&mut s, SPOT, "lit").as_deref(), Some("true"));
}

#[test]
fn water_flowing_into_a_lit_campfire_puts_it_out() {
    let mut s = meadow(Vec::new());
    campfire(&mut s, SPOT, "true", "false");
    set(
        &mut s,
        SPOT.offset(1, 0, 0),
        "minecraft:water",
        &[("level", "0")],
    );
    s.tick(40);
    assert_eq!(property(&mut s, SPOT, "lit").as_deref(), Some("false"));
    assert_eq!(
        property(&mut s, SPOT, "waterlogged").as_deref(),
        Some("true")
    );
}

#[test]
fn water_poured_into_a_lit_campfire_waterlogs_and_puts_it_out() {
    let mut s = meadow(Vec::new());
    campfire(&mut s, SPOT, "true", "false");
    console(&mut s, "give Alice minecraft:water_bucket");
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 9,
            rotation: Rotation::new(-45.0, 22.0),
        },
    );
    s.tick(3);
    assert_eq!(property(&mut s, SPOT, "lit").as_deref(), Some("false"));
    assert_eq!(
        property(&mut s, SPOT, "waterlogged").as_deref(),
        Some("true")
    );
}

#[test]
fn a_burning_arrow_lights_an_unlit_campfire() {
    let mut s = meadow(Vec::new());
    campfire(&mut s, SPOT, "false", "false");
    console(
        &mut s,
        "summon minecraft:arrow 10.5 64.2 9.2 {Fire:200s,Motion:[0.0d,0.0d,1.0d]}",
    );
    s.tick(20);
    assert_eq!(property(&mut s, SPOT, "lit").as_deref(), Some("true"));
}

#[test]
fn a_plain_arrow_leaves_an_unlit_campfire_alone() {
    let mut s = meadow(Vec::new());
    campfire(&mut s, SPOT, "false", "false");
    console(
        &mut s,
        "summon minecraft:arrow 10.5 64.2 9.2 {Motion:[0.0d,0.0d,1.0d]}",
    );
    s.tick(20);
    assert_eq!(property(&mut s, SPOT, "lit").as_deref(), Some("false"));
}

fn ignite_asked(s: &mut Scene) -> Fired {
    for _ in 0..5 {
        if let Some(asked) = asked_for(&s.plugin, EventKind::BlockIgnite).pop() {
            return asked;
        }
        s.tick(1);
    }
    panic!("a BlockIgnite was asked");
}

#[test]
fn flint_and_steel_asks_block_ignite_with_the_flint_cause() {
    let mut s = meadow(vec![deciding(1, EventKind::BlockIgnite)]);
    campfire(&mut s, SPOT, "false", "false");
    console(&mut s, "give Alice minecraft:flint_and_steel");
    use_on(&mut s, SPOT, Direction::Up, 1.0);
    let asked = ignite_asked(&mut s);
    match &asked.payload {
        EventPayload::BlockIgnite(event) => {
            assert_eq!(event.block, SPOT);
            assert_eq!(event.cause, BlockIgniteCause::FlintAndSteel);
            assert!(event.player.is_some());
        }
        other => panic!("expected an ignite, got {other:?}"),
    }
    rule_on(&mut s.server, 1, asked.id, false, None);
    s.tick(2);
    assert_eq!(property(&mut s, SPOT, "lit").as_deref(), Some("true"));
    assert_eq!(damage_of(&mut s, "minecraft:flint_and_steel"), 1);
}

#[test]
fn a_fire_charge_asks_block_ignite_with_the_fireball_cause() {
    let mut s = meadow(vec![deciding(1, EventKind::BlockIgnite)]);
    console(&mut s, "give Alice minecraft:fire_charge");
    use_on(&mut s, GRASS, Direction::Up, 1.0);
    let asked = ignite_asked(&mut s);
    match &asked.payload {
        EventPayload::BlockIgnite(event) => {
            assert_eq!(event.block, SPOT);
            assert_eq!(event.cause, BlockIgniteCause::Fireball);
        }
        other => panic!("expected an ignite, got {other:?}"),
    }
}

#[test]
fn a_cancelled_block_ignite_wears_nothing_and_changes_nothing() {
    let mut s = meadow(vec![deciding(1, EventKind::BlockIgnite)]);
    campfire(&mut s, SPOT, "false", "false");
    console(&mut s, "give Alice minecraft:flint_and_steel");
    use_on(&mut s, SPOT, Direction::Up, 1.0);
    let asked = ignite_asked(&mut s);
    s.players[0].link.take();
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(3);
    assert_eq!(property(&mut s, SPOT, "lit").as_deref(), Some("false"));
    assert_eq!(damage_of(&mut s, "minecraft:flint_and_steel"), 0);
    assert!(!heard(&s, "minecraft:item.flintandsteel.use"));
}

#[test]
fn a_burning_arrow_asks_block_ignite_with_the_arrow_cause() {
    let mut s = meadow(vec![deciding(1, EventKind::BlockIgnite)]);
    campfire(&mut s, SPOT, "false", "false");
    console(
        &mut s,
        "summon minecraft:arrow 10.5 64.2 9.2 {Fire:200s,Motion:[0.0d,0.0d,1.0d]}",
    );
    let mut asked = None;
    for _ in 0..30 {
        asked = asked_for(&s.plugin, EventKind::BlockIgnite).pop();
        if asked.is_some() {
            break;
        }
        s.tick(1);
    }
    match asked.expect("an ignite was asked").payload {
        EventPayload::BlockIgnite(event) => {
            assert_eq!(event.block, SPOT);
            assert_eq!(event.cause, BlockIgniteCause::Arrow);
            assert!(event.igniting_entity.is_some());
        }
        other => panic!("expected an ignite, got {other:?}"),
    }
}
