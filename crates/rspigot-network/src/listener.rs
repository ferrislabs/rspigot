use std::io;
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use crossbeam_channel::Sender;
use parking_lot::Mutex;
use rspigot_core::events::Inbound;
use rspigot_core::ports::SessionVerifier;
use rspigot_core::status::StatusBoard;
use rspigot_protocol::crypto::ServerKey;
use rustc_hash::FxHashMap;
use tokio::net::TcpListener;
use tokio::sync::watch;
use uuid::Uuid;

use crate::NetworkConfig;

const SHARED_FRAMES_BYTES: u64 = 16 * 1024 * 1024;
const SHARED_FRAMES_IDLE: std::time::Duration = std::time::Duration::from_secs(2);
use crate::cache::{ChunkPacketCache, SharedFrameCache};
use crate::connection;
use crate::link::NetLink;

pub struct Shared<V> {
    pub config: NetworkConfig,
    pub verifier: Arc<V>,
    pub status: StatusBoard,
    pub game: Sender<Inbound<NetLink>>,
    pub key: ServerKey,
    pub session_id: Uuid,
    pub chunks: ChunkPacketCache,
    pub frames: SharedFrameCache,
    throttle: Mutex<FxHashMap<IpAddr, Instant>>,
    sessions: AtomicU64,
}

impl<V> Shared<V> {
    pub fn next_session(&self) -> u64 {
        self.sessions.fetch_add(1, Ordering::Relaxed)
    }

    pub fn throttled(&self, ip: IpAddr) -> bool {
        let window = self.config.connection_throttle;
        if window.is_zero() || ip.is_loopback() {
            return false;
        }
        let now = Instant::now();
        let mut throttle = self.throttle.lock();
        throttle.retain(|_, at| now.duration_since(*at) < window);
        let blocked = throttle.contains_key(&ip);
        throttle.insert(ip, now);
        blocked
    }
}

pub async fn run<V: SessionVerifier>(
    config: NetworkConfig,
    verifier: Arc<V>,
    status: StatusBoard,
    game: Sender<Inbound<NetLink>>,
    shutdown: watch::Receiver<bool>,
) -> io::Result<()> {
    let listener = TcpListener::bind(config.bind).await?;
    serve(listener, config, verifier, status, game, shutdown).await
}

pub async fn serve<V: SessionVerifier>(
    listener: TcpListener,
    config: NetworkConfig,
    verifier: Arc<V>,
    status: StatusBoard,
    game: Sender<Inbound<NetLink>>,
    mut shutdown: watch::Receiver<bool>,
) -> io::Result<()> {
    let key = ServerKey::generate().map_err(|e| io::Error::other(e.to_string()))?;
    tracing::info!("Starting Minecraft server on {}", config.bind);
    if !config.online_mode {
        tracing::warn!("**** SERVER IS RUNNING IN OFFLINE/INSECURE MODE!");
        tracing::warn!("The server will make no attempt to authenticate usernames. Beware.");
    }
    let shared = Arc::new(Shared {
        chunks: ChunkPacketCache::new(config.chunk_cache_bytes, config.chunk_cache_idle),
        frames: SharedFrameCache::new(SHARED_FRAMES_BYTES, SHARED_FRAMES_IDLE),
        config,
        verifier,
        status,
        game,
        key,
        session_id: Uuid::new_v4(),
        throttle: Mutex::new(FxHashMap::default()),
        sessions: AtomicU64::new(1),
    });
    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, addr)) => {
                    let _ = stream.set_nodelay(true);
                    let shared = Arc::clone(&shared);
                    tokio::spawn(async move {
                        if let Err(error) = connection::handle(shared, stream, addr).await {
                            tracing::debug!(%addr, %error, "connection closed");
                        }
                    });
                }
                Err(error) => tracing::warn!(%error, "accept failed"),
            },
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return Ok(());
                }
            }
        }
    }
}
