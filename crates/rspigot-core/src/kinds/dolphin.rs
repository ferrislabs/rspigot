use rspigot_ai::{DolphinFacts, MobView, inflate, thrown_item_velocity};
use rspigot_domain::damage::DamageType;
use rspigot_domain::effect::{EffectDuration, EffectId, EffectInstance};
use rspigot_domain::entity::living::{self, slot_index};
use rspigot_domain::entity::species::{Species, dolphin};
use rspigot_domain::entity::{Entity, EntityState, ageable};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::random::RandomSource;
use rspigot_domain::{EntityId, Vec3};
use rspigot_items::EquipmentSlot;

use crate::events::Hand;
use crate::health::DamageSource;
use crate::interact::{Interaction, item_tag};
use crate::plugins::events::{CreatureSpawnReason, EntityPickupItem, EntityPotionEffectCause};
use crate::plugins::steps::MobPickupStep;
use crate::plugins::{Continuation, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

const FISHES: &str = "minecraft:fishes";
const DOLPHINS_GRACE: &str = "minecraft:dolphins_grace";
const PICKUP_REACH: (f64, f64, f64) = (1.0, 0.0, 1.0);
const ASSIST_SPEED: f32 = 0.02;

fn dolphin_state(entity: &Entity) -> Option<dolphin::DolphinState> {
    match &entity.state {
        EntityState::Living(living) => match living.mob.as_ref().map(|mob| mob.species) {
            Some(Species::Dolphin(state)) => Some(state),
            _ => None,
        },
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn dolphin_view(&self, entity: &Entity, view: &mut MobView) {
        let Some(state) = dolphin_state(entity) else {
            return;
        };
        view.species.dolphin = DolphinFacts {
            got_fish: dolphin::got_fish(&entity.base),
            treasure_pos: state.treasure_pos,
        };
    }

    pub(crate) fn dolphin_post_tick(&mut self, id: EntityId) {
        let wet = match self.entities.store.get(id) {
            Some(entity) => entity.base.fluid.in_water || self.is_in_rain(&entity.base),
            None => return,
        };
        let mut random = self.mob_random(id);
        let Some(base) = self
            .entities
            .store
            .get_mut(id)
            .map(|entity| &mut entity.base)
        else {
            return;
        };
        let mut dry_out = false;
        if living::is_no_ai(base) {
            base.set_air_supply(base.max_air_ticks);
        } else {
            dry_out = dolphin::tick_moistness(base, wet, &mut random).dry_out;
        }
        self.store_mob_random(id, random);
        if dry_out {
            self.hurt_mob(id, &DamageSource::new(DamageType::DryOut), 1.0);
        }
        self.dolphin_pick_up_loot(id);
    }

    fn dolphin_pick_up_loot(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(living) = entity.living() else {
            return;
        };
        let can_pick_up = living.mob.as_ref().is_some_and(|mob| mob.can_pick_up_loot);
        if !can_pick_up
            || living::is_dead_or_dying(&entity.base, living)
            || entity.base.is_removed()
            || !self.config.rules.mob_griefing
            || living.equipment[slot_index(EquipmentSlot::Mainhand)].is_some()
        {
            return;
        }
        let area = inflate(
            &entity.base.bounding_box(),
            PICKUP_REACH.0,
            PICKUP_REACH.1,
            PICKUP_REACH.2,
        );
        let item = self
            .entities
            .store
            .iter()
            .find(|other| {
                matches!(&other.state, EntityState::Item(item) if item.pickup_delay <= 0)
                    && !other.base.is_removed()
                    && other.base.bounding_box().intersects(&area)
            })
            .map(|other| other.base.id);
        if let Some(item) = item {
            self.mob_pick_up_item(id, item);
        }
    }

    pub(crate) fn breath_assist(&mut self, id: EntityId) {
        let chunks = &self.world.chunks;
        if let Some(Entity {
            base,
            state: EntityState::Living(living),
            ..
        }) = self.entities.store.get_mut(id)
        {
            living::breath_assist(base, living, chunks, ASSIST_SPEED);
        }
    }

    pub(crate) fn dolphin_drop_held_item(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let EntityState::Living(living) = &mut entity.state else {
            return;
        };
        let Some(stack) = living.equipment[slot_index(EquipmentSlot::Mainhand)].take() else {
            return;
        };
        let (position, yaw, pitch, eye) = (
            entity.base.position,
            entity.base.yaw,
            entity.base.pitch,
            entity.base.eye_y(),
        );
        self.set_panda_main_hand(id, None);
        let mut random = self.mob_random(id);
        let angle = random.next_float() * std::f32::consts::TAU;
        let spread = random.next_float();
        self.store_mob_random(id, random);
        let velocity = thrown_item_velocity(yaw, pitch, angle, spread);
        let at = Vec3::new(
            position.x,
            eye - f64::from(rspigot_ai::ITEM_EYE_DROP as f32),
            position.z,
        );
        let item = self.item_entity(at, velocity, rspigot_ai::ITEM_PICKUP_DELAY as u16, stack);
        self.add_entity(item, CreatureSpawnReason::Default);
    }

    pub(crate) fn dolphins_grace(&mut self, _dolphin: EntityId, player: EntityId) {
        let Some(grace) = EffectId::from_name(DOLPHINS_GRACE) else {
            return;
        };
        let effect = EffectInstance::new(
            grace,
            EffectDuration::Ticks(rspigot_ai::GRACE_TICKS as u32),
            0,
            true,
        );
        self.add_effect_to(player, effect, EntityPotionEffectCause::Dolphin);
    }

    pub(crate) fn dolphin_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        let stack = held?;
        if !item_tag(stack.item, FISHES) {
            return None;
        }
        self.entity_sound(target, dolphin::FEEDING_SOUND, 1.0, 1.0);
        let growth = self.entities.store.get(target).and_then(|entity| {
            entity
                .living()
                .and_then(|l| l.mob.as_ref())
                .and_then(|m| m.ageable.as_ref())
                .filter(|state| ageable::can_age_up(&entity.base, state))
                .map(|state| state.age)
        });
        self.consume_held(player, hand, 1);
        match growth {
            Some(age) => {
                if let Some(Entity {
                    base,
                    state: EntityState::Living(state),
                    ..
                }) = self.entities.store.get_mut(target)
                    && let Some(ageable) = state.mob.as_mut().and_then(|m| m.ageable.as_mut())
                {
                    let seconds = ageable::speed_up_seconds_when_feeding(-age);
                    ageable::age_up(base, ageable, seconds, true);
                }
            }
            None => {
                if let Some(entity) = self.entities.store.get_mut(target) {
                    dolphin::set_got_fish(&mut entity.base, true);
                }
            }
        }
        Some(Interaction::Predicted)
    }

    fn mob_pick_up_item(&mut self, id: EntityId, item: EntityId) {
        let Some(stack) = self.item_stack_of(item) else {
            return;
        };
        let step = MobPickupStep {
            collector: id,
            item,
            stack,
        };
        let lane = self.lane_for(None, id);
        let describe = move |server: &Self| {
            let entity = server.subject_view(id)?;
            Some(EventPayload::EntityPickupItem(EntityPickupItem {
                entity_type: entity.kind.clone(),
                entity,
                item: server.entity_view(item)?,
                remaining: 0,
            }))
        };
        self.fire(lane, Continuation::MobPickup(Box::new(step)), describe);
    }
}
