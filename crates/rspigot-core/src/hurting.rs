use rspigot_domain::damage::DamageType;
use rspigot_domain::effect::{EffectDuration, EffectId, EffectInstance};
use rspigot_domain::entity::hurting::{self, HurtingState};
use rspigot_domain::entity::{Entity, EntityState, RemovalReason, arrow, fire, fluid, raycast};
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

use crate::explosion::fire_state_on;
use crate::health::DamageSource;
use crate::plugins::events::{
    BlockIgnite, BlockIgniteCause, CreatureSpawnReason, EntityPotionEffectCause,
    EntityRegainHealthRegainReason, EntityRemoveCause, ExplosionPrime, ExplosionPrimePatch,
};
use crate::plugins::steps::{HurtingFollow, IgniteStep, PrimeStep};
use crate::plugins::{AfterHurt, Combuster, Continuation, EventPayload, Holder};
use crate::ports::ClientLink;
use crate::server::Server;
use crate::thrown::Strike;

const WITHER_AMPLIFIER: u8 = 1;
const TICKS_PER_SECOND: u32 = 20;

impl<L: ClientLink> Server<L> {
    pub(crate) fn launch_hurting(
        &mut self,
        kind: Kind,
        owner: EntityId,
        direction: Vec3,
        position: Vec3,
        explosion_power: i32,
    ) -> bool {
        let Some(source) = self.entities.store.get(owner) else {
            return false;
        };
        let owner_uuid = source.base.uuid;
        let (yaw, pitch) = (source.base.yaw, source.base.pitch);
        let owner_is_mob = source.living().is_some_and(|living| living.is_mob());
        let griefing = self.config.rules.mob_griefing;
        let mut base = self.new_entity_base(kind);
        base.position = position;
        base.old_position = position;
        base.set_rotation(yaw, pitch);
        let mut state = HurtingState::new(&mut base);
        state.owner = Some(owner_uuid);
        state.explosion_power = explosion_power;
        state.incendiary = match kind {
            Kind::Fireball => griefing,
            Kind::SmallFireball => !owner_is_mob || griefing,
            _ => true,
        };
        hurting::assign_directional_movement(&mut base, direction, state.acceleration_power);
        let projectile = Entity::new(base, EntityState::Hurting(Box::new(state)));
        self.add_entity(projectile, CreatureSpawnReason::Default);
        true
    }

    pub(crate) fn tick_hurting(&mut self, id: EntityId) {
        if self.projectile_waits(id) {
            return;
        }
        let in_rain = self
            .entities
            .store
            .get(id)
            .is_some_and(|entity| self.is_in_rain(&entity.base));
        let loaded = self.entities.store.get(id).is_some_and(|entity| {
            let chunk = entity.base.position.block().chunk();
            self.world.chunks.is_loaded(chunk)
        });
        let Some(Entity {
            base,
            state: EntityState::Hurting(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        base.old_position = base.position;
        base.tick_count = base.tick_count.wrapping_add(1);
        hurting::apply_inertia(base, state);
        if !loaded {
            self.remove_entity(
                id,
                RemovalReason::Discarded,
                Some(EntityRemoveCause::Despawn),
            );
            return;
        }
        let from = base.position;
        let to = from + base.velocity;
        let margin = f64::from(arrow::hit_margin(base.tick_count));
        let area = arrow::search_box(base);
        let (owner, left_owner) = (state.owner, state.left_owner);
        let strike = self.find_strike(id, owner, left_owner, from, to, margin, &area);
        let location = match strike {
            (Strike::Miss, _) => to,
            (_, location) => location,
        };
        let chunks = &self.world.chunks;
        let Some(Entity {
            base,
            state: EntityState::Hurting(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        hurting::rotate_towards_movement(base, hurting::ROTATION_SPEED);
        base.position = location;
        state.has_been_shot = true;
        fluid::update(base, chunks);
        self.apply_fire_inside(id);
        self.check_hurting_left_owner(id);
        let Some(Entity {
            base,
            state: EntityState::Hurting(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        if base.fluid.in_water || in_rain {
            fire::clear_fire(base);
        }
        fire::base_tick(base);
        let burning = base.remaining_fire_ticks > 0;
        fire::set_on_fire_flag(base, burning);
        state.left_owner_checked = false;
        if hurting::should_burn(base) {
            fire::ignite_for_ticks(base, fire::seconds_to_ticks(hurting::BURN_SECONDS));
        }
        match strike.0 {
            Strike::Miss => {}
            Strike::Entity(target) => self.ask_projectile_hit(id, target),
            Strike::Block(hit) => self.hurting_hit_block(id, hit),
        }
    }

    fn check_hurting_left_owner(&mut self, id: EntityId) {
        let Some(Entity {
            base,
            state: EntityState::Hurting(state),
            ..
        }) = self.entities.store.get(id)
        else {
            return;
        };
        if state.left_owner || state.left_owner_checked {
            return;
        }
        let area = arrow::search_box(base);
        let owner = self.owner_entity(state.owner);
        let left = owner
            .and_then(|owner| self.entity_box(owner))
            .is_none_or(|(_, _, bounds)| !bounds.intersects(&area));
        if let Some(EntityState::Hurting(state)) = self
            .entities
            .store
            .get_mut(id)
            .map(|entity| &mut entity.state)
        {
            state.left_owner = left;
            state.left_owner_checked = true;
        }
    }

    fn hurting_hit_block(&mut self, id: EntityId, hit: raycast::BlockHit) {
        self.notice_block_hit(id, hit.pos, hit.face);
        let state_at = self.world.chunks.block(hit.pos).unwrap_or_default();
        if state_at.is_air() {
            return;
        }
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let EntityState::Hurting(state) = &entity.state else {
            return;
        };
        if entity.base.kind != Kind::SmallFireball || !state.incendiary {
            self.hurting_on_hit(id);
            return;
        }
        let (dx, dy, dz) = hit.face.offset();
        let pos = hit.pos.offset(dx, dy, dz);
        let empty = self
            .world
            .chunks
            .block(pos)
            .is_some_and(BlockStateId::is_air);
        if empty {
            self.ignite_block(id, pos);
        } else {
            self.hurting_on_hit(id);
        }
    }

    fn ignite_block(&mut self, projectile: EntityId, pos: BlockPos) {
        let below = self
            .world
            .chunks
            .block(pos.offset(0, -1, 0))
            .unwrap_or_default();
        let state = fire_state_on(below);
        let Some(view) = self.entity_view(projectile) else {
            return;
        };
        let lane = self.lane_for(None, projectile);
        let step = IgniteStep {
            projectile,
            pos,
            state,
        };
        self.fire(lane, Continuation::Ignite(step), move |_| {
            Some(EventPayload::BlockIgnite(BlockIgnite {
                block: pos,
                cause: BlockIgniteCause::Fireball,
                igniting_block: None,
                igniting_entity: Some(view),
                player: None,
            }))
        });
    }

    pub(crate) fn finish_ignite(&mut self, step: IgniteStep, cancelled: bool) {
        let alive = self.entities.store.get(step.projectile).is_some();
        if !cancelled
            && alive
            && self
                .world
                .chunks
                .block(step.pos)
                .is_some_and(BlockStateId::is_air)
        {
            self.set_block(step.pos, step.state);
        }
        if alive {
            self.hurting_on_hit(step.projectile);
        }
    }

    pub(crate) fn hurting_hit_entity(&mut self, id: EntityId, target: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let EntityState::Hurting(state) = &entity.state else {
            return;
        };
        let kind = entity.base.kind;
        let velocity = entity.base.velocity;
        let owner_uuid = state.owner;
        let owner = self.owner_entity(owner_uuid);
        let owner_living = owner.is_some_and(|owner| self.owner_is_living(owner));
        let causing = owner.and_then(|owner| self.source_for(owner));
        let Some(direct) = self.mob_source(id) else {
            return;
        };
        let knockback = Some((-velocity.x, -velocity.z));
        let follow = HurtingFollow {
            projectile: id,
            target,
            owner,
            saved_fire: self.fire_ticks_of(target),
        };
        if !self.living_entity_alive(target) && kind != Kind::WitherSkull {
            self.hurting_on_hit(id);
            return;
        }
        let victim = self.struck(target);
        match kind {
            Kind::SmallFireball => self.small_fireball_hit(id, victim, follow, causing, direct),
            Kind::Fireball => {
                let kind = if causing.is_some() {
                    DamageType::Fireball
                } else {
                    DamageType::UnattributedFireball
                };
                let source = DamageSource {
                    kind,
                    causing,
                    direct: Some(direct),
                    position: None,
                    knockback,
                    direct_block: None,
                };
                self.strike(
                    None,
                    victim,
                    &source,
                    hurting::LARGE_FIREBALL_DAMAGE,
                    AfterHurt::Hurting(follow),
                );
            }
            Kind::WitherSkull => {
                let (source, amount) = if owner_living {
                    (
                        DamageSource {
                            kind: DamageType::WitherSkull,
                            causing,
                            direct: Some(direct),
                            position: None,
                            knockback,
                            direct_block: None,
                        },
                        hurting::SKULL_DAMAGE,
                    )
                } else {
                    (
                        DamageSource::new(DamageType::Magic),
                        hurting::SKULL_MAGIC_DAMAGE,
                    )
                };
                self.strike(None, victim, &source, amount, AfterHurt::Hurting(follow));
            }
            _ => self.hurting_on_hit(id),
        }
    }

    fn small_fireball_hit(
        &mut self,
        id: EntityId,
        victim: crate::plugins::Struck,
        follow: HurtingFollow,
        causing: Option<crate::health::SourceEntity>,
        direct: crate::health::SourceEntity,
    ) {
        let target = follow.target;
        let holder = self
            .player_of_entity(target)
            .map_or(Holder::Mob(target), Holder::Player);
        if let Some(view) = self.entity_view(id) {
            self.combust(
                holder,
                hurting::SMALL_FIREBALL_FIRE_SECONDS,
                Combuster::Entity(view),
            );
        }
        let lane = self.lane_for(None, target);
        let ignition_waits = self.decisions.in_flight(
            lane,
            |then| matches!(then, Continuation::Combust(step) if step.target == holder),
        );
        let velocity = self
            .entities
            .store
            .get(id)
            .map_or(Vec3::ZERO, |entity| entity.base.velocity);
        let kind = if causing.is_some() {
            DamageType::Fireball
        } else {
            DamageType::UnattributedFireball
        };
        let source = DamageSource {
            kind,
            causing,
            direct: Some(direct),
            position: None,
            knockback: Some((-velocity.x, -velocity.z)),
            direct_block: None,
        };
        let then = AfterHurt::Hurting(follow);
        if ignition_waits {
            self.strike_behind(lane, victim, source, hurting::SMALL_FIREBALL_DAMAGE, then);
        } else {
            self.strike(None, victim, &source, hurting::SMALL_FIREBALL_DAMAGE, then);
        }
    }

    pub(crate) fn finish_hurting_hit(&mut self, follow: HurtingFollow, hurt: bool) {
        let HurtingFollow {
            projectile,
            target,
            owner,
            saved_fire,
        } = follow;
        let Some(kind) = self
            .entities
            .store
            .get(projectile)
            .map(|entity| entity.base.kind)
        else {
            return;
        };
        match kind {
            Kind::SmallFireball if !hurt => self.restore_fire_ticks(target, saved_fire),
            Kind::WitherSkull => self.skull_aftermath(target, owner, hurt),
            _ => {}
        }
        self.hurting_on_hit(projectile);
    }

    fn skull_aftermath(&mut self, target: EntityId, owner: Option<EntityId>, hurt: bool) {
        if !hurt {
            return;
        }
        let owner_living = owner.is_some_and(|owner| self.owner_is_living(owner));
        if owner_living && !self.living_entity_alive(target) {
            if let Some(owner) = owner {
                self.heal_mob(
                    None,
                    owner,
                    hurting::SKULL_HEAL,
                    EntityRegainHealthRegainReason::Wither,
                );
            }
            return;
        }
        let seconds = hurting::skull_wither_seconds(self.world.difficulty.id());
        if seconds <= 0 || !self.is_living_target(target) {
            return;
        }
        let Some(wither) = EffectId::from_name("minecraft:wither") else {
            return;
        };
        let ticks = TICKS_PER_SECOND * seconds as u32;
        let effect =
            EffectInstance::new(wither, EffectDuration::Ticks(ticks), WITHER_AMPLIFIER, true);
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

    pub(crate) fn owner_is_living(&self, owner: EntityId) -> bool {
        self.players
            .values()
            .any(|player| player.entity_id == owner)
            || self
                .entities
                .store
                .get(owner)
                .is_some_and(|entity| entity.living().is_some())
    }

    pub(crate) fn is_living_target(&self, target: EntityId) -> bool {
        self.owner_is_living(target)
    }

    pub(crate) fn hurting_on_hit(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        match entity.base.kind {
            Kind::Fireball | Kind::WitherSkull => self.prime_projectile(id),
            _ => {
                self.remove_entity(id, RemovalReason::Discarded, Some(EntityRemoveCause::Hit));
            }
        }
    }

    fn prime_projectile(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let EntityState::Hurting(state) = &entity.state else {
            return;
        };
        let (radius, fire) = if entity.base.kind == Kind::WitherSkull {
            (hurting::SKULL_EXPLOSION_RADIUS, false)
        } else {
            (state.explosion_power as f32, state.incendiary)
        };
        let Some(view) = self.entity_view(id) else {
            return;
        };
        let lane = self.lane_for(None, id);
        let step = PrimeStep {
            source: id,
            radius,
            fire,
            projectile: true,
        };
        self.fire(lane, Continuation::Prime(step), move |_| {
            Some(EventPayload::ExplosionPrime(ExplosionPrime {
                entity_type: view.kind.clone(),
                entity: view,
                fire,
                radius,
            }))
        });
    }

    pub(crate) fn finish_projectile_prime(
        &mut self,
        step: PrimeStep,
        cancelled: bool,
        patch: ExplosionPrimePatch,
    ) {
        let id = step.source;
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if cancelled {
            self.remove_entity(id, RemovalReason::Discarded, Some(EntityRemoveCause::Hit));
            return;
        }
        let center = entity.base.position;
        let capped = entity.base.kind == Kind::WitherSkull && hurting::is_dangerous(&entity.base);
        let owner = match &entity.state {
            EntityState::Hurting(state) => self.owner_entity(state.owner),
            _ => None,
        };
        let Some(direct) = self.mob_source(id) else {
            return;
        };
        let causing = owner
            .filter(|owner| self.owner_is_living(*owner))
            .and_then(|owner| self.source_for(owner));
        let kind = if causing.is_some() {
            DamageType::PlayerExplosion
        } else {
            DamageType::Explosion
        };
        let damage = DamageSource {
            kind,
            causing,
            direct: Some(direct),
            position: None,
            knockback: None,
            direct_block: None,
        };
        let radius = patch.radius.unwrap_or(step.radius);
        let fire = patch.fire.unwrap_or(step.fire);
        let interaction = self.mob_block_interaction();
        let cap = capped.then_some(hurting::SKULL_RESISTANCE_CAP);
        self.explode_capped(
            Some(id),
            &damage,
            center,
            radius,
            interaction,
            fire,
            Some(id),
            cap,
        );
    }

    pub(crate) fn deflect_projectile(
        &mut self,
        id: EntityId,
        deflector: uuid::Uuid,
        look: Vec3,
    ) -> bool {
        let Some(Entity {
            base,
            state: EntityState::Hurting(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return false;
        };
        hurting::deflect_aim(base, look, 1.0);
        state.owner = Some(deflector);
        hurting::on_deflection(state, true);
        true
    }
}
