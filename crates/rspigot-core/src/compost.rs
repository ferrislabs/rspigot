use rand::Rng;
use rspigot_domain::composter;
use rspigot_domain::entity::species::villager::MerchantState;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, ItemId, Vec3};
use rspigot_entities::SoundSource;
use rspigot_items::ItemKind;

use crate::events::Hand;
use crate::kinds::villager::with_villager;
use crate::mobs::ServerRandom;
use crate::plugins::steps::Grief;
use crate::ports::ClientLink;
use crate::server::Server;

const COMPOSTABLE_ITEMS: [ItemKind; 2] = [ItemKind::WheatSeeds, ItemKind::BeetrootSeeds];
const ITEM_BUDGET: i32 = 20;
const ITEMS_KEPT: i32 = 10;
const PRODUCE_OFFSET: Vec3 = Vec3::new(0.5, 1.01, 0.5);
const PRODUCE_SPREAD: f32 = 0.7;
const PRODUCE_PICKUP_DELAY: u16 = 10;
const PRODUCE_LIFT: f64 = 0.2;
const PRODUCE_SCATTER: f64 = 0.2;

#[derive(Debug, Clone, Copy)]
struct Inner {
    slot: usize,
    remaining: i32,
}

#[derive(Debug, Clone, Copy)]
enum Pending {
    Extract,
    Insert(BlockStateId),
}

#[derive(Debug)]
pub(crate) struct CompostRun {
    villager: EntityId,
    pos: BlockPos,
    before: BlockStateId,
    state: BlockStateId,
    budget: i32,
    cursor: i32,
    totals: [i32; 2],
    inner: Option<Inner>,
    pending: Pending,
}

impl CompostRun {
    pub(crate) fn villager(&self) -> EntityId {
        self.villager
    }
}

fn compost_index(item: ItemId) -> Option<usize> {
    COMPOSTABLE_ITEMS.iter().position(|kind| kind.id() == item)
}

fn offset_random_xz(
    origin: Vec3,
    random: &mut impl rspigot_domain::random::RandomSource,
    spread: f32,
) -> Vec3 {
    let dx = f64::from((random.next_float() - 0.5) * spread);
    let dz = f64::from((random.next_float() - 0.5) * spread);
    Vec3::new(origin.x + dx, origin.y, origin.z + dz)
}

fn stack_at(merchant: &MerchantState, slot: usize) -> Option<(ItemId, i32)> {
    merchant
        .inventory
        .get(slot)?
        .as_ref()
        .map(|stack: &ItemStack| (stack.item, i32::from(stack.count.get())))
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn villager_spend(&mut self, id: EntityId, kind: ItemKind) -> bool {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return false;
        };
        with_villager(entity, |_, _, merchant| {
            let slot = merchant
                .inventory
                .iter()
                .position(|stack| stack.as_ref().is_some_and(|stack| stack.item == kind.id()));
            slot.is_some_and(|slot| merchant.remove_item(slot, 1).is_some())
        })
        .unwrap_or(false)
    }

    pub(crate) fn villager_work_at_composter(&mut self, id: EntityId, pos: BlockPos) {
        let Some(state) = self
            .world
            .chunks
            .block(pos)
            .filter(|state| composter::is_composter(*state))
        else {
            return;
        };
        self.villager_make_bread(id);
        let size = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| with_villager(entity, |_, _, merchant| merchant.inventory.len()))
            .unwrap_or(0);
        let run = CompostRun {
            villager: id,
            pos,
            before: state,
            state,
            budget: ITEM_BUDGET,
            cursor: size as i32,
            totals: [0; 2],
            inner: None,
            pending: Pending::Extract,
        };
        if composter::level(state) == Some(composter::READY_LEVEL) {
            self.compost_extract(run);
        } else {
            self.compost_loop(run);
        }
    }

    fn compost_extract(&mut self, mut run: CompostRun) {
        let Some(view) = self.entity_view(run.villager) else {
            return;
        };
        let to = composter::emptied(run.state);
        run.pending = Pending::Extract;
        let pos = run.pos;
        self.change_block(view, pos, to, Grief::Compost(Box::new(run)), false);
    }

    pub(crate) fn finish_compost(&mut self, mut run: CompostRun, cancelled: bool) {
        if self.entities.store.get(run.villager).is_none() {
            return;
        }
        match run.pending {
            Pending::Extract => {
                if !cancelled {
                    self.extract_produce(run.pos, run.state);
                    run.state = composter::emptied(run.state);
                }
                run.before = run.state;
                self.compost_loop(run);
            }
            Pending::Insert(to) => {
                if !cancelled {
                    self.set_block(run.pos, to);
                    if let Some(inner) = run.inner {
                        self.villager_shrink_slot(run.villager, inner.slot);
                    }
                    run.state = to;
                }
                self.compost_after_insert(run);
            }
        }
    }

    fn villager_shrink_slot(&mut self, id: EntityId, slot: usize) {
        if let Some(entity) = self.entities.store.get_mut(id) {
            with_villager(entity, |_, _, merchant| merchant.remove_item(slot, 1));
        }
    }

    fn compost_after_insert(&mut self, mut run: CompostRun) {
        if composter::level(run.state) == Some(composter::MAX_FILL_LEVEL) {
            self.compost_effects(&run);
            return;
        }
        if let Some(inner) = run.inner.as_mut() {
            inner.remaining -= 1;
        }
        self.compost_loop(run);
    }

    fn compost_effects(&mut self, run: &CompostRun) {
        self.level_event(
            composter::FILL_EVENT,
            run.pos,
            i32::from(run.state != run.before),
        );
    }

    fn compost_loop(&mut self, mut run: CompostRun) {
        loop {
            if let Some(inner) = run.inner {
                if inner.remaining <= 0 {
                    run.inner = None;
                    continue;
                }
                match self.compost_iteration(&run, inner.slot) {
                    Some(to) => {
                        let Some(view) = self.entity_view(run.villager) else {
                            return;
                        };
                        run.pending = Pending::Insert(to);
                        let pos = run.pos;
                        self.change_block(view, pos, to, Grief::Compost(Box::new(run)), false);
                        return;
                    }
                    None => {
                        if composter::level(run.state) == Some(composter::MAX_FILL_LEVEL) {
                            self.compost_effects(&run);
                            return;
                        }
                        if let Some(inner) = run.inner.as_mut() {
                            inner.remaining -= 1;
                        }
                        continue;
                    }
                }
            }
            run.cursor -= 1;
            if run.cursor < 0 || run.budget <= 0 {
                self.compost_effects(&run);
                return;
            }
            let slot = run.cursor as usize;
            let stack = self
                .entities
                .store
                .get_mut(run.villager)
                .and_then(|entity| with_villager(entity, |_, _, merchant| stack_at(merchant, slot)))
                .flatten();
            let Some((item, count)) = stack else {
                continue;
            };
            let Some(index) = compost_index(item) else {
                continue;
            };
            run.totals[index] += count;
            let amount = (run.totals[index] - ITEMS_KEPT).min(run.budget).min(count);
            if amount > 0 {
                run.budget -= amount;
                run.inner = Some(Inner {
                    slot,
                    remaining: amount,
                });
            }
        }
    }

    fn compost_iteration(&mut self, run: &CompostRun, slot: usize) -> Option<BlockStateId> {
        let level = composter::level(run.state)?;
        if level > composter::MAX_FILL_LEVEL - 1 {
            return None;
        }
        let item = self
            .entities
            .store
            .get_mut(run.villager)
            .and_then(|entity| with_villager(entity, |_, _, merchant| stack_at(merchant, slot)))
            .flatten()
            .map(|(item, _)| item)?;
        let mut random = ServerRandom(&mut self.rng);
        let count = composter::layer_count(item, level, &mut random)?;
        let to = composter::add_layer(run.state, level, count);
        (to != run.state).then_some(to)
    }

    fn extract_produce(&mut self, pos: BlockPos, state: BlockStateId) {
        let origin = Vec3::new(
            f64::from(pos.x) + PRODUCE_OFFSET.x,
            f64::from(pos.y) + PRODUCE_OFFSET.y,
            f64::from(pos.z) + PRODUCE_OFFSET.z,
        );
        let at = {
            let mut random = ServerRandom(&mut self.rng);
            offset_random_xz(origin, &mut random, PRODUCE_SPREAD)
        };
        let velocity = Vec3::new(
            self.rng.random::<f64>() * PRODUCE_SCATTER - PRODUCE_SCATTER / 2.0,
            PRODUCE_LIFT,
            self.rng.random::<f64>() * PRODUCE_SCATTER - PRODUCE_SCATTER / 2.0,
        );
        let stack = ItemStack::new(ItemKind::BoneMeal.id(), std::num::NonZeroU8::MIN);
        self.spawn_item_entity(at, velocity, PRODUCE_PICKUP_DELAY, stack);
        self.set_block(pos, composter::emptied(state));
        let center = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.5,
            f64::from(pos.z) + 0.5,
        );
        self.play_sound(
            None,
            center,
            composter::EMPTY_SOUND,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
    }

    pub(crate) fn composter_ready(&mut self, pos: BlockPos, state: BlockStateId) {
        if !composter::needs_ready_tick(state) {
            return;
        }
        self.set_block(pos, composter::made_ready(state));
        let center = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.5,
            f64::from(pos.z) + 0.5,
        );
        self.play_sound(
            None,
            center,
            composter::READY_SOUND,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
    }

    pub(crate) fn use_composter(
        &mut self,
        id: PlayerId,
        hand: Hand,
        held: Option<&ItemStack>,
        pos: BlockPos,
        clicked: BlockStateId,
        sequence: i32,
    ) -> bool {
        let Some(level) = composter::level(clicked) else {
            return false;
        };
        let compostable = held.filter(|stack| composter::is_compostable(stack.item));
        if let Some(stack) = compostable
            && level < composter::READY_LEVEL
        {
            if level < composter::MAX_FILL_LEVEL {
                let mut random = ServerRandom(&mut self.rng);
                let count = composter::layer_count(stack.item, level, &mut random).unwrap_or(0);
                let to = composter::add_layer(clicked, level, count);
                self.set_block(pos, to);
                self.level_event(composter::FILL_EVENT, pos, i32::from(to != clicked));
                self.consume_held(id, hand, 1);
                self.refresh_hands(id);
            }
            self.player_swing(id, hand, false);
            self.ack_block(id, sequence);
            return true;
        }
        if level == composter::READY_LEVEL {
            self.extract_produce(pos, clicked);
            self.player_swing(id, hand, false);
            self.ack_block(id, sequence);
            return true;
        }
        false
    }
}
