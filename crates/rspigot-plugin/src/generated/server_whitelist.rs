use rspigot_plugin_protocol::v1::{self, from_plugin};

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct ServerWhitelistHandle {
    pub(crate) link: Link,
}

impl ServerWhitelistHandle {
    #[allow(dead_code)]
    pub(crate) fn new(link: Link) -> Self {
        Self { link }
    }

    pub fn set_enabled(&self, value: bool) {
        self.link.order(from_plugin::Body::Server(v1::ServerOrder {
            order: Some(v1::server_order::Order::SetWhitelistEnabled(value)),
        }));
    }

    pub fn add(&self, value: impl Into<String>) {
        self.link.order(from_plugin::Body::Server(v1::ServerOrder {
            order: Some(v1::server_order::Order::WhitelistAdd(value.into())),
        }));
    }

    pub fn remove(&self, value: impl Into<String>) {
        self.link.order(from_plugin::Body::Server(v1::ServerOrder {
            order: Some(v1::server_order::Order::WhitelistRemove(value.into())),
        }));
    }
}
