use rspigot_plugin_protocol::v1::{self, from_plugin};

use std::sync::Arc;

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct PlayerEffectsHandle {
    pub(crate) target: Arc<str>,
    pub(crate) link: Link,
}

impl PlayerEffectsHandle {
    #[allow(dead_code)]
    pub(crate) fn new(target: Arc<str>, link: Link) -> Self {
        Self { target, link }
    }

    pub fn add(&self, value: impl Into<v1::Effect>) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.to_string(),
            order: Some(v1::player_order::Order::AddEffect(value.into())),
        }));
    }

    pub fn remove(&self, value: impl Into<String>) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.to_string(),
            order: Some(v1::player_order::Order::RemoveEffect(value.into())),
        }));
    }

    pub fn clear(&self) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.to_string(),
            order: Some(v1::player_order::Order::ClearEffects(v1::Empty {})),
        }));
    }
}
