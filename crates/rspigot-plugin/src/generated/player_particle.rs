use rspigot_plugin_protocol::v1::{self, from_plugin};

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct PlayerParticleHandle {
    pub(crate) target: String,
    pub(crate) link: Link,
}

impl PlayerParticleHandle {
    #[allow(dead_code)]
    pub(crate) fn new(target: String, link: Link) -> Self {
        Self { target, link }
    }

    pub fn spawn(&self, value: impl Into<v1::Particle>) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.clone(),
            order: Some(v1::player_order::Order::SpawnParticle(value.into())),
        }));
    }
}
