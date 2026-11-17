use rspigot_plugin_protocol::v1::{self, from_plugin};

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct PlayerLocationHandle {
    pub(crate) target: String,
    pub(crate) link: Link,
}

impl PlayerLocationHandle {
    #[allow(dead_code)]
    pub(crate) fn new(target: String, link: Link) -> Self {
        Self { target, link }
    }

    pub fn teleport(&self, value: impl Into<v1::Vec3>) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.clone(),
            order: Some(v1::player_order::Order::Teleport(value.into())),
        }));
    }

    pub fn teleport_facing(&self, value: impl Into<v1::Facing>) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.clone(),
            order: Some(v1::player_order::Order::TeleportFacing(value.into())),
        }));
    }

    pub fn teleport_to(&self, value: impl Into<v1::Location>) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.clone(),
            order: Some(v1::player_order::Order::TeleportTo(value.into())),
        }));
    }
}
