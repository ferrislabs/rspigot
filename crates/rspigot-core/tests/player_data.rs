use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rspigot_core::admin::{AdminLists, ListEntry};
use rspigot_core::chunks::ChunkManagerConfig;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{
    Abilities, ClickInput, ClientEvent, ContainerClick, Direction, Hand, HashedSlot, Inbound,
    PlayerInput,
};
use rspigot_core::plugins::{FromPlugin, Order, PlayerOrder, PluginIntro, ToPlugin};
use rspigot_core::ports::{
    ChunkGenerator, ChunkStore, ClientLink, EntityStore, PlayerStore, PluginOutbox, StoreError,
};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::player::{ClientSettings, GameMode, GameProfile, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Identifier, Vec3};
use rspigot_items::ItemKind;
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
    ) -> Result<Option<Vec<Compound>>, StoreError> {
        Ok(None)
    }

    fn save_entities(&self, _: &Identifier, _: ChunkPos, _: &[Compound]) -> Result<(), StoreError> {
        Ok(())
    }
}

#[derive(Default)]
struct Players {
    files: Mutex<HashMap<PlayerId, Compound>>,
    fail_loads: AtomicBool,
}

impl Players {
    fn saved(&self, player: PlayerId) -> Option<Compound> {
        self.files.lock().ok()?.get(&player).cloned()
    }

    fn wait_saved(&self, player: PlayerId) -> Option<Compound> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(saved) = self.saved(player) {
                return Some(saved);
            }
            if Instant::now() > deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl PlayerStore for Players {
    fn load_player(&self, player: PlayerId) -> Result<Option<Compound>, StoreError> {
        if self.fail_loads.load(Ordering::Acquire) {
            return Err(StoreError::CorruptPlayer {
                player,
                reason: "broken".to_owned(),
            });
        }
        Ok(self.saved(player))
    }

    fn save_player(&self, player: PlayerId, data: &Compound) -> Result<(), StoreError> {
        if let Ok(mut files) = self.files.lock() {
            files.insert(player, data.clone());
        }
        Ok(())
    }
}

struct Floor;

impl ChunkGenerator for Floor {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(0));
        for x in 0..16 {
            for z in 0..16 {
                for y in 60..=63 {
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        BlockStateId(1),
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

const SPAWN: Vec3 = Vec3::new(0.5, 64.0, 0.5);

fn profile(name: &str) -> GameProfile {
    GameProfile {
        id: PlayerId::offline(name),
        name: name.to_owned(),
        properties: Vec::new(),
    }
}

fn server(players: &Arc<Players>, autosave_ticks: u64) -> Server<Link> {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        chunks: ChunkManagerConfig {
            autosave_interval_ticks: autosave_ticks,
            ..ChunkManagerConfig::default()
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
        Arc::new(NoStore),
        Arc::new(Floor),
        StatusBoard::new(20),
    )
    .with_player_store(Arc::clone(players));
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn join(server: &mut Server<Link>, name: &str) -> (PlayerId, Link, Vec<ClientEvent>) {
    let link = Link::default();
    let profile = profile(name);
    let id = profile.id;
    server.handle(Inbound::Join {
        profile,
        settings: ClientSettings::default(),
        link: link.clone(),
        address: std::net::Ipv4Addr::LOCALHOST.into(),
        session: 0,
    });
    server.deliver();
    let mut events = link.take();
    for event in events.clone() {
        if let ClientEvent::Teleport { id: teleport, .. } = event {
            server.handle(Inbound::Input {
                player: id,
                input: PlayerInput::TeleportConfirm(teleport),
            });
        }
    }
    server.handle(Inbound::Input {
        player: id,
        input: PlayerInput::Loaded,
    });
    run_ticks(server, id, &link, 30, &mut events);
    (id, link, events)
}

fn run_ticks(
    server: &mut Server<Link>,
    id: PlayerId,
    link: &Link,
    ticks: usize,
    log: &mut Vec<ClientEvent>,
) {
    for _ in 0..ticks {
        server.wait_for_spawn(Duration::from_millis(20));
        server.tick();
        let events = link.take();
        let batches = events
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
        log.extend(events);
    }
}

struct Deaf;

impl PluginOutbox for Deaf {
    fn send(&self, _: ToPlugin) {}
}

fn connect_plugin(server: &mut Server<Link>) {
    server.handle(Inbound::PluginConnected {
        connection: 1,
        intro: PluginIntro {
            commands: Vec::new(),
            name: "tester".to_owned(),
            version: "1.0.0".to_owned(),
            handlers: Vec::new(),
        },
        outbox: Box::new(Deaf),
    });
}

fn order(server: &mut Server<Link>, player: PlayerId, order: PlayerOrder) {
    server.handle(Inbound::PluginMessage {
        connection: 1,
        message: FromPlugin::Order(Order::Player { player, order }),
    });
    server.deliver();
}

fn abilities(events: &[ClientEvent]) -> Vec<Abilities> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Abilities(abilities) => Some(*abilities),
            _ => None,
        })
        .collect()
}

fn console(server: &mut Server<Link>, line: &str) {
    server.handle(Inbound::Console(line.to_owned()));
    server.deliver();
}

fn join_game_mode(events: &[ClientEvent]) -> Option<GameMode> {
    events.iter().find_map(|event| match event {
        ClientEvent::Join(info) => Some(info.spawn.game_mode),
        _ => None,
    })
}

fn first_position(events: &[ClientEvent]) -> Option<Vec3> {
    events.iter().find_map(|event| match event {
        ClientEvent::Teleport { position, .. } => Some(*position),
        _ => None,
    })
}

fn last_health(events: &[ClientEvent]) -> Option<f32> {
    events.iter().rev().find_map(|event| match event {
        ClientEvent::Health { health, .. } => Some(*health),
        _ => None,
    })
}

fn holds_diamonds(events: &[ClientEvent], count: u8) -> bool {
    let diamond = ItemKind::from_name("minecraft:diamond").map(ItemKind::id);
    events.iter().any(|event| match event {
        ClientEvent::InventoryContent { slots, .. } => slots
            .iter()
            .flatten()
            .any(|stack| Some(stack.item) == diamond && stack.count.get() == count),
        _ => false,
    })
}

#[test]
fn a_new_player_spawns_at_the_world_spawn_with_the_default_game_mode() {
    let players = Arc::new(Players::default());
    let mut server = server(&players, 6000);
    let (_, _, events) = join(&mut server, "Alice");
    assert_eq!(first_position(&events), Some(SPAWN));
    assert_eq!(join_game_mode(&events), Some(GameMode::Survival));
    assert!(!holds_diamonds(&events, 3));
}

#[test]
fn position_game_mode_and_inventory_survive_a_reconnect() {
    let players = Arc::new(Players::default());
    let mut server = server(&players, 6000);
    let (alice, link, mut events) = join(&mut server, "Alice");
    console(&mut server, "gamemode creative Alice");
    console(&mut server, "give Alice minecraft:diamond 3");
    console(&mut server, "tp Alice 100 64 100");
    run_ticks(&mut server, alice, &link, 5, &mut events);

    server.handle(Inbound::Leave {
        player: alice,
        session: 0,
    });
    server.deliver();
    assert_eq!(server.players_online(), 0);
    assert!(players.wait_saved(alice).is_some());

    let (_, _, events) = join(&mut server, "Alice");
    assert_eq!(join_game_mode(&events), Some(GameMode::Creative));
    assert_eq!(first_position(&events), Some(Vec3::new(100.5, 64.0, 100.5)));
    assert!(holds_diamonds(&events, 3));
}

#[test]
fn a_survival_player_a_plugin_made_fly_comes_back_not_flying() {
    let players = Arc::new(Players::default());
    let mut server = server(&players, 6000);
    connect_plugin(&mut server);
    let (alice, link, _) = join(&mut server, "Alice");
    order(&mut server, alice, PlayerOrder::SetAllowFlight(true));
    order(&mut server, alice, PlayerOrder::SetFlying(true));
    let granted = abilities(&link.take());
    assert!(granted.last().is_some_and(|a| a.may_fly && a.flying));

    server.handle(Inbound::Leave {
        player: alice,
        session: 0,
    });
    server.deliver();
    let saved = players.wait_saved(alice).unwrap();
    assert_eq!(
        saved
            .get_compound("abilities")
            .and_then(|abilities| abilities.get_bool("flying")),
        Some(true)
    );

    let (_, _, events) = join(&mut server, "Alice");
    assert_eq!(join_game_mode(&events), Some(GameMode::Survival));
    let joined = abilities(&events);
    assert!(!joined.is_empty());
    assert!(!joined[0].may_fly);
    assert!(!joined[0].flying);
}

#[test]
fn a_dead_player_comes_back_at_the_world_spawn_with_full_health() {
    let players = Arc::new(Players::default());
    let mut server = server(&players, 6000);
    let (alice, link, mut events) = join(&mut server, "Alice");
    console(&mut server, "tp Alice 100 64 100");
    console(&mut server, "kill Alice");
    run_ticks(&mut server, alice, &link, 5, &mut events);

    server.handle(Inbound::Leave {
        player: alice,
        session: 0,
    });
    server.deliver();

    let (_, _, events) = join(&mut server, "Alice");
    assert_eq!(first_position(&events), Some(SPAWN));
    assert_eq!(last_health(&events), Some(20.0));
}

#[test]
fn unreadable_player_data_does_not_block_the_login() {
    let players = Arc::new(Players::default());
    let mut server = server(&players, 6000);
    let (alice, _, _) = join(&mut server, "Alice");
    server.handle(Inbound::Leave {
        player: alice,
        session: 0,
    });
    server.deliver();
    players.fail_loads.store(true, Ordering::Release);

    let (_, _, events) = join(&mut server, "Alice");
    assert_eq!(server.players_online(), 1);
    assert_eq!(first_position(&events), Some(SPAWN));
}

#[test]
fn online_players_are_saved_on_the_autosave_tick() {
    let players = Arc::new(Players::default());
    let mut server = server(&players, 40);
    let (alice, link, mut events) = join(&mut server, "Alice");
    assert!(players.saved(alice).is_none());
    console(&mut server, "tp Alice 100 64 100");
    run_ticks(&mut server, alice, &link, 60, &mut events);

    let saved = players
        .wait_saved(alice)
        .expect("autosave wrote the player");
    let position = saved.get_list("Pos").expect("Pos");
    assert_eq!(position[0].as_f64(), Some(100.5));
    assert_eq!(server.players_online(), 1);
}

fn window_of(events: &[ClientEvent]) -> Option<i32> {
    events.iter().rev().find_map(|event| match event {
        ClientEvent::OpenScreen { window, .. } => Some(*window),
        _ => None,
    })
}

fn ender_slot(events: &[ClientEvent], window: i32, slot: usize) -> Option<(u16, u8)> {
    events.iter().rev().find_map(|event| match event {
        ClientEvent::ContainerContent {
            window: at, slots, ..
        } if *at == window => slots
            .get(slot)
            .and_then(Option::as_ref)
            .map(|stack| (stack.item.0, stack.count.get())),
        _ => None,
    })
}

fn use_block(server: &mut Server<Link>, id: PlayerId, pos: BlockPos, sequence: i32) {
    server.handle(Inbound::Input {
        player: id,
        input: PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence,
        },
    });
    server.deliver();
}

fn click(server: &mut Server<Link>, id: PlayerId, window: i32, state: i32, slot: i16) {
    server.handle(Inbound::Input {
        player: id,
        input: PlayerInput::ContainerClick(ContainerClick {
            container: window,
            state_id: state,
            slot,
            button: 0,
            input: ClickInput::Pickup,
            changed: Vec::new(),
            carried: HashedSlot::default(),
        }),
    });
    server.deliver();
}

fn state_of(events: &[ClientEvent], window: i32) -> i32 {
    events
        .iter()
        .rev()
        .find_map(|event| match event {
            ClientEvent::ContainerContent {
                window: at,
                state_id,
                ..
            }
            | ClientEvent::ContainerSlot {
                window: at,
                state_id,
                ..
            } if *at == window => Some(*state_id),
            _ => None,
        })
        .unwrap_or(0)
}

#[test]
fn ender_chest_items_survive_leaving_and_rejoining() {
    let players = Arc::new(Players::default());
    let mut server = server(&players, 6000);
    let (alice, link, mut events) = join(&mut server, "Alice");
    console(&mut server, "give Alice minecraft:ender_chest");
    console(&mut server, "give Alice minecraft:stone");
    let chest = BlockPos::new(2, 64, 2);
    server.handle(Inbound::Input {
        player: alice,
        input: PlayerInput::HeldSlot(0),
    });
    use_block(&mut server, alice, BlockPos::new(2, 63, 2), 1);
    server.handle(Inbound::Input {
        player: alice,
        input: PlayerInput::HeldSlot(8),
    });
    use_block(&mut server, alice, chest, 2);
    run_ticks(&mut server, alice, &link, 2, &mut events);
    let window = window_of(&events).expect("the ender chest opened");

    let state = state_of(&events, window);
    click(&mut server, alice, window, state, 55);
    click(&mut server, alice, window, state, 4);
    server.handle(Inbound::Input {
        player: alice,
        input: PlayerInput::ContainerClose(window),
    });
    server.handle(Inbound::Leave {
        player: alice,
        session: 0,
    });
    server.deliver();
    assert!(players.wait_saved(alice).is_some());

    let (alice, link, mut events) = join(&mut server, "Alice");
    server.handle(Inbound::Input {
        player: alice,
        input: PlayerInput::HeldSlot(8),
    });
    use_block(&mut server, alice, chest, 3);
    run_ticks(&mut server, alice, &link, 2, &mut events);
    let window = window_of(&events).expect("the ender chest opened again");

    let stone = rspigot_items::ItemKind::from_name("minecraft:stone").map(|kind| kind.id().0);
    let kept = ender_slot(&events, window, 4);
    assert_eq!(kept.map(|(item, _)| item), stone);
}
