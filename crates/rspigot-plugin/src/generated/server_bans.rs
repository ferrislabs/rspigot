use rspigot_plugin_protocol::v1::{self, from_plugin};

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct ServerBansHandle {
    pub(crate) link: Link,
}

impl ServerBansHandle {
    #[allow(dead_code)]
    pub(crate) fn new(link: Link) -> Self {
        Self { link }
    }

    pub fn add(&self, value: impl Into<v1::Ban>) {
        self.link.order(from_plugin::Body::Server(v1::ServerOrder {
            order: Some(v1::server_order::Order::BanAdd(value.into())),
        }));
    }

    pub fn remove(&self, value: impl Into<String>) {
        self.link.order(from_plugin::Body::Server(v1::ServerOrder {
            order: Some(v1::server_order::Order::BanRemove(value.into())),
        }));
    }
}
