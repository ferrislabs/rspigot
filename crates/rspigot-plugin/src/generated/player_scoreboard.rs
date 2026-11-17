use rspigot_plugin_protocol::v1::{self, from_plugin};

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct PlayerScoreboardHandle {
    pub(crate) target: String,
    pub(crate) link: Link,
}

impl PlayerScoreboardHandle {
    #[allow(dead_code)]
    pub(crate) fn new(target: String, link: Link) -> Self {
        Self { target, link }
    }

    pub fn set(&self, value: impl Into<u32>) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.clone(),
            order: Some(v1::player_order::Order::SetScoreboard(value.into())),
        }));
    }
}
