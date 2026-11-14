use rspigot_ai::{AiWorld, MobView, TargetKind, TargetReason};
use rspigot_domain::entity::species::{Species, zombified_piglin};
use rspigot_domain::entity::{Aabb, Entity, EntityState};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::random::RandomSource;
use rspigot_domain::{EntityId, Vec3};

use crate::events::Hand;
use crate::health::DamageSource;
use crate::interact::Interaction;
use crate::mobs::sky_darken;
use crate::plugins::Continuation;
use crate::plugins::events::{EventPayload, PigZombieAnger, PigZombieAngerPatch};
use crate::plugins::steps::PigAngerStep;
use crate::ports::ClientLink;
use crate::server::Server;

const ANGRY_SOUND: &str = "minecraft:entity.zombified_piglin.angry";
const ALERT_RANGE_Y: f64 = 10.0;
const BABY_PITCH: f32 = 1.5;
const ADULT_PITCH: f32 = 1.0;
const PITCH_SPREAD: f32 = 0.2;

impl<L: ClientLink> Server<L> {
    pub(crate) fn zombified_piglin_view(&self, _entity: &Entity, _view: &mut MobView) {}

    pub(crate) fn zombified_piglin_pre_tick(&mut self, _id: EntityId) -> bool {
        true
    }

    pub(crate) fn zombified_piglin_post_tick(&mut self, _id: EntityId) {}

    pub(crate) fn zombified_piglin_custom_ai_step(&mut self, id: EntityId) {
        let game_time = self.world.level.game_time;
        let Some(angry) = self.piglin_update_speed(id, game_time) else {
            return;
        };
        if angry {
            self.piglin_first_anger_sound(id);
        }
        self.update_persistent_anger(id, true);
        if self.mob_target(id).is_some() {
            self.piglin_maybe_alert_others(id);
        }
    }

    pub(crate) fn zombified_piglin_interact(
        &mut self,
        _player: PlayerId,
        _target: EntityId,
        _hand: Hand,
        _held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        None
    }

    pub(crate) fn zombified_piglin_hurt_amount(
        &mut self,
        _id: EntityId,
        _source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        Some(amount)
    }

    pub(crate) fn zombified_piglin_after_hurt(
        &mut self,
        _id: EntityId,
        _source: &DamageSource,
        _dead: bool,
    ) {
    }

    pub(crate) fn zombified_piglin_acquired_target(&mut self, id: EntityId) {
        let mut random = self.mob_random(id);
        if let Some(piglin) = self.piglin_state(id) {
            piglin.acquire_target(&mut random);
        }
        self.store_mob_random(id, random);
    }

    pub(crate) fn ask_pig_zombie_anger(&mut self, id: EntityId) {
        let mut random = self.mob_random(id);
        let anger = zombified_piglin::sample_anger(&mut random);
        self.store_mob_random(id, random);
        let step = PigAngerStep { mob: id, anger };
        let lane = self.lane_for(None, id);
        self.fire(lane, Continuation::PigAnger(step), move |server| {
            let entity = server.entity_view(id)?;
            let target = server
                .entities
                .store
                .get(id)
                .and_then(Entity::living)
                .and_then(|living| living.mob.as_ref())
                .and_then(|mob| mob.neutral)
                .and_then(|anger| anger.target)
                .and_then(|uuid| server.entity_by_uuid(uuid))
                .and_then(|target| server.subject_view(target));
            Some(EventPayload::PigZombieAnger(PigZombieAnger {
                entity_type: entity.kind.clone(),
                entity,
                new_anger: anger,
                target,
            }))
        });
    }

    pub(crate) fn finish_pig_zombie_anger(
        &mut self,
        step: PigAngerStep,
        cancelled: bool,
        patch: PigZombieAngerPatch,
    ) {
        let game_time = self.world.level.game_time;
        let Some(entity) = self.entities.store.get_mut(step.mob) else {
            return;
        };
        let Some(anger) = entity
            .living_mut()
            .and_then(|living| living.mob.as_mut())
            .and_then(|mob| mob.neutral.as_mut())
        else {
            return;
        };
        if cancelled {
            anger.target = None;
            return;
        }
        let remaining = patch.new_anger.unwrap_or(step.anger);
        anger.set_time_to_remain_angry(game_time, i64::from(remaining));
        let state = *anger;
        rspigot_domain::entity::neutral::sync(&mut entity.base, &state);
    }

    fn entity_by_uuid(&self, uuid: uuid::Uuid) -> Option<EntityId> {
        self.players
            .values()
            .find(|player| player.profile.id.0 == uuid)
            .map(|player| player.entity_id)
            .or_else(|| self.entities.store.by_uuid(uuid))
    }

    fn piglin_state(
        &mut self,
        id: EntityId,
    ) -> Option<&mut zombified_piglin::ZombifiedPiglinState> {
        let EntityState::Living(living) = &mut self.entities.store.get_mut(id)?.state else {
            return None;
        };
        match living.mob.as_mut().map(|mob| &mut mob.species) {
            Some(Species::Zombie(zombie)) => Some(&mut zombie.piglin),
            _ => None,
        }
    }

    fn piglin_update_speed(&mut self, id: EntityId, game_time: i64) -> Option<bool> {
        let entity = self.entities.store.get_mut(id)?;
        let baby = entity.is_baby();
        let living = entity.living_mut()?;
        let angry = living
            .mob
            .as_ref()
            .and_then(|mob| mob.neutral)
            .is_some_and(|anger| anger.is_angry(game_time));
        zombified_piglin::update_attacking_speed(&mut living.attributes, angry, baby);
        Some(angry)
    }

    fn piglin_first_anger_sound(&mut self, id: EntityId) {
        let due = self
            .piglin_state(id)
            .is_some_and(|piglin| piglin.first_anger_sound_due());
        if !due {
            return;
        }
        let baby = self.entities.store.get(id).is_some_and(Entity::is_baby);
        let mut random = self.mob_random(id);
        let spread = (random.next_float() - random.next_float()) * PITCH_SPREAD;
        self.store_mob_random(id, random);
        let pitch = (spread + if baby { BABY_PITCH } else { ADULT_PITCH })
            * zombified_piglin::ANGER_SOUND_PITCH_FACTOR;
        self.entity_sound(
            id,
            ANGRY_SOUND,
            zombified_piglin::ANGER_SOUND_VOLUME_FACTOR,
            pitch,
        );
    }

    fn piglin_maybe_alert_others(&mut self, id: EntityId) {
        let due = self
            .piglin_state(id)
            .is_some_and(|piglin| piglin.alert_due());
        if !due {
            return;
        }
        if self.piglin_sees_target(id) {
            self.piglin_alert_others(id);
        }
        let mut random = self.mob_random(id);
        if let Some(piglin) = self.piglin_state(id) {
            piglin.restart_alert(&mut random);
        }
        self.store_mob_random(id, random);
    }

    fn piglin_sees_target(&mut self, id: EntityId) -> bool {
        let Some(target) = self.mob_target(id) else {
            return false;
        };
        let Some(view) = self.mob_view(id) else {
            return false;
        };
        let Some(mut entry) = self.entities.mobs.entries.remove(&id) else {
            return false;
        };
        let sees = {
            let world = self.mob_world(sky_darken(&self.world.level));
            world.living(target).is_some_and(|living| {
                entry
                    .ai
                    .brain_mut()
                    .sensing
                    .has_line_of_sight(&world, &view, &living)
            })
        };
        self.entities.mobs.entries.insert(id, entry);
        sees
    }

    fn piglin_alert_others(&mut self, id: EntityId) {
        let Some(target) = self.mob_target(id) else {
            return;
        };
        let Some(view) = self.mob_view(id) else {
            return;
        };
        let follow = view.follow_range;
        let corner: Vec3 = view.position;
        let area = Aabb::new(
            corner.x - follow,
            corner.y - ALERT_RANGE_Y,
            corner.z - follow,
            corner.x + 1.0 + follow,
            corner.y + 1.0 + ALERT_RANGE_Y,
            corner.z + 1.0 + follow,
        );
        let idle: Vec<EntityId> = {
            let world = self.mob_world(sky_darken(&self.world.level));
            world
                .living_of(TargetKind::Class("ZombifiedPiglin"), &area)
                .into_iter()
                .filter(|other| other.id != id && !other.spectator && !other.has_target)
                .map(|other| other.id)
                .collect()
        };
        for other in idle {
            if let Some(other_view) = self.entity_view(other) {
                self.request_target(other_view, target, TargetReason::TargetAttackedNearbyEntity);
            }
        }
    }
}
