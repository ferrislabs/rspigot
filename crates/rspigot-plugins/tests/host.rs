use std::collections::BTreeMap;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rspigot_core::events::{ClientEvent, Inbound};
use rspigot_core::plugins::events::{EntityTargetPatch, PlayerMove, PluginDisable};
use rspigot_core::plugins::{
    Access, EventKind, EventPatch, EventPayload, FromPlugin, HandlerSpec, Location, Phase,
    PlayerSnapshot, PluginInfo, PluginIntro, Priority, ToPlugin, Verdict,
};
use rspigot_core::ports::{ClientLink, PluginOutbox};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::{Rotation, Vec3};
use rspigot_plugin_protocol::PROTOCOL_VERSION;
use rspigot_plugin_protocol::v1::plugin_host_client::PluginHostClient;
use rspigot_plugin_protocol::v1::{self, event, from_plugin, patch, player_order, to_plugin};
use rspigot_plugins::{PluginHostConfig, PluginHostError, run, serve};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tonic::transport::{Channel, Endpoint};
use tonic::{Code, Status, Streaming};
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
enum Seen {
    Connected {
        connection: u64,
        intro: PluginIntro,
    },
    Message {
        connection: u64,
        message: FromPlugin,
    },
    Disconnected {
        connection: u64,
    },
}

type Outboxes = Arc<Mutex<BTreeMap<u64, (String, Box<dyn PluginOutbox>)>>>;

struct Host {
    address: SocketAddr,
    seen: mpsc::UnboundedReceiver<Seen>,
    outboxes: Outboxes,
    stop: watch::Sender<bool>,
    served: JoinHandle<Result<(), PluginHostError>>,
}

impl Host {
    async fn start(token: Option<&str>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (inbound, inbox) = crossbeam_channel::unbounded::<Inbound<NoLink>>();
        let (stop, stopped) = watch::channel(false);
        let (witness, seen) = mpsc::unbounded_channel();
        let outboxes = Outboxes::default();
        let shared = Arc::clone(&outboxes);
        std::thread::spawn(move || play_the_core(inbox, shared, witness));
        let served = tokio::spawn(serve(listener, token.map(str::to_owned), inbound, stopped));
        Self {
            address,
            seen,
            outboxes,
            stop,
            served,
        }
    }

    async fn next(&mut self) -> Seen {
        within(self.seen.recv()).await.unwrap()
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
            Inbound::PluginMessage {
                connection,
                message,
            } => Seen::Message {
                connection,
                message,
            },
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
    tokio::time::timeout(Duration::from_secs(5), future)
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

async fn silent_client(
    address: SocketAddr,
) -> (
    mpsc::UnboundedSender<v1::FromPlugin>,
    impl Future<Output = Result<(), Status>>,
) {
    let (says, said) = mpsc::unbounded_channel();
    let mut client = PluginHostClient::new(channel(address).await);
    let answer = async move {
        client
            .connect(UnboundedReceiverStream::new(said))
            .await
            .map(drop)
    };
    (says, answer)
}

struct Plugin {
    says: mpsc::UnboundedSender<v1::FromPlugin>,
    hears: Streaming<v1::ToPlugin>,
}

impl Plugin {
    async fn open(address: SocketAddr, first: from_plugin::Body) -> Result<Self, Status> {
        let (says, said) = mpsc::unbounded_channel();
        says.send(v1::FromPlugin {
            body: Some(first),
            origin: None,
            request: None,
            in_world: None,
        })
        .unwrap();
        let mut client = PluginHostClient::new(channel(address).await);
        let hears = within(client.connect(UnboundedReceiverStream::new(said)))
            .await?
            .into_inner();
        Ok(Self { says, hears })
    }

    async fn accepted(address: SocketAddr, hello: v1::Hello) -> Self {
        let mut plugin = Self::open(address, from_plugin::Body::Hello(hello))
            .await
            .unwrap();
        assert_eq!(
            plugin.hear().await,
            Some(to_plugin::Body::Accepted(v1::Accepted {}))
        );
        plugin
    }

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

    async fn hear(&mut self) -> Option<to_plugin::Body> {
        within(self.hears.message()).await.unwrap().unwrap().body
    }
}

fn hello(name: &str) -> v1::Hello {
    v1::Hello {
        protocol_version: PROTOCOL_VERSION,
        name: name.to_owned(),
        version: "0.1.0".to_owned(),
        token: String::new(),
        commands: Vec::new(),
        handlers: vec![
            v1::Handler {
                id: 1,
                event: v1::EventKind::PlayerJoin as i32,
                access: v1::Access::Observe as i32,
                priority: v1::Priority::Normal as i32,
                ignore_cancelled: false,
            },
            v1::Handler {
                id: 2,
                event: v1::EventKind::PlayerMove as i32,
                access: v1::Access::Decide as i32,
                priority: v1::Priority::High as i32,
                ignore_cancelled: false,
            },
        ],
    }
}

fn steve() -> PlayerSnapshot {
    PlayerSnapshot {
        meta: Default::default(),
        permissions: Vec::new(),
        id: PlayerId(Uuid::from_u128(1)),
        name: "Steve".to_owned(),
        entity_id: 12,
        world: "world".to_owned(),
        position: Vec3 {
            x: 1.0,
            y: 64.0,
            z: -3.5,
        },
        rotation: Rotation::new(90.0, -30.0),
        game_mode: GameMode::Creative,
    }
}

#[tokio::test]
async fn a_plugin_that_introduces_itself_is_accepted() {
    let mut host = Host::start(None).await;
    let mut plugin = Plugin::accepted(host.address, hello("welcome")).await;

    let Seen::Connected { connection, intro } = host.next().await else {
        panic!("expected a connection");
    };
    assert_eq!(connection, 1);
    assert_eq!(intro.name, "welcome");
    assert_eq!(intro.version, "0.1.0");
    assert_eq!(
        intro.handlers,
        vec![
            HandlerSpec {
                id: 1,
                event: EventKind::PlayerJoin,
                access: Access::Observe,
                priority: Priority::Normal,
                ignore_cancelled: false,
            },
            HandlerSpec {
                id: 2,
                event: EventKind::PlayerMove,
                access: Access::Decide,
                priority: Priority::High,
                ignore_cancelled: false,
            },
        ]
    );

    host.tell(
        connection,
        ToPlugin::Event {
            blocks: Vec::new(),
            id: 7,
            handlers: vec![2],
            phase: Phase::Decide,
            cancelled: false,
            payload: EventPayload::PlayerMove(PlayerMove {
                player: steve(),
                from: Location {
                    world: "minecraft:overworld".to_owned(),
                    position: Vec3 {
                        x: 1.0,
                        y: 64.0,
                        z: -3.5,
                    },
                    rotation: Rotation::default(),
                },
                to: Some(Location {
                    world: "minecraft:overworld".to_owned(),
                    position: Vec3 {
                        x: 2.0,
                        y: 64.0,
                        z: -3.5,
                    },
                    rotation: Rotation::default(),
                }),
            }),
        },
    );
    let Some(to_plugin::Body::Event(event)) = plugin.hear().await else {
        panic!("expected an event");
    };
    assert_eq!(event.id, 7);
    assert_eq!(event.handlers, vec![2]);
    assert_eq!(event.phase, v1::Phase::Decide as i32);
    assert!(!event.cancelled);
    let Some(event::Payload::PlayerMove(moved)) = event.payload else {
        panic!("expected a move");
    };
    let player = moved.player.unwrap();
    assert_eq!(player.uuid, "00000000-0000-0000-0000-000000000001");
    assert_eq!(player.name, "Steve");
    assert_eq!(player.entity_id, 12);
    assert_eq!(player.game_mode, v1::GameMode::Creative as i32);
    assert_eq!(
        player.rotation,
        Some(v1::Rotation {
            yaw: 90.0,
            pitch: -30.0
        })
    );
    assert_eq!(
        moved.to.and_then(|to| to.position).map(|to| to.x),
        Some(2.0)
    );

    plugin.say(from_plugin::Body::Player(v1::PlayerOrder {
        player: "not a uuid".to_owned(),
        order: Some(player_order::Order::SendMessage("lost".to_owned())),
    }));
    plugin.say(from_plugin::Body::CustomInventory(
        v1::CustomInventoryOrder {
            inventory: 1,
            order: None,
        },
    ));
    plugin.say(from_plugin::Body::Hello(hello("welcome")));
    plugin.say(from_plugin::Body::Decision(v1::Decision {
        event: 7,
        verdicts: vec![v1::Verdict {
            handler: 1,
            cancel: true,
            patch: None,
        }],
    }));
    let Seen::Message {
        connection,
        message,
    } = host.next().await
    else {
        panic!("expected a message");
    };
    assert_eq!(connection, 1);
    assert_eq!(
        message,
        FromPlugin::Decision {
            event: 7,
            verdicts: vec![Verdict {
                handler: 1,
                cancel: true,
                patch: None
            }]
        }
    );
}

fn targeting(uuid: Option<&str>) -> Option<v1::Patch> {
    Some(v1::Patch {
        change: Some(patch::Change::EntityTarget(v1::EntityTargetPatch {
            target: Some(v1::NullableEntitySnapshot {
                value: uuid.map(|uuid| v1::EntitySnapshot {
                    entity_id: 4,
                    uuid: uuid.to_owned(),
                    kind: "minecraft:zombie".to_owned(),
                    position: Some(v1::Vec3 {
                        x: 0.0,
                        y: 64.0,
                        z: 0.0,
                    }),
                    player: None,
                    ..Default::default()
                }),
            }),
        })),
    })
}

#[tokio::test]
async fn a_malformed_patch_is_dropped_alone_and_the_plugin_is_warned() {
    let mut host = Host::start(None).await;
    let mut plugin = Plugin::accepted(host.address, hello("welcome")).await;
    let Seen::Connected { connection, .. } = host.next().await else {
        panic!("expected a connection");
    };

    plugin.say(from_plugin::Body::Decision(v1::Decision {
        event: 7,
        verdicts: vec![
            v1::Verdict {
                handler: 2,
                cancel: true,
                patch: targeting(Some("nobody")),
            },
            v1::Verdict {
                handler: 3,
                cancel: false,
                patch: targeting(None),
            },
        ],
    }));
    let Seen::Message {
        connection: from,
        message,
    } = host.next().await
    else {
        panic!("expected a message");
    };
    assert_eq!(from, connection);
    assert_eq!(
        message,
        FromPlugin::Decision {
            event: 7,
            verdicts: vec![
                Verdict {
                    handler: 2,
                    cancel: true,
                    patch: None,
                },
                Verdict {
                    handler: 3,
                    cancel: false,
                    patch: Some(EventPatch::EntityTarget(EntityTargetPatch {
                        target: Some(None)
                    })),
                },
            ],
        }
    );
    assert_eq!(
        plugin.hear().await,
        Some(to_plugin::Body::Warning(v1::Warning {
            message: "patch of handler 2 for event 7 was dropped: `nobody` is not an entity UUID"
                .to_owned(),
        }))
    );
}

#[tokio::test]
async fn a_bad_introduction_is_refused_with_a_status() {
    let mut host = Host::start(Some("secret")).await;
    let with_token = |mut hello: v1::Hello| {
        hello.token = "secret".to_owned();
        hello
    };
    let refusal = |result: Result<Plugin, Status>| match result {
        Ok(_) => panic!("the introduction was accepted"),
        Err(status) => status.code(),
    };

    let decision = from_plugin::Body::Decision(v1::Decision {
        event: 1,
        verdicts: Vec::new(),
    });
    assert_eq!(
        refusal(Plugin::open(host.address, decision).await),
        Code::InvalidArgument
    );

    let mut wrong_token = hello("welcome");
    wrong_token.token = "guess".to_owned();
    assert_eq!(
        refusal(Plugin::open(host.address, from_plugin::Body::Hello(wrong_token)).await),
        Code::Unauthenticated
    );

    for version in [3, PROTOCOL_VERSION + 1] {
        let mut other = with_token(hello("welcome"));
        other.protocol_version = version;
        assert_eq!(
            refusal(Plugin::open(host.address, from_plugin::Body::Hello(other)).await),
            Code::FailedPrecondition
        );
    }

    let mut twins = with_token(hello("welcome"));
    twins.handlers[1].id = 1;
    assert_eq!(
        refusal(Plugin::open(host.address, from_plugin::Body::Hello(twins)).await),
        Code::InvalidArgument
    );

    let mut unknown = with_token(hello("welcome"));
    unknown.handlers[0].event = 9999;
    assert_eq!(
        refusal(Plugin::open(host.address, from_plugin::Body::Hello(unknown)).await),
        Code::InvalidArgument
    );

    Plugin::accepted(host.address, with_token(hello("welcome"))).await;
    let Seen::Connected { connection, .. } = host.next().await else {
        panic!("a refused introduction reached the core");
    };
    assert_eq!(connection, 1);
}

#[tokio::test]
async fn closing_the_stream_disconnects_the_plugin() {
    let mut host = Host::start(None).await;
    let Plugin { says, mut hears } = Plugin::accepted(host.address, hello("welcome")).await;
    assert!(matches!(
        host.next().await,
        Seen::Connected { connection: 1, .. }
    ));

    drop(says);
    assert!(matches!(
        host.next().await,
        Seen::Disconnected { connection: 1 }
    ));
    assert!(matches!(within(hears.message()).await, Ok(None)));
}

#[tokio::test]
async fn health_reports_each_connected_plugin() {
    let mut host = Host::start(None).await;
    assert_eq!(host.health("").await, ServingStatus::Serving);

    let plugin = Plugin::accepted(host.address, hello("welcome")).await;
    assert!(matches!(host.next().await, Seen::Connected { .. }));
    assert_eq!(host.health("welcome").await, ServingStatus::Serving);

    drop(plugin);
    assert!(matches!(host.next().await, Seen::Disconnected { .. }));
    assert_eq!(host.health("welcome").await, ServingStatus::NotServing);
    assert_eq!(host.health("").await, ServingStatus::Serving);
}

#[tokio::test]
async fn a_replaced_plugin_stays_serving() {
    let mut host = Host::start(None).await;
    let mut first = Plugin::accepted(host.address, hello("welcome")).await;
    assert!(matches!(
        host.next().await,
        Seen::Connected { connection: 1, .. }
    ));

    let _second = Plugin::accepted(host.address, hello("welcome")).await;
    assert!(matches!(
        host.next().await,
        Seen::Connected { connection: 2, .. }
    ));

    assert_eq!(
        first.hear().await,
        Some(to_plugin::Body::Replaced(v1::Replaced {}))
    );
    let ended = within(first.hears.message()).await.unwrap_err();
    assert_eq!(ended.code(), Code::AlreadyExists);
    assert!(matches!(
        host.next().await,
        Seen::Disconnected { connection: 1 }
    ));
    assert_eq!(host.health("welcome").await, ServingStatus::Serving);
}

#[tokio::test]
async fn a_network_host_without_a_token_does_not_start() {
    let network = PluginHostConfig {
        host: "0.0.0.0".parse().unwrap(),
        port: 0,
        token: None,
    };
    assert!(matches!(
        network.validate(),
        Err(PluginHostError::TokenRequired)
    ));

    let (inbound, _inbox) = crossbeam_channel::unbounded::<Inbound<NoLink>>();
    let (_stop, stopped) = watch::channel(false);
    assert!(matches!(
        run(network.clone(), inbound, stopped).await,
        Err(PluginHostError::TokenRequired)
    ));

    let guarded = PluginHostConfig {
        token: Some("secret".to_owned()),
        ..network
    };
    assert!(guarded.validate().is_ok());
    let local = PluginHostConfig {
        host: "127.0.0.1".parse().unwrap(),
        port: 0,
        token: None,
    };
    assert!(local.validate().is_ok());
}

#[tokio::test]
async fn a_plugin_that_stops_reading_is_disconnected() {
    let mut host = Host::start(None).await;
    let mut plugin = Plugin::accepted(host.address, hello("welcome")).await;
    let Seen::Connected { connection, .. } = host.next().await else {
        panic!("expected a connection");
    };

    let warning = "w".repeat(1024);
    let mut disconnected = None;
    for _ in 0..64 {
        for _ in 0..1024 {
            host.tell(connection, ToPlugin::Warning(warning.clone()));
        }
        if let Ok(seen) = tokio::time::timeout(Duration::from_millis(50), host.seen.recv()).await {
            disconnected = seen;
            break;
        }
    }
    assert!(matches!(
        disconnected,
        Some(Seen::Disconnected { connection: 1 })
    ));

    let ended = loop {
        match within(plugin.hears.message()).await {
            Ok(Some(_)) => {}
            Ok(None) => panic!("the stream ended without a status"),
            Err(status) => break status,
        }
    };
    assert_eq!(ended.code(), Code::ResourceExhausted);
}

#[tokio::test]
async fn stopping_the_host_disconnects_its_plugins() {
    let mut host = Host::start(None).await;
    let mut plugin = Plugin::accepted(host.address, hello("welcome")).await;
    assert!(matches!(
        host.next().await,
        Seen::Connected { connection: 1, .. }
    ));
    let (_says, answer) = silent_client(host.address).await;
    let waiting = tokio::spawn(answer);
    tokio::time::sleep(Duration::from_millis(100)).await;

    host.stop.send(true).unwrap();
    assert!(matches!(
        host.next().await,
        Seen::Disconnected { connection: 1 }
    ));
    assert!(matches!(within(plugin.hears.message()).await, Ok(None)));
    within(host.served).await.unwrap().unwrap();
    assert!(within(waiting).await.unwrap().is_err());
}

#[tokio::test]
async fn what_the_core_sent_before_a_stop_reaches_the_plugin() {
    let mut host = Host::start(None).await;
    let mut plugin = Plugin::accepted(host.address, hello("welcome")).await;
    assert!(matches!(
        host.next().await,
        Seen::Connected { connection: 1, .. }
    ));
    for round in 0..50 {
        host.tell(1, ToPlugin::Warning(format!("last words {round}")));
    }
    host.stop.send(true).unwrap();
    let mut heard = 0;
    while let Ok(Some(_)) = within(plugin.hears.message()).await {
        heard += 1;
    }
    assert_eq!(heard, 50);
}

#[tokio::test]
async fn a_plugin_disable_the_core_sends_before_a_stop_reaches_the_plugin_before_its_stream_ends() {
    let mut host = Host::start(None).await;
    let mut plugin = Plugin::accepted(host.address, hello("welcome")).await;
    assert!(matches!(
        host.next().await,
        Seen::Connected { connection: 1, .. }
    ));
    host.tell(
        1,
        ToPlugin::Event {
            blocks: Vec::new(),
            id: 4,
            handlers: vec![1],
            phase: Phase::Settled,
            cancelled: false,
            payload: EventPayload::PluginDisable(PluginDisable {
                plugin: PluginInfo {
                    name: "guard".to_owned(),
                    version: "2.0.0".to_owned(),
                },
            }),
        },
    );
    host.stop.send(true).unwrap();
    let Some(to_plugin::Body::Event(heard)) = plugin.hear().await else {
        panic!("expected an event");
    };
    assert_eq!((heard.id, heard.handlers), (4, vec![1]));
    assert_eq!(
        heard.payload,
        Some(event::Payload::PluginDisable(v1::PluginDisable {
            plugin: Some(v1::PluginInfo {
                name: "guard".to_owned(),
                version: "2.0.0".to_owned(),
            }),
        }))
    );
    assert!(matches!(within(plugin.hears.message()).await, Ok(None)));
}
