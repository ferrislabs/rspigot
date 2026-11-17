use rspigot_plugin_protocol::v1::{self, from_plugin};

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct WorldTimeHandle {
    pub(crate) link: Link,
}

impl WorldTimeHandle {
    #[allow(dead_code)]
    pub(crate) fn new(link: Link) -> Self {
        Self { link }
    }

    pub fn set(&self, value: i64) {
        self.link.order(from_plugin::Body::World(v1::WorldOrder {
            order: Some(v1::world_order::Order::SetTime(value)),
        }));
    }
}
