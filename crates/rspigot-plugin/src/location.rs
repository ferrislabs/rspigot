use rspigot_plugin_protocol::v1::{self, from_plugin, player_order};

use crate::Error;
use crate::link::Link;

#[derive(Debug, Clone)]
pub struct PlayerLocationHandle {
    target: String,
    link: Link,
}

impl PlayerLocationHandle {
    pub(crate) const fn new(target: String, link: Link) -> Self {
        Self { target, link }
    }

    pub async fn teleport(&self, value: impl Into<v1::Vec3>) -> Result<(), Error> {
        self.send(player_order::Order::Teleport(value.into())).await
    }

    pub async fn teleport_facing(&self, value: impl Into<v1::Facing>) -> Result<(), Error> {
        self.send(player_order::Order::TeleportFacing(value.into()))
            .await
    }

    pub async fn teleport_to(&self, value: impl Into<v1::Location>) -> Result<(), Error> {
        self.send(player_order::Order::TeleportTo(value.into()))
            .await
    }

    async fn send(&self, order: player_order::Order) -> Result<(), Error> {
        self.link
            .act(from_plugin::Body::Player(v1::PlayerOrder {
                player: self.target.clone(),
                order: Some(order),
            }))
            .await
    }
}
