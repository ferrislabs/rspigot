use rspigot_plugin_protocol::v1::{self, from_plugin};

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct ServerSettingsHandle {
    pub(crate) link: Link,
}

impl ServerSettingsHandle {
    #[allow(dead_code)]
    pub(crate) fn new(link: Link) -> Self {
        Self { link }
    }

    pub fn set_motd(&self, value: impl Into<String>) {
        self.link.order(from_plugin::Body::Server(v1::ServerOrder {
            order: Some(v1::server_order::Order::SetMotd(value.into())),
        }));
    }

    pub fn set_max_players(&self, value: i32) {
        self.link.order(from_plugin::Body::Server(v1::ServerOrder {
            order: Some(v1::server_order::Order::SetMaxPlayers(value)),
        }));
    }
}
