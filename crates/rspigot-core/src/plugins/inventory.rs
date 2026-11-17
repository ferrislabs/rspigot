use rspigot_domain::Vec3;
use rspigot_domain::item::{self, menu_slot};
use rspigot_domain::player::PlayerId;
use rspigot_items::ItemKind;

use crate::commands::CommandContext;
use crate::events::ClientEvent;
use crate::menu::Equipment;
use crate::plugins::items::{spec_of, stack_of};
use crate::plugins::{InventoryOrder, InventoryState};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const SPIGOT_SLOTS: usize = 41;
const STORAGE_SLOTS: usize = 36;
const HOTBAR_SLOTS: u8 = 9;

impl<L: ClientLink> Server<L> {
    pub(super) fn apply_inventory_order(
        &mut self,
        connection: u64,
        player: PlayerId,
        order: InventoryOrder,
    ) {
        let name = order.name();
        let Some(before) = self.players.get(&player).map(Self::equipment_of) else {
            self.warn_plugin(connection, name, "the player is not online");
            return;
        };
        if let Err(reason) = self.inventory_order(player, order, before) {
            self.warn_plugin(connection, name, reason);
        }
    }

    fn inventory_order(
        &mut self,
        id: PlayerId,
        order: InventoryOrder,
        before: Equipment,
    ) -> Result<(), String> {
        match order {
            InventoryOrder::Add(spec) => {
                let mut stack = stack_of(&spec)?;
                if let Some(player) = self.players.get_mut(&id) {
                    player.inventory.add(&mut stack, false);
                    let feet = player.position;
                    if let Some(leftover) = stack {
                        self.spawn_item_entity(feet, Vec3::ZERO, 0, leftover);
                    }
                }
            }
            InventoryOrder::Set { slot, item } => {
                let slot = usize::try_from(slot)
                    .ok()
                    .and_then(menu_slot)
                    .ok_or_else(|| format!("slot {slot} is outside 0..=40"))?;
                let stack = match item {
                    Some(spec) => stack_of(&spec)?,
                    None => None,
                };
                if let Some(player) = self.players.get_mut(&id) {
                    player.inventory.set(slot, stack);
                }
            }
            InventoryOrder::Remove { key, count } => {
                let wanted = ItemKind::from_name(&key)
                    .ok_or_else(|| format!("unknown item {key}"))?
                    .id();
                let mut left = u32::try_from(count)
                    .ok()
                    .filter(|count| *count > 0)
                    .ok_or_else(|| format!("count {count} is below 1"))?;
                if let Some(player) = self.players.get_mut(&id) {
                    for slot in (0..STORAGE_SLOTS).filter_map(menu_slot) {
                        if left == 0 {
                            break;
                        }
                        if player.inventory.get(slot).is_some_and(|s| s.item == wanted) {
                            left -= item::count(&player.inventory.remove(slot, left));
                        }
                    }
                }
            }
            InventoryOrder::Clear => {
                CommandContext::clear_inventory(self, id);
            }
            InventoryOrder::SetHeldSlot(slot) => {
                let slot = u8::try_from(slot)
                    .ok()
                    .filter(|slot| *slot < HOTBAR_SLOTS)
                    .ok_or_else(|| format!("held slot {slot} is outside 0..=8"))?;
                if let Some(player) = self.players.get_mut(&id) {
                    player.inventory.selected_hotbar = slot;
                }
                self.push(Audience::One(id), ClientEvent::HeldSlot(slot));
                self.broadcast_held_item(id);
                return Ok(());
            }
        }
        self.sync_inventory(id);
        self.broadcast_equipment_changes(id, before);
        Ok(())
    }

    pub(super) fn inventory_state(&self, id: PlayerId) -> Option<InventoryState> {
        let inventory = &self.players.get(&id)?.inventory;
        Some(InventoryState {
            slots: (0..SPIGOT_SLOTS)
                .map(|slot| {
                    menu_slot(slot)
                        .and_then(|menu| inventory.get(menu))
                        .map(spec_of)
                })
                .collect(),
            held_slot: inventory.selected_hotbar,
        })
    }
}
