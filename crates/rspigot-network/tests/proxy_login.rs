use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, unbounded};
use rspigot_core::events::{Inbound, PreLoginVerdict};
use rspigot_core::ports::{AuthError, SessionVerifier};
use rspigot_core::status::StatusBoard;
use rspigot_domain::player::GameProfile;
use rspigot_network::{NetLink, NetworkConfig, ProxyForwarding, hmac_sha256, serve};
use rspigot_protocol::codec::{Decode, Encode, VarInt};
use rspigot_protocol::frame::{FrameDecoder, FrameEncoder};
use rspigot_protocol::handshake::{ClientIntent, Intention};
use rspigot_protocol::ids;
use rspigot_protocol::login::{CustomQuery, CustomQueryAnswer, ServerboundHello};
use rspigot_protocol::{split_packet_id, write_packet_with_id};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use uuid::Uuid;

struct Never;

impl SessionVerifier for Never {
    fn verify(&self, username: &str, _: &str, _: Option<IpAddr>) -> Result<GameProfile, AuthError> {
        Err(AuthError::NotAuthenticated(username.to_owned()))
    }
}

struct Started {
    address: SocketAddr,
    game: Receiver<Inbound<NetLink>>,
    _stop: watch::Sender<bool>,
}

async fn start(proxy: ProxyForwarding) -> Started {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a free port");
    let address = listener.local_addr().expect("bound");
    let config = NetworkConfig {
        bind: address,
        online_mode: true,
        connection_throttle: Duration::ZERO,
        proxy,
        ..NetworkConfig::default()
    };
    let (game_sender, game): (Sender<_>, _) = unbounded();
    let (stop, stopped) = watch::channel(false);
    tokio::spawn(serve(
        listener,
        config,
        Arc::new(Never),
        StatusBoard::new(20),
        game_sender,
        stopped,
    ));
    Started {
        address,
        game,
        _stop: stop,
    }
}

struct Client {
    stream: TcpStream,
    encoder: FrameEncoder,
    decoder: FrameDecoder,
}

impl Client {
    async fn connect(address: SocketAddr, host: &str) -> Self {
        let mut client = Self {
            stream: TcpStream::connect(address).await.expect("connected"),
            encoder: FrameEncoder::new(),
            decoder: FrameDecoder::new(),
        };
        client
            .send(
                ids::handshake::serverbound::INTENTION,
                &Intention {
                    protocol_version: rspigot_data::PROTOCOL_VERSION,
                    host: host.to_owned(),
                    port: 25565,
                    intent: ClientIntent::Login,
                },
            )
            .await;
        client
    }

    async fn send<P: Encode>(&mut self, id: i32, packet: &P) {
        let mut body = Vec::new();
        write_packet_with_id(id, packet, &mut body);
        let mut frames = Vec::new();
        self.encoder
            .encode_packet(&body, &mut frames)
            .expect("framed");
        self.stream.write_all(&frames).await.expect("sent");
    }

    async fn next(&mut self) -> (i32, Vec<u8>) {
        let mut buffer = [0u8; 4096];
        loop {
            if let Some(packet) = self.decoder.next_packet().expect("a frame") {
                let (id, body) = split_packet_id(&packet).expect("an id");
                return (id, body.to_vec());
            }
            let read = tokio::time::timeout(Duration::from_secs(10), self.stream.read(&mut buffer))
                .await
                .expect("in time")
                .expect("read");
            assert!(read > 0, "closed");
            self.decoder.feed(&buffer[..read]);
        }
    }
}

fn hello(name: &str) -> ServerboundHello {
    ServerboundHello {
        name: name.to_owned(),
        profile_id: Uuid::nil(),
    }
}

fn data(address: &str, id: Uuid, name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    VarInt(1).encode(&mut out);
    address.to_owned().encode(&mut out);
    id.encode(&mut out);
    name.to_owned().encode(&mut out);
    VarInt(0).encode(&mut out);
    out
}

fn answer(secret: &[u8], body: Vec<u8>) -> CustomQueryAnswer {
    let mut payload = hmac_sha256(secret, &body).to_vec();
    payload.extend(body);
    CustomQueryAnswer {
        transaction_id: 100,
        payload: Some(payload),
    }
}

fn pre_login(game: &Receiver<Inbound<NetLink>>) -> (String, Uuid, IpAddr) {
    let Ok(Inbound::PreLogin {
        name,
        id,
        address,
        reply,
        ..
    }) = game.recv_timeout(Duration::from_secs(10))
    else {
        panic!("expected a pre-login");
    };
    let _ = reply.send(PreLoginVerdict::Refused("done".to_owned()));
    (name, id.0, address)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn velocity_forwarding_replaces_the_profile_and_address() {
    let started = start(ProxyForwarding::Velocity(b"s3cret".to_vec())).await;
    let mut client = Client::connect(started.address, "localhost").await;
    client
        .send(ids::login::serverbound::HELLO, &hello("Proxy"))
        .await;
    let (id, body) = client.next().await;
    assert_eq!(id, ids::login::clientbound::CUSTOM_QUERY);
    let query = CustomQuery::decode(&mut &body[..]).expect("a query");
    assert_eq!(query.channel.to_string(), "velocity:player_info");
    let uuid = Uuid::from_u128(0xABCD);
    client
        .send(
            ids::login::serverbound::CUSTOM_QUERY_ANSWER,
            &answer(b"s3cret", data("203.0.113.9", uuid, "Steve")),
        )
        .await;
    let (name, id, address) = pre_login(&started.game);
    assert_eq!(name, "Steve");
    assert_eq!(id, uuid);
    assert_eq!(address, "203.0.113.9".parse::<IpAddr>().unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn velocity_forwarding_rejects_a_bad_signature() {
    let started = start(ProxyForwarding::Velocity(b"s3cret".to_vec())).await;
    let mut client = Client::connect(started.address, "localhost").await;
    client
        .send(ids::login::serverbound::HELLO, &hello("Proxy"))
        .await;
    client.next().await;
    client
        .send(
            ids::login::serverbound::CUSTOM_QUERY_ANSWER,
            &answer(b"wrong", data("203.0.113.9", Uuid::nil(), "Steve")),
        )
        .await;
    let (id, _) = client.next().await;
    assert_eq!(id, ids::login::clientbound::LOGIN_DISCONNECT);
    assert!(started.game.try_recv().is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bungeecord_forwarding_reads_the_handshake_host() {
    let started = start(ProxyForwarding::BungeeCord).await;
    let host = "mc\u{0}198.51.100.4\u{0}11111111222233334444555555555555\u{0}[]";
    let mut client = Client::connect(started.address, host).await;
    client
        .send(ids::login::serverbound::HELLO, &hello("Alex"))
        .await;
    let (name, id, address) = pre_login(&started.game);
    assert_eq!(name, "Alex");
    assert_eq!(
        id,
        Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap()
    );
    assert_eq!(address, "198.51.100.4".parse::<IpAddr>().unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bungeecord_forwarding_refuses_a_direct_connection() {
    let started = start(ProxyForwarding::BungeeCord).await;
    let mut client = Client::connect(started.address, "localhost").await;
    let (id, _) = client.next().await;
    assert_eq!(id, ids::login::clientbound::LOGIN_DISCONNECT);
}
