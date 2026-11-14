use std::num::NonZeroU8;

use rspigot_ai::{
    ADMIRE_DURATION, ATE_RECENTLY_TICKS, BrainMessage, Hand as AiHand, MemoryModuleType,
    MemoryValue, MobBody, MobView, PIGLIN_INVENTORY_SIZE, PiglinAction,
};
use rspigot_domain::entity::item as item_entity;
use rspigot_domain::entity::living::{self, slot_index};
use rspigot_domain::entity::save::{item_stack_from_nbt, item_stack_to_nbt};
use rspigot_domain::entity::species::{Species, piglin};
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::random::RandomSource;
use rspigot_domain::world::Difficulty;
use rspigot_domain::{EntityId, ItemId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::{EquipmentSlot, ItemKind};
use rspigot_nbt::Tag;

use crate::events::{ClientEvent, Hand};
use crate::health::DamageSource;
use crate::interact::Interaction;
use crate::loot::{self, LootContext};
use crate::mobs::{ServerRandom, sky_darken};
use crate::plugins::events::{EntityPickupItem, PiglinBarter};
use crate::plugins::steps::{BarterStep, PiglinPickupStep};
use crate::plugins::{Continuation, EventKind, EventPayload, spec_of};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const INVENTORY: &str = "Inventory";
const BARTERING_TABLE: &str = "minecraft:gameplay/piglin_bartering";
const CONVERTED_SOUND: &str = "minecraft:entity.piglin.converted_to_zombified";
const BRUTE_CONVERTED_SOUND: &str = "minecraft:entity.piglin_brute.converted_to_zombified";
const GUARANTEED_DROP: f32 = 2.0;
const THROW_SPEED: f64 = 0.3;
const HAND_DISTANCE_FROM_EYE: f64 = 0.3;
const THROW_TARGET_LIFT: f64 = 1.0;
const PICKUP_DELAY: u16 = 10;
const PICKUP_REACH: f64 = 1.0;
const NEARBY_THROW_HORIZONTAL: i32 = 4;
const NEARBY_THROW_VERTICAL: i32 = 2;
const DROP_SPREAD: f64 = 0.2;
const DROP_LIFT: f64 = 0.2;
const REPLACED_DROP_MARGIN: f32 = 0.1;
const MAX_STACK_FALLBACK: u8 = 64;
const ANGER_REACH: f64 = 16.0;

fn piglin_state(entity: &mut Entity) -> Option<&mut piglin::PiglinState> {
    match entity
        .living_mut()
        .and_then(|state| state.mob.as_mut())
        .map(|mob| &mut mob.species)
    {
        Some(Species::Piglin(state)) => Some(state),
        _ => None,
    }
}

pub(crate) fn read_inventory(entity: &Entity) -> Vec<ItemStack> {
    entity
        .extra
        .get_list(INVENTORY)
        .map(|list| {
            list.iter()
                .filter_map(Tag::as_compound)
                .filter_map(item_stack_from_nbt)
                .take(PIGLIN_INVENTORY_SIZE)
                .collect()
        })
        .unwrap_or_default()
}

fn write_inventory(entity: &mut Entity, items: &[ItemStack]) {
    if items.is_empty() && !entity.extra.contains_key(INVENTORY) {
        return;
    }
    entity.extra.insert(
        INVENTORY,
        Tag::List(
            items
                .iter()
                .map(|stack| Tag::Compound(item_stack_to_nbt(stack)))
                .collect(),
        ),
    );
}

fn add_to_inventory(items: &mut Vec<ItemStack>, mut stack: ItemStack) -> Option<ItemStack> {
    let max = stack
        .item
        .kind()
        .map_or(MAX_STACK_FALLBACK, ItemKind::max_stack_size);
    for held in items.iter_mut() {
        if held.item == stack.item && held.components == stack.components {
            let room = max.saturating_sub(held.count.get());
            let moved = room.min(stack.count.get());
            if moved > 0 {
                held.count = NonZeroU8::new(held.count.get() + moved).unwrap_or(held.count);
                stack.count = NonZeroU8::new(stack.count.get() - moved)?;
            }
        }
    }
    if items.len() < PIGLIN_INVENTORY_SIZE {
        items.push(stack);
        return None;
    }
    Some(stack)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn piglin_view(&self, entity: &Entity, view: &mut MobView) {
        let Some(state) = entity.living() else {
            return;
        };
        let facts = &mut view.species.piglin;
        let Some(Species::Piglin(piglin_state)) = state.mob.as_ref().map(|mob| mob.species) else {
            return;
        };
        facts.cannot_hunt = piglin_state.cannot_hunt;
        facts.dancing = piglin::is_dancing(&entity.base);
        facts.converting =
            self.nether_mob_converting(entity, piglin::is_immune_to_zombification(&entity.base));
        facts.feet = state.equipment(EquipmentSlot::Feet).map(|s| s.item);
        facts.legs = state.equipment(EquipmentSlot::Legs).map(|s| s.item);
        facts.chest = state.equipment(EquipmentSlot::Chest).map(|s| s.item);
        facts.head = state.equipment(EquipmentSlot::Head).map(|s| s.item);
        for (slot, stack) in facts
            .inventory
            .iter_mut()
            .zip(read_inventory(entity).iter())
        {
            *slot = Some((stack.item, stack.count.get()));
        }
        if let Some(charge) = self.crossbow_view_charge(entity) {
            view.species.crossbow.charge_ticks = charge;
        }
    }

    pub(crate) fn piglin_post_tick(&mut self, _id: EntityId) {}

    pub(crate) fn piglin_registered(&mut self, id: EntityId, ai: &mut rspigot_ai::MobAi) {
        let dimension = self.world.dimension.key.as_str().to_owned();
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let position = entity.base.position.block();
        let (cooldown, home) = match piglin_state(entity) {
            Some(state) => (
                state.pending_hunt_cooldown.take(),
                std::mem::take(&mut state.needs_home),
            ),
            None => (None, false),
        };
        let Some(brain) = ai.behavior_brain_mut() else {
            return;
        };
        if let Some(ticks) = cooldown {
            brain.memories_mut().set_with_expiry(
                MemoryModuleType::HuntedRecently,
                MemoryValue::Bool(true),
                i64::from(ticks),
            );
        }
        if home {
            brain.memories_mut().set(
                MemoryModuleType::Home,
                MemoryValue::GlobalPos(rspigot_ai::GlobalPos {
                    dimension,
                    pos: position,
                }),
            );
        }
    }

    pub(crate) fn piglin_custom_ai_step(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let kind = entity.base.kind;
        let immune = piglin::is_immune_to_zombification(&entity.base);
        let converting = self.nether_mob_converting(entity, immune);
        let peaceful = self.world.difficulty == Difficulty::Peaceful;
        let mut convert = false;
        if let Some(state) = self.entities.store.get_mut(id).and_then(piglin_state) {
            if converting {
                state.time_in_overworld += 1;
            } else {
                state.time_in_overworld = 0;
            }
            convert = state.time_in_overworld > piglin::CONVERSION_TICKS;
        }
        if convert {
            if !peaceful {
                let sound = if kind == Kind::PiglinBrute {
                    BRUTE_CONVERTED_SOUND
                } else {
                    CONVERTED_SOUND
                };
                self.play_mob_voice(id, move |_| Some(sound));
            }
            if kind == Kind::Piglin {
                self.piglin_cancel_admiring(id);
                self.piglin_drop_inventory(id);
            }
            self.convert_mob(id, Kind::ZombifiedPiglin, None);
            return;
        }
        self.piglin_pick_up_loot(id);
    }

    pub(crate) fn piglin_action(&mut self, id: EntityId, action: PiglinAction) {
        match action {
            PiglinAction::StopHoldingOffHand { bartering_enabled } => {
                self.piglin_stop_holding_off_hand(id, bartering_enabled);
            }
            PiglinAction::SetDancing(dancing) => {
                if let Some(entity) = self.entities.store.get_mut(id) {
                    piglin::set_dancing(&mut entity.base, dancing);
                }
            }
        }
    }

    pub(crate) fn piglin_after_hurt(&mut self, id: EntityId, source: &DamageSource, dead: bool) {
        if dead {
            return;
        }
        let Some(attacker) = source
            .causing
            .as_ref()
            .filter(|cause| cause.kind.living())
            .map(|cause| cause.entity)
        else {
            return;
        };
        let message = if self.kind_of(id) == Some(Kind::Piglin) {
            BrainMessage::PiglinHurtBy { attacker }
        } else {
            BrainMessage::PiglinBruteHurtBy { attacker }
        };
        self.brain_message(id, message);
    }

    pub(crate) fn piglin_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        if self.kind_of(target) != Some(Kind::Piglin) {
            return None;
        }
        let held = held?;
        let view = self.mob_view(target)?;
        let admires = self
            .entities
            .mobs
            .entries
            .get(&target)?
            .ai
            .piglin_can_admire(&view, held.item);
        if !admires {
            return None;
        }
        let taken = ItemStack {
            count: NonZeroU8::MIN,
            ..held.clone()
        };
        self.consume_held(player, hand, 1);
        self.piglin_hold_in_off_hand(target, taken);
        self.piglin_admire_gold_item(target);
        self.piglin_stop_walking(target);
        Some(Interaction::Predicted)
    }

    pub fn anger_nearby_piglins(&mut self, player: PlayerId, only_if_they_see_the_player: bool) {
        let Some(entity) = self.player_entity_id(player) else {
            return;
        };
        let Some(area) = self
            .entities
            .mobs
            .players
            .iter()
            .find(|candidate| candidate.id == entity)
            .map(|player| {
                rspigot_ai::inflate(&player.bounds, ANGER_REACH, ANGER_REACH, ANGER_REACH)
            })
        else {
            return;
        };
        let piglins: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|other| other.base.kind == Kind::Piglin && !other.base.is_removed())
            .filter(|other| other.base.bounding_box().intersects(&area))
            .map(|other| other.base.id)
            .collect();
        for piglin in piglins {
            self.brain_message(
                piglin,
                BrainMessage::PiglinAngerFromPlayer {
                    player: entity,
                    only_if_sees: only_if_they_see_the_player,
                },
            );
        }
    }

    fn piglin_admire_gold_item(&mut self, id: EntityId) {
        self.with_memories(id, |memories| {
            memories.set_with_expiry(
                MemoryModuleType::AdmiringItem,
                MemoryValue::Bool(true),
                i64::from(ADMIRE_DURATION),
            );
        });
    }

    fn piglin_stop_walking(&mut self, id: EntityId) {
        self.with_memories(id, |memories| memories.erase(MemoryModuleType::WalkTarget));
        if let Some(entry) = self.entities.mobs.entries.get_mut(&id) {
            entry.ai.brain_mut().navigation.stop();
        }
    }

    pub(crate) fn with_memories(
        &mut self,
        id: EntityId,
        update: impl FnOnce(&mut rspigot_ai::Memories),
    ) {
        if let Some(brain) = self
            .entities
            .mobs
            .entries
            .get_mut(&id)
            .and_then(|entry| entry.ai.behavior_brain_mut())
        {
            update(brain.memories_mut());
        }
    }

    fn piglin_memory_present(&self, id: EntityId, memory: MemoryModuleType) -> bool {
        self.entities
            .mobs
            .entries
            .get(&id)
            .and_then(|entry| entry.ai.behavior_brain())
            .is_some_and(|brain| brain.memories().has_value(memory))
    }

    fn piglin_off_hand(&self, id: EntityId) -> Option<ItemStack> {
        self.entities
            .store
            .get(id)?
            .living()?
            .equipment(EquipmentSlot::Offhand)
            .cloned()
    }

    fn piglin_cancel_admiring(&mut self, id: EntityId) {
        if !self.piglin_memory_present(id, MemoryModuleType::AdmiringItem) {
            return;
        }
        if let Some(stack) = self.piglin_off_hand(id) {
            self.mob_spawn_at_location(id, stack);
            self.set_equipment_stack(id, EquipmentSlot::Offhand, None);
        }
    }

    fn piglin_drop_inventory(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let items = read_inventory(entity);
        write_inventory(entity, &[]);
        for stack in items {
            self.mob_spawn_at_location(id, stack);
        }
    }

    pub(crate) fn piglin_death_drops(&mut self, id: EntityId) -> Vec<ItemStack> {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return Vec::new();
        };
        let items = read_inventory(entity);
        write_inventory(entity, &[]);
        items
    }

    pub(crate) fn mob_spawn_at_location(&mut self, id: EntityId, stack: ItemStack) {
        let Some(position) = self.entities.store.get(id).map(|e| e.base.position) else {
            return;
        };
        let mut random = ServerRandom(&mut self.rng);
        let velocity = Vec3::new(
            random.next_double() * DROP_SPREAD - DROP_SPREAD / 2.0,
            DROP_LIFT,
            random.next_double() * DROP_SPREAD - DROP_SPREAD / 2.0,
        );
        self.spawn_item_entity(position, velocity, PICKUP_DELAY, stack);
    }

    fn piglin_hold_in_off_hand(&mut self, id: EntityId, stack: ItemStack) {
        if let Some(previous) = self.piglin_off_hand(id) {
            self.mob_spawn_at_location(id, previous);
        }
        let barter = piglin_is_barter(&stack);
        self.set_equipment_stack(id, EquipmentSlot::Offhand, Some(stack));
        if let Some(mob) = self
            .entities
            .store
            .get_mut(id)
            .and_then(Entity::living_mut)
            .and_then(|living| living.mob.as_mut())
        {
            mob.drop_chances[slot_index(EquipmentSlot::Offhand)] = GUARANTEED_DROP;
            if !barter {
                mob.persistence_required = true;
            }
        }
    }

    fn piglin_hold_in_main_hand(&mut self, id: EntityId, stack: ItemStack) {
        self.set_equipment_stack(id, EquipmentSlot::Mainhand, Some(stack));
        if let Some(mob) = self
            .entities
            .store
            .get_mut(id)
            .and_then(Entity::living_mut)
            .and_then(|living| living.mob.as_mut())
        {
            mob.drop_chances[slot_index(EquipmentSlot::Mainhand)] = GUARANTEED_DROP;
            mob.persistence_required = true;
        }
    }

    fn piglin_equip_if_possible(&mut self, id: EntityId, stack: &ItemStack) -> bool {
        let slot = rspigot_ai::equipment_slot_for(stack.item);
        let Some(view) = self.mob_view(id) else {
            return false;
        };
        let replace = rspigot_ai::can_replace_current_item(&view, stack.item);
        if !replace {
            return false;
        }
        let current = self
            .entities
            .store
            .get(id)
            .and_then(Entity::living)
            .and_then(|living| living.equipment(slot).cloned());
        let drop_chance = self
            .entities
            .store
            .get(id)
            .and_then(Entity::living)
            .and_then(|living| living.mob.as_ref())
            .map_or(0.0, |mob| mob.drop_chances[slot_index(slot)]);
        if let Some(current) = current {
            let roll = ServerRandom(&mut self.rng).next_float();
            if (roll - REPLACED_DROP_MARGIN).max(0.0) < drop_chance {
                self.mob_spawn_at_location(id, current);
            }
        }
        self.set_equipment_stack(id, slot, Some(stack.clone()));
        if let Some(mob) = self
            .entities
            .store
            .get_mut(id)
            .and_then(Entity::living_mut)
            .and_then(|living| living.mob.as_mut())
        {
            mob.drop_chances[slot_index(slot)] = GUARANTEED_DROP;
        }
        true
    }

    fn piglin_put_in_inventory(&mut self, id: EntityId, stack: ItemStack) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let mut items = read_inventory(entity);
        let rest = add_to_inventory(&mut items, stack);
        write_inventory(entity, &items);
        if let Some(rest) = rest {
            let target = self.piglin_random_nearby_position(id);
            self.piglin_throw_items_toward(id, vec![rest], target);
        }
    }

    fn piglin_random_nearby_position(&mut self, id: EntityId) -> Vec3 {
        let fallback = self
            .entities
            .store
            .get(id)
            .map_or(Vec3::ZERO, |entity| entity.base.position);
        let Some(view) = self.mob_view(id) else {
            return fallback;
        };
        let Some(mut entry) = self.entities.mobs.entries.remove(&id) else {
            return fallback;
        };
        let found = {
            let world = self.mob_world(sky_darken(&self.world.level));
            let mut body = MobBody::of(&view);
            entry.ai.random_nearby_position(
                &view,
                &world,
                &mut body,
                NEARBY_THROW_HORIZONTAL,
                NEARBY_THROW_VERTICAL,
            )
        };
        self.entities.mobs.entries.insert(id, entry);
        found.unwrap_or(fallback)
    }

    fn piglin_throw_items(&mut self, id: EntityId, items: Vec<ItemStack>) {
        let player = self
            .entities
            .mobs
            .entries
            .get(&id)
            .and_then(|entry| entry.ai.behavior_brain())
            .and_then(|brain| {
                brain
                    .memories()
                    .get(MemoryModuleType::NearestVisiblePlayer)
                    .and_then(MemoryValue::as_entity)
            })
            .and_then(|player| self.entities.mobs.players.iter().find(|p| p.id == player))
            .map(|player| player.position);
        let target = match player {
            Some(position) => position,
            None => self.piglin_random_nearby_position(id),
        };
        self.piglin_throw_items_toward(id, items, target);
    }

    fn piglin_throw_items_toward(&mut self, id: EntityId, items: Vec<ItemStack>, target: Vec3) {
        if items.is_empty() {
            return;
        }
        self.mob_swing(id, AiHand::Off);
        for stack in items {
            self.throw_item(id, stack, target + Vec3::new(0.0, THROW_TARGET_LIFT, 0.0));
        }
    }

    fn throw_item(&mut self, thrower: EntityId, stack: ItemStack, target: Vec3) {
        let Some(entity) = self.entities.store.get(thrower) else {
            return;
        };
        let position = entity.base.position;
        let eye = entity.base.eye_y();
        let origin = Vec3::new(position.x, eye - HAND_DISTANCE_FROM_EYE, position.z);
        let velocity =
            (target - position)
                .normalize()
                .multiply(THROW_SPEED, THROW_SPEED, THROW_SPEED);
        self.spawn_item_entity(origin, velocity, PICKUP_DELAY, stack);
    }

    fn piglin_stop_holding_off_hand(&mut self, id: EntityId, bartering_enabled: bool) {
        let Some(stack) = self.piglin_off_hand(id) else {
            return;
        };
        self.set_equipment_stack(id, EquipmentSlot::Offhand, None);
        let adult = self
            .entities
            .store
            .get(id)
            .is_some_and(|entity| !entity.is_baby());
        let barter = piglin_is_barter(&stack);
        if adult {
            if bartering_enabled && barter {
                let outcome = self.barter_response_items();
                self.ask_barter(id, stack, outcome);
            } else if !barter && !self.piglin_equip_if_possible(id, &stack) {
                self.piglin_put_in_inventory(id, stack);
            }
            return;
        }
        if !self.piglin_equip_if_possible(id, &stack) {
            let main = self
                .entities
                .store
                .get(id)
                .and_then(Entity::living)
                .and_then(|living| living.equipment(EquipmentSlot::Mainhand).cloned());
            if let Some(main) = main {
                if rspigot_ai::is_loved_item(main.item) {
                    self.piglin_put_in_inventory(id, main);
                } else {
                    self.piglin_throw_items(id, vec![main]);
                }
            }
            self.piglin_hold_in_main_hand(id, stack);
        }
    }

    fn ask_barter(&mut self, id: EntityId, input: ItemStack, outcome: Vec<ItemStack>) {
        let Some(view) = self.entity_view(id) else {
            return;
        };
        let described = self
            .listened(EventKind::PiglinBarter)
            .then(|| outcome.iter().map(spec_of).collect::<Vec<_>>());
        let input = spec_of(&input);
        let lane = self.lane_for(None, id);
        let step = BarterStep {
            piglin: id,
            outcome,
        };
        self.fire(lane, Continuation::Barter(Box::new(step)), move |_| {
            Some(EventPayload::PiglinBarter(PiglinBarter {
                entity_type: view.kind.clone(),
                entity: view,
                input,
                outcome: described?,
            }))
        });
    }

    pub(crate) fn finish_barter(&mut self, step: BarterStep, cancelled: bool) {
        let BarterStep { piglin, outcome } = step;
        if cancelled || self.entities.store.get(piglin).is_none() {
            return;
        }
        self.piglin_throw_items(piglin, outcome);
    }

    fn barter_response_items(&mut self) -> Vec<ItemStack> {
        let Some(table) = rspigot_data::loot::table(BARTERING_TABLE) else {
            return Vec::new();
        };
        loot::table_drops(table, &LootContext::default(), &mut self.rng)
    }

    fn piglin_pick_up_loot(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(living) = entity.living() else {
            return;
        };
        let can_pick = living.mob.as_ref().is_some_and(|mob| mob.can_pick_up_loot);
        if !can_pick
            || entity.base.is_removed()
            || living::is_dead_or_dying(&entity.base, living)
            || !self.config.rules.mob_griefing
        {
            return;
        }
        let area =
            rspigot_ai::inflate(&entity.base.bounding_box(), PICKUP_REACH, 0.0, PICKUP_REACH);
        let candidates: Vec<(EntityId, ItemId)> = self
            .entities
            .store
            .iter()
            .filter_map(|other| {
                let EntityState::Item(state) = &other.state else {
                    return None;
                };
                if other.base.is_removed()
                    || state.pickup_delay > 0
                    || !other.base.bounding_box().intersects(&area)
                {
                    return None;
                }
                Some((other.base.id, item_entity::stack(&other.base)?.item))
            })
            .collect();
        if candidates.is_empty() {
            return;
        }
        let Some(view) = self.mob_view(id) else {
            return;
        };
        for (item, kind) in candidates {
            let wants = self
                .entities
                .mobs
                .entries
                .get(&id)
                .is_some_and(|entry| match view.kind {
                    Kind::PiglinBrute => kind == ItemKind::GoldenAxe.id(),
                    _ => entry.ai.piglin_wants_to_pickup(&view, kind),
                });
            if wants {
                self.piglin_pick_up_item(id, item);
            }
        }
    }

    fn piglin_pick_up_item(&mut self, id: EntityId, item: EntityId) {
        self.piglin_stop_walking(id);
        let Some(stack) = self.item_stack_of(item) else {
            return;
        };
        let whole = stack.item == ItemKind::GoldNugget.id();
        self.ask_piglin_pickup(id, item, stack, whole);
    }

    fn ask_piglin_pickup(&mut self, id: EntityId, item: EntityId, stack: ItemStack, whole: bool) {
        let remaining = if whole {
            0
        } else {
            i32::from(stack.count.get()) - 1
        };
        let step = PiglinPickupStep {
            collector: id,
            item,
            stack,
            whole,
        };
        let lane = self.lane_for(None, id);
        self.fire(
            lane,
            Continuation::PiglinPickup(Box::new(step)),
            move |server| {
                let entity = server.subject_view(id)?;
                Some(EventPayload::EntityPickupItem(EntityPickupItem {
                    entity_type: entity.kind.clone(),
                    entity,
                    item: server.entity_view(item)?,
                    remaining,
                }))
            },
        );
    }

    pub(crate) fn finish_piglin_pickup(&mut self, step: PiglinPickupStep, cancelled: bool) {
        let PiglinPickupStep {
            collector: id,
            item,
            stack,
            whole,
        } = step;
        if self.item_stack_of(item).as_ref() != Some(&stack)
            || self.entities.store.get(id).is_none()
        {
            return;
        }
        if cancelled {
            if whole {
                self.ask_piglin_pickup(id, item, stack, false);
            }
            return;
        }
        self.push(
            Audience::Trackers(item),
            ClientEvent::ItemPickup {
                collected: item,
                collector: id,
                amount: if whole {
                    i32::from(stack.count.get())
                } else {
                    1
                },
            },
        );
        let picked = if whole {
            self.remove_item(item);
            stack
        } else {
            let one = ItemStack {
                count: NonZeroU8::MIN,
                ..stack.clone()
            };
            match NonZeroU8::new(stack.count.get() - 1) {
                Some(rest) => {
                    if let Some(entity) = self.entities.store.get_mut(item) {
                        item_entity::set_stack(
                            &mut entity.base,
                            Some(ItemStack {
                                count: rest,
                                ..stack
                            }),
                        );
                    }
                }
                None => self.remove_item(item),
            }
            one
        };
        if rspigot_ai::is_loved_item(picked.item) {
            self.with_memories(id, |memories| {
                memories.erase(MemoryModuleType::TimeTryingToReachAdmireItem);
            });
            self.piglin_hold_in_off_hand(id, picked);
            self.piglin_admire_gold_item(id);
        } else if rspigot_ai::is_food(picked.item)
            && !self.piglin_memory_present(id, MemoryModuleType::AteRecently)
        {
            self.with_memories(id, |memories| {
                memories.set_with_expiry(
                    MemoryModuleType::AteRecently,
                    MemoryValue::Bool(true),
                    ATE_RECENTLY_TICKS,
                );
            });
        } else if !self.piglin_equip_if_possible(id, &picked) {
            self.piglin_put_in_inventory(id, picked);
        }
    }
}

fn piglin_is_barter(stack: &ItemStack) -> bool {
    rspigot_ai::is_barter_currency(stack.item)
}
