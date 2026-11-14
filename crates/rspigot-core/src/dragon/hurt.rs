use rspigot_domain::dragon::Part;
use rspigot_domain::dragon::parts;
use rspigot_domain::dragon::phase::DragonPhase;
use rspigot_domain::entity::species::ender_dragon;
use rspigot_domain::entity::{Aabb, living};
use rspigot_domain::{EntityId, Vec3};

use super::ai::state_of;
use crate::health::DamageSource;
use crate::ports::ClientLink;
use crate::server::Server;

const ALWAYS_HURTS: &str = "minecraft:always_hurts_ender_dragons";
const IGNITE_SECONDS: f32 = 1.0;

impl<L: ClientLink> Server<L> {
    pub(crate) fn dragon_part_of(&self, id: EntityId) -> Option<(EntityId, Part)> {
        if self.entities.dragons.is_empty() {
            return None;
        }
        self.entities.dragons.keys().find_map(|dragon| {
            let offset = id.0.checked_sub(dragon.0)?;
            let index = usize::try_from(offset.checked_sub(1)?).ok()?;
            Part::from_index(index).map(|part| (*dragon, part))
        })
    }

    pub(crate) fn dragon_note_part(&mut self, dragon: EntityId, part: Part) {
        if let Some(runtime) = self.entities.dragons.get_mut(&dragon) {
            runtime.hit_part = Some(part);
        }
    }

    pub(crate) fn dragon_part_candidates(&self, area: &Aabb) -> Vec<(EntityId, Aabb, Vec3)> {
        let mut out = Vec::new();
        for (dragon, runtime) in &self.entities.dragons {
            let alive = self.entities.store.get(*dragon).is_some_and(|entity| {
                !entity.base.is_removed()
                    && entity
                        .living()
                        .is_some_and(|state| !living::is_dead_or_dying(&entity.base, state))
            });
            if !alive {
                continue;
            }
            for part in Part::ALL {
                let position = runtime.parts[part.index()];
                let bounds = parts::part_box(part, position);
                if bounds.intersects(area) {
                    out.push((*dragon, bounds, position));
                }
            }
        }
        out
    }

    pub(crate) fn dragon_explosion_parts(
        &self,
        source: Option<EntityId>,
        area: &Aabb,
    ) -> Vec<crate::explosion::HitEntity> {
        self.dragon_part_candidates(area)
            .into_iter()
            .filter(|(dragon, _, _)| Some(*dragon) != source)
            .filter_map(|(dragon, bounds, position)| {
                let part = Part::ALL
                    .into_iter()
                    .find(|part| parts::part_box(*part, position) == bounds)?;
                Some(crate::explosion::HitEntity {
                    id: dragon,
                    player: None,
                    position,
                    eye: position,
                    bounds,
                    living: true,
                    part: Some(part),
                })
            })
            .collect()
    }

    fn dragon_closest_part(&self, dragon: EntityId, at: Vec3) -> Part {
        let Some(runtime) = self.entities.dragons.get(&dragon) else {
            return Part::Body;
        };
        Part::ALL
            .into_iter()
            .min_by(|a, b| {
                let reach = |part: &Part| {
                    parts::part_box(*part, runtime.parts[part.index()])
                        .inflate(0.3)
                        .distance_to_sqr_box(&Aabb::new(at.x, at.y, at.z, at.x, at.y, at.z))
                };
                reach(a).total_cmp(&reach(b))
            })
            .unwrap_or(Part::Body)
    }

    pub(crate) fn dragon_hurt_amount(
        &mut self,
        id: EntityId,
        source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        let phase = self.dragon_phase(id)?;
        if phase == DragonPhase::Dying {
            return None;
        }
        let known = self
            .entities
            .dragons
            .get_mut(&id)
            .and_then(|runtime| runtime.hit_part.take());
        let part = known.unwrap_or_else(|| {
            source
                .direct
                .as_ref()
                .filter(|direct| direct.player.is_none())
                .map_or(Part::Body, |direct| {
                    self.dragon_closest_part(id, direct.position)
                })
        });
        let mut damage = amount;
        if phase.sitting_attack_immune()
            && let Some(direct) = &source.direct
            && (direct.kind.extends("AbstractArrow")
                || rspigot_domain::entity::hurting::is_wind_charge(direct.kind))
        {
            self.ignite_projectile(direct.entity, IGNITE_SECONDS);
            damage = 0.0;
        }
        damage = parts::damage_through(part, damage);
        if damage < ender_dragon::MIN_HURT_DAMAGE {
            return None;
        }
        let by_player = source
            .causing
            .as_ref()
            .is_some_and(|causing| causing.kind == rspigot_entities::EntityKind::Player);
        if !by_player && !source.kind.in_tag(ALWAYS_HURTS) {
            return None;
        }
        let health = self
            .entities
            .store
            .get(id)
            .map_or(0.0, |entity| living::health(&entity.base));
        if let Some(runtime) = self.entities.dragons.get_mut(&id) {
            runtime.health_before = health;
        }
        Some(damage)
    }

    fn ignite_projectile(&mut self, id: EntityId, seconds: f32) {
        if let Some(entity) = self.entities.store.get_mut(id) {
            rspigot_domain::entity::fire::ignite_for_ticks(
                &mut entity.base,
                rspigot_domain::entity::fire::seconds_to_ticks(seconds),
            );
        }
    }

    pub(crate) fn dragon_after_hurt(&mut self, id: EntityId, dead: bool) {
        let _ = dead;
        let Some(runtime) = self.entities.dragons.get(&id) else {
            return;
        };
        let before = runtime.health_before;
        if !runtime.phases.is_sitting() {
            return;
        }
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let health = living::health(&entity.base);
        let max = entity
            .living_mut()
            .map_or(0.0, |state| state.max_health() as f32);
        let Some(state) = state_of(entity) else {
            return;
        };
        state.sitting_damage_received += before - health;
        if state.sitting_damage_received > ender_dragon::SITTING_DAMAGE_RATIO * max {
            state.sitting_damage_received = 0.0;
            self.dragon_request_phase(id, DragonPhase::Takeoff);
        }
    }

    pub(crate) fn dragon_handle_killing_blow(&mut self, id: EntityId) {
        let sitting = self
            .entities
            .dragons
            .get(&id)
            .is_none_or(|runtime| runtime.phases.is_sitting());
        if sitting {
            return;
        }
        self.dragon_set_health(id, 1.0);
        self.dragon_request_phase(id, DragonPhase::Dying);
    }

    pub(crate) fn dragon_blocks_knockback(&self, id: EntityId) -> bool {
        self.entities
            .dragons
            .get(&id)
            .is_some_and(|runtime| runtime.phases.is_sitting())
    }

    pub(crate) fn dragon_crystal_destroyed(
        &mut self,
        dragon: EntityId,
        crystal: EntityId,
        pos: rspigot_domain::BlockPos,
        source: &DamageSource,
    ) {
        let player = source
            .causing
            .as_ref()
            .and_then(|causing| causing.player)
            .or_else(|| self.nearest_attackable_player(pos, ender_dragon::CRYSTAL_DESTROY_RANGE));
        let nearest = self
            .entities
            .dragons
            .get(&dragon)
            .and_then(|runtime| runtime.nearest_crystal);
        if nearest == Some(crystal)
            && let Some(direct) = self.mob_source(crystal)
        {
            let causing = player.and_then(|player| self.source_of(player));
            let blast = DamageSource {
                kind: rspigot_domain::damage::DamageType::Explosion,
                causing,
                direct: Some(direct),
                position: None,
                knockback: None,
                direct_block: None,
            };
            self.dragon_note_part(dragon, Part::Head);
            self.hurt_entity(dragon, &blast, ender_dragon::CRYSTAL_DESTROY_DAMAGE);
        }
        if self.dragon_phase(dragon) == Some(DragonPhase::HoldingPattern)
            && let Some(player) = player
            && let Some(foe) = self.player_foe(player)
        {
            self.dragon_request_strafe(dragon, foe);
        }
    }

    fn nearest_attackable_player(
        &self,
        pos: rspigot_domain::BlockPos,
        range: f64,
    ) -> Option<rspigot_domain::player::PlayerId> {
        let at = Vec3::new(f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
        self.entities
            .mobs
            .players
            .iter()
            .filter(|player| player.can_be_seen_as_enemy())
            .filter(|player| player.position.distance_squared(at) <= range * range)
            .min_by(|a, b| {
                a.position
                    .distance_squared(at)
                    .total_cmp(&b.position.distance_squared(at))
            })
            .and_then(|living| {
                self.players
                    .values()
                    .find(|player| player.entity_id == living.id)
                    .map(|player| player.id)
            })
    }

    fn player_foe(
        &self,
        player: rspigot_domain::player::PlayerId,
    ) -> Option<rspigot_domain::dragon::phase::Foe> {
        let entity = self.players.get(&player)?.entity_id;
        let living = self
            .entities
            .mobs
            .players
            .iter()
            .find(|living| living.id == entity)?;
        living
            .can_be_seen_as_enemy()
            .then_some(rspigot_domain::dragon::phase::Foe {
                id: living.id,
                position: living.position,
                height: living.bounds.max_y - living.bounds.min_y,
            })
    }

    pub(crate) fn dragon_request_strafe(
        &mut self,
        dragon: EntityId,
        foe: rspigot_domain::dragon::phase::Foe,
    ) {
        self.dragon_change_with(
            dragon,
            rspigot_domain::dragon::phase::PhaseChange {
                to: DragonPhase::StrafePlayer,
                then: Some(rspigot_domain::dragon::phase::Followup::StrafeTarget(foe)),
            },
        );
    }
}
