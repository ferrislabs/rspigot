use rspigot_plugin_protocol::v1::{self, from_plugin};

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct WorldStorageHandle {
    pub(crate) link: Link,
}

impl WorldStorageHandle {
    #[allow(dead_code)]
    pub(crate) fn new(link: Link) -> Self {
        Self { link }
    }

    pub fn save(&self) {
        self.link.order(from_plugin::Body::World(v1::WorldOrder {
            order: Some(v1::world_order::Order::Save(v1::Empty {})),
        }));
    }
}
