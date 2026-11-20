use rspigot_domain::entity::living;
use rspigot_domain::entity::species::equine;
use rspigot_domain::item::{HOTBAR_START, ItemStack, OFFHAND_SLOT, Slot};
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, EntityId};
use rspigot_items::EquipmentSlot;

use super::dispatch::Target;
use super::slots::{
    BODY, CHEST, CRAFTING_BASE, CURSOR, ENDER_BASE, FEET, HEAD, HORSE_BASE, LEGS, MAINHAND,
    OFFHAND, SADDLE, SlotHolder,
};
use crate::entities::wire_slot;
use crate::events::ClientEvent;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const ENDER_SLOTS: i32 = 27;
const CRAFTING_SLOTS: i32 = 4;
const HORSE_SLOTS: i32 = 15;

fn equipment_of(id: i32) -> Option<EquipmentSlot> {
    match id {
        MAINHAND => Some(EquipmentSlot::Mainhand),
        OFFHAND => Some(EquipmentSlot::Offhand),
        FEET => Some(EquipmentSlot::Feet),
        LEGS => Some(EquipmentSlot::Legs),
        CHEST => Some(EquipmentSlot::Chest),
        HEAD => Some(EquipmentSlot::Head),
        BODY => Some(EquipmentSlot::Body),
        SADDLE => Some(EquipmentSlot::Saddle),
        _ => None,
    }
}

fn player_menu_slot(selected: u8, id: i32) -> Option<usize> {
    match id {
        0..=8 => Some(HOTBAR_START + id as usize),
        9..=35 => Some(id as usize),
        MAINHAND => Some(HOTBAR_START + usize::from(selected)),
        OFFHAND => Some(OFFHAND_SLOT),
        FEET => Some(8),
        LEGS => Some(7),
        CHEST => Some(6),
        HEAD => Some(5),
        _ if (CRAFTING_BASE..CRAFTING_BASE + CRAFTING_SLOTS).contains(&id) => {
            Some(1 + (id - CRAFTING_BASE) as usize)
        }
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn read_command_slot(&self, holder: SlotHolder, id: i32) -> Option<Slot> {
        match holder {
            SlotHolder::Entity(Target::Player(player)) => self.read_player_slot(player, id),
            SlotHolder::Entity(Target::Entity(entity)) => self.read_entity_slot(entity, id),
            SlotHolder::Block(pos) => self.read_block_slot(pos, id),
        }
    }

    pub(crate) fn write_command_slot(&mut self, holder: SlotHolder, id: i32, stack: Slot) -> bool {
        if self.read_command_slot(holder, id).is_none() {
            return false;
        }
        match holder {
            SlotHolder::Entity(Target::Player(player)) => self.write_player_slot(player, id, stack),
            SlotHolder::Entity(Target::Entity(entity)) => self.write_entity_slot(entity, id, stack),
            SlotHolder::Block(pos) => self.write_block_slot(pos, id, stack),
        }
        true
    }

    pub(crate) fn command_slot_count(&self, holder: SlotHolder) -> usize {
        match holder {
            SlotHolder::Block(pos) => self.storage_backing(pos).map_or(0, |(_, size)| size),
            SlotHolder::Entity(_) => 0,
        }
    }

    fn read_player_slot(&self, player: PlayerId, id: i32) -> Option<Slot> {
        let state = self.players.get(&player)?;
        if id == CURSOR {
            return Some(state.menu.carried().clone());
        }
        if (ENDER_BASE..ENDER_BASE + ENDER_SLOTS).contains(&id) {
            let stack = state.ender_items.get((id - ENDER_BASE) as usize);
            return Some(stack.cloned().flatten());
        }
        let slot = player_menu_slot(state.inventory.selected_hotbar, id)?;
        Some(state.inventory.get(slot).cloned())
    }

    fn write_player_slot(&mut self, player: PlayerId, id: i32, stack: Slot) {
        let Some(state) = self.players.get_mut(&player) else {
            return;
        };
        if id == CURSOR {
            state.menu.set_carried(stack);
            self.send_inventory(player);
            return;
        }
        if (ENDER_BASE..ENDER_BASE + ENDER_SLOTS).contains(&id) {
            let index = (id - ENDER_BASE) as usize;
            if state.ender_items.len() <= index {
                state.ender_items.resize(index + 1, None);
            }
            state.ender_items[index] = stack;
            return;
        }
        if let Some(slot) = player_menu_slot(state.inventory.selected_hotbar, id) {
            self.set_slot_by_player(player, slot, stack);
        }
    }

    fn mount_index(id: i32) -> Option<usize> {
        (HORSE_BASE..HORSE_BASE + HORSE_SLOTS)
            .contains(&id)
            .then(|| equine::BODY_SLOT + 1 + (id - HORSE_BASE) as usize)
    }

    fn read_entity_slot(&self, entity: EntityId, id: i32) -> Option<Slot> {
        let found = self.entities.store.get(entity)?;
        if let Some(index) = Self::mount_index(id) {
            let state = crate::kinds::mount_of(found)?;
            let slot = state.inventory.get(index - equine::BODY_SLOT - 1)?;
            return Some(slot.clone());
        }
        let slot = equipment_of(id)?;
        Some(found.living()?.equipment(slot).cloned())
    }

    fn write_entity_slot(&mut self, entity: EntityId, id: i32, stack: Slot) {
        if let Some(index) = Self::mount_index(id) {
            let size = index + 1;
            let mut items = self.mount_menu_items(entity, size);
            items[index] = stack;
            self.write_mount_items(entity, &items);
            return;
        }
        let Some(slot) = equipment_of(id) else {
            return;
        };
        let Some(living) = self
            .entities
            .store
            .get_mut(entity)
            .and_then(|found| found.living_mut())
        else {
            return;
        };
        living.equipment[living::slot_index(slot)] = stack.clone();
        self.push(
            Audience::Trackers(entity),
            ClientEvent::EntityEquipment {
                entity,
                slots: vec![(wire_slot(slot), stack)],
            },
        );
    }

    fn read_block_slot(&self, pos: BlockPos, id: i32) -> Option<Slot> {
        let (backing, size) = self.storage_backing(pos)?;
        let index = usize::try_from(id).ok().filter(|index| *index < size)?;
        self.container_items(backing, size).get(index).cloned()
    }

    fn write_block_slot(&mut self, pos: BlockPos, id: i32, stack: Slot) {
        let Some((backing, size)) = self.storage_backing(pos) else {
            return;
        };
        let Some(index) = usize::try_from(id).ok().filter(|index| *index < size) else {
            return;
        };
        let mut items = self.container_items(backing, size);
        items[index] = stack;
        self.store_container(backing, &items);
        self.sync_viewers(pos, None);
    }

    pub(crate) fn insert_into_container(
        &mut self,
        pos: BlockPos,
        stack: ItemStack,
    ) -> Option<Option<ItemStack>> {
        let (backing, size) = self.storage_backing(pos)?;
        let mut items = self.container_items(backing, size);
        let mut rest: Slot = Some(stack);
        for slot in items.iter_mut() {
            let Some(left) = rest.as_ref() else {
                break;
            };
            let limit = rspigot_domain::item::max_stack(left);
            match slot {
                Some(held) if rspigot_domain::item::same_item_same_components(held, left) => {
                    let space = limit.saturating_sub(u32::from(held.count.get()));
                    let moved = space.min(u32::from(left.count.get()));
                    if moved > 0 {
                        let total = u32::from(held.count.get()) + moved;
                        *slot = rspigot_domain::item::with_count(held, total);
                        let remaining = u32::from(left.count.get()) - moved;
                        rest = rspigot_domain::item::with_count(left, remaining);
                    }
                }
                None => {
                    let moved = limit.min(u32::from(left.count.get()));
                    *slot = rspigot_domain::item::with_count(left, moved);
                    let remaining = u32::from(left.count.get()) - moved;
                    rest = rspigot_domain::item::with_count(left, remaining);
                }
                Some(_) => {}
            }
        }
        self.store_container(backing, &items);
        self.sync_viewers(pos, None);
        Some(rest)
    }
}
