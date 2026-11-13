use std::num::NonZeroU8;

use rspigot_ai::{AllayAction, AllayFacts, BrainMessage, MemoryModuleType, MobView};
use rspigot_domain::entity::living::{self, slot_index};
use rspigot_domain::entity::save::{item_stack_from_nbt, item_stack_to_nbt};
use rspigot_domain::entity::species::{Species, allay};
use rspigot_domain::entity::{Aabb, Entity, EntityState};
use rspigot_domain::item::{ItemStack, max_stack, same_item_same_components};
use rspigot_domain::player::PlayerId;
use rspigot_domain::random::RandomSource;
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::EquipmentSlot;
use rspigot_nbt::Tag;

use crate::entities::wire_slot;
use crate::events::{ClientEvent, EntityStatus, Hand};
use crate::health::DamageSource;
use crate::interact::{Interaction, item_tag};
use crate::mobs::ServerRandom;
use crate::plugins::events::{
    CreatureSpawnReason, EntityPickupItem, EntityRegainHealthRegainReason,
};
use crate::plugins::steps::MobPickupStep;
use crate::plugins::{Continuation, EventPayload};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const THROW_PICKUP_DELAY: u16 = 10;
const DROP_VELOCITY: (f64, f64, f64) = (0.3, 0.3, 0.3);
const GUARANTEED_DROP: f32 = 1.0;

fn inflate(area: &Aabb, x: f64, y: f64, z: f64) -> Aabb {
    Aabb::new(
        area.min_x - x,
        area.min_y - y,
        area.min_z - z,
        area.max_x + x,
        area.max_y + y,
        area.max_z + z,
    )
}

fn inventory_of(entity: &Entity) -> Option<ItemStack> {
    entity
        .extra
        .get_list(allay::INVENTORY_KEY)
        .unwrap_or_default()
        .iter()
        .filter_map(Tag::as_compound)
        .find_map(item_stack_from_nbt)
}

fn store_inventory(entity: &mut Entity, stack: Option<&ItemStack>) {
    match stack {
        Some(stack) => {
            let list: Vec<Tag> = vec![Tag::Compound(item_stack_to_nbt(stack))];
            entity.extra.insert(allay::INVENTORY_KEY, Tag::List(list));
        }
        None => {
            entity
                .extra
                .insert(allay::INVENTORY_KEY, Tag::List(Vec::new()));
        }
    }
}

fn hand_of(entity: &Entity) -> Option<&ItemStack> {
    entity
        .living()
        .and_then(|state| state.equipment(EquipmentSlot::Mainhand))
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn allay_view(&self, entity: &Entity, view: &mut MobView) {
        let inventory = inventory_of(entity);
        view.species.allay = AllayFacts {
            has_item_to_throw: inventory.is_some(),
            hand_item: hand_of(entity).map(|stack| stack.item),
            inventory_item: inventory.as_ref().map(|stack| stack.item),
            inventory_full: inventory
                .as_ref()
                .is_some_and(|stack| u32::from(stack.count.get()) >= max_stack(stack)),
        };
    }

    pub(crate) fn allay_inventory(&self, id: EntityId) -> Option<ItemStack> {
        self.entities.store.get(id).and_then(inventory_of)
    }

    fn set_allay_hand(&mut self, id: EntityId, stack: Option<ItemStack>) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let Some(living) = entity.living_mut() else {
            return;
        };
        living.equipment[slot_index(EquipmentSlot::Mainhand)] = stack.clone();
        if stack.is_some()
            && let Some(mob) = living.mob.as_mut()
        {
            mob.drop_chances[slot_index(EquipmentSlot::Mainhand)] = GUARANTEED_DROP;
        }
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityEquipment {
                entity: id,
                slots: vec![(wire_slot(EquipmentSlot::Mainhand), stack)],
            },
        );
    }

    fn allay_has_brain_memory(&self, id: EntityId, memory: MemoryModuleType) -> bool {
        self.entities
            .mobs
            .entries
            .get(&id)
            .and_then(|entry| entry.ai.behavior_brain())
            .is_some_and(|brain| brain.memories().has_value(memory))
    }

    fn allay_set_liked_player(&mut self, id: EntityId, player: Option<uuid::Uuid>) {
        if let Some(brain) = self
            .entities
            .mobs
            .entries
            .get_mut(&id)
            .and_then(|entry| entry.ai.behavior_brain_mut())
        {
            match player {
                Some(uuid) => brain.memories_mut().set(
                    MemoryModuleType::LikedPlayer,
                    rspigot_ai::MemoryValue::Uuid(uuid),
                ),
                None => brain.memories_mut().erase(MemoryModuleType::LikedPlayer),
            }
        }
    }

    fn allay_liked_player(&self, id: EntityId) -> Option<uuid::Uuid> {
        self.entities
            .mobs
            .entries
            .get(&id)
            .and_then(|entry| entry.ai.behavior_brain())
            .and_then(|brain| brain.memories().get(MemoryModuleType::LikedPlayer))
            .and_then(rspigot_ai::MemoryValue::as_uuid)
    }

    pub(crate) fn allay_action(&mut self, id: EntityId, action: AllayAction) {
        let AllayAction::ThrowItem { target } = action;
        let Some(mut stack) = self.allay_inventory(id) else {
            return;
        };
        let thrown = ItemStack {
            count: NonZeroU8::MIN,
            ..stack.clone()
        };
        let remaining = stack.count.get() - 1;
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        match NonZeroU8::new(remaining) {
            Some(count) => {
                stack.count = count;
                store_inventory(entity, Some(&stack));
            }
            None => store_inventory(entity, None),
        }
        let lifted = Vec3::new(target.x, target.y + allay::THROW_TARGET_LIFT, target.z);
        self.throw_item_from(
            id,
            thrown,
            lifted,
            Vec3::new(
                allay::THROW_VELOCITY.0,
                allay::THROW_VELOCITY.1,
                allay::THROW_VELOCITY.2,
            ),
            allay::THROW_YAW_OFFSET,
        );
        let game_time = self.world.level.game_time;
        if game_time % allay::THROW_SOUND_EVERY == 0 {
            let mut random = ServerRandom(&mut self.rng);
            if random.next_double() < allay::THROW_SOUND_CHANCE {
                let index = random.next_int(allay::THROW_SOUND_PITCHES.len() as i32) as usize;
                self.entity_sound(
                    id,
                    allay::THROW_SOUND,
                    1.0,
                    allay::THROW_SOUND_PITCHES[index],
                );
            }
        }
    }

    pub(crate) fn throw_item_from(
        &mut self,
        id: EntityId,
        stack: ItemStack,
        target: Vec3,
        throw_velocity: Vec3,
        y_offset: f32,
    ) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let body = entity.base.position;
        let uuid = entity.base.uuid;
        let eye = entity.base.eye_y() - f64::from(y_offset);
        let direction = (target - body).normalize();
        let velocity = direction.multiply(throw_velocity.x, throw_velocity.y, throw_velocity.z);
        let mut item = self.item_entity(
            Vec3::new(body.x, eye, body.z),
            velocity,
            THROW_PICKUP_DELAY,
            stack,
        );
        if let EntityState::Item(state) = &mut item.state {
            state.thrower = Some(uuid);
        }
        self.add_entity(item, CreatureSpawnReason::Default);
    }

    pub(crate) fn allay_ai_step(&mut self, id: EntityId) {
        let alive = self.living_entity_alive(id);
        let panicking = self.allay_has_brain_memory(id, MemoryModuleType::IsPanicking);
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let tick_count = entity.base.tick_count;
        if allay::is_dancing(&entity.base) && (panicking || tick_count % 20 == 0) {
            allay::set_dancing(&mut entity.base, false);
        }
        if let rspigot_domain::entity::Entity {
            base,
            state: EntityState::Living(living),
            ..
        } = entity
            && let Some(Species::Allay(state)) = living.mob.as_mut().map(|mob| &mut mob.species)
        {
            allay::tick_duplication_cooldown(base, state);
        }
        if alive && tick_count % allay::REGEN_INTERVAL == 0 {
            self.heal_entity(
                id,
                allay::REGEN_AMOUNT,
                EntityRegainHealthRegainReason::Regen,
            );
        }
        self.allay_pick_up_loot(id);
    }

    fn allay_can_pick_up(&self, id: EntityId) -> bool {
        let holding = self.entities.store.get(id).and_then(hand_of).is_some();
        holding && !self.allay_has_brain_memory(id, MemoryModuleType::ItemPickupCooldownTicks)
    }

    fn allay_pick_up_loot(&mut self, id: EntityId) {
        if !self.config.rules.mob_griefing || !self.allay_can_pick_up(id) {
            return;
        }
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if entity.base.is_removed() {
            return;
        }
        let area = inflate(
            &entity.base.bounding_box(),
            allay::PICKUP_REACH.0,
            allay::PICKUP_REACH.1,
            allay::PICKUP_REACH.2,
        );
        let items: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|other| {
                matches!(&other.state, EntityState::Item(item) if item.pickup_delay <= 0)
                    && !other.base.is_removed()
                    && other.base.bounding_box().intersects(&area)
            })
            .map(|other| other.base.id)
            .collect();
        for item in items {
            self.allay_pick_up_item(id, item);
        }
    }

    fn allay_wants(&self, id: EntityId, stack: &ItemStack) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return false;
        };
        let Some(hand) = hand_of(entity) else {
            return false;
        };
        if !same_item_same_components(
            &ItemStack {
                count: stack.count,
                ..hand.clone()
            },
            stack,
        ) {
            return false;
        }
        match inventory_of(entity) {
            None => true,
            Some(held) => {
                same_item_same_components(
                    &ItemStack {
                        count: stack.count,
                        ..held.clone()
                    },
                    stack,
                ) && u32::from(held.count.get()) < max_stack(&held)
            }
        }
    }

    fn allay_pick_up_item(&mut self, id: EntityId, item: EntityId) {
        let Some(stack) = self.item_stack_of(item) else {
            return;
        };
        if !self.allay_wants(id, &stack) {
            return;
        }
        let step = MobPickupStep {
            collector: id,
            item,
            stack,
        };
        let lane = self.lane_for(None, id);
        self.fire(
            lane,
            Continuation::MobPickup(Box::new(step)),
            move |server| {
                let entity = server.subject_view(id)?;
                Some(EventPayload::EntityPickupItem(EntityPickupItem {
                    entity_type: entity.kind.clone(),
                    entity,
                    item: server.entity_view(item)?,
                    remaining: 0,
                }))
            },
        );
    }

    pub(crate) fn finish_allay_pickup(&mut self, step: MobPickupStep, cancelled: bool) {
        let MobPickupStep {
            collector,
            item,
            stack,
        } = step;
        if cancelled
            || self.item_stack_of(item).as_ref() != Some(&stack)
            || !self.living_entity_alive(collector)
            || !self.allay_wants(collector, &stack)
        {
            return;
        }
        let Some(entity) = self.entities.store.get_mut(collector) else {
            return;
        };
        let current = inventory_of(entity);
        let limit = max_stack(&stack);
        let held = current
            .as_ref()
            .map_or(0, |held| u32::from(held.count.get()));
        let incoming = u32::from(stack.count.get());
        let accepted = incoming.min(limit.saturating_sub(held));
        if accepted == 0 {
            return;
        }
        let total = (held + accepted) as u8;
        let merged = ItemStack {
            count: NonZeroU8::new(total).unwrap_or(NonZeroU8::MIN),
            ..current.unwrap_or_else(|| stack.clone())
        };
        store_inventory(entity, Some(&merged));
        self.push(
            Audience::Trackers(item),
            ClientEvent::ItemPickup {
                collected: item,
                collector,
                amount: accepted as i32,
            },
        );
        if accepted >= incoming {
            self.remove_item(item);
        } else if let Some(entity) = self.entities.store.get_mut(item) {
            let left = ItemStack {
                count: NonZeroU8::new((incoming - accepted) as u8).unwrap_or(NonZeroU8::MIN),
                ..stack
            };
            rspigot_domain::entity::item::set_stack(&mut entity.base, Some(left));
        }
    }

    pub(crate) fn allay_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        let entity = self.entities.store.get(target)?;
        let in_hand = hand_of(entity).cloned();
        let dancing = allay::is_dancing(&entity.base);
        let can_duplicate = allay::can_duplicate(&entity.base);
        if dancing
            && can_duplicate
            && held.is_some_and(|stack| item_tag(stack.item, allay::DUPLICATION_ITEM_TAG))
        {
            if self.duplicate_allay(target) {
                self.mob_entity_event_all(target, allay::DUPLICATE_EVENT);
                self.entity_sound(
                    target,
                    allay::CHIME_SOUND,
                    allay::INTERACT_SOUND_VOLUME,
                    1.0,
                );
                self.consume_held(player, hand, 1);
            }
            return Some(Interaction::Predicted);
        }
        if in_hand.is_none()
            && let Some(stack) = held
        {
            let single = ItemStack {
                count: NonZeroU8::MIN,
                ..stack.clone()
            };
            self.set_allay_hand(target, Some(single));
            self.consume_held(player, hand, 1);
            self.entity_sound(
                target,
                allay::ITEM_GIVEN_SOUND,
                allay::INTERACT_SOUND_VOLUME,
                1.0,
            );
            let uuid = self.players.get(&player)?.profile.id.0;
            self.allay_set_liked_player(target, Some(uuid));
            return Some(Interaction::Predicted);
        }
        if let Some(taken) = in_hand
            && hand == Hand::Main
            && held.is_none()
        {
            self.set_allay_hand(target, None);
            self.entity_sound(
                target,
                allay::ITEM_TAKEN_SOUND,
                allay::INTERACT_SOUND_VOLUME,
                1.0,
            );
            self.mob_swing(target, rspigot_ai::Hand::Main);
            self.allay_drop_inventory(target);
            self.allay_set_liked_player(target, None);
            self.give_stack_to_player(player, taken);
            return Some(Interaction::Predicted);
        }
        None
    }

    fn give_stack_to_player(&mut self, id: PlayerId, stack: ItemStack) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let mut rest = Some(stack);
        player.inventory.add(&mut rest, false);
        self.sync_inventory(id);
        self.broadcast_held_item(id);
        if let Some(rest) = rest {
            self.drop_from_player(id, rest, false, true);
        }
    }

    fn allay_drop_inventory(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let Some(stack) = inventory_of(entity) else {
            return;
        };
        store_inventory(entity, None);
        let at = entity.base.position;
        self.throw_item_from(
            id,
            stack,
            at,
            Vec3::new(DROP_VELOCITY.0, DROP_VELOCITY.1, DROP_VELOCITY.2),
            0.0,
        );
    }

    fn duplicate_allay(&mut self, id: EntityId) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return false;
        };
        let at = entity.base.position;
        let yaw = entity.base.yaw;
        let base = self.new_entity_base(Kind::Allay);
        let mut copy = crate::mob_actions::new_mob(base);
        copy.base.snap_to(at, yaw, 0.0);
        if let Entity {
            base,
            state: EntityState::Living(living),
            ..
        } = &mut copy
            && let Some(mob) = living.mob.as_mut()
        {
            mob.persistence_required = true;
            if let Species::Allay(state) = &mut mob.species {
                allay::reset_duplication_cooldown(base, state);
            }
        }
        if let Some(Entity {
            base,
            state: EntityState::Living(living),
            ..
        }) = self.entities.store.get_mut(id)
            && let Some(Species::Allay(state)) = living.mob.as_mut().map(|mob| &mut mob.species)
        {
            allay::reset_duplication_cooldown(base, state);
        }
        self.add_entity(copy, CreatureSpawnReason::Duplication);
        true
    }

    pub(crate) fn allay_hurt_amount(
        &mut self,
        id: EntityId,
        source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        let Some(liked) = self.allay_liked_player(id) else {
            return Some(amount);
        };
        let by_liked = source
            .causing
            .as_ref()
            .and_then(|causing| causing.player)
            .is_some_and(|player| player.0 == liked);
        if by_liked { None } else { Some(amount) }
    }

    pub(crate) fn allay_death_drops(&mut self, id: EntityId) -> Vec<ItemStack> {
        let mut drops: Vec<ItemStack> = self.allay_inventory(id).into_iter().collect();
        if let Some(living) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
            && let Some(hand) = living.equipment[slot_index(EquipmentSlot::Mainhand)].take()
        {
            drops.push(hand);
        }
        drops
    }

    pub(crate) fn allay_ambient_voice(&mut self, id: EntityId) {
        let holding = self.entities.store.get(id).and_then(hand_of).is_some();
        let sound = if holding {
            allay::AMBIENT_WITH_ITEM
        } else {
            allay::AMBIENT_WITHOUT_ITEM
        };
        self.play_mob_voice(id, move |_| Some(sound));
    }

    pub(crate) fn allay_hear(&mut self, id: EntityId, pos: BlockPos) {
        self.brain_message(id, BrainMessage::AllayHeardNoteblock { pos });
    }

    pub(crate) fn allay_listener_section(
        &self,
        id: EntityId,
    ) -> Option<rspigot_domain::SectionPos> {
        let base = &self.entities.store.get(id)?.base;
        let eyes = Vec3::new(
            base.position.x,
            base.position.y + f64::from(base.eye_height()),
            base.position.z,
        );
        Some(rspigot_domain::SectionPos::of(eyes.block()))
    }

    pub(crate) fn allay_can_receive_vibration(&self, id: EntityId, pos: BlockPos) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return false;
        };
        if living::is_no_ai(&entity.base) {
            return false;
        }
        let liked = self
            .entities
            .mobs
            .entries
            .get(&id)
            .and_then(|entry| entry.ai.behavior_brain())
            .and_then(|brain| {
                brain
                    .memories()
                    .get(MemoryModuleType::LikedNoteblockPosition)
            })
            .and_then(rspigot_ai::MemoryValue::as_global_pos)
            .cloned();
        let Some(liked) = liked else {
            return true;
        };
        let from = entity.base.position.block();
        let (dx, dy, dz) = (
            i64::from(liked.pos.x - from.x),
            i64::from(liked.pos.y - from.y),
            i64::from(liked.pos.z - from.z),
        );
        let reach = i64::from(allay::NOTEBLOCK_REACH);
        liked.dimension == self.world.dimension.key.as_str()
            && dx * dx + dy * dy + dz * dz <= reach * reach
            && liked.pos == pos
    }

    pub(crate) fn mob_entity_event_all(&mut self, id: EntityId, event: i8) {
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityStatus {
                entity: id,
                status: EntityStatus::Event(event),
            },
        );
    }
}
