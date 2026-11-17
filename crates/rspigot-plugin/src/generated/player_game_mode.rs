use rspigot_plugin_protocol::v1::{self, from_plugin};

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct PlayerGameModeHandle {
    pub(crate) target: String,
    pub(crate) link: Link,
}

impl PlayerGameModeHandle {
    #[allow(dead_code)]
    pub(crate) fn new(target: String, link: Link) -> Self {
        Self { target, link }
    }

    pub fn set(&self, value: impl Into<v1::GameMode>) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.clone(),
            order: Some(v1::player_order::Order::SetGameMode(Into::<v1::GameMode>::into(value) as i32)),
        }));
    }
}
