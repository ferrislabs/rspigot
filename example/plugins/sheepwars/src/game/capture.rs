use rspigot_plugin::{InventorySlot, InventoryState, Material, Particle, Sound};

use crate::domain::SheepKind;

pub const CAPTURE_SOUND: Sound = Sound::EntityItemPickup;
pub const CAPTURE_PARTICLE: Particle = Particle::Cloud;
pub const CAPTURE_PARTICLES: i32 = 8;
pub const STORAGE_SLOTS: usize = 36;
pub const MAX_STACK: u8 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capture {
    Denied,
    Fragment,
    Taken { kind: SheepKind },
}

pub fn extract<T>(items: &mut Vec<T>, matches: impl FnMut(&T) -> bool) -> Option<T> {
    let index = items.iter().position(matches)?;
    Some(items.remove(index))
}

pub fn has_room(inventory: &InventoryState, material: Material) -> bool {
    let storage: Vec<_> = inventory
        .slots
        .iter()
        .filter(|(slot, _)| matches!(slot, InventorySlot::Hotbar(_) | InventorySlot::Main(_)))
        .collect();
    storage.len() < STORAGE_SLOTS
        || storage.iter().any(|(_, item)| match item {
            None => true,
            Some(item) => item.material == material && item.count < MAX_STACK,
        })
}

#[cfg(test)]
mod tests {
    use rspigot_plugin::ItemStack;

    use super::*;

    fn inventory(slots: Vec<(InventorySlot, Option<ItemStack>)>) -> InventoryState {
        InventoryState {
            slots,
            held_slot: 0,
        }
    }

    fn full(material: Material, count: u8) -> InventoryState {
        let hotbar = (0..9u8).map(InventorySlot::Hotbar);
        let main = (0..27u8).map(InventorySlot::Main);
        inventory(
            hotbar
                .chain(main)
                .map(|slot| (slot, Some(ItemStack::new(material, count))))
                .collect(),
        )
    }

    #[test]
    fn an_empty_listing_has_room() {
        assert!(has_room(&inventory(Vec::new()), Material::RedWool));
    }

    #[test]
    fn a_full_inventory_has_no_room() {
        assert!(!has_room(&full(Material::Stone, 1), Material::RedWool));
    }

    #[test]
    fn a_full_inventory_with_an_open_matching_stack_has_room() {
        assert!(has_room(&full(Material::RedWool, 3), Material::RedWool));
        assert!(!has_room(
            &full(Material::RedWool, MAX_STACK),
            Material::RedWool
        ));
    }

    #[test]
    fn a_listed_empty_slot_has_room() {
        let mut state = full(Material::Stone, 1);
        state.slots[4].1 = None;
        assert!(has_room(&state, Material::RedWool));
    }

    #[test]
    fn extract_removes_the_matching_item_only() {
        let mut items = vec![1, 2, 3];
        assert_eq!(extract(&mut items, |item| *item == 2), Some(2));
        assert_eq!(items, vec![1, 3]);
        assert_eq!(extract(&mut items, |item| *item == 9), None);
        assert_eq!(items, vec![1, 3]);
    }
}
