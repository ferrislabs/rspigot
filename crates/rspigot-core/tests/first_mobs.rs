use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rspigot_blocks::{Block, Lava, Stone, Water};
use rspigot_core::admin::{AdminLists, ListEntry};
use rspigot_core::chunks::ChunkManagerConfig;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{
    ClientEvent, Direction, EntityKind, EntityStatus, EquipmentSlot as WireSlot, Hand, Inbound,
    MetadataValue, PlayerInput, VariantKind,
};
use rspigot_core::ports::{ChunkGenerator, ChunkStore, ClientLink, EntityStore, StoreError};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_data::variants::Farm;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::damage::DamageType;
use rspigot_domain::item::{ItemStack, Slot};
use rspigot_domain::player::{ClientSettings, GameProfile, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::world::Dimension;
use rspigot_domain::{
    BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Identifier, Rotation, Vec3,
};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::ItemKind;
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

impl MemoryStore {
    fn stored(&self, pos: ChunkPos) -> Option<Vec<u8>> {
        self.entities.lock().ok()?.get(&pos).cloned()
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
                let root = Compound::new()
                    .with("DataVersion", 5023)
                    .with(
                        "Entities",
                        Tag::List(entities.iter().cloned().map(Tag::Compound).collect()),
                    )
                    .with("Position", Tag::IntArray(vec![pos.x, pos.z]));
                let mut out = Vec::new();
                write_named(&mut out, "", &root);
                map.insert(pos, out);
            }
        }
        Ok(())
    }
}

struct Flat;

fn feature(at: BlockPos) -> BlockStateId {
    let pool = (-6..=-4).contains(&at.x) && (-6..=-4).contains(&at.z) && at.y >= 61;
    match (at.x, at.y, at.z) {
        (3, 63, 0) => Lava::DEFAULT_STATE,
        (6, 63, 0) => Water::DEFAULT_STATE,
        _ if pool => Water::DEFAULT_STATE,
        _ => Stone::DEFAULT_STATE,
    }
}

impl ChunkGenerator for Flat {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(0));
        for x in 0..16 {
            for z in 0..16 {
                for y in 60..=63 {
                    let at = BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z);
                    chunk.set_block(at, feature(at));
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
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        chunks: ChunkManagerConfig {
            unload_delay_ticks: 5,
            autosave_interval_ticks: 0,
        },
        admin: AdminLists {
            operators: vec![ListEntry::named("Alice"), ListEntry::named("Bob")],
            whitelist: Vec::new(),
            whitelist_enabled: false,
            ..AdminLists::default()
        },
        ..GameConfig::default()
    };
    let level = LevelData {
        spawn_mobs: false,
        day_time: 1000,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        level,
        Arc::new(MemoryStore::default()),
        Arc::new(Flat),
        StatusBoard::new(20),
    )
    .with_rng_seed(0);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

struct Client {
    id: PlayerId,
    link: Link,
    events: Vec<ClientEvent>,
    slots: Vec<Slot>,
}

impl Client {
    fn drain(&mut self) -> Vec<ClientEvent> {
        self.events.extend(self.link.take());
        std::mem::take(&mut self.events)
    }

    fn absorb(&mut self, events: &[ClientEvent]) {
        for event in events {
            match event {
                ClientEvent::InventoryContent { slots, .. } => self.slots = slots.clone(),
                ClientEvent::InventorySlot { slot, stack, .. } => {
                    if let Ok(index) = usize::try_from(*slot) {
                        self.slots[index] = stack.clone();
                    }
                }
                ClientEvent::PlayerInventorySlot { slot, stack } => {
                    let menu = match *slot {
                        0..=8 => 36 + *slot as usize,
                        40 => 45,
                        other => other as usize,
                    };
                    self.slots[menu] = stack.clone();
                }
                _ => {}
            }
        }
    }

    fn hand(&self) -> Slot {
        self.slots.get(36).cloned().flatten()
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
        client.absorb(&events);
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
        slots: vec![None; 46],
    };
    let events = client.link.take();
    client.absorb(&events);
    for event in events {
        if let ClientEvent::Teleport { id: teleport, .. } = event {
            input(server, id, PlayerInput::TeleportConfirm(teleport));
        }
    }
    input(server, id, PlayerInput::Loaded);
    pump(server, &mut client, 30);
    stand(server, &client, Vec3::new(0.5, 64.0, 0.5));
    pump(server, &mut client, 2);
    client
}

fn stand(server: &mut Server<Link>, client: &Client, position: Vec3) {
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

fn command(server: &mut Server<Link>, client: &mut Client, line: &str) {
    input(server, client.id, PlayerInput::Command(line.to_owned()));
    pump(server, client, 2);
}

fn summon(server: &mut Server<Link>, client: &mut Client, line: &str, kind: Kind) -> EntityId {
    let before: Vec<EntityId> = server.entities_of(kind).iter().map(|e| e.id).collect();
    command(server, client, line);
    let added: Vec<EntityId> = server
        .entities_of(kind)
        .iter()
        .map(|e| e.id)
        .filter(|id| !before.contains(id))
        .collect();
    assert_eq!(added.len(), 1, "{line}");
    added[0]
}

fn interact(server: &mut Server<Link>, client: &mut Client, target: EntityId) {
    input(
        server,
        client.id,
        PlayerInput::Interact {
            target,
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::ZERO,
        },
    );
    pump(server, client, 1);
}

fn sounds(events: &[ClientEvent]) -> Vec<&'static str> {
    events
        .iter()
        .filter_map(|e| match e {
            ClientEvent::Sound { sound, .. } => Some(*sound),
            _ => None,
        })
        .collect()
}

fn metadata(events: &[ClientEvent], entity: EntityId, index: u8) -> Vec<MetadataValue> {
    events
        .iter()
        .filter_map(|e| match e {
            ClientEvent::EntityMetadata {
                entity: id,
                entries,
            } if *id == entity => Some(entries),
            _ => None,
        })
        .flatten()
        .filter(|entry| entry.index == index)
        .map(|entry| entry.value.clone())
        .collect()
}

fn entity_events(events: &[ClientEvent], entity: EntityId) -> Vec<EntityStatus> {
    events
        .iter()
        .filter_map(|e| match e {
            ClientEvent::EntityStatus { entity: id, status } if *id == entity => Some(*status),
            _ => None,
        })
        .collect()
}

fn spawned(events: &[ClientEvent], kind: Kind) -> Vec<(EntityId, i32)> {
    events
        .iter()
        .filter_map(|e| match e {
            ClientEvent::SpawnEntity {
                entity,
                kind: EntityKind::Other(id),
                data,
                ..
            } if *id == kind.id().0 => Some((*entity, *data)),
            _ => None,
        })
        .collect()
}

fn saved(server: &Server<Link>, id: EntityId) -> Compound {
    server.saved_entity(id).expect("entity is gone")
}

fn ground(server: &Server<Link>, item: ItemKind) -> u32 {
    server
        .ground_items()
        .iter()
        .filter(|(_, stack)| stack.item == item.id())
        .map(|(_, stack)| u32::from(stack.count.get()))
        .sum()
}

fn holding(client: &Client, item: ItemKind) -> u8 {
    client
        .hand()
        .filter(|stack| stack.item == item.id())
        .map_or(0, |stack| stack.count.get())
}

fn give(server: &mut Server<Link>, client: &mut Client, item: &str, count: u8) {
    command(server, client, &format!("/give @s {item} {count}"));
    pump(server, client, 3);
}

#[test]
fn cows_fed_with_wheat_fall_in_love_and_breed_a_calf() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let first = summon(
        &mut server,
        &mut alice,
        "/summon cow 0.5 64 2.5 {}",
        Kind::Cow,
    );
    let second = summon(
        &mut server,
        &mut alice,
        "/summon cow 1.5 64 2.5 {}",
        Kind::Cow,
    );
    give(&mut server, &mut alice, "minecraft:wheat", 3);
    alice.drain();
    interact(&mut server, &mut alice, first);
    interact(&mut server, &mut alice, second);
    let events = alice.drain();
    assert_eq!(entity_events(&events, first), vec![EntityStatus::Event(18)]);
    assert_eq!(
        entity_events(&events, second),
        vec![EntityStatus::Event(18)]
    );
    assert_eq!(holding(&alice, ItemKind::Wheat), 1);
    assert!(
        saved(&server, first)
            .get_i32("InLove")
            .is_some_and(|love| (595..600).contains(&love))
    );
    let mut calf = None;
    for _ in 0..200 {
        pump(&mut server, &mut alice, 1);
        if let Some(id) = server
            .entities_of(Kind::Cow)
            .iter()
            .map(|e| e.id)
            .find(|id| *id != first && *id != second)
        {
            calf = Some(id);
            break;
        }
    }
    let calf = calf.expect("no calf was born");
    pump(&mut server, &mut alice, 2);
    let events = alice.drain();
    assert!(
        spawned(&events, Kind::Cow)
            .iter()
            .any(|(id, _)| *id == calf)
    );
    assert!(metadata(&events, calf, 16).contains(&MetadataValue::Boolean(true)));
    assert!(
        saved(&server, calf)
            .get_i32("Age")
            .is_some_and(|age| age < -23_990)
    );
    assert!(
        saved(&server, first)
            .get_i32("Age")
            .is_some_and(|age| age > 5_990)
    );
    assert_eq!(saved(&server, first).get_i32("InLove"), Some(0));
    assert!(
        !server.experience_orbs().is_empty()
            || server
                .player_experience(alice.id)
                .is_some_and(|(_, _, total)| total > 0)
    );
    interact(&mut server, &mut alice, first);
    assert!(entity_events(&alice.drain(), first).is_empty());
    assert_eq!(holding(&alice, ItemKind::Wheat), 1);
}

#[test]
fn feeding_a_calf_ages_it_up_by_a_tenth() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let calf = summon(
        &mut server,
        &mut alice,
        "/summon cow 0.5 64 2.5 {NoAI:1b,Age:-24000}",
        Kind::Cow,
    );
    give(&mut server, &mut alice, "minecraft:wheat", 1);
    let before = saved(&server, calf).get_i32("Age").unwrap();
    interact(&mut server, &mut alice, calf);
    let bonus = ((-before / 20) as f32 * 0.1) as i32 * 20;
    let nbt = saved(&server, calf);
    assert_eq!(nbt.get_i32("ForcedAge"), Some(bonus));
    let age = nbt.get_i32("Age").unwrap();
    assert!(
        (before + bonus..=before + bonus + 1).contains(&age),
        "{before} {age}"
    );
    assert_eq!(holding(&alice, ItemKind::Wheat), 0);
}

#[test]
fn milking_a_cow_fills_the_bucket() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let mut bob = join(&mut server, "Bob");
    let cow = summon(
        &mut server,
        &mut alice,
        "/summon cow 0.5 64 2.5 {NoAI:1b}",
        Kind::Cow,
    );
    give(&mut server, &mut alice, "minecraft:bucket", 1);
    alice.drain();
    bob.drain();
    interact(&mut server, &mut alice, cow);
    pump(&mut server, &mut bob, 1);
    assert!(sounds(&bob.drain()).contains(&"minecraft:entity.cow.milk"));
    assert!(!sounds(&alice.drain()).contains(&"minecraft:entity.cow.milk"));
    assert_eq!(holding(&alice, ItemKind::MilkBucket), 1);
}

#[test]
fn shearing_drops_wool_and_marks_the_sheep() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let sheep = summon(
        &mut server,
        &mut alice,
        "/summon sheep 0.5 64 2.5 {NoAI:1b,Color:14b}",
        Kind::Sheep,
    );
    give(&mut server, &mut alice, "minecraft:shears", 1);
    alice.drain();
    interact(&mut server, &mut alice, sheep);
    pump(&mut server, &mut alice, 1);
    let events = alice.drain();
    assert!(sounds(&events).contains(&"minecraft:entity.sheep.shear"));
    assert_eq!(
        metadata(&events, sheep, 18),
        vec![MetadataValue::Byte(0x10 | 14)]
    );
    let wool = ground(&server, ItemKind::RedWool);
    assert!((1..=3).contains(&wool), "{wool}");
    assert_eq!(saved(&server, sheep).get_bool("Sheared"), Some(true));
    let shears = alice.hand().unwrap();
    assert_eq!(
        shears
            .components
            .get("minecraft:damage")
            .and_then(Tag::as_i64),
        Some(1)
    );
    interact(&mut server, &mut alice, sheep);
    pump(&mut server, &mut alice, 1);
    assert_eq!(ground(&server, ItemKind::RedWool), wool);
}

#[test]
fn dye_recolours_a_sheep() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let sheep = summon(
        &mut server,
        &mut alice,
        "/summon sheep 0.5 64 2.5 {NoAI:1b}",
        Kind::Sheep,
    );
    give(&mut server, &mut alice, "minecraft:blue_dye", 1);
    alice.drain();
    interact(&mut server, &mut alice, sheep);
    pump(&mut server, &mut alice, 1);
    let events = alice.drain();
    assert_eq!(metadata(&events, sheep, 18), vec![MetadataValue::Byte(11)]);
    assert_eq!(saved(&server, sheep).get_i32("Color"), Some(11));
    assert_eq!(holding(&alice, ItemKind::BlueDye), 0);
}

#[test]
fn a_saddled_pig_shows_its_saddle_and_drops_it_on_death() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let pig = summon(
        &mut server,
        &mut alice,
        "/summon pig 0.5 64 2.5 {NoAI:1b}",
        Kind::Pig,
    );
    give(&mut server, &mut alice, "minecraft:saddle", 1);
    alice.drain();
    interact(&mut server, &mut alice, pig);
    pump(&mut server, &mut alice, 1);
    let events = alice.drain();
    let saddled = events.iter().any(|e| {
        matches!(e, ClientEvent::EntityEquipment { entity, slots }
            if *entity == pig
                && slots.iter().any(|(slot, stack)| *slot == WireSlot::Saddle
                    && stack.as_ref().is_some_and(|s| s.item == ItemKind::Saddle.id())))
    });
    assert!(saddled);
    assert!(sounds(&events).contains(&"minecraft:entity.pig.saddle"));
    assert_eq!(holding(&alice, ItemKind::Saddle), 0);
    command(&mut server, &mut alice, "/kill @e[type=pig]");
    assert_eq!(ground(&server, ItemKind::Saddle), 1);
}

#[test]
fn a_spawn_egg_used_on_an_adult_spawns_a_baby() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let cow = summon(
        &mut server,
        &mut alice,
        "/summon cow 0.5 64 2.5 {NoAI:1b}",
        Kind::Cow,
    );
    give(&mut server, &mut alice, "minecraft:cow_spawn_egg", 2);
    alice.drain();
    interact(&mut server, &mut alice, cow);
    pump(&mut server, &mut alice, 1);
    let cows = server.entities_of(Kind::Cow);
    assert_eq!(cows.len(), 2);
    let baby = cows.iter().map(|e| e.id).find(|id| *id != cow).unwrap();
    assert_eq!(
        saved(&server, baby).get_i32("Age").map(|age| age < -23_990),
        Some(true)
    );
    let events = alice.drain();
    assert!(metadata(&events, baby, 16).contains(&MetadataValue::Boolean(true)));
    assert_eq!(holding(&alice, ItemKind::CowSpawnEgg), 1);
}

#[test]
fn a_spawn_egg_used_on_a_block_spawns_the_mob_above_it() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    give(&mut server, &mut alice, "minecraft:cow_spawn_egg", 2);
    alice.drain();
    input(
        &mut server,
        alice.id,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: BlockPos::new(0, 63, 2),
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 1,
        },
    );
    pump(&mut server, &mut alice, 2);
    let cows = server.entities_of(Kind::Cow);
    assert_eq!(cows.len(), 1);
    assert_eq!(cows[0].position, Vec3::new(0.5, 64.0, 2.5));
    assert_eq!(holding(&alice, ItemKind::CowSpawnEgg), 1);
}

#[test]
fn a_name_tag_names_the_mob_and_makes_it_persistent() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let zombie = summon(
        &mut server,
        &mut alice,
        "/summon zombie 0.5 64 2.5 {NoAI:1b}",
        Kind::Zombie,
    );
    assert_eq!(
        saved(&server, zombie).get_bool("PersistenceRequired"),
        Some(false)
    );
    command(&mut server, &mut alice, "/gamemode creative");
    let mut tag = ItemStack::new(ItemKind::NameTag.id(), std::num::NonZeroU8::MIN);
    tag.components
        .insert("minecraft:custom_name", Tag::String("Bob".to_owned()));
    input(
        &mut server,
        alice.id,
        PlayerInput::CreativeSlot {
            slot: 36,
            stack: Some(tag),
        },
    );
    pump(&mut server, &mut alice, 2);
    interact(&mut server, &mut alice, zombie);
    let nbt = saved(&server, zombie);
    assert_eq!(nbt.get_bool("PersistenceRequired"), Some(true));
    assert!(nbt.get("CustomName").is_some());
}

#[test]
fn flint_and_steel_ignites_a_creeper_which_explodes() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let mut bob = join(&mut server, "Bob");
    let creeper = summon(
        &mut server,
        &mut alice,
        "/summon creeper 0.5 64 3.5 {NoAI:1b}",
        Kind::Creeper,
    );
    give(&mut server, &mut alice, "minecraft:flint_and_steel", 1);
    alice.drain();
    bob.drain();
    interact(&mut server, &mut alice, creeper);
    pump(&mut server, &mut bob, 1);
    assert!(sounds(&bob.drain()).contains(&"minecraft:item.flintandsteel.use"));
    let ignition = alice.drain();
    assert_eq!(
        metadata(&ignition, creeper, 18),
        vec![MetadataValue::Boolean(true)]
    );
    let mut exploded = None;
    for tick in 0..40 {
        pump(&mut server, &mut alice, 1);
        if server.entities_of(Kind::Creeper).is_empty() {
            exploded = Some(tick);
            break;
        }
    }
    assert!(exploded.is_some(), "the creeper never exploded");
    let events = alice.drain();
    assert!(
        metadata(&ignition, creeper, 16)
            .iter()
            .chain(&metadata(&events, creeper, 16))
            .any(|value| *value == MetadataValue::VarInt(1))
    );
    let explosion = events.iter().find_map(|e| match e {
        ClientEvent::Explode {
            center,
            radius,
            block_count,
            ..
        } => Some((*center, *radius, *block_count)),
        _ => None,
    });
    let (center, radius, block_count) = explosion.expect("no explode packet");
    assert_eq!(radius, 3.0);
    assert!(block_count > 0);
    assert!((center.y - 64.0).abs() < 0.5);
    let broken = events
        .iter()
        .filter(|e| matches!(e, ClientEvent::BlockUpdate { state, .. } if state.is_air()))
        .count();
    assert!(broken > 0);
    let hurt = events.iter().any(|e| {
        matches!(
            e,
            ClientEvent::DamageEvent {
                kind: DamageType::PlayerExplosion | DamageType::Explosion,
                ..
            }
        )
    });
    assert!(hurt);
    assert!(sounds(&ignition).contains(&"minecraft:entity.creeper.primed"));
}

#[test]
fn a_skeleton_shoots_arrows_that_hurt_the_player() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    command(&mut server, &mut alice, "/time set 18000");
    let skeleton = summon(
        &mut server,
        &mut alice,
        "/summon skeleton 0.5 64 9.5 {equipment:{mainhand:{id:\"minecraft:bow\",count:1}}}",
        Kind::Skeleton,
    );
    let player = server.player_entity_id(alice.id).unwrap();
    let mut arrow = None;
    let mut hit = false;
    for _ in 0..300 {
        pump(&mut server, &mut alice, 1);
        let events = alice.drain();
        if arrow.is_none() {
            arrow = spawned(&events, Kind::Arrow).first().copied();
        }
        hit |= events.iter().any(|e| {
            matches!(e, ClientEvent::DamageEvent { entity, kind: DamageType::Arrow, cause, .. }
                if *entity == player && *cause == Some(skeleton))
        });
        if hit {
            break;
        }
        stand(&mut server, &alice, Vec3::new(0.5, 64.0, 0.5));
    }
    let (_, owner) = arrow.expect("the skeleton never shot");
    assert_eq!(owner, skeleton.0);
    assert!(hit, "no arrow hit the player");
    assert!(server.player_health(alice.id).unwrap() < 20.0);
}

#[test]
fn arrows_stick_in_the_ground_can_be_picked_up_and_despawn() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let pickable = summon(
        &mut server,
        &mut alice,
        "/summon arrow 4.5 67 4.5 {pickup:1b}",
        Kind::Arrow,
    );
    let stuck = summon(
        &mut server,
        &mut alice,
        "/summon arrow -6.5 67 -6.5 {pickup:0}",
        Kind::Arrow,
    );
    pump(&mut server, &mut alice, 40);
    let arrows = server.arrows();
    assert!(
        arrows
            .iter()
            .all(|(_, position, in_ground)| *in_ground && position.y < 64.1)
    );
    let nbt = saved(&server, stuck);
    assert_eq!(nbt.get_bool("inGround"), Some(true));
    assert_eq!(nbt.get_i32("pickup"), Some(0));
    alice.drain();
    stand(&mut server, &alice, Vec3::new(4.5, 64.0, 4.5));
    pump(&mut server, &mut alice, 5);
    let events = alice.drain();
    assert!(
        events.iter().any(
            |e| matches!(e, ClientEvent::ItemPickup { collected, .. } if *collected == pickable)
        )
    );
    assert_eq!(holding(&alice, ItemKind::Arrow), 1);
    stand(&mut server, &alice, Vec3::new(-6.5, 64.0, -6.5));
    pump(&mut server, &mut alice, 5);
    assert_eq!(server.arrows().len(), 1);
    stand(&mut server, &alice, Vec3::new(0.5, 64.0, 0.5));
    pump(&mut server, &mut alice, 1200);
    assert!(server.arrows().is_empty());
}

#[test]
fn zombies_burn_in_daylight_unless_wearing_a_helmet() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let bare = summon(
        &mut server,
        &mut alice,
        "/summon zombie 4.5 64 4.5 {NoAI:1b}",
        Kind::Zombie,
    );
    let helmeted = summon(
        &mut server,
        &mut alice,
        "/summon zombie -3.5 64 4.5 {NoAI:1b,equipment:{head:{id:\"minecraft:leather_helmet\",count:1}}}",
        Kind::Zombie,
    );
    let mut burning = false;
    for _ in 0..200 {
        pump(&mut server, &mut alice, 1);
        if server
            .entity_fire_ticks(bare)
            .is_some_and(|ticks| ticks > 0)
        {
            burning = true;
            break;
        }
    }
    assert!(burning, "the zombie never caught fire");
    pump(&mut server, &mut alice, 25);
    let events = alice.drain();
    assert!(
        metadata(&events, bare, 0)
            .iter()
            .any(|value| matches!(value, MetadataValue::Byte(flags) if flags & 1 == 1))
    );
    assert!(events.iter().any(|e| matches!(e, ClientEvent::DamageEvent { entity, kind: DamageType::OnFire, .. } if *entity == bare)));
    assert_eq!(server.entity_fire_ticks(helmeted), Some(0));
}

#[test]
fn lava_sets_players_on_fire_and_water_puts_them_out() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    alice.drain();
    stand(&mut server, &alice, Vec3::new(3.5, 63.0, 0.5));
    pump(&mut server, &mut alice, 3);
    assert!(
        server
            .player_fire_ticks(alice.id)
            .is_some_and(|ticks| ticks > 0)
    );
    let events = alice.drain();
    let player = server.player_entity_id(alice.id).unwrap();
    assert!(events.iter().any(|e| matches!(e, ClientEvent::DamageEvent { entity, kind: DamageType::Lava, .. } if *entity == player)));
    stand(&mut server, &alice, Vec3::new(6.5, 63.0, 0.5));
    pump(&mut server, &mut alice, 2);
    assert!(
        server
            .player_fire_ticks(alice.id)
            .is_some_and(|ticks| ticks <= 0)
    );
    assert!(sounds(&alice.drain()).contains(&"minecraft:entity.generic.extinguish_fire"));
}

#[test]
fn a_drowning_zombie_converts_into_a_drowned() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    command(&mut server, &mut alice, "/time set 18000");
    command(&mut server, &mut alice, "/gamemode creative");
    let zombie = summon(
        &mut server,
        &mut alice,
        "/summon zombie -5 61 -5 {}",
        Kind::Zombie,
    );
    let mut converted = false;
    for _ in 0..1000 {
        pump(&mut server, &mut alice, 1);
        if !server.entities_of(Kind::Drowned).is_empty() {
            converted = true;
            break;
        }
    }
    assert!(converted, "the zombie never became a drowned");
    assert!(server.saved_entity(zombie).is_none());
    let events = alice.drain();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ClientEvent::LevelEvent { event: 1040, .. }))
    );
}

#[test]
fn summoned_farm_animals_get_their_biome_variant_and_pair_it() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let cow = summon(&mut server, &mut alice, "/summon cow 0.5 64 2.5", Kind::Cow);
    let sheep = summon(
        &mut server,
        &mut alice,
        "/summon sheep 2.5 64 2.5",
        Kind::Sheep,
    );
    let variant = saved(&server, cow)
        .get_str("variant")
        .map(str::to_owned)
        .unwrap();
    let biome =
        rspigot_data::registries::static_index("minecraft:worldgen/biome", "minecraft:badlands");
    assert!(biome.is_none() || biome == Some(0));
    assert_eq!(variant, "minecraft:warm");
    let colour = saved(&server, sheep).get_i32("Color").unwrap();
    let mut bob = join(&mut server, "Bob");
    let events = bob.drain();
    assert!(
        metadata(&events, cow, 18)
            .contains(&MetadataValue::Variant(VariantKind::Mob(Farm::Cow), 2))
    );
    if colour != 0 {
        assert!(metadata(&events, sheep, 18).contains(&MetadataValue::Byte(colour as i8)));
    }
}

#[test]
fn an_ignited_creeper_out_of_activation_range_still_explodes() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    summon(
        &mut server,
        &mut alice,
        "/summon creeper 36.5 64 0.5 {ignited:1b}",
        Kind::Creeper,
    );
    for _ in 0..120 {
        pump(&mut server, &mut alice, 1);
        if server.entities_of(Kind::Creeper).is_empty() {
            break;
        }
    }
    assert!(server.entities_of(Kind::Creeper).is_empty());
    assert!(
        alice
            .drain()
            .iter()
            .any(|e| matches!(e, ClientEvent::Explode { .. }))
    );
}
