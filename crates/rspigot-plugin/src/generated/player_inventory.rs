use rspigot_plugin_protocol::v1::{self, from_plugin};

use std::sync::Arc;

use crate::link::Link;

#[derive(Debug, Clone)]
pub struct PlayerInventoryHandle {
    pub(crate) target: Arc<str>,
    pub(crate) link: Link,
}

impl PlayerInventoryHandle {
    #[allow(dead_code)]
    pub(crate) fn new(target: Arc<str>, link: Link) -> Self {
        Self { target, link }
    }

    pub fn close(&self) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.to_string(),
            order: Some(v1::player_order::Order::CloseInventory(v1::Empty {})),
        }));
    }

    pub fn update(&self) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.to_string(),
            order: Some(v1::player_order::Order::UpdateInventory(v1::Empty {})),
        }));
    }

    pub fn add(&self, value: impl Into<v1::Item>) {
        self.link.order(from_plugin::Body::Inventory(v1::InventoryOrder {
            player: self.target.to_string(),
            order: Some(v1::inventory_order::Order::Add(value.into())),
        }));
    }

    pub fn set(&self, value: impl Into<v1::SetSlot>) {
        self.link.order(from_plugin::Body::Inventory(v1::InventoryOrder {
            player: self.target.to_string(),
            order: Some(v1::inventory_order::Order::Set(value.into())),
        }));
    }

    pub fn remove(&self, value: impl Into<v1::RemoveItems>) {
        self.link.order(from_plugin::Body::Inventory(v1::InventoryOrder {
            player: self.target.to_string(),
            order: Some(v1::inventory_order::Order::Remove(value.into())),
        }));
    }

    pub fn clear(&self) {
        self.link.order(from_plugin::Body::Inventory(v1::InventoryOrder {
            player: self.target.to_string(),
            order: Some(v1::inventory_order::Order::Clear(v1::Empty {})),
        }));
    }

    pub fn set_held_slot(&self, value: i32) {
        self.link.order(from_plugin::Body::Inventory(v1::InventoryOrder {
            player: self.target.to_string(),
            order: Some(v1::inventory_order::Order::SetHeldSlot(value)),
        }));
    }
}
