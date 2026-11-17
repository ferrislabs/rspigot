use rspigot_plugin_protocol::v1::{self, from_plugin};

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct ServerLifecycleHandle {
    pub(crate) link: Link,
}

impl ServerLifecycleHandle {
    #[allow(dead_code)]
    pub(crate) fn new(link: Link) -> Self {
        Self { link }
    }

    pub fn shutdown(&self) {
        self.link.order(from_plugin::Body::Server(v1::ServerOrder {
            order: Some(v1::server_order::Order::Shutdown(v1::Empty {})),
        }));
    }

    pub fn save(&self) {
        self.link.order(from_plugin::Body::Server(v1::ServerOrder {
            order: Some(v1::server_order::Order::Save(v1::Empty {})),
        }));
    }
}
