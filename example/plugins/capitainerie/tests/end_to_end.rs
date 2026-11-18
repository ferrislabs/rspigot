use std::collections::HashMap;
use std::net::SocketAddr;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{Form, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use crossbeam_channel::{Receiver, RecvTimeoutError};
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, EntityKind, Hand, Inbound, MetadataValue, PlayerInput};
use rspigot_core::ports::{ChunkGenerator, ChunkStore, ClientLink, EntityStore, StoreError};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::player::{ClientSettings, GameProfile, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Identifier, Vec3};
use rspigot_nbt::Compound;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::watch;

const TICK: Duration = Duration::from_millis(50);
const PATIENCE: Duration = Duration::from_secs(20);
const SPAWN: Vec3 = Vec3::new(0.5, 64.0, 0.5);

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
        SPAWN
    }
}

struct Player {
    id: PlayerId,
    link: Link,
    log: Vec<ClientEvent>,
}

impl Player {
    fn heard(&mut self) -> &[ClientEvent] {
        self.log.extend(self.link.take());
        &self.log
    }

    fn received_chunk(&mut self, pos: ChunkPos) -> bool {
        self.heard()
            .iter()
            .any(|event| matches!(event, ClientEvent::Chunk { chunk, .. } if chunk.pos() == pos))
    }

    fn npc_entity(&mut self, name: &str) -> Option<i32> {
        let log = self.heard();
        let uuid = log.iter().find_map(|event| match event {
            ClientEvent::PlayerInfo(entries) => entries
                .iter()
                .find(|entry| entry.profile.name == name)
                .map(|entry| entry.profile.id.0),
            _ => None,
        })?;
        log.iter().find_map(|event| match event {
            ClientEvent::SpawnEntity {
                entity,
                uuid: spawned,
                kind: EntityKind::Player,
                ..
            } if *spawned == uuid => Some(entity.0),
            _ => None,
        })
    }

    fn label_has(&mut self, text: &str) -> bool {
        self.heard().iter().any(|event| match event {
            ClientEvent::EntityMetadata { entries, .. } => entries.iter().any(|entry| {
                matches!(&entry.value, MetadataValue::Component(shown) if shown.plain_text().contains(text))
            }),
            _ => false,
        })
    }

    fn said(&mut self, text: &str) -> bool {
        self.heard().iter().any(|event| {
            matches!(event, ClientEvent::SystemChat { message, .. } if message.plain_text().contains(text))
        })
    }

    fn dialog_title(&mut self) -> Option<String> {
        self.heard().iter().rev().find_map(|event| match event {
            ClientEvent::ShowDialog(dialog) => Some(dialog.title.plain_text()),
            _ => None,
        })
    }

    fn dialog_buttons(&mut self) -> Vec<String> {
        self.heard()
            .iter()
            .rev()
            .find_map(|event| match event {
                ClientEvent::ShowDialog(dialog) => Some(
                    dialog
                        .buttons
                        .iter()
                        .map(|button| button.id.to_string())
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn broadcast_chat_seen(&mut self) -> bool {
        self.heard()
            .iter()
            .any(|event| matches!(event, ClientEvent::PlayerChat { .. }))
    }
}

fn server() -> Server<Link> {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    let mut server = Server::new(
        config,
        LevelData::default(),
        Arc::new(NoStore),
        Arc::new(Floor),
        StatusBoard::new(20),
    );
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn join(server: &mut Server<Link>, name: &str) -> Player {
    let link = Link::default();
    let id = PlayerId::offline(name);
    server.handle(Inbound::Join {
        profile: GameProfile {
            id,
            name: name.to_owned(),
            properties: Vec::new(),
        },
        settings: ClientSettings::default(),
        link: link.clone(),
        address: std::net::Ipv4Addr::LOCALHOST.into(),
        session: 0,
    });
    server.deliver();
    server.tick();
    let mut player = Player {
        id,
        link,
        log: Vec::new(),
    };
    assert!(
        player.received_chunk(SPAWN.chunk()),
        "{name} receives the spawn chunk before loading in"
    );
    confirm_teleports(server, &player);
    server.handle(Inbound::Input {
        player: id,
        input: PlayerInput::Loaded,
    });
    player
}

fn confirm_teleports(server: &mut Server<Link>, player: &Player) {
    for event in &player.log {
        if let ClientEvent::Teleport { id: teleport, .. } = event {
            server.handle(Inbound::Input {
                player: player.id,
                input: PlayerInput::TeleportConfirm(*teleport),
            });
        }
    }
}

struct PluginProcess(Child);

impl PluginProcess {
    fn start(host: SocketAddr, fake: SocketAddr) -> Self {
        let child = Command::new(env!("CARGO_BIN_EXE_capitainerie"))
            .env("RSPIGOT_URL", format!("http://{host}"))
            .env_remove("RSPIGOT_TOKEN")
            .env("CAPITAINERIE_URL", format!("http://{fake}"))
            .env("CAPITAINERIE_TOKEN_URL", format!("http://{fake}/token"))
            .env("CAPITAINERIE_CLIENT_ID", "capitainerie-minecraft")
            .env("CAPITAINERIE_USERNAME", "bridge")
            .env("CAPITAINERIE_PASSWORD", "s3cret")
            .env("CAPITAINERIE_POLL_SECONDS", "1")
            .spawn()
            .expect("the capitainerie plugin starts");
        Self(child)
    }
}

impl Drop for PluginProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Core {
    server: Server<Link>,
    inbox: Receiver<Inbound<Link>>,
    plugin: Option<u64>,
    next_tick: Instant,
}

impl Core {
    fn new(inbox: Receiver<Inbound<Link>>) -> Self {
        Self {
            server: server(),
            inbox,
            plugin: None,
            next_tick: Instant::now(),
        }
    }

    fn run_until(&mut self, what: &str, mut done: impl FnMut(&Self) -> bool) {
        let given_up = Instant::now() + PATIENCE;
        while !done(self) {
            let now = Instant::now();
            assert!(now < given_up, "gave up waiting until {what}");
            if now >= self.next_tick {
                let inbox = self.inbox.clone();
                let waiting = inbox.len();
                for message in inbox.try_iter().take(waiting) {
                    self.handle(message);
                }
                self.server.tick();
                self.next_tick += TICK;
                continue;
            }
            match self.inbox.recv_timeout(self.next_tick - now) {
                Ok(message) => self.handle(message),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => panic!("the plugin host stopped"),
            }
        }
    }

    fn handle(&mut self, message: Inbound<Link>) {
        match &message {
            Inbound::PluginConnected { connection, .. } => self.plugin = Some(*connection),
            Inbound::PluginDisconnected { connection } if self.plugin == Some(*connection) => {
                self.plugin = None;
            }
            _ => {}
        }
        self.server.handle(message);
        self.server.deliver();
    }
}

#[derive(Default)]
struct Fake {
    runs: Vec<Value>,
    cards: Vec<Value>,
    agent_runs: Vec<String>,
    messages: Vec<Value>,
    token_requests: Vec<HashMap<String, String>>,
    down: bool,
}

type Shared = Arc<Mutex<Fake>>;

fn authorized(headers: &HeaderMap) -> bool {
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        == Some("Bearer tok")
}

async fn token(
    State(fake): State<Shared>,
    Form(form): Form<HashMap<String, String>>,
) -> Json<Value> {
    fake.lock().unwrap().token_requests.push(form);
    Json(json!({"access_token": "tok", "expires_in": 300}))
}

async fn agents(State(fake): State<Shared>, headers: HeaderMap) -> impl IntoResponse {
    if fake.lock().unwrap().down {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({}))).into_response();
    }
    if !authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(json!({"items": [{
        "id": "a1", "slug": "scribe", "name": "Scribe", "role": "Rédige les comptes rendus",
        "status": "idle", "hue": 200, "archived": false, "paused": false
    }]}))
    .into_response()
}

async fn active(State(fake): State<Shared>, headers: HeaderMap) -> impl IntoResponse {
    let fake = fake.lock().unwrap();
    if fake.down {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({}))).into_response();
    }
    if !authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(json!({"items": fake.runs})).into_response()
}

async fn create_card(
    State(fake): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if !authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    fake.lock().unwrap().cards.push(body);
    (StatusCode::CREATED, Json(json!({"id": "c1", "version": 4}))).into_response()
}

async fn agent_run(
    State(fake): State<Shared>,
    headers: HeaderMap,
    Path(card): Path<String>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if !authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    fake.lock()
        .unwrap()
        .agent_runs
        .push(format!("{card}:{}", body["expected_version"]));
    (StatusCode::ACCEPTED, Json(json!({"run_id": "r1"}))).into_response()
}

async fn conversation(
    State(fake): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if !authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    fake.lock().unwrap().messages.push(json!({"create": body}));
    (StatusCode::CREATED, Json(json!({"id": "conv1"}))).into_response()
}

async fn message(
    State(fake): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if !authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    fake.lock()
        .unwrap()
        .messages
        .push(json!({"send": id, "body": body}));
    let stream = "event: delta\ndata: {\"text\":\"Bonjour\\nJe suis \"}\n\n\
        event: delta\ndata: {\"text\":\"le Capitaine\"}\n\n\
        event: card\ndata: {\"card_id\":\"c9\",\"title\":\"Relire le contrat\"}\n\n\
        event: done\ndata: {\"status\":\"complete\"}\n\n";
    ([("content-type", "text/event-stream")], stream).into_response()
}

fn router(fake: Shared) -> Router {
    Router::new()
        .route("/token", post(token))
        .route("/api/v1/team/agents", get(agents))
        .route("/api/v1/runs/active", get(active))
        .route("/api/v1/cards", post(create_card))
        .route("/api/v1/cards/{card}/agent-runs", post(agent_run))
        .route("/api/v1/conversations", post(conversation))
        .route("/api/v1/conversations/{id}/messages", post(message))
        .with_state(fake)
}

fn interact(server: &mut Server<Link>, player: &Player, target: i32) {
    server.handle(Inbound::Input {
        player: player.id,
        input: PlayerInput::Interact {
            target: EntityId(target),
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::new(0.0, 0.9, 0.0),
        },
    });
    server.deliver();
}

fn send(server: &mut Server<Link>, player: &Player, input: PlayerInput) {
    server.handle(Inbound::Input {
        player: player.id,
        input,
    });
    server.deliver();
}

#[test]
fn agents_appear_as_npcs_show_their_progress_take_tasks_and_chat_with_the_captain() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let host = runtime.block_on(TcpListener::bind("127.0.0.1:0")).unwrap();
    let host_address = host.local_addr().unwrap();
    let (inbound, inbox) = crossbeam_channel::unbounded::<Inbound<Link>>();
    let (_stop, stopped) = watch::channel(false);
    runtime.spawn(rspigot_plugins::serve(host, None, inbound, stopped));

    let fake: Shared = Arc::default();
    let fake_listener = runtime.block_on(TcpListener::bind("127.0.0.1:0")).unwrap();
    let fake_address = fake_listener.local_addr().unwrap();
    let app = router(Arc::clone(&fake));
    runtime.spawn(async move { axum::serve(fake_listener, app).await });

    let mut core = Core::new(inbox);
    let _plugin = PluginProcess::start(host_address, fake_address);
    core.run_until("the plugin connects", |core| core.plugin.is_some());

    let mut alice = join(&mut core.server, "Alice");
    let (mut captain, mut scribe) = (None, None);
    core.run_until("Alice sees the captain and the agent", |_| {
        captain = alice.npc_entity("Capitaine");
        scribe = alice.npc_entity("scribe");
        captain.is_some() && scribe.is_some()
    });
    let (captain, scribe) = (captain.unwrap(), scribe.unwrap());
    core.run_until("the agent shows as available", |_| {
        alice.label_has("Disponible")
    });
    core.run_until("the captain invites to chat", |_| {
        alice.label_has("Clic droit pour discuter")
    });
    {
        let fake = fake.lock().unwrap();
        let first = &fake.token_requests[0];
        assert_eq!(first["grant_type"], "password");
        assert_eq!(first["username"], "bridge");
        assert_eq!(first["client_id"], "capitainerie-minecraft");
    }

    fake.lock().unwrap().runs = vec![json!({
        "run": {"id": "r1", "status": "running", "title": "Compte rendu", "agent": {"id": "a1", "name": "Scribe"}},
        "current_step": {"kind": "extract"},
        "steps_done": 1,
        "steps_total": 3
    })];
    core.run_until("the label shows the step and the progress", |_| {
        alice.label_has("extraction") && alice.label_has("1/3") && alice.label_has("Compte rendu")
    });

    interact(&mut core.server, &alice, scribe);
    core.run_until("Alice gets the task dialog", |_| {
        alice
            .dialog_title()
            .is_some_and(|title| title.contains("Scribe"))
    });
    assert_eq!(
        alice.dialog_buttons(),
        ["capitainerie:run/scribe", "capitainerie:cancel"]
    );
    send(
        &mut core.server,
        &alice,
        PlayerInput::CustomClick {
            id: "capitainerie:run/scribe".to_owned(),
            data: Some(r#"{"title":"Relire le contrat","description":"Page 3"}"#.to_owned()),
        },
    );
    core.run_until("Alice is told the task started", |_| {
        alice.said("Scribe prend la tâche")
    });
    {
        let fake = fake.lock().unwrap();
        assert_eq!(fake.cards[0]["title"], "Relire le contrat");
        assert_eq!(fake.cards[0]["description"], "Page 3");
        assert_eq!(fake.cards[0]["assignee_agent_id"], "a1");
        assert_eq!(fake.agent_runs, ["c1:4"]);
    }

    send(
        &mut core.server,
        &alice,
        PlayerInput::CustomClick {
            id: "capitainerie:run/scribe".to_owned(),
            data: Some(r#"{"title":"  ","description":"x"}"#.to_owned()),
        },
    );
    core.run_until("a blank title is refused", |_| {
        alice.said("Il faut un titre")
    });
    assert_eq!(fake.lock().unwrap().cards.len(), 1);

    interact(&mut core.server, &alice, captain);
    core.run_until("Alice is talking to the captain", |_| {
        alice.said("Vous parlez au Capitaine")
    });
    send(
        &mut core.server,
        &alice,
        PlayerInput::Chat("Bonjour".to_owned()),
    );
    core.run_until("the captain answers line by line", |_| {
        alice.said("Capitaine: Bonjour") && alice.said("Capitaine: Je suis le Capitaine")
    });
    core.run_until("the proposed card is announced", |_| {
        alice.said("Carte proposée : Relire le contrat")
    });
    assert!(
        !alice.broadcast_chat_seen(),
        "the message stays out of the public chat"
    );
    send(
        &mut core.server,
        &alice,
        PlayerInput::Chat("Et ensuite ?".to_owned()),
    );
    core.run_until("the second message reuses the conversation", |_| {
        fake.lock().unwrap().messages.len() >= 3
    });
    {
        let fake = fake.lock().unwrap();
        assert_eq!(fake.messages[0]["create"]["message"], "Bonjour");
        assert_eq!(fake.messages[1]["send"], "conv1");
        assert_eq!(fake.messages[1]["body"]["message"], Value::Null);
        assert_eq!(fake.messages[2]["send"], "conv1");
        assert_eq!(fake.messages[2]["body"]["message"], "Et ensuite ?");
    }
    send(
        &mut core.server,
        &alice,
        PlayerInput::Chat("quitter".to_owned()),
    );
    core.run_until("the conversation ends", |_| {
        alice.said("Conversation terminée")
    });

    fake.lock().unwrap().down = true;
    core.run_until("the captain says Capitainerie is unreachable", |_| {
        alice.label_has("injoignable")
    });
    fake.lock().unwrap().down = false;
    alice.log.clear();
    core.run_until("the captain recovers", |_| {
        alice.label_has("Clic droit pour discuter")
    });
}
