use rspigot_plugin_protocol::v1::{self, from_plugin};

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct ServerOperatorsHandle {
    pub(crate) link: Link,
}

impl ServerOperatorsHandle {
    #[allow(dead_code)]
    pub(crate) fn new(link: Link) -> Self {
        Self { link }
    }

    pub fn add(&self, value: impl Into<String>) {
        self.link.order(from_plugin::Body::Server(v1::ServerOrder {
            order: Some(v1::server_order::Order::OperatorAdd(value.into())),
        }));
    }

    pub fn remove(&self, value: impl Into<String>) {
        self.link.order(from_plugin::Body::Server(v1::ServerOrder {
            order: Some(v1::server_order::Order::OperatorRemove(value.into())),
        }));
    }
}
