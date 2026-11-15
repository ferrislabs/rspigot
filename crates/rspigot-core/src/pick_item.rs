use std::num::NonZeroU8;

use rspigot_blocks::BlockKind;
use rspigot_domain::entity::hanging;
use rspigot_domain::entity::{Aabb, Entity};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, EntityId, ItemId};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::ItemKind;

use crate::events::ClientEvent;
use crate::interact::box_distance_squared;
use crate::ports::ClientLink;
use crate::server::Server;
use crate::vehicles::drop_item_of;

const BLOCK_REACH: f64 = 4.5;
const CREATIVE_BLOCK_REACH: f64 = 5.0;
const PICK_BLOCK_BUFFER: f64 = 1.0;

fn single(item: ItemId) -> ItemStack {
    ItemStack::new(item, NonZeroU8::MIN)
}

fn pick_result(entity: &Entity) -> Option<ItemStack> {
    let kind = entity.base.kind;
    if hanging::is_frame(kind) {
        return Some(hanging::item(&entity.base).map_or_else(
            || {
                single(
                    if kind == Kind::GlowItemFrame {
                        ItemKind::GlowItemFrame
                    } else {
                        ItemKind::ItemFrame
                    }
                    .id(),
                )
            },
            Clone::clone,
        ));
    }
    let item = match kind {
        Kind::Painting => ItemKind::Painting,
        Kind::ArmorStand => ItemKind::ArmorStand,
        Kind::LeashKnot => ItemKind::Lead,
        Kind::EndCrystal => ItemKind::EndCrystal,
        Kind::SpawnerMinecart => ItemKind::Minecart,
        Kind::CommandBlockMinecart => ItemKind::CommandBlockMinecart,
        other if other.extends("AbstractMinecart") || other.extends("AbstractBoat") => {
            drop_item_of(other)?
        }
        other => return other.spawn_egg().map(|egg| single(ItemId(egg))),
    };
    Some(single(item.id()))
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn handle_pick_block(&mut self, id: PlayerId, pos: BlockPos) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let reach = if player.game_mode == GameMode::Creative {
            CREATIVE_BLOCK_REACH
        } else {
            BLOCK_REACH
        } + PICK_BLOCK_BUFFER;
        let cube = Aabb::new(
            f64::from(pos.x),
            f64::from(pos.y),
            f64::from(pos.z),
            f64::from(pos.x) + 1.0,
            f64::from(pos.y) + 1.0,
            f64::from(pos.z) + 1.0,
        );
        if box_distance_squared(&cube, player.eye_position()) >= reach * reach {
            return;
        }
        let Some(state) = self.world.chunks.block(pos) else {
            return;
        };
        let Some(item) = state.kind().and_then(BlockKind::item) else {
            return;
        };
        self.try_pick_item(id, single(ItemId(item)));
    }

    pub(crate) fn handle_pick_entity(&mut self, id: PlayerId, target: EntityId) {
        if !self.within_entity_reach(id, target) {
            return;
        }
        let Some(stack) = self.entities.store.get(target).and_then(pick_result) else {
            return;
        };
        self.try_pick_item(id, stack);
    }

    fn try_pick_item(&mut self, id: PlayerId, stack: ItemStack) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let infinite = player.game_mode == GameMode::Creative;
        match player.inventory.find_matching(&stack) {
            None if infinite => player.inventory.add_and_pick(stack),
            None => {}
            Some(slot) if slot < 9 => player.inventory.selected_hotbar = slot as u8,
            Some(slot) => player.inventory.pick_slot(slot),
        }
        let selected = player.inventory.selected_hotbar;
        player.link.send(ClientEvent::HeldSlot(selected));
        self.sync_inventory(id);
        self.broadcast_held_item(id);
    }
}
