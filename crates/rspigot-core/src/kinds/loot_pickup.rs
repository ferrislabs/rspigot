use std::num::NonZeroU8;

use rspigot_ai::inflate;
use rspigot_domain::EntityId;
use rspigot_domain::entity::item as item_entity;
use rspigot_domain::entity::living::{self, slot_index};
use rspigot_domain::entity::pickup::{self, DROP_CURRENT_BIAS, GUARANTEED_DROP_CHANCE};
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::item::ItemStack;
use rspigot_domain::random::RandomSource;

use crate::entities::wire_slot;
use crate::events::ClientEvent;
use crate::plugins::events::EntityPickupItem;
use crate::plugins::steps::MobPickupStep;
use crate::plugins::{Continuation, EventPayload};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

impl<L: ClientLink> Server<L> {
    pub(crate) fn mob_loot_pickup(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(living) = entity.living() else {
            return;
        };
        let wants_loot = living.mob.as_ref().is_some_and(|mob| mob.can_pick_up_loot);
        if !wants_loot
            || pickup::handled_elsewhere(entity.base.kind)
            || living::is_dead_or_dying(&entity.base, living)
            || entity.base.is_removed()
            || !self.config.rules.mob_griefing
        {
            return;
        }
        let kind = entity.base.kind;
        let (reach_x, reach_y, reach_z) = pickup::REACH;
        let area = inflate(&entity.base.bounding_box(), reach_x, reach_y, reach_z);
        let wanted: Vec<(EntityId, ItemStack)> = self
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
                let stack = item_entity::stack(&other.base)?;
                pickup::plan(kind, living, stack)?;
                Some((other.base.id, stack.clone()))
            })
            .collect();
        for (item, stack) in wanted {
            self.ask_loot_pickup(id, item, stack);
        }
    }

    fn ask_loot_pickup(&mut self, id: EntityId, item: EntityId, stack: ItemStack) {
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

    pub(crate) fn finish_loot_pickup(&mut self, step: MobPickupStep, cancelled: bool) {
        let MobPickupStep {
            collector,
            item,
            stack,
        } = step;
        if cancelled || self.item_stack_of(item).as_ref() != Some(&stack) {
            return;
        }
        let Some(entity) = self.entities.store.get(collector) else {
            return;
        };
        let Some(living) = entity.living() else {
            return;
        };
        if entity.base.is_removed() || living::is_dead_or_dying(&entity.base, living) {
            return;
        }
        let Some(plan) = pickup::plan(entity.base.kind, living, &stack) else {
            return;
        };
        let position = entity.base.position;
        let slot = plan.slot;
        let index = slot_index(slot);
        let previous = living.equipment[index].clone();
        let drop_chance = living
            .mob
            .as_ref()
            .map_or(0.0, |mob| mob.drop_chances[index]);
        let Some(taken) = NonZeroU8::new(plan.taken) else {
            return;
        };
        let equipped = ItemStack {
            count: taken,
            ..stack.clone()
        };
        let mut random = self.mob_random(collector);
        let drops_previous = previous
            .as_ref()
            .is_some_and(|_| (random.next_float() - DROP_CURRENT_BIAS).max(0.0) < drop_chance);
        self.store_mob_random(collector, random);
        if let (true, Some(previous)) = (drops_previous, previous) {
            self.spawn_at_location(position, 0.0, previous);
        }
        if let Some(Entity {
            state: EntityState::Living(living),
            ..
        }) = self.entities.store.get_mut(collector)
        {
            living.equipment[index] = Some(equipped.clone());
            if let Some(mob) = living.mob.as_mut() {
                mob.drop_chances[index] = GUARANTEED_DROP_CHANCE;
                mob.persistence_required = true;
            }
        }
        self.trigger_thrown_item_picked_up_by_entity(item, collector);
        self.push(
            Audience::Trackers(collector),
            ClientEvent::EntityEquipment {
                entity: collector,
                slots: vec![(wire_slot(slot), Some(equipped))],
            },
        );
        self.push(
            Audience::Trackers(item),
            ClientEvent::ItemPickup {
                collected: item,
                collector,
                amount: i32::from(plan.taken),
            },
        );
        match NonZeroU8::new(stack.count.get() - plan.taken) {
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
    }
}
