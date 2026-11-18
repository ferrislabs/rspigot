use rspigot_plugin_protocol::v1::{self, from_plugin};

use std::sync::Arc;

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct PlayerDialogHandle {
    pub(crate) target: Arc<str>,
    pub(crate) link: Link,
}

impl PlayerDialogHandle {
    #[allow(dead_code)]
    pub(crate) fn new(target: Arc<str>, link: Link) -> Self {
        Self { target, link }
    }

    pub fn show(&self, value: impl Into<v1::Dialog>) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.to_string(),
            order: Some(v1::player_order::Order::ShowDialog(value.into())),
        }));
    }

    pub fn clear(&self) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.to_string(),
            order: Some(v1::player_order::Order::ClearDialog(v1::Empty {})),
        }));
    }
}
