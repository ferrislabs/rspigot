use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rspigot_blocks::{Block, GrassBlock, Stone};
use rspigot_core::admin::{AdminLists, ListEntry};
use rspigot_core::chunks::ChunkManagerConfig;
use rspigot_core::config::{GameConfig, LevelData, SpawningConfig};
use rspigot_core::events::{ClientEvent, EntityKind, Inbound, PlayerInput};
use rspigot_core::ports::{ChunkGenerator, ChunkStore, ClientLink, EntityStore, StoreError};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::player::{ClientSettings, GameProfile, PlayerId};
use rspigot_domain::spawn::state::{CategorySettings, SpawnSettings};
use rspigot_domain::text::Component;
use rspigot_domain::world::{Difficulty, Dimension};
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Identifier, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;
use uuid::Uuid;

const ANIMALS: [Kind; 4] = [Kind::Cow, Kind::Pig, Kind::Sheep, Kind::Chicken];
const CREATURES: [Kind; 6] = [
    Kind::Cow,
    Kind::Pig,
    Kind::Sheep,
    Kind::Chicken,
    Kind::Horse,
    Kind::Donkey,
];
const MONSTERS: [Kind; 4] = [Kind::Zombie, Kind::Skeleton, Kind::Creeper, Kind::Spider];
const POCKET: std::ops::RangeInclusive<i32> = 30..=32;
const CAVE: std::ops::Range<i32> = -48..64;

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

#[derive(Default)]
struct DiskStore {
    chunks: Mutex<HashMap<ChunkPos, ChunkColumn>>,
    entities: Mutex<HashMap<ChunkPos, Vec<Compound>>>,
}

impl DiskStore {
    fn stored_uuids(&self) -> HashSet<Vec<i32>> {
        self.entities
            .lock()
            .map(|map| {
                map.values()
                    .flatten()
                    .filter_map(|nbt| nbt.get_int_array("UUID").map(<[i32]>::to_vec))
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl ChunkStore for DiskStore {
    fn load(&self, _: &Identifier, pos: ChunkPos) -> Result<Option<ChunkColumn>, StoreError> {
        Ok(self
            .chunks
            .lock()
            .ok()
            .and_then(|map| map.get(&pos).cloned()))
    }

    fn save(&self, _: &Identifier, chunk: &ChunkColumn) -> Result<(), StoreError> {
        if let Ok(mut map) = self.chunks.lock() {
            map.insert(chunk.pos(), chunk.clone());
        }
        Ok(())
    }

    fn flush(&self) -> Result<(), StoreError> {
        Ok(())
    }
}

impl EntityStore for DiskStore {
    fn load_entities(
        &self,
        _: &Identifier,
        pos: ChunkPos,
    ) -> Result<Option<Vec<Compound>>, StoreError> {
        Ok(self
            .entities
            .lock()
            .ok()
            .and_then(|map| map.get(&pos).cloned()))
    }

    fn save_entities(
        &self,
        _: &Identifier,
        pos: ChunkPos,
        entities: &[Compound],
    ) -> Result<(), StoreError> {
        if let Ok(mut map) = self.entities.lock() {
            map.insert(pos, entities.to_vec());
        }
        Ok(())
    }
}

fn plains() -> BiomeId {
    BiomeId(rspigot_data::registries::biome_id("minecraft:plains").unwrap_or(0))
}

#[derive(Clone, Copy)]
enum Terrain {
    Grass,
    Caves,
}

struct World {
    terrain: Terrain,
    original_mobs: bool,
}

impl World {
    fn column(&self, chunk: &mut ChunkColumn, x: i32, z: i32) {
        let set = |chunk: &mut ChunkColumn, y: i32, state: BlockStateId| {
            chunk.set_block(BlockPos::new(x, y, z), state);
        };
        match self.terrain {
            Terrain::Grass => {
                for y in 60..63 {
                    set(chunk, y, Stone::DEFAULT_STATE);
                }
                set(chunk, 63, GrassBlock::DEFAULT_STATE);
            }
            Terrain::Caves => {
                let open = CAVE.contains(&x) && CAVE.contains(&z);
                for y in 20..=63 {
                    if !(open && POCKET.contains(&y)) {
                        set(chunk, y, Stone::DEFAULT_STATE);
                    }
                }
            }
        }
    }
}

impl ChunkGenerator for World {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let mut chunk = ChunkColumn::empty(pos, dimension.height, plains());
        for x in 0..16 {
            for z in 0..16 {
                self.column(&mut chunk, pos.min_block_x() + x, pos.min_block_z() + z);
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }

    fn spawns_original_mobs(&self, _: &Dimension) -> bool {
        self.original_mobs
    }
}

fn every_tick(limit: i32) -> CategorySettings {
    CategorySettings {
        limit,
        ticks_per: 1,
    }
}

fn categories(monsters: i32, animals: i32) -> SpawnSettings {
    SpawnSettings {
        monster: every_tick(monsters),
        creature: every_tick(animals),
        ambient: every_tick(0),
        axolotls: every_tick(0),
        underground_water_creature: every_tick(0),
        water_creature: every_tick(0),
        water_ambient: every_tick(0),
    }
}

struct Setup {
    terrain: Terrain,
    original_mobs: bool,
    spawning: SpawnSettings,
    view_distance: u8,
    difficulty: Difficulty,
    spawn_mobs: bool,
}

impl Setup {
    fn new(terrain: Terrain) -> Self {
        Self {
            terrain,
            original_mobs: false,
            spawning: categories(70, 10),
            view_distance: 4,
            difficulty: Difficulty::Normal,
            spawn_mobs: true,
        }
    }

    fn start(self, store: Arc<DiskStore>) -> Server<Link> {
        let config = GameConfig {
            view_distance: self.view_distance,
            simulation_distance: self.view_distance,
            chunk_workers: 2,
            difficulty: self.difficulty,
            chunks: ChunkManagerConfig {
                unload_delay_ticks: 5,
                autosave_interval_ticks: 0,
            },
            admin: AdminLists {
                operators: vec![ListEntry::named("Alice")],
                whitelist: Vec::new(),
                whitelist_enabled: false,
                ..AdminLists::default()
            },
            spawning: SpawningConfig {
                categories: self.spawning,
                ..SpawningConfig::default()
            },
            ..GameConfig::default()
        };
        let level = LevelData {
            day_time: 6000,
            daylight_cycle: false,
            spawn_mobs: self.spawn_mobs,
            ..LevelData::default()
        };
        let world = World {
            terrain: self.terrain,
            original_mobs: self.original_mobs,
        };
        let mut server = Server::new(config, level, store, Arc::new(world), StatusBoard::new(20));
        server.wait_for_spawn(Duration::from_secs(10));
        server
    }
}

struct Client {
    id: PlayerId,
    link: Link,
    events: Vec<ClientEvent>,
}

fn input(server: &mut Server<Link>, player: PlayerId, input: PlayerInput) {
    server.handle(Inbound::Input { player, input });
    server.deliver();
}

fn console(server: &mut Server<Link>, line: &str) {
    server.handle(Inbound::Console(line.to_owned()));
    server.deliver();
}

fn pump(server: &mut Server<Link>, client: &mut Client, ticks: usize) {
    for _ in 0..ticks {
        server.wait_for_spawn(Duration::from_millis(20));
        server.tick();
        let events = client.link.take();
        for event in &events {
            match event {
                ClientEvent::ChunkBatchFinished { .. } => input(
                    server,
                    client.id,
                    PlayerInput::ChunkBatchReceived {
                        chunks_per_tick: 64.0,
                    },
                ),
                ClientEvent::Teleport { id, .. } => {
                    input(server, client.id, PlayerInput::TeleportConfirm(*id))
                }
                _ => {}
            }
        }
        client.events.extend(events);
    }
}

fn settle(server: &mut Server<Link>, client: &mut Client) {
    for _ in 0..600 {
        pump(server, client, 1);
        if server.chunk_stats().pending == 0 {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn join(server: &mut Server<Link>, name: &str) -> Client {
    let link = Link::default();
    let profile = GameProfile {
        id: PlayerId::offline(name),
        name: name.to_owned(),
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
    let mut client = Client {
        id,
        link,
        events: Vec::new(),
    };
    for event in client.link.take() {
        if let ClientEvent::Teleport { id: teleport, .. } = event {
            input(server, id, PlayerInput::TeleportConfirm(teleport));
        }
    }
    input(server, id, PlayerInput::Loaded);
    settle(server, &mut client);
    client
}

fn of_kinds(server: &Server<Link>, kinds: &[Kind]) -> Vec<rspigot_core::entities::EntitySnapshot> {
    kinds
        .iter()
        .flat_map(|kind| server.entities_of(*kind))
        .collect()
}

fn added_kinds(client: &Client) -> HashSet<u16> {
    client
        .events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::SpawnEntity {
                kind: EntityKind::Other(id),
                ..
            } => Some(*id),
            _ => None,
        })
        .collect()
}

fn animals_added_at(client: &Client) -> Vec<Vec3> {
    client
        .events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::SpawnEntity {
                kind: EntityKind::Other(id),
                position,
                ..
            } if ANIMALS.iter().any(|kind| kind.id().0 == *id) => Some(*position),
            _ => None,
        })
        .collect()
}

#[test]
fn lit_grass_spawns_farm_animals_that_clients_see() {
    let mut server = Setup::new(Terrain::Grass).start(Arc::default());
    let mut client = join(&mut server, "Alice");
    pump(&mut server, &mut client, 40);
    let animals = of_kinds(&server, &ANIMALS);
    assert!(!animals.is_empty(), "no animal spawned on lit grass");
    assert!(of_kinds(&server, &MONSTERS).is_empty());
    for animal in &animals {
        assert_eq!(animal.position.y, 64.0);
    }
    let added = animals_added_at(&client);
    assert!(
        !added.is_empty(),
        "the client never received add_entity for a natural animal"
    );
    for position in added {
        let dx = position.x - 8.5;
        let dz = position.z - 8.5;
        assert!(
            dx * dx + dz * dz > 24.0 * 24.0,
            "an animal spawned at {position:?}"
        );
    }
}

#[test]
fn dark_caves_spawn_monsters_but_no_animals() {
    let mut server = Setup::new(Terrain::Caves).start(Arc::default());
    let mut client = join(&mut server, "Alice");
    pump(&mut server, &mut client, 60);
    let monsters = of_kinds(&server, &MONSTERS);
    assert!(!monsters.is_empty(), "no monster spawned in the dark");
    for monster in &monsters {
        assert!(POCKET.contains(&monster.position.block().y));
    }
    assert!(of_kinds(&server, &ANIMALS).is_empty());
    let seen = added_kinds(&client);
    assert!(MONSTERS.iter().any(|kind| seen.contains(&kind.id().0)));
}

#[test]
fn zero_limits_and_the_spawn_mobs_rule_stop_spawning() {
    let mut setup = Setup::new(Terrain::Caves);
    setup.spawning = categories(0, 0);
    let mut server = setup.start(Arc::default());
    let mut client = join(&mut server, "Alice");
    pump(&mut server, &mut client, 60);
    assert!(of_kinds(&server, &MONSTERS).is_empty());
    let mut ruled = Setup::new(Terrain::Caves);
    ruled.spawn_mobs = false;
    let mut server = ruled.start(Arc::default());
    let mut client = join(&mut server, "Alice");
    pump(&mut server, &mut client, 60);
    assert!(of_kinds(&server, &MONSTERS).is_empty());
}

#[test]
fn the_global_cap_counts_existing_monsters() {
    let mut setup = Setup::new(Terrain::Caves);
    setup.spawning = categories(3, 0);
    let mut server = setup.start(Arc::default());
    for x in 0..3 {
        console(
            &mut server,
            &format!("summon minecraft:zombie {} 30 8", 4 + x),
        );
    }
    let mut client = join(&mut server, "Alice");
    pump(&mut server, &mut client, 60);
    assert_eq!(server.entities_of(Kind::Zombie).len(), 3);
    assert_eq!(of_kinds(&server, &MONSTERS).len(), 3);
}

#[test]
fn peaceful_spawns_no_monsters_and_removes_existing_ones() {
    let mut server = Setup::new(Terrain::Caves).start(Arc::default());
    let mut client = join(&mut server, "Alice");
    pump(&mut server, &mut client, 60);
    assert!(!of_kinds(&server, &MONSTERS).is_empty());
    console(&mut server, "summon minecraft:cow 8 64 8");
    console(&mut server, "difficulty peaceful");
    pump(&mut server, &mut client, 2);
    assert!(of_kinds(&server, &MONSTERS).is_empty());
    assert_eq!(server.entities_of(Kind::Cow).len(), 1);
    pump(&mut server, &mut client, 60);
    assert!(of_kinds(&server, &MONSTERS).is_empty());
}

#[test]
fn far_monsters_despawn_while_animals_and_persistent_mobs_stay() {
    let mut setup = Setup::new(Terrain::Grass);
    setup.spawning = categories(0, 0);
    setup.view_distance = 10;
    let mut server = setup.start(Arc::default());
    let mut client = join(&mut server, "Alice");
    console(&mut server, "summon minecraft:zombie 8 64 8");
    console(
        &mut server,
        "summon minecraft:zombie 9 64 9 {PersistenceRequired:1b}",
    );
    console(&mut server, "summon minecraft:cow 7 64 7");
    pump(&mut server, &mut client, 5);
    assert_eq!(server.entities_of(Kind::Zombie).len(), 2);
    console(&mut server, "tp Alice 150 64 8");
    pump(&mut server, &mut client, 5);
    settle(&mut server, &mut client);
    pump(&mut server, &mut client, 5);
    assert_eq!(server.entities_of(Kind::Zombie).len(), 1);
    assert_eq!(server.entities_of(Kind::Cow).len(), 1);
}

#[test]
fn chunk_generation_animals_are_saved_and_not_spawned_again() {
    let store = Arc::new(DiskStore::default());
    let mut setup = Setup::new(Terrain::Grass);
    setup.original_mobs = true;
    setup.spawning = categories(0, 0);
    setup.view_distance = 5;
    let mut server = setup.start(Arc::clone(&store));
    let mut client = join(&mut server, "Alice");
    let near = |server: &Server<Link>| -> HashSet<Uuid> {
        of_kinds(server, &ANIMALS)
            .into_iter()
            .filter(|animal| animal.position.x.abs() < 200.0 && animal.position.z.abs() < 200.0)
            .map(|animal| animal.uuid)
            .collect()
    };
    let original = near(&server);
    assert!(!original.is_empty(), "chunk generation spawned no animal");
    for animal in of_kinds(&server, &ANIMALS) {
        assert_eq!(animal.position.y, 64.0);
    }
    console(&mut server, "tp Alice 3000 64 3000");
    pump(&mut server, &mut client, 20);
    settle(&mut server, &mut client);
    for _ in 0..40 {
        pump(&mut server, &mut client, 1);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(near(&server).is_empty());
    assert!(store.stored_uuids().len() >= original.len());
    console(&mut server, "tp Alice 8 64 8");
    pump(&mut server, &mut client, 5);
    settle(&mut server, &mut client);
    pump(&mut server, &mut client, 5);
    assert_eq!(near(&server), original);
    assert!(server.chunk_stats().loaded_from_disk > 0);
}

#[test]
fn the_creature_cap_stops_spawning_near_its_limit() {
    let mut server = Setup::new(Terrain::Grass).start(Arc::default());
    let mut client = join(&mut server, "Alice");
    pump(&mut server, &mut client, 80);
    let settled = of_kinds(&server, &CREATURES).len();
    assert!((10..=13).contains(&settled), "{settled} animals");
    pump(&mut server, &mut client, 40);
    assert_eq!(of_kinds(&server, &CREATURES).len(), settled);
}
