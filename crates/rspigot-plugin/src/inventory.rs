use rspigot_plugin_protocol::v1::{self, answer, from_plugin, player_order, question};

use crate::{CustomInventory, Error, ItemStack, PlayerInventoryHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArmorSlot {
    Helmet,
    Chestplate,
    Leggings,
    Boots,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InventorySlot {
    Hotbar(u8),
    Main(u8),
    Armor(ArmorSlot),
    Offhand,
}

impl InventorySlot {
    pub const fn index(self) -> Option<i32> {
        match self {
            Self::Hotbar(slot) if slot < 9 => Some(slot as i32),
            Self::Main(slot) if slot < 27 => Some(9 + slot as i32),
            Self::Hotbar(_) | Self::Main(_) => None,
            Self::Armor(ArmorSlot::Boots) => Some(36),
            Self::Armor(ArmorSlot::Leggings) => Some(37),
            Self::Armor(ArmorSlot::Chestplate) => Some(38),
            Self::Armor(ArmorSlot::Helmet) => Some(39),
            Self::Offhand => Some(40),
        }
    }

    pub const fn from_index(index: i32) -> Option<Self> {
        match index {
            0..=8 => Some(Self::Hotbar(index as u8)),
            9..=35 => Some(Self::Main((index - 9) as u8)),
            36 => Some(Self::Armor(ArmorSlot::Boots)),
            37 => Some(Self::Armor(ArmorSlot::Leggings)),
            38 => Some(Self::Armor(ArmorSlot::Chestplate)),
            39 => Some(Self::Armor(ArmorSlot::Helmet)),
            40 => Some(Self::Offhand),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryState {
    pub slots: Vec<(InventorySlot, Option<ItemStack>)>,
    pub held_slot: u8,
}

impl InventoryState {
    fn from_wire(state: v1::InventoryState) -> Result<Self, Error> {
        let slots = state
            .slots
            .into_iter()
            .map(|slot| {
                let index = InventorySlot::from_index(slot.slot)
                    .ok_or_else(|| Error::Malformed(format!("an unknown slot {}", slot.slot)))?;
                Ok((index, slot.item.map(ItemStack::from_wire).transpose()?))
            })
            .collect::<Result<_, Error>>()?;
        let held_slot = u8::try_from(state.held_slot)
            .ok()
            .filter(|slot| *slot < 9)
            .ok_or_else(|| Error::Malformed(format!("an unknown held slot {}", state.held_slot)))?;
        Ok(Self { slots, held_slot })
    }
}

impl PlayerInventoryHandle {
    pub fn open(&self, inventory: &CustomInventory) {
        self.link.order_from(
            inventory.generation(),
            from_plugin::Body::Player(v1::PlayerOrder {
                player: self.target.clone(),
                order: Some(player_order::Order::OpenInventory(inventory.id())),
            }),
        );
    }

    pub async fn resolve(&self) -> Result<InventoryState, Error> {
        let state = self
            .link
            .ask(
                question::Question::Inventory(self.target.clone()),
                "an inventory",
                |body| match body {
                    answer::Body::Inventory(state) => Some(state),
                    _ => None,
                },
            )
            .await?;
        InventoryState::from_wire(state)
    }
}

#[cfg(test)]
mod tests {
    use super::InventorySlot;

    #[test]
    fn slots_round_trip_through_their_index() {
        for index in 0..=40 {
            assert_eq!(
                InventorySlot::from_index(index).unwrap().index(),
                Some(index)
            );
        }
        assert_eq!(InventorySlot::from_index(41), None);
    }
}
