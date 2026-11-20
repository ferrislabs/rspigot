use std::sync::atomic::{AtomicU64, Ordering};

use rspigot_plugin_protocol::v1::{
    self, answer, custom_inventory_order, from_plugin, question, server_order,
};

use crate::link::Link;
use crate::{Error, ItemStack, Player};

static NEXT_INVENTORY: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub struct CustomInventoryState {
    pub items: Vec<Option<ItemStack>>,
    pub viewers: Vec<Player>,
}

impl CustomInventoryState {
    fn from_wire(state: v1::CustomInventoryState, link: &Link) -> Result<Self, Error> {
        let items = (0..)
            .zip(state.slots)
            .map(|(index, slot)| {
                if slot.slot != index {
                    return Err(Error::Malformed(format!(
                        "slot {} where slot {index} was expected",
                        slot.slot
                    )));
                }
                slot.item.map(ItemStack::from_wire).transpose()
            })
            .collect::<Result<_, Error>>()?;
        let viewers = state
            .viewers
            .into_iter()
            .map(|viewer| Player::from_wire(Some(viewer), link))
            .collect::<Result<_, Error>>()?;
        Ok(Self { items, viewers })
    }
}

#[derive(Debug, Clone)]
pub struct CustomInventory {
    pub id: u64,
    pub plugin: String,
    generation: u64,
    link: Link,
}

impl PartialEq for CustomInventory {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.plugin == other.plugin && self.generation == other.generation
    }
}

impl Eq for CustomInventory {}

impl CustomInventory {
    pub(crate) async fn create(
        link: &Link,
        rows: u8,
        title: String,
        locked: bool,
    ) -> Result<Self, Error> {
        let inventory = Self::new(NEXT_INVENTORY.fetch_add(1, Ordering::Relaxed), link);
        link.act_from(
            inventory.generation,
            from_plugin::Body::Server(v1::ServerOrder {
                order: Some(server_order::Order::CreateInventory(v1::CreateInventory {
                    id: inventory.id,
                    rows: i32::from(rows),
                    title,
                    locked,
                })),
            }),
        )
        .await?;
        Ok(inventory)
    }

    pub(crate) fn new(id: u64, link: &Link) -> Self {
        Self::owned_by(id, link.name(), link)
    }

    pub(crate) fn owned_by(id: u64, plugin: String, link: &Link) -> Self {
        Self {
            id,
            plugin,
            generation: link.generation(),
            link: link.clone(),
        }
    }

    pub fn is_mine(&self) -> bool {
        self.plugin == self.link.name()
    }

    pub(crate) const fn generation(&self) -> u64 {
        self.generation
    }

    pub fn set(&self, slot: u16, item: Option<ItemStack>) {
        self.order(custom_inventory_order::Order::Set(v1::SetSlot {
            slot: i32::from(slot),
            item: item.map(ItemStack::into_wire),
        }));
    }

    pub fn clear(&self) {
        self.order(custom_inventory_order::Order::Clear(v1::Empty {}));
    }

    pub fn close(&self) {
        self.order(custom_inventory_order::Order::Close(v1::Empty {}));
    }

    pub fn discard(&self) {
        self.order(custom_inventory_order::Order::Discard(v1::Empty {}));
    }

    pub async fn get_state(&self) -> Result<CustomInventoryState, Error> {
        if self.link.generation() != self.generation || !self.is_mine() {
            return Err(Error::NotFound);
        }
        let state = self
            .link
            .ask(
                question::Question::CustomInventory(self.id),
                "a custom inventory",
                |body| match body {
                    answer::Body::CustomInventory(state) => Some(state),
                    _ => None,
                },
            )
            .await?;
        CustomInventoryState::from_wire(state, &self.link)
    }

    fn order(&self, order: custom_inventory_order::Order) {
        if !self.is_mine() {
            tracing::warn!(
                "Dropped an order on inventory {} of plugin {}",
                self.id,
                self.plugin
            );
            return;
        }
        self.link.order_from(
            self.generation,
            from_plugin::Body::CustomInventory(v1::CustomInventoryOrder {
                inventory: self.id,
                order: Some(order),
            }),
        );
    }
}
