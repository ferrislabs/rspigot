use std::sync::Arc;

use rspigot_plugin_protocol::v1::{self, from_plugin, player_extra};
use uuid::Uuid;

use crate::link::Link;
use crate::{BlockPos, BlockState, EquipmentSlot, ItemStack};

const fn slot_key(slot: EquipmentSlot) -> &'static str {
    match slot {
        EquipmentSlot::Hand => "mainhand",
        EquipmentSlot::OffHand => "offhand",
        EquipmentSlot::Head => "head",
        EquipmentSlot::Chest => "chest",
        EquipmentSlot::Legs => "legs",
        EquipmentSlot::Feet => "feet",
        EquipmentSlot::Body => "body",
        EquipmentSlot::Saddle => "saddle",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourcePack {
    pub id: Uuid,
    pub url: String,
    pub hash: String,
    pub required: bool,
    pub prompt: Option<String>,
}

impl ResourcePack {
    pub fn new(id: Uuid, url: impl Into<String>, hash: impl Into<String>) -> Self {
        Self {
            id,
            url: url.into(),
            hash: hash.into(),
            required: false,
            prompt: None,
        }
    }

    #[must_use]
    pub const fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    #[must_use]
    pub fn prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = Some(prompt.into());
        self
    }
}

#[derive(Debug, Clone)]
pub struct PlayerClientHandle {
    target: Arc<str>,
    entity_id: i32,
    link: Link,
}

impl PlayerClientHandle {
    pub(crate) fn new(target: Arc<str>, entity_id: i32, link: Link) -> Self {
        Self {
            target,
            entity_id,
            link,
        }
    }

    fn order(&self, order: player_extra::Order) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.to_string(),
            order: Some(v1::player_order::Order::Extra(v1::PlayerExtra {
                order: Some(order),
            })),
        }));
    }

    pub fn send_block_change(&self, pos: BlockPos, state: &BlockState) {
        self.send_block_changes(&[(pos, state.clone())]);
    }

    pub fn send_block_changes(&self, changes: &[(BlockPos, BlockState)]) {
        self.order(player_extra::Order::SendBlockChanges(v1::ClientBlocks {
            changes: changes
                .iter()
                .map(|(pos, state)| v1::ClientBlock {
                    pos: Some(pos.to_wire()),
                    state: Some(state.to_wire()),
                })
                .collect(),
        }));
    }

    pub fn send_sign_change(
        &self,
        pos: BlockPos,
        lines: &[&str],
        color: Option<&str>,
        glowing: bool,
    ) {
        self.order(player_extra::Order::SendSignChange(v1::ClientSign {
            pos: Some(pos.to_wire()),
            lines: lines.iter().map(|line| (*line).to_owned()).collect(),
            color: color.map(str::to_owned),
            glowing,
        }));
    }

    pub fn send_equipment_change(
        &self,
        entity_id: i32,
        slots: &[(EquipmentSlot, Option<ItemStack>)],
    ) {
        self.order(player_extra::Order::SendEquipmentChange(
            v1::ClientEquipment {
                entity: entity_id,
                slots: slots
                    .iter()
                    .map(|(slot, item)| v1::EquipmentEdit {
                        slot: slot_key(*slot).to_owned(),
                        item: item.as_ref().map(ItemStack::to_wire),
                    })
                    .collect(),
            },
        ));
    }

    pub fn send_hurt_animation(&self, yaw: f32) {
        self.order(player_extra::Order::SendHurtAnimation(v1::ClientHurt {
            entity: self.entity_id,
            yaw,
        }));
    }

    pub fn send_resource_pack(&self, pack: &ResourcePack) {
        self.order(player_extra::Order::SendResourcePack(v1::PackOffer {
            id: pack.id.to_string(),
            url: pack.url.clone(),
            hash: pack.hash.clone(),
            required: pack.required,
            prompt: pack.prompt.clone(),
        }));
    }

    pub fn remove_resource_pack(&self, id: Uuid) {
        self.order(player_extra::Order::RemoveResourcePack(v1::PackRemoval {
            id: Some(id.to_string()),
        }));
    }

    pub fn remove_resource_packs(&self) {
        self.order(player_extra::Order::RemoveResourcePack(v1::PackRemoval {
            id: None,
        }));
    }
}
