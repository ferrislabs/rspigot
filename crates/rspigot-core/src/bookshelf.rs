use std::num::NonZeroU8;

use rspigot_blocks::BlockKind;
use rspigot_domain::bookshelf::{SLOTS, hit_slot};
use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::item::{ItemStack, Slot};
use rspigot_domain::player::GameMode;
use rspigot_domain::{BlockPos, BlockStateId, Identifier};
use rspigot_entities::SoundSource;
use rspigot_items::ItemKind;
use rspigot_nbt::Compound;

use crate::containers::write_items;
use crate::events::Direction;
use crate::plugins::steps::UseOnStep;
use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::flag;

const ENTITY: &str = "chiseled_bookshelf";
const LAST_INTERACTED: &str = "last_interacted_slot";
const BOOKS: &str = "minecraft:bookshelf_books";
const INSERT: &str = "minecraft:block.chiseled_bookshelf.insert";
const INSERT_ENCHANTED: &str = "minecraft:block.chiseled_bookshelf.insert.enchanted";
const PICKUP: &str = "minecraft:block.chiseled_bookshelf.pickup";
const PICKUP_ENCHANTED: &str = "minecraft:block.chiseled_bookshelf.pickup.enchanted";

pub(crate) fn is_bookshelf(state: BlockStateId) -> bool {
    state.kind() == Some(BlockKind::ChiseledBookshelf)
}

pub(crate) fn is_book(stack: &ItemStack) -> bool {
    rspigot_data::tags::tag("minecraft:item", BOOKS)
        .is_some_and(|ids| ids.binary_search(&u32::from(stack.item.0)).is_ok())
}

const fn face_name(face: Direction) -> &'static str {
    match face {
        Direction::Down => "down",
        Direction::Up => "up",
        Direction::North => "north",
        Direction::South => "south",
        Direction::West => "west",
        Direction::East => "east",
    }
}

fn occupied_state(state: BlockStateId, items: &[Slot]) -> BlockStateId {
    let mut next = state;
    for (slot, stack) in items.iter().enumerate() {
        let value = if stack.is_some() { "true" } else { "false" };
        if let Some(changed) = next.with_property(&format!("slot_{slot}_occupied"), value) {
            next = changed;
        }
    }
    next
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn bookshelf_items(&self, pos: BlockPos) -> Vec<Slot> {
        self.half_items(pos, SLOTS)
    }

    pub(crate) fn bookshelf_last_slot(&self, pos: BlockPos) -> i32 {
        self.world
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .and_then(|entity| entity.data.get_i32(LAST_INTERACTED))
            .unwrap_or(-1)
    }

    pub(crate) fn store_bookshelf(&mut self, pos: BlockPos, items: &[Slot]) {
        let before = self.bookshelf_items(pos);
        let last = (0..SLOTS)
            .rev()
            .find(|slot| before.get(*slot) != items.get(*slot))
            .map_or_else(
                || self.bookshelf_last_slot(pos),
                |slot| i32::try_from(slot).unwrap_or(-1),
            );
        let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) else {
            return;
        };
        let mut data = chunk
            .block_entity(pos)
            .map_or_else(Compound::new, |entity| entity.data.clone());
        write_items(&mut data, items);
        data.insert(LAST_INTERACTED, last);
        chunk.set_block_entity(BlockEntity {
            pos,
            kind: Identifier::minecraft(ENTITY),
            data,
        });
        let state = self.block_or_air(pos);
        let next = occupied_state(state, items);
        if next != state {
            self.set_block(pos, next);
        }
        self.update_neighbour_for_output_signal(pos);
    }

    pub(crate) fn shelf_signal(&self, pos: BlockPos) -> i32 {
        self.half_items(pos, rspigot_domain::shelf::SLOTS)
            .iter()
            .enumerate()
            .filter(|(_, stack)| stack.is_some())
            .fold(0, |bits, (slot, _)| bits | (1 << slot))
    }

    pub(crate) fn bookshelf_comparator(&self, pos: BlockPos) -> i32 {
        rspigot_domain::bookshelf::comparator_signal(self.bookshelf_last_slot(pos))
    }

    pub(crate) fn bookshelf_items_lost_by(
        &self,
        pos: BlockPos,
        old: BlockStateId,
        new: BlockStateId,
    ) -> Vec<ItemStack> {
        if !is_bookshelf(old) || old.kind() == new.kind() {
            return Vec::new();
        }
        self.bookshelf_items(pos).into_iter().flatten().collect()
    }

    pub(crate) fn use_chiseled_bookshelf(&mut self, step: &UseOnStep) -> bool {
        let (id, hand, pos, clicked) = (step.player, step.hand, step.pos, step.clicked);
        let cursor = step.cursor;
        if !is_bookshelf(clicked) {
            return false;
        }
        let Some(facing) = clicked.property("facing") else {
            return false;
        };
        let Some(slot) = hit_slot(facing, face_name(step.face), cursor.x, cursor.y, cursor.z)
        else {
            return false;
        };
        if self
            .players
            .get(&id)
            .is_none_or(|player| player.game_mode == GameMode::Spectator)
        {
            return false;
        }
        let occupied = flag(clicked, &format!("slot_{slot}_occupied"));
        let held = self.held(id, hand);
        let mut items = self.bookshelf_items(pos);
        if !occupied {
            let Some(book) = held.filter(is_book) else {
                return false;
            };
            let enchanted = book.item == ItemKind::EnchantedBook.id();
            if let Some(cell) = items.get_mut(slot) {
                *cell = Some(ItemStack {
                    count: NonZeroU8::MIN,
                    ..book
                });
            }
            self.consume_held(id, hand, 1);
            self.store_bookshelf(pos, &items);
            let sound = if enchanted { INSERT_ENCHANTED } else { INSERT };
            self.play_sound(None, pos.center(), sound, SoundSource::Blocks, 1.0, 1.0);
        } else {
            let Some(taken) = items.get_mut(slot).and_then(Option::take) else {
                return false;
            };
            let enchanted = taken.item == ItemKind::EnchantedBook.id();
            self.store_bookshelf(pos, &items);
            self.give_or_drop(id, taken);
            let sound = if enchanted { PICKUP_ENCHANTED } else { PICKUP };
            self.play_sound(None, pos.center(), sound, SoundSource::Blocks, 1.0, 1.0);
        }
        self.player_swing(id, hand, false);
        self.refresh_hands(id);
        self.ack_block(id, step.sequence);
        true
    }
}
