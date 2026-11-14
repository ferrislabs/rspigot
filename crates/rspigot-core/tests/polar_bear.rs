use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rspigot_ai::{MobAi, MobGoal};
use rspigot_blocks::{Block, BlockKind, Stone};
use rspigot_core::admin::{AdminLists, ListEntry};
use rspigot_core::chunks::ChunkManagerConfig;
use rspigot_core::commands::Target;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, Inbound, MetadataValue, PlayerInput};
use rspigot_core::ports::{ChunkGenerator, ChunkStore, ClientLink, EntityStore, StoreError};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::player::{ClientSettings, GameProfile, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, EntityId, Identifier, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;

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
struct MemoryStore {
    entities: Mutex<HashMap<ChunkPos, Vec<Compound>>>,
}

impl ChunkStore for MemoryStore {
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

impl EntityStore for MemoryStore {
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

const POOL: ChunkPos = ChunkPos::new(0, 2);

struct Meadow;

impl ChunkGenerator for Meadow {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(0));
        let water = BlockKind::Water.default_state();
        for x in 0..16 {
            for z in 0..16 {
                let (bx, bz) = (pos.min_block_x() + x, pos.min_block_z() + z);
                let (floor, top) = if pos == POOL { (40, 55) } else { (60, 63) };
                for y in floor..=top {
                    chunk.set_block(BlockPos::new(bx, y, bz), Stone::DEFAULT_STATE);
                }
                if pos == POOL {
                    for y in 56..=63 {
                        chunk.set_block(BlockPos::new(bx, y, bz), water);
                    }
                }
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

fn server() -> Server<Link> {
    server_with(Arc::default())
}

fn server_with(store: Arc<MemoryStore>) -> Server<Link> {
    let config = GameConfig {
        view_distance: 3,
        simulation_distance: 3,
        chunk_workers: 2,
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
        ..GameConfig::default()
    };
    let mut server = Server::new(
        config,
        LevelData {
            day_time: 6000,
            daylight_cycle: false,
            ..LevelData::default()
        },
        store,
        Arc::new(Meadow),
        StatusBoard::new(20),
    );
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

struct Client {
    id: PlayerId,
    link: Link,
    events: Vec<ClientEvent>,
}

impl Client {
    fn drain(&mut self) -> Vec<ClientEvent> {
        self.events.extend(self.link.take());
        std::mem::take(&mut self.events)
    }
}

fn input(server: &mut Server<Link>, player: PlayerId, input: PlayerInput) {
    server.handle(Inbound::Input { player, input });
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
    pump(server, &mut client, 30);
    client.drain();
    client
}

fn walk(server: &mut Server<Link>, client: &Client, position: Vec3) {
    input(
        server,
        client.id,
        PlayerInput::Move {
            position: Some(position),
            rotation: Some(Rotation::new(0.0, 0.0)),
            on_ground: true,
        },
    );
}

fn command(server: &mut Server<Link>, client: &Client, line: &str) {
    input(server, client.id, PlayerInput::Command(line.to_owned()));
}

fn ai(server: &Server<Link>, id: EntityId) -> &MobAi {
    server.mob_ai(id).expect("mob has an AI")
}

fn running(server: &Server<Link>, id: EntityId, pick: fn(&MobGoal) -> bool) -> bool {
    ai(server, id)
        .goals()
        .goals()
        .iter()
        .any(|goal| goal.is_running() && pick(goal.goal()))
}

fn fresh(server: &Server<Link>, kind: Kind, known: &[EntityId]) -> EntityId {
    server
        .entities_of(kind)
        .into_iter()
        .map(|snapshot| snapshot.id)
        .find(|id| !known.contains(id))
        .expect("a new entity")
}

fn summon_with(
    server: &mut Server<Link>,
    client: &mut Client,
    kind: Kind,
    at: Vec3,
    nbt: &str,
) -> EntityId {
    let known: Vec<EntityId> = server
        .entities_of(kind)
        .into_iter()
        .map(|snapshot| snapshot.id)
        .collect();
    let line = format!("/summon {} {} {} {} {nbt}", kind.name(), at.x, at.y, at.z);
    command(server, client, &line);
    pump(server, client, 1);
    fresh(server, kind, &known)
}

fn standing_flags(events: &[ClientEvent], bear: EntityId) -> Vec<bool> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::EntityMetadata { entity, entries } if *entity == bear => entries
                .iter()
                .find(|entry| entry.index == 18)
                .and_then(|entry| match entry.value {
                    MetadataValue::Boolean(value) => Some(value),
                    _ => None,
                }),
            _ => None,
        })
        .collect()
}

fn warnings(events: &[ClientEvent]) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                ClientEvent::Sound { sound, .. } if *sound == "minecraft:entity.polar_bear.warning"
            )
        })
        .count()
}

fn uuid_ints(uuid: uuid::Uuid) -> String {
    let (high, low) = uuid.as_u64_pair();
    format!(
        "[I;{},{},{},{}]",
        (high >> 32) as i32,
        high as i32,
        (low >> 32) as i32,
        low as i32
    )
}

#[test]
fn a_bear_defending_its_cub_stands_up_warns_once_and_strikes() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    walk(&mut server, &alice, Vec3::new(0.5, 64.0, 0.5));
    let bear = summon_with(
        &mut server,
        &mut alice,
        Kind::PolarBear,
        Vec3::new(6.5, 64.0, 0.5),
        "{}",
    );
    summon_with(
        &mut server,
        &mut alice,
        Kind::PolarBear,
        Vec3::new(7.5, 64.0, 1.5),
        "{Age:-24000}",
    );
    alice.drain();
    let player = server.player_entity_id(alice.id).unwrap();
    let mut events = Vec::new();
    for _ in 0..140 {
        pump(&mut server, &mut alice, 1);
        events.extend(alice.drain());
        if server.mob_target(bear) == Some(player)
            && standing_flags(&events, bear).contains(&false)
            && standing_flags(&events, bear).contains(&true)
        {
            break;
        }
    }
    assert_eq!(server.mob_target(bear), Some(player));
    let flags = standing_flags(&events, bear);
    let up = flags.iter().position(|flag| *flag).expect("it stood up");
    assert!(flags[up..].contains(&false), "it went back down: {flags:?}");
    assert_eq!(warnings(&events), 1);
    let health = events
        .iter()
        .any(|event| matches!(event, ClientEvent::DamageEvent { entity, .. } if *entity == player));
    assert!(health, "the bear struck the player");
}

#[test]
fn an_adult_without_a_cub_around_leaves_passers_by_alone() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    walk(&mut server, &alice, Vec3::new(0.5, 64.0, 0.5));
    let bear = summon_with(
        &mut server,
        &mut alice,
        Kind::PolarBear,
        Vec3::new(5.5, 64.0, 0.5),
        "{}",
    );
    pump(&mut server, &mut alice, 200);
    assert_eq!(server.mob_target(bear), None);
}

#[test]
fn a_remembered_anger_keeps_the_bear_after_its_player_and_is_saved() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    walk(&mut server, &alice, Vec3::new(0.5, 64.0, 0.5));
    let uuid = PlayerId::offline("Alice").0;
    let nbt = format!("{{anger_end_time:9999999L,angry_at:{}}}", uuid_ints(uuid));
    let bear = summon_with(
        &mut server,
        &mut alice,
        Kind::PolarBear,
        Vec3::new(8.5, 64.0, 0.5),
        &nbt,
    );
    let player = server.player_entity_id(alice.id).unwrap();
    let mut found = false;
    for _ in 0..200 {
        pump(&mut server, &mut alice, 1);
        if server.mob_target(bear) == Some(player) {
            found = true;
            break;
        }
    }
    assert!(found, "the angry bear never went for its player");
    pump(&mut server, &mut alice, 10);
    let saved = server.saved_entity(bear).expect("the bear is saved");
    let end = saved.get_i64("anger_end_time").unwrap();
    assert!((1..9_999_999).contains(&end), "{end}");
    assert!(saved.get("angry_at").is_some());
}

#[test]
fn only_a_hurt_cub_panics_when_a_mob_bites() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    walk(&mut server, &alice, Vec3::new(0.5, 64.0, -20.5));
    let adult = summon_with(
        &mut server,
        &mut alice,
        Kind::PolarBear,
        Vec3::new(4.5, 64.0, 4.5),
        "{NoAI:0b}",
    );
    let cub = summon_with(
        &mut server,
        &mut alice,
        Kind::PolarBear,
        Vec3::new(-4.5, 64.0, 4.5),
        "{Age:-24000}",
    );
    let zombie = summon_with(
        &mut server,
        &mut alice,
        Kind::Zombie,
        Vec3::new(0.5, 64.0, 4.5),
        "{NoAI:1b}",
    );
    pump(&mut server, &mut alice, 4);
    assert!(server.do_hurt_target(zombie, Target::Entity(adult)));
    assert!(server.do_hurt_target(zombie, Target::Entity(cub)));
    let mut adult_panicked = false;
    let mut cub_panicked = false;
    for _ in 0..8 {
        pump(&mut server, &mut alice, 1);
        adult_panicked |= running(&server, adult, |goal| matches!(goal, MobGoal::Panic(_)));
        cub_panicked |= running(&server, cub, |goal| matches!(goal, MobGoal::Panic(_)));
    }
    assert!(cub_panicked);
    assert!(!adult_panicked);
}
