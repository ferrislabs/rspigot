mod backend;
mod client;
pub mod forwarding;
mod session;
mod shared;
mod status;
pub mod text;
mod wire;

use std::sync::Arc;
use std::time::Duration;

use rspigot_core::ports::SessionVerifier;
use rspigot_protocol::codec::DecodeError;
use rspigot_protocol::crypto::CryptoError;
use rspigot_protocol::frame::FrameError;
use rspigot_protocol::types::JsonText;
use rspigot_proxy_core::ProxyHooks;
use tokio::net::TcpListener;

pub use crate::shared::{Shared, State};

#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("frame: {0}")]
    Frame(#[from] FrameError),
    #[error("decode: {0}")]
    Decode(#[from] DecodeError),
    #[error("crypto: {0}")]
    Crypto(#[from] CryptoError),
    #[error("connection closed")]
    Closed,
    #[error("the connection to the backend was lost")]
    BackendLost,
    #[error("timed out")]
    Timeout,
    #[error("{0}")]
    Rejected(String),
    #[error("the backend refused the player: {}", .0.0)]
    BackendRefused(JsonText),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxySettings {
    pub online_mode: bool,
    pub motd: String,
    pub max_players: u32,
    pub favicon: Option<String>,
    pub compression_threshold: Option<usize>,
    pub compression_level: u32,
    pub forwarding_secret: Vec<u8>,
    pub login_timeout: Duration,
    pub connect_timeout: Duration,
}

pub async fn serve<V: SessionVerifier, H: ProxyHooks>(
    listener: TcpListener,
    shared: Arc<Shared<V, H>>,
) -> Result<(), ProxyError> {
    loop {
        let (stream, peer) = listener.accept().await?;
        let shared = Arc::clone(&shared);
        tokio::spawn(async move {
            if let Err(error) = client::handle(stream, peer, shared).await {
                tracing::debug!("connection from {peer} ended: {error}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn settings() -> ProxySettings {
        ProxySettings {
            online_mode: false,
            motd: "A proxy".to_owned(),
            max_players: 100,
            favicon: None,
            compression_threshold: Some(64),
            compression_level: 6,
            forwarding_secret: b"secret".to_vec(),
            login_timeout: Duration::from_secs(5),
            connect_timeout: Duration::from_secs(5),
        }
    }
}
