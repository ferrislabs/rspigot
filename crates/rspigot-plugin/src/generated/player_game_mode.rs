use rspigot_plugin_protocol::v1::{self, from_plugin};

use std::sync::Arc;

use crate::link::Link;
use crate::GameMode;

#[derive(Debug, Clone)]
pub struct PlayerGameModeHandle {
    pub(crate) target: Arc<str>,
    pub(crate) link: Link,
    pub(crate) value: GameMode,
}

impl PlayerGameModeHandle {
    #[allow(dead_code)]
    pub(crate) fn new(target: Arc<str>, link: Link, value: GameMode) -> Self {
        Self { target, link, value }
    }

    pub const fn value(&self) -> GameMode {
        self.value
    }

    pub fn set(&self, value: impl Into<v1::GameMode>) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.to_string(),
            order: Some(v1::player_order::Order::SetGameMode(Into::<v1::GameMode>::into(value) as i32)),
        }));
    }
}
