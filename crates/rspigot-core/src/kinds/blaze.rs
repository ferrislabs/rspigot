use rspigot_ai::{AiWorld, BlazeAction, MobView, mob_can_attack};
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::Entity;
use rspigot_domain::entity::species::{self, Species, blaze};
use rspigot_domain::random::RandomSource;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

use crate::health::DamageSource;
use crate::mobs::sky_darken;
use crate::ports::ClientLink;
use crate::server::Server;

const WATER_DAMAGE: f32 = 1.0;

fn state_of(entity: &mut Entity) -> Option<&mut species::blaze::BlazeState> {
    match entity
        .living_mut()
        .and_then(|living| living.mob.as_mut())
        .map(|mob| &mut mob.species)
    {
        Some(Species::Blaze(state)) => Some(state),
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn blaze_action(&mut self, id: EntityId, action: BlazeAction) {
        match action {
            BlazeAction::SetCharged(charged) => {
                if let Some(entity) = self.entities.store.get_mut(id) {
                    blaze::set_charged(&mut entity.base, charged);
                }
            }
            BlazeAction::Fireball(direction) => self.blaze_shoot(id, direction),
        }
    }

    fn blaze_shoot(&mut self, id: EntityId, direction: Vec3) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let base = &entity.base;
        let height_offset = f64::from(base.dimensions.height) * 0.5 + 0.5;
        let position = Vec3::new(
            base.position.x,
            base.position.y + height_offset,
            base.position.z,
        );
        self.launch_hurting(Kind::SmallFireball, id, direction, position, 1);
    }

    pub(crate) fn blaze_view(&self, _entity: &Entity, _view: &mut MobView) {}

    pub(crate) fn blaze_pre_tick(&mut self, _id: EntityId) -> bool {
        true
    }

    pub(crate) fn blaze_ai_step(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let base = &mut entity.base;
        if !base.on_ground && base.velocity.y < 0.0 {
            base.velocity = base.velocity.multiply(1.0, blaze::FALL_DRAG, 1.0);
        }
    }

    pub(crate) fn blaze_custom_ai_step(&mut self, id: EntityId) {
        let mut random = self.mob_random(id);
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let Some(state) = state_of(entity) else {
            return;
        };
        state.next_height_offset_change_tick -= 1;
        if state.next_height_offset_change_tick < 1 {
            state.next_height_offset_change_tick = blaze::HEIGHT_OFFSET_INTERVAL;
            state.allowed_height_offset =
                random.triangle(blaze::HEIGHT_OFFSET_MEAN, blaze::HEIGHT_OFFSET_SPREAD) as f32;
        }
        let allowed = f64::from(state.allowed_height_offset);
        let eye = entity.base.eye_y();
        let velocity = entity.base.velocity;
        self.store_mob_random(id, random);
        let Some(target) = self.mob_target(id) else {
            return;
        };
        let Some((_, target_eye)) = self.entity_eye(target) else {
            return;
        };
        if eye + allowed >= target_eye || !self.blaze_can_attack(id, target) {
            return;
        }
        if let Some(entity) = self.entities.store.get_mut(id) {
            entity.base.velocity = velocity
                + Vec3::new(
                    0.0,
                    (blaze::CLIMB_TARGET - velocity.y) * blaze::CLIMB_BLEND,
                    0.0,
                );
            entity.base.needs_sync = true;
        }
    }

    fn blaze_can_attack(&self, id: EntityId, target: EntityId) -> bool {
        let Some(mob) = self.mob_view(id) else {
            return false;
        };
        let world = self.mob_world(sky_darken(&self.world.level));
        world
            .living(target)
            .is_some_and(|living| mob_can_attack(&mob, &world, &living))
    }

    pub(crate) fn blaze_post_tick(&mut self, id: EntityId) {
        self.water_sensitive_damage(id);
    }

    pub(crate) fn water_sensitive_damage(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let wet = entity.base.fluid.in_water || self.is_in_rain(&entity.base);
        if wet && self.living_entity_alive(id) {
            self.hurt_mob(id, &DamageSource::new(DamageType::Drown), WATER_DAMAGE);
        }
    }
}
