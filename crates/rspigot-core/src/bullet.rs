use rand::Rng;
use rspigot_blocks::Face;
use rspigot_domain::damage::DamageType;
use rspigot_domain::effect::{EffectDuration, EffectId, EffectInstance};
use rspigot_domain::entity::bullet::{self, BulletState, Pursuit, Quarry};
use rspigot_domain::entity::species::shulker;
use rspigot_domain::entity::{Axis, Entity, EntityState, RemovalReason, arrow, hurting, raycast};
use rspigot_domain::particle::Particle;
use rspigot_domain::player::GameMode;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};

use crate::events::ClientEvent;
use crate::health::DamageSource;
use crate::plugins::events::{CreatureSpawnReason, EntityPotionEffectCause, EntityRemoveCause};
use crate::plugins::{AfterHurt, BulletFollow};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use crate::thrown::Strike;

const HIT_SOUND: &str = "minecraft:entity.shulker_bullet.hit";
const HURT_SOUND: &str = "minecraft:entity.shulker_bullet.hurt";
const EXPLOSION_PARTICLE: &str = "minecraft:explosion";
const CRIT_PARTICLE: &str = "minecraft:crit";
const PARTICLE_RANGE: f64 = 32.0;

impl<L: ClientLink> Server<L> {
    pub(crate) fn launch_shulker_bullet(
        &mut self,
        owner: EntityId,
        target: EntityId,
        avoid: Axis,
    ) -> bool {
        let Some(source) = self.entities.store.get(owner) else {
            return false;
        };
        let owner_uuid = source.base.uuid;
        let center = source.base.bounding_box().center();
        let Some((target_uuid, quarry)) = self.quarry_of(target) else {
            return false;
        };
        let seed = self.rng.random::<i64>();
        let mut base = self.new_entity_base(Kind::ShulkerBullet);
        base.position = center;
        base.old_position = center;
        let mut state = BulletState::new(&mut base, seed);
        state.owner = Some(owner_uuid);
        state.target = Some(target_uuid);
        bullet::launch(&mut base, &mut state, quarry, avoid, &self.world.chunks);
        let projectile = Entity::new(base, EntityState::Bullet(Box::new(state)));
        self.add_entity(projectile, CreatureSpawnReason::Default);
        true
    }

    fn quarry_of(&self, id: EntityId) -> Option<(uuid::Uuid, Quarry)> {
        if let Some(player) = self.players.values().find(|p| p.entity_id == id) {
            return Some((
                player.profile.id.0,
                Quarry {
                    position: player.position,
                    height: player.height() as f32,
                },
            ));
        }
        let entity = self.entities.store.get(id)?;
        Some((
            entity.base.uuid,
            Quarry {
                position: entity.base.position,
                height: entity.base.dimensions.height,
            },
        ))
    }

    fn pursuit_of(&self, target: Option<EntityId>) -> Pursuit {
        let Some(target) = target else {
            return Pursuit::Lost;
        };
        let spectating = self
            .players
            .values()
            .any(|p| p.entity_id == target && p.game_mode == GameMode::Spectator);
        let alive = self
            .players
            .values()
            .find(|p| p.entity_id == target)
            .map_or_else(
                || {
                    self.entities
                        .store
                        .get(target)
                        .is_some_and(|entity| !entity.base.is_removed())
                },
                |player| !player.vitals.is_dead(),
            );
        if !alive || spectating {
            Pursuit::Falling
        } else {
            Pursuit::Homing
        }
    }

    pub(crate) fn tick_bullet(&mut self, id: EntityId) {
        if self.projectile_waits(id) {
            return;
        }
        if self.world.difficulty == rspigot_domain::world::Difficulty::Peaceful {
            self.remove_entity(
                id,
                RemovalReason::Discarded,
                Some(EntityRemoveCause::Despawn),
            );
            return;
        }
        let Some(Entity {
            base,
            state: EntityState::Bullet(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        base.old_position = base.position;
        base.tick_count = base.tick_count.wrapping_add(1);
        state.has_been_shot = true;
        let target_uuid = state.target;
        let owner_uuid = state.owner;
        let left_owner = state.left_owner;
        let target = self.owner_entity(target_uuid);
        let left_owner = left_owner || self.bullet_left_owner(id, owner_uuid);
        let pursuit = self.pursuit_of(target);
        let quarry = target
            .and_then(|target| self.quarry_of(target))
            .map(|(_, q)| q);
        let Some(Entity {
            base,
            state: EntityState::Bullet(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        state.left_owner = left_owner;
        if target.is_none() {
            state.target = None;
        }
        bullet::accelerate(base, state, pursuit);
        let from = base.position;
        let to = from + base.velocity;
        let margin = f64::from(arrow::hit_margin(base.tick_count));
        let area = arrow::search_box(base);
        let strike = self.find_strike(id, owner_uuid, left_owner, from, to, margin, &area);
        let chunks = &self.world.chunks;
        let Some(Entity {
            base,
            state: EntityState::Bullet(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        base.position = to;
        hurting::rotate_towards_movement(base, bullet::ROTATION_SPEED);
        if let Some(quarry) = quarry {
            bullet::steer(base, state, quarry, chunks, |pos| {
                shulker::can_stand_on_face(chunks, pos, Face::Up)
            });
        }
        match strike.0 {
            Strike::Miss => {}
            Strike::Entity(target) => self.ask_projectile_hit(id, target),
            Strike::Block(hit) => self.bullet_hit_block(id, hit),
        }
    }

    fn bullet_left_owner(&self, id: EntityId, owner: Option<uuid::Uuid>) -> bool {
        let Some(Entity {
            base,
            state: EntityState::Bullet(state),
            ..
        }) = self.entities.store.get(id)
        else {
            return false;
        };
        if state.left_owner {
            return false;
        }
        let area = arrow::search_box(base);
        self.owner_entity(owner)
            .and_then(|owner| self.entity_box(owner))
            .is_none_or(|(_, _, bounds)| !bounds.intersects(&area))
    }

    fn bullet_hit_block(&mut self, id: EntityId, hit: raycast::BlockHit) {
        self.notice_block_hit(id, hit.pos, hit.face);
        let Some(position) = self.entities.store.get(id).map(|e| e.base.position) else {
            return;
        };
        self.bullet_particles(position, EXPLOSION_PARTICLE, bullet::HIT_PARTICLES);
        self.play_sound(None, position, HIT_SOUND, SoundSource::Hostile, 1.0, 1.0);
        self.destroy_bullet(id, EntityRemoveCause::Hit);
    }

    fn bullet_particles(&mut self, position: Vec3, kind: &'static str, count: i32) {
        let spread = Vec3::new(
            bullet::PARTICLE_SPREAD,
            bullet::PARTICLE_SPREAD,
            bullet::PARTICLE_SPREAD,
        );
        self.push(
            Audience::Nearby {
                position,
                range: PARTICLE_RANGE,
                except: None,
            },
            ClientEvent::Particle(Particle {
                count,
                spread,
                ..Particle::single(kind, position)
            }),
        );
    }

    fn destroy_bullet(&mut self, id: EntityId, cause: EntityRemoveCause) {
        self.remove_entity(id, RemovalReason::Discarded, Some(cause));
    }

    pub(crate) fn bullet_hit_entity(&mut self, id: EntityId, target: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let EntityState::Bullet(state) = &entity.state else {
            return;
        };
        let velocity = entity.base.velocity;
        let owner = self.owner_entity(state.owner);
        let causing = owner.and_then(|owner| self.source_for(owner));
        let Some(direct) = self.mob_source(id) else {
            return;
        };
        let source = DamageSource {
            kind: DamageType::MobProjectile,
            causing,
            direct: Some(direct),
            position: None,
            knockback: Some((-velocity.x, -velocity.z)),
            direct_block: None,
        };
        let victim = self.struck(target);
        let follow = BulletFollow {
            projectile: id,
            target,
        };
        self.strike(
            None,
            victim,
            &source,
            bullet::DAMAGE,
            AfterHurt::Bullet(follow),
        );
    }

    pub(crate) fn finish_bullet_hit(&mut self, follow: BulletFollow, hurt: bool) {
        let BulletFollow { projectile, target } = follow;
        if hurt
            && self.is_living_target(target)
            && let Some(levitation) = EffectId::from_name("minecraft:levitation")
        {
            let effect = EffectInstance::new(
                levitation,
                EffectDuration::Ticks(bullet::LEVITATION_TICKS),
                0,
                true,
            );
            let cause = EntityPotionEffectCause::Attack;
            match self.player_of_entity(target) {
                Some(player) => {
                    self.apply_effect(player, effect, Some(cause));
                }
                None => {
                    self.apply_entity_effect(target, effect, cause);
                }
            }
        }
        if self.entities.store.get(projectile).is_some() {
            self.destroy_bullet(projectile, EntityRemoveCause::Hit);
        }
    }

    pub(crate) fn bullet_shot_down(&mut self, id: EntityId) -> bool {
        let Some(position) = self.entities.store.get(id).map(|e| e.base.position) else {
            return false;
        };
        self.play_sound(None, position, HURT_SOUND, SoundSource::Hostile, 1.0, 1.0);
        self.bullet_particles(position, CRIT_PARTICLE, bullet::HURT_PARTICLES);
        self.destroy_bullet(id, EntityRemoveCause::Death);
        true
    }
}
