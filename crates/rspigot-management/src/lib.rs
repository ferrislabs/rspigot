mod rpc;
mod ws;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::Sender;
use rspigot_core::events::Inbound;
use rspigot_core::management::{ManagementCall, ManagementNotice, ManagementReply};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, watch};

use ws::Frame;

pub const SUBPROTOCOL: &str = "minecraft-v1";
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum ManagementError {
    #[error("the management server secret must be 40 alphanumeric characters")]
    InvalidSecret,
    #[error(
        "the management server needs TLS, which is not supported yet: set management-server-tls-enabled=false and keep it on localhost"
    )]
    TlsUnsupported,
    #[error("the management server refuses to listen beyond loopback without TLS")]
    PublicWithoutTls,
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagementConfig {
    pub host: String,
    pub port: u16,
    pub secret: String,
    pub tls_enabled: bool,
    pub allowed_origins: Vec<String>,
}

impl ManagementConfig {
    pub fn validate(&self) -> Result<(), ManagementError> {
        if self.secret.len() != 40 || !self.secret.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(ManagementError::InvalidSecret);
        }
        if self.tls_enabled {
            return Err(ManagementError::TlsUnsupported);
        }
        if !self.is_loopback() {
            return Err(ManagementError::PublicWithoutTls);
        }
        Ok(())
    }

    fn is_loopback(&self) -> bool {
        self.host.eq_ignore_ascii_case("localhost")
            || self
                .host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    }

    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

pub fn bridge(
    notices: crossbeam_channel::Receiver<ManagementNotice>,
) -> broadcast::Sender<ManagementNotice> {
    let (out, _) = broadcast::channel(256);
    let forward = out.clone();
    let spawned = std::thread::Builder::new()
        .name("management-notices".to_owned())
        .spawn(move || {
            for notice in notices {
                let _ = forward.send(notice);
            }
        });
    if let Err(error) = spawned {
        tracing::error!(%error, "management notice bridge failed to start");
    }
    out
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diff |= usize::from(a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0));
    }
    diff == 0
}

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Allowed { protocol: Option<&'static str> },
    Denied(&'static str),
}

pub fn authenticate(head: &ws::Head, config: &ManagementConfig) -> Verdict {
    if let Some(origin) = head.header("origin")
        && !config.allowed_origins.iter().any(|o| o == origin)
    {
        return Verdict::Denied("Origin Not Allowed");
    }
    let mut offered_protocol = false;
    let mut token: Option<String> = head
        .header("authorization")
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|v| v.trim().to_owned());
    if token.is_none() {
        if let Some(list) = head.header("sec-websocket-protocol") {
            let mut parts = list.split(',').map(str::trim);
            if parts.next() == Some(SUBPROTOCOL) {
                offered_protocol = true;
                token = parts.next().map(str::to_owned);
            }
        }
    } else if head
        .header("sec-websocket-protocol")
        .is_some_and(|v| v.split(',').any(|p| p.trim() == SUBPROTOCOL))
    {
        offered_protocol = true;
    }
    match token {
        None => Verdict::Denied("Missing API key"),
        Some(token) if constant_time_eq(&token, &config.secret) => Verdict::Allowed {
            protocol: offered_protocol.then_some(SUBPROTOCOL),
        },
        Some(_) => Verdict::Denied("Invalid API key"),
    }
}

pub async fn serve<L: Send + 'static>(
    listener: TcpListener,
    config: ManagementConfig,
    inbound: Sender<Inbound<L>>,
    notices: broadcast::Sender<ManagementNotice>,
    mut stopped: watch::Receiver<bool>,
) {
    let config = Arc::new(config);
    match listener.local_addr() {
        Ok(address) => tracing::info!("Json-RPC Management connection listening on {address}"),
        Err(error) => tracing::warn!(%error, "management listener address unknown"),
    }
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let Ok((stream, peer)) = accepted else { continue };
                let config = Arc::clone(&config);
                let inbound = inbound.clone();
                let notices = notices.subscribe();
                let stopped = stopped.clone();
                tokio::spawn(async move {
                    if let Err(error) =
                        connection(stream, peer, &config, inbound, notices, stopped).await
                    {
                        tracing::debug!(%peer, %error, "management connection ended");
                    }
                });
            }
            _ = stopped.changed() => break,
        }
    }
}

async fn connection<L: Send + 'static>(
    mut stream: TcpStream,
    peer: SocketAddr,
    config: &ManagementConfig,
    inbound: Sender<Inbound<L>>,
    notices: broadcast::Receiver<ManagementNotice>,
    stopped: watch::Receiver<bool>,
) -> Result<(), ws::WsError> {
    let head = tokio::time::timeout(HANDSHAKE_TIMEOUT, ws::read_head(&mut stream))
        .await
        .map_err(|_| ws::WsError::BadRequest)??;
    if !head.is_upgrade() {
        ws::write_all(
            &mut stream,
            ws::rejection(400, "Bad Request", "Expected a WebSocket upgrade").as_bytes(),
        )
        .await?;
        return Ok(());
    }
    match authenticate(&head, config) {
        Verdict::Denied(reason) => {
            tracing::warn!("Authentication rejected for connection with ip {peer}: {reason}");
            ws::write_all(
                &mut stream,
                ws::rejection(401, "Unauthorized", reason).as_bytes(),
            )
            .await?;
            Ok(())
        }
        Verdict::Allowed { protocol } => {
            let key = head.header("sec-websocket-key").unwrap_or_default();
            ws::write_all(&mut stream, ws::upgrade_response(key, protocol).as_bytes()).await?;
            tracing::info!("Management connection opened for {peer}");
            let outcome = session(stream, inbound, notices, stopped).await;
            tracing::info!("Management connection closed for {peer}");
            outcome
        }
    }
}

async fn ask<L: Send + 'static>(
    inbound: &Sender<Inbound<L>>,
    request: rspigot_core::management::ManagementRequest,
) -> Option<ManagementReply> {
    let (reply, answer) = crossbeam_channel::bounded(1);
    inbound
        .send(Inbound::Management(ManagementCall { request, reply }))
        .ok()?;
    tokio::task::spawn_blocking(move || answer.recv_timeout(REPLY_TIMEOUT).ok())
        .await
        .ok()
        .flatten()
}

async fn answer<L: Send + 'static>(inbound: &Sender<Inbound<L>>, text: &str) -> Option<Value> {
    let call = match rpc::parse_call(text) {
        Ok(call) => call,
        Err(response) => return Some(response),
    };
    let id = call.id.clone();
    let shown = id.clone().unwrap_or(Value::Null);
    let outcome = if call.method == "rpc.discover" {
        Ok(rpc::discovery())
    } else {
        match rpc::to_request(&call.method, &call.params) {
            Err(error) => Err(error),
            Ok(request) => match ask(inbound, request).await {
                Some(reply) => rpc::reply_json(&reply),
                None => Err(rpc::RpcError {
                    code: rpc::INTERNAL_ERROR,
                    message: "Internal error".to_owned(),
                    data: Some("The server did not answer".to_owned()),
                }),
            },
        }
    };
    id?;
    Some(match outcome {
        Ok(result) => rpc::result_response(&shown, result),
        Err(error) => rpc::error_response(&shown, &error),
    })
}

async fn session<S, L>(
    stream: S,
    inbound: Sender<Inbound<L>>,
    mut notices: broadcast::Receiver<ManagementNotice>,
    mut stopped: watch::Receiver<bool>,
) -> Result<(), ws::WsError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    L: Send + 'static,
{
    let (mut reader, mut writer) = tokio::io::split(stream);
    let (frames, mut incoming) = tokio::sync::mpsc::channel::<Result<Frame, ws::WsError>>(8);
    tokio::spawn(async move {
        loop {
            let frame = ws::read_frame(&mut reader).await;
            let done = !matches!(frame, Ok(Frame::Text(_) | Frame::Ping(_) | Frame::Pong));
            if frames.send(frame).await.is_err() || done {
                break;
            }
        }
    });
    loop {
        tokio::select! {
            frame = incoming.recv() => match frame {
                Some(Ok(Frame::Text(text))) => {
                    if let Some(response) = answer(&inbound, &text).await {
                        ws::write_all(&mut writer, &ws::text_frame(&response.to_string())).await?;
                    }
                }
                Some(Ok(Frame::Ping(payload))) => {
                    ws::write_all(&mut writer, &ws::pong_frame(&payload)).await?;
                }
                Some(Ok(Frame::Pong)) => {}
                Some(Ok(Frame::Close)) | None => {
                    let _ = ws::write_all(&mut writer, &ws::close_frame()).await;
                    return Ok(());
                }
                Some(Err(error)) => {
                    let _ = ws::write_all(&mut writer, &ws::close_frame()).await;
                    return Err(error);
                }
            },
            notice = notices.recv() => match notice {
                Ok(notice) => {
                    let body = rpc::notification(&notice).to_string();
                    ws::write_all(&mut writer, &ws::text_frame(&body)).await?;
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => {}
            },
            _ = stopped.changed() => {
                let _ = ws::write_all(&mut writer, &ws::close_frame()).await;
                let _ = writer.shutdown().await;
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rspigot_core::management::{PlayerRef, StatusView};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    const SECRET: &str = "abcdefghijABCDEFGHIJ0123456789abcdefghij";

    fn config() -> ManagementConfig {
        ManagementConfig {
            host: "localhost".to_owned(),
            port: 0,
            secret: SECRET.to_owned(),
            tls_enabled: false,
            allowed_origins: vec!["http://panel.local".to_owned()],
        }
    }

    fn head(headers: &[(&str, &str)]) -> ws::Head {
        ws::Head {
            path: "/".to_owned(),
            headers: headers
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        }
    }

    #[test]
    fn the_secret_must_be_forty_alphanumerics() {
        assert!(config().validate().is_ok());
        let mut bad = config();
        bad.secret = "short".to_owned();
        assert!(matches!(
            bad.validate(),
            Err(ManagementError::InvalidSecret)
        ));
        let mut tls = config();
        tls.tls_enabled = true;
        assert!(matches!(
            tls.validate(),
            Err(ManagementError::TlsUnsupported)
        ));
        let mut public = config();
        public.host = "0.0.0.0".to_owned();
        assert!(matches!(
            public.validate(),
            Err(ManagementError::PublicWithoutTls)
        ));
    }

    #[test]
    fn a_bearer_header_or_the_subprotocol_authenticates() {
        let bearer = format!("Bearer {SECRET}");
        assert_eq!(
            authenticate(&head(&[("authorization", &bearer)]), &config()),
            Verdict::Allowed { protocol: None }
        );
        let protocol = format!("minecraft-v1,{SECRET}");
        assert_eq!(
            authenticate(&head(&[("sec-websocket-protocol", &protocol)]), &config()),
            Verdict::Allowed {
                protocol: Some(SUBPROTOCOL)
            }
        );
    }

    #[test]
    fn wrong_missing_and_foreign_origins_are_denied() {
        assert_eq!(
            authenticate(&head(&[]), &config()),
            Verdict::Denied("Missing API key")
        );
        assert_eq!(
            authenticate(&head(&[("authorization", "Bearer nope")]), &config()),
            Verdict::Denied("Invalid API key")
        );
        let bearer = format!("Bearer {SECRET}");
        assert_eq!(
            authenticate(
                &head(&[("authorization", &bearer), ("origin", "http://evil.test")]),
                &config()
            ),
            Verdict::Denied("Origin Not Allowed")
        );
        assert_eq!(
            authenticate(
                &head(&[("authorization", &bearer), ("origin", "http://panel.local")]),
                &config()
            ),
            Verdict::Allowed { protocol: None }
        );
    }

    fn masked(text: &str) -> Vec<u8> {
        let mask = [9u8, 8, 7, 6];
        let payload = text.as_bytes();
        let mut out = vec![0x81];
        if payload.len() < 126 {
            out.push(0x80 | payload.len() as u8);
        } else {
            out.push(0x80 | 126);
            out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        }
        out.extend_from_slice(&mask);
        out.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        out
    }

    async fn read_text<R: AsyncRead + Unpin>(reader: &mut R) -> Value {
        let mut head = [0u8; 2];
        reader.read_exact(&mut head).await.unwrap();
        let length = match head[1] {
            126 => {
                let mut raw = [0u8; 2];
                reader.read_exact(&mut raw).await.unwrap();
                usize::from(u16::from_be_bytes(raw))
            }
            n => usize::from(n),
        };
        let mut body = vec![0u8; length];
        reader.read_exact(&mut body).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn a_session_answers_requests_and_pushes_notifications() {
        let (client, server) = tokio::io::duplex(64 * 1024);
        let (inbound, game) = crossbeam_channel::unbounded::<Inbound<()>>();
        let (notices, _) = broadcast::channel(16);
        let (_stop, stopped) = watch::channel(false);
        std::thread::spawn(move || {
            for message in game {
                if let Inbound::Management(call) = message {
                    let _ = call.reply.send(ManagementReply::Status(StatusView {
                        started: true,
                        players: vec![PlayerRef::named("Bob")],
                        version_name: "26.3".to_owned(),
                        protocol: 777,
                    }));
                }
            }
        });
        let receiver = notices.subscribe();
        let task = tokio::spawn(session(server, inbound, receiver, stopped));
        let (mut reader, mut writer) = tokio::io::split(client);
        writer
            .write_all(&masked(
                r#"{"jsonrpc":"2.0","id":7,"method":"minecraft:server/status"}"#,
            ))
            .await
            .unwrap();
        let response = read_text(&mut reader).await;
        assert_eq!(response["id"], 7);
        assert_eq!(response["result"]["version"]["protocol"], 777);
        assert_eq!(response["result"]["players"][0]["name"], "Bob");
        notices.send(ManagementNotice::Saved).unwrap();
        let pushed = read_text(&mut reader).await;
        assert_eq!(pushed["method"], "minecraft:notification/server/saved");
        writer
            .write_all(&masked(
                r#"{"jsonrpc":"2.0","id":8,"method":"minecraft:nope"}"#,
            ))
            .await
            .unwrap();
        let error = read_text(&mut reader).await;
        assert_eq!(error["error"]["code"], rpc::METHOD_NOT_FOUND);
        writer
            .write_all(&masked(
                r#"{"jsonrpc":"2.0","method":"minecraft:server/status"}"#,
            ))
            .await
            .unwrap();
        writer.write_all(&masked("not json")).await.unwrap();
        let parse = read_text(&mut reader).await;
        assert_eq!(parse["error"]["code"], rpc::PARSE_ERROR);
        drop(writer);
        drop(reader);
        let _ = task.await;
    }
}
