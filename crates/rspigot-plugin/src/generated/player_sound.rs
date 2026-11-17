use rspigot_plugin_protocol::v1::{self, from_plugin};

use std::sync::Arc;

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct PlayerSoundHandle {
    pub(crate) target: Arc<str>,
    pub(crate) link: Link,
}

impl PlayerSoundHandle {
    #[allow(dead_code)]
    pub(crate) fn new(target: Arc<str>, link: Link) -> Self {
        Self { target, link }
    }

    pub fn play(&self, value: impl Into<v1::Sound>) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.to_string(),
            order: Some(v1::player_order::Order::PlaySound(value.into())),
        }));
    }
}
