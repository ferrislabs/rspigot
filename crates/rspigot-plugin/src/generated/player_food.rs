use rspigot_plugin_protocol::v1::{self, from_plugin};

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct PlayerFoodHandle {
    pub(crate) target: String,
    pub(crate) link: Link,
}

impl PlayerFoodHandle {
    #[allow(dead_code)]
    pub(crate) fn new(target: String, link: Link) -> Self {
        Self { target, link }
    }

    pub fn set_level(&self, value: i32) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.clone(),
            order: Some(v1::player_order::Order::SetFoodLevel(value)),
        }));
    }

    pub fn set_saturation(&self, value: f32) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.clone(),
            order: Some(v1::player_order::Order::SetSaturation(value)),
        }));
    }

    pub fn set_exhaustion(&self, value: f32) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.clone(),
            order: Some(v1::player_order::Order::SetExhaustion(value)),
        }));
    }
}
