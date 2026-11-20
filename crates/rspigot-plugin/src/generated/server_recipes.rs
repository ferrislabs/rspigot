use rspigot_plugin_protocol::v1::{self, from_plugin};

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct ServerRecipesHandle {
    pub(crate) link: Link,
}

impl ServerRecipesHandle {
    #[allow(dead_code)]
    pub(crate) fn new(link: Link) -> Self {
        Self { link }
    }

    pub fn remove(&self, value: impl Into<String>) {
        self.link.order(from_plugin::Body::Server(v1::ServerOrder {
            order: Some(v1::server_order::Order::RemoveRecipe(value.into())),
        }));
    }
}
