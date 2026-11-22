use std::collections::BTreeMap;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rspigot_core::events::{ClientEvent, Inbound};
use rspigot_core::plugins::events::{PlayerJoin, PlayerMove};
use rspigot_core::plugins::{
    EventPayload, FromPlugin, Location, Order, Phase, PlayerOrder, PlayerSnapshot, PluginIntro,
    ToPlugin, Verdict,
};
use rspigot_core::ports::{ClientLink, PluginOutbox};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::{Rotation, Vec3};
use rspigot_plugin::{Error as SdkError, Plugin, listeners, run_with};
use rspigot_plugin_protocol::PROTOCOL_VERSION;
use rspigot_plugin_protocol::v1::plugin_host_client::PluginHostClient;
use rspigot_plugin_protocol::v1::{self, from_plugin, player_order};
use rspigot_plugins::{
    CONNECT_LIMIT, ExpectedPlugin, HostOptions, MAX_HANDLERS, MESSAGE_BURST, MESSAGES_PER_SECOND,
    PluginHostError, PluginState, Roster, serve_supervised,
};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tonic::Code;
use tonic::transport::{Channel, Endpoint};
use tonic_health::pb::HealthCheckRequest;
use tonic_health::pb::health_check_response::ServingStatus;
use tonic_health::pb::health_client::HealthClient;
use uuid::Uuid;

struct NoLink;

impl ClientLink for NoLink {
    fn send(&self, _event: ClientEvent) {}

    fn disconnect(&self, _reason: Component) {}

    fn is_connected(&self) -> bool {
        false
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum Seen {
    Connected { connection: u64, intro: PluginIntro },
    Message { message: FromPlugin },
    Disconnected { connection: u64 },
}

type Outboxes = Arc<Mutex<BTreeMap<u64, (String, Box<dyn PluginOutbox>)>>>;

struct Core {
    address: SocketAddr,
    seen: mpsc::UnboundedReceiver<Seen>,
    outboxes: Outboxes,
    roster: Roster,
    stop: watch::Sender<bool>,
    _served: JoinHandle<Result<(), PluginHostError>>,
}

impl Core {
    async fn start(token: Option<&str>, expected: &[&str], soft_wait: Duration) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (inbound, inbox) = crossbeam_channel::unbounded::<Inbound<NoLink>>();
        let (stop, stopped) = watch::channel(false);
        let (witness, seen) = mpsc::unbounded_channel();
        let outboxes = Outboxes::default();
        let shared = Arc::clone(&outboxes);
        std::thread::spawn(move || play_the_core(inbox, shared, witness));
        let roster = Roster::new(expected.iter().map(|name| (*name).to_owned()));
        let mut options = HostOptions::new(token.map(str::to_owned));
        options.soft_wait = soft_wait;
        options.expected = expected
            .iter()
            .map(|name| ExpectedPlugin {
                name: (*name).to_owned(),
                launch: None,
            })
            .collect();
        let served = tokio::spawn(serve_supervised(
            listener,
            options,
            roster.clone(),
            inbound,
            stopped,
        ));
        Self {
            address,
            seen,
            outboxes,
            roster,
            stop,
            _served: served,
        }
    }

    fn url(&self) -> String {
        format!("http://{}", self.address)
    }

    async fn next(&mut self) -> Seen {
        within(self.seen.recv()).await.unwrap()
    }

    async fn connected(&mut self) -> (u64, PluginIntro) {
        match self.next().await {
            Seen::Connected { connection, intro } => (connection, intro),
            other => panic!("expected a connection, got {other:?}"),
        }
    }

    async fn message(&mut self) -> FromPlugin {
        match self.next().await {
            Seen::Message { message } => message,
            other => panic!("expected a message, got {other:?}"),
        }
    }

    fn tell(&self, connection: u64, message: ToPlugin) {
        if let Some((_, outbox)) = self.outboxes.lock().unwrap().get(&connection) {
            outbox.send(message);
        }
    }

    async fn health(&self, service: &str) -> ServingStatus {
        let mut client = HealthClient::new(channel(self.address).await);
        let request = HealthCheckRequest {
            service: service.to_owned(),
        };
        let response = within(client.check(request)).await.unwrap().into_inner();
        ServingStatus::try_from(response.status).unwrap()
    }

    async fn quiet(&mut self, wait: Duration) {
        assert!(
            tokio::time::timeout(wait, self.seen.recv()).await.is_err(),
            "the core heard something"
        );
    }
}

fn play_the_core(
    inbox: crossbeam_channel::Receiver<Inbound<NoLink>>,
    outboxes: Outboxes,
    witness: mpsc::UnboundedSender<Seen>,
) {
    for message in inbox {
        let seen = match message {
            Inbound::PluginConnected {
                connection,
                intro,
                outbox,
            } => {
                let mut outboxes = outboxes.lock().unwrap();
                let taken = outboxes
                    .iter()
                    .find(|(_, (name, _))| *name == intro.name)
                    .map(|(id, _)| *id);
                if let Some((_, old)) = taken.and_then(|id| outboxes.remove(&id)) {
                    old.send(ToPlugin::Replaced);
                }
                outbox.send(ToPlugin::Accepted);
                outboxes.insert(connection, (intro.name.clone(), outbox));
                Seen::Connected { connection, intro }
            }
            Inbound::PluginMessage { message, .. } => Seen::Message { message: *message },
            Inbound::PluginDisconnected { connection } => {
                outboxes.lock().unwrap().remove(&connection);
                Seen::Disconnected { connection }
            }
            _ => continue,
        };
        let _ = witness.send(seen);
    }
}

async fn within<F: Future>(future: F) -> F::Output {
    tokio::time::timeout(Duration::from_secs(10), future)
        .await
        .expect("timed out")
}

async fn channel(address: SocketAddr) -> Channel {
    Endpoint::from_shared(format!("http://{address}"))
        .unwrap()
        .connect()
        .await
        .unwrap()
}

fn steve() -> PlayerSnapshot {
    PlayerSnapshot {
        meta: Default::default(),
        permissions: Vec::new(),
        id: PlayerId(Uuid::from_u128(1)),
        name: "Steve".to_owned(),
        entity_id: 12,
        world: "world".to_owned(),
        position: Vec3::new(1.0, 64.0, -3.5),
        rotation: Rotation::new(90.0, -30.0),
        game_mode: GameMode::Creative,
    }
}

fn joining(id: u64, handler: u32) -> ToPlugin {
    ToPlugin::Event {
        blocks: Vec::new(),
        id,
        handlers: vec![handler],
        phase: Phase::Settled,
        cancelled: false,
        payload: EventPayload::PlayerJoin(PlayerJoin {
            join_message: None,
            player: steve(),
        }),
    }
}

fn at(x: f64) -> Location {
    Location {
        world: "minecraft:overworld".to_owned(),
        position: Vec3::new(x, 64.0, -3.5),
        rotation: Rotation::default(),
    }
}

fn moving(id: u64, handler: u32) -> ToPlugin {
    ToPlugin::Event {
        blocks: Vec::new(),
        id,
        handlers: vec![handler],
        phase: Phase::Decide,
        cancelled: false,
        payload: EventPayload::PlayerMove(PlayerMove {
            player: steve(),
            from: at(1.0),
            to: Some(at(2.0)),
        }),
    }
}

mod echo {
    use rspigot_plugin::{PlayerJoin, PlayerMove, Plugin, listeners};

    #[derive(Plugin)]
    #[plugin(name = "echo", version = "1.0.0")]
    pub struct Echo;

    #[listeners]
    impl Echo {
        async fn on_player_join(&self, event: &PlayerJoin) {
            let player = event.player();
            player.message.send(format!("Welcome, {}!", player.name));
        }

        #[priority(High)]
        async fn on_player_move(&self, event: &mut PlayerMove) {
            event.cancel();
        }
    }
}

use echo::Echo;

fn spawn_plugin<P: Plugin>(
    plugin: P,
    url: String,
    token: Option<&str>,
) -> JoinHandle<Result<(), SdkError>> {
    tokio::spawn(run_with(plugin, url, token.map(str::to_owned)))
}

#[tokio::test]
async fn the_real_sdk_round_trips_an_event_an_order_and_a_decision() {
    let mut core = Core::start(None, &[], Duration::from_secs(1)).await;
    let plugin = spawn_plugin(Echo, core.url(), None);

    let (connection, intro) = core.connected().await;
    assert_eq!(
        (intro.name.as_str(), intro.version.as_str()),
        ("echo", "1.0.0")
    );
    assert_eq!(intro.handlers.len(), 2);

    core.tell(connection, joining(5, 1));
    assert_eq!(
        core.message().await,
        FromPlugin::Reaction {
            event: 5,
            order: Order::Player {
                player: PlayerId(Uuid::from_u128(1)),
                order: PlayerOrder::SendMessage("Welcome, Steve!".to_owned()),
            },
        }
    );

    core.tell(connection, moving(7, 2));
    assert_eq!(
        core.message().await,
        FromPlugin::Decision {
            event: 7,
            verdicts: vec![Verdict {
                handler: 2,
                cancel: true,
                patch: None,
            }],
        }
    );
    assert_eq!(core.health("echo").await, ServingStatus::Serving);
    plugin.abort();
}

#[tokio::test]
async fn the_real_sdk_is_refused_with_a_wrong_token_and_does_not_retry() {
    let mut core = Core::start(Some("right"), &[], Duration::from_secs(1)).await;
    let ended = within(spawn_plugin(Echo, core.url(), Some("wrong")))
        .await
        .unwrap();
    assert!(
        matches!(&ended, Err(SdkError::Refused(status)) if status.code() == Code::Unauthenticated),
        "{ended:?}"
    );
    core.quiet(Duration::from_millis(200)).await;

    let accepted = spawn_plugin(Echo, core.url(), Some("right"));
    core.connected().await;
    accepted.abort();
}

#[tokio::test]
async fn a_second_session_under_the_same_name_replaces_the_first_cleanly() {
    let mut core = Core::start(None, &[], Duration::from_secs(1)).await;
    let first = spawn_plugin(Echo, core.url(), None);
    let (first_connection, _) = core.connected().await;

    let second = spawn_plugin(Echo, core.url(), None);
    let (second_connection, _) = core.connected().await;
    assert_ne!(first_connection, second_connection);

    let ended = within(first).await.unwrap();
    assert!(matches!(ended, Err(SdkError::Replaced)), "{ended:?}");
    assert!(matches!(
        core.next().await,
        Seen::Disconnected { connection } if connection == first_connection
    ));
    assert_eq!(core.health("echo").await, ServingStatus::Serving);
    assert_eq!(core.roster.state("echo"), Some(PluginState::Connected));
    core.tell(second_connection, joining(9, 1));
    assert!(matches!(
        core.message().await,
        FromPlugin::Reaction { event: 9, .. }
    ));
    second.abort();
}

#[tokio::test]
async fn a_plugin_that_crashes_is_reported_down_then_restored_with_its_handlers_on_reconnect() {
    let mut core = Core::start(None, &["echo"], Duration::from_secs(1)).await;
    assert_eq!(core.roster.state("echo"), Some(PluginState::Expected));
    assert_eq!(core.health("echo").await, ServingStatus::NotServing);

    let first = spawn_plugin(Echo, core.url(), None);
    let (connection, intro) = core.connected().await;
    assert_eq!(core.roster.state("echo"), Some(PluginState::Connected));
    assert_eq!(core.health("echo").await, ServingStatus::Serving);

    first.abort();
    assert!(matches!(
        core.next().await,
        Seen::Disconnected { connection: gone } if gone == connection
    ));
    assert_eq!(core.roster.state("echo"), Some(PluginState::Down));
    assert_eq!(core.health("echo").await, ServingStatus::NotServing);

    let again = spawn_plugin(Echo, core.url(), None);
    let (reconnected, restored) = core.connected().await;
    assert_ne!(reconnected, connection);
    assert_eq!(restored.handlers, intro.handlers);
    assert_eq!(core.roster.state("echo"), Some(PluginState::Connected));
    core.tell(reconnected, joining(11, 1));
    assert!(matches!(
        core.message().await,
        FromPlugin::Reaction { event: 11, .. }
    ));
    again.abort();
}

struct Raw {
    says: mpsc::UnboundedSender<v1::FromPlugin>,
    #[allow(dead_code)]
    hears: tonic::Streaming<v1::ToPlugin>,
}

fn raw_hello(name: &str, handlers: usize) -> v1::Hello {
    v1::Hello {
        protocol_version: PROTOCOL_VERSION,
        name: name.to_owned(),
        version: "0.1.0".to_owned(),
        token: String::new(),
        commands: Vec::new(),
        depends: Vec::new(),
        soft_depends: Vec::new(),
        load_before: Vec::new(),
        handlers: (1..=handlers as u32)
            .map(|id| v1::Handler {
                id,
                event: v1::EventKind::PlayerJoin as i32,
                access: v1::Access::Observe as i32,
                priority: v1::Priority::Normal as i32,
                ignore_cancelled: false,
            })
            .collect(),
    }
}

impl Raw {
    fn say(&self, body: from_plugin::Body) {
        self.says
            .send(v1::FromPlugin {
                body: Some(body),
                origin: None,
                request: None,
                in_world: None,
            })
            .unwrap();
    }
}

async fn knock(address: SocketAddr, hello: v1::Hello) -> Result<Raw, tonic::Status> {
    let (says, said) = mpsc::unbounded_channel();
    says.send(v1::FromPlugin {
        body: Some(from_plugin::Body::Hello(hello)),
        origin: None,
        request: None,
        in_world: None,
    })
    .unwrap();
    let response = within(
        PluginHostClient::new(channel(address).await).connect(UnboundedReceiverStream::new(said)),
    )
    .await?;
    Ok(Raw {
        says,
        hears: response.into_inner(),
    })
}

#[tokio::test]
async fn a_hello_with_too_many_handlers_is_refused() {
    let core = Core::start(None, &[], Duration::from_secs(1)).await;
    let refused = knock(core.address, raw_hello("greedy", MAX_HANDLERS + 1))
        .await
        .err()
        .expect("refused");
    assert_eq!(refused.code(), Code::InvalidArgument);
    assert!(
        refused.message().contains("handlers"),
        "{}",
        refused.message()
    );
    assert!(
        knock(core.address, raw_hello("fair", MAX_HANDLERS))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn a_plugin_listing_itself_or_too_many_relations_is_refused() {
    let core = Core::start(None, &[], Duration::from_secs(1)).await;
    let mut selfish = raw_hello("selfish", 1);
    selfish.depends = vec!["selfish".to_owned()];
    let refused = knock(core.address, selfish).await.err().expect("refused");
    assert_eq!(refused.code(), Code::InvalidArgument);

    let mut crowded = raw_hello("crowded", 1);
    crowded.soft_depends = (0..=rspigot_plugins::MAX_DEPENDENCIES)
        .map(|index| format!("p{index}"))
        .collect();
    let refused = knock(core.address, crowded).await.err().expect("refused");
    assert_eq!(refused.code(), Code::InvalidArgument);
}

#[tokio::test]
async fn a_crash_loop_is_rate_limited_per_name() {
    let mut core = Core::start(None, &[], Duration::from_secs(1)).await;
    let mut kept = Vec::new();
    for _ in 0..CONNECT_LIMIT {
        kept.push(knock(core.address, raw_hello("flappy", 1)).await.unwrap());
        loop {
            if let Seen::Connected { .. } = core.next().await {
                break;
            }
        }
    }
    let refused = knock(core.address, raw_hello("flappy", 1))
        .await
        .err()
        .expect("refused");
    assert_eq!(refused.code(), Code::ResourceExhausted);
    assert!(
        refused.message().contains("too fast"),
        "{}",
        refused.message()
    );
    assert!(knock(core.address, raw_hello("calm", 1)).await.is_ok());
}

#[derive(Plugin)]
#[plugin(name = "shop", version = "1.0.0", depends = ["base"])]
struct Shop;

#[listeners]
impl Shop {}

#[derive(Plugin)]
#[plugin(name = "base", version = "1.0.0")]
struct Base;

#[listeners]
impl Base {}

#[tokio::test]
async fn a_plugin_whose_hard_dependency_is_missing_is_held_until_it_connects() {
    let mut core = Core::start(None, &[], Duration::from_secs(1)).await;
    let shop = spawn_plugin(Shop, core.url(), None);
    core.quiet(Duration::from_millis(500)).await;
    assert_eq!(
        core.roster.state("shop"),
        Some(PluginState::Waiting(vec!["base".to_owned()]))
    );

    let base = spawn_plugin(Base, core.url(), None);
    let (_, first) = core.connected().await;
    let (_, second) = core.connected().await;
    assert_eq!(
        (first.name.as_str(), second.name.as_str()),
        ("base", "shop")
    );
    assert_eq!(core.roster.state("shop"), Some(PluginState::Connected));
    shop.abort();
    base.abort();
}

#[tokio::test]
async fn a_held_plugin_that_leaves_is_forgotten_and_never_announced() {
    let mut core = Core::start(None, &[], Duration::from_secs(1)).await;
    let shop = spawn_plugin(Shop, core.url(), None);
    core.quiet(Duration::from_millis(300)).await;
    shop.abort();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let base = spawn_plugin(Base, core.url(), None);
    let (_, only) = core.connected().await;
    assert_eq!(only.name, "base");
    core.quiet(Duration::from_millis(500)).await;
    base.abort();
}

#[derive(Plugin)]
#[plugin(name = "late", version = "1.0.0", soft_depends = ["economy"])]
struct Late;

#[listeners]
impl Late {}

#[tokio::test]
async fn a_soft_dependency_that_never_shows_up_only_delays_the_plugin() {
    let mut core = Core::start(None, &["economy"], Duration::from_millis(400)).await;
    let started = Instant::now();
    let late = spawn_plugin(Late, core.url(), None);
    let (_, intro) = core.connected().await;
    assert_eq!(intro.name, "late");
    assert!(started.elapsed() >= Duration::from_millis(400));
    late.abort();
}

#[derive(Plugin)]
#[plugin(name = "early", version = "1.0.0", load_before = ["tail"])]
struct Early;

#[listeners]
impl Early {}

#[derive(Plugin)]
#[plugin(name = "tail", version = "1.0.0")]
struct Tail;

#[listeners]
impl Tail {}

#[tokio::test]
async fn load_before_orders_the_announcements_of_expected_plugins() {
    let mut core = Core::start(None, &["early", "tail"], Duration::from_secs(5)).await;
    let early = spawn_plugin(Early, core.url(), None);
    let (_, intro) = core.connected().await;
    assert_eq!(intro.name, "early");
    early.abort();
    assert!(matches!(core.next().await, Seen::Disconnected { .. }));

    let tail = spawn_plugin(Tail, core.url(), None);
    core.quiet(Duration::from_millis(300)).await;
    assert_eq!(
        core.roster.state("tail"),
        Some(PluginState::Waiting(vec!["early".to_owned()]))
    );
    let early = spawn_plugin(Early, core.url(), None);
    let (_, first) = core.connected().await;
    let (_, second) = core.connected().await;
    assert_eq!(
        (first.name.as_str(), second.name.as_str()),
        ("early", "tail")
    );
    tail.abort();
    early.abort();
}

#[tokio::test]
async fn a_flood_of_orders_is_slowed_down_to_the_rate_limit_and_none_is_lost() {
    let mut core = Core::start(None, &[], Duration::from_secs(1)).await;
    let raw = knock(core.address, raw_hello("flood", 1)).await.unwrap();
    core.connected().await;
    let total = MESSAGE_BURST as usize + MESSAGES_PER_SECOND as usize;
    let started = Instant::now();
    for _ in 0..total {
        raw.say(from_plugin::Body::Player(v1::PlayerOrder {
            player: Uuid::from_u128(1).to_string(),
            order: Some(player_order::Order::SendMessage("x".to_owned())),
        }));
    }
    for _ in 0..total {
        assert!(matches!(core.message().await, FromPlugin::Order(_)));
    }
    assert!(
        started.elapsed() >= Duration::from_millis(800),
        "{:?}",
        started.elapsed()
    );
    drop(raw);
}

#[tokio::test]
async fn stopping_the_host_ends_a_held_plugin_too() {
    let core = Core::start(None, &[], Duration::from_secs(1)).await;
    let shop = spawn_plugin(Shop, core.url(), None);
    tokio::time::sleep(Duration::from_millis(300)).await;
    core.stop.send(true).unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(core.roster.state("shop"), Some(PluginState::Down));
    shop.abort();
}
