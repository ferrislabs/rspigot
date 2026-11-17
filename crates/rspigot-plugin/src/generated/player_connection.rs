use rspigot_plugin_protocol::v1::{self, from_plugin};

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct PlayerConnectionHandle {
    pub(crate) target: String,
    pub(crate) link: Link,
}

impl PlayerConnectionHandle {
    #[allow(dead_code)]
    pub(crate) fn new(target: String, link: Link) -> Self {
        Self { target, link }
    }

    pub fn kick(&self, value: impl Into<String>) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.clone(),
            order: Some(v1::player_order::Order::Kick(value.into())),
        }));
    }

    pub fn ban(&self, value: impl Into<v1::BanTerms>) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.clone(),
            order: Some(v1::player_order::Order::Ban(value.into())),
        }));
    }
}
