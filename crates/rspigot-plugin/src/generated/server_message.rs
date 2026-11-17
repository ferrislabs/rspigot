use rspigot_plugin_protocol::v1::{self, from_plugin};

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct ServerMessageHandle {
    pub(crate) link: Link,
}

impl ServerMessageHandle {
    #[allow(dead_code)]
    pub(crate) fn new(link: Link) -> Self {
        Self { link }
    }

    pub fn broadcast(&self, value: impl Into<String>) {
        self.link.order(from_plugin::Body::Server(v1::ServerOrder {
            order: Some(v1::server_order::Order::Broadcast(value.into())),
        }));
    }
}
