use std::sync::Arc;
use std::time::Duration;

use rspigot_core::ports::{AuthError, SessionVerifier};
use rspigot_domain::player::{GameProfile, PlayerId};
use rspigot_network::verify_velocity;
use rspigot_protocol::codec::{Encode, VarInt, decode_exact};
use rspigot_protocol::common::Identifier;
use rspigot_protocol::frame::{Compression, FrameDecoder, FrameEncoder};
use rspigot_protocol::handshake::{ClientIntent, Intention};
use rspigot_protocol::ids;
use rspigot_protocol::login::{
    CustomQuery, CustomQueryAnswer, LoginAcknowledged, LoginCompression, LoginDisconnect,
    LoginFinished, ServerboundHello,
};
use rspigot_protocol::play::commands::{
    CommandNode, CommandNodeKind, CommandSuggestions, Commands,
};
use rspigot_protocol::play::player::{PlayerInfoActions, PlayerInfoEntry, PlayerInfoUpdate};
use rspigot_protocol::play::types::GameMode;
use rspigot_protocol::status::{PingRequest, PongResponse, StatusRequest, StatusResponse};
use rspigot_protocol::types::{GameProfile as WireProfile, JsonText};
use rspigot_protocol::{Packet, split_packet_id, write_packet, write_packet_with_id};
use std::future::Future;
use std::sync::Mutex;

use rspigot_proxy_core::{
    ArgumentKind, CommandArgument, CommandSpec, ConnectCause, ConnectOutcome, Invocation,
    Permissions, PlayerView, ProxyHooks, ServerRegistry, Verdict,
};
use rspigot_proxy_network::{ProxySettings, Shared, serve};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use uuid::Uuid;

const SECRET: &[u8] = b"forwarding-secret";
const BIG: usize = 4096;

struct Offline;

impl SessionVerifier for Offline {
    fn verify(
        &self,
        name: &str,
        _: &str,
        _: Option<std::net::IpAddr>,
    ) -> Result<GameProfile, AuthError> {
        Err(AuthError::NotAuthenticated(name.to_owned()))
    }
}

struct Peer {
    stream: TcpStream,
    encoder: FrameEncoder,
    decoder: FrameDecoder,
}

impl Peer {
    fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            encoder: FrameEncoder::new(),
            decoder: FrameDecoder::new(),
        }
    }

    async fn send<P: Packet>(&mut self, packet: &P) {
        let mut body = Vec::new();
        write_packet(packet, &mut body);
        self.send_raw(&body).await;
    }

    async fn send_raw(&mut self, packet: &[u8]) {
        let mut out = Vec::new();
        self.encoder.encode_packet(packet, &mut out).unwrap();
        self.stream.write_all(&out).await.unwrap();
    }

    async fn next(&mut self) -> Vec<u8> {
        let mut chunk = [0u8; 8192];
        loop {
            if let Some(packet) = self.decoder.next_packet().unwrap() {
                return packet;
            }
            let read = tokio::time::timeout(Duration::from_secs(5), self.stream.read(&mut chunk))
                .await
                .expect("timed out")
                .unwrap();
            assert!(read > 0, "connection closed");
            self.decoder.feed(&chunk[..read]);
        }
    }

    fn compress(&mut self, threshold: usize) {
        self.encoder.set_compression(Some(Compression {
            threshold,
            level: 6,
        }));
        self.decoder.set_compression(Some(threshold));
    }
}

fn settings(secret: &[u8]) -> ProxySettings {
    ProxySettings {
        online_mode: false,
        motd: "Hypolia".to_owned(),
        max_players: 50,
        favicon: None,
        compression_threshold: Some(256),
        compression_level: 6,
        forwarding_secret: secret.to_vec(),
        login_timeout: Duration::from_secs(5),
        connect_timeout: Duration::from_secs(2),
    }
}

async fn start_proxy(backend: String, secret: &[u8]) -> String {
    start_proxy_with(&[backend], secret).await
}

async fn start_proxy_with(backends: &[String], secret: &[u8]) -> String {
    start_hooked(backends, secret, Recorder::default()).await.0
}

async fn start_hooked(
    backends: &[String],
    secret: &[u8],
    hooks: Recorder,
) -> (String, Arc<Shared<Offline, Recorder>>) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("rspigot_proxy_network=debug")
        .with_test_writer()
        .try_init();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let names: Vec<String> = (0..backends.len()).map(|i| format!("server-{i}")).collect();
    let servers = names
        .iter()
        .cloned()
        .zip(backends.iter().cloned())
        .collect();
    let registry = ServerRegistry::new(servers, names).unwrap();
    let shared = Arc::new(Shared::new(settings(secret), registry, Offline, hooks).unwrap());
    tokio::spawn(serve(listener, Arc::clone(&shared)));
    (address, shared)
}

#[derive(Default)]
struct Recorder {
    invocations: Mutex<Vec<Invocation>>,
    connected: Mutex<Vec<(String, Option<String>)>>,
    initial: Mutex<Option<Verdict>>,
    pre_connect: Mutex<Option<Verdict>>,
    kicked: Mutex<Option<Verdict>>,
}

impl ProxyHooks for Recorder {
    fn command(&self, _player: &PlayerView, invocation: Invocation) {
        self.invocations.lock().unwrap().push(invocation);
    }

    fn connected(&self, _player: &PlayerView, server: &str, previous: Option<&str>) {
        self.connected
            .lock()
            .unwrap()
            .push((server.to_owned(), previous.map(str::to_owned)));
    }

    fn initial_server(
        &self,
        _player: PlayerView,
        _candidates: Vec<String>,
    ) -> impl Future<Output = Verdict> + Send {
        let verdict = self
            .initial
            .lock()
            .unwrap()
            .clone()
            .unwrap_or(Verdict::Allow);
        async move { verdict }
    }

    fn pre_connect(
        &self,
        _player: PlayerView,
        _target: String,
        _cause: ConnectCause,
    ) -> impl Future<Output = Verdict> + Send {
        let verdict = self
            .pre_connect
            .lock()
            .unwrap()
            .clone()
            .unwrap_or(Verdict::Allow);
        async move { verdict }
    }

    fn kicked(
        &self,
        _player: PlayerView,
        _server: String,
        _reason: String,
        _fallbacks: Vec<String>,
    ) -> impl Future<Output = Verdict> + Send {
        let verdict = self
            .kicked
            .lock()
            .unwrap()
            .clone()
            .unwrap_or(Verdict::Allow);
        async move { verdict }
    }
}

async fn listener() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    (listener, address)
}

async fn login(proxy: &str, name: &str) -> Peer {
    let mut player = client(proxy, ClientIntent::Login).await;
    player
        .send(&ServerboundHello {
            name: name.to_owned(),
            profile_id: Uuid::nil(),
        })
        .await;
    player
}

async fn finish_login(player: &mut Peer, backend: &mut Peer, id: Uuid) {
    backend
        .send(&LoginFinished {
            profile: WireProfile {
                id,
                name: "Steve".to_owned(),
                properties: Vec::new(),
            },
            session_id: Uuid::from_u128(9),
        })
        .await;
    let packet = player.next().await;
    let (id, body) = split_packet_id(&packet).unwrap();
    assert_eq!(id, ids::login::clientbound::LOGIN_COMPRESSION);
    let LoginCompression { threshold } = decode_exact(body).unwrap();
    player.compress(usize::try_from(threshold).unwrap());
    let packet = player.next().await;
    let (id, _) = split_packet_id(&packet).unwrap();
    assert_eq!(id, ids::login::clientbound::LOGIN_FINISHED);
    player.send(&LoginAcknowledged).await;
    let packet = backend.next().await;
    let (id, _) = split_packet_id(&packet).unwrap();
    assert_eq!(id, ids::login::serverbound::LOGIN_ACKNOWLEDGED);
}

async fn expect_closed(peer: &mut Peer) {
    let mut chunk = [0u8; 64];
    let read = tokio::time::timeout(Duration::from_secs(5), peer.stream.read(&mut chunk))
        .await
        .expect("timed out");
    assert!(matches!(read, Ok(0) | Err(_)), "connection still open");
}

fn raw(id: i32, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    VarInt(id).encode(&mut out);
    out.extend_from_slice(body);
    out
}

async fn client(proxy: &str, intent: ClientIntent) -> Peer {
    let mut peer = Peer::new(TcpStream::connect(proxy).await.unwrap());
    peer.send(&Intention {
        protocol_version: rspigot_data::PROTOCOL_VERSION,
        host: "play.example.com".to_owned(),
        port: 25565,
        intent,
    })
    .await;
    peer
}

async fn backend_accepts_forwarded_player(
    listener: &TcpListener,
) -> (Peer, rspigot_network::Forwarded) {
    let (stream, _) = listener.accept().await.unwrap();
    let mut backend = Peer::new(stream);
    let packet = backend.next().await;
    let (id, _) = split_packet_id(&packet).unwrap();
    assert_eq!(id, ids::handshake::serverbound::INTENTION);
    let packet = backend.next().await;
    let (id, body) = split_packet_id(&packet).unwrap();
    assert_eq!(id, ids::login::serverbound::HELLO);
    let hello: ServerboundHello = decode_exact(body).unwrap();
    assert_eq!(hello.name, "Steve");
    backend
        .send(&CustomQuery {
            transaction_id: 7,
            channel: Identifier::parse("velocity:player_info").unwrap(),
            data: vec![4],
        })
        .await;
    let packet = backend.next().await;
    let (_, body) = split_packet_id(&packet).unwrap();
    let answer: CustomQueryAnswer = decode_exact(body).unwrap();
    assert_eq!(answer.transaction_id, 7);
    let payload = answer.payload.expect("forwarding payload");
    match verify_velocity(SECRET, &payload) {
        Ok(forwarded) => (backend, forwarded),
        Err(error) => {
            backend
                .send(&LoginDisconnect {
                    reason: JsonText(format!("{{\"text\":\"{error}\"}}")),
                })
                .await;
            panic!("forwarding refused: {error}");
        }
    }
}

#[tokio::test]
async fn a_client_reaches_the_backend_with_its_forwarded_identity() {
    let backend_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let backend_address = backend_listener.local_addr().unwrap().to_string();
    let proxy = start_proxy(backend_address, SECRET).await;

    let mut player = client(&proxy, ClientIntent::Login).await;
    player
        .send(&ServerboundHello {
            name: "Steve".to_owned(),
            profile_id: Uuid::nil(),
        })
        .await;

    let (mut backend, forwarded) = backend_accepts_forwarded_player(&backend_listener).await;
    assert_eq!(forwarded.id, PlayerId::offline("Steve").0);
    assert_eq!(forwarded.name.as_deref(), Some("Steve"));
    assert!(forwarded.address.is_loopback());

    backend.send(&LoginCompression { threshold: 128 }).await;
    backend.compress(128);
    backend
        .send(&LoginFinished {
            profile: WireProfile {
                id: forwarded.id,
                name: "Steve".to_owned(),
                properties: Vec::new(),
            },
            session_id: Uuid::from_u128(9),
        })
        .await;

    let packet = player.next().await;
    let (id, body) = split_packet_id(&packet).unwrap();
    assert_eq!(id, ids::login::clientbound::LOGIN_COMPRESSION);
    let LoginCompression { threshold } = decode_exact(body).unwrap();
    player.compress(usize::try_from(threshold).unwrap());

    let packet = player.next().await;
    let (id, body) = split_packet_id(&packet).unwrap();
    assert_eq!(id, ids::login::clientbound::LOGIN_FINISHED);
    let finished: LoginFinished = decode_exact(body).unwrap();
    assert_eq!(finished.profile.id, forwarded.id);

    player.send(&LoginAcknowledged).await;
    let packet = backend.next().await;
    let (id, _) = split_packet_id(&packet).unwrap();
    assert_eq!(id, ids::login::serverbound::LOGIN_ACKNOWLEDGED);

    let mut big = Vec::new();
    write_packet_with_id(
        ids::configuration::clientbound::CUSTOM_PAYLOAD,
        &vec![7u8; BIG],
        &mut big,
    );
    backend.send_raw(&big).await;
    assert_eq!(player.next().await, big);

    let mut small = Vec::new();
    write_packet_with_id(
        ids::configuration::serverbound::CUSTOM_PAYLOAD,
        &vec![1u8; 3],
        &mut small,
    );
    player.send_raw(&small).await;
    player.send_raw(&big).await;
    assert_eq!(backend.next().await, small);
    assert_eq!(backend.next().await, big);
}

#[tokio::test]
async fn a_wrong_forwarding_secret_shows_the_backend_refusal() {
    let backend_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let backend_address = backend_listener.local_addr().unwrap().to_string();
    let proxy = start_proxy(backend_address, b"not-the-secret").await;

    let mut player = client(&proxy, ClientIntent::Login).await;
    player
        .send(&ServerboundHello {
            name: "Steve".to_owned(),
            profile_id: Uuid::nil(),
        })
        .await;

    let backend =
        tokio::spawn(async move { backend_accepts_forwarded_player(&backend_listener).await });
    let packet = player.next().await;
    let (id, body) = split_packet_id(&packet).unwrap();
    assert_eq!(id, ids::login::clientbound::LOGIN_DISCONNECT);
    let LoginDisconnect { reason } = decode_exact(body).unwrap();
    assert!(reason.0.contains("signature"), "{}", reason.0);
    assert!(backend.await.is_err());
}

#[tokio::test]
async fn an_unreachable_backend_disconnects_the_client_with_a_reason() {
    let unused = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = unused.local_addr().unwrap().to_string();
    drop(unused);
    let proxy = start_proxy(address, SECRET).await;

    let mut player = client(&proxy, ClientIntent::Login).await;
    player
        .send(&ServerboundHello {
            name: "Steve".to_owned(),
            profile_id: Uuid::nil(),
        })
        .await;
    let packet = player.next().await;
    let (id, _) = split_packet_id(&packet).unwrap();
    assert_eq!(id, ids::login::clientbound::LOGIN_DISCONNECT);
}

#[tokio::test]
async fn an_outdated_client_is_told_which_version_to_use() {
    let proxy = start_proxy("127.0.0.1:1".to_owned(), SECRET).await;
    let mut player = Peer::new(TcpStream::connect(&proxy).await.unwrap());
    player
        .send(&Intention {
            protocol_version: 700,
            host: "play.example.com".to_owned(),
            port: 25565,
            intent: ClientIntent::Login,
        })
        .await;
    let packet = player.next().await;
    let (id, body) = split_packet_id(&packet).unwrap();
    assert_eq!(id, ids::login::clientbound::LOGIN_DISCONNECT);
    let LoginDisconnect { reason } = decode_exact(body).unwrap();
    assert!(reason.0.contains("multiplayer.disconnect.incompatible"));
}

#[tokio::test]
async fn the_server_list_ping_shows_the_motd_and_answers_the_ping() {
    let proxy = start_proxy("127.0.0.1:1".to_owned(), SECRET).await;
    let mut player = client(&proxy, ClientIntent::Status).await;
    player.send(&StatusRequest).await;
    let packet = player.next().await;
    let (id, body) = split_packet_id(&packet).unwrap();
    assert_eq!(id, StatusResponse::ID);
    let response: StatusResponse = decode_exact(body).unwrap();
    let value: serde_json::Value = serde_json::from_str(&response.json).unwrap();
    assert_eq!(value["description"]["text"], "Hypolia");
    assert_eq!(value["players"]["max"], 50);
    assert_eq!(value["players"]["online"], 0);

    player.send(&PingRequest { time: 42 }).await;
    let packet = player.next().await;
    let (id, body) = split_packet_id(&packet).unwrap();
    assert_eq!(id, PongResponse::ID);
    let pong: PongResponse = decode_exact(body).unwrap();
    assert_eq!(pong.time, 42);
}

#[tokio::test]
async fn a_dead_first_server_sends_the_player_to_the_next_try_server() {
    let (dead, dead_address) = listener().await;
    drop(dead);
    let (alive, alive_address) = listener().await;
    let proxy = start_proxy_with(&[dead_address, alive_address], SECRET).await;
    let mut player = login(&proxy, "Steve").await;
    let (mut backend, forwarded) = backend_accepts_forwarded_player(&alive).await;
    finish_login(&mut player, &mut backend, forwarded.id).await;
}

#[tokio::test]
async fn a_refusal_is_shown_without_trying_the_next_server() {
    let (first, first_address) = listener().await;
    let (second, second_address) = listener().await;
    let proxy = start_proxy_with(&[first_address, second_address], SECRET).await;
    let mut player = login(&proxy, "Steve").await;
    let (mut backend, _) = backend_accepts_forwarded_player(&first).await;
    backend
        .send(&LoginDisconnect {
            reason: JsonText("{\"text\":\"You are banned\"}".to_owned()),
        })
        .await;
    let packet = player.next().await;
    let (id, body) = split_packet_id(&packet).unwrap();
    assert_eq!(id, ids::login::clientbound::LOGIN_DISCONNECT);
    let LoginDisconnect { reason } = decode_exact(body).unwrap();
    assert!(reason.0.contains("banned"));
    let next = tokio::time::timeout(Duration::from_millis(300), second.accept()).await;
    assert!(next.is_err(), "the proxy tried the next server");
}

#[tokio::test]
async fn the_same_account_cannot_join_twice() {
    let (backend_listener, address) = listener().await;
    let proxy = start_proxy(address, SECRET).await;
    let mut first = login(&proxy, "Steve").await;
    let (mut backend, forwarded) = backend_accepts_forwarded_player(&backend_listener).await;
    finish_login(&mut first, &mut backend, forwarded.id).await;

    let mut second = login(&proxy, "Steve").await;
    let packet = second.next().await;
    let (id, body) = split_packet_id(&packet).unwrap();
    assert_eq!(id, ids::login::clientbound::LOGIN_DISCONNECT);
    let LoginDisconnect { reason } = decode_exact(body).unwrap();
    assert!(reason.0.contains("already connected"));
}

#[tokio::test]
async fn the_ping_counts_the_players_connected_through_the_proxy() {
    let (backend_listener, address) = listener().await;
    let proxy = start_proxy(address, SECRET).await;
    let mut player = login(&proxy, "Steve").await;
    let (mut backend, forwarded) = backend_accepts_forwarded_player(&backend_listener).await;
    finish_login(&mut player, &mut backend, forwarded.id).await;

    let mut ping = client(&proxy, ClientIntent::Status).await;
    ping.send(&StatusRequest).await;
    let packet = ping.next().await;
    let (_, body) = split_packet_id(&packet).unwrap();
    let response: StatusResponse = decode_exact(body).unwrap();
    let value: serde_json::Value = serde_json::from_str(&response.json).unwrap();
    assert_eq!(value["players"]["online"], 1);
}

#[tokio::test]
async fn losing_the_backend_in_play_tells_the_player_and_closes() {
    let (backend_listener, address) = listener().await;
    let proxy = start_proxy(address, SECRET).await;
    let mut player = login(&proxy, "Steve").await;
    let (mut backend, forwarded) = backend_accepts_forwarded_player(&backend_listener).await;
    finish_login(&mut player, &mut backend, forwarded.id).await;

    let finish = raw(ids::configuration::serverbound::FINISH_CONFIGURATION, &[]);
    player.send_raw(&finish).await;
    assert_eq!(backend.next().await, finish);
    drop(backend);

    let packet = player.next().await;
    let (id, _) = split_packet_id(&packet).unwrap();
    assert_eq!(id, ids::play::clientbound::DISCONNECT);
    expect_closed(&mut player).await;
}

#[tokio::test]
async fn losing_the_backend_in_configuration_uses_the_configuration_disconnect() {
    let (backend_listener, address) = listener().await;
    let proxy = start_proxy(address, SECRET).await;
    let mut player = login(&proxy, "Steve").await;
    let (mut backend, forwarded) = backend_accepts_forwarded_player(&backend_listener).await;
    finish_login(&mut player, &mut backend, forwarded.id).await;
    drop(backend);

    let packet = player.next().await;
    let (id, _) = split_packet_id(&packet).unwrap();
    assert_eq!(id, ids::configuration::clientbound::DISCONNECT);
}

#[tokio::test]
async fn a_kick_reaches_the_player_and_ends_the_session() {
    let (backend_listener, address) = listener().await;
    let proxy = start_proxy(address, SECRET).await;
    let mut player = login(&proxy, "Steve").await;
    let (mut backend, forwarded) = backend_accepts_forwarded_player(&backend_listener).await;
    finish_login(&mut player, &mut backend, forwarded.id).await;

    let kick = raw(
        ids::configuration::clientbound::DISCONNECT,
        &[8, 0, 4, b'b', b'y', b'e', b'!'],
    );
    backend.send_raw(&kick).await;
    assert_eq!(player.next().await, kick);
    expect_closed(&mut player).await;

    let mut again = login(&proxy, "Steve").await;
    let (mut backend, forwarded) = backend_accepts_forwarded_player(&backend_listener).await;
    finish_login(&mut again, &mut backend, forwarded.id).await;
}

fn string_body(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    text.to_owned().encode(&mut out);
    out
}

fn id_of(packet: &[u8]) -> i32 {
    split_packet_id(packet).unwrap().0
}

struct InPlay {
    player: Peer,
    backend: Peer,
    information: Vec<u8>,
    brand: Vec<u8>,
    chat_session: Vec<u8>,
}

async fn join_and_play(proxy: &str, first: &TcpListener) -> InPlay {
    let mut player = login(proxy, "Steve").await;
    let (mut backend, forwarded) = backend_accepts_forwarded_player(first).await;
    finish_login(&mut player, &mut backend, forwarded.id).await;
    let information = raw(
        ids::configuration::serverbound::CLIENT_INFORMATION,
        &[5, b'e', b'n', b'_', b'u', b's', 10],
    );
    let mut brand_body = string_body("minecraft:brand");
    brand_body.extend(string_body("vanilla"));
    let brand = raw(ids::configuration::serverbound::CUSTOM_PAYLOAD, &brand_body);
    let finish = raw(ids::configuration::serverbound::FINISH_CONFIGURATION, &[]);
    let chat_session = raw(ids::play::serverbound::CHAT_SESSION_UPDATE, &[1, 2, 3, 4]);
    for packet in [&information, &brand, &finish, &chat_session] {
        player.send_raw(packet).await;
        assert_eq!(&backend.next().await, packet);
    }
    let mut tab = Vec::new();
    write_packet(
        &PlayerInfoUpdate {
            actions: PlayerInfoActions(1),
            entries: vec![PlayerInfoEntry {
                profile_id: Uuid::from_u128(77),
                name: "Npc".to_owned(),
                properties: Vec::new(),
                chat_session: None,
                game_mode: GameMode::Survival,
                listed: true,
                latency: 0,
                display_name: None,
                list_order: 0,
                show_hat: true,
            }],
        },
        &mut tab,
    );
    backend.send_raw(&tab).await;
    assert_eq!(player.next().await, tab);
    InPlay {
        player,
        backend,
        information,
        brand,
        chat_session,
    }
}

async fn expect_moved(game: &mut InPlay, second: &TcpListener) -> Peer {
    expect_moved_with(game, second, true).await
}

async fn expect_moved_with(game: &mut InPlay, second: &TcpListener, notice: bool) -> Peer {
    let (mut next, forwarded) = backend_accepts_forwarded_player(second).await;
    next.send(&LoginFinished {
        profile: WireProfile {
            id: forwarded.id,
            name: "Steve".to_owned(),
            properties: Vec::new(),
        },
        session_id: Uuid::from_u128(10),
    })
    .await;
    let mut seen = Vec::new();
    for _ in 0..(3 + usize::from(notice)) {
        let packet = game.player.next().await;
        seen.push(id_of(&packet));
    }
    if !notice {
        seen.insert(0, ids::play::clientbound::SYSTEM_CHAT);
    }
    assert_eq!(
        seen,
        [
            ids::play::clientbound::SYSTEM_CHAT,
            ids::play::clientbound::PLAYER_INFO_REMOVE,
            ids::play::clientbound::CLEAR_TITLES,
            ids::play::clientbound::START_CONFIGURATION,
        ]
    );
    game.player
        .send_raw(&raw(
            ids::play::serverbound::CONFIGURATION_ACKNOWLEDGED,
            &[],
        ))
        .await;
    assert_eq!(
        id_of(&next.next().await),
        ids::login::serverbound::LOGIN_ACKNOWLEDGED
    );
    assert_eq!(next.next().await, game.information);
    assert_eq!(next.next().await, game.brand);

    let registry = raw(ids::configuration::clientbound::REGISTRY_DATA, &[1, 2, 3]);
    next.send_raw(&registry).await;
    assert_eq!(game.player.next().await, registry);
    let finish = raw(ids::configuration::serverbound::FINISH_CONFIGURATION, &[]);
    game.player.send_raw(&finish).await;
    assert_eq!(next.next().await, finish);
    assert_eq!(next.next().await, game.chat_session);
    next
}

#[tokio::test]
async fn a_kick_in_play_moves_the_player_to_the_next_server() {
    let (first, first_address) = listener().await;
    let (second, second_address) = listener().await;
    let proxy = start_proxy_with(&[first_address, second_address], SECRET).await;
    let mut game = join_and_play(&proxy, &first).await;

    game.backend
        .send_raw(&raw(
            ids::play::clientbound::DISCONNECT,
            &[8, 0, 4, b'b', b'y', b'e', b'!'],
        ))
        .await;
    let mut next = expect_moved(&mut game, &second).await;

    let chat = raw(ids::play::serverbound::CHAT_COMMAND, &string_body("list"));
    game.player.send_raw(&chat).await;
    assert_eq!(next.next().await, chat);
}

#[tokio::test]
async fn a_backend_lost_in_play_moves_the_player_to_the_next_server() {
    let (first, first_address) = listener().await;
    let (second, second_address) = listener().await;
    let proxy = start_proxy_with(&[first_address, second_address], SECRET).await;
    let mut game = join_and_play(&proxy, &first).await;
    game.backend.stream.shutdown().await.unwrap();
    let _next = expect_moved(&mut game, &second).await;
}

#[tokio::test]
async fn a_kick_with_no_live_fallback_reaches_the_player() {
    let (first, first_address) = listener().await;
    let (dead, dead_address) = listener().await;
    drop(dead);
    let proxy = start_proxy_with(&[first_address, dead_address], SECRET).await;
    let mut game = join_and_play(&proxy, &first).await;

    let kick = raw(
        ids::play::clientbound::DISCONNECT,
        &[8, 0, 4, b'b', b'y', b'e', b'!'],
    );
    game.backend.send_raw(&kick).await;
    assert_eq!(game.player.next().await, kick);
    expect_closed(&mut game.player).await;
}

fn spec(name: &str, permission: Option<&str>, arguments: Vec<CommandArgument>) -> CommandSpec {
    CommandSpec {
        name: name.to_owned(),
        aliases: Vec::new(),
        permission: permission.map(str::to_owned),
        arguments,
    }
}

fn steve() -> Uuid {
    PlayerId::offline("Steve").0
}

fn tree_packet(literals: &[&str]) -> Vec<u8> {
    let mut nodes = vec![CommandNode {
        kind: CommandNodeKind::Root,
        executable: false,
        restricted: false,
        children: (1..=literals.len() as i32).collect(),
        redirect: None,
    }];
    for name in literals {
        nodes.push(CommandNode {
            kind: CommandNodeKind::Literal {
                name: (*name).to_owned(),
            },
            executable: true,
            restricted: false,
            children: Vec::new(),
            redirect: None,
        });
    }
    let mut out = Vec::new();
    write_packet(
        &Commands {
            nodes,
            root_index: 0,
        },
        &mut out,
    );
    out
}

fn literals_of(packet: &[u8]) -> Vec<String> {
    let (id, body) = split_packet_id(packet).unwrap();
    assert_eq!(id, ids::play::clientbound::COMMANDS);
    let tree: Commands = decode_exact(body).unwrap();
    tree.nodes[tree.root_index as usize]
        .children
        .iter()
        .filter_map(|child| match &tree.nodes[*child as usize].kind {
            CommandNodeKind::Literal { name } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_proxy_command_reaches_the_plugin_and_never_the_backend() {
    let (first, first_address) = listener().await;
    let (proxy, shared) = start_hooked(&[first_address], SECRET, Recorder::default()).await;
    shared.register_command(1, spec("hub", None, Vec::new()));
    let mut game = join_and_play(&proxy, &first).await;

    game.player
        .send_raw(&raw(
            ids::play::serverbound::CHAT_COMMAND,
            &string_body("hub"),
        ))
        .await;
    let list = raw(ids::play::serverbound::CHAT_COMMAND, &string_body("list"));
    game.player.send_raw(&list).await;
    assert_eq!(game.backend.next().await, list);
    let invocations = shared.hooks().invocations.lock().unwrap().clone();
    assert_eq!(invocations.len(), 1);
    assert_eq!(invocations[0].command, "hub");
}

#[tokio::test]
async fn the_command_tree_shows_proxy_commands_the_player_may_use() {
    let (first, first_address) = listener().await;
    let (proxy, shared) = start_hooked(&[first_address], SECRET, Recorder::default()).await;
    shared.register_command(1, spec("hub", None, Vec::new()));
    shared.register_command(
        1,
        spec(
            "server",
            Some("proxy.command.server"),
            vec![CommandArgument {
                name: "server".to_owned(),
                kind: ArgumentKind::Server,
                optional: true,
                choices: Vec::new(),
            }],
        ),
    );
    let mut game = join_and_play(&proxy, &first).await;

    game.backend
        .send_raw(&tree_packet(&["gamemode", "hub"]))
        .await;
    assert_eq!(literals_of(&game.player.next().await), ["gamemode", "hub"]);

    shared.set_permissions(
        steve(),
        Permissions::new(["proxy.command.*".to_owned()], []),
    );
    assert_eq!(
        literals_of(&game.player.next().await),
        ["gamemode", "hub", "server"]
    );

    let mut request = Vec::new();
    VarInt(3).encode(&mut request);
    request.extend(string_body("/server se"));
    game.player
        .send_raw(&raw(ids::play::serverbound::COMMAND_SUGGESTION, &request))
        .await;
    let packet = game.player.next().await;
    let (id, body) = split_packet_id(&packet).unwrap();
    assert_eq!(id, ids::play::clientbound::COMMAND_SUGGESTIONS);
    let suggestions: CommandSuggestions = decode_exact(body).unwrap();
    assert_eq!(suggestions.id, 3);
    assert_eq!(suggestions.start, 8);
    let texts: Vec<&str> = suggestions
        .suggestions
        .iter()
        .map(|s| s.text.as_str())
        .collect();
    assert_eq!(texts, ["server-0"]);
}

#[tokio::test]
async fn a_plugin_can_move_a_player_and_learns_the_outcome() {
    let (first, first_address) = listener().await;
    let (second, second_address) = listener().await;
    let (proxy, shared) = start_hooked(
        &[first_address, second_address],
        SECRET,
        Recorder::default(),
    )
    .await;
    let mut game = join_and_play(&proxy, &first).await;

    assert_eq!(
        shared.connect(steve(), "server-0").await,
        ConnectOutcome::AlreadyThere
    );
    assert_eq!(
        shared.connect(steve(), "nowhere").await,
        ConnectOutcome::UnknownServer
    );
    let mover = Arc::clone(&shared);
    let outcome = tokio::spawn(async move { mover.connect(steve(), "server-1").await });
    let _next = expect_moved_with(&mut game, &second, false).await;
    assert_eq!(outcome.await.unwrap(), ConnectOutcome::Connected);
    let connected = shared.hooks().connected.lock().unwrap().clone();
    assert_eq!(
        connected,
        [
            ("server-0".to_owned(), None),
            ("server-1".to_owned(), Some("server-0".to_owned()))
        ]
    );
    assert_eq!(
        shared.player("Steve").unwrap().server.as_deref(),
        Some("server-1")
    );
}

#[tokio::test]
async fn plugins_send_messages_and_kick() {
    let (first, first_address) = listener().await;
    let (proxy, shared) = start_hooked(&[first_address], SECRET, Recorder::default()).await;
    let mut game = join_and_play(&proxy, &first).await;

    shared.send_message(None, &rspigot_proxy_network::text::legacy("§aHello"));
    assert_eq!(
        id_of(&game.player.next().await),
        ids::play::clientbound::SYSTEM_CHAT
    );
    assert!(shared.kick(steve(), rspigot_proxy_network::text::legacy("§cBye")));
    assert_eq!(
        id_of(&game.player.next().await),
        ids::play::clientbound::DISCONNECT
    );
    expect_closed(&mut game.player).await;
}

#[tokio::test]
async fn a_pre_connect_deny_refuses_the_login_with_its_message() {
    let (_first, first_address) = listener().await;
    let hooks = Recorder::default();
    *hooks.pre_connect.lock().unwrap() = Some(Verdict::Deny("§cMaintenance".to_owned()));
    let (proxy, _shared) = start_hooked(&[first_address], SECRET, hooks).await;
    let mut player = login(&proxy, "Steve").await;
    let packet = player.next().await;
    let (id, body) = split_packet_id(&packet).unwrap();
    assert_eq!(id, ids::login::clientbound::LOGIN_DISCONNECT);
    let LoginDisconnect { reason } = decode_exact(body).unwrap();
    assert!(reason.0.contains("Maintenance"));
}

#[tokio::test]
async fn the_initial_server_hook_picks_where_the_player_lands() {
    let (_first, first_address) = listener().await;
    let (second, second_address) = listener().await;
    let hooks = Recorder::default();
    *hooks.initial.lock().unwrap() = Some(Verdict::Redirect {
        server: "server-1".to_owned(),
        message: None,
    });
    let (proxy, _shared) = start_hooked(&[first_address, second_address], SECRET, hooks).await;
    let mut player = login(&proxy, "Steve").await;
    let (mut backend, forwarded) = backend_accepts_forwarded_player(&second).await;
    finish_login(&mut player, &mut backend, forwarded.id).await;
}

#[tokio::test]
async fn a_kicked_hook_can_refuse_the_fallback() {
    let (first, first_address) = listener().await;
    let (second, second_address) = listener().await;
    let hooks = Recorder::default();
    *hooks.kicked.lock().unwrap() = Some(Verdict::Deny("§cBanned".to_owned()));
    let (proxy, _shared) = start_hooked(&[first_address, second_address], SECRET, hooks).await;
    let mut game = join_and_play(&proxy, &first).await;
    game.backend
        .send_raw(&raw(ids::play::clientbound::DISCONNECT, &[8, 0, 1, b'x']))
        .await;
    assert_eq!(
        id_of(&game.player.next().await),
        ids::play::clientbound::DISCONNECT
    );
    expect_closed(&mut game.player).await;
    let next = tokio::time::timeout(Duration::from_millis(300), second.accept()).await;
    assert!(next.is_err(), "the proxy moved the player anyway");
}
