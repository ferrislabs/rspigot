use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::crafting;
use rspigot_domain::item::{ItemStack, Slot, with_count};
use rspigot_domain::{BlockPos, BlockStateId, Vec3};

use super::storage::insert_stack;
use super::{face_named, step};
use crate::items::PICKUP_DELAY;
use crate::plugins::events::CrafterCraft;
use crate::plugins::{Continuation, EventPayload, ItemSpec, Lane, spec_of, stack_of};
use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::flag;

const CRAFT_DELAY: u64 = 4;
const CRAFTING_TICKS: u64 = 6;
const FAIL_EVENT: i32 = 1050;
const CRAFT_EVENT: i32 = 1049;
const GRID: usize = 3;
const DISPENSE_SPEED: f64 = 0.2;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CrafterStep {
    pub(crate) pos: BlockPos,
    pub(crate) recipe: &'static str,
    pub(crate) result: ItemStack,
    pub(crate) remaining: Vec<ItemStack>,
}

fn front_of(state: BlockStateId) -> Face {
    state
        .property("orientation")
        .and_then(|value| value.split('_').next())
        .and_then(face_named)
        .unwrap_or(Face::North)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn crafter_neighbor_changed(&mut self, pos: BlockPos, state: BlockStateId) {
        let powered = self.has_neighbor_signal(pos);
        let triggered = flag(state, "triggered");
        if powered && !triggered {
            self.schedule(pos, BlockKind::Crafter, CRAFT_DELAY);
            if let Some(next) = state.with_property("triggered", "true") {
                self.set_block_quiet(pos, next);
            }
        } else if !powered
            && triggered
            && let Some(next) = state.with_property("triggered", "false")
        {
            self.set_block_quiet(pos, next);
        }
    }

    pub(crate) fn crafter_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        if flag(state, "crafting") {
            if let Some(next) = state.with_property("crafting", "false") {
                self.set_block_quiet(pos, next);
            }
            return;
        }
        let Some(me) = self.holder_at(pos) else {
            return;
        };
        let items = self.holder_items(&me);
        let Some(recipe) = crafting::recipe(GRID, GRID, &items) else {
            self.level_event(FAIL_EVENT, pos, 0);
            return;
        };
        let Some(result) = crafting::result(GRID, GRID, &items) else {
            self.level_event(FAIL_EVENT, pos, 0);
            return;
        };
        let remaining: Vec<ItemStack> = items
            .iter()
            .flatten()
            .filter_map(|stack| stack.item.kind()?.crafting_remainder())
            .map(|item| ItemStack::new(item, std::num::NonZeroU8::MIN))
            .collect();
        let preview = spec_of(&result);
        let leftovers: Vec<ItemSpec> = remaining.iter().map(spec_of).collect();
        let step = CrafterStep {
            pos,
            recipe: recipe.id,
            result,
            remaining,
        };
        self.fire(
            Lane::Block(pos),
            Continuation::Crafter(Box::new(step)),
            move |_| {
                Some(EventPayload::CrafterCraft(CrafterCraft {
                    block: pos,
                    recipe: recipe.id.to_owned(),
                    remaining_items: leftovers,
                    result: preview,
                }))
            },
        );
    }

    pub(crate) fn finish_crafter(
        &mut self,
        step: CrafterStep,
        cancelled: bool,
        result: Option<ItemSpec>,
    ) {
        if cancelled {
            return;
        }
        let state = self.block_or_air(step.pos);
        if state.kind() != Some(BlockKind::Crafter) {
            return;
        }
        let Some(me) = self.holder_at(step.pos) else {
            return;
        };
        let mut items = self.holder_items(&me);
        if crafting::recipe(GRID, GRID, &items).map(|recipe| recipe.id) != Some(step.recipe) {
            return;
        }
        let produced = match result.as_ref().map(stack_of) {
            Some(Ok(replaced)) => replaced,
            _ => Some(step.result.clone()),
        };
        for cell in &mut items {
            let shrunk: Slot = cell
                .as_ref()
                .and_then(|stack| with_count(stack, u32::from(stack.count.get()) - 1));
            *cell = shrunk;
        }
        self.store_holder(&me, &items);
        if let Some(crafting_state) = state.with_property("crafting", "true") {
            self.set_block_quiet(step.pos, crafting_state);
            self.schedule(step.pos, BlockKind::Crafter, CRAFTING_TICKS);
        }
        let front = front_of(state);
        for stack in produced.into_iter().chain(step.remaining) {
            self.crafter_output(step.pos, front, stack);
        }
        self.level_event(CRAFT_EVENT, step.pos, 0);
    }

    fn crafter_output(&mut self, pos: BlockPos, front: Face, stack: ItemStack) {
        let target = step(pos, front);
        if let Some(destination) = self.container_at(target) {
            let mut into = self.holder_items(&destination);
            let toward = front.opposite();
            let slots = destination.slots_toward(toward);
            let before = into.clone();
            let leftover = insert_stack(&mut into, &slots, stack, |at, candidate| {
                destination.may_place(at, candidate, toward, &before)
            });
            self.store_holder(&destination, &into);
            if let Some(rest) = leftover {
                self.crafter_drop(pos, front, rest);
            }
            return;
        }
        self.crafter_drop(pos, front, stack);
    }

    fn crafter_drop(&mut self, pos: BlockPos, front: Face, stack: ItemStack) {
        let (dx, dy, dz) = front.offset();
        let toward = Vec3::new(f64::from(dx), f64::from(dy), f64::from(dz));
        let spot = Vec3::new(
            f64::from(pos.x) + 0.5 + 0.7 * toward.x,
            f64::from(pos.y) + 0.5 + 0.7 * toward.y - 0.15625,
            f64::from(pos.z) + 0.5 + 0.7 * toward.z,
        );
        let velocity = Vec3::new(
            toward.x * DISPENSE_SPEED,
            DISPENSE_SPEED,
            toward.z * DISPENSE_SPEED,
        );
        self.spawn_item_entity(spot, velocity, PICKUP_DELAY, stack);
    }
}
