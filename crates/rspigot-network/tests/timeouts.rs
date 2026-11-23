use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::{Receiver, unbounded};
use rspigot_core::events::Inbound;
use rspigot_core::ports::{AuthError, SessionVerifier};
use rspigot_core::status::StatusBoard;
use rspigot_domain::player::GameProfile;
use rspigot_network::{NetLink, NetworkConfig, serve};
use rspigot_protocol::codec::Encode;
use rspigot_protocol::frame::{FrameDecoder, FrameEncoder};
use rspigot_protocol::handshake::{ClientIntent, Intention};
use rspigot_protocol::ids;
use rspigot_protocol::login::ServerboundHello;
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
    _game: Receiver<Inbound<NetLink>>,
    _stop: watch::Sender<bool>,
}

async fn start(read_timeout: Duration, login_timeout: Duration) -> Started {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a free port");
    let address = listener.local_addr().expect("bound");
    let config = NetworkConfig {
        bind: address,
        online_mode: false,
        connection_throttle: Duration::ZERO,
        read_timeout,
        login_timeout,
        ..NetworkConfig::default()
    };
    let (sender, game) = unbounded();
    let (stop, stopped) = watch::channel(false);
    tokio::spawn(serve(
        listener,
        config,
        Arc::new(Never),
        StatusBoard::new(20),
        sender,
        stopped,
    ));
    Started {
        address,
        _game: game,
        _stop: stop,
    }
}

fn frame<P: Encode>(id: i32, packet: &P) -> Vec<u8> {
    let mut body = Vec::new();
    write_packet_with_id(id, packet, &mut body);
    let mut frames = Vec::new();
    FrameEncoder::new()
        .encode_packet(&body, &mut frames)
        .expect("framed");
    frames
}

fn handshake() -> Vec<u8> {
    frame(
        ids::handshake::serverbound::INTENTION,
        &Intention {
            protocol_version: rspigot_data::PROTOCOL_VERSION,
            host: "localhost".to_owned(),
            port: 25565,
            intent: ClientIntent::Login,
        },
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn a_silent_socket_is_closed_after_the_read_timeout() {
    let server = start(Duration::from_millis(150), Duration::from_secs(30)).await;
    let mut stream = TcpStream::connect(server.address).await.expect("connected");
    stream.write_all(&handshake()).await.expect("sent");
    let mut buffer = [0u8; 64];
    let read = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buffer))
        .await
        .expect("closed before the test gives up");
    assert!(matches!(read, Ok(0) | Err(_)));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_login_that_never_finishes_is_kicked_with_slow_login() {
    let server = start(Duration::from_secs(30), Duration::from_millis(200)).await;
    let mut stream = TcpStream::connect(server.address).await.expect("connected");
    stream.write_all(&handshake()).await.expect("sent");
    stream
        .write_all(&frame(
            ids::login::serverbound::HELLO,
            &ServerboundHello {
                name: "Steve".to_owned(),
                profile_id: Uuid::nil(),
            },
        ))
        .await
        .expect("sent");
    let mut decoder = FrameDecoder::new();
    let mut buffer = [0u8; 1024];
    let packet = loop {
        if let Some(packet) = decoder.next_packet().expect("a frame") {
            break packet;
        }
        let read = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buffer))
            .await
            .expect("in time")
            .expect("read");
        assert!(read > 0, "closed without a reason");
        decoder.feed(&buffer[..read]);
    };
    let (id, body) = split_packet_id(&packet).expect("an id");
    assert_eq!(id, ids::login::clientbound::LOGIN_DISCONNECT);
    let text = String::from_utf8_lossy(body);
    assert!(text.contains("multiplayer.disconnect.slow_login"), "{text}");
}
