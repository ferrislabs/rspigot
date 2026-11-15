use std::num::NonZeroU8;

use crate::plugins::InventoryOwner;
use rand::Rng;
use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::BlockStateId;
use rspigot_domain::brewing;
use rspigot_domain::entity::Aabb;
use rspigot_domain::item::{ItemStack, Slot, same_item_same_components};
use rspigot_domain::{BlockPos, EntityId};
use rspigot_items::ItemKind;

use crate::containers::Backing;
use crate::cooking::cooking_of;
use crate::ports::ClientLink;
use crate::server::Server;

const FURNACE_INPUT: usize = 0;
const FURNACE_FUEL: usize = 1;
const FURNACE_OUTPUT: usize = 2;
const CRAFTER_SLOTS: usize = 9;
const BREWING_INGREDIENT: usize = rspigot_domain::brewing::INGREDIENT;
const BREWING_FUEL: usize = rspigot_domain::brewing::FUEL;
const CONTAINER_LIMIT: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Cart {
    pub(crate) id: EntityId,
    pub(crate) hopper: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Holder {
    pub(crate) pos: BlockPos,
    pub(crate) block: BlockKind,
    pub(crate) size: usize,
    backing: Option<Backing>,
    pub(crate) cart: Option<Cart>,
    disabled: u16,
}

impl Holder {
    pub(crate) fn is_furnace(&self) -> bool {
        self.cart.is_none() && cooking_of(self.block).is_some()
    }

    pub(crate) fn is_hopper(&self) -> bool {
        self.cart.is_none() && self.block == BlockKind::Hopper
    }

    pub(crate) fn owner(&self) -> InventoryOwner {
        match self.cart {
            Some(cart) => InventoryOwner::Entity(cart.id),
            None => InventoryOwner::Block(self.pos),
        }
    }

    pub(crate) fn is_crafter(&self) -> bool {
        self.cart.is_none() && self.block == BlockKind::Crafter
    }

    fn crafter_slot_disabled(&self, slot: usize) -> bool {
        slot < CRAFTER_SLOTS && self.disabled & (1 << slot) != 0
    }

    fn crafter_accepts(&self, slot: usize, items: &[Slot]) -> bool {
        if slot >= CRAFTER_SLOTS || self.crafter_slot_disabled(slot) {
            return false;
        }
        let Some(existing) = items.get(slot).and_then(Option::as_ref) else {
            return true;
        };
        let count = existing.count.get();
        if u32::from(count) >= u32::from(existing.item.max_stack_size()) {
            return false;
        }
        !(slot + 1..CRAFTER_SLOTS)
            .filter(|other| !self.crafter_slot_disabled(*other))
            .any(|other| {
                items
                    .get(other)
                    .and_then(Option::as_ref)
                    .is_none_or(|stack| {
                        stack.count.get() < count && same_item_same_components(stack, existing)
                    })
            })
    }

    pub(crate) fn is_jukebox(&self) -> bool {
        self.cart.is_none() && self.block == BlockKind::Jukebox
    }

    pub(crate) fn is_brewing_stand(&self) -> bool {
        self.cart.is_none() && self.block == BlockKind::BrewingStand
    }

    pub(crate) fn kind_name(&self) -> &'static str {
        if let Some(cart) = self.cart {
            return if cart.hopper { "HOPPER" } else { "CHEST" };
        }
        match self.block {
            BlockKind::BrewingStand => "BREWING",
            BlockKind::Jukebox => "JUKEBOX",
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
        if self.is_brewing_stand() {
            return match direction {
                Face::Up => vec![BREWING_INGREDIENT],
                Face::Down => vec![0, 1, 2, BREWING_INGREDIENT],
                _ => vec![0, 1, 2, BREWING_FUEL],
            };
        }
        if !self.is_furnace() {
            return (0..self.size).collect();
        }
        match direction {
            Face::Down => vec![FURNACE_OUTPUT, FURNACE_FUEL],
            Face::Up => vec![FURNACE_INPUT],
            _ => vec![FURNACE_FUEL],
        }
    }

    pub(crate) fn may_place(
        &self,
        slot: usize,
        stack: &ItemStack,
        direction: Face,
        items: &[Slot],
    ) -> bool {
        if self.is_crafter() {
            return self.crafter_accepts(slot, items);
        }
        if self.is_jukebox() {
            return rspigot_domain::jukebox::is_playable(stack)
                && items.get(slot).is_none_or(Option::is_none);
        }
        if self.is_brewing_stand() {
            return match slot {
                BREWING_FUEL => brewing::is_fuel(stack),
                BREWING_INGREDIENT => brewing::is_reagent(stack),
                _ => brewing::is_container(stack) && items.get(slot).is_none_or(Option::is_none),
            };
        }
        if self.is_furnace() {
            return match slot {
                FURNACE_OUTPUT => false,
                FURNACE_FUEL => {
                    rspigot_domain::furnace::is_fuel(stack)
                        || stack.item == ItemKind::Bucket.id()
                            && items
                                .get(FURNACE_FUEL)
                                .and_then(Option::as_ref)
                                .is_none_or(|fuel| fuel.item != ItemKind::Bucket.id())
                }
                _ => direction != Face::Down,
            };
        }
        if self.cart.is_none() && self.block.name().ends_with("shulker_box") {
            return !stack
                .item
                .kind()
                .is_some_and(|kind| kind.name().ends_with("shulker_box"));
        }
        true
    }

    pub(crate) fn may_take(&self, slot: usize, stack: &ItemStack, direction: Face) -> bool {
        if self.is_brewing_stand() && slot == BREWING_INGREDIENT {
            return stack.item == ItemKind::GlassBottle.id();
        }
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
        if block == BlockKind::Jukebox {
            return Some(Holder {
                pos,
                block,
                size: 1,
                backing: None,
                cart: None,
                disabled: 0,
            });
        }
        if cooking_of(block).is_some() {
            return Some(Holder {
                pos,
                block,
                size: 3,
                backing: None,
                cart: None,
                disabled: 0,
            });
        }
        let (backing, size) = self.storage_backing(pos)?;
        Some(Holder {
            pos,
            block,
            size,
            backing: Some(backing),
            cart: None,
            disabled: if block == BlockKind::Crafter {
                self.crafter_disabled_mask(pos)
            } else {
                0
            },
        })
    }

    fn crafter_disabled_mask(&self, pos: BlockPos) -> u16 {
        self.world
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .and_then(|entity| entity.data.get_int_array("disabled_slots"))
            .map_or(0, |slots| {
                slots
                    .iter()
                    .filter(|slot| (0..CRAFTER_SLOTS as i32).contains(slot))
                    .fold(0, |mask, slot| mask | 1 << slot)
            })
    }

    pub(crate) fn cart_holder(&self, id: EntityId) -> Option<Holder> {
        let entity = self.entities.store.get(id)?;
        let cargo = crate::vehicles::cargo_of(entity)?;
        Some(Holder {
            pos: BlockPos::new(
                entity.base.position.x.floor() as i32,
                entity.base.position.y.floor() as i32,
                entity.base.position.z.floor() as i32,
            ),
            block: BlockKind::Air,
            size: cargo.items.len(),
            backing: None,
            disabled: 0,
            cart: Some(Cart {
                id,
                hopper: entity.base.kind == rspigot_entities::EntityKind::HopperMinecart,
            }),
        })
    }

    pub(crate) fn container_at(&mut self, pos: BlockPos) -> Option<Holder> {
        if let Some(holder) = self.holder_at(pos) {
            return Some(holder);
        }
        let area = Aabb::block(pos);
        let mut carts: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|entity| {
                entity.base.removal.is_none()
                    && crate::vehicles::cargo_of(entity).is_some()
                    && entity.base.bounding_box().intersects(&area)
            })
            .map(|entity| entity.base.id)
            .collect();
        if carts.is_empty() {
            return None;
        }
        carts.sort_by_key(|id| id.0);
        let pick = self.rng.random_range(0..carts.len());
        self.cart_holder(carts[pick])
    }

    pub(crate) fn holder_items(&self, holder: &Holder) -> Vec<Slot> {
        if let Some(cart) = holder.cart {
            return self
                .entities
                .store
                .get(cart.id)
                .and_then(crate::vehicles::cargo_of)
                .map_or_else(|| vec![None; holder.size], |cargo| cargo.items.clone());
        }
        if holder.is_jukebox() {
            return vec![self.jukebox_record(holder.pos)];
        }
        match holder.backing {
            Some(backing) => self.container_items(backing, holder.size),
            None => self
                .furnace_at(holder.pos)
                .map_or_else(|| vec![None; 3], |furnace| furnace.items.to_vec()),
        }
    }

    pub(crate) fn store_holder(&mut self, holder: &Holder, items: &[Slot]) {
        if let Some(cart) = holder.cart {
            if let Some(cargo) = self
                .entities
                .store
                .get_mut(cart.id)
                .and_then(crate::vehicles::cargo_mut)
            {
                for (slot, stack) in items.iter().enumerate() {
                    if let Some(cell) = cargo.items.get_mut(slot) {
                        cell.clone_from(stack);
                    }
                }
            }
            return;
        }
        if holder.is_jukebox() {
            let next = items.first().cloned().flatten();
            if next != self.jukebox_record(holder.pos) {
                self.set_jukebox_record(holder.pos, next);
            }
            return;
        }
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
