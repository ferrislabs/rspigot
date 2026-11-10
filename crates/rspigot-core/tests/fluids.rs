use std::num::NonZeroU8;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rspigot_blocks::{Block, BlockKind, Stone};
use rspigot_core::admin::{AdminLists, ListEntry};
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, Direction, Hand, Inbound, PlayerInput};
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

const LEVEL: &str = "level";

fn at(chunk: &mut ChunkColumn, x: i32, y: i32, z: i32, state: BlockStateId) {
    chunk.set_block(BlockPos::new(x, y, z), state);
}

struct Terrain;

impl ChunkGenerator for Terrain {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(0));
        for x in 0..16 {
            for z in 0..16 {
                for y in 59..=63 {
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        Stone::DEFAULT_STATE,
                    );
                }
            }
        }
        if pos == ChunkPos::new(0, 0) {
            let water = BlockKind::Water.default_state();
            let lava = BlockKind::Lava.default_state();
            at(&mut chunk, 8, 64, 2, water);
            at(&mut chunk, 3, 64, 12, water);
            for y in 60..=63 {
                at(&mut chunk, 6, y, 12, BlockStateId::AIR);
            }
            at(&mut chunk, 11, 64, 10, water);
            at(&mut chunk, 13, 64, 10, water);
            at(&mut chunk, 1, 64, 15, lava);
            at(&mut chunk, 2, 64, 15, water);
            at(&mut chunk, 1, 64, 8, lava);
            at(&mut chunk, 3, 64, 8, water);
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

fn creative(server: &mut Server<Link>) -> (PlayerId, Link) {
    let (alice, link) = join(server);
    input(
        server,
        alice,
        PlayerInput::Command("gamemode creative".to_owned()),
    );
    give(server, alice, BlockKind::Stone);
    link.take();
    (alice, link)
}

fn trigger(server: &mut Server<Link>, player: PlayerId, stand: Vec3, floor: BlockPos) {
    input(
        server,
        player,
        PlayerInput::Command(format!("tp @s {} {} {}", stand.x, stand.y, stand.z)),
    );
    place(server, player, floor);
}

fn block_at(events: &[ClientEvent], target: BlockPos) -> Option<BlockStateId> {
    events.iter().rev().find_map(|e| match e {
        ClientEvent::BlockUpdate { pos, state } if *pos == target => Some(*state),
        _ => None,
    })
}

fn settled(server: &mut Server<Link>, link: &Link, count: usize) -> Vec<ClientEvent> {
    let mut events = Vec::new();
    for _ in 0..count {
        server.tick();
        events.extend(link.take());
    }
    events
}

fn level_at(events: &[ClientEvent], pos: BlockPos) -> Option<String> {
    let state = block_at(events, pos).filter(|s| s.kind() == Some(BlockKind::Water))?;
    state.property(LEVEL).map(str::to_owned)
}

#[test]
fn a_water_source_spreads_seven_blocks_on_flat_ground() {
    let mut server = server();
    let (alice, link) = creative(&mut server);
    trigger(
        &mut server,
        alice,
        Vec3::new(8.5, 64.0, 5.5),
        BlockPos::new(8, 63, 3),
    );
    let events = settled(&mut server, &link, 120);
    for step in 1..=7 {
        let pos = BlockPos::new(8, 64, 2 - step);
        assert_eq!(level_at(&events, pos), Some(step.to_string()), "{pos:?}");
    }
    assert_eq!(level_at(&events, BlockPos::new(8, 64, -6)), None);
}

#[test]
fn water_runs_to_a_hole_and_falls_down_it() {
    let mut server = server();
    let (alice, link) = creative(&mut server);
    trigger(
        &mut server,
        alice,
        Vec3::new(3.5, 64.0, 10.5),
        BlockPos::new(3, 63, 13),
    );
    let events = settled(&mut server, &link, 150);
    assert_eq!(
        level_at(&events, BlockPos::new(4, 64, 12)),
        Some("1".to_owned())
    );
    for y in 60..=63 {
        let pos = BlockPos::new(6, y, 12);
        assert_eq!(level_at(&events, pos), Some("8".to_owned()), "{pos:?}");
    }
}

#[test]
fn two_water_sources_make_a_third_between_them() {
    let mut server = server();
    let (alice, link) = creative(&mut server);
    trigger(
        &mut server,
        alice,
        Vec3::new(11.5, 64.0, 8.5),
        BlockPos::new(11, 63, 11),
    );
    let events = settled(&mut server, &link, 80);
    assert_eq!(
        level_at(&events, BlockPos::new(12, 64, 10)),
        Some("0".to_owned())
    );
}

#[test]
fn lava_next_to_water_becomes_obsidian() {
    let mut server = server();
    let (alice, link) = creative(&mut server);
    trigger(
        &mut server,
        alice,
        Vec3::new(1.5, 64.0, 12.5),
        BlockPos::new(1, 63, 14),
    );
    let events = settled(&mut server, &link, 10);
    let state = block_at(&events, BlockPos::new(1, 64, 15));
    assert_eq!(
        state.and_then(BlockStateId::kind),
        Some(BlockKind::Obsidian)
    );
}

#[test]
fn flowing_lava_that_meets_water_becomes_cobblestone() {
    let mut server = server();
    let (alice, link) = creative(&mut server);
    trigger(
        &mut server,
        alice,
        Vec3::new(1.5, 64.0, 5.5),
        BlockPos::new(1, 63, 7),
    );
    let events = settled(&mut server, &link, 100);
    let state = block_at(&events, BlockPos::new(2, 64, 8));
    assert_eq!(
        state.and_then(BlockStateId::kind),
        Some(BlockKind::Cobblestone)
    );
}
