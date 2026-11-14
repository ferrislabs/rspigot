use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rspigot_blocks::{Block, Stone};
use rspigot_core::admin::{AdminLists, ListEntry};
use rspigot_core::chunks::ChunkManagerConfig;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::entities::EntitySnapshot;
use rspigot_core::events::{ClientEvent, EntityKind, EntityStatus, Inbound, Movement, PlayerInput};
use rspigot_core::ports::{ChunkGenerator, ChunkStore, ClientLink, EntityStore, StoreError};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::player::{ClientSettings, GameProfile, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, EntityId, Identifier, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::{Compound, Tag, read_named, write_named};
use rustc_hash::FxHashMap;

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
    entities: Mutex<FxHashMap<ChunkPos, Vec<u8>>>,
}

fn encode_chunk(pos: ChunkPos, entities: &[Compound]) -> Vec<u8> {
    let root = Compound::new()
        .with("DataVersion", 5023)
        .with(
            "Entities",
            Tag::List(entities.iter().cloned().map(Tag::Compound).collect()),
        )
        .with("Position", Tag::IntArray(vec![pos.x, pos.z]));
    let mut out = Vec::new();
    write_named(&mut out, "", &root);
    out
}

impl MemoryStore {
    fn stored(&self, pos: ChunkPos) -> Option<Vec<u8>> {
        self.entities.lock().ok()?.get(&pos).cloned()
    }

    fn put(&self, pos: ChunkPos, entities: &[Compound]) {
        if let Ok(mut map) = self.entities.lock() {
            map.insert(pos, encode_chunk(pos, entities));
        }
    }
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
        let Some(bytes) = self.stored(pos) else {
            return Ok(None);
        };
        let mut input = bytes.as_slice();
        let (_, root) = read_named(&mut input).map_err(|e| StoreError::Corrupt {
            pos,
            reason: e.to_string(),
        })?;
        Ok(root
            .get_list("Entities")
            .map(|list| list.iter().filter_map(Tag::as_compound).cloned().collect()))
    }

    fn save_entities(
        &self,
        _: &Identifier,
        pos: ChunkPos,
        entities: &[Compound],
    ) -> Result<(), StoreError> {
        if let Ok(mut map) = self.entities.lock() {
            if entities.is_empty() {
                map.remove(&pos);
            } else {
                map.insert(pos, encode_chunk(pos, entities));
            }
        }
        Ok(())
    }
}

struct Flat;

impl ChunkGenerator for Flat {
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
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

fn server(store: Arc<MemoryStore>) -> Server<Link> {
    let config = GameConfig {
        view_distance: 3,
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
        LevelData::default(),
        store,
        Arc::new(Flat),
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

fn pump(server: &mut Server<Link>, clients: &mut [&mut Client], ticks: usize) {
    for _ in 0..ticks {
        server.wait_for_spawn(Duration::from_millis(20));
        server.tick();
        for client in clients.iter_mut() {
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
    pump(server, &mut [&mut client], 30);
    client.drain();
    client
}

fn walk(server: &mut Server<Link>, client: &Client, position: Vec3) {
    input(
        server,
        client.id,
        PlayerInput::Move {
            position: Some(position),
            rotation: None,
            on_ground: true,
        },
    );
}

fn command(server: &mut Server<Link>, client: &Client, line: &str) {
    input(server, client.id, PlayerInput::Command(line.to_owned()));
}

fn texts(events: &[ClientEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            ClientEvent::SystemChat { message, .. } => Some(message.plain_text()),
            _ => None,
        })
        .collect()
}

fn cows(server: &Server<Link>) -> Vec<EntitySnapshot> {
    server.entities_of(Kind::Cow)
}

fn spawned(events: &[ClientEvent], kind: Kind) -> Vec<(EntityId, Vec3)> {
    events
        .iter()
        .filter_map(|e| match e {
            ClientEvent::SpawnEntity {
                entity,
                kind: EntityKind::Other(id),
                position,
                ..
            } if *id == kind.id().0 => Some((*entity, *position)),
            _ => None,
        })
        .collect()
}

#[test]
fn summon_into_an_unloaded_chunk_succeeds_like_vanilla() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    command(&mut server, &alice, "/summon minecraft:cow 100000 64 0");
    assert!(
        texts(&alice.drain()).contains(&"commands.summon.success entity.minecraft.cow".to_owned())
    );
    let summoned = cows(&server);
    assert_eq!(summoned.len(), 1);
    assert_eq!(summoned[0].position.chunk(), ChunkPos::new(6250, 0));
}

#[test]
fn summoned_cow_falls_and_settles_on_the_floor() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    command(&mut server, &alice, "/summon minecraft:cow ~ ~3 ~2");
    let events = alice.drain();
    assert!(texts(&events).contains(&"commands.summon.success entity.minecraft.cow".to_owned()));
    let spawn = spawned(&events, Kind::Cow);
    assert_eq!(spawn.len(), 1);
    assert_eq!(spawn[0].1, Vec3::new(0.5, 67.0, 2.5));
    let cow = cows(&server)[0];
    assert_eq!(cow.health, Some(10.0));
    assert_eq!(cow.yaw, 0.0);
    let mut heights = Vec::new();
    for _ in 0..20 {
        pump(&mut server, &mut [&mut alice], 1);
        heights.push(cows(&server)[0].position.y);
    }
    let landed = heights.iter().position(|y| *y == 64.0);
    assert_eq!(landed, Some(9));
    let cow = cows(&server)[0];
    assert_eq!(cow.position.y, 64.0);
    let drift = cow.position - Vec3::new(0.5, 64.0, 2.5);
    assert!(drift.horizontal_distance_sqr() < 4.0, "{drift:?}");
    assert!(cow.on_ground);
    let events = alice.drain();
    let moves = events
        .iter()
        .filter(|e| matches!(e, ClientEvent::EntityMove { entity, .. } if *entity == cow.id))
        .count();
    assert!(moves >= 3);
    assert!(events.iter().any(|e| matches!(
        e,
        ClientEvent::EntityMove {
            movement: Movement::Sync {
                on_ground: true,
                ..
            },
            ..
        }
    )));
}

#[test]
fn nearby_players_track_the_cow_and_far_ones_pick_it_up_when_they_come_close() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    let mut bob = join(&mut server, "Bob");
    let mut carol = join(&mut server, "Carol");
    server.handle(Inbound::Console("tp Carol 200 64 200".to_owned()));
    pump(&mut server, &mut [&mut alice, &mut bob, &mut carol], 40);
    walk(&mut server, &carol, Vec3::new(200.5, 64.0, 200.5));
    walk(&mut server, &bob, Vec3::new(10.5, 64.0, 10.5));
    bob.drain();
    carol.drain();
    command(&mut server, &alice, "/summon cow 3 66 3");
    pump(&mut server, &mut [&mut alice, &mut bob, &mut carol], 12);
    let bob_events = bob.drain();
    let carol_events = carol.drain();
    let seen = spawned(&bob_events, Kind::Cow);
    assert_eq!(seen.len(), 1);
    let cow = seen[0].0;
    assert!(
        bob_events
            .iter()
            .any(|e| matches!(e, ClientEvent::EntityMove { entity, .. } if *entity == cow))
    );
    assert!(
        bob_events
            .iter()
            .any(|e| matches!(e, ClientEvent::BundleDelimiter))
    );
    assert!(
        spawned(&carol_events, Kind::Cow).is_empty(),
        "{:?} {:?}",
        server.player_location(carol.id),
        carol_events
            .iter()
            .filter(|e| matches!(
                e,
                ClientEvent::SpawnEntity { .. } | ClientEvent::Teleport { .. }
            ))
            .collect::<Vec<_>>()
    );
    server.handle(Inbound::Console("tp Carol 8 64 8".to_owned()));
    pump(&mut server, &mut [&mut alice, &mut bob, &mut carol], 40);
    walk(&mut server, &carol, Vec3::new(8.5, 64.0, 8.5));
    let carol_events = carol.drain();
    let seen = spawned(&carol_events, Kind::Cow);
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].1.y, 64.0);
    server.handle(Inbound::Console("tp Carol 300 64 300".to_owned()));
    pump(&mut server, &mut [&mut alice, &mut bob, &mut carol], 5);
    walk(&mut server, &carol, Vec3::new(300.5, 64.0, 300.5));
    assert!(
        carol
            .drain()
            .iter()
            .any(|e| matches!(e, ClientEvent::RemoveEntities(ids) if ids.contains(&cow)))
    );
}

#[test]
fn unloaded_cow_comes_back_with_the_same_uuid_and_position() {
    let store = Arc::new(MemoryStore::default());
    let mut server = server(Arc::clone(&store));
    let mut alice = join(&mut server, "Alice");
    server.handle(Inbound::Console("tp Alice 100 64 104".to_owned()));
    pump(&mut server, &mut [&mut alice], 40);
    command(
        &mut server,
        &alice,
        "/summon minecraft:cow 100 64 100 {CustomName:\"Daisy\",NoAI:1b}",
    );
    pump(&mut server, &mut [&mut alice], 5);
    let before = cows(&server)[0];
    let chunk = before.position.chunk();
    server.handle(Inbound::Console("tp Alice 2000 64 2000".to_owned()));
    pump(&mut server, &mut [&mut alice], 40);
    assert!(cows(&server).is_empty());
    server.wait_for_entity_saves();
    let saved = store.stored(chunk).expect("cow chunk saved on unload");
    let mut input = saved.as_slice();
    let (_, root) = read_named(&mut input).unwrap();
    assert_eq!(root.get_i32("DataVersion"), Some(5023));
    assert_eq!(
        root.get_int_array("Position"),
        Some(&[chunk.x, chunk.z][..])
    );
    let entities = root.get_list("Entities").unwrap();
    assert_eq!(entities.len(), 1);
    let saved_cow = entities[0].as_compound().unwrap();
    assert_eq!(saved_cow.get_str("id"), Some("minecraft:cow"));
    assert_eq!(saved_cow.get_str("CustomName"), Some("Daisy"));
    server.handle(Inbound::Console("tp Alice 100 64 104".to_owned()));
    pump(&mut server, &mut [&mut alice], 40);
    let after = cows(&server);
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].uuid, before.uuid);
    assert_eq!(after[0].position, before.position);
}

#[test]
fn entities_of_unsimulated_types_round_trip_byte_for_byte() {
    let store = Arc::new(MemoryStore::default());
    let chunk = ChunkPos::new(6, 6);
    let display = Compound::new()
        .with("id", "minecraft:block_display")
        .with(
            "Pos",
            Tag::List(vec![
                Tag::Double(100.5),
                Tag::Double(64.0),
                Tag::Double(100.5),
            ]),
        )
        .with("UUID", Tag::IntArray(vec![1, 2, 3, 4]))
        .with("ShowArms", 1i8)
        .with(
            "Pose",
            Compound::new().with("Head", Tag::List(vec![Tag::Float(1.5)])),
        )
        .with("Weird", Tag::LongArray(vec![i64::MIN, 7]))
        .with("Empty", Tag::List(Vec::new()));
    store.put(chunk, std::slice::from_ref(&display));
    let original = store.stored(chunk).unwrap();
    let mut server = server(Arc::clone(&store));
    let mut alice = join(&mut server, "Alice");
    server.handle(Inbound::Console("tp Alice 100 64 104".to_owned()));
    pump(&mut server, &mut [&mut alice], 40);
    let displays = server.entities_of(Kind::BlockDisplay);
    assert_eq!(displays.len(), 1);
    assert!(!displays[0].simulated);
    server.handle(Inbound::Console("tp Alice 2000 64 2000".to_owned()));
    pump(&mut server, &mut [&mut alice], 40);
    assert!(server.entities_of(Kind::BlockDisplay).is_empty());
    assert_eq!(store.stored(chunk), Some(original));
}

#[test]
fn kill_selector_kills_only_cows_with_a_death_animation() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    command(&mut server, &alice, "/summon cow 4 64 4");
    command(&mut server, &alice, "/summon cow 6 64 4");
    command(&mut server, &alice, "/summon zombie 8 64 4");
    pump(&mut server, &mut [&mut alice], 3);
    alice.drain();
    command(&mut server, &alice, "/kill @e[type=cow]");
    let events = alice.drain();
    assert!(texts(&events).contains(&"commands.kill.success.multiple 2".to_owned()));
    let deaths = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                ClientEvent::EntityStatus {
                    status: EntityStatus::Death,
                    ..
                }
            )
        })
        .count();
    assert_eq!(deaths, 2);
    assert_eq!(
        cows(&server)
            .iter()
            .filter(|c| c.health == Some(0.0))
            .count(),
        2
    );
    pump(&mut server, &mut [&mut alice], 30);
    assert!(cows(&server).is_empty());
    assert_eq!(server.entities_of(Kind::Zombie).len(), 1);
    let events = alice.drain();
    let removed: usize = events
        .iter()
        .map(|e| match e {
            ClientEvent::RemoveEntities(ids) => ids.len(),
            _ => 0,
        })
        .sum();
    assert_eq!(removed, 2);
    command(&mut server, &alice, "/kill @n[type=zombie]");
    assert!(
        texts(&alice.drain())
            .contains(&"commands.kill.success.single entity.minecraft.zombie".to_owned())
    );
    command(&mut server, &alice, "/summon minecraft:player");
    assert!(texts(&alice.drain()).contains(&"entity.not_summonable minecraft:player".to_owned()));
    command(&mut server, &alice, "/summon cow ~ ~ ~ {Health:");
    assert!(cows(&server).is_empty());
}

#[test]
fn teleport_and_effects_accept_entities() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    command(
        &mut server,
        &alice,
        "/summon cow 4 64 4 {NoAI:1b,Tags:[\"pet\"]}",
    );
    pump(&mut server, &mut [&mut alice], 2);
    alice.drain();
    command(&mut server, &alice, "/tp @e[tag=pet] 10 64 12");
    let events = alice.drain();
    assert!(texts(&events).iter().any(|t| t.starts_with("commands.teleport.success.location.single entity.minecraft.cow 10.500000 64.000000 12.500000")));
    assert_eq!(cows(&server)[0].position, Vec3::new(10.5, 64.0, 12.5));
    command(
        &mut server,
        &alice,
        "/effect give @e[type=cow,limit=1,sort=nearest] minecraft:speed 30 1",
    );
    assert!(
        texts(&alice.drain()).contains(
            &"commands.effect.give.success.single effect.minecraft.speed entity.minecraft.cow"
                .to_owned()
        )
    );
    pump(&mut server, &mut [&mut alice], 4);
    assert!(
        alice
            .drain()
            .iter()
            .any(|e| matches!(e, ClientEvent::Attributes { attributes, .. } if attributes.iter().any(|a| a.attribute == "minecraft:movement_speed")))
    );
    command(&mut server, &alice, "/tp @n[type=cow]");
    assert_eq!(
        server.player_location(alice.id),
        Some(Vec3::new(10.5, 64.0, 12.5))
    );
}
