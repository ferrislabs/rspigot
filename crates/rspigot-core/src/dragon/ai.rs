use rspigot_domain::damage::DamageType;
use rspigot_domain::dragon::parts::{self, Part};
use rspigot_domain::dragon::phase::{DragonBody, DragonPhase, PhaseOutput, fly};
use rspigot_domain::entity::species::Species;
use rspigot_domain::entity::species::ender_dragon::{self, DragonState};
use rspigot_domain::entity::{Aabb, Entity, living};
use rspigot_domain::mth;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

use super::DragonRuntime;
use super::env::DragonWorld;
use crate::health::DamageSource;
use crate::plugins::events::EntityRegainHealthRegainReason;
use crate::ports::ClientLink;
use crate::server::Server;

const EYE_HEIGHT: f64 = 6.8;
const DAMAGE_SCALE: &str = "minecraft:scale";

pub(crate) fn state_of(entity: &mut Entity) -> Option<&mut DragonState> {
    match entity
        .living_mut()
        .and_then(|living| living.mob.as_mut())
        .map(|mob| &mut mob.species)
    {
        Some(Species::EnderDragon(state)) => Some(state),
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn dragon_registered(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(Species::EnderDragon(state)) = entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .map(|mob| mob.species)
        else {
            return;
        };
        let mut runtime = DragonRuntime::default();
        runtime.phases.restore(state.phase);
        let position = entity.base.position;
        runtime.parts = [position; rspigot_domain::dragon::parts::PART_COUNT];
        runtime.old_parts = runtime.parts;
        self.entities.dragons.insert(id, runtime);
        self.dragon_registered_in_fight(id);
    }

    pub(crate) fn dragon_step(&mut self, id: EntityId) {
        if !self.entities.dragons.contains_key(&id) {
            self.dragon_registered(id);
        }
        let dying = self.entities.store.get(id).is_some_and(|entity| {
            entity
                .living()
                .is_some_and(|state| living::is_dead_or_dying(&entity.base, state))
        });
        if dying {
            self.dragon_tick_death(id);
            return;
        }
        self.dragon_ai_step(id);
    }

    pub(super) fn dragon_body(&self, id: EntityId, runtime: &DragonRuntime) -> Option<DragonBody> {
        let mut entity = self.entities.store.get(id)?.clone();
        let base = &entity.base;
        let (position, velocity, yaw, silent) =
            (base.position, base.velocity, base.yaw, base.is_silent());
        let state = *state_of(&mut entity)?;
        Some(DragonBody {
            position,
            velocity,
            yaw,
            y_rot_a: state.y_rot_a,
            head: runtime.parts[Part::Head.index()],
            silent,
            origin: state.fight_origin,
        })
    }

    pub(super) fn dragon_store_body(&mut self, id: EntityId, body: &DragonBody) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        entity.base.position = body.position;
        entity.base.velocity = body.velocity;
        entity.base.set_yaw(body.yaw);
        let yaw = entity.base.yaw;
        if let Some(state) = state_of(entity) {
            state.y_rot_a = body.y_rot_a;
        }
        if let Some(living) = entity.living_mut() {
            living.body_yaw = yaw;
        }
    }

    pub(super) fn dragon_alive_crystals(&self, id: EntityId) -> Option<i32> {
        let uuid = self.entities.store.get(id)?.base.uuid;
        self.entities
            .fight
            .as_ref()
            .filter(|fight| fight.data.dragon_uuid == Some(uuid))
            .map(|fight| fight.alive_crystals)
    }

    fn dragon_ai_step(&mut self, id: EntityId) {
        self.dragon_check_crystals(id);
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let moving = entity.base.velocity;
        let no_ai = living::is_no_ai(&entity.base);
        let sitting = self
            .entities
            .dragons
            .get(&id)
            .is_some_and(|runtime| runtime.phases.is_sitting());
        let in_wall = state_of(entity).is_some_and(|state| state.in_wall);
        let flap_speed = 0.2 / ((moving.horizontal_distance_sqr().sqrt() as f32) * 10.0 + 1.0)
            * 2.0_f64.powf(moving.y) as f32;
        let yaw = mth::wrap_degrees(entity.base.yaw);
        entity.base.set_yaw(yaw);
        if let Some(state) = state_of(entity) {
            state.old_flap_time = state.flap_time;
            state.flap_time = if no_ai {
                0.5
            } else if sitting {
                state.flap_time + 0.1
            } else if in_wall {
                state.flap_time + flap_speed * 0.5
            } else {
                state.flap_time + flap_speed
            };
        }
        if no_ai {
            return;
        }
        let y = entity.base.position.y;
        if let Some(runtime) = self.entities.dragons.get_mut(&id) {
            runtime.history.record(y, yaw);
        }
        let before = self.dragon_phase(id);
        self.dragon_phase_tick(id);
        if self.dragon_phase(id) != before {
            self.dragon_phase_tick(id);
        }
        self.dragon_fly(id);
        self.dragon_place_parts(id);
        self.dragon_strike_nearby(id);
        self.dragon_break_walls(id);
        self.dragon_update_fight(id);
    }

    pub(super) fn dragon_phase(&self, id: EntityId) -> Option<DragonPhase> {
        self.entities
            .dragons
            .get(&id)
            .map(|runtime| runtime.phases.current())
    }

    pub(super) fn dragon_eye(body: &DragonBody, scale: f64) -> Vec3 {
        Vec3::new(
            body.position.x,
            body.position.y + EYE_HEIGHT * scale,
            body.position.z,
        )
    }

    pub(super) fn dragon_scale(&mut self, id: EntityId) -> f64 {
        self.entities
            .store
            .get_mut(id)
            .and_then(Entity::living_mut)
            .map_or(1.0, |state| state.attribute(DAMAGE_SCALE))
    }

    fn dragon_phase_tick(&mut self, id: EntityId) {
        let Some(mut runtime) = self.entities.dragons.remove(&id) else {
            return;
        };
        if runtime.pending_phase {
            self.entities.dragons.insert(id, runtime);
            return;
        }
        let Some(mut body) = self.dragon_body(id, &runtime) else {
            self.entities.dragons.insert(id, runtime);
            return;
        };
        let crystals = self.dragon_alive_crystals(id);
        let scale = self.dragon_scale(id);
        let mut output = PhaseOutput::default();
        {
            let mut world = DragonWorld {
                eye: Self::dragon_eye(&body, scale),
                position: body.position,
                server: self,
            };
            runtime.phases.tick(
                &mut body,
                &mut runtime.nodes,
                crystals,
                &mut world,
                &mut output,
            );
        }
        self.dragon_store_body(id, &body);
        self.entities.dragons.insert(id, runtime);
        self.dragon_apply_output(id, output);
    }

    fn dragon_fly(&mut self, id: EntityId) {
        let Some(runtime) = self.entities.dragons.get(&id) else {
            return;
        };
        let phase = runtime.phases.current();
        let Some(target) = runtime.phases.fly_target() else {
            return;
        };
        if phase == DragonPhase::Hovering {
            return;
        }
        let Some(mut body) = self.dragon_body(id, runtime) else {
            return;
        };
        let in_wall = self
            .entities
            .store
            .get_mut(id)
            .and_then(state_of)
            .is_some_and(|state| state.in_wall);
        fly(&mut body, phase, target, in_wall);
        self.dragon_store_body(id, &body);
    }

    fn dragon_place_parts(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let position = entity.base.position;
        let yaw = entity.base.yaw;
        let Some(y_rot_a) = state_of(entity).map(|state| state.y_rot_a) else {
            return;
        };
        let Some(runtime) = self.entities.dragons.get_mut(&id) else {
            return;
        };
        let offsets = parts::offsets(&runtime.history, yaw, y_rot_a, runtime.phases.is_sitting());
        runtime.old_parts = runtime.parts;
        for (slot, offset) in offsets.into_iter().enumerate() {
            runtime.parts[slot] = position + offset;
        }
    }

    pub(crate) fn dragon_part_box(&self, id: EntityId, part: Part) -> Option<Aabb> {
        let runtime = self.entities.dragons.get(&id)?;
        Some(parts::part_box(part, runtime.parts[part.index()]))
    }

    fn dragon_check_crystals(&mut self, id: EntityId) {
        let Some(tick) = self
            .entities
            .store
            .get(id)
            .map(|entity| entity.base.tick_count)
        else {
            return;
        };
        let nearest = self
            .entities
            .dragons
            .get(&id)
            .and_then(|runtime| runtime.nearest_crystal);
        if let Some(crystal) = nearest {
            let gone = self
                .entities
                .store
                .get(crystal)
                .is_none_or(|entity| entity.base.is_removed());
            if gone {
                self.dragon_set_nearest_crystal(id, None);
            } else if tick % ender_dragon::CRYSTAL_HEAL_INTERVAL == 0 {
                let hurt = self.entities.store.get_mut(id).is_some_and(|entity| {
                    let health = living::health(&entity.base);
                    let max = entity
                        .living_mut()
                        .map_or(0.0, |state| state.max_health() as f32);
                    health < max
                });
                if hurt {
                    self.heal_entity(
                        id,
                        ender_dragon::CRYSTAL_HEAL_AMOUNT,
                        EntityRegainHealthRegainReason::EnderCrystal,
                    );
                }
            }
        }
        if self.rng_next_int(ender_dragon::CRYSTAL_SCAN_ODDS) == 0 {
            let Some(area) = self.entities.store.get(id).map(|entity| {
                entity
                    .base
                    .bounding_box()
                    .inflate(ender_dragon::CRYSTAL_SCAN_RADIUS)
            }) else {
                return;
            };
            let from = self
                .entities
                .store
                .get(id)
                .map(|entity| entity.base.position);
            let closest = self
                .entities
                .store
                .iter()
                .filter(|other| other.base.kind == Kind::EndCrystal && !other.base.is_removed())
                .filter(|other| other.base.bounding_box().intersects(&area))
                .min_by(|a, b| {
                    let from = from.unwrap_or_default();
                    a.base
                        .position
                        .distance_squared(from)
                        .total_cmp(&b.base.position.distance_squared(from))
                })
                .map(|other| other.base.id);
            self.dragon_set_nearest_crystal(id, closest);
        }
    }

    fn dragon_set_nearest_crystal(&mut self, id: EntityId, crystal: Option<EntityId>) {
        if let Some(runtime) = self.entities.dragons.get_mut(&id) {
            runtime.nearest_crystal = crystal;
        }
    }

    fn rng_next_int(&mut self, bound: i32) -> i32 {
        use rspigot_domain::random::RandomSource;
        crate::mobs::ServerRandom(&mut self.rng).next_int(bound)
    }

    fn dragon_targets(&self, id: EntityId, area: &Aabb) -> Vec<EntityId> {
        let mut found: Vec<EntityId> = self
            .players
            .values()
            .filter(|player| !player.vitals.is_dead())
            .filter(|player| {
                !matches!(
                    player.game_mode,
                    rspigot_domain::player::GameMode::Creative
                        | rspigot_domain::player::GameMode::Spectator
                )
            })
            .filter(|player| {
                Aabb::of_size(player.position, 0.6, player.height() as f32).intersects(area)
            })
            .map(|player| player.entity_id)
            .collect();
        found.extend(
            self.entities
                .store
                .iter()
                .filter(|other| other.base.id != id && !other.base.is_removed())
                .filter(|other| other.base.bounding_box().intersects(area))
                .filter(|other| {
                    other
                        .living()
                        .is_some_and(|state| !living::is_dead_or_dying(&other.base, state))
                })
                .map(|other| other.base.id),
        );
        found
    }

    fn dragon_push(&mut self, target: EntityId, push: Vec3) {
        if let Some(player) = self
            .players
            .values_mut()
            .find(|player| player.entity_id == target)
        {
            player.vitals.velocity = player.vitals.velocity + push;
            player.vitals.sync_velocity = true;
        } else if let Some(entity) = self.entities.store.get_mut(target) {
            entity.base.velocity = entity.base.velocity + push;
            entity.base.needs_sync = true;
        }
    }

    fn dragon_hurt_victim(&mut self, dragon: EntityId, victim: EntityId, amount: f32) {
        let Some(source) = self.mob_source(dragon) else {
            return;
        };
        self.hurt_entity(
            victim,
            &DamageSource::by(DamageType::MobAttack, source),
            amount,
        );
    }

    fn dragon_victim_recently_hurt(&self, victim: EntityId) -> bool {
        if let Some(player) = self
            .players
            .values()
            .find(|player| player.entity_id == victim)
        {
            return player.vitals.tick_count as i32 - player.vitals.last_hurt_by_mob_at
                < ender_dragon::HURT_RECENTLY_TICKS;
        }
        self.entities.store.get(victim).is_some_and(|entity| {
            entity.living().is_some_and(|state| {
                state.damage.last_hurt_by_mob_at
                    >= entity.base.tick_count - ender_dragon::HURT_RECENTLY_TICKS
            })
        })
    }

    fn dragon_strike_nearby(&mut self, id: EntityId) {
        let recently = self
            .entities
            .store
            .get_mut(id)
            .and_then(Entity::living_mut)
            .is_some_and(|state| state.hurt_time > 0);
        if recently {
            return;
        }
        let Some(runtime) = self.entities.dragons.get(&id) else {
            return;
        };
        let sitting = runtime.phases.is_sitting();
        let body = parts::part_box(Part::Body, runtime.parts[Part::Body.index()]);
        let center_x = (body.min_x + body.max_x) / 2.0;
        let center_z = (body.min_z + body.max_z) / 2.0;
        let wings: Vec<Aabb> = [Part::Wing1, Part::Wing2]
            .into_iter()
            .map(|part| parts::part_box(part, runtime.parts[part.index()]))
            .map(|area| {
                area.inflate_by(4.0, 2.0, 4.0)
                    .move_by(Vec3::new(0.0, -2.0, 0.0))
            })
            .collect();
        let strikers: Vec<Aabb> = [Part::Head, Part::Neck]
            .into_iter()
            .map(|part| parts::part_box(part, runtime.parts[part.index()]).inflate(1.0))
            .collect();
        for area in wings {
            for victim in self.dragon_targets(id, &area) {
                self.dragon_knock_back(id, victim, center_x, center_z, sitting);
            }
        }
        for area in strikers {
            for victim in self.dragon_targets(id, &area) {
                self.dragon_hurt_victim(id, victim, ender_dragon::HEAD_DAMAGE);
            }
        }
    }

    fn dragon_knock_back(
        &mut self,
        dragon: EntityId,
        victim: EntityId,
        center_x: f64,
        center_z: f64,
        sitting: bool,
    ) {
        let position = self
            .players
            .values()
            .find(|player| player.entity_id == victim)
            .map(|player| player.position)
            .or_else(|| {
                self.entities
                    .store
                    .get(victim)
                    .map(|entity| entity.base.position)
            });
        let Some(position) = position else {
            return;
        };
        let dx = position.x - center_x;
        let dz = position.z - center_z;
        let distance = (dx * dx + dz * dz).max(ender_dragon::PUSH_MIN_DISTANCE);
        self.dragon_push(
            victim,
            Vec3::new(
                dx / distance * ender_dragon::WING_KNOCKBACK,
                ender_dragon::PUSH_UP,
                dz / distance * ender_dragon::WING_KNOCKBACK,
            ),
        );
        if !sitting && !self.dragon_victim_recently_hurt(victim) {
            self.dragon_hurt_victim(dragon, victim, ender_dragon::WING_DAMAGE);
        }
    }
}
