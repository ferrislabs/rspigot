use rspigot_ai::{MobAi, MobView};
use rspigot_domain::EntityId;
use rspigot_domain::entity::living;
use rspigot_domain::entity::species::ocelot;
use rspigot_domain::entity::{Entity, EntityState, Pose};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::random::{LegacyRandom, RandomSource};

use crate::events::Hand;
use crate::health::DamageSource;
use crate::interact::{Interaction, item_tag};
use crate::plugins::steps::TameStep;
use crate::ports::ClientLink;
use crate::server::Server;

const TRUST_FAILURE_EVENT: i8 = 40;
const TRUST_SUCCESS_EVENT: i8 = 41;
const OCELOT_FOOD: &str = "minecraft:ocelot_food";
const TRUST_ONE_IN: i32 = 3;
const TRUST_DISTANCE_SQR: f64 = 9.0;
const PARTICLES: usize = 7;
const CROUCH_SPEED: f64 = 0.6;
const SPRINT_SPEED: f64 = 1.33;

fn consume_trust_particle_draws(random: &mut LegacyRandom) {
    for _ in 0..PARTICLES {
        for _ in 0..3 {
            random.next_gaussian();
        }
        for _ in 0..3 {
            random.next_double();
        }
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn ocelot_view(&self, entity: &Entity, view: &mut MobView) {
        view.trusting = ocelot::is_trusting(&entity.base);
    }

    pub(super) fn ocelot_registered(&self, id: EntityId, ai: &mut MobAi) {
        if self
            .entities
            .store
            .get(id)
            .is_some_and(|entity| ocelot::is_trusting(&entity.base))
        {
            ai.reassess_trusting(true);
        }
    }

    pub(crate) fn ocelot_pre_tick(&mut self, _id: EntityId) -> bool {
        true
    }

    pub(crate) fn ocelot_post_tick(&mut self, _id: EntityId) {}

    pub(crate) fn ocelot_custom_ai_step(&mut self, id: EntityId) {
        let Some(entry) = self.entities.mobs.entries.get(&id) else {
            return;
        };
        let movement = &entry.ai.brain().movement;
        let speed = movement.speed_modifier();
        let (pose, sprinting) = if !movement.has_wanted() {
            (Pose::Standing, false)
        } else if speed == CROUCH_SPEED {
            (Pose::Crouching, false)
        } else if speed == SPRINT_SPEED {
            (Pose::Standing, true)
        } else {
            (Pose::Standing, false)
        };
        if let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        {
            base.set_pose(pose);
            living::set_sprinting(base, state, sprinting);
        }
    }

    pub(crate) fn ocelot_set_trusting(&mut self, id: EntityId, trusting: bool) {
        if let Some(entity) = self.entities.store.get_mut(id) {
            ocelot::set_trusting(&mut entity.base, trusting);
        }
        if let Some(entry) = self.entities.mobs.entries.get_mut(&id) {
            entry.ai.reassess_trusting(trusting);
        }
    }

    pub(crate) fn ocelot_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        let stack = held?;
        let entry = self.entities.mobs.entries.get(&target)?;
        let ocelot_entity = self.entities.store.get(target)?;
        let near = self.players.get(&player).is_some_and(|p| {
            p.position.distance_squared(ocelot_entity.base.position) < TRUST_DISTANCE_SQR
        });
        if !entry.ai.is_tempting()
            || ocelot::is_trusting(&ocelot_entity.base)
            || !item_tag(stack.item, OCELOT_FOOD)
            || !near
        {
            return None;
        }
        self.consume_held(player, hand, 1);
        let mut random = self.mob_random(target);
        let lucky = random.next_int(TRUST_ONE_IN) == 0;
        if lucky {
            self.store_mob_random(target, random);
            self.ask_tame(player, target);
        } else {
            consume_trust_particle_draws(&mut random);
            self.store_mob_random(target, random);
            self.mob_entity_event(target, TRUST_FAILURE_EVENT);
        }
        Some(Interaction::Predicted)
    }

    pub(crate) fn finish_ocelot_trust(&mut self, step: TameStep, cancelled: bool) {
        let target = step.target;
        let mut random = self.mob_random(target);
        consume_trust_particle_draws(&mut random);
        self.store_mob_random(target, random);
        if cancelled {
            self.mob_entity_event(target, TRUST_FAILURE_EVENT);
            return;
        }
        self.ocelot_set_trusting(target, true);
        self.mob_entity_event(target, TRUST_SUCCESS_EVENT);
    }

    pub(crate) fn ocelot_hurt_amount(
        &mut self,
        _id: EntityId,
        _source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        Some(amount)
    }

    pub(crate) fn ocelot_after_hurt(&mut self, _id: EntityId, _source: &DamageSource, _dead: bool) {
    }
}
