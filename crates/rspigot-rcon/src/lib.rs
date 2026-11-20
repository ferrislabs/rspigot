use std::sync::Arc;

use rspigot_core::ports::RconExecutor;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

pub const MAX_PAYLOAD: usize = 4096;
const MAX_REQUEST_LENGTH: i32 = 1460;
const MIN_LENGTH: i32 = 10;
const LOGIN: i32 = 3;
const COMMAND: i32 = 2;
const RESPONSE: i32 = 0;
const AUTH_RESPONSE: i32 = 2;
const AUTH_FAILED: i32 = -1;

#[derive(Debug, thiserror::Error)]
pub enum RconError {
    #[error("rcon is enabled but rcon.password is empty: set a password, rcon is not started")]
    EmptyPassword,
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RconConfig {
    pub host: String,
    pub port: u16,
    pub password: String,
}

impl RconConfig {
    pub fn validate(&self) -> Result<(), RconError> {
        if self.password.is_empty() {
            return Err(RconError::EmptyPassword);
        }
        Ok(())
    }

    pub fn address(&self) -> String {
        let host = if self.host.is_empty() {
            "0.0.0.0"
        } else {
            &self.host
        };
        if host.contains(':') {
            format!("[{host}]:{}", self.port)
        } else {
            format!("{host}:{}", self.port)
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Packet {
    id: i32,
    kind: i32,
    body: Vec<u8>,
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diff |= usize::from(a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0));
    }
    diff == 0
}

async fn read_int<R: AsyncRead + Unpin>(reader: &mut R) -> std::io::Result<i32> {
    let mut raw = [0u8; 4];
    reader.read_exact(&mut raw).await?;
    Ok(i32::from_le_bytes(raw))
}

async fn read_packet<R: AsyncRead + Unpin>(reader: &mut R) -> std::io::Result<Option<Packet>> {
    let length = read_int(reader).await?;
    if !(MIN_LENGTH..=MAX_REQUEST_LENGTH).contains(&length) {
        return Ok(None);
    }
    let id = read_int(reader).await?;
    let kind = read_int(reader).await?;
    let mut rest = vec![0u8; usize::try_from(length - 8).unwrap_or(2)];
    reader.read_exact(&mut rest).await?;
    rest.truncate(rest.len() - 2);
    Ok(Some(Packet {
        id,
        kind,
        body: rest,
    }))
}

fn encode(id: i32, kind: i32, body: &[u8]) -> Vec<u8> {
    let length = i32::try_from(body.len() + 10).unwrap_or(i32::MAX);
    let mut out = Vec::with_capacity(body.len() + 14);
    out.extend_from_slice(&length.to_le_bytes());
    out.extend_from_slice(&id.to_le_bytes());
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(body);
    out.extend_from_slice(&[0, 0]);
    out
}

fn chunks(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return vec![text];
    }
    let mut parts = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let mut end = rest.len().min(MAX_PAYLOAD);
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        let (head, tail) = rest.split_at(end);
        parts.push(head);
        rest = tail;
    }
    parts
}

async fn send_output<W: AsyncWrite + Unpin>(
    writer: &mut W,
    id: i32,
    text: &str,
) -> std::io::Result<()> {
    let mut frames = Vec::new();
    for part in chunks(text) {
        frames.extend_from_slice(&encode(id, RESPONSE, part.as_bytes()));
    }
    writer.write_all(&frames).await
}

async fn run<E: RconExecutor>(executor: &Arc<E>, command: String) -> String {
    let executor = Arc::clone(executor);
    tokio::task::spawn_blocking(move || executor.execute(&command))
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
}

async fn session<S, E>(stream: S, password: &str, executor: &Arc<E>) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
    E: RconExecutor,
{
    let (mut reader, mut writer) = tokio::io::split(stream);
    let mut authed = false;
    while let Some(packet) = read_packet(&mut reader).await? {
        match packet.kind {
            LOGIN => {
                authed = constant_time_eq(&packet.body, password.as_bytes());
                let id = if authed { packet.id } else { AUTH_FAILED };
                writer.write_all(&encode(id, AUTH_RESPONSE, &[])).await?;
            }
            COMMAND if authed => {
                let command = String::from_utf8_lossy(&packet.body).into_owned();
                let output = run(executor, command).await;
                send_output(&mut writer, packet.id, &output).await?;
            }
            COMMAND => {
                writer
                    .write_all(&encode(AUTH_FAILED, AUTH_RESPONSE, &[]))
                    .await?;
            }
            other => {
                let text = format!("Unknown request {other:x}");
                send_output(&mut writer, packet.id, &text).await?;
            }
        }
    }
    Ok(())
}

pub async fn serve<E: RconExecutor>(
    listener: TcpListener,
    config: RconConfig,
    executor: Arc<E>,
    mut stopped: watch::Receiver<bool>,
) {
    match listener.local_addr() {
        Ok(address) => tracing::info!("RCON running on {address}"),
        Err(error) => tracing::warn!(%error, "rcon listener address unknown"),
    }
    let password: Arc<str> = Arc::from(config.password);
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let Ok((stream, peer)) = accepted else { continue };
                let password = Arc::clone(&password);
                let executor = Arc::clone(&executor);
                let mut stopped = stopped.clone();
                tokio::spawn(async move {
                    tracing::info!("Rcon connection from: {}", peer.ip());
                    let outcome = tokio::select! {
                        outcome = connection(stream, &password, &executor) => outcome,
                        _ = stopped.changed() => Ok(()),
                    };
                    if let Err(error) = outcome {
                        tracing::debug!(%peer, %error, "rcon connection ended");
                    }
                });
            }
            _ = stopped.changed() => break,
        }
    }
}

async fn connection<E: RconExecutor>(
    stream: TcpStream,
    password: &str,
    executor: &Arc<E>,
) -> std::io::Result<()> {
    session(stream, password, executor).await
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Echo;

    impl RconExecutor for Echo {
        fn execute(&self, command: &str) -> Option<String> {
            Some(match command {
                "silent" => String::new(),
                "big" => "a".repeat(MAX_PAYLOAD * 2 + 10),
                "wide" => "é".repeat(MAX_PAYLOAD),
                other => format!("ran {other}"),
            })
        }
    }

    async fn start() -> (std::net::SocketAddr, watch::Sender<bool>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (stop, stopped) = watch::channel(false);
        let config = RconConfig {
            host: String::new(),
            port: 0,
            password: "secret".to_owned(),
        };
        tokio::spawn(serve(listener, config, Arc::new(Echo), stopped));
        (address, stop)
    }

    async fn send(stream: &mut TcpStream, id: i32, kind: i32, body: &str) {
        stream
            .write_all(&encode(id, kind, body.as_bytes()))
            .await
            .unwrap();
    }

    async fn receive(stream: &mut TcpStream) -> Packet {
        read_packet_any(stream).await
    }

    async fn read_packet_any(stream: &mut TcpStream) -> Packet {
        let length = read_int(stream).await.unwrap();
        let id = read_int(stream).await.unwrap();
        let kind = read_int(stream).await.unwrap();
        let mut rest = vec![0u8; usize::try_from(length - 8).unwrap()];
        stream.read_exact(&mut rest).await.unwrap();
        rest.truncate(rest.len() - 2);
        Packet {
            id,
            kind,
            body: rest,
        }
    }

    async fn login(address: std::net::SocketAddr) -> TcpStream {
        let mut stream = TcpStream::connect(address).await.unwrap();
        send(&mut stream, 1, LOGIN, "secret").await;
        let reply = receive(&mut stream).await;
        assert_eq!((reply.id, reply.kind), (1, AUTH_RESPONSE));
        stream
    }

    #[tokio::test]
    async fn a_good_password_logs_in_and_a_command_answers_its_output() {
        let (address, _stop) = start().await;
        let mut stream = login(address).await;
        send(&mut stream, 7, COMMAND, "list").await;
        let reply = receive(&mut stream).await;
        assert_eq!(
            reply,
            Packet {
                id: 7,
                kind: RESPONSE,
                body: b"ran list".to_vec()
            }
        );
    }

    #[tokio::test]
    async fn a_wrong_password_answers_minus_one_and_keeps_the_connection() {
        let (address, _stop) = start().await;
        let mut stream = TcpStream::connect(address).await.unwrap();
        send(&mut stream, 5, LOGIN, "nope").await;
        let reply = receive(&mut stream).await;
        assert_eq!((reply.id, reply.kind), (AUTH_FAILED, AUTH_RESPONSE));
        send(&mut stream, 6, COMMAND, "list").await;
        let refused = receive(&mut stream).await;
        assert_eq!(refused.id, AUTH_FAILED);
        send(&mut stream, 8, LOGIN, "secret").await;
        let accepted = receive(&mut stream).await;
        assert_eq!(accepted.id, 8);
    }

    #[tokio::test]
    async fn output_beyond_the_limit_is_split_into_several_packets() {
        let (address, _stop) = start().await;
        let mut stream = login(address).await;
        send(&mut stream, 9, COMMAND, "big").await;
        let first = receive(&mut stream).await;
        let second = receive(&mut stream).await;
        let third = receive(&mut stream).await;
        assert_eq!(first.body.len(), MAX_PAYLOAD);
        assert_eq!(second.body.len(), MAX_PAYLOAD);
        assert_eq!(third.body.len(), 10);
        assert!([&first, &second, &third].iter().all(|p| p.id == 9));
    }

    #[tokio::test]
    async fn a_split_never_cuts_a_character_in_two() {
        let (address, _stop) = start().await;
        let mut stream = login(address).await;
        send(&mut stream, 4, COMMAND, "wide").await;
        let mut text = String::new();
        while text.chars().count() < MAX_PAYLOAD {
            let part = receive(&mut stream).await;
            assert!(part.body.len() <= MAX_PAYLOAD);
            text.push_str(std::str::from_utf8(&part.body).unwrap());
        }
        assert_eq!(text, "é".repeat(MAX_PAYLOAD));
    }

    #[tokio::test]
    async fn a_command_without_output_answers_an_empty_body() {
        let (address, _stop) = start().await;
        let mut stream = login(address).await;
        send(&mut stream, 3, COMMAND, "silent").await;
        let reply = receive(&mut stream).await;
        assert_eq!(
            reply,
            Packet {
                id: 3,
                kind: RESPONSE,
                body: Vec::new()
            }
        );
    }

    #[tokio::test]
    async fn an_unknown_request_type_is_named_in_hex() {
        let (address, _stop) = start().await;
        let mut stream = login(address).await;
        send(&mut stream, 2, 10, "x").await;
        let reply = receive(&mut stream).await;
        assert_eq!(reply.body, b"Unknown request a");
    }

    #[test]
    fn an_empty_password_refuses_to_start_and_the_host_picks_the_bind_address() {
        let mut config = RconConfig {
            host: String::new(),
            port: 25575,
            password: String::new(),
        };
        assert!(matches!(config.validate(), Err(RconError::EmptyPassword)));
        assert_eq!(config.address(), "0.0.0.0:25575");
        config.host = "10.0.0.2".to_owned();
        assert_eq!(config.address(), "10.0.0.2:25575");
        config.host = "::1".to_owned();
        assert_eq!(config.address(), "[::1]:25575");
    }
}
