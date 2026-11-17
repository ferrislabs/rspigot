use rspigot_plugin_protocol::v1::{self, from_plugin};

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct WorldExplosionHandle {
    pub(crate) link: Link,
}

impl WorldExplosionHandle {
    #[allow(dead_code)]
    pub(crate) fn new(link: Link) -> Self {
        Self { link }
    }

    pub fn create(&self, value: impl Into<v1::Explode>) {
        self.link.order(from_plugin::Body::World(v1::WorldOrder {
            order: Some(v1::world_order::Order::Explode(value.into())),
        }));
    }
}
