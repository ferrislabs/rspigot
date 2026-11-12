use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rspigot_blocks::{Block, Lava, Stone, Water};
use rspigot_core::admin::{AdminLists, ListEntry};
use rspigot_core::chunks::ChunkManagerConfig;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{
    ClientEvent, EntityStatus, Hand, Inbound, MetadataValue, PlayerInput, VariantKind,
};
use rspigot_core::ports::{ChunkGenerator, ChunkStore, ClientLink, EntityStore, StoreError};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_data::variants::VariantFamily;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::item::Slot;
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

fn saved(server: &Server<Link>, id: EntityId) -> Compound {
    server.saved_entity(id).expect("entity is gone")
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

fn owner_tag() -> String {
    let id = PlayerId::offline("Alice").0.as_u128();
    let parts: Vec<i32> = (0..4)
        .map(|i| ((id >> (96 - 32 * i)) & 0xFFFF_FFFF) as u32 as i32)
        .collect();
    format!("[I;{},{},{},{}]", parts[0], parts[1], parts[2], parts[3])
}

fn tame_cat(server: &mut Server<Link>, alice: &mut Client, extra: &str) -> EntityId {
    summon(
        server,
        alice,
        &format!(
            "/summon cat 0.5 64 2.5 {{NoAI:1b,Owner:{}{extra}}}",
            owner_tag()
        ),
        Kind::Cat,
    )
}

fn hold(server: &mut Server<Link>, alice: &mut Client, item: &str, count: u8) {
    give(server, alice, item, count);
    alice.drain();
}

fn cat_sounds(events: &[ClientEvent]) -> Vec<&'static str> {
    sounds(events)
        .into_iter()
        .filter(|sound| sound.contains("cat"))
        .collect()
}

#[test]
fn feeding_a_wild_cat_cod_tames_it_one_time_in_three_with_hearts_or_smoke() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let cat = summon(
        &mut server,
        &mut alice,
        "/summon cat 0.5 64 2.5 {NoAI:1b}",
        Kind::Cat,
    );
    hold(&mut server, &mut alice, "minecraft:cod", 64);
    let mut failures = 0;
    let mut tamed = false;
    let mut fed = 0;
    for _ in 0..60 {
        interact(&mut server, &mut alice, cat);
        fed += 1;
        let events = alice.drain();
        assert!(cat_sounds(&events).contains(&"minecraft:entity.cat.eat"));
        let statuses = entity_events(&events, cat);
        if statuses.contains(&EntityStatus::Event(7)) {
            tamed = true;
            break;
        }
        assert!(statuses.contains(&EntityStatus::Event(6)));
        failures += 1;
    }
    assert!(tamed, "never tamed after {fed} fish");
    assert!(failures > 0 || fed == 1);
    assert_eq!(holding(&alice, ItemKind::Cod), 64 - fed);
    let nbt = saved(&server, cat);
    assert_eq!(nbt.get_bool("Sitting"), Some(true));
    assert!(nbt.get("Owner").is_some());
    assert_eq!(nbt.get_bool("PersistenceRequired"), Some(true));
}

#[test]
fn a_non_food_item_does_not_tame_and_is_not_consumed() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let cat = summon(
        &mut server,
        &mut alice,
        "/summon cat 0.5 64 2.5 {NoAI:1b}",
        Kind::Cat,
    );
    hold(&mut server, &mut alice, "minecraft:bone", 3);
    interact(&mut server, &mut alice, cat);
    let events = alice.drain();
    assert!(entity_events(&events, cat).is_empty());
    assert_eq!(holding(&alice, ItemKind::Bone), 3);
    assert!(saved(&server, cat).get("Owner").is_none());
}

#[test]
fn the_owner_recolours_the_collar_with_a_dye_other_than_the_current_one() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let cat = tame_cat(&mut server, &mut alice, ",Sitting:1b");
    hold(&mut server, &mut alice, "minecraft:blue_dye", 2);
    interact(&mut server, &mut alice, cat);
    let events = alice.drain();
    assert_eq!(metadata(&events, cat, 23), vec![MetadataValue::VarInt(11)]);
    assert_eq!(saved(&server, cat).get_i64("CollarColor"), Some(11));
    assert_eq!(holding(&alice, ItemKind::BlueDye), 1);
    assert_eq!(
        saved(&server, cat).get_bool("PersistenceRequired"),
        Some(true)
    );
    interact(&mut server, &mut alice, cat);
    assert_eq!(holding(&alice, ItemKind::BlueDye), 1);
    assert_eq!(saved(&server, cat).get_bool("Sitting"), Some(false));
}

#[test]
fn feeding_the_owners_hurt_cat_heals_by_the_nutrition_and_a_healthy_one_falls_in_love() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let cat = tame_cat(&mut server, &mut alice, ",Health:4f,Sitting:1b");
    hold(&mut server, &mut alice, "minecraft:cod", 5);
    interact(&mut server, &mut alice, cat);
    let events = alice.drain();
    assert!(cat_sounds(&events).contains(&"minecraft:entity.cat.eat"));
    assert_eq!(saved(&server, cat).get_f64("Health"), Some(6.0));
    assert_eq!(holding(&alice, ItemKind::Cod), 4);
    let healthy = tame_cat(&mut server, &mut alice, ",Sitting:1b");
    alice.drain();
    interact(&mut server, &mut alice, healthy);
    let events = alice.drain();
    assert_eq!(holding(&alice, ItemKind::Cod), 3);
    assert!(entity_events(&events, healthy).contains(&EntityStatus::Event(18)));
    assert_eq!(saved(&server, healthy).get_bool("Sitting"), Some(true));
}

#[test]
fn an_empty_hand_toggles_sitting_for_the_owner_only() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let cat = tame_cat(&mut server, &mut alice, ",Sitting:1b");
    let stranger = summon(
        &mut server,
        &mut alice,
        "/summon cat 1.5 64 2.5 {NoAI:1b,Sitting:1b,Owner:[I;1,2,3,4]}",
        Kind::Cat,
    );
    alice.drain();
    interact(&mut server, &mut alice, cat);
    assert_eq!(saved(&server, cat).get_bool("Sitting"), Some(false));
    interact(&mut server, &mut alice, cat);
    assert_eq!(saved(&server, cat).get_bool("Sitting"), Some(true));
    interact(&mut server, &mut alice, stranger);
    assert_eq!(saved(&server, stranger).get_bool("Sitting"), Some(true));
}

#[test]
fn a_cats_variant_sound_variant_and_collar_survive_a_save_and_are_synchronised() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let cat = summon(
        &mut server,
        &mut alice,
        "/summon cat 2.5 64 2.5 {NoAI:1b,variant:\"minecraft:calico\",sound_variant:\"minecraft:royal\",CollarColor:3b}",
        Kind::Cat,
    );
    let nbt = saved(&server, cat);
    assert_eq!(nbt.get_str("variant"), Some("minecraft:calico"));
    assert_eq!(nbt.get_str("sound_variant"), Some("minecraft:royal"));
    assert_eq!(nbt.get_i64("CollarColor"), Some(3));
    let mut bob = join(&mut server, "Bob");
    let events = bob.drain();
    assert!(metadata(&events, cat, 20).contains(&MetadataValue::Variant(
        VariantKind::Mob(VariantFamily::Cat),
        3
    )));
    assert!(metadata(&events, cat, 23).contains(&MetadataValue::VarInt(3)));
}

#[test]
fn a_fresh_cat_gets_a_spawn_variant_and_a_sound_variant() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let cat = summon(
        &mut server,
        &mut alice,
        "/summon cat 2.5 64 2.5 {NoAI:1b}",
        Kind::Cat,
    );
    let nbt = saved(&server, cat);
    assert!(nbt.get_str("variant").is_some());
    assert!(nbt.get_str("sound_variant").is_some());
    assert_eq!(nbt.get_i64("CollarColor"), Some(14));
}

#[test]
fn two_fed_tame_cats_breed_a_tame_kitten_with_a_parent_variant_and_a_mixed_collar() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let owner = owner_tag();
    let summon_cat = |server: &mut Server<Link>, alice: &mut Client, x: f64, variant: &str| {
        summon(
            server,
            alice,
            &format!(
                "/summon cat {x} 64 2.5 {{Owner:{owner},variant:\"minecraft:{variant}\",CollarColor:14b}}"
            ),
            Kind::Cat,
        )
    };
    let first = summon_cat(&mut server, &mut alice, 0.5, "calico");
    let second = summon_cat(&mut server, &mut alice, 2.5, "red");
    hold(&mut server, &mut alice, "minecraft:cod", 4);
    interact(&mut server, &mut alice, first);
    interact(&mut server, &mut alice, second);
    let mut kitten = None;
    for _ in 0..400 {
        pump(&mut server, &mut alice, 1);
        if let Some(id) = server
            .entities_of(Kind::Cat)
            .iter()
            .map(|e| e.id)
            .find(|id| *id != first && *id != second)
        {
            kitten = Some(id);
            break;
        }
    }
    let kitten = kitten.expect("no kitten was born");
    let nbt = saved(&server, kitten);
    assert!(nbt.get("Owner").is_some());
    assert_eq!(nbt.get_i64("CollarColor"), Some(14));
    let variant = nbt.get_str("variant").unwrap();
    assert!(
        variant == "minecraft:calico" || variant == "minecraft:red",
        "{variant}"
    );
}

#[test]
fn a_wild_cat_follows_a_player_holding_fish_and_crouches_while_sneaking_up() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    hold(&mut server, &mut alice, "minecraft:cod", 1);
    let cat = summon(
        &mut server,
        &mut alice,
        "/summon cat 4.5 64 0.5 {}",
        Kind::Cat,
    );
    let mut crouched = false;
    for _ in 0..40 {
        pump(&mut server, &mut alice, 1);
        let events = alice.drain();
        crouched |= metadata(&events, cat, 6)
            .iter()
            .any(|value| matches!(value, MetadataValue::Pose(_)));
    }
    assert!(crouched);
}
