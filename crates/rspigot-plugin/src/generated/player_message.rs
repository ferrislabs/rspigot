use rspigot_plugin_protocol::v1::{self, from_plugin};

use std::sync::Arc;

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct PlayerMessageHandle {
    pub(crate) target: Arc<str>,
    pub(crate) link: Link,
}

impl PlayerMessageHandle {
    #[allow(dead_code)]
    pub(crate) fn new(target: Arc<str>, link: Link) -> Self {
        Self { target, link }
    }

    pub fn send(&self, value: impl Into<String>) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.to_string(),
            order: Some(v1::player_order::Order::SendMessage(value.into())),
        }));
    }

    pub fn chat(&self, value: impl Into<String>) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.to_string(),
            order: Some(v1::player_order::Order::Chat(value.into())),
        }));
    }

    pub fn send_rich(&self, value: impl Into<v1::RichMessage>) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.to_string(),
            order: Some(v1::player_order::Order::SendRichMessage(value.into())),
        }));
    }
}
