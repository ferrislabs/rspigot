use rand::Rng;
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::thrown::{self, ThrownState};
use rspigot_domain::entity::{Entity, EntityState, RemovalReason, arrow, fire, fluid, raycast};
use rspigot_domain::item::ItemStack;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

use crate::events::{ClientEvent, EntityStatus};
use crate::health::DamageSource;
use crate::plugins::AfterHurt;
use crate::plugins::events::{CreatureSpawnReason, EntityRemoveCause};
use crate::plugins::steps::ThrownFollow;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

#[derive(Debug, Clone, Copy)]
pub(crate) enum Strike {
    Miss,
    Block(raycast::BlockHit),
    Entity(EntityId),
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn launch_thrown(
        &mut self,
        kind: Kind,
        owner: EntityId,
        aim: impl FnOnce(Vec3) -> Vec3,
        power: f32,
        uncertainty: f32,
    ) {
        self.launch_thrown_item(kind, owner, None, aim, power, uncertainty);
    }

    pub(crate) fn launch_thrown_item(
        &mut self,
        kind: Kind,
        owner: EntityId,
        item: Option<&ItemStack>,
        aim: impl FnOnce(Vec3) -> Vec3,
        power: f32,
        uncertainty: f32,
    ) {
        let Some(source) = self.entities.store.get(owner) else {
            return;
        };
        let owner_uuid = source.base.uuid;
        let origin = Vec3::new(
            source.base.position.x,
            source.base.eye_y() - thrown::OWNER_EYE_OFFSET,
            source.base.position.z,
        );
        let mut base = self.new_entity_base(kind);
        base.position = origin;
        base.old_position = origin;
        let seed = self.rng.random::<i64>();
        let mut state = ThrownState::new(&mut base, seed);
        if let Some(item) = item {
            thrown::set_item(&mut base, item);
        }
        state.owner = Some(owner_uuid);
        let direction = aim(origin);
        thrown::shoot(&mut base, &mut state, direction, power, uncertainty);
        let projectile = Entity::new(base, EntityState::Thrown(Box::new(state)));
        self.add_entity(projectile, CreatureSpawnReason::Default);
    }

    pub(crate) fn tick_thrown(&mut self, id: EntityId) {
        if self.projectile_waits(id) {
            return;
        }
        let in_rain = self
            .entities
            .store
            .get(id)
            .is_some_and(|entity| self.is_in_rain(&entity.base));
        let Some(Entity {
            base,
            state: EntityState::Thrown(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        base.old_position = base.position;
        base.tick_count = base.tick_count.wrapping_add(1);
        thrown::apply_gravity(base);
        thrown::apply_inertia(base);
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
            state: EntityState::Thrown(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        base.position = location;
        thrown::update_rotation(base);
        fluid::update(base, chunks);
        if base.fluid.in_water || in_rain {
            fire::clear_fire(base);
        }
        state.has_been_shot = true;
        self.apply_fire_inside(id);
        self.check_thrown_left_owner(id);
        let Some(Entity {
            base,
            state: EntityState::Thrown(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        fire::base_tick(base);
        let on_fire = base.remaining_fire_ticks > 0;
        fire::set_on_fire_flag(base, on_fire);
        state.left_owner_checked = false;
        match strike.0 {
            Strike::Miss => {}
            Strike::Entity(target) => self.ask_projectile_hit(id, target),
            Strike::Block(hit) => self.thrown_hit_block(id, hit),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn find_strike(
        &self,
        id: EntityId,
        owner: Option<uuid::Uuid>,
        left_owner: bool,
        from: Vec3,
        to: Vec3,
        margin: f64,
        area: &rspigot_domain::entity::Aabb,
    ) -> (Strike, Vec3) {
        let block = raycast::clip_blocks(&self.world.chunks, from, to);
        let end = block.map_or(to, |hit| hit.location);
        let owner = self.owner_entity(owner);
        let nearest = self
            .hit_candidates(id, owner, left_owner, &[], area)
            .into_iter()
            .filter_map(|(target, bounds, _)| {
                raycast::clip_box(&bounds.inflate(margin), from, end).map(|at| (target, at))
            })
            .min_by(|(_, one), (_, other)| {
                from.distance_squared(*one)
                    .total_cmp(&from.distance_squared(*other))
            });
        match (nearest, block) {
            (Some((target, at)), _) => (Strike::Entity(target), at),
            (None, Some(hit)) => (Strike::Block(hit), hit.location),
            (None, None) => (Strike::Miss, to),
        }
    }

    fn check_thrown_left_owner(&mut self, id: EntityId) {
        let Some(Entity {
            base,
            state: EntityState::Thrown(state),
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
        if let Some(EntityState::Thrown(state)) = self
            .entities
            .store
            .get_mut(id)
            .map(|entity| &mut entity.state)
        {
            state.left_owner = left;
            state.left_owner_checked = true;
        }
    }

    fn thrown_hit_block(&mut self, id: EntityId, hit: raycast::BlockHit) {
        self.notice_block_hit(id, hit.pos, hit.face);
        let state_at = self.world.chunks.block(hit.pos).unwrap_or_default();
        if state_at.is_air() {
            return;
        }
        if self
            .entities
            .store
            .get(id)
            .is_some_and(|entity| thrown::is_potion(entity.base.kind))
        {
            self.potion_hit_block(id, hit);
            return;
        }
        self.finish_thrown(id);
    }

    pub(crate) fn thrown_hit_entity(&mut self, id: EntityId, target: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if thrown::is_potion(entity.base.kind) {
            self.potion_hit_entity(id, target);
            return;
        }
        if !self.living_entity_alive(target) {
            self.finish_thrown(id);
            return;
        }
        let damage = match entity.base.kind {
            Kind::Snowball => {
                let blaze = self
                    .entities
                    .store
                    .get(target)
                    .is_some_and(|hit| hit.base.kind == Kind::Blaze);
                if blaze {
                    thrown::SNOWBALL_BLAZE_DAMAGE
                } else {
                    thrown::SNOWBALL_DAMAGE
                }
            }
            _ => 0.0,
        };
        let EntityState::Thrown(state) = &entity.state else {
            return;
        };
        let knockback = thrown::knockback_direction(&entity.base);
        let causing = self
            .owner_entity(state.owner)
            .and_then(|owner| self.source_for(owner));
        let Some(direct) = self.mob_source(id) else {
            return;
        };
        let source = DamageSource {
            kind: DamageType::Thrown,
            causing,
            direct: Some(direct),
            position: None,
            knockback: Some(knockback),
            direct_block: None,
        };
        let victim = self.struck(target);
        self.strike(
            None,
            victim,
            &source,
            damage,
            AfterHurt::Thrown(ThrownFollow { projectile: id }),
        );
    }

    pub(crate) fn finish_thrown_hit(&mut self, follow: ThrownFollow) {
        self.finish_thrown(follow.projectile);
    }

    fn finish_thrown(&mut self, id: EntityId) {
        if self.entities.store.get(id).is_none() {
            return;
        }
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityStatus {
                entity: id,
                status: EntityStatus::Event(thrown::HIT_EVENT),
            },
        );
        self.remove_entity(id, RemovalReason::Discarded, Some(EntityRemoveCause::Hit));
    }
}
