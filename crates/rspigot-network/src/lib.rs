mod book;
mod cache;
mod components;
mod connection;
mod decode;
mod encode;
mod extras;
mod link;
mod listener;
mod play;
mod proxy;
mod query;
mod rate_limit;
mod signature_cache;
mod status;
mod text;

use std::net::SocketAddr;
use std::time::Duration;

pub use cache::ChunkPacketCache;
pub use link::NetLink;
pub use listener::{run, serve};
pub use proxy::{ProxyForwarding, hmac_sha256};
pub use query::{QueryConfig, serve_query};

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub bind: SocketAddr,
    pub online_mode: bool,
    pub prevent_proxy_connections: bool,
    pub compression_threshold: Option<usize>,
    pub compression_level: u32,
    pub motd: String,
    pub favicon: Option<String>,
    pub version_name: String,
    pub hide_online_players: bool,
    pub enable_status: bool,
    pub enforce_secure_profile: bool,
    pub accepts_transfers: bool,
    pub brand: String,
    pub keep_alive_interval: Duration,
    pub keep_alive_timeout: Duration,
    pub connection_throttle: Duration,
    pub outdated_client_message: String,
    pub outdated_server_message: String,
    pub code_of_conduct: Option<String>,
    pub rate_limit: u32,
    pub resource_pack: Option<rspigot_core::client_extras::ResourcePackOffer>,
    pub server_links: Vec<rspigot_core::client_extras::ServerLink>,
    pub chunk_cache_bytes: u64,
    pub proxy: ProxyForwarding,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([0, 0, 0, 0], 25565)),
            online_mode: true,
            prevent_proxy_connections: false,
            compression_threshold: Some(256),
            compression_level: 4,
            motd: "A Minecraft Server".to_owned(),
            favicon: None,
            version_name: format!("rspigot {}", rspigot_data::VERSION_NAME),
            hide_online_players: false,
            enable_status: true,
            enforce_secure_profile: false,
            accepts_transfers: false,
            brand: "Rspigot".to_owned(),
            keep_alive_interval: Duration::from_secs(15),
            keep_alive_timeout: Duration::from_secs(30),
            connection_throttle: Duration::from_millis(4000),
            outdated_client_message: "Outdated client! Please use {0}".to_owned(),
            outdated_server_message: "Outdated server! I'm still on {0}".to_owned(),
            code_of_conduct: None,
            rate_limit: 0,
            resource_pack: None,
            server_links: Vec::new(),
            chunk_cache_bytes: 256 * 1024 * 1024,
            proxy: ProxyForwarding::Off,
        }
    }
}
