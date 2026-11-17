use rspigot_plugin_protocol::v1::{self, from_plugin};

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct PlayerHealthHandle {
    pub(crate) target: String,
    pub(crate) link: Link,
}

impl PlayerHealthHandle {
    #[allow(dead_code)]
    pub(crate) fn new(target: String, link: Link) -> Self {
        Self { target, link }
    }

    pub fn set(&self, value: f32) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.clone(),
            order: Some(v1::player_order::Order::SetHealth(value)),
        }));
    }

    pub fn set_absorption(&self, value: f32) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.clone(),
            order: Some(v1::player_order::Order::SetAbsorption(value)),
        }));
    }

    pub fn damage(&self, value: f64) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.clone(),
            order: Some(v1::player_order::Order::Damage(value)),
        }));
    }

    pub fn kill(&self) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.clone(),
            order: Some(v1::player_order::Order::Kill(v1::Empty {})),
        }));
    }

    pub fn set_fire_ticks(&self, value: i32) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.clone(),
            order: Some(v1::player_order::Order::SetFireTicks(value)),
        }));
    }

    pub fn respawn(&self) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.clone(),
            order: Some(v1::player_order::Order::Respawn(v1::Empty {})),
        }));
    }
}
