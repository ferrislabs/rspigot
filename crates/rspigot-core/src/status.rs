use std::net::IpAddr;
use std::sync::Arc;

use parking_lot::RwLock;
use rspigot_domain::player::PlayerId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerSample {
    pub id: PlayerId,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PingHook {
    #[default]
    Off,
    Notify,
    Ask,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StatusSnapshot {
    pub online: u32,
    pub max_players: u32,
    pub sample: Vec<PlayerSample>,
    pub ping: PingHook,
    pub motd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusPing {
    pub address: IpAddr,
    pub hostname: String,
    pub motd: String,
    pub legacy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusReply {
    pub motd: String,
    pub max_players: i32,
    pub online: i32,
    pub sample: Vec<PlayerSample>,
    pub icon: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Default)]
pub struct StatusBoard(Arc<RwLock<StatusSnapshot>>);

impl StatusBoard {
    pub fn new(max_players: u32) -> Self {
        Self(Arc::new(RwLock::new(StatusSnapshot {
            max_players,
            ..StatusSnapshot::default()
        })))
    }

    pub fn snapshot(&self) -> StatusSnapshot {
        self.0.read().clone()
    }

    pub fn update(&self, update: impl FnOnce(&mut StatusSnapshot)) {
        update(&mut self.0.write());
    }
}
