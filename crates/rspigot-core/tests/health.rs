use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rspigot_blocks::{Block, BlockKind, Stone};
use rspigot_core::admin::{AdminLists, ListEntry};
use rspigot_core::config::{GameConfig, GameRules, LevelData};
use rspigot_core::events::{
    ClientEvent, DigAction, Direction, EntityKind, EntityStatus, Inbound, PlayerInput,
};
use rspigot_core::ports::{ChunkGenerator, ChunkStore, ClientLink, EntityStore, StoreError};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::damage::DamageType;
use rspigot_domain::player::{ClientSettings, GameProfile, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, EntityId, Identifier, Vec3};

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

const ANVIL: (i32, i32) = (8, 8);
const HAY: (i32, i32) = (4, 4);
const ANVIL_Y: i32 = 70;

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
            let (x, z) = ANVIL;
            chunk.set_block(BlockPos::new(x, ANVIL_Y - 1, z), Stone::DEFAULT_STATE);
            chunk.set_block(
                BlockPos::new(x, ANVIL_Y, z),
                BlockKind::Anvil
                    .state_with(&[("facing", "east")])
                    .unwrap_or_default(),
            );
            let (hx, hz) = HAY;
            chunk.set_block(
                BlockPos::new(hx, 64, hz),
                BlockKind::HayBlock.default_state(),
            );
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

fn server_with(configure: impl FnOnce(&mut GameConfig)) -> Server<Link> {
    let mut config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        admin: AdminLists {
            operators: vec![ListEntry::named("Alice")],
            whitelist: Vec::new(),
            whitelist_enabled: false,
            ..AdminLists::default()
        },
        ..GameConfig::default()
    };
    configure(&mut config);
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

fn server() -> Server<Link> {
    server_with(|_| {})
}

struct Client {
    id: PlayerId,
    link: Link,
    events: Vec<ClientEvent>,
}

impl Client {
    fn drain(&mut self) -> Vec<ClientEvent> {
        self.collect();
        std::mem::take(&mut self.events)
    }

    fn collect(&mut self) {
        self.events.extend(self.link.take());
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
    for event in client.drain() {
        if let ClientEvent::Teleport { id: teleport, .. } = event {
            input(server, id, PlayerInput::TeleportConfirm(teleport));
        }
    }
    input(server, id, PlayerInput::Loaded);
    client
}

fn input(server: &mut Server<Link>, player: PlayerId, input: PlayerInput) {
    server.handle(Inbound::Input { player, input });
    server.deliver();
}

fn command(server: &mut Server<Link>, client: &Client, line: &str) {
    input(server, client.id, PlayerInput::Command(line.to_owned()));
}

fn ticks(server: &mut Server<Link>, clients: &mut [&mut Client], count: usize) {
    for _ in 0..count {
        server.wait_for_spawn(Duration::from_millis(5));
        server.tick();
        for client in clients.iter_mut() {
            let events = client.link.take();
            let batches = events
                .iter()
                .filter(|e| matches!(e, ClientEvent::ChunkBatchFinished { .. }))
                .count();
            for _ in 0..batches {
                input(
                    server,
                    client.id,
                    PlayerInput::ChunkBatchReceived {
                        chunks_per_tick: 64.0,
                    },
                );
            }
            client.events.extend(events);
        }
    }
}

fn settle(server: &mut Server<Link>, clients: &mut [&mut Client]) {
    ticks(server, clients, 30);
    for client in clients.iter_mut() {
        client.drain();
    }
}

fn walk(server: &mut Server<Link>, client: &Client, position: Vec3, on_ground: bool) {
    input(
        server,
        client.id,
        PlayerInput::Move {
            position: Some(position),
            rotation: None,
            on_ground,
        },
    );
}

fn fall(server: &mut Server<Link>, client: &Client, x: f64, z: f64, from: f64, to: f64) {
    let mut climbed = server.player_location(client.id).map_or(from, |at| at.y);
    while from - climbed > 6.0 {
        climbed += 6.0;
        walk(server, client, Vec3::new(x, climbed, z), false);
        server.tick();
    }
    walk(server, client, Vec3::new(x, from, z), false);
    let mut y = from;
    while y - 1.0 > to {
        y -= 1.0;
        walk(server, client, Vec3::new(x, y, z), false);
    }
    walk(server, client, Vec3::new(x, to, z), true);
}

fn health(server: &Server<Link>, client: &Client) -> f32 {
    server.player_health(client.id).unwrap_or(-1.0)
}

fn entity_of(events: &[ClientEvent], player: PlayerId) -> Option<EntityId> {
    events.iter().find_map(|e| match e {
        ClientEvent::SpawnEntity {
            entity,
            uuid,
            kind: EntityKind::Player,
            ..
        } if *uuid == player.0 => Some(*entity),
        _ => None,
    })
}

fn damage_kinds(events: &[ClientEvent]) -> Vec<DamageType> {
    events
        .iter()
        .filter_map(|e| match e {
            ClientEvent::DamageEvent { kind, .. } => Some(*kind),
            _ => None,
        })
        .collect()
}

fn last_health_packet(events: &[ClientEvent]) -> Option<(f32, i32, f32)> {
    events.iter().rev().find_map(|e| match e {
        ClientEvent::Health {
            health,
            food,
            saturation,
        } => Some((*health, *food, *saturation)),
        _ => None,
    })
}

fn system_texts(events: &[ClientEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            ClientEvent::SystemChat { message, .. } => Some(message.plain_text()),
            _ => None,
        })
        .collect()
}

#[test]
fn falling_twenty_blocks_deals_seventeen_damage() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    settle(&mut server, &mut [&mut alice]);
    fall(&mut server, &alice, 0.5, 0.5, 84.0, 64.0);
    assert_eq!(health(&server, &alice), 3.0);
    ticks(&mut server, &mut [&mut alice], 1);
    let events = alice.drain();
    assert_eq!(damage_kinds(&events), vec![DamageType::Fall]);
    assert_eq!(last_health_packet(&events).map(|h| h.0), Some(3.0));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, ClientEvent::HurtAnimation { .. }))
    );
}

#[test]
fn jump_boost_and_hay_soften_falls() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    settle(&mut server, &mut [&mut alice]);
    command(&mut server, &alice, "effect give @s jump_boost 60 1");
    fall(&mut server, &alice, 0.5, 0.5, 84.0, 64.0);
    assert_eq!(health(&server, &alice), 5.0);
    let mut bob = join(&mut server, "Bob");
    settle(&mut server, &mut [&mut alice, &mut bob]);
    let (x, z) = HAY;
    fall(
        &mut server,
        &bob,
        f64::from(x) + 0.5,
        f64::from(z) + 0.5,
        84.0,
        65.0,
    );
    assert_eq!(health(&server, &bob), 17.0);
}

#[test]
fn creative_players_ignore_falls_and_anvils() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    settle(&mut server, &mut [&mut alice]);
    command(&mut server, &alice, "gamemode creative");
    fall(&mut server, &alice, 0.5, 0.5, 84.0, 64.0);
    assert_eq!(health(&server, &alice), 20.0);
    let (x, z) = ANVIL;
    walk(
        &mut server,
        &alice,
        Vec3::new(f64::from(x) + 0.5, 64.0, f64::from(z) + 0.5),
        true,
    );
    dig(&mut server, &alice, BlockPos::new(x, ANVIL_Y - 1, z));
    ticks(&mut server, &mut [&mut alice], 40);
    assert_eq!(server.falling_blocks(), 0);
    assert_eq!(health(&server, &alice), 20.0);
    assert!(damage_kinds(&alice.drain()).is_empty());
}

fn dig(server: &mut Server<Link>, client: &Client, pos: BlockPos) {
    input(
        server,
        client.id,
        PlayerInput::Dig {
            action: DigAction::Start,
            pos,
            face: Direction::Up,
            sequence: 1,
        },
    );
}

#[test]
fn anvil_dropped_on_a_player_hurts_them() {
    let mut server = server_with(|config| {
        config.rules = GameRules {
            natural_regeneration: false,
            ..GameRules::default()
        };
    });
    let mut alice = join(&mut server, "Alice");
    settle(&mut server, &mut [&mut alice]);
    let (x, z) = ANVIL;
    walk(
        &mut server,
        &alice,
        Vec3::new(f64::from(x) + 0.5, 64.0, 0.5),
        true,
    );
    server.tick();
    walk(
        &mut server,
        &alice,
        Vec3::new(f64::from(x) + 0.5, 64.0, f64::from(z) + 0.5),
        true,
    );
    command(&mut server, &alice, "gamemode creative");
    dig(&mut server, &alice, BlockPos::new(x, ANVIL_Y - 1, z));
    command(&mut server, &alice, "gamemode survival");
    alice.drain();
    ticks(&mut server, &mut [&mut alice], 40);
    assert_eq!(server.falling_blocks(), 0);
    let hurt = 20.0 - health(&server, &alice);
    assert!(hurt == 10.0 || hurt == 12.0, "{hurt}");
    let events = alice.drain();
    assert_eq!(damage_kinds(&events), vec![DamageType::FallingAnvil]);
    assert!(events.iter().any(|e| matches!(
        e,
        ClientEvent::DamageEvent {
            cause: Some(_),
            direct: Some(_),
            position: None,
            ..
        }
    )));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ClientEvent::HurtAnimation { .. }))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ClientEvent::EntityVelocity { .. }))
    );
}

#[test]
fn void_kills_with_the_out_of_world_message() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    settle(&mut server, &mut [&mut alice]);
    command(&mut server, &alice, "tp 0.5 -200 0.5");
    ticks(&mut server, &mut [&mut alice], 9);
    assert_eq!(health(&server, &alice), 16.0);
    let mut waited = 0;
    while server.player_is_dead(alice.id) == Some(false) && waited < 200 {
        ticks(&mut server, &mut [&mut alice], 1);
        waited += 1;
    }
    assert_eq!(server.player_is_dead(alice.id), Some(true));
    let events = alice.drain();
    let message = events.iter().find_map(|e| match e {
        ClientEvent::CombatKill { message, .. } => Some(message.plain_text()),
        _ => None,
    });
    assert_eq!(message.as_deref(), Some("death.attack.outOfWorld Alice"));
    assert!(system_texts(&events).contains(&"death.attack.outOfWorld Alice".to_owned()));
}

#[test]
fn poison_stops_at_one_health_and_regeneration_heals() {
    let mut server = server_with(|config| {
        config.rules = GameRules {
            natural_regeneration: false,
            ..GameRules::default()
        };
    });
    let mut alice = join(&mut server, "Alice");
    settle(&mut server, &mut [&mut alice]);
    command(&mut server, &alice, "effect give @s poison 100 5");
    ticks(&mut server, &mut [&mut alice], 300);
    assert_eq!(health(&server, &alice), 1.0);
    let events = alice.drain();
    assert!(
        damage_kinds(&events)
            .iter()
            .all(|kind| *kind == DamageType::Magic)
    );
    assert_eq!(server.player_is_dead(alice.id), Some(false));
    command(&mut server, &alice, "effect clear @s poison");
    command(&mut server, &alice, "effect give @s regeneration 100 1");
    ticks(&mut server, &mut [&mut alice], 100);
    assert_eq!(health(&server, &alice), 5.0);
}

#[test]
fn instant_damage_drains_absorption_before_health() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    settle(&mut server, &mut [&mut alice]);
    command(&mut server, &alice, "effect give @s absorption 60 0");
    assert_eq!(server.player_absorption(alice.id), Some(4.0));
    command(&mut server, &alice, "effect give @s instant_damage");
    ticks(&mut server, &mut [&mut alice], 1);
    assert_eq!(server.player_absorption(alice.id), Some(0.0));
    assert_eq!(health(&server, &alice), 18.0);
    ticks(&mut server, &mut [&mut alice], 1);
    let events = alice.drain();
    assert!(events.iter().any(|e| matches!(
        e,
        ClientEvent::EffectRemoved { effect, .. }
            if effect.name() == Some("minecraft:absorption")
    )));
    command(&mut server, &alice, "effect give @s instant_health");
    ticks(&mut server, &mut [&mut alice], 1);
    assert_eq!(health(&server, &alice), 20.0);
}

#[test]
fn starving_at_zero_food_on_hard_hurts_every_eighty_ticks() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    settle(&mut server, &mut [&mut alice]);
    command(&mut server, &alice, "difficulty hard");
    command(&mut server, &alice, "effect give @s hunger 1000 255");
    let mut waited = 0;
    while server.player_food(alice.id).map(|f| f.0) != Some(0) && waited < 400 {
        ticks(&mut server, &mut [&mut alice], 1);
        waited += 1;
    }
    assert_eq!(server.player_food(alice.id).map(|f| f.0), Some(0));
    let before = health(&server, &alice);
    ticks(&mut server, &mut [&mut alice], 79);
    assert_eq!(health(&server, &alice), before - 1.0);
    ticks(&mut server, &mut [&mut alice], 80);
    assert_eq!(health(&server, &alice), before - 2.0);
    let events = alice.drain();
    assert!(damage_kinds(&events).contains(&DamageType::Starve));
    assert_eq!(last_health_packet(&events).map(|h| h.1), Some(0));
}

#[test]
fn pvp_hit_damages_and_knocks_back() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let mut bob = join(&mut server, "Bob");
    walk(&mut server, &bob, Vec3::new(1.5, 64.0, 0.5), true);
    ticks(&mut server, &mut [&mut alice, &mut bob], 30);
    let bob_entity = entity_of(&alice.events, bob.id);
    let alice_entity = entity_of(&bob.events, alice.id);
    alice.drain();
    bob.drain();
    let (Some(bob_entity), Some(_)) = (bob_entity, alice_entity) else {
        panic!("players never saw each other");
    };
    input(&mut server, alice.id, PlayerInput::Attack(bob_entity));
    assert_eq!(health(&server, &bob), 19.0);
    ticks(&mut server, &mut [&mut alice, &mut bob], 1);
    let events = bob.drain();
    assert_eq!(damage_kinds(&events), vec![DamageType::PlayerAttack]);
    let velocity = events.iter().find_map(|e| match e {
        ClientEvent::EntityVelocity { entity, velocity } if *entity == bob_entity => {
            Some(*velocity)
        }
        _ => None,
    });
    let Some(velocity) = velocity else {
        panic!("no knockback");
    };
    assert!(velocity.x > 0.3);
    assert!((velocity.y - 0.4).abs() < 1e-9);
    assert!(damage_kinds(&alice.drain()).contains(&DamageType::PlayerAttack));
    let (food, _, exhaustion) = server.player_food(alice.id).unwrap_or_default();
    assert_eq!(food, 20);
    assert!((exhaustion - 0.1).abs() < 1e-6);
    input(&mut server, alice.id, PlayerInput::Attack(bob_entity));
    assert_eq!(health(&server, &bob), 19.0);
}

#[test]
fn pvp_disabled_blocks_player_damage() {
    let mut server = server_with(|config| config.pvp = false);
    let mut alice = join(&mut server, "Alice");
    let mut bob = join(&mut server, "Bob");
    ticks(&mut server, &mut [&mut alice, &mut bob], 30);
    let Some(bob_entity) = entity_of(&alice.events, bob.id) else {
        panic!("Bob never appeared");
    };
    bob.drain();
    input(&mut server, alice.id, PlayerInput::Attack(bob_entity));
    ticks(&mut server, &mut [&mut alice, &mut bob], 1);
    assert_eq!(health(&server, &bob), 20.0);
    let events = bob.drain();
    assert!(damage_kinds(&events).is_empty());
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, ClientEvent::EntityVelocity { .. }))
    );
}

#[test]
fn death_broadcasts_sends_combat_kill_and_drops_the_inventory() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let mut bob = join(&mut server, "Bob");
    ticks(&mut server, &mut [&mut alice, &mut bob], 30);
    let alice_entity = entity_of(&bob.events, alice.id);
    alice.drain();
    bob.drain();
    command(&mut server, &alice, "give @s diamond 5");
    command(&mut server, &alice, "give @s stick 3");
    assert_eq!(server.items_on_ground(), 0);
    command(&mut server, &alice, "kill");
    assert_eq!(server.player_is_dead(alice.id), Some(true));
    assert_eq!(server.items_on_ground(), 2);
    ticks(&mut server, &mut [&mut alice, &mut bob], 1);
    let events = alice.drain();
    let kill = events.iter().find_map(|e| match e {
        ClientEvent::CombatKill { player, message } => Some((*player, message.plain_text())),
        _ => None,
    });
    assert_eq!(
        kill,
        alice_entity.map(|entity| (entity, "death.attack.genericKill Alice".to_owned()))
    );
    assert!(system_texts(&events).contains(&"commands.kill.success.single Alice".to_owned()));
    assert_eq!(last_health_packet(&events).map(|h| h.0), Some(0.0));
    let inventory_empty = events.iter().rev().find_map(|e| match e {
        ClientEvent::InventoryContent { slots, .. } => Some(slots.iter().all(Option::is_none)),
        _ => None,
    });
    assert_eq!(inventory_empty, Some(true));
    let seen = bob.drain();
    assert!(system_texts(&seen).contains(&"death.attack.genericKill Alice".to_owned()));
    assert!(seen.iter().any(|e| matches!(
        e,
        ClientEvent::EntityStatus {
            status: EntityStatus::Death,
            ..
        }
    )));
    ticks(&mut server, &mut [&mut alice, &mut bob], 20);
    let seen = bob.drain();
    assert!(seen.iter().any(|e| matches!(
        e,
        ClientEvent::EntityStatus {
            status: EntityStatus::Poof,
            ..
        }
    )));
    assert!(
        seen.iter()
            .any(|e| matches!(e, ClientEvent::RemoveEntities(ids) if alice_entity.is_some_and(|a| ids.contains(&a))))
    );
}

#[test]
fn respawn_restores_health_at_spawn_and_reappears_for_viewers() {
    let mut server = server_with(|config| config.rules.respawn_radius = 0);
    let mut alice = join(&mut server, "Alice");
    let mut bob = join(&mut server, "Bob");
    ticks(&mut server, &mut [&mut alice, &mut bob], 30);
    walk(&mut server, &alice, Vec3::new(3.5, 64.0, 2.5), true);
    command(&mut server, &alice, "effect give @s speed 100 1");
    command(&mut server, &alice, "kill");
    ticks(&mut server, &mut [&mut alice, &mut bob], 30);
    alice.drain();
    bob.drain();
    input(&mut server, alice.id, PlayerInput::Respawn);
    let events = alice.drain();
    let respawn = events.iter().find_map(|e| match e {
        ClientEvent::Respawn { spawn, keep_data } => {
            Some((spawn.death_location.clone(), *keep_data))
        }
        _ => None,
    });
    assert_eq!(
        respawn,
        Some((
            Some((Identifier::minecraft("overworld"), BlockPos::new(3, 64, 2))),
            0
        ))
    );
    let teleported = events.iter().find_map(|e| match e {
        ClientEvent::Teleport { position, .. } => Some(*position),
        _ => None,
    });
    assert_eq!(teleported, Some(Vec3::new(0.5, 64.0, 0.5)));
    assert_eq!(
        server.player_location(alice.id),
        Some(Vec3::new(0.5, 64.0, 0.5))
    );
    assert_eq!(server.player_is_dead(alice.id), Some(false));
    for event in &events {
        if let ClientEvent::Teleport { id: teleport, .. } = event {
            input(
                &mut server,
                alice.id,
                PlayerInput::TeleportConfirm(*teleport),
            );
        }
    }
    input(&mut server, alice.id, PlayerInput::Loaded);
    ticks(&mut server, &mut [&mut alice, &mut bob], 2);
    assert_eq!(last_health_packet(&alice.drain()), Some((20.0, 20, 5.0)));
    assert_eq!(health(&server, &alice), 20.0);
    assert!(entity_of(&bob.drain(), alice.id).is_some());
    input(&mut server, alice.id, PlayerInput::Respawn);
    assert!(
        !alice
            .drain()
            .iter()
            .any(|e| matches!(e, ClientEvent::Respawn { .. }))
    );
}

#[test]
fn kill_command_targets_other_players_and_needs_operator() {
    let mut server = server();
    let mut alice = join(&mut server, "Alice");
    let mut bob = join(&mut server, "Bob");
    ticks(&mut server, &mut [&mut alice, &mut bob], 5);
    alice.drain();
    command(&mut server, &bob, "kill Alice");
    assert_eq!(server.player_is_dead(alice.id), Some(false));
    command(&mut server, &alice, "kill Bob");
    assert_eq!(server.player_is_dead(bob.id), Some(true));
    assert!(system_texts(&alice.drain()).contains(&"commands.kill.success.single Bob".to_owned()));
    command(&mut server, &alice, "gamemode creative");
    command(&mut server, &alice, "kill");
    assert_eq!(server.player_is_dead(alice.id), Some(true));
}

#[test]
fn immediate_respawn_rule_hides_the_death_screen() {
    let mut server = server_with(|config| {
        config.rules = GameRules {
            immediate_respawn: true,
            ..GameRules::default()
        };
    });
    let link = Link::default();
    server.handle(Inbound::Join {
        profile: GameProfile {
            id: PlayerId::offline("Carol"),
            name: "Carol".to_owned(),
            properties: Vec::new(),
        },
        settings: ClientSettings::default(),
        link: link.clone(),
        address: std::net::Ipv4Addr::LOCALHOST.into(),
        session: 0,
    });
    server.deliver();
    let shown = link.take().iter().find_map(|e| match e {
        ClientEvent::Join(info) => Some(info.enable_respawn_screen),
        _ => None,
    });
    assert_eq!(shown, Some(false));
}

fn health_after_damage(adjust: impl FnOnce(&mut GameRules), kind: &str) -> f32 {
    let mut server = server_with(|config| adjust(&mut config.rules));
    let mut alice = join(&mut server, "Alice");
    settle(&mut server, &mut [&mut alice]);
    command(
        &mut server,
        &alice,
        &format!("damage @s 4 minecraft:{kind}"),
    );
    ticks(&mut server, &mut [&mut alice], 1);
    health(&server, &alice)
}

#[test]
fn the_damage_rules_spare_players_from_their_damage_type() {
    assert_eq!(health_after_damage(|_| {}, "drown"), 16.0);
    assert_eq!(
        health_after_damage(|rules| rules.drowning_damage = false, "drown"),
        20.0
    );
    assert_eq!(
        health_after_damage(|rules| rules.fall_damage = false, "fall"),
        20.0
    );
    assert_eq!(health_after_damage(|_| {}, "on_fire"), 16.0);
    assert_eq!(
        health_after_damage(|rules| rules.fire_damage = false, "on_fire"),
        20.0
    );
    assert_eq!(
        health_after_damage(|rules| rules.fire_damage = false, "lava"),
        20.0
    );
    assert_eq!(
        health_after_damage(|rules| rules.fire_damage = false, "generic"),
        16.0
    );
}

fn bounced_after_a_huge_step(adjust: impl FnOnce(&mut GameRules)) -> bool {
    let mut server = server_with(|config| adjust(&mut config.rules));
    let mut alice = join(&mut server, "Alice");
    settle(&mut server, &mut [&mut alice]);
    alice.drain();
    walk(&mut server, &alice, Vec3::new(500.5, 64.0, 0.5), true);
    alice
        .drain()
        .iter()
        .any(|event| matches!(event, ClientEvent::Teleport { .. }))
}

#[test]
fn the_player_movement_check_rule_decides_whether_a_huge_step_is_rejected() {
    assert!(bounced_after_a_huge_step(|_| {}));
    assert!(!bounced_after_a_huge_step(|rules| {
        rules.player_movement_check = false;
    }));
}

#[test]
fn the_combat_exhaustion_setting_scales_what_an_attack_costs() {
    let mut server = server_with(|config| config.combat_exhaustion = 0.5);
    let mut alice = join(&mut server, "Alice");
    let mut bob = join(&mut server, "Bob");
    ticks(&mut server, &mut [&mut alice, &mut bob], 30);
    let Some(bob_entity) = entity_of(&alice.events, bob.id) else {
        panic!("Bob never appeared");
    };
    input(&mut server, alice.id, PlayerInput::Attack(bob_entity));
    let (_, _, exhaustion) = server.player_food(alice.id).unwrap_or_default();
    assert!((exhaustion - 0.5).abs() < 1e-6, "{exhaustion}");
}
