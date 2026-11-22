use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rspigot_blocks::{Block, Stone};
use rspigot_core::admin::{AdminLists, ListEntry};
use rspigot_core::chunks::ChunkManagerConfig;
use rspigot_core::commands::Target;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{
    ClientEvent, EntityKind, EntityStatus, Inbound, MovementAction, PlayerInput,
};
use rspigot_core::ports::{ChunkGenerator, ChunkStore, ClientLink, EntityStore, StoreError};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::damage::DamageType;
use rspigot_domain::player::{ClientSettings, GameProfile, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, EntityId, Identifier, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::{EquipmentSlot, ItemKind};
use rspigot_nbt::{Compound, Tag, read_named, write_named};
use rustc_hash::FxHashMap;

const DEFAULT_KNOCKBACK: f64 = 0.400_000_005_960_464_5;

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
    server_at(store, LevelData::default())
}

fn night_server() -> Server<Link> {
    server_at(
        Arc::default(),
        LevelData {
            day_time: 18_000,
            spawn_mobs: false,
            daylight_cycle: false,
            ..LevelData::default()
        },
    )
}

fn server_at(store: Arc<MemoryStore>, level: LevelData) -> Server<Link> {
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
    let mut server = Server::new(config, level, store, Arc::new(Flat), StatusBoard::new(20));
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
    stand(server, &client, Vec3::new(0.5, 64.0, 0.5));
    pump(server, &mut client, 2);
    client.drain();
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

fn command(server: &mut Server<Link>, client: &Client, line: &str) {
    input(server, client.id, PlayerInput::Command(line.to_owned()));
}

fn only(server: &Server<Link>, kind: Kind) -> EntityId {
    let found = server.entities_of(kind);
    assert_eq!(found.len(), 1, "{kind:?}");
    found[0].id
}

fn summon(server: &mut Server<Link>, client: &mut Client, line: &str, kind: Kind) -> EntityId {
    command(server, client, line);
    pump(server, client, 2);
    only(server, kind)
}

fn give_sword(server: &mut Server<Link>, client: &mut Client) {
    command(server, client, "/give @s minecraft:diamond_sword");
    pump(server, client, 15);
}

fn attack(server: &mut Server<Link>, client: &Client, target: EntityId) {
    input(server, client.id, PlayerInput::Attack(target));
}

fn spawned(events: &[ClientEvent], kind: Kind) -> usize {
    events
        .iter()
        .filter(|e| match e {
            ClientEvent::SpawnEntity {
                kind: EntityKind::Other(id),
                ..
            } => *id == kind.id().0,
            ClientEvent::SpawnEntity {
                kind: EntityKind::Item,
                ..
            } => kind == Kind::Item,
            _ => false,
        })
        .count()
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

fn statuses(events: &[ClientEvent], entity: EntityId) -> Vec<EntityStatus> {
    events
        .iter()
        .filter_map(|e| match e {
            ClientEvent::EntityStatus { entity: e, status } if *e == entity => Some(*status),
            _ => None,
        })
        .collect()
}

fn damage_events(events: &[ClientEvent], entity: EntityId) -> Vec<(DamageType, Option<EntityId>)> {
    events
        .iter()
        .filter_map(|e| match e {
            ClientEvent::DamageEvent {
                entity: e,
                kind,
                cause,
                ..
            } if *e == entity => Some((*kind, *cause)),
            _ => None,
        })
        .collect()
}

fn health(server: &Server<Link>, id: EntityId) -> f32 {
    server.mob_damage(id).map_or(-1.0, |m| m.health)
}

fn ground(server: &Server<Link>, name: &str) -> u32 {
    let item = ItemKind::from_name(name).unwrap().id();
    server
        .ground_items()
        .iter()
        .filter(|(_, stack)| stack.item == item)
        .map(|(_, stack)| u32::from(stack.count.get()))
        .sum()
}

#[test]
fn player_kills_a_cow_and_collects_its_loot_and_experience() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    give_sword(&mut server, &mut alice);
    let cow = summon(
        &mut server,
        &mut alice,
        "/summon cow 0.5 64 2.5 {NoAI:1b}",
        Kind::Cow,
    );
    alice.drain();
    attack(&mut server, &alice, cow);
    assert_eq!(health(&server, cow), 3.0);
    let state = server.mob_damage(cow).unwrap();
    assert_eq!(state.last_hurt_by_player, Some(alice.id));
    assert_eq!(state.player_memory, 100);
    assert_eq!(state.hurt_time, 10);
    let events = alice.drain();
    let kinds = damage_events(&events, cow);
    assert_eq!(kinds.len(), 1);
    assert_eq!(kinds[0].0, DamageType::PlayerAttack);
    let heard = sounds(&events);
    assert!(heard.contains(&"minecraft:entity.cow.hurt"), "{heard:?}");
    assert!(
        heard.contains(&"minecraft:entity.player.attack.sweep"),
        "{heard:?}"
    );
    pump(&mut server, &mut alice, 15);
    attack(&mut server, &alice, cow);
    let state = server.mob_damage(cow).unwrap();
    assert!(state.dead);
    assert_eq!(state.health, 0.0);
    assert!(
        server
            .ground_items()
            .iter()
            .any(|(_, s)| s.item == ItemKind::Beef.id())
    );
    let orbs = server.experience_orbs();
    let total: i32 = orbs.iter().map(|o| o.value * o.count).sum();
    assert!((1..=3).contains(&total), "{orbs:?}");
    let events = alice.drain();
    assert!(statuses(&events, cow).contains(&EntityStatus::Death));
    assert!(sounds(&events).contains(&"minecraft:entity.cow.death"));
    assert!(spawned(&events, Kind::ExperienceOrb) >= 1);
    assert!(spawned(&events, Kind::Item) >= 1);
    pump(&mut server, &mut alice, 80);
    let events = alice.drain();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ClientEvent::ItemPickup { amount: 1, .. }))
    );
    let (_, _, gained) = server.player_experience(alice.id).unwrap();
    assert_eq!(gained, total);
    assert!(events.iter().any(|e| matches!(
        e,
        ClientEvent::Experience { total: t, .. } if *t == total
    )));
    assert!(server.experience_orbs().is_empty());
    assert!(server.entities_of(Kind::Cow).is_empty());
}

#[test]
fn cow_killed_by_command_drops_loot_but_no_experience() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    let cow = summon(
        &mut server,
        &mut alice,
        "/summon cow 6.5 64 6.5 {}",
        Kind::Cow,
    );
    alice.drain();
    command(&mut server, &alice, "/kill @e[type=cow]");
    assert!(server.mob_damage(cow).unwrap().dead);
    assert!(ground(&server, "minecraft:beef") >= 1);
    assert!(server.experience_orbs().is_empty());
    let events = alice.drain();
    assert_eq!(
        damage_events(&events, cow),
        vec![(DamageType::GenericKill, None)]
    );
    assert!(spawned(&events, Kind::Item) >= 1);
    assert_eq!(spawned(&events, Kind::ExperienceOrb), 0);
    pump(&mut server, &mut alice, 5);
    assert!(server.experience_orbs().is_empty());
}

#[test]
fn a_hit_shortly_before_the_kill_command_still_rewards_experience() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    let cow = summon(
        &mut server,
        &mut alice,
        "/summon cow 0.5 64 2.5 {}",
        Kind::Cow,
    );
    attack(&mut server, &alice, cow);
    pump(&mut server, &mut alice, 40);
    command(&mut server, &alice, "/kill @e[type=cow]");
    let total: i32 = server
        .experience_orbs()
        .iter()
        .map(|o| o.value * o.count)
        .sum();
    assert!((1..=3).contains(&total));
}

#[test]
fn zombie_gear_drops_at_the_vanilla_chance() {
    let mut server = server(Arc::default());
    let alice = join(&mut server, "Alice");
    const ZOMBIES: usize = 400;
    for _ in 0..ZOMBIES {
        command(
            &mut server,
            &alice,
            "/summon zombie 0.5 64 2.5 {NoAI:1b,equipment:{head:{id:\"minecraft:iron_helmet\",count:1}}}",
        );
    }
    let zombies: Vec<EntityId> = server
        .entities_of(Kind::Zombie)
        .iter()
        .map(|z| z.id)
        .collect();
    assert_eq!(zombies.len(), ZOMBIES);
    for zombie in &zombies {
        attack(&mut server, &alice, *zombie);
    }
    command(&mut server, &alice, "/kill @e[type=zombie]");
    let helmets: Vec<_> = server
        .ground_items()
        .into_iter()
        .filter(|(_, s)| s.item == ItemKind::IronHelmet.id())
        .collect();
    let dropped = helmets.len();
    assert!((12..=58).contains(&dropped), "{dropped}");
    for (_, helmet) in &helmets {
        let damage = helmet.components.get_i32("minecraft:damage").unwrap_or(0);
        assert!((3..=165).contains(&damage), "{damage}");
    }
    for zombie in &zombies {
        let left = server.mob_damage(*zombie).unwrap().equipment;
        assert!(left.is_empty() || left[0].0 == EquipmentSlot::Head);
    }
    command(
        &mut server,
        &alice,
        "/summon zombie 0.5 64 2.5 {NoAI:1b,equipment:{head:{id:\"minecraft:iron_helmet\",count:1}}}",
    );
    command(&mut server, &alice, "/kill @e[type=zombie]");
    assert_eq!(
        server
            .ground_items()
            .iter()
            .filter(|(_, s)| s.item == ItemKind::IronHelmet.id())
            .count(),
        dropped
    );
    command(
        &mut server,
        &alice,
        "/summon zombie 0.5 64 2.5 {NoAI:1b,drop_chances:{head:2.0f},equipment:{head:{id:\"minecraft:iron_helmet\",count:1}}}",
    );
    command(&mut server, &alice, "/kill @e[type=zombie]");
    let guaranteed: Vec<_> = server
        .ground_items()
        .into_iter()
        .filter(|(_, s)| s.item == ItemKind::IronHelmet.id())
        .collect();
    assert_eq!(guaranteed.len(), dropped + 1);
    assert!(
        guaranteed
            .iter()
            .any(|(_, s)| !s.components.contains_key("minecraft:damage"))
    );
}

#[test]
fn invulnerable_time_blocks_a_second_hit_and_a_stronger_one_deals_the_difference() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    let cow = summon(
        &mut server,
        &mut alice,
        "/summon cow 0.5 64 2.5 {}",
        Kind::Cow,
    );
    alice.drain();
    attack(&mut server, &alice, cow);
    assert_eq!(health(&server, cow), 9.0);
    pump(&mut server, &mut alice, 6);
    attack(&mut server, &alice, cow);
    assert_eq!(health(&server, cow), 9.0);
    assert_eq!(damage_events(&alice.drain(), cow).len(), 1);
    command(&mut server, &alice, "/give @s minecraft:diamond_sword");
    pump(&mut server, &mut alice, 1);
    let state = server.mob_damage(cow).unwrap();
    assert!(state.cooldown > 10, "{}", state.cooldown);
    attack(&mut server, &alice, cow);
    let after = health(&server, cow);
    assert!(after < 9.0 && after > 2.0, "{after}");
    assert!(damage_events(&alice.drain(), cow).is_empty());
    pump(&mut server, &mut alice, 20);
    command(&mut server, &alice, "/tp @e[type=cow] 0.5 64 2.5");
    attack(&mut server, &alice, cow);
    assert_eq!(health(&server, cow), after - 7.0);
    assert_eq!(damage_events(&alice.drain(), cow).len(), 1);
}

#[test]
fn knockback_velocity_matches_vanilla_numbers() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    let cow = summon(
        &mut server,
        &mut alice,
        "/summon cow 0.5 64 2.5 {}",
        Kind::Cow,
    );
    pump(&mut server, &mut alice, 10);
    let before = server.entities_of(Kind::Cow)[0];
    assert!(before.on_ground);
    let player = server.player_location(alice.id).unwrap();
    attack(&mut server, &alice, cow);
    let after = server.entities_of(Kind::Cow)[0].velocity;
    let expected = pushed(
        before.velocity,
        DEFAULT_KNOCKBACK,
        player.x - before.position.x,
        player.z - before.position.z,
    );
    assert_eq!(after, expected);
    alice.drain();
    pump(&mut server, &mut alice, 1);
    let sent = alice.drain().iter().find_map(|e| match e {
        ClientEvent::EntityVelocity { entity, velocity } if *entity == cow => Some(*velocity),
        _ => None,
    });
    assert_eq!(sent, Some(after));
    pump(&mut server, &mut alice, 120);
    command(&mut server, &alice, "/tp @e[type=cow] 0.5 64 2.5");
    pump(&mut server, &mut alice, 30);
    let resting = server.entities_of(Kind::Cow)[0];
    assert!(resting.on_ground);
    input(
        &mut server,
        alice.id,
        PlayerInput::Movement(MovementAction::StartSprinting),
    );
    attack(&mut server, &alice, cow);
    let first = pushed(
        resting.velocity,
        DEFAULT_KNOCKBACK,
        player.x - resting.position.x,
        player.z - resting.position.z,
    );
    let sprinted = server.entities_of(Kind::Cow)[0].velocity;
    assert_eq!(sprinted, pushed(first, 0.5, 0.0, -1.0));
    let heard = sounds(&alice.drain());
    assert!(heard.contains(&"minecraft:entity.player.attack.knockback"));
}

fn pushed(current: Vec3, strength: f64, dx: f64, dz: f64) -> Vec3 {
    let length = (dx * dx + dz * dz).sqrt();
    Vec3::new(
        current.x / 2.0 - dx / length * strength,
        (current.y / 2.0 + strength).min(0.4),
        current.z / 2.0 - dz / length * strength,
    )
}

#[test]
fn fall_damage_kills_a_mob() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    let cow = summon(
        &mut server,
        &mut alice,
        "/summon cow 3.5 94 3.5 {}",
        Kind::Cow,
    );
    alice.drain();
    let mut landed = None;
    for tick in 0..120 {
        pump(&mut server, &mut alice, 1);
        if server.mob_damage(cow).is_some_and(|m| m.dead) {
            landed = Some(tick);
            break;
        }
    }
    assert!(landed.is_some());
    let events = alice.drain();
    assert_eq!(damage_events(&events, cow), vec![(DamageType::Fall, None)]);
    let heard = sounds(&events);
    assert!(
        heard.contains(&"minecraft:entity.generic.big_fall"),
        "{heard:?}"
    );
    assert!(heard.contains(&"minecraft:entity.cow.death"));
    assert!(server.experience_orbs().is_empty());
}

#[test]
fn short_falls_only_hurt_and_safe_falls_do_nothing() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    let cow = summon(
        &mut server,
        &mut alice,
        "/summon cow 3.5 70 3.5 {}",
        Kind::Cow,
    );
    pump(&mut server, &mut alice, 40);
    let state = server.mob_damage(cow).unwrap();
    assert!(!state.dead);
    assert_eq!(state.health, 10.0 - 3.0);
    let chicken = summon(
        &mut server,
        &mut alice,
        "/summon chicken -3.5 80 -3.5",
        Kind::Chicken,
    );
    pump(&mut server, &mut alice, 60);
    assert_eq!(health(&server, chicken), 4.0);
}

#[test]
fn the_death_animation_removes_the_entity_after_twenty_ticks() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    let cow = summon(
        &mut server,
        &mut alice,
        "/summon cow 4.5 64 4.5 {}",
        Kind::Cow,
    );
    alice.drain();
    command(&mut server, &alice, "/kill @e[type=cow]");
    let mut seen = Vec::new();
    let mut ticks = 0;
    while let Some(state) = server.mob_damage(cow) {
        seen.push(state.death_time);
        pump(&mut server, &mut alice, 1);
        ticks += 1;
        assert!(ticks < 40);
    }
    assert!(ticks >= 20);
    seen.dedup();
    assert_eq!(seen, (0..20).collect::<Vec<_>>());
    let events = alice.drain();
    assert_eq!(
        statuses(&events, cow),
        vec![EntityStatus::Death, EntityStatus::Poof]
    );
    let poof = events.iter().position(|e| {
        matches!(
            e,
            ClientEvent::EntityStatus {
                status: EntityStatus::Poof,
                ..
            }
        )
    });
    let removal = events
        .iter()
        .position(|e| matches!(e, ClientEvent::RemoveEntities(ids) if ids.contains(&cow)));
    assert!(poof < removal);
}

#[test]
fn orbs_merge_and_are_picked_up_two_ticks_apart() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    command(&mut server, &alice, "/tp @s 40 64 40");
    pump(&mut server, &mut alice, 40);
    stand(&mut server, &alice, Vec3::new(40.5, 64.0, 40.5));
    command(
        &mut server,
        &alice,
        "/summon experience_orb 0.5 64 6.5 {Value:3s}",
    );
    for index in 0..39 {
        command(
            &mut server,
            &alice,
            &format!(
                "/summon experience_orb {}.5 64 -9.5 {{Value:7s}}",
                index * 2
            ),
        );
    }
    command(
        &mut server,
        &alice,
        "/summon experience_orb 0.5 64 6.5 {Value:3s}",
    );
    let threes: Vec<_> = server
        .experience_orbs()
        .into_iter()
        .filter(|o| o.value == 3)
        .collect();
    assert_eq!(threes.len(), 2);
    assert_eq!(threes[1].id.0 - threes[0].id.0, 40);
    pump(&mut server, &mut alice, 25);
    let merged: Vec<_> = server
        .experience_orbs()
        .into_iter()
        .filter(|o| o.value == 3)
        .collect();
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].count, 2);
    command(&mut server, &alice, "/tp @s 0.5 64 6.5");
    pump(&mut server, &mut alice, 1);
    stand(&mut server, &alice, Vec3::new(0.5, 64.0, 6.5));
    alice.drain();
    let mut pickups = Vec::new();
    for tick in 0..10 {
        pump(&mut server, &mut alice, 1);
        let picked = alice
            .drain()
            .iter()
            .filter(|e| matches!(e, ClientEvent::ItemPickup { collected, amount: 1, .. } if *collected == merged[0].id))
            .count();
        for _ in 0..picked {
            pickups.push(tick);
        }
    }
    assert_eq!(pickups.len(), 2, "{pickups:?}");
    assert_eq!(pickups[1] - pickups[0], 2);
    assert_eq!(server.player_experience(alice.id), Some((0, 6.0 / 7.0, 6)));
    assert!(server.experience_orbs().iter().all(|o| o.value != 3));
}

#[test]
fn orbs_follow_the_nearest_player_and_survive_a_reload() {
    let store = Arc::new(MemoryStore::default());
    let mut server = server(Arc::clone(&store));
    let mut alice = join(&mut server, "Alice");
    command(&mut server, &alice, "/tp @s 100.5 64 100.5");
    pump(&mut server, &mut alice, 40);
    stand(&mut server, &alice, Vec3::new(100.5, 64.0, 100.5));
    command(
        &mut server,
        &alice,
        "/summon experience_orb 106.5 64 100.5 {Value:17s,Count:2}",
    );
    pump(&mut server, &mut alice, 1);
    let orb = server.experience_orbs()[0];
    assert!(orb.velocity.x < 0.0, "{orb:?}");
    command(&mut server, &alice, "/tp @s 2000 64 2000");
    let chunk = ChunkPos::new(6, 6);
    for _ in 0..200 {
        if store.stored(chunk).is_some() {
            break;
        }
        pump(&mut server, &mut alice, 1);
    }
    let saved = store.stored(chunk).expect("orb chunk saved");
    let mut input = saved.as_slice();
    let (_, root) = read_named(&mut input).unwrap();
    let orbs = root.get_list("Entities").unwrap();
    let saved_orb = orbs
        .iter()
        .filter_map(Tag::as_compound)
        .find(|e| e.get_str("id") == Some("minecraft:experience_orb"))
        .expect("orb saved");
    assert_eq!(saved_orb.get("Value"), Some(&Tag::Short(17)));
    assert_eq!(saved_orb.get("Count"), Some(&Tag::Int(2)));
    command(&mut server, &alice, "/tp @s 130.5 64 130.5");
    pump(&mut server, &mut alice, 60);
    let back = server.experience_orbs();
    assert_eq!(back.len(), 1);
    assert_eq!((back[0].value, back[0].count), (17, 2));
}

#[test]
fn mob_melee_hurts_players_and_mobs_with_difficulty_scaling() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    let zombie = summon(
        &mut server,
        &mut alice,
        "/summon zombie 0.5 64 1.5 {NoAI:1b}",
        Kind::Zombie,
    );
    alice.drain();
    assert!(server.do_hurt_target(zombie, Target::Player(alice.id)));
    assert_eq!(server.player_health(alice.id), Some(17.5));
    assert!(!server.do_hurt_target(zombie, Target::Player(alice.id)));
    server.deliver();
    let events = alice.drain();
    assert!(events.iter().any(|e| matches!(
        e,
        ClientEvent::DamageEvent { kind: DamageType::MobAttack, cause: Some(c), .. } if *c == zombie
    )));
    let cow = summon(
        &mut server,
        &mut alice,
        "/summon cow 2.5 64 1.5 {NoAI:1b}",
        Kind::Cow,
    );
    assert!(server.do_hurt_target(zombie, Target::Entity(cow)));
    let state = server.mob_damage(cow).unwrap();
    assert_eq!(state.health, 7.0);
    assert_eq!(state.last_hurt_by_mob, Some(zombie));
    assert_eq!(state.last_hurt_by_player, None);
    command(&mut server, &alice, "/kill @e[type=cow]");
    assert!(server.experience_orbs().is_empty());
}

#[test]
fn dying_players_drop_seven_experience_per_level_and_respawn_empty() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    server.give_experience(alice.id, 100);
    pump(&mut server, &mut alice, 1);
    assert_eq!(server.player_experience(alice.id).map(|x| x.0), Some(7));
    assert!(alice.drain().iter().any(|e| matches!(
        e,
        ClientEvent::Experience {
            level: 7,
            total: 100,
            ..
        }
    )));
    command(&mut server, &alice, "/kill");
    let total: i32 = server
        .experience_orbs()
        .iter()
        .map(|o| o.value * o.count)
        .sum();
    assert_eq!(total, 49);
    pump(&mut server, &mut alice, 2);
    alice.drain();
    input(&mut server, alice.id, PlayerInput::Respawn);
    assert_eq!(server.player_experience(alice.id), Some((0, 0.0, 0)));
    assert!(alice.drain().iter().any(|e| matches!(
        e,
        ClientEvent::Experience {
            level: 0,
            total: 0,
            ..
        }
    )));
}

#[test]
fn falling_anvils_hurt_mobs_below_them() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    let cow = summon(
        &mut server,
        &mut alice,
        "/summon cow -4.5 64 4.5 {NoAI:1b}",
        Kind::Cow,
    );
    command(
        &mut server,
        &alice,
        "/summon falling_block -4.5 70 4.5 {BlockState:{Name:\"minecraft:anvil\"},HurtEntities:1b,FallHurtAmount:2.0f,FallHurtMax:40}",
    );
    alice.drain();
    pump(&mut server, &mut alice, 40);
    let events = alice.drain();
    let hits = damage_events(&events, cow);
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].0, DamageType::FallingAnvil);
    assert!(health(&server, cow) < 10.0);
}

fn geared_zombie(server: &mut Server<Link>, alice: &mut Client, enchantments: &str) -> EntityId {
    summon(
        server,
        alice,
        &format!(
            "/summon zombie 0.5 64 2.5 {{NoAI:1b,equipment:{{head:{{id:\"minecraft:diamond_helmet\",count:1,components:{{\"minecraft:enchantments\":{enchantments}}}}}}}}}"
        ),
        Kind::Zombie,
    )
}

#[test]
fn a_zombie_with_a_thorns_helmet_hurts_its_attacker_and_wears_the_helmet() {
    let mut server = night_server();
    let mut alice = join(&mut server, "Alice");
    give_sword(&mut server, &mut alice);
    let zombie = geared_zombie(&mut server, &mut alice, "{\"minecraft:thorns\":10}");
    alice.drain();
    attack(&mut server, &alice, zombie);
    assert!(server.player_health(alice.id).unwrap() < 20.0);
    let events = alice.drain();
    assert!(
        damage_events(&events, zombie)
            .iter()
            .all(|(kind, _)| *kind != DamageType::Thorns)
    );
    let helmet = server
        .mob_damage(zombie)
        .unwrap()
        .equipment
        .into_iter()
        .find(|(slot, _)| *slot == EquipmentSlot::Head)
        .map(|(_, stack)| stack)
        .expect("the helmet is still worn");
    assert_eq!(helmet.components.get_i32("minecraft:damage"), Some(2));
}

fn zombie_health_after_a_hit(enchantments: &str) -> f32 {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    give_sword(&mut server, &mut alice);
    let zombie = geared_zombie(&mut server, &mut alice, enchantments);
    attack(&mut server, &alice, zombie);
    server.mob_damage(zombie).unwrap().health
}

#[test]
fn a_zombie_wearing_protection_takes_less_damage() {
    let plain = zombie_health_after_a_hit("{}");
    let protected = zombie_health_after_a_hit("{\"minecraft:protection\":4}");
    assert!(plain < 20.0, "{plain}");
    assert!(protected > plain, "{protected} vs {plain}");
}
