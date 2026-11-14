use rspigot_domain::effect::{EffectDuration, EffectId, EffectInstance};
use rspigot_domain::entity::cloud::{self, CloudState};
use rspigot_domain::entity::{Aabb, Entity, EntityState, ParticleSetting, RemovalReason, living};
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

use crate::plugins::events::{
    AreaEffectCloudApply, CreatureSpawnReason, EntityPotionEffectCause, EntityRemoveCause,
};
use crate::plugins::steps::CloudApplyStep;
use crate::plugins::{Continuation, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

const FLAME_RADIUS: f32 = 5.0;
const FLAME_DURATION: i32 = 200;
const FLAME_SCALE: f32 = 0.25;
const BREATH_RADIUS: f32 = 3.0;
const BREATH_DURATION: i32 = 600;
const BREATH_TARGET_RADIUS: f32 = 7.0;
const BREATH_AMPLIFIER: u8 = 1;
const BREATH_EFFECT_TICKS: u32 = 1;
const SPLASH_EVENT: i32 = 2006;
const INSTANT_DAMAGE: &str = "minecraft:instant_damage";

fn breath_particle() -> ParticleSetting {
    ParticleSetting::Power {
        name: cloud::DRAGON_BREATH.to_owned(),
        power: 1.0,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn new_cloud(&mut self, position: Vec3) -> Entity {
        let mut base = self.new_entity_base(Kind::AreaEffectCloud);
        let state = cloud::define(&mut base);
        base.snap_to(position, 0.0, 0.0);
        Entity::new(base, EntityState::Cloud(Box::new(state)))
    }

    pub(super) fn dragon_spawn_flame(&mut self, dragon: EntityId, position: Vec3) {
        let Some(owner) = self
            .entities
            .store
            .get(dragon)
            .map(|entity| entity.base.uuid)
        else {
            return;
        };
        let mut entity = self.new_cloud(position);
        let id = entity.base.id;
        if let EntityState::Cloud(state) = &mut entity.state {
            state.owner = Some(owner);
            state.duration = FLAME_DURATION;
            state.custom_particle = Some(cloud::particle_tag(&breath_particle()));
            state.potion_duration_scale = FLAME_SCALE;
            if let Some(harm) = EffectId::from_name(INSTANT_DAMAGE) {
                state.add_effect(&EffectInstance::new(
                    harm,
                    EffectDuration::Ticks(0),
                    0,
                    true,
                ));
            }
            state.refresh_particle(&mut entity.base);
        }
        cloud::set_radius(&mut entity.base, FLAME_RADIUS);
        if let Some(runtime) = self.entities.dragons.get_mut(&dragon) {
            runtime.phases.set_flame(id);
        }
        self.add_entity(entity, CreatureSpawnReason::Default);
    }

    pub(super) fn dragon_discard_flame(&mut self, dragon: EntityId) {
        let Some(flame) = self
            .entities
            .dragons
            .get(&dragon)
            .and_then(|runtime| runtime.phases.flame())
        else {
            return;
        };
        self.remove_entity(
            flame,
            RemovalReason::Discarded,
            Some(EntityRemoveCause::Despawn),
        );
    }

    pub(crate) fn dragon_fireball_burst(&mut self, fireball: EntityId) {
        let Some(entity) = self.entities.store.get(fireball) else {
            return;
        };
        let position = entity.base.position;
        let silent = entity.base.is_silent();
        let area = entity.base.bounding_box().inflate_by(4.0, 2.0, 4.0);
        let owner = match &entity.state {
            EntityState::Hurting(state) => state.owner,
            _ => None,
        };
        let owner_living = owner
            .and_then(|uuid| self.owner_entity(Some(uuid)))
            .is_some_and(|owner| self.owner_is_living(owner));
        let mut cloud_entity = self.new_cloud(position);
        if let EntityState::Cloud(state) = &mut cloud_entity.state {
            if owner_living {
                state.owner = owner;
            }
            state.custom_particle = Some(cloud::particle_tag(&breath_particle()));
            state.duration = BREATH_DURATION;
            state.radius_per_tick = (BREATH_TARGET_RADIUS - BREATH_RADIUS) / BREATH_DURATION as f32;
            state.potion_duration_scale = FLAME_SCALE;
            if let Some(harm) = EffectId::from_name(INSTANT_DAMAGE) {
                state.add_effect(&EffectInstance::new(
                    harm,
                    EffectDuration::Ticks(BREATH_EFFECT_TICKS),
                    BREATH_AMPLIFIER,
                    true,
                ));
            }
            state.refresh_particle(&mut cloud_entity.base);
        }
        cloud::set_radius(&mut cloud_entity.base, BREATH_RADIUS);
        let near = self
            .living_positions_in(&area)
            .into_iter()
            .find(|(_, at)| at.distance_squared(position) < 16.0);
        if let Some((_, at)) = near {
            cloud_entity.base.position = at;
        }
        self.level_event(SPLASH_EVENT, position.block(), if silent { -1 } else { 1 });
        self.add_entity(cloud_entity, CreatureSpawnReason::Default);
        self.remove_entity(
            fireball,
            RemovalReason::Discarded,
            Some(EntityRemoveCause::Hit),
        );
    }

    fn living_positions_in(&self, area: &Aabb) -> Vec<(EntityId, Vec3)> {
        let mut found: Vec<(EntityId, Vec3)> = self
            .players
            .values()
            .filter(|player| !player.vitals.is_dead())
            .filter(|player| {
                Aabb::of_size(player.position, 0.6, player.height() as f32).intersects(area)
            })
            .map(|player| (player.entity_id, player.position))
            .collect();
        found.extend(
            self.entities
                .store
                .iter()
                .filter(|other| !other.base.is_removed())
                .filter(|other| other.base.bounding_box().intersects(area))
                .filter(|other| other.living().is_some())
                .map(|other| (other.base.id, other.base.position)),
        );
        found
    }

    pub(crate) fn tick_cloud(&mut self, id: EntityId) {
        let Some(Entity {
            base,
            state: EntityState::Cloud(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        base.old_position = base.position;
        base.tick_count = base.tick_count.wrapping_add(1);
        let tick = base.tick_count;
        if state.duration != -1 && tick - state.wait_time >= state.duration {
            self.remove_cloud(id);
            return;
        }
        let waiting = tick < state.wait_time;
        if cloud::is_waiting(base) != waiting {
            cloud::set_waiting(base, waiting);
        }
        if waiting {
            return;
        }
        let mut radius = cloud::radius(base);
        if state.radius_per_tick != 0.0 {
            radius += state.radius_per_tick;
            if radius < cloud::MIN_RADIUS {
                self.remove_cloud(id);
                return;
            }
            cloud::set_radius(base, radius);
        }
        if tick % cloud::APPLY_INTERVAL != 0 {
            return;
        }
        state.victims.retain(|(_, until)| *until > tick);
        if !state.has_effects() {
            state.victims.clear();
            return;
        }
        let effects = state.scaled_effects();
        let area = base.bounding_box();
        let center = base.position;
        let victims: Vec<EntityId> = state.victims.iter().map(|(victim, _)| *victim).collect();
        let affected: Vec<EntityId> = self
            .living_positions_in(&area)
            .into_iter()
            .filter(|(target, _)| !victims.contains(target))
            .filter(|(target, _)| self.cloud_can_affect(*target))
            .filter(|(_, at)| {
                let dx = at.x - center.x;
                let dz = at.z - center.z;
                dx * dx + dz * dz <= f64::from(radius * radius)
            })
            .map(|(target, _)| target)
            .collect();
        if affected.is_empty() {
            return;
        }
        let lane = self.lane_for(None, id);
        let step = CloudApplyStep {
            cloud: id,
            affected: affected.clone(),
            effects,
        };
        self.fire(
            lane,
            Continuation::CloudApply(Box::new(step)),
            move |server| {
                let entity = server.entity_view(id)?;
                Some(EventPayload::AreaEffectCloudApply(AreaEffectCloudApply {
                    affected_entities: affected
                        .iter()
                        .filter_map(|target| server.subject_view(*target))
                        .collect(),
                    entity_type: entity.kind.clone(),
                    entity,
                }))
            },
        );
    }

    fn cloud_can_affect(&self, target: EntityId) -> bool {
        if self.wither_refuses_effects(target) {
            return false;
        }
        if self
            .players
            .values()
            .any(|player| player.entity_id == target)
        {
            return true;
        }
        self.entities.store.get(target).is_some_and(|entity| {
            entity
                .living()
                .is_some_and(|state| !living::is_dead_or_dying(&entity.base, state))
        })
    }

    fn remove_cloud(&mut self, id: EntityId) {
        self.remove_entity(
            id,
            RemovalReason::Discarded,
            Some(EntityRemoveCause::Despawn),
        );
    }

    pub(crate) fn finish_cloud_apply(&mut self, step: CloudApplyStep, cancelled: bool) {
        if cancelled {
            return;
        }
        let CloudApplyStep {
            cloud: id,
            affected,
            effects,
        } = step;
        let owner = match self.entities.store.get(id).map(|entity| &entity.state) {
            Some(EntityState::Cloud(state)) => state.owner,
            _ => return,
        };
        let owner = self.owner_entity(owner);
        for target in affected {
            let Some(Entity {
                base,
                state: EntityState::Cloud(state),
                ..
            }) = self.entities.store.get_mut(id)
            else {
                return;
            };
            let until = base.tick_count + state.reapplication_delay;
            state.victims.retain(|(victim, _)| *victim != target);
            state.victims.push((target, until));
            for effect in &effects {
                let Some(kind) = effect.effect.kind() else {
                    continue;
                };
                if kind.instantaneous() {
                    self.splash_instant(
                        id,
                        owner,
                        target,
                        kind,
                        effect.amplifier,
                        cloud::INSTANT_SCALE,
                    );
                } else {
                    self.add_effect_to(target, *effect, EntityPotionEffectCause::AreaEffectCloud);
                }
            }
            if !self.cloud_spend_use(id) {
                return;
            }
        }
    }

    fn cloud_spend_use(&mut self, id: EntityId) -> bool {
        let Some(Entity {
            base,
            state: EntityState::Cloud(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return false;
        };
        let CloudState {
            radius_on_use,
            duration_on_use,
            ..
        } = **state;
        if radius_on_use != 0.0 {
            let radius = cloud::radius(base) + radius_on_use;
            if radius < cloud::MIN_RADIUS {
                self.remove_cloud(id);
                return false;
            }
            cloud::set_radius(base, radius);
        }
        if duration_on_use != 0 && state.duration != -1 {
            state.duration += duration_on_use;
            if state.duration < 1 {
                self.remove_cloud(id);
                return false;
            }
        }
        true
    }
}
