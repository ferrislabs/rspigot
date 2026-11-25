use std::sync::Arc;
use std::time::Duration;

use rspigot_core::ports::{AuthError, SessionVerifier};
use rspigot_domain::player::{GameProfile, PlayerId};
use rspigot_network::verify_velocity;
use rspigot_protocol::codec::decode_exact;
use rspigot_protocol::common::Identifier;
use rspigot_protocol::frame::{Compression, FrameDecoder, FrameEncoder};
use rspigot_protocol::handshake::{ClientIntent, Intention};
use rspigot_protocol::ids;
use rspigot_protocol::login::{
    CustomQuery, CustomQueryAnswer, LoginAcknowledged, LoginCompression, LoginDisconnect,
    LoginFinished, ServerboundHello,
};
use rspigot_protocol::status::{PingRequest, PongResponse, StatusRequest, StatusResponse};
use rspigot_protocol::types::{GameProfile as WireProfile, JsonText};
use rspigot_protocol::{Packet, split_packet_id, write_packet, write_packet_with_id};
use rspigot_proxy_core::ServerRegistry;
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
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let names: Vec<String> = (0..backends.len()).map(|i| format!("server-{i}")).collect();
    let servers = names
        .iter()
        .cloned()
        .zip(backends.iter().cloned())
        .collect();
    let registry = ServerRegistry::new(servers, names).unwrap();
    let shared = Arc::new(Shared::new(settings(secret), registry, Offline).unwrap());
    tokio::spawn(serve(listener, shared));
    address
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
    write_packet_with_id(id, &body.to_vec(), &mut out);
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
