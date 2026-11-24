use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::entity::{Aabb, EntityState, item};
use rspigot_domain::item::{ItemStack, with_count};
use rspigot_domain::{BlockPos, BlockStateId, EntityId};
use rustc_hash::FxHashSet;

use super::storage::{Holder, insert_stack, is_empty, is_full, takes_all};
use super::{facing, step};
use crate::plugins::events::{
    HopperInventorySearch, HopperInventorySearchContainerType, HopperInventorySearchPatch,
    InventoryMoveItem, InventoryMoveItemPatch, InventoryPickupItem,
};
use crate::plugins::{
    Continuation, EventKind, EventPayload, InventoryOwner, InventoryRef, Lane, spec_of, stack_of,
};
use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::flag;

const BUSY_LIMIT: u64 = 100;
const HOPPER_SLOTS: usize = 5;
const SUCK_LOW: f64 = 0.6875;
const SUCK_HIGH: f64 = 2.0;
const CART_REACH: f64 = 0.25;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Clock {
    pub(crate) cooldown: i32,
    pub(crate) ticked: u64,
    pub(crate) busy: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    Eject,
    Suck,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Transfer {
    pub(crate) source: Holder,
    pub(crate) destination: Holder,
    pub(crate) slot: usize,
    pub(crate) stack: ItemStack,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Pickup {
    pub(crate) item: EntityId,
    pub(crate) stack: ItemStack,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Stage {
    Search,
    Move(Transfer),
    Pickup(Pickup),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HopperStep {
    pub(crate) hopper: BlockPos,
    pub(crate) phase: Phase,
    pub(crate) at: BlockPos,
    pub(crate) found: Option<Holder>,
    pub(crate) changed: bool,
    pub(crate) stage: Stage,
    pub(crate) cart: Option<EntityId>,
}

impl HopperStep {
    pub(crate) fn kind(&self) -> EventKind {
        match self.stage {
            Stage::Search => EventKind::HopperInventorySearch,
            Stage::Move(_) => EventKind::InventoryMoveItem,
            Stage::Pickup(_) => EventKind::InventoryPickupItem,
        }
    }
}

fn suck_area(pos: BlockPos) -> Aabb {
    let (x, y, z) = (f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
    Aabb::new(x, y + SUCK_LOW, z, x + 1.0, y + SUCK_HIGH, z + 1.0)
}

impl<L: ClientLink> Server<L> {
    fn inventory_ref(holder: &Holder) -> InventoryRef {
        InventoryRef {
            owner: holder.owner(),
            size: i32::try_from(holder.size).unwrap_or(i32::MAX),
            kind: holder.kind_name().to_owned(),
            id: 0,
        }
    }

    fn hopper_holder(&self, hopper: BlockPos, cart: Option<EntityId>) -> Option<Holder> {
        match cart {
            Some(id) => self.cart_holder(id),
            None => self.holder_at(hopper),
        }
    }

    fn suck_areas(&self, hopper: BlockPos, cart: Option<EntityId>) -> Vec<Aabb> {
        let Some(entity) = cart.and_then(|id| self.entities.store.get(id)) else {
            return vec![suck_area(hopper)];
        };
        let at = entity.base.position;
        let level_y = at.y + 0.5;
        let column = Aabb::new(
            at.x - 0.5,
            level_y - 0.5 + SUCK_LOW,
            at.z - 0.5,
            at.x + 0.5,
            level_y - 0.5 + SUCK_HIGH,
            at.z + 0.5,
        );
        let body = entity.base.bounding_box();
        vec![
            column,
            Aabb::new(
                body.min_x - CART_REACH,
                body.min_y,
                body.min_z - CART_REACH,
                body.max_x + CART_REACH,
                body.max_y,
                body.max_z + CART_REACH,
            ),
        ]
    }

    fn items_in(&self, areas: &[Aabb]) -> Vec<(EntityId, ItemStack)> {
        let mut found: Vec<(EntityId, ItemStack)> = Vec::new();
        for area in areas {
            let mut group: Vec<(EntityId, ItemStack)> = self
                .entities
                .store
                .touching(*area)
                .filter(|entity| {
                    matches!(entity.state, EntityState::Item(_)) && entity.base.removal.is_none()
                })
                .filter_map(|entity| {
                    item::stack(&entity.base).map(|stack| (entity.base.id, stack.clone()))
                })
                .collect();
            group.sort_by_key(|(id, _)| id.0);
            for entry in group {
                if found.iter().all(|(id, _)| *id != entry.0) {
                    found.push(entry);
                }
            }
        }
        found
    }

    pub(crate) fn tick_hopper_carts(&mut self) {
        let ids: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|entity| {
                entity.base.kind == rspigot_entities::EntityKind::HopperMinecart
                    && entity.base.removal.is_none()
                    && matches!(&entity.state, EntityState::Minecart(state) if state.enabled)
            })
            .map(|entity| entity.base.id)
            .collect();
        for id in ids {
            self.hopper_cart_tick(id);
        }
    }

    fn hopper_cart_tick(&mut self, id: EntityId) {
        let Some(me) = self.cart_holder(id) else {
            return;
        };
        let lane = Lane::Block(me.pos);
        if self.decisions.in_flight(
            lane,
            |then| matches!(then, Continuation::Hopper(step) if step.cart == Some(id)),
        ) {
            return;
        }
        if is_full(&self.holder_items(&me), &me.slots_toward(Face::Down)) {
            return;
        }
        let Some(position) = self
            .entities
            .store
            .get(id)
            .map(|entity| entity.base.position)
        else {
            return;
        };
        let at = rspigot_domain::Vec3::new(position.x, position.y + 1.5, position.z).block();
        let found = self.container_at(at);
        self.ask_hopper(HopperStep {
            hopper: me.pos,
            phase: Phase::Suck,
            at,
            found,
            changed: false,
            stage: Stage::Search,
            cart: Some(id),
        });
    }

    pub(crate) fn hopper_neighbor_changed(&mut self, pos: BlockPos, state: BlockStateId) {
        let enabled = !self.has_neighbor_signal(pos);
        if enabled != flag(state, "enabled")
            && let Some(next) =
                state.with_property("enabled", if enabled { "true" } else { "false" })
        {
            self.set_block_quiet(pos, next);
        }
    }

    pub(crate) fn tick_hoppers(&mut self) {
        let mut found: Vec<BlockPos> = Vec::new();
        for chunk_pos in self.world.chunks.loaded_positions() {
            let Some(chunk) = self.world.chunks.get(chunk_pos) else {
                continue;
            };
            found.extend(
                chunk
                    .block_entities
                    .iter()
                    .filter(|entity| entity.kind.as_str() == "minecraft:hopper")
                    .map(|entity| entity.pos),
            );
        }
        let alive: FxHashSet<BlockPos> = found.iter().copied().collect();
        self.falls
            .redstone
            .hoppers
            .retain(|pos, _| alive.contains(pos));
        for pos in found {
            self.hopper_tick(pos);
        }
    }

    fn hopper_tick(&mut self, pos: BlockPos) {
        let state = self.block_or_air(pos);
        if state.kind() != Some(BlockKind::Hopper) {
            return;
        }
        let now = self.current_tick();
        let clock = self.falls.redstone.hoppers.entry(pos).or_default();
        clock.ticked = now;
        if let Some(since) = clock.busy {
            if now.saturating_sub(since) < BUSY_LIMIT {
                return;
            }
            clock.busy = None;
        }
        if clock.cooldown > 0 {
            clock.cooldown -= 1;
        }
        if clock.cooldown > 0 || !flag(state, "enabled") {
            return;
        }
        clock.busy = Some(now);
        self.hopper_eject(pos, state);
    }

    fn hopper_amount(&self) -> u32 {
        self.config.world_keys.hopper_amount.max(1) as u32
    }

    fn hopper_finished(&mut self, hopper: BlockPos, changed: bool) {
        let transfer_cooldown = self.config.hopper_transfer_ticks;
        let check_cooldown = self.config.world_keys.hopper_check;
        let clock = self.falls.redstone.hoppers.entry(hopper).or_default();
        clock.busy = None;
        if changed {
            clock.cooldown = transfer_cooldown;
        } else if clock.cooldown <= 0 {
            clock.cooldown = check_cooldown;
        }
    }

    fn hopper_cancelled(&mut self, hopper: BlockPos) {
        let transfer_cooldown = self.config.hopper_transfer_ticks;
        self.falls
            .redstone
            .hoppers
            .entry(hopper)
            .or_default()
            .cooldown = transfer_cooldown;
    }

    fn hopper_eject(&mut self, pos: BlockPos, state: BlockStateId) {
        let Some(direction) = facing(state) else {
            return self.hopper_finished(pos, false);
        };
        let Some(me) = self.holder_at(pos) else {
            return self.hopper_finished(pos, false);
        };
        if is_empty(&self.holder_items(&me), &me.slots_toward(Face::Down)) {
            return self.hopper_start_suck(pos, false);
        }
        let at = step(pos, direction);
        let found = self.container_at(at);
        self.ask_hopper(HopperStep {
            hopper: pos,
            phase: Phase::Eject,
            at,
            found,
            changed: false,
            stage: Stage::Search,
            cart: None,
        });
    }

    fn hopper_start_suck(&mut self, pos: BlockPos, changed: bool) {
        let Some(me) = self.holder_at(pos) else {
            return self.hopper_finished(pos, changed);
        };
        if is_full(&self.holder_items(&me), &me.slots_toward(Face::Down)) {
            return self.hopper_finished(pos, changed);
        }
        let at = step(pos, Face::Up);
        let found = self.container_at(at);
        self.ask_hopper(HopperStep {
            hopper: pos,
            phase: Phase::Suck,
            at,
            found,
            changed,
            stage: Stage::Search,
            cart: None,
        });
    }

    pub(crate) fn ask_hopper(&mut self, hopper_step: HopperStep) {
        let lane = Lane::Block(hopper_step.hopper);
        let hopper = hopper_step.hopper;
        let cart = hopper_step.cart;
        let at = hopper_step.at;
        let phase = hopper_step.phase;
        let found = hopper_step.found;
        let stage = hopper_step.stage.clone();
        self.fire(
            lane,
            Continuation::Hopper(Box::new(hopper_step)),
            move |server| match stage {
                Stage::Search => Some(EventPayload::HopperInventorySearch(HopperInventorySearch {
                    block: hopper,
                    container_type: match phase {
                        Phase::Eject => HopperInventorySearchContainerType::Destination,
                        Phase::Suck => HopperInventorySearchContainerType::Source,
                    },
                    inventory: found.as_ref().map(Self::inventory_ref),
                    search_block: at,
                })),
                Stage::Move(transfer) => {
                    let me = server.hopper_holder(hopper, cart)?;
                    Some(EventPayload::InventoryMoveItem(InventoryMoveItem {
                        destination: Self::inventory_ref(&transfer.destination),
                        initiator: Self::inventory_ref(&me),
                        item: spec_of(&transfer.stack),
                        source: Self::inventory_ref(&transfer.source),
                    }))
                }
                Stage::Pickup(pickup) => {
                    let me = server.hopper_holder(hopper, cart)?;
                    Some(EventPayload::InventoryPickupItem(InventoryPickupItem {
                        inventory: Self::inventory_ref(&me),
                        item: server.entity_view(pickup.item)?,
                    }))
                }
            },
        );
    }

    pub(crate) fn finish_hopper_search(
        &mut self,
        mut hopper_step: HopperStep,
        patch: HopperInventorySearchPatch,
    ) {
        if let Some(replaced) = patch.inventory {
            hopper_step.found = replaced.and_then(|reference| match reference.owner {
                InventoryOwner::Block(pos) => self.holder_at(pos),
                InventoryOwner::Entity(id) => self.cart_holder(id),
                _ => None,
            });
        }
        match hopper_step.phase {
            Phase::Eject => self.hopper_push(hopper_step),
            Phase::Suck => self.hopper_pull(hopper_step),
        }
    }

    fn hopper_push(&mut self, search: HopperStep) {
        let hopper = search.hopper;
        let state = self.block_or_air(hopper);
        let (Some(direction), Some(destination), Some(me)) = (
            facing(state).map(Face::opposite),
            search.found,
            self.holder_at(hopper),
        ) else {
            return self.hopper_start_suck(hopper, search.changed);
        };
        let target = self.holder_items(&destination);
        let slots = destination.slots_toward(direction);
        if is_full(&target, &slots) {
            return self.hopper_start_suck(hopper, search.changed);
        }
        let own = self.holder_items(&me);
        let amount = self.hopper_amount();
        for slot in 0..HOPPER_SLOTS {
            let Some(one) = own
                .get(slot)
                .and_then(Option::as_ref)
                .and_then(|stack| with_count(stack, amount.min(u32::from(stack.count.get()))))
            else {
                continue;
            };
            if !takes_all(&target, &slots, &one, |at, stack| {
                destination.may_place(at, stack, direction, &target)
            }) {
                continue;
            }
            self.ask_hopper(HopperStep {
                stage: Stage::Move(Transfer {
                    source: me,
                    destination,
                    slot,
                    stack: one,
                }),
                ..search
            });
            return;
        }
        self.hopper_start_suck(hopper, search.changed);
    }

    fn hopper_pull(&mut self, search: HopperStep) {
        let hopper = search.hopper;
        let Some(me) = self.hopper_holder(hopper, search.cart) else {
            return self.hopper_finished(hopper, search.changed);
        };
        let own = self.holder_items(&me);
        let own_slots = me.slots_toward(Face::Down);
        let amount = self.hopper_amount();
        if let Some(source) = search.found {
            let items = self.holder_items(&source);
            for slot in source.slots_toward(Face::Down) {
                let Some(one) = items
                    .get(slot)
                    .and_then(Option::as_ref)
                    .filter(|stack| source.may_take(slot, stack, Face::Down))
                    .and_then(|stack| with_count(stack, amount.min(u32::from(stack.count.get()))))
                else {
                    continue;
                };
                if !takes_all(&own, &own_slots, &one, |_, _| true) {
                    continue;
                }
                self.ask_hopper(HopperStep {
                    stage: Stage::Move(Transfer {
                        source,
                        destination: me,
                        slot,
                        stack: one,
                    }),
                    ..search
                });
                return;
            }
            return self.hopper_finished(hopper, search.changed);
        }
        let mut candidates = self.items_in(&self.suck_areas(hopper, search.cart));
        candidates.sort_by_key(|(id, _)| id.0);
        for (id, stack) in candidates {
            let mut trial = own.clone();
            let rest = insert_stack(&mut trial, &own_slots, stack.clone(), |_, _| true);
            if rest.as_ref() == Some(&stack) {
                continue;
            }
            self.ask_hopper(HopperStep {
                stage: Stage::Pickup(Pickup { item: id, stack }),
                ..search
            });
            return;
        }
        self.hopper_finished(hopper, search.changed);
    }

    pub(crate) fn finish_hopper_move(
        &mut self,
        hopper_step: HopperStep,
        transfer: Transfer,
        cancelled: bool,
        patch: InventoryMoveItemPatch,
    ) {
        let HopperStep {
            hopper,
            phase,
            changed,
            cart,
            ..
        } = hopper_step;
        let proceed = |server: &mut Self, changed: bool| match phase {
            Phase::Eject => server.hopper_start_suck(hopper, changed),
            Phase::Suck => server.hopper_finished(hopper, changed),
        };
        if cancelled {
            self.hopper_cancelled(hopper);
            return proceed(self, changed);
        }
        let Transfer {
            source,
            destination,
            slot,
            stack,
        } = transfer;
        let moved = match patch.item.as_ref().map(stack_of) {
            Some(Ok(Some(replaced))) => replaced,
            _ => stack.clone(),
        };
        let mut from = self.holder_items(&source);
        let mut into = self.holder_items(&destination);
        let still_there = from
            .get(slot)
            .and_then(Option::as_ref)
            .is_some_and(|present| {
                present.item == stack.item
                    && present.components == stack.components
                    && present.count >= stack.count
            });
        if !still_there || self.hopper_holder(hopper, cart).is_none() {
            return proceed(self, changed);
        }
        let direction = match phase {
            Phase::Eject => facing(self.block_or_air(hopper)).map(Face::opposite),
            Phase::Suck => None,
        };
        let slots = destination.slots_toward(direction.unwrap_or(Face::Down));
        let was_empty = is_empty(&into, &slots);
        let before = into.clone();
        let leftover = insert_stack(&mut into, &slots, moved, |at, candidate| {
            direction.is_none_or(|face| destination.may_place(at, candidate, face, &before))
        });
        if leftover.is_some() {
            return proceed(self, changed);
        }
        if let Some(cell) = from.get_mut(slot) {
            let remaining = cell.as_ref().and_then(|present| {
                with_count(
                    present,
                    u32::from(present.count.get()).saturating_sub(u32::from(stack.count.get())),
                )
            });
            *cell = remaining;
        }
        self.store_holder(&source, &from);
        self.store_holder(&destination, &into);
        if was_empty && destination.is_hopper() {
            self.hopper_received(&source, destination.pos);
        }
        proceed(self, true);
    }

    fn hopper_received(&mut self, source: &Holder, destination: BlockPos) {
        let source_ticked = self
            .falls
            .redstone
            .hoppers
            .get(&source.pos)
            .filter(|_| source.is_hopper())
            .map(|clock| clock.ticked);
        let transfer_cooldown = self.config.hopper_transfer_ticks;
        let clock = self.falls.redstone.hoppers.entry(destination).or_default();
        if clock.cooldown > transfer_cooldown {
            return;
        }
        let behind = source_ticked.is_some_and(|ticked| clock.ticked >= ticked);
        clock.cooldown = transfer_cooldown - i32::from(behind);
    }

    pub(crate) fn finish_hopper_pickup(
        &mut self,
        hopper_step: HopperStep,
        pickup: Pickup,
        cancelled: bool,
    ) {
        let HopperStep {
            hopper,
            changed,
            cart,
            ..
        } = hopper_step;
        let Some(me) = self.hopper_holder(hopper, cart) else {
            return self.hopper_finished(hopper, changed);
        };
        let current = self.item_stack_of(pickup.item);
        if cancelled || current.as_ref() != Some(&pickup.stack) {
            return self.hopper_finished(hopper, changed);
        }
        let mut own = self.holder_items(&me);
        let slots = me.slots_toward(Face::Down);
        let was_empty = is_empty(&own, &slots);
        let leftover = insert_stack(&mut own, &slots, pickup.stack.clone(), |_, _| true);
        if leftover.as_ref() == Some(&pickup.stack) {
            return self.hopper_finished(hopper, changed);
        }
        self.store_holder(&me, &own);
        if was_empty {
            let transfer_cooldown = self.config.hopper_transfer_ticks;
            let clock = self.falls.redstone.hoppers.entry(hopper).or_default();
            if clock.cooldown <= transfer_cooldown {
                clock.cooldown = transfer_cooldown;
            }
        }
        match leftover {
            None => {
                self.remove_item(pickup.item);
                self.hopper_finished(hopper, true);
            }
            Some(rest) => {
                if let Some(entity) = self.entities.store.get_mut(pickup.item) {
                    item::set_stack(&mut entity.base, Some(rest));
                }
                self.hopper_finished(hopper, changed);
            }
        }
    }
}
