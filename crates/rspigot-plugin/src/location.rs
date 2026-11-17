use std::sync::Arc;

use rspigot_plugin_protocol::v1::{self, from_plugin, player_order};

use crate::Error;
use crate::link::Link;
use crate::world::World;

#[derive(Debug, Clone)]
pub struct PlayerLocationHandle {
    target: Arc<str>,
    link: Link,
}

impl PlayerLocationHandle {
    pub(crate) const fn new(target: Arc<str>, link: Link) -> Self {
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

    pub async fn teleport_to_world(&self, world: &World) -> Result<(), Error> {
        let spawn = world.get_state().await?.spawn;
        self.teleport_to(v1::Location {
            world: world.id().to_owned(),
            position: Some(v1::Vec3 {
                x: f64::from(spawn.x) + 0.5,
                y: f64::from(spawn.y),
                z: f64::from(spawn.z) + 0.5,
            }),
            rotation: None,
        })
        .await
    }

    async fn send(&self, order: player_order::Order) -> Result<(), Error> {
        self.link
            .act(from_plugin::Body::Player(v1::PlayerOrder {
                player: self.target.to_string(),
                order: Some(order),
            }))
            .await
    }
}
