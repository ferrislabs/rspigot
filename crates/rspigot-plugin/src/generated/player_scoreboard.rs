use rspigot_plugin_protocol::v1::{self, from_plugin};

use std::sync::Arc;

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct PlayerScoreboardHandle {
    pub(crate) target: Arc<str>,
    pub(crate) link: Link,
}

impl PlayerScoreboardHandle {
    #[allow(dead_code)]
    pub(crate) fn new(target: Arc<str>, link: Link) -> Self {
        Self { target, link }
    }

    pub fn set(&self, value: impl Into<u32>) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.to_string(),
            order: Some(v1::player_order::Order::SetScoreboard(value.into())),
        }));
    }
}
