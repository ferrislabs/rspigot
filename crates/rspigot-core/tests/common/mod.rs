#![allow(dead_code)]

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rspigot_core::config::{GameConfig, LevelData, PluginSettings};
use rspigot_core::events::{ClientEvent, Inbound, PlayerInput};
use rspigot_core::player_data::PlayerSave;
use rspigot_core::plugins::{
    Access, Answer, EventKind, EventPatch, EventPayload, FromPlugin, HandlerSpec, ListDiff, Order,
    Phase, PlayerOrder, PluginIntro, Priority, Question, ToPlugin, Verdict,
};
use rspigot_core::ports::{
    ChunkGenerator, ChunkStore, ClientLink, EntityStore, PlayerStore, PluginOutbox, StoreError,
};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::{ClientSettings, GameProfile, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Identifier, Vec3};
use rspigot_nbt::Compound;

static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Default)]
pub struct Link {
    events: Arc<Mutex<Vec<ClientEvent>>>,
    closed: Arc<AtomicBool>,
    reason: Arc<Mutex<Option<Component>>>,
}

impl Link {
    pub fn take(&self) -> Vec<ClientEvent> {
        self.events
            .lock()
            .map(|mut e| std::mem::take(&mut *e))
            .unwrap_or_default()
    }

    pub fn disconnected_with(&self) -> Option<Component> {
        self.reason.lock().ok().and_then(|reason| reason.clone())
    }
}

impl ClientLink for Link {
    fn send(&self, event: ClientEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
    }

    fn disconnect(&self, reason: Component) {
        if let Ok(mut last) = self.reason.lock() {
            *last = Some(reason);
        }
        self.closed.store(true, Ordering::Release);
    }

    fn is_connected(&self) -> bool {
        !self.closed.load(Ordering::Acquire)
    }
}

pub struct NoStore;

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

pub struct Floor;

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

#[derive(Clone, Default)]
pub struct Mailbox(Arc<Mutex<Vec<ToPlugin>>>);

impl Mailbox {
    pub fn received(&self) -> Vec<ToPlugin> {
        self.0.lock().map(|m| m.clone()).unwrap_or_default()
    }
}

impl PluginOutbox for Mailbox {
    fn send(&self, message: ToPlugin) {
        if let Ok(mut inbox) = self.0.lock() {
            inbox.push(message);
        }
    }
}

#[derive(Clone)]
pub struct Player {
    pub id: PlayerId,
    pub link: Link,
    pub session: u64,
}

pub fn leave_of(player: &Player) -> Inbound<Link> {
    Inbound::Leave {
        player: player.id,
        session: player.session,
    }
}

pub fn server(plugins: PluginSettings) -> Server<Link> {
    configured(|config| config.plugins = plugins)
}

pub fn configured(adjust: impl FnOnce(&mut GameConfig)) -> Server<Link> {
    configured_level(adjust, |_| {})
}

pub fn calm_server() -> Server<Link> {
    configured_level(|_| {}, |level| level.day_time = 18_000)
}

pub fn configured_level(
    adjust: impl FnOnce(&mut GameConfig),
    level: impl FnOnce(&mut LevelData),
) -> Server<Link> {
    let mut config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    adjust(&mut config);
    let mut data = LevelData {
        spawn_mobs: false,
        ..LevelData::default()
    };
    level(&mut data);
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(Floor),
        StatusBoard::new(20),
    );
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

pub fn join(server: &mut Server<Link>, name: &str) -> Player {
    join_from(server, name, IpAddr::from([127, 0, 0, 1]))
}

pub fn join_from(server: &mut Server<Link>, name: &str, address: IpAddr) -> Player {
    join_as(server, name, address, ClientSettings::default())
}

pub fn join_with(server: &mut Server<Link>, name: &str, settings: ClientSettings) -> Player {
    join_as(server, name, IpAddr::from([127, 0, 0, 1]), settings)
}

pub fn arrive(server: &mut Server<Link>, name: &str) -> Player {
    let link = Link::default();
    let id = PlayerId::offline(name);
    let session = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
    server.handle(Inbound::Join {
        profile: GameProfile {
            id,
            name: name.to_owned(),
            properties: Vec::new(),
        },
        settings: ClientSettings::default(),
        link: link.clone(),
        address: IpAddr::from([127, 0, 0, 1]),
        session,
    });
    server.deliver();
    Player { id, link, session }
}

pub fn settle_arrival(server: &mut Server<Link>, player: &Player) {
    for event in player.link.take() {
        if let ClientEvent::Teleport { id: teleport, .. } = event {
            server.handle(Inbound::Input {
                player: player.id,
                input: PlayerInput::TeleportConfirm(teleport),
            });
        }
    }
    server.handle(Inbound::Input {
        player: player.id,
        input: PlayerInput::Loaded,
    });
}

fn join_as(
    server: &mut Server<Link>,
    name: &str,
    address: IpAddr,
    settings: ClientSettings,
) -> Player {
    let link = Link::default();
    let id = PlayerId::offline(name);
    let session = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
    server.handle(Inbound::Join {
        profile: GameProfile {
            id,
            name: name.to_owned(),
            properties: Vec::new(),
        },
        settings,
        link: link.clone(),
        address,
        session,
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
    server.handle(Inbound::Input {
        player: id,
        input: PlayerInput::Loaded,
    });
    Player { id, link, session }
}

pub fn observe(id: u32, event: EventKind) -> HandlerSpec {
    HandlerSpec {
        id,
        event,
        access: Access::Observe,
        priority: Priority::Normal,
        ignore_cancelled: false,
    }
}

pub fn decide(id: u32) -> HandlerSpec {
    HandlerSpec {
        id,
        event: EventKind::PlayerMove,
        access: Access::Decide,
        priority: Priority::Normal,
        ignore_cancelled: false,
    }
}

pub fn plug(
    server: &mut Server<Link>,
    connection: u64,
    name: &str,
    handlers: Vec<HandlerSpec>,
    outbox: Box<dyn PluginOutbox>,
) {
    server.handle(Inbound::PluginConnected {
        connection,
        intro: PluginIntro {
            name: name.to_owned(),
            version: "1.0.0".to_owned(),
            handlers,
        },
        outbox,
    });
}

pub fn connect_with(
    server: &mut Server<Link>,
    connection: u64,
    name: &str,
    handlers: Vec<HandlerSpec>,
) -> Mailbox {
    let mailbox = Mailbox::default();
    plug(
        server,
        connection,
        name,
        handlers,
        Box::new(mailbox.clone()),
    );
    mailbox
}

pub fn connect(server: &mut Server<Link>, connection: u64, name: &str) -> Mailbox {
    connect_with(server, connection, name, Vec::new())
}

pub fn heard(mailbox: &Mailbox) -> Vec<ToPlugin> {
    mailbox
        .received()
        .into_iter()
        .filter(|message| *message != ToPlugin::Accepted)
        .collect()
}

pub fn order(server: &mut Server<Link>, connection: u64, message: FromPlugin) {
    server.handle(Inbound::PluginMessage {
        connection,
        message,
    });
    server.deliver();
}

pub fn plugins_line(server: &mut Server<Link>, player: &Player) -> String {
    server.handle(Inbound::Input {
        player: player.id,
        input: PlayerInput::Command("plugins".to_owned()),
    });
    server.deliver();
    player
        .link
        .take()
        .into_iter()
        .find_map(|event| match event {
            ClientEvent::SystemChat { message, .. } => Some(message.plain_text()),
            _ => None,
        })
        .expect("the plugins command answers")
}

pub fn texts(player: &Player) -> Vec<(String, bool)> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::SystemChat { message, overlay } => Some((message.plain_text(), overlay)),
            _ => None,
        })
        .collect()
}

pub struct Scene {
    pub server: Server<Link>,
    pub plugin: Mailbox,
    pub players: Vec<Player>,
}

pub fn scene(names: &[&str]) -> Scene {
    scene_with(names, Vec::new())
}

pub fn scene_with(names: &[&str], handlers: Vec<HandlerSpec>) -> Scene {
    scene_on(server(PluginSettings::default()), names, handlers)
}

#[derive(Default)]
pub struct Saves(Mutex<HashMap<PlayerId, Compound>>);

impl Saves {
    pub fn items(&self, player: PlayerId) -> Vec<(String, u8)> {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let saved = loop {
            let found = self
                .0
                .lock()
                .ok()
                .and_then(|files| files.get(&player).cloned());
            match found {
                Some(saved) => break saved,
                None if std::time::Instant::now() > deadline => panic!("the player was not saved"),
                None => std::thread::sleep(Duration::from_millis(5)),
            }
        };
        let save = PlayerSave::from_nbt(&saved).expect("the save decodes");
        save.inventory
            .slots()
            .iter()
            .flatten()
            .map(|stack: &ItemStack| {
                let name = stack.item.kind().map_or("unknown", |kind| kind.name());
                (name.to_owned(), stack.count.get())
            })
            .collect()
    }
}

impl PlayerStore for Saves {
    fn load_player(&self, player: PlayerId) -> Result<Option<Compound>, StoreError> {
        Ok(self
            .0
            .lock()
            .ok()
            .and_then(|files| files.get(&player).cloned()))
    }

    fn save_player(&self, player: PlayerId, data: &Compound) -> Result<(), StoreError> {
        if let Ok(mut files) = self.0.lock() {
            files.insert(player, data.clone());
        }
        Ok(())
    }
}

pub fn saving_scene(names: &[&str], handlers: Vec<HandlerSpec>) -> (Scene, Arc<Saves>) {
    let saves = Arc::new(Saves::default());
    let server = server(PluginSettings::default()).with_player_store(Arc::clone(&saves));
    (scene_on(server, names, handlers), saves)
}

pub fn scene_on(mut server: Server<Link>, names: &[&str], handlers: Vec<HandlerSpec>) -> Scene {
    let plugin = connect_with(&mut server, 1, "tester", handlers);
    let players: Vec<Player> = names.iter().map(|name| join(&mut server, name)).collect();
    server.deliver();
    for player in &players {
        player.link.take();
    }
    Scene {
        server,
        plugin,
        players,
    }
}

impl Scene {
    pub fn act(&mut self, order: Order) {
        self.server.handle(Inbound::PluginMessage {
            connection: 1,
            message: FromPlugin::Order(order),
        });
        self.server.deliver();
    }

    pub fn ask(&mut self, question: Question) -> Answer {
        self.ask_on(1, question)
    }

    pub fn ask_on(&mut self, connection: u64, question: Question) -> Answer {
        let request = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed);
        self.server.handle(Inbound::PluginMessage {
            connection,
            message: FromPlugin::Question { request, question },
        });
        self.server.deliver();
        self.plugin
            .received()
            .into_iter()
            .find_map(|message| match message {
                ToPlugin::Answer {
                    request: answered,
                    answer,
                } if answered == request => Some(answer),
                _ => None,
            })
            .expect("the server answers the question")
    }

    pub fn warnings(&self) -> Vec<String> {
        self.plugin
            .received()
            .into_iter()
            .filter_map(|message| match message {
                ToPlugin::Warning(text) => Some(text),
                _ => None,
            })
            .collect()
    }

    pub fn tick(&mut self, ticks: u32) {
        for _ in 0..ticks {
            self.server.tick();
            self.server.deliver();
        }
    }
}

pub fn stream_chunk_of(s: &mut Scene, pos: BlockPos) {
    let mut waiting = s.players.clone();
    for _ in 0..40 {
        if waiting.is_empty() {
            break;
        }
        s.tick(1);
        waiting.retain(|player| {
            !player.link.take().iter().any(
                |event| matches!(event, ClientEvent::Chunk { chunk, .. } if chunk.pos() == pos.chunk()),
            )
        });
    }
    assert!(waiting.is_empty(), "every client received the chunk");
}

pub fn to(player: &Player, order: PlayerOrder) -> Order {
    Order::Player {
        player: player.id,
        order,
    }
}

pub fn deciding(id: u32, event: EventKind) -> HandlerSpec {
    HandlerSpec {
        id,
        event,
        access: Access::Decide,
        priority: Priority::Normal,
        ignore_cancelled: false,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fired {
    pub id: u64,
    pub phase: Phase,
    pub cancelled: bool,
    pub payload: EventPayload,
}

pub fn fired(mailbox: &Mailbox) -> Vec<Fired> {
    mailbox
        .received()
        .into_iter()
        .filter_map(|message| match message {
            ToPlugin::Event {
                id,
                phase,
                cancelled,
                payload,
                ..
            } => Some(Fired {
                id,
                phase,
                cancelled,
                payload,
            }),
            _ => None,
        })
        .collect()
}

pub fn fired_kinds(mailbox: &Mailbox) -> Vec<(Phase, EventKind)> {
    fired(mailbox)
        .into_iter()
        .map(|event| (event.phase, event.payload.kind()))
        .collect()
}

pub fn asked_for(mailbox: &Mailbox, kind: EventKind) -> Vec<Fired> {
    fired(mailbox)
        .into_iter()
        .filter(|event| event.phase == Phase::Decide && event.payload.kind() == kind)
        .collect()
}

pub fn settled_for(mailbox: &Mailbox, kind: EventKind) -> Vec<Fired> {
    fired(mailbox)
        .into_iter()
        .filter(|event| event.phase == Phase::Settled && event.payload.kind() == kind)
        .collect()
}

pub fn last_asked(mailbox: &Mailbox, kind: EventKind) -> Fired {
    asked_for(mailbox, kind)
        .pop()
        .unwrap_or_else(|| panic!("a {kind:?} was asked"))
}

pub fn rule_on(
    server: &mut Server<Link>,
    connection: u64,
    event: u64,
    cancel: bool,
    patch: Option<EventPatch>,
) {
    judge(server, connection, event, 1, cancel, patch);
}

pub fn judge(
    server: &mut Server<Link>,
    connection: u64,
    event: u64,
    handler: u32,
    cancel: bool,
    patch: Option<EventPatch>,
) {
    server.handle(Inbound::PluginMessage {
        connection,
        message: FromPlugin::Decision {
            event,
            verdicts: vec![Verdict {
                handler,
                cancel,
                patch,
            }],
        },
    });
    server.deliver();
}

pub fn send_input(server: &mut Server<Link>, player: &Player, input: PlayerInput) {
    server.handle(Inbound::Input {
        player: player.id,
        input,
    });
    server.deliver();
}

pub fn system_texts(player: &Player) -> Vec<String> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::SystemChat { message, .. } => {
                Some(rspigot_core::console::console_text(&message))
            }
            _ => None,
        })
        .collect()
}

fn indices<T>(snapshot: &[T], chosen: impl Fn(&T) -> bool) -> Vec<u32> {
    snapshot
        .iter()
        .enumerate()
        .filter(|(_, value)| chosen(value))
        .filter_map(|(index, _)| u32::try_from(index).ok())
        .collect()
}

pub fn removing<T, const SET: bool>(snapshot: &[T], gone: impl Fn(&T) -> bool) -> ListDiff<T, SET> {
    ListDiff {
        removed: indices(snapshot, gone),
        replaced: Vec::new(),
        added: Vec::new(),
    }
}

pub fn replacing<T, const SET: bool>(snapshot: &[T], added: Vec<T>) -> ListDiff<T, SET> {
    ListDiff {
        removed: indices(snapshot, |_| true),
        replaced: Vec::new(),
        added,
    }
}
