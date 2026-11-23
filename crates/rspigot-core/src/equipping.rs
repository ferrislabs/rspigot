use std::num::NonZeroU8;

use rspigot_domain::Vec3;
use rspigot_domain::enchant;
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::item::{ItemStack, OFFHAND_SLOT, same_item_same_components};
use rspigot_domain::player::PlayerId;
use rspigot_domain::stack_components;
use rspigot_entities::SoundSource;
use rspigot_items::EquipmentSlot;

use crate::events::Hand;
use crate::game_events::GameEventContext;
use crate::health::{CHEST_SLOT, FEET_SLOT, HEAD_SLOT, LEGS_SLOT};
use crate::ports::ClientLink;
use crate::server::Server;

pub(crate) const fn inventory_slot(slot: EquipmentSlot) -> Option<usize> {
    match slot {
        EquipmentSlot::Head => Some(HEAD_SLOT),
        EquipmentSlot::Chest => Some(CHEST_SLOT),
        EquipmentSlot::Legs => Some(LEGS_SLOT),
        EquipmentSlot::Feet => Some(FEET_SLOT),
        EquipmentSlot::Offhand => Some(OFFHAND_SLOT),
        EquipmentSlot::Mainhand | EquipmentSlot::Body | EquipmentSlot::Saddle => None,
    }
}

fn one_of(stack: &ItemStack) -> ItemStack {
    ItemStack {
        count: NonZeroU8::MIN,
        ..stack.clone()
    }
}

fn remainder_of(stack: &ItemStack) -> Option<ItemStack> {
    NonZeroU8::new(stack.count.get() - 1).map(|count| ItemStack {
        count,
        ..stack.clone()
    })
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn use_equippable(&mut self, id: PlayerId, hand: Hand) -> bool {
        let Some(stack) = self.held(id, hand) else {
            return false;
        };
        let Some(equippable) = stack_components::equippable(&stack) else {
            return false;
        };
        if !equippable.swappable {
            return false;
        }
        let Some(target) = inventory_slot(equippable.slot) else {
            return false;
        };
        let sound = equippable.equip_sound.to_owned();
        let Some(hand_slot) = self.hand_slot_of(id, hand) else {
            return false;
        };
        let creative = self.infinite_materials(id);
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        if hand_slot == target {
            return true;
        }
        let in_slot = player.inventory.get(target).cloned();
        let locked = in_slot
            .as_ref()
            .is_some_and(|worn| enchant::prevents_armor_change(worn) && !creative);
        let same = in_slot
            .as_ref()
            .is_some_and(|worn| same_item_same_components(&stack, worn));
        if locked || same {
            return true;
        }
        let before = Self::equipment_of(player);
        let position = player.position;
        let entity = player.entity_id;
        let mut displaced = None;
        let Some(player) = self.players.get_mut(&id) else {
            return false;
        };
        if stack.count.get() <= 1 {
            let new_hand = match &in_slot {
                None => creative.then(|| stack.clone()),
                Some(worn) => Some(worn.clone()),
            };
            player.inventory.set(target, Some(stack.clone()));
            player.inventory.set(hand_slot, new_hand);
        } else {
            player.inventory.set(target, Some(one_of(&stack)));
            if !creative {
                player.inventory.set(hand_slot, remainder_of(&stack));
            }
            displaced = in_slot;
        }
        if let Some(old) = displaced {
            let mut spare = Some(old);
            player.inventory.add(&mut spare, false);
            if let Some(spare) = spare {
                self.drop_at_location(entity, 1.0, spare, Vec3::ZERO);
            }
        }
        self.broadcast_equipment_changes(id, before);
        self.play_named_sound(None, position, &sound, SoundSource::Players, 1.0, 1.0);
        self.game_event(
            GameEvent::Equip,
            position,
            GameEventContext::of(Some(entity)),
        );
        self.count_used(id, &stack);
        self.refresh_hands(id);
        true
    }
}
