use std::num::NonZeroU8;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rspigot_blocks::{Block, BlockKind, Sand, Stone, Torch};
use rspigot_core::admin::{AdminLists, ListEntry};
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{
    ClientEvent, DigAction, Direction, EntityKind, Hand, Inbound, PlayerInput,
};
use rspigot_core::ports::{ChunkGenerator, ChunkStore, ClientLink, EntityStore, StoreError};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::{ClientSettings, GameProfile, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Identifier, ItemId, Vec3};

#[derive(Clone, Default)]
struct Link {
    events: Arc<Mutex<Vec<ClientEvent>>>,
    closed: Arc<AtomicBool>,
}

impl Link {
    fn take(&self) -> Vec<ClientEvent> {
        self.events
            .lock()
            .map(|mut e| std::mem::take(&mut *e))
            .unwrap_or_default()
    }
}

impl ClientLink for Link {
    fn send(&self, event: ClientEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
    }

    fn disconnect(&self, _: Component) {
        self.closed.store(true, Ordering::Release);
    }

    fn is_connected(&self) -> bool {
        !self.closed.load(Ordering::Acquire)
    }
}

struct NoStore;

impl ChunkStore for NoStore {
    fn load(&self, _: &Identifier, _: ChunkPos) -> Result<Option<ChunkColumn>, StoreError> {
        Ok(None)
    }

    fn save(&self, _: &Identifier, _: &ChunkColumn) -> Result<(), StoreError> {
        Ok(())
    }

    fn flush(&self) -> Result<(), StoreError> {
        Ok(())
    }
}

impl EntityStore for NoStore {
    fn load_entities(
        &self,
        _: &Identifier,
        _: ChunkPos,
    ) -> Result<Option<Vec<rspigot_nbt::Compound>>, StoreError> {
        Ok(None)
    }

    fn save_entities(
        &self,
        _: &Identifier,
        _: ChunkPos,
        _: &[rspigot_nbt::Compound],
    ) -> Result<(), StoreError> {
        Ok(())
    }
}

const COLUMN: (i32, i32) = (5, 5);
const TORCH: (i32, i32) = (5, 2);
const POOL: (i32, i32) = (3, 4);
const STALE_POWDER: BlockPos = BlockPos::new(3, 64, 3);
const ANVIL: (i32, i32) = (8, 8);
const WORN_ANVIL: (i32, i32) = (11, 8);
const STALACTITE: (i32, i32) = (12, 12);
const STALAGMITE: (i32, i32) = (12, 3);
const BRIDGE_Z: i32 = 12;
const BRIDGE_LEFT: i32 = 1;
const BRIDGE_RIGHT: i32 = 9;

fn state(kind: BlockKind, properties: &[(&str, &str)]) -> BlockStateId {
    kind.state_with(properties).unwrap_or_default()
}

fn dripstone(thickness: &str, direction: &str) -> BlockStateId {
    state(
        BlockKind::PointedDripstone,
        &[
            ("thickness", thickness),
            ("vertical_direction", direction),
            ("waterlogged", "false"),
        ],
    )
}

fn scaffolding(distance: i32, bottom: bool) -> BlockStateId {
    let distance = distance.to_string();
    let bottom = if bottom { "true" } else { "false" };
    state(
        BlockKind::Scaffolding,
        &[
            ("distance", distance.as_str()),
            ("bottom", bottom),
            ("waterlogged", "false"),
        ],
    )
}

fn bridge_distance(x: i32) -> i32 {
    (x - BRIDGE_LEFT).min(BRIDGE_RIGHT - x)
}

struct Terrain;

impl ChunkGenerator for Terrain {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(0));
        for x in 0..16 {
            for z in 0..16 {
                for y in 60..=63 {
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        Stone::DEFAULT_STATE,
                    );
                }
            }
        }
        if pos == ChunkPos::new(0, 0) {
            let (x, z) = COLUMN;
            chunk.set_block(BlockPos::new(x, 64, z), Stone::DEFAULT_STATE);
            for y in 65..=67 {
                chunk.set_block(BlockPos::new(x, y, z), Sand::DEFAULT_STATE);
            }
            let (tx, tz) = TORCH;
            chunk.set_block(BlockPos::new(tx, 64, tz), Torch::DEFAULT_STATE);
            chunk.set_block(BlockPos::new(tx, 65, tz), Stone::DEFAULT_STATE);
            chunk.set_block(BlockPos::new(tx, 66, tz), Sand::DEFAULT_STATE);
            for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                chunk.set_block(BlockPos::new(tx + dx, 64, tz + dz), Stone::DEFAULT_STATE);
            }
            let (px, pz) = POOL;
            let water = BlockKind::Water.default_state();
            chunk.set_block(BlockPos::new(px, 64, pz), water);
            chunk.set_block(BlockPos::new(px, 65, pz), water);
            chunk.set_block(STALE_POWDER, BlockKind::WhiteConcretePowder.default_state());
            for ((x, z), kind) in [
                (ANVIL, BlockKind::Anvil),
                (WORN_ANVIL, BlockKind::DamagedAnvil),
            ] {
                chunk.set_block(BlockPos::new(x, 89, z), Stone::DEFAULT_STATE);
                chunk.set_block(BlockPos::new(x, 90, z), state(kind, &[("facing", "east")]));
            }
            let (sx, sz) = STALACTITE;
            chunk.set_block(BlockPos::new(sx, 75, sz), Stone::DEFAULT_STATE);
            for (y, thickness) in [(74, "base"), (73, "middle"), (72, "frustum"), (71, "tip")] {
                chunk.set_block(BlockPos::new(sx, y, sz), dripstone(thickness, "down"));
            }
            let (gx, gz) = STALAGMITE;
            chunk.set_block(BlockPos::new(gx, 70, gz), Stone::DEFAULT_STATE);
            chunk.set_block(BlockPos::new(gx, 71, gz), dripstone("frustum", "up"));
            chunk.set_block(BlockPos::new(gx, 72, gz), dripstone("tip", "up"));
            for x in [BRIDGE_LEFT, BRIDGE_RIGHT] {
                for y in 64..=66 {
                    chunk.set_block(BlockPos::new(x, y, BRIDGE_Z), scaffolding(0, false));
                }
            }
            for x in BRIDGE_LEFT + 1..BRIDGE_RIGHT {
                chunk.set_block(
                    BlockPos::new(x, 66, BRIDGE_Z),
                    scaffolding(bridge_distance(x), true),
                );
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

fn server() -> Server<Link> {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        admin: AdminLists {
            operators: vec![ListEntry::named("alice")],
            whitelist: Vec::new(),
            whitelist_enabled: false,
            ..AdminLists::default()
        },
        ..GameConfig::default()
    };
    let mut server = Server::new(
        config,
        LevelData::default(),
        Arc::new(NoStore),
        Arc::new(Terrain),
        StatusBoard::new(20),
    );
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn join(server: &mut Server<Link>) -> (PlayerId, Link) {
    let link = Link::default();
    let profile = GameProfile {
        id: PlayerId::offline("Alice"),
        name: "Alice".to_owned(),
        properties: Vec::new(),
    };
    let id = profile.id;
    server.handle(Inbound::Join {
        profile,
        settings: ClientSettings::default(),
        link: link.clone(),
        address: std::net::Ipv4Addr::LOCALHOST.into(),
        session: 0,
    });
    server.deliver();
    for event in link.take() {
        if let ClientEvent::Teleport { id: teleport, .. } = event {
            server.handle(Inbound::Input {
                player: id,
                input: PlayerInput::TeleportConfirm(teleport),
            });
        }
    }
    for _ in 0..30 {
        server.wait_for_spawn(Duration::from_millis(20));
        server.tick();
        let batches = link
            .take()
            .iter()
            .filter(|e| matches!(e, ClientEvent::ChunkBatchFinished { .. }))
            .count();
        for _ in 0..batches {
            server.handle(Inbound::Input {
                player: id,
                input: PlayerInput::ChunkBatchReceived {
                    chunks_per_tick: 64.0,
                },
            });
        }
    }
    (id, link)
}

fn input(server: &mut Server<Link>, player: PlayerId, input: PlayerInput) {
    server.handle(Inbound::Input { player, input });
    server.deliver();
}

fn ticks(server: &mut Server<Link>, count: usize) {
    for _ in 0..count {
        server.tick();
    }
}

fn give(server: &mut Server<Link>, player: PlayerId, kind: BlockKind) {
    let item = kind.item().unwrap_or_default();
    input(
        server,
        player,
        PlayerInput::CreativeSlot {
            slot: 36,
            stack: Some(ItemStack::new(ItemId(item), NonZeroU8::MIN)),
        },
    );
}

fn place(server: &mut Server<Link>, player: PlayerId, pos: BlockPos) {
    input(
        server,
        player,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 2,
        },
    );
}

fn dig(server: &mut Server<Link>, player: PlayerId, pos: BlockPos) {
    input(
        server,
        player,
        PlayerInput::Dig {
            action: DigAction::Start,
            pos,
            face: Direction::Up,
            sequence: 1,
        },
    );
}

fn walk(server: &mut Server<Link>, player: PlayerId, position: Vec3) {
    input(
        server,
        player,
        PlayerInput::Move {
            position: Some(position),
            rotation: None,
            on_ground: false,
        },
    );
}

fn creative(server: &mut Server<Link>) -> (PlayerId, Link) {
    let (alice, link) = join(server);
    input(
        server,
        alice,
        PlayerInput::Command("gamemode creative".to_owned()),
    );
    link.take();
    (alice, link)
}

fn falling_spawns(events: &[ClientEvent]) -> Vec<i32> {
    events
        .iter()
        .filter_map(|e| match e {
            ClientEvent::SpawnEntity {
                kind: EntityKind::FallingBlock,
                data,
                ..
            } => Some(*data),
            _ => None,
        })
        .collect()
}

fn block_at(events: &[ClientEvent], target: BlockPos) -> Option<BlockStateId> {
    events.iter().rev().find_map(|e| match e {
        ClientEvent::BlockUpdate { pos, state } if *pos == target => Some(*state),
        _ => None,
    })
}

#[test]
fn breaking_the_support_drops_the_whole_sand_column() {
    let mut server = server();
    let (alice, link) = join(&mut server);
    input(
        &mut server,
        alice,
        PlayerInput::Command("gamemode creative".to_owned()),
    );
    let (x, z) = COLUMN;
    input(
        &mut server,
        alice,
        PlayerInput::Dig {
            action: DigAction::Start,
            pos: BlockPos::new(x, 64, z),
            face: Direction::Up,
            sequence: 1,
        },
    );
    link.take();
    ticks(&mut server, 3);
    let spawned = link
        .take()
        .iter()
        .filter(|e| matches!(e, ClientEvent::SpawnEntity { kind: EntityKind::FallingBlock, data, .. } if *data == i32::from(Sand::DEFAULT_STATE.0)))
        .count();
    assert_eq!(spawned, 1);
    ticks(&mut server, 60);
    assert_eq!(server.falling_blocks(), 0);
    let events = link.take();
    assert_eq!(
        block_at(&events, BlockPos::new(x, 64, z)),
        Some(Sand::DEFAULT_STATE)
    );
    assert_eq!(
        block_at(&events, BlockPos::new(x, 65, z)),
        Some(Sand::DEFAULT_STATE)
    );
    assert_eq!(
        block_at(&events, BlockPos::new(x, 66, z)),
        Some(Sand::DEFAULT_STATE)
    );
    assert_eq!(
        block_at(&events, BlockPos::new(x, 67, z)),
        Some(BlockStateId::AIR)
    );
}

#[test]
fn sand_placed_in_mid_air_falls_and_lands() {
    let mut server = server();
    let (alice, link) = join(&mut server);
    input(
        &mut server,
        alice,
        PlayerInput::Command("gamemode creative".to_owned()),
    );
    let sand_item = BlockKind::from_name("sand")
        .and_then(BlockKind::item)
        .unwrap_or_default();
    input(
        &mut server,
        alice,
        PlayerInput::CreativeSlot {
            slot: 36,
            stack: Some(ItemStack::new(ItemId(sand_item), NonZeroU8::MIN)),
        },
    );
    input(
        &mut server,
        alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: BlockPos::new(2, 66, 2),
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 2,
        },
    );
    assert_eq!(server.falling_blocks(), 0);
    ticks(&mut server, 2);
    assert_eq!(server.falling_blocks(), 1);
    link.take();
    ticks(&mut server, 40);
    assert_eq!(server.falling_blocks(), 0);
    assert_eq!(
        block_at(&link.take(), BlockPos::new(2, 64, 2)),
        Some(Sand::DEFAULT_STATE)
    );
}

#[test]
fn sand_falling_onto_a_torch_breaks_into_an_item() {
    let mut server = server();
    let (alice, link) = join(&mut server);
    input(
        &mut server,
        alice,
        PlayerInput::Command("gamemode creative".to_owned()),
    );
    let (x, z) = TORCH;
    input(
        &mut server,
        alice,
        PlayerInput::Dig {
            action: DigAction::Start,
            pos: BlockPos::new(x, 65, z),
            face: Direction::Up,
            sequence: 1,
        },
    );
    link.take();
    ticks(&mut server, 40);
    assert_eq!(server.falling_blocks(), 0);
    assert_eq!(server.items_on_ground(), 1);
    let events = link.take();
    assert_eq!(block_at(&events, BlockPos::new(x, 64, z)), None);
}

#[test]
fn concrete_powder_dropped_into_water_becomes_concrete() {
    let mut server = server();
    let (alice, link) = creative(&mut server);
    give(&mut server, alice, BlockKind::WhiteConcretePowder);
    let (x, z) = POOL;
    place(&mut server, alice, BlockPos::new(x, 68, z));
    ticks(&mut server, 2);
    assert_eq!(server.falling_blocks(), 1);
    ticks(&mut server, 40);
    assert_eq!(server.falling_blocks(), 0);
    let events = link.take();
    assert_eq!(
        block_at(&events, BlockPos::new(x, 65, z)),
        Some(BlockKind::WhiteConcrete.default_state())
    );
    assert_eq!(block_at(&events, BlockPos::new(x, 64, z)), None);
    assert_eq!(server.items_on_ground(), 0);
}

#[test]
fn concrete_powder_placed_against_or_into_water_hardens_at_once() {
    let mut server = server();
    let (alice, link) = creative(&mut server);
    give(&mut server, alice, BlockKind::WhiteConcretePowder);
    let (x, z) = POOL;
    let beside = BlockPos::new(x + 1, 65, z);
    let inside = BlockPos::new(x, 65, z);
    place(&mut server, alice, beside);
    place(&mut server, alice, inside);
    let concrete = BlockKind::WhiteConcrete.default_state();
    let events = link.take();
    assert_eq!(block_at(&events, beside), Some(concrete));
    assert_eq!(block_at(&events, inside), Some(concrete));
    ticks(&mut server, 5);
    assert_eq!(server.falling_blocks(), 0);
}

#[test]
fn concrete_powder_already_beside_water_hardens_on_a_neighbour_update() {
    let mut server = server();
    let (alice, link) = creative(&mut server);
    give(&mut server, alice, BlockKind::Stone);
    place(&mut server, alice, STALE_POWDER.offset(0, 1, 0));
    assert_eq!(
        block_at(&link.take(), STALE_POWDER),
        Some(BlockKind::WhiteConcrete.default_state())
    );
}

#[test]
fn anvil_dropped_from_high_enough_chips_and_keeps_its_facing() {
    let mut server = server();
    let (alice, link) = creative(&mut server);
    let (x, z) = ANVIL;
    walk(
        &mut server,
        alice,
        Vec3::new(f64::from(x) + 0.5, 89.0, f64::from(z) + 2.5),
    );
    dig(&mut server, alice, BlockPos::new(x, 89, z));
    ticks(&mut server, 3);
    assert_eq!(server.falling_blocks(), 1);
    ticks(&mut server, 60);
    assert_eq!(server.falling_blocks(), 0);
    let events = link.take();
    assert_eq!(
        block_at(&events, BlockPos::new(x, 64, z)),
        Some(state(BlockKind::ChippedAnvil, &[("facing", "east")]))
    );
    assert_eq!(server.items_on_ground(), 0);
}

#[test]
fn damaged_anvil_destroyed_by_a_long_fall_leaves_nothing() {
    let mut server = server();
    let (alice, link) = creative(&mut server);
    let (x, z) = WORN_ANVIL;
    walk(
        &mut server,
        alice,
        Vec3::new(f64::from(x) + 0.5, 89.0, f64::from(z) + 2.5),
    );
    dig(&mut server, alice, BlockPos::new(x, 89, z));
    ticks(&mut server, 63);
    assert_eq!(server.falling_blocks(), 0);
    let events = link.take();
    assert_eq!(
        block_at(&events, BlockPos::new(x, 90, z)),
        Some(BlockStateId::AIR)
    );
    assert_eq!(block_at(&events, BlockPos::new(x, 64, z)), None);
    assert_eq!(server.items_on_ground(), 0);
}

#[test]
fn stalactite_column_falls_when_its_root_breaks_and_shatters_on_landing() {
    let mut server = server();
    let (alice, link) = creative(&mut server);
    let (x, z) = STALACTITE;
    walk(
        &mut server,
        alice,
        Vec3::new(f64::from(x) + 0.5, 74.0, f64::from(z) + 2.5),
    );
    dig(&mut server, alice, BlockPos::new(x, 75, z));
    ticks(&mut server, 1);
    assert_eq!(server.falling_blocks(), 0);
    ticks(&mut server, 1);
    assert_eq!(server.falling_blocks(), 4);
    let events = link.take();
    let mut expected: Vec<i32> = ["base", "middle", "frustum", "tip"]
        .into_iter()
        .map(|thickness| i32::from(dripstone(thickness, "down").0))
        .collect();
    expected.sort_unstable();
    let mut spawned = falling_spawns(&events);
    spawned.sort_unstable();
    assert_eq!(spawned, expected);
    for y in 71..=74 {
        assert_eq!(
            block_at(&events, BlockPos::new(x, y, z)),
            Some(BlockStateId::AIR)
        );
    }
    ticks(&mut server, 60);
    assert_eq!(server.falling_blocks(), 0);
    let events = link.take();
    for y in 64..=74 {
        assert_eq!(block_at(&events, BlockPos::new(x, y, z)), None);
    }
    assert_eq!(server.items_on_ground(), 4);
}

#[test]
fn stalagmite_losing_its_floor_breaks_into_items() {
    let mut server = server();
    let (alice, link) = creative(&mut server);
    let (x, z) = STALAGMITE;
    walk(
        &mut server,
        alice,
        Vec3::new(f64::from(x) + 0.5, 69.0, f64::from(z) + 4.5),
    );
    dig(&mut server, alice, BlockPos::new(x, 70, z));
    ticks(&mut server, 1);
    let events = link.take();
    assert_eq!(
        block_at(&events, BlockPos::new(x, 71, z)),
        Some(BlockStateId::AIR)
    );
    assert_eq!(block_at(&events, BlockPos::new(x, 72, z)), None);
    ticks(&mut server, 1);
    assert_eq!(
        block_at(&link.take(), BlockPos::new(x, 72, z)),
        Some(BlockStateId::AIR)
    );
    assert_eq!(server.falling_blocks(), 0);
    assert_eq!(server.items_on_ground(), 2);
}

#[test]
fn scaffolding_seven_blocks_from_its_last_support_breaks() {
    let mut server = server();
    let (alice, link) = creative(&mut server);
    walk(
        &mut server,
        alice,
        Vec3::new(
            f64::from(BRIDGE_RIGHT) + 0.5,
            64.0,
            f64::from(BRIDGE_Z) + 4.5,
        ),
    );
    dig(
        &mut server,
        alice,
        BlockPos::new(BRIDGE_RIGHT, 64, BRIDGE_Z),
    );
    let mut events = Vec::new();
    for _ in 0..200 {
        server.tick();
        events.extend(link.take());
    }
    let at = |x: i32, y: i32, initial: BlockStateId| {
        block_at(&events, BlockPos::new(x, y, BRIDGE_Z)).unwrap_or(initial)
    };
    for y in 64..=66 {
        assert_eq!(
            at(BRIDGE_LEFT, y, scaffolding(0, false)),
            scaffolding(0, false)
        );
    }
    for x in BRIDGE_LEFT + 1..BRIDGE_LEFT + 7 {
        let initial = scaffolding(bridge_distance(x), true);
        assert_eq!(at(x, 66, initial), scaffolding(x - BRIDGE_LEFT, true));
    }
    let initial = scaffolding(bridge_distance(BRIDGE_LEFT + 7), true);
    assert_eq!(at(BRIDGE_LEFT + 7, 66, initial), BlockStateId::AIR);
    for y in 64..=66 {
        assert_eq!(
            at(BRIDGE_RIGHT, y, scaffolding(0, false)),
            BlockStateId::AIR
        );
    }
    assert_eq!(server.falling_blocks(), 0);
    assert_eq!(server.items_on_ground(), 3);
}

#[test]
fn unsupported_scaffolding_falls_and_settles_where_it_lands() {
    let mut server = server();
    let (alice, link) = creative(&mut server);
    give(&mut server, alice, BlockKind::Scaffolding);
    let pos = BlockPos::new(4, 67, 1);
    place(&mut server, alice, pos);
    assert_eq!(block_at(&link.take(), pos), Some(scaffolding(7, true)));
    ticks(&mut server, 1);
    assert_eq!(server.falling_blocks(), 1);
    assert_eq!(
        falling_spawns(&link.take()),
        vec![i32::from(scaffolding(7, true).0)]
    );
    ticks(&mut server, 40);
    assert_eq!(server.falling_blocks(), 0);
    let events = link.take();
    assert_eq!(
        block_at(&events, BlockPos::new(4, 64, 1)),
        Some(scaffolding(0, false))
    );
    assert_eq!(server.items_on_ground(), 0);
}
