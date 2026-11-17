use rspigot_plugin_protocol::v1::{self, from_plugin};

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct WorldDifficultyHandle {
    pub(crate) link: Link,
}

impl WorldDifficultyHandle {
    #[allow(dead_code)]
    pub(crate) fn new(link: Link) -> Self {
        Self { link }
    }

    pub fn set(&self, value: impl Into<v1::Difficulty>) {
        self.link.order(from_plugin::Body::World(v1::WorldOrder {
            order: Some(v1::world_order::Order::SetDifficulty(Into::<v1::Difficulty>::into(value) as i32)),
        }));
    }
}
