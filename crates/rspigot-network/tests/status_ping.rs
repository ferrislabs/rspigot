use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use crossbeam_channel::{Receiver, Sender, TryRecvError, unbounded};
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::Inbound;
use rspigot_core::plugins::events::ServerListPingPatch;
use rspigot_core::plugins::{
    Access, EventKind, EventPatch, FromPlugin, HandlerSpec, Phase, PluginIntro, Priority, ToPlugin,
    Verdict,
};
use rspigot_core::ports::{
    AuthError, ChunkGenerator, ChunkStore, EntityStore, PluginOutbox, SessionVerifier, StoreError,
};
use rspigot_core::server::Server;
use rspigot_core::status::{PingHook, PlayerSample, StatusBoard, StatusPing, StatusReply};
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::player::{GameProfile, PlayerId};
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, ChunkPos, Identifier, Vec3};
use rspigot_nbt::Compound;
use rspigot_network::{NetLink, NetworkConfig, serve};
use rspigot_protocol::codec::Decode;
use rspigot_protocol::frame::{FrameDecoder, FrameEncoder};
use rspigot_protocol::handshake::{ClientIntent, Intention};
use rspigot_protocol::ids;
use rspigot_protocol::status::{StatusRequest, StatusResponse};
use rspigot_protocol::{split_packet_id, write_packet_with_id};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

struct Offline;

impl SessionVerifier for Offline {
    fn verify(&self, username: &str, _: &str, _: Option<IpAddr>) -> Result<GameProfile, AuthError> {
        Err(AuthError::NotAuthenticated(username.to_owned()))
    }
}

struct Network {
    address: SocketAddr,
    stop: watch::Sender<bool>,
}

impl Drop for Network {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

async fn network(status: StatusBoard, game: Sender<Inbound<NetLink>>) -> Network {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a free port");
    let address = listener.local_addr().expect("bound");
    let config = NetworkConfig {
        bind: address,
        online_mode: false,
        connection_throttle: Duration::ZERO,
        ..NetworkConfig::default()
    };
    let (stop, stopped) = watch::channel(false);
    tokio::spawn(serve(
        listener,
        config,
        Arc::new(Offline),
        status,
        game,
        stopped,
    ));
    Network { address, stop }
}

async fn status_json(address: SocketAddr) -> Value {
    let mut stream = TcpStream::connect(address).await.expect("connected");
    let mut encoder = FrameEncoder::new();
    let mut frames = Vec::new();
    let mut body = Vec::new();
    let intention = Intention {
        protocol_version: rspigot_data::PROTOCOL_VERSION,
        host: "play.example".to_owned(),
        port: 25565,
        intent: ClientIntent::Status,
    };
    write_packet_with_id(
        ids::handshake::serverbound::INTENTION,
        &intention,
        &mut body,
    );
    encoder.encode_packet(&body, &mut frames).expect("framed");
    body.clear();
    write_packet_with_id(
        ids::status::serverbound::STATUS_REQUEST,
        &StatusRequest,
        &mut body,
    );
    encoder.encode_packet(&body, &mut frames).expect("framed");
    stream.write_all(&frames).await.expect("sent");
    let mut decoder = FrameDecoder::new();
    let mut buffer = [0u8; 4096];
    let packet = loop {
        if let Some(packet) = decoder.next_packet().expect("a frame") {
            break packet;
        }
        let read = within(stream.read(&mut buffer)).await.expect("read");
        assert!(read > 0, "the server closed the connection");
        decoder.feed(&buffer[..read]);
    };
    let (id, mut body) = split_packet_id(&packet).expect("an id");
    assert_eq!(id, ids::status::clientbound::STATUS_RESPONSE);
    let response = StatusResponse::decode(&mut body).expect("a status response");
    serde_json::from_str(&response.json).expect("json")
}

async fn legacy_fields(address: SocketAddr) -> Vec<String> {
    let mut stream = TcpStream::connect(address).await.expect("connected");
    stream.write_all(&[0xFE, 0x01]).await.expect("sent");
    let mut bytes = Vec::new();
    within(stream.read_to_end(&mut bytes)).await.expect("read");
    assert_eq!(bytes[0], 0xFF);
    let units: Vec<u16> = bytes[3..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_be_bytes(*pair))
        .collect();
    String::from_utf16(&units)
        .expect("utf-16")
        .split('\0')
        .map(str::to_owned)
        .collect()
}

async fn within<F: std::future::Future>(future: F) -> F::Output {
    tokio::time::timeout(Duration::from_secs(10), future)
        .await
        .expect("timed out")
}

fn board(hook: PingHook) -> StatusBoard {
    let status = StatusBoard::new(20);
    status.update(|snapshot| snapshot.ping = hook);
    status
}

fn icon() -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend_from_slice(&64u32.to_be_bytes());
    png.extend_from_slice(&64u32.to_be_bytes());
    png.extend_from_slice(&[8, 6, 0, 0, 0]);
    png
}

fn answer_one_ping(
    inbox: Receiver<Inbound<NetLink>>,
    reply: StatusReply,
) -> std::thread::JoinHandle<StatusPing> {
    std::thread::spawn(move || {
        loop {
            match inbox.recv_timeout(Duration::from_secs(10)) {
                Ok(Inbound::StatusPing {
                    ping,
                    reply: Some(answer),
                }) => {
                    answer.send(reply).expect("the network waits");
                    return ping;
                }
                Ok(_) => {}
                Err(error) => panic!("no ping reached the game: {error}"),
            }
        }
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_ping_nobody_listens_to_is_answered_from_the_board_without_waking_the_game() {
    let (game, inbox) = unbounded();
    let net = network(board(PingHook::Off), game).await;
    let json = status_json(net.address).await;
    assert_eq!(json["description"]["text"], "A Minecraft Server");
    assert_eq!(json["players"]["max"], 20);
    assert_eq!(json["players"]["online"], 0);
    assert!(json.get("favicon").is_none());
    assert!(matches!(inbox.try_recv(), Err(TryRecvError::Empty)));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_observed_ping_is_answered_at_once_and_announced_to_the_game() {
    let (game, inbox) = unbounded();
    let net = network(board(PingHook::Notify), game).await;
    let json = status_json(net.address).await;
    assert_eq!(json["description"]["text"], "A Minecraft Server");
    match inbox.try_recv() {
        Ok(Inbound::StatusPing { ping, reply: None }) => assert_eq!(
            ping,
            StatusPing {
                address: IpAddr::from([127, 0, 0, 1]),
                hostname: "play.example:25565".to_owned(),
                motd: "A Minecraft Server".to_owned(),
                legacy: false,
            }
        ),
        _ => panic!("expected an announced ping"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_decided_ping_shows_what_the_game_answered() {
    let (game, inbox) = unbounded();
    let bob = PlayerId::offline("Bob");
    let game_side = answer_one_ping(
        inbox,
        StatusReply {
            motd: "§bPatched".to_owned(),
            max_players: 5,
            online: 1,
            sample: vec![PlayerSample {
                id: bob,
                name: "Bob".to_owned(),
            }],
            icon: Some(icon()),
        },
    );
    let net = network(board(PingHook::Ask), game).await;
    let json = status_json(net.address).await;
    assert_eq!(json["description"]["text"], "§bPatched");
    assert_eq!(json["players"]["max"], 5);
    assert_eq!(json["players"]["online"], 1);
    assert_eq!(json["players"]["sample"][0]["name"], "Bob");
    assert_eq!(
        json["players"]["sample"][0]["id"],
        bob.0.hyphenated().to_string()
    );
    assert_eq!(
        json["favicon"],
        format!("data:image/png;base64,{}", STANDARD.encode(icon()))
    );
    let ping = game_side.join().expect("the game answered");
    assert_eq!(ping.hostname, "play.example:25565");
    assert!(!ping.legacy);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_legacy_ping_goes_through_the_same_hook_without_a_hostname() {
    let (game, inbox) = unbounded();
    let game_side = answer_one_ping(
        inbox,
        StatusReply {
            motd: "Old".to_owned(),
            max_players: 7,
            online: 3,
            sample: Vec::new(),
            icon: None,
        },
    );
    let net = network(board(PingHook::Ask), game).await;
    let fields = legacy_fields(net.address).await;
    assert_eq!(&fields[3..], ["Old", "3", "7"]);
    let ping = game_side.join().expect("the game answered");
    assert_eq!((ping.hostname.as_str(), ping.legacy), ("", true));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_legacy_ping_whose_request_arrives_late_still_reads_the_whole_answer() {
    let (game, _inbox) = unbounded();
    let net = network(board(PingHook::Off), game).await;
    let mut stream = TcpStream::connect(net.address).await.expect("connected");
    stream.write_all(&[0xFE, 0x01, 0xFA]).await.expect("sent");
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut hostname = vec![0x00, 0x0B];
    hostname.extend("MC|PingHost".encode_utf16().flat_map(u16::to_be_bytes));
    hostname.extend(std::iter::repeat_n(0x61, 4096));
    let _ = stream.write_all(&hostname).await;
    let mut bytes = Vec::new();
    within(stream.read_to_end(&mut bytes))
        .await
        .expect("the answer arrives without a reset");
    assert_eq!(bytes[0], 0xFF);
    assert!(bytes.len() > 20);
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

impl ChunkGenerator for NoStore {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        ChunkColumn::empty(pos, dimension.height, BiomeId(0))
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

#[derive(Clone, Default)]
struct Mailbox(Arc<Mutex<Vec<ToPlugin>>>);

impl PluginOutbox for Mailbox {
    fn send(&self, message: ToPlugin) {
        if let Ok(mut inbox) = self.0.lock() {
            inbox.push(message);
        }
    }
}

impl Mailbox {
    fn asked_ping(&self) -> Option<u64> {
        self.0
            .lock()
            .ok()?
            .iter()
            .find_map(|message| match message {
                ToPlugin::Event {
                    id,
                    phase: Phase::Decide,
                    payload,
                    ..
                } if payload.kind() == EventKind::ServerListPing => Some(*id),
                _ => None,
            })
    }
}

fn deciding_core(status: &StatusBoard) -> (Server<NetLink>, Mailbox) {
    let mut server: Server<NetLink> = Server::new(
        GameConfig {
            chunk_workers: 1,
            spawn_chunk_radius: 0,
            ..GameConfig::default()
        },
        LevelData::default(),
        Arc::new(NoStore),
        Arc::new(NoStore),
        status.clone(),
    );
    let plugin = Mailbox::default();
    server.handle(Inbound::PluginConnected {
        connection: 1,
        intro: PluginIntro {
            commands: Vec::new(),
            name: "motd".to_owned(),
            version: "1.0.0".to_owned(),
            handlers: vec![HandlerSpec {
                id: 1,
                event: EventKind::ServerListPing,
                access: Access::Decide,
                priority: Priority::Normal,
                ignore_cancelled: false,
            }],
        },
        outbox: Box::new(plugin.clone()),
    });
    assert_eq!(status.snapshot().ping, PingHook::Ask);
    (server, plugin)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_ping_in_flight_during_shutdown_answers_from_the_board_without_delay() {
    let status = StatusBoard::new(20);
    let (game, inbox) = unbounded::<Inbound<NetLink>>();
    let (server, _plugin) = deciding_core(&status);
    game.send(Inbound::Shutdown).expect("the core listens");
    let net = network(status, game).await;
    let asking = tokio::spawn(status_json(net.address));
    let deadline = Instant::now() + Duration::from_secs(10);
    while inbox.len() < 2 {
        assert!(Instant::now() < deadline, "the ping never reached the game");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let stopping = Instant::now();
    let core = std::thread::spawn(move || {
        server.run(&inbox);
        inbox
    });
    let json = within(asking).await.expect("the client got an answer");
    assert!(stopping.elapsed() < Duration::from_secs(2));
    assert_eq!(json["description"]["text"], "A Minecraft Server");
    let inbox = core.join().expect("the core stops");
    assert!(inbox.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_status_ping_over_the_network_waits_for_a_deciding_plugin_and_shows_its_patch() {
    let status = StatusBoard::new(20);
    let (game, inbox) = unbounded::<Inbound<NetLink>>();
    let (server, plugin) = deciding_core(&status);
    let core = std::thread::spawn(move || server.run(&inbox));
    let decider = std::thread::spawn({
        let game = game.clone();
        move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let event = loop {
                if let Some(event) = plugin.asked_ping() {
                    break event;
                }
                assert!(Instant::now() < deadline, "the plugin was never asked");
                std::thread::sleep(Duration::from_millis(5));
            };
            let patch = ServerListPingPatch {
                motd: Some("Patched by a plugin".to_owned()),
                max_players: Some(3),
                ..ServerListPingPatch::default()
            };
            game.send(Inbound::PluginMessage {
                connection: 1,
                message: FromPlugin::Decision {
                    event,
                    verdicts: vec![Verdict {
                        handler: 1,
                        cancel: false,
                        patch: Some(EventPatch::ServerListPing(patch)),
                    }],
                },
            })
            .expect("the core listens");
        }
    });
    let net = network(status, game.clone()).await;
    let json = status_json(net.address).await;
    decider.join().expect("the plugin decided");
    assert_eq!(json["description"]["text"], "Patched by a plugin");
    assert_eq!(json["players"]["max"], 3);
    assert_eq!(json["players"]["online"], 0);
    game.send(Inbound::Shutdown).expect("the core listens");
    core.join().expect("the core stops");
}
