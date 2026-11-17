use rspigot_plugin_protocol::v1::{self, from_plugin};

use std::sync::Arc;

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct PlayerAbilitiesHandle {
    pub(crate) target: Arc<str>,
    pub(crate) link: Link,
}

impl PlayerAbilitiesHandle {
    #[allow(dead_code)]
    pub(crate) fn new(target: Arc<str>, link: Link) -> Self {
        Self { target, link }
    }

    pub fn set_allow_flight(&self, value: bool) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.to_string(),
            order: Some(v1::player_order::Order::SetAllowFlight(value)),
        }));
    }

    pub fn set_flying(&self, value: bool) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.to_string(),
            order: Some(v1::player_order::Order::SetFlying(value)),
        }));
    }

    pub fn set_fly_speed(&self, value: f32) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.to_string(),
            order: Some(v1::player_order::Order::SetFlySpeed(value)),
        }));
    }

    pub fn set_walk_speed(&self, value: f32) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.to_string(),
            order: Some(v1::player_order::Order::SetWalkSpeed(value)),
        }));
    }
}
