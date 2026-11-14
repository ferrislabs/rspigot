use rand::Rng;
use rspigot_ai::{MobView, ShulkerAction, ShulkerFacts, TargetReason};
use rspigot_domain::entity::living::{self, LivingState};
use rspigot_domain::entity::physics::{self, MoveTraits};
use rspigot_domain::entity::species::{Species, shulker};
use rspigot_domain::entity::{Aabb, Entity, EntityBase, EntityState};
use rspigot_domain::random::{LegacyRandom, RandomSource};
use rspigot_domain::spawn::block_within_border;
use rspigot_domain::{BlockPos, ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

use crate::health::DamageSource;
use crate::mob_actions::new_mob;
use crate::plugins::events::CreatureSpawnReason;
use crate::plugins::{Birth, ShulkerHop};
use crate::ports::ClientLink;
use crate::server::Server;

const OPEN_SOUND: &str = "minecraft:entity.shulker.open";
const CLOSE_SOUND: &str = "minecraft:entity.shulker.close";
const SHOOT_SOUND: &str = "minecraft:entity.shulker.shoot";
const TELEPORT_SOUND: &str = "minecraft:entity.shulker.teleport";
const HURT_SOUND: &str = "minecraft:entity.shulker.hurt";
const PROJECTILE_DAMAGE: &str = "minecraft:is_projectile";
const PACK_RADIUS: i32 = 8;
const CHUNK_MARGIN: f64 = 1.0e-7;

fn collidable(entity: &Entity) -> bool {
    let kind = entity.base.kind;
    if kind == Kind::Shulker {
        return entity
            .living()
            .is_some_and(|state| !living::is_dead_or_dying(&entity.base, state));
    }
    kind.extends("AbstractBoat") || kind.extends("AbstractMinecart")
}

fn packed_difference(from: BlockPos, to: BlockPos) -> i32 {
    let diff = |a: i32, b: i32| (b - a).clamp(-PACK_RADIUS, PACK_RADIUS);
    let pack = |value: i32| (value + PACK_RADIUS) & 0xFF;
    (pack(diff(from.x, to.x)) << 16) | (pack(diff(from.y, to.y)) << 8) | pack(diff(from.z, to.z))
}

impl<L: ClientLink> Server<L> {
    fn entities_touching(&self, area: &Aabb) -> Vec<&Entity> {
        let wide = area.inflate(CHUNK_MARGIN);
        let (x0, x1) = (
            (wide.min_x.floor() as i32) >> 4,
            (wide.max_x.floor() as i32) >> 4,
        );
        let (z0, z1) = (
            (wide.min_z.floor() as i32) >> 4,
            (wide.max_z.floor() as i32) >> 4,
        );
        let mut found = Vec::new();
        for x in x0..=x1 {
            for z in z0..=z1 {
                found.extend(
                    self.entities
                        .store
                        .in_chunk(ChunkPos::new(x, z))
                        .iter()
                        .filter_map(|id| self.entities.store.get(*id))
                        .filter(|entity| {
                            !entity.base.is_removed()
                                && entity.base.bounding_box().intersects(&wide)
                        }),
                );
            }
        }
        found
    }

    fn shulker_free(&self, me: EntityId, area: &Aabb) -> bool {
        !self
            .entities_touching(area)
            .into_iter()
            .any(|other| other.base.id != me && collidable(other))
    }

    fn with_shulker<R>(
        &mut self,
        id: EntityId,
        run: impl FnOnce(&mut EntityBase, &mut LivingState, &mut shulker::ShulkerState) -> R,
    ) -> Option<R> {
        let Entity {
            base,
            state: EntityState::Living(living),
            ..
        } = self.entities.store.get_mut(id)?
        else {
            return None;
        };
        let Species::Shulker(mut state) = living.mob.as_ref()?.species else {
            return None;
        };
        let result = run(base, living, &mut state);
        if let Some(mob) = living.mob.as_mut() {
            mob.species = Species::Shulker(state);
        }
        Some(result)
    }

    pub(crate) fn shulker_view(&self, entity: &Entity, view: &mut MobView) {
        let base = &entity.base;
        let face = shulker::attach_face(base);
        let here = base.position.block();
        view.species.shulker = ShulkerFacts {
            attach_face: face,
            raw_peek: shulker::raw_peek(base),
            can_stay: shulker::can_stay_at(&self.world.chunks, here, here, face, |area| {
                self.shulker_free(base.id, area)
            }),
        };
    }

    pub(crate) fn shulker_action(&mut self, id: EntityId, action: ShulkerAction) {
        match action {
            ShulkerAction::SetRawPeek(amount) => self.shulker_set_raw_peek(id, amount),
            ShulkerAction::Shoot {
                target,
                axis,
                pitch,
            } => {
                self.launch_shulker_bullet(id, target, axis);
                self.entity_sound(id, SHOOT_SOUND, shulker::SHOOT_SOUND_VOLUME, pitch);
            }
        }
    }

    fn shulker_set_raw_peek(&mut self, id: EntityId, amount: i32) {
        let Some(change) = self.with_shulker(id, |base, living, _| {
            shulker::set_raw_peek(base, living, amount)
        }) else {
            return;
        };
        let sound = match change {
            shulker::PeekChange::Opened => OPEN_SOUND,
            shulker::PeekChange::Closed => CLOSE_SOUND,
        };
        self.entity_sound(id, sound, 1.0, 1.0);
    }

    pub(crate) fn shulker_post_tick(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let here = entity.base.position.block();
        let face = shulker::attach_face(&entity.base);
        let stays = shulker::can_stay_at(&self.world.chunks, here, here, face, |area| {
            self.shulker_free(id, area)
        });
        if !stays {
            self.shulker_find_new_attachment(id);
        }
        let changed = self
            .with_shulker(id, |base, _, state| {
                let changed = shulker::update_peek(base, state);
                if changed {
                    shulker::refresh_shape(base, state);
                }
                changed
            })
            .unwrap_or(false);
        if changed {
            self.shulker_push_entities(id);
        }
    }

    fn shulker_find_new_attachment(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let here = entity.base.position.block();
        let surface = shulker::find_attachable_surface(&self.world.chunks, here, here, &|area| {
            self.shulker_free(id, area)
        });
        match surface {
            Some(face) => {
                self.with_shulker(id, |base, _, state| {
                    shulker::set_attach_face(base, state, face);
                });
            }
            None => {
                self.shulker_teleport(id, None);
            }
        }
    }

    fn shulker_push_entities(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(Species::Shulker(state)) = entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .map(|mob| mob.species)
        else {
            return;
        };
        let (lid, push) = shulker::peek_push(&entity.base, &state);
        if push <= 0.0 {
            return;
        }
        let area = shulker::peek_sweep(&entity.base, &state);
        let (dx, dy, dz) = lid.offset();
        let delta = Vec3::new(
            f64::from(push * dx as f32),
            f64::from(push * dy as f32),
            f64::from(push * dz as f32),
        );
        let movable: Vec<EntityId> = self
            .entities_touching(&area)
            .into_iter()
            .filter(|other| {
                other.base.id != id && other.base.kind != Kind::Shulker && !other.base.no_physics
            })
            .map(|other| other.base.id)
            .collect();
        for target in movable {
            let chunks = &self.world.chunks;
            let Some(Entity { base, state, .. }) = self.entities.store.get_mut(target) else {
                continue;
            };
            let traits = match state {
                EntityState::Living(living) => living.move_traits(base),
                _ => MoveTraits::plain(0.0),
            };
            physics::move_entity(base, chunks, delta, &traits);
            base.needs_sync = true;
            self.entities.store.reindex(target);
        }
    }

    pub(crate) fn shulker_teleport(&mut self, id: EntityId, breed: Option<(Vec3, Aabb)>) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return false;
        };
        let alive = entity
            .living()
            .is_some_and(|state| !living::is_dead_or_dying(&entity.base, state));
        if living::is_no_ai(&entity.base) || !alive {
            return false;
        }
        let origin = entity.base.position.block();
        let min_y = self.world.dimension.height.min_y;
        let mut random = self.mob_random(id);
        let hop = shulker::find_teleport(
            &self.world.chunks,
            &mut random,
            origin,
            min_y,
            block_within_border,
            |area| self.shulker_free(id, area),
        );
        self.store_mob_random(id, random);
        let Some(hop) = hop else {
            return false;
        };
        let to = Vec3::new(
            f64::from(hop.target.x),
            f64::from(hop.target.y),
            f64::from(hop.target.z),
        );
        self.relocate_shulker(
            id,
            to,
            ShulkerHop {
                face: hop.face,
                breed,
            },
        );
        true
    }

    pub(crate) fn finish_shulker_teleport(&mut self, id: EntityId, to: Vec3, hop: ShulkerHop) {
        let Some(old) = self.entities.store.get(id).map(|e| e.base.position.block()) else {
            return;
        };
        let target = to.block();
        self.with_shulker(id, |base, _, state| {
            shulker::set_attach_face(base, state, hop.face);
        });
        self.entity_sound(id, TELEPORT_SOUND, 1.0, 1.0);
        let landing = Vec3::new(
            f64::from(target.x) + 0.5,
            f64::from(target.y),
            f64::from(target.z) + 0.5,
        );
        if let Some(entity) = self.entities.store.get_mut(id) {
            let base = &mut entity.base;
            shulker::set_position(base, landing);
            base.data.set(
                shulker::DATA_PEEK,
                rspigot_domain::entity::SyncedValue::Byte(0),
            );
            base.needs_sync = true;
        }
        self.entities.store.reindex(id);
        let now = self
            .entities
            .store
            .get(id)
            .map_or(target, |entity| entity.base.position.block());
        self.level_event(
            shulker::TELEPORT_LEVEL_EVENT,
            old,
            packed_difference(old, now),
        );
        self.forget_target(id, TargetReason::Unknown);
        if let Some((from, area)) = hop.breed {
            self.shulker_breed(id, from, &area);
        }
    }

    fn shulker_breed(&mut self, parent: EntityId, from: Vec3, area: &Aabb) {
        let nearby = self
            .entities_touching(&area.inflate(shulker::BREEDING_RADIUS))
            .into_iter()
            .filter(|other| other.base.kind == Kind::Shulker && collidable(other))
            .count();
        let chance = shulker::breeding_chance(nearby);
        if self.rng.random::<f32>() < chance {
            return;
        }
        let Some(color) = self
            .entities
            .store
            .get(parent)
            .map(|entity| shulker::color(&entity.base))
        else {
            return;
        };
        let mut child = new_mob(self.new_entity_base(Kind::Shulker));
        shulker::set_color(&mut child.base, color);
        let (yaw, pitch) = (child.base.yaw, child.base.pitch);
        child.base.snap_to(from, yaw, pitch);
        let own = LegacyRandom::new(self.rng.random());
        self.spawn_entity(
            child,
            Birth::of(CreatureSpawnReason::Breeding).with_random(own),
        );
    }

    pub(crate) fn shulker_hurt_amount(
        &mut self,
        id: EntityId,
        source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        let closed = self
            .entities
            .store
            .get(id)
            .is_some_and(|entity| shulker::is_closed(&entity.base));
        let arrow = source
            .direct
            .as_ref()
            .is_some_and(|direct| direct.kind.extends("AbstractArrow"));
        (!(closed && arrow)).then_some(amount)
    }

    pub(crate) fn shulker_after_hurt(&mut self, id: EntityId, source: &DamageSource, dead: bool) {
        if dead {
            return;
        }
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let health = living::health(&entity.base);
        let Some(max_health) = self
            .entities
            .store
            .get_mut(id)
            .and_then(Entity::living_mut)
            .map(|state| state.max_health() as f32)
        else {
            return;
        };
        if health < max_health * shulker::TELEPORT_HEALTH_RATIO {
            let mut random = self.mob_random(id);
            let roll = random.next_int(shulker::TELEPORT_ODDS);
            self.store_mob_random(id, random);
            if shulker::should_teleport_when_hurt(health, max_health, roll) {
                self.shulker_teleport(id, None);
                return;
            }
        }
        let by_bullet = source.kind.in_tag(PROJECTILE_DAMAGE)
            && source
                .direct
                .as_ref()
                .is_some_and(|direct| direct.kind == Kind::ShulkerBullet);
        if by_bullet {
            self.shulker_hit_by_bullet(id);
        }
    }

    fn shulker_hit_by_bullet(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if shulker::is_closed(&entity.base) {
            return;
        }
        let from = entity.base.position;
        let area = entity.base.bounding_box();
        self.shulker_teleport(id, Some((from, area)));
    }

    pub(crate) fn shulker_hurt_sound(&mut self, id: EntityId) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return false;
        };
        if shulker::is_closed(&entity.base) {
            return false;
        }
        self.entity_sound(id, HURT_SOUND, 1.0, 1.0);
        true
    }
}
