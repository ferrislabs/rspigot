use rspigot_plugin_protocol::v1::{self, from_plugin};

use std::sync::Arc;

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct PlayerRecipesHandle {
    pub(crate) target: Arc<str>,
    pub(crate) link: Link,
}

impl PlayerRecipesHandle {
    #[allow(dead_code)]
    pub(crate) fn new(target: Arc<str>, link: Link) -> Self {
        Self { target, link }
    }

    pub fn discover(&self, value: impl Into<String>) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.to_string(),
            order: Some(v1::player_order::Order::DiscoverRecipe(value.into())),
        }));
    }

    pub fn undiscover(&self, value: impl Into<String>) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.to_string(),
            order: Some(v1::player_order::Order::UndiscoverRecipe(value.into())),
        }));
    }
}
