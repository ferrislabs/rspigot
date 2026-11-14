use rand::Rng;
use rspigot_domain::attribute::KNOCKBACK_RESISTANCE;
use rspigot_domain::player::GameMode;
use rspigot_domain::{EntityId, Vec3};

use crate::health::attributes;
use crate::mobs::{MIN_KNOCKBACK_DIRECTION, knockback_velocity};
use crate::plugins::decisions::Continuation;
use crate::plugins::events::{EntityKnockback, EntityKnockbackByEntity, EntityKnockbackCause};
use crate::plugins::steps::{Holder, KnockStep, Push};
use crate::plugins::{EventKind, EventPatch, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

const RANDOM_DIRECTION_SCALE: f64 = 0.01;

pub(crate) fn settled_knockback(patch: Option<EventPatch>) -> Option<Vec3> {
    match patch {
        Some(EventPatch::EntityKnockback(patch)) => patch.final_knockback,
        Some(EventPatch::EntityKnockbackByEntity(patch)) => patch.final_knockback,
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    fn knocked_motion(&mut self, target: Holder) -> Option<(f64, Vec3, bool)> {
        match target {
            Holder::Player(id) => {
                let player = self.players.get(&id)?;
                let resistance = attributes(player).get(KNOCKBACK_RESISTANCE);
                Some((resistance, player.vitals.velocity, player.on_ground))
            }
            Holder::Mob(id) => {
                if !self.creaking_can_move(id) || self.dragon_blocks_knockback(id) {
                    return None;
                }
                let entity = self.entities.store.get_mut(id)?;
                let velocity = entity.base.velocity;
                let on_ground = entity.base.on_ground;
                let resistance = entity.living_mut()?.attribute(KNOCKBACK_RESISTANCE);
                Some((resistance, velocity, on_ground))
            }
        }
    }

    fn holder_of(&self, target: Holder) -> Option<EntityId> {
        match target {
            Holder::Player(id) => self.players.get(&id).map(|player| player.entity_id),
            Holder::Mob(id) => Some(id),
        }
    }

    pub(crate) fn knock(
        &mut self,
        target: Holder,
        strength: f64,
        direction: (f64, f64),
        cause: EntityKnockbackCause,
        source: Option<EntityId>,
    ) {
        let Some((resistance, velocity, on_ground)) = self.knocked_motion(target) else {
            return;
        };
        let strength = strength * (1.0 - resistance);
        let (mut dx, mut dz) = direction;
        while dx * dx + dz * dz < MIN_KNOCKBACK_DIRECTION {
            dx = (self.rng.random::<f64>() - self.rng.random::<f64>()) * RANDOM_DIRECTION_SCALE;
            dz = (self.rng.random::<f64>() - self.rng.random::<f64>()) * RANDOM_DIRECTION_SCALE;
        }
        let raw = Vec3::new(dx, 0.0, dz).normalize().scale(-strength);
        let proposed = knockback_velocity(velocity, strength, dx, dz, on_ground);
        let push = Push::Knock {
            strength,
            direction: (dx, dz),
        };
        self.fire_knock(target, push, cause, source, strength, raw, proposed);
    }

    pub(crate) fn blast_push(&mut self, target: Holder, delta: Vec3, source: Option<EntityId>) {
        let velocity = match target {
            Holder::Player(id) => self.players.get(&id).map(|player| player.vitals.velocity),
            Holder::Mob(id) => self
                .entities
                .store
                .get(id)
                .map(|entity| entity.base.velocity),
        };
        let Some(velocity) = velocity else {
            return;
        };
        let push = Push::Blast(delta);
        self.fire_knock(
            target,
            push,
            EntityKnockbackCause::Explosion,
            source,
            delta.length(),
            delta,
            velocity + delta,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn fire_knock(
        &mut self,
        target: Holder,
        push: Push,
        cause: EntityKnockbackCause,
        source: Option<EntityId>,
        force: f64,
        knockback: Vec3,
        proposed: Vec3,
    ) {
        let Some(entity) = self.holder_of(target) else {
            return;
        };
        let kind = if source.is_some() {
            EventKind::EntityKnockbackByEntity
        } else {
            EventKind::EntityKnockback
        };
        let lane = self.lane_for(None, entity);
        let step = KnockStep { kind, target, push };
        self.fire(lane, Continuation::Knock(step), move |server| {
            let entity = server.subject_view(entity)?;
            let entity_type = entity.kind.clone();
            Some(match source {
                Some(source) => EventPayload::EntityKnockbackByEntity(EntityKnockbackByEntity {
                    cause,
                    entity,
                    entity_type,
                    final_knockback: proposed,
                    force,
                    knockback,
                    source_entity: server.subject_view(source)?,
                }),
                None => EventPayload::EntityKnockback(EntityKnockback {
                    cause,
                    entity,
                    entity_type,
                    final_knockback: proposed,
                    force,
                    knockback,
                }),
            })
        });
    }

    pub(super) fn finish_knock(&mut self, step: KnockStep, cancelled: bool, settled: Option<Vec3>) {
        let KnockStep { target, push, .. } = step;
        if cancelled {
            if let (Push::Blast(_), Holder::Player(id)) = (push, target) {
                self.blast_packet(id, Vec3::ZERO);
            }
            return;
        }
        match target {
            Holder::Player(id) => {
                let Some(player) = self.players.get_mut(&id) else {
                    return;
                };
                if player.vitals.is_dead() {
                    return;
                }
                let current = player.vitals.velocity;
                let next = match (settled, push) {
                    (Some(velocity), _) => velocity,
                    (
                        None,
                        Push::Knock {
                            strength,
                            direction,
                        },
                    ) => knockback_velocity(
                        current,
                        strength,
                        direction.0,
                        direction.1,
                        player.on_ground,
                    ),
                    (None, Push::Blast(delta)) => current + delta,
                };
                player.vitals.velocity = next;
                match push {
                    Push::Knock { .. } => player.vitals.sync_velocity = true,
                    Push::Blast(_) => self.blast_packet(id, next - current),
                }
            }
            Holder::Mob(id) => {
                let Some(entity) = self.entities.store.get_mut(id) else {
                    return;
                };
                let base = &mut entity.base;
                base.velocity = match (settled, push) {
                    (Some(velocity), _) => velocity,
                    (
                        None,
                        Push::Knock {
                            strength,
                            direction,
                        },
                    ) => knockback_velocity(
                        base.velocity,
                        strength,
                        direction.0,
                        direction.1,
                        base.on_ground,
                    ),
                    (None, Push::Blast(delta)) => base.velocity + delta,
                };
                base.needs_sync = true;
            }
        }
    }

    fn blast_packet(&mut self, id: rspigot_domain::player::PlayerId, delta: Vec3) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let flying_creative = player.game_mode == GameMode::Creative && player.flying;
        if player.game_mode == GameMode::Spectator || flying_creative {
            return;
        }
        match &mut self.impacts.knocked {
            Some(knocked) => knocked.push((id, delta)),
            None => player.vitals.sync_velocity = true,
        }
    }
}
