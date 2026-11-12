use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rspigot_blocks::{Block, Lava, Stone, Water};
use rspigot_core::admin::{AdminLists, ListEntry};
use rspigot_core::chunks::ChunkManagerConfig;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, EntityStatus, Hand, Inbound, MetadataValue, PlayerInput};
use rspigot_core::ports::{ChunkGenerator, ChunkStore, ClientLink, EntityStore, StoreError};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
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
        (2, 63, 2) => rspigot_blocks::BlockKind::Farmland.default_state(),
        (2, 64, 2) => rspigot_blocks::BlockKind::Carrots
            .state_with(&[("age", "7")])
            .expect("carrot age"),
        (_, 64, _) => BlockStateId::AIR,
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
                for y in 60..=64 {
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

fn events_of(events: &[ClientEvent], entity: EntityId, status: i8) -> usize {
    entity_events(events, entity)
        .into_iter()
        .filter(|found| *found == EntityStatus::Event(status))
        .count()
}

fn tick(server: &mut Server<Link>, client: &mut Client, ticks: usize) -> Vec<ClientEvent> {
    pump(server, client, ticks);
    client.drain()
}

#[test]
fn a_summoned_rabbit_has_a_synchronised_variant_saved_as_rabbit_type() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let rabbit = summon(
        &mut server,
        &mut alice,
        "/summon rabbit 0.5 64 6.5 {NoAI:1b,RabbitType:5,MoreCarrotTicks:30}",
        Kind::Rabbit,
    );
    let nbt = saved(&server, rabbit);
    assert_eq!(nbt.get_i32("RabbitType"), Some(5));
    assert_eq!(nbt.get_i32("MoreCarrotTicks"), Some(30));
    let mut bob = join(&mut server, "Bob");
    let events = bob.drain();
    assert!(metadata(&events, rabbit, 18).contains(&MetadataValue::VarInt(5)));
}

#[test]
fn a_rabbit_summoned_without_data_gets_a_biome_variant() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let rabbit = summon(
        &mut server,
        &mut alice,
        "/summon rabbit 0.5 64 6.5",
        Kind::Rabbit,
    );
    let variant = saved(&server, rabbit).get_i32("RabbitType").unwrap();
    assert!([0, 1, 2, 3, 4, 5].contains(&variant));
}

#[test]
fn the_killer_bunny_is_named_armoured_and_hunts_with_extra_goals() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let rabbit = summon(
        &mut server,
        &mut alice,
        "/summon rabbit 0.5 64 20.5 {RabbitType:99}",
        Kind::Rabbit,
    );
    pump(&mut server, &mut alice, 4);
    let ai = server.mob_ai(rabbit).expect("rabbit has an ai");
    assert!(
        ai.goals().goals().iter().any(|goal| goal.priority() == 4
            && matches!(goal.goal(), rspigot_ai::MobGoal::MeleeAttack(_)))
    );
    let targets: Vec<i32> = ai.targets().goals().iter().map(|g| g.priority()).collect();
    assert_eq!(targets, vec![1, 2, 2]);
    let nbt = saved(&server, rabbit);
    assert_eq!(nbt.get_i32("RabbitType"), Some(99));
    let name = nbt
        .get_compound("CustomName")
        .and_then(|name| name.get_str("translate"));
    assert_eq!(name, Some("entity.minecraft.killer_bunny"));
}

#[test]
fn a_plain_rabbit_has_no_melee_goal_and_no_targets() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let rabbit = summon(
        &mut server,
        &mut alice,
        "/summon rabbit 0.5 64 20.5 {RabbitType:1}",
        Kind::Rabbit,
    );
    pump(&mut server, &mut alice, 4);
    let ai = server.mob_ai(rabbit).expect("rabbit has an ai");
    assert!(ai.targets().goals().is_empty());
    assert!(
        !ai.goals()
            .goals()
            .iter()
            .any(|goal| matches!(goal.goal(), rspigot_ai::MobGoal::MeleeAttack(_)))
    );
}

#[test]
fn a_killer_bunny_hops_at_a_nearby_player_with_hostile_sounds() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    summon(
        &mut server,
        &mut alice,
        "/summon rabbit 0.5 64 3.5 {RabbitType:99}",
        Kind::Rabbit,
    );
    alice.drain();
    let events = tick(&mut server, &mut alice, 120);
    let jumps: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Sound {
                sound: "minecraft:entity.rabbit.jump",
                source,
                ..
            } => Some(*source),
            _ => None,
        })
        .collect();
    assert!(!jumps.is_empty());
    assert!(
        jumps
            .iter()
            .all(|source| *source == rspigot_entities::SoundSource::Hostile)
    );
}

#[test]
fn rabbits_that_hop_broadcast_the_jump_event() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let rabbit = summon(
        &mut server,
        &mut alice,
        "/summon rabbit 0.5 64 3.5 {RabbitType:99}",
        Kind::Rabbit,
    );
    alice.drain();
    let events = tick(&mut server, &mut alice, 120);
    assert!(events_of(&events, rabbit, 1) > 0);
}

#[test]
fn a_calm_rabbit_hops_with_neutral_sounds_and_rises_off_the_ground() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let rabbit = summon(
        &mut server,
        &mut alice,
        "/summon rabbit 0.5 64 30.5 {RabbitType:0}",
        Kind::Rabbit,
    );
    command(&mut server, &mut alice, "/gamemode creative");
    alice.drain();
    let mut peak = 64.0_f64;
    let mut hops = 0;
    for _ in 0..600 {
        pump(&mut server, &mut alice, 1);
        let events = alice.drain();
        hops += events_of(&events, rabbit, 1);
        if let Some(entity) = server
            .entities_of(Kind::Rabbit)
            .iter()
            .find(|e| e.id == rabbit)
        {
            peak = peak.max(entity.position.y);
        }
    }
    assert!(hops > 0);
    assert!(peak > 64.05);
}

#[test]
fn a_trusting_ocelot_is_saved_and_synchronised() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let ocelot = summon(
        &mut server,
        &mut alice,
        "/summon ocelot 0.5 64 30.5 {NoAI:1b,Trusting:1b}",
        Kind::Ocelot,
    );
    assert_eq!(saved(&server, ocelot).get_bool("Trusting"), Some(true));
    let mut bob = join(&mut server, "Bob");
    let events = bob.drain();
    assert!(metadata(&events, ocelot, 18).contains(&MetadataValue::Boolean(true)));
}

#[test]
fn a_wild_ocelot_is_not_trusting_and_flees_players() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let ocelot = summon(
        &mut server,
        &mut alice,
        "/summon ocelot 0.5 64 6.5",
        Kind::Ocelot,
    );
    assert_eq!(saved(&server, ocelot).get_bool("Trusting"), Some(false));
    let mut fled = false;
    for _ in 0..40 {
        pump(&mut server, &mut alice, 1);
        fled |= server.mob_ai(ocelot).is_some_and(|ai| {
            ai.goals().goals().iter().any(|goal| {
                goal.is_running() && matches!(goal.goal(), rspigot_ai::MobGoal::AvoidEntity(_))
            })
        });
    }
    assert!(fled);
}

#[test]
fn a_trusting_ocelot_has_no_avoid_goal() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let ocelot = summon(
        &mut server,
        &mut alice,
        "/summon ocelot 0.5 64 6.5 {Trusting:1b}",
        Kind::Ocelot,
    );
    pump(&mut server, &mut alice, 6);
    let ai = server.mob_ai(ocelot).expect("ocelot has an ai");
    assert!(
        !ai.goals()
            .goals()
            .iter()
            .any(|goal| matches!(goal.goal(), rspigot_ai::MobGoal::AvoidEntity(_)))
    );
}

#[test]
fn fish_in_hand_wins_an_ocelot_over_with_one_chance_in_three() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let ocelot = summon(
        &mut server,
        &mut alice,
        "/summon ocelot 0.5 64 2.5",
        Kind::Ocelot,
    );
    give(&mut server, &mut alice, "minecraft:cod", 40);
    alice.drain();
    pump(&mut server, &mut alice, 6);
    let mut failures = 0;
    let mut trusted = false;
    let mut eaten = 0;
    for _ in 0..40 {
        let tempting = server.mob_ai(ocelot).is_some_and(|ai| ai.is_tempting());
        if !tempting {
            pump(&mut server, &mut alice, 2);
            continue;
        }
        let before = holding(&alice, ItemKind::Cod);
        interact(&mut server, &mut alice, ocelot);
        pump(&mut server, &mut alice, 1);
        let events = alice.drain();
        let after = holding(&alice, ItemKind::Cod);
        assert_eq!(after + 1, before);
        eaten += 1;
        failures += events_of(&events, ocelot, 40);
        if events_of(&events, ocelot, 41) > 0 {
            trusted = true;
            break;
        }
    }
    assert!(trusted);
    assert_eq!(failures + 1, eaten);
    assert_eq!(saved(&server, ocelot).get_bool("Trusting"), Some(true));
    pump(&mut server, &mut alice, 4);
    let ai = server.mob_ai(ocelot).expect("ocelot has an ai");
    assert!(
        !ai.goals()
            .goals()
            .iter()
            .any(|goal| matches!(goal.goal(), rspigot_ai::MobGoal::AvoidEntity(_)))
    );
}

#[test]
fn ocelots_only_trust_when_a_fish_tempts_them() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let ocelot = summon(
        &mut server,
        &mut alice,
        "/summon ocelot 0.5 64 2.5 {NoAI:1b}",
        Kind::Ocelot,
    );
    give(&mut server, &mut alice, "minecraft:cod", 4);
    alice.drain();
    interact(&mut server, &mut alice, ocelot);
    let events = alice.drain();
    assert_eq!(
        events_of(&events, ocelot, 40) + events_of(&events, ocelot, 41),
        0
    );
    assert_eq!(saved(&server, ocelot).get_bool("Trusting"), Some(false));
}

#[test]
fn a_stalking_ocelot_crouches_then_sprints_when_pouncing_on_a_chicken() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    command(&mut server, &mut alice, "/gamemode creative");
    let ocelot = summon(
        &mut server,
        &mut alice,
        "/summon ocelot 0.5 64 30.5",
        Kind::Ocelot,
    );
    summon(
        &mut server,
        &mut alice,
        "/summon chicken 0.5 64 35.5 {NoAI:1b}",
        Kind::Chicken,
    );
    alice.drain();
    let mut crouched = false;
    let mut sprinted = false;
    for _ in 0..300 {
        pump(&mut server, &mut alice, 1);
        let events = alice.drain();
        crouched |= metadata(&events, ocelot, 6).contains(&MetadataValue::Pose(
            rspigot_domain::entity::Pose::Crouching,
        ));
        sprinted |= metadata(&events, ocelot, 0)
            .iter()
            .any(|value| matches!(value, MetadataValue::Byte(flags) if flags & 8 != 0));
    }
    assert!(crouched);
    assert!(sprinted);
}

#[test]
fn rabbits_bred_from_two_salt_parents_are_salt_babies() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    command(&mut server, &mut alice, "/gamemode creative");
    let first = summon(
        &mut server,
        &mut alice,
        "/summon rabbit 0.5 64 2.5 {RabbitType:5}",
        Kind::Rabbit,
    );
    let second = summon(
        &mut server,
        &mut alice,
        "/summon rabbit 1.5 64 2.5 {RabbitType:5}",
        Kind::Rabbit,
    );
    give(&mut server, &mut alice, "minecraft:carrot", 3);
    alice.drain();
    interact(&mut server, &mut alice, first);
    interact(&mut server, &mut alice, second);
    let mut baby = None;
    for _ in 0..300 {
        pump(&mut server, &mut alice, 1);
        baby = server
            .entities_of(Kind::Rabbit)
            .iter()
            .map(|e| e.id)
            .find(|id| *id != first && *id != second);
        if baby.is_some() {
            break;
        }
    }
    let baby = baby.expect("no baby rabbit was born");
    let nbt = saved(&server, baby);
    assert!(nbt.get_i32("Age").is_some_and(|age| age < -23_990));
    assert!([0, 1, 2, 3, 4, 5].contains(&nbt.get_i32("RabbitType").unwrap()));
    assert_eq!(nbt.get_i32("RabbitType"), Some(5));
}

#[test]
fn a_rabbit_eats_a_ripe_carrot_and_is_full_for_a_while() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    command(&mut server, &mut alice, "/gamemode creative");
    command(
        &mut server,
        &mut alice,
        "/setblock 2 63 2 minecraft:farmland",
    );
    let rabbit = summon(
        &mut server,
        &mut alice,
        "/summon rabbit 2.5 64 2.5 {RabbitType:0}",
        Kind::Rabbit,
    );
    let mut full = 0;
    let mut seen = Vec::new();
    for _ in 0..800 {
        pump(&mut server, &mut alice, 1);
        seen.extend(alice.drain());
        full = saved(&server, rabbit)
            .get_i32("MoreCarrotTicks")
            .unwrap_or(0);
        if full > 0 {
            break;
        }
    }
    assert!(full > 30, "carrot ticks {full}");
    assert!(seen.iter().any(|event| matches!(
        event,
        ClientEvent::LevelEvent { event: 2001, pos, .. } if *pos == BlockPos::new(2, 64, 2)
    )));
}
