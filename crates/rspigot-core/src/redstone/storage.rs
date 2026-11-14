use std::num::NonZeroU8;

use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::BlockPos;
use rspigot_domain::BlockStateId;
use rspigot_domain::item::{ItemStack, Slot, same_item_same_components};
use rspigot_items::ItemKind;

use crate::containers::Backing;
use crate::cooking::cooking_of;
use crate::ports::ClientLink;
use crate::server::Server;

const FURNACE_INPUT: usize = 0;
const FURNACE_FUEL: usize = 1;
const FURNACE_OUTPUT: usize = 2;
const CONTAINER_LIMIT: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Holder {
    pub(crate) pos: BlockPos,
    pub(crate) block: BlockKind,
    pub(crate) size: usize,
    backing: Option<Backing>,
}

impl Holder {
    pub(crate) fn is_furnace(&self) -> bool {
        cooking_of(self.block).is_some()
    }

    pub(crate) fn is_hopper(&self) -> bool {
        self.block == BlockKind::Hopper
    }

    pub(crate) fn kind_name(&self) -> &'static str {
        match self.block {
            BlockKind::Hopper => "HOPPER",
            BlockKind::Dispenser => "DISPENSER",
            BlockKind::Dropper => "DROPPER",
            BlockKind::Barrel => "BARREL",
            BlockKind::Crafter => "CRAFTER",
            BlockKind::Furnace => "FURNACE",
            BlockKind::BlastFurnace => "BLAST_FURNACE",
            BlockKind::Smoker => "SMOKER",
            block if block.name().ends_with("shulker_box") => "SHULKER_BOX",
            _ => "CHEST",
        }
    }

    pub(crate) fn slots_toward(&self, direction: Face) -> Vec<usize> {
        if !self.is_furnace() {
            return (0..self.size).collect();
        }
        match direction {
            Face::Down => vec![FURNACE_OUTPUT, FURNACE_FUEL],
            Face::Up => vec![FURNACE_INPUT],
            _ => vec![FURNACE_FUEL],
        }
    }

    pub(crate) fn may_place(&self, slot: usize, stack: &ItemStack, direction: Face) -> bool {
        if self.is_furnace() {
            return match slot {
                FURNACE_OUTPUT => false,
                FURNACE_FUEL => {
                    rspigot_domain::furnace::is_fuel(stack) || stack.item == ItemKind::Bucket.id()
                }
                _ => direction != Face::Down,
            };
        }
        if self.block.name().ends_with("shulker_box") {
            return !stack
                .item
                .kind()
                .is_some_and(|kind| kind.name().ends_with("shulker_box"));
        }
        true
    }

    pub(crate) fn may_take(&self, slot: usize, stack: &ItemStack, direction: Face) -> bool {
        if self.is_furnace() && direction == Face::Down && slot == FURNACE_FUEL {
            return stack.item == ItemKind::Bucket.id() || stack.item == ItemKind::WaterBucket.id();
        }
        true
    }
}

pub(crate) fn limit_of(stack: &ItemStack) -> u32 {
    u32::from(stack.item.max_stack_size()).min(CONTAINER_LIMIT)
}

pub(crate) fn insert_stack(
    items: &mut [Slot],
    order: &[usize],
    mut stack: ItemStack,
    allowed: impl Fn(usize, &ItemStack) -> bool,
) -> Option<ItemStack> {
    for &slot in order {
        if !allowed(slot, &stack) {
            continue;
        }
        let limit = limit_of(&stack);
        let Some(cell) = items.get_mut(slot) else {
            continue;
        };
        match cell {
            None => {
                let moved = u32::from(stack.count.get()).min(limit);
                if let Some(count) = NonZeroU8::new(moved as u8) {
                    *cell = Some(ItemStack {
                        count,
                        ..stack.clone()
                    });
                }
                stack.count = NonZeroU8::new(stack.count.get() - moved as u8)?;
            }
            Some(existing) if same_item_same_components(existing, &stack) => {
                let space = limit.saturating_sub(u32::from(existing.count.get()));
                let moved = u32::from(stack.count.get()).min(space);
                if moved == 0 {
                    continue;
                }
                if let Some(total) = NonZeroU8::new((u32::from(existing.count.get()) + moved) as u8)
                {
                    existing.count = total;
                }
                stack.count = NonZeroU8::new(stack.count.get() - moved as u8)?;
            }
            Some(_) => {}
        }
    }
    Some(stack)
}

pub(crate) fn takes_all(
    items: &[Slot],
    order: &[usize],
    stack: &ItemStack,
    allowed: impl Fn(usize, &ItemStack) -> bool,
) -> bool {
    let mut trial = items.to_vec();
    insert_stack(&mut trial, order, stack.clone(), allowed).is_none()
}

pub(crate) fn is_full(items: &[Slot], order: &[usize]) -> bool {
    order.iter().all(|slot| {
        items
            .get(*slot)
            .and_then(Option::as_ref)
            .is_some_and(|stack| u32::from(stack.count.get()) >= limit_of(stack))
    })
}

pub(crate) fn is_empty(items: &[Slot], order: &[usize]) -> bool {
    order
        .iter()
        .all(|slot| items.get(*slot).is_none_or(Option::is_none))
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn holder_at(&self, pos: BlockPos) -> Option<Holder> {
        let state: BlockStateId = self.block_or_air(pos);
        let block = state.kind()?;
        if cooking_of(block).is_some() {
            return Some(Holder {
                pos,
                block,
                size: 3,
                backing: None,
            });
        }
        let (backing, size) = self.storage_backing(pos)?;
        Some(Holder {
            pos,
            block,
            size,
            backing: Some(backing),
        })
    }

    pub(crate) fn holder_items(&self, holder: &Holder) -> Vec<Slot> {
        match holder.backing {
            Some(backing) => self.container_items(backing, holder.size),
            None => self
                .furnace_at(holder.pos)
                .map_or_else(|| vec![None; 3], |furnace| furnace.items.to_vec()),
        }
    }

    pub(crate) fn store_holder(&mut self, holder: &Holder, items: &[Slot]) {
        match holder.backing {
            Some(backing) => self.store_container(backing, items),
            None => {
                let Some(mut furnace) = self.furnace_at(holder.pos) else {
                    return;
                };
                for (slot, stack) in items.iter().take(3).enumerate() {
                    furnace.items[slot] = stack.clone();
                }
                self.store_furnace(holder.pos, &furnace);
            }
        }
    }
}
