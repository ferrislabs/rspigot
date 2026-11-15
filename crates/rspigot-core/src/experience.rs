use rand::Rng;
use rspigot_domain::entity::orb::{self, OrbState};
use rspigot_domain::entity::{Aabb, Entity, EntityState, RemovalReason};
use rspigot_domain::experience::PICKUP_DELAY;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};

use crate::events::ClientEvent;
use crate::plugins::events::{
    CreatureSpawnReason, EntityRemoveCause, PlayerExpChange, PlayerExpChangePatch,
    PlayerExpCooldownChange, PlayerExpCooldownChangePatch, PlayerExpCooldownChangeReason,
};
use crate::plugins::steps::{CooldownStep, OrbStep, Seeker};
use crate::plugins::{Continuation, EventPayload, Lane};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use rspigot_ai::{TargetReason, TargetRequest};

const BELOW_WORLD: i32 = 64;
const PICKUP_INFLATE_X: f64 = 1.0;
const PICKUP_INFLATE_Y: f64 = 0.5;
const PLAYER_WIDTH: f32 = 0.6;
const LEVEL_UP_SOUND: &str = "minecraft:entity.player.levelup";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbSnapshot {
    pub id: EntityId,
    pub position: Vec3,
    pub velocity: Vec3,
    pub value: i32,
    pub count: i32,
    pub age: i32,
}

fn pickup_box(position: Vec3, height: f64) -> Aabb {
    let half = f64::from(PLAYER_WIDTH / 2.0);
    Aabb::new(
        position.x - half - PICKUP_INFLATE_X,
        position.y - PICKUP_INFLATE_Y,
        position.z - half - PICKUP_INFLATE_X,
        position.x + half + PICKUP_INFLATE_X,
        position.y + height + PICKUP_INFLATE_Y,
        position.z + half + PICKUP_INFLATE_X,
    )
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn award_experience(&mut self, position: Vec3, amount: i32) {
        let mut amount = amount;
        while amount > 0 {
            let value = orb::experience_value(amount);
            amount -= value;
            if !self.merge_into_existing_orb(position, value) {
                self.spawn_orb(position, value);
            }
        }
    }

    fn merge_into_existing_orb(&mut self, position: Vec3, value: i32) -> bool {
        let half = orb::AWARD_MERGE_SIZE / 2.0;
        let area = Aabb::new(
            position.x - half,
            position.y - half,
            position.z - half,
            position.x + half,
            position.y + half,
            position.z + half,
        );
        let group = self.rng.random_range(0..orb::ORB_GROUPS_PER_AREA);
        let found = self.entities.store.iter().find(|e| {
            matches!(e.state, EntityState::Orb(_))
                && !e.base.is_removed()
                && e.base.bounding_box().intersects(&area)
                && orb::can_merge(e.base.id.0, orb::value(&e.base), group, value)
        });
        let Some(id) = found.map(|e| e.base.id) else {
            return false;
        };
        if let Some(Entity {
            state: EntityState::Orb(state),
            ..
        }) = self.entities.store.get_mut(id)
        {
            state.count += 1;
            state.age = 0;
        }
        true
    }

    pub(crate) fn spawn_orb(&mut self, position: Vec3, value: i32) {
        let mut base = self.new_entity_base(Kind::ExperienceOrb);
        base.position = position;
        base.old_position = position;
        let yaw = self.rng.random::<f32>() * 360.0;
        base.set_yaw(yaw);
        let rng = &mut self.rng;
        let mut next_double = || rng.random::<f64>();
        base.velocity = orb::spawn_velocity(&mut next_double);
        let state = OrbState::new(&mut base, value);
        self.add_entity(
            Entity::new(base, EntityState::Orb(state)),
            CreatureSpawnReason::Default,
        );
    }

    pub(crate) fn tick_orb(&mut self, id: EntityId) {
        let floor = f64::from(self.world.dimension.height.min_y - BELOW_WORLD);
        let below = match self.entities.store.get_mut(id) {
            Some(Entity {
                base,
                state: EntityState::Orb(_),
                ..
            }) => {
                base.old_position = base.position;
                base.tick_count = base.tick_count.wrapping_add(1);
                base.position.y < floor
            }
            _ => return,
        };
        if below {
            self.remove_entity(
                id,
                RemovalReason::Discarded,
                Some(EntityRemoveCause::OutOfWorld),
            );
            return;
        }
        let chunks = &self.world.chunks;
        let rng = &mut self.rng;
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let base = &mut entity.base;
        let mut next_float = || rng.random::<f32>();
        let stuck = orb::before_move(base, chunks, &mut next_float);
        let scan = base.tick_count % orb::SCAN_PERIOD == 1;
        if scan {
            self.scan_for_merges(id);
        }
        let following = self.follow_nearby_player(id);
        let chunks = &self.world.chunks;
        let rng = &mut self.rng;
        let Some(Entity {
            base,
            state: EntityState::Orb(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        let mut next_float = || rng.random::<f32>();
        let expired = orb::after_follow(base, state, chunks, following, stuck, &mut next_float);
        if expired {
            self.remove_entity(
                id,
                RemovalReason::Discarded,
                Some(EntityRemoveCause::Despawn),
            );
        }
    }

    fn scan_for_merges(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let area = entity.base.bounding_box().inflate(orb::MERGE_INFLATE);
        let value = orb::value(&entity.base);
        let others: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|other| {
                other.base.id != id
                    && matches!(other.state, EntityState::Orb(_))
                    && !other.base.is_removed()
                    && other.base.bounding_box().intersects(&area)
                    && orb::can_merge(other.base.id.0, orb::value(&other.base), id.0, value)
            })
            .map(|other| other.base.id)
            .collect();
        for other in others {
            let Some(Entity {
                state: EntityState::Orb(absorbed),
                ..
            }) = self.entities.store.get(other)
            else {
                continue;
            };
            let absorbed = *absorbed;
            if let Some(Entity {
                state: EntityState::Orb(state),
                ..
            }) = self.entities.store.get_mut(id)
            {
                state.count += absorbed.count;
                state.age = state.age.min(absorbed.age);
            }
            self.remove_entity(
                other,
                RemovalReason::Discarded,
                Some(EntityRemoveCause::Merge),
            );
        }
    }

    pub(crate) fn settle_orb(
        &mut self,
        orb: EntityId,
        from: Option<EntityId>,
        target: Option<EntityId>,
    ) {
        let player_of =
            |entity: Option<EntityId>| entity.and_then(|entity| self.player_of_entity(entity));
        let (from, target) = (player_of(from), player_of(target));
        let players = &self.players;
        if let Some(Entity {
            state: EntityState::Orb(state),
            ..
        }) = self.entities.store.get_mut(orb)
            && state
                .following
                .filter(|player| players.contains_key(player))
                == from
        {
            state.following = target;
        }
    }

    fn follow_nearby_player(&mut self, id: EntityId) -> bool {
        let Some(Entity {
            base,
            state: EntityState::Orb(state),
            ..
        }) = self.entities.store.get(id)
        else {
            return false;
        };
        let position = base.position;
        let keep = state
            .following
            .and_then(|player| self.players.get(&player))
            .is_some_and(|p| {
                p.game_mode != GameMode::Spectator
                    && orb::keeps_following(p.position.distance_squared(position))
            });
        let mut following = state.following.filter(|_| keep);
        if !keep {
            let range = orb::MAX_FOLLOW_DISTANCE * orb::MAX_FOLLOW_DISTANCE;
            following = self
                .players
                .values()
                .filter(|p| p.game_mode != GameMode::Spectator)
                .map(|p| {
                    (
                        p.id,
                        p.position.distance_squared(position),
                        p.vitals.is_dead(),
                    )
                })
                .filter(|(_, distance, _)| *distance < range)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .filter(|(_, _, dead)| !dead)
                .map(|(player, _, _)| player);
        }
        let before = state.following;
        if following != before {
            let deferred = self.defers_targets();
            let entity_of = |player: Option<PlayerId>| {
                player
                    .and_then(|player| self.players.get(&player))
                    .map(|p| p.entity_id)
            };
            let request = TargetRequest {
                from: entity_of(before),
                to: entity_of(following),
                reason: if following.is_some() {
                    TargetReason::ClosestPlayer
                } else {
                    TargetReason::ForgotTarget
                },
                tempt: false,
            };
            if let Some(orb) = self.entity_view(id) {
                self.target_event(orb, request, Seeker::Orb, deferred);
            }
            if deferred {
                return false;
            }
        }
        let target = following
            .and_then(|player| self.players.get(&player))
            .map(|p| (p.position, p.eye_height()));
        let Some(Entity {
            base,
            state: EntityState::Orb(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return false;
        };
        state.following = following;
        if let Some((player, eye)) = target {
            orb::pull_towards(base, player, eye);
        }
        following.is_some()
    }

    pub(crate) fn collect_orbs(&mut self) {
        let decisions = &self.decisions;
        let collectors: Vec<(PlayerId, EntityId, Vec3, f64)> = self
            .players
            .values_mut()
            .filter_map(|p| {
                let experience = &mut p.vitals.experience;
                if experience.take_delay > 0 {
                    experience.take_delay -= 1;
                }
                let collecting = decisions.in_flight(Lane::Player(p.id), |then| {
                    matches!(
                        then,
                        Continuation::ExpCooldown(_)
                            | Continuation::ExpChange(_)
                            | Continuation::Mend(_)
                    )
                });
                (p.game_mode != GameMode::Spectator
                    && p.pending_teleport.is_none()
                    && !p.vitals.is_dead()
                    && !collecting)
                    .then(|| (p.id, p.entity_id, p.position, p.height()))
            })
            .collect();
        for (player, collector, position, height) in collectors {
            let area = pickup_box(position, height);
            let touching: Vec<EntityId> = self
                .entities
                .store
                .iter()
                .filter(|e| {
                    matches!(e.state, EntityState::Orb(_))
                        && !e.base.is_removed()
                        && e.base.bounding_box().intersects(&area)
                })
                .map(|e| e.base.id)
                .collect();
            if touching.is_empty() {
                continue;
            }
            let pick = touching[self.rng.random_range(0..touching.len())];
            self.player_touch_orb(player, collector, pick);
        }
    }

    fn player_touch_orb(&mut self, player: PlayerId, collector: EntityId, id: EntityId) {
        let Some(Entity {
            base,
            state: EntityState::Orb(_),
            ..
        }) = self.entities.store.get(id)
        else {
            return;
        };
        let value = orb::value(base);
        if self
            .players
            .get(&player)
            .is_none_or(|target| target.vitals.experience.take_delay != 0)
        {
            return;
        }
        let cooldown = CooldownStep {
            player,
            value: PICKUP_DELAY,
        };
        self.fire(player, Continuation::ExpCooldown(cooldown), move |server| {
            Some(EventPayload::PlayerExpCooldownChange(
                PlayerExpCooldownChange {
                    new_cooldown: PICKUP_DELAY,
                    player: server.snapshot_of(player)?,
                    reason: PlayerExpCooldownChangeReason::PickupOrb,
                },
            ))
        });
        self.mend_with_orb(OrbStep {
            player,
            collector,
            orb: id,
            amount: value,
        });
    }

    pub(crate) fn ask_orb_experience(&mut self, step: OrbStep) {
        let (player, amount) = (step.player, step.amount);
        if amount <= 0 {
            self.finish_exp_change(step, PlayerExpChangePatch::default());
            return;
        }
        self.fire(player, Continuation::ExpChange(step), move |server| {
            Some(EventPayload::PlayerExpChange(PlayerExpChange {
                amount,
                player: server.snapshot_of(player)?,
            }))
        });
    }

    pub(crate) fn finish_cooldown(
        &mut self,
        step: CooldownStep,
        patch: PlayerExpCooldownChangePatch,
    ) {
        if let Some(target) = self.players.get_mut(&step.player) {
            target.vitals.experience.take_delay = patch.new_cooldown.unwrap_or(step.value);
        }
    }

    pub(crate) fn finish_exp_change(&mut self, step: OrbStep, patch: PlayerExpChangePatch) {
        let id = step.orb;
        let present = matches!(
            self.entities.store.get(id),
            Some(Entity {
                base,
                state: EntityState::Orb(state),
                ..
            }) if !base.is_removed() && state.count > 0
        );
        let Some(target) = self.players.get_mut(&step.player) else {
            return;
        };
        let able = target.game_mode != GameMode::Spectator
            && target.pending_teleport.is_none()
            && !target.vitals.is_dead();
        if !present || !able {
            return;
        }
        let tick_count = target.vitals.tick_count as i32;
        let position = target.position;
        let amount = patch.amount.unwrap_or(step.amount);
        let level_up = target.vitals.experience.give_points(amount, tick_count);
        self.push(
            Audience::Trackers(id),
            ClientEvent::ItemPickup {
                collected: id,
                collector: step.collector,
                amount: 1,
            },
        );
        if let Some(level_up) = level_up {
            self.play_sound(
                None,
                position,
                LEVEL_UP_SOUND,
                SoundSource::Players,
                level_up.volume,
                1.0,
            );
        }
        let emptied = match self.entities.store.get_mut(id) {
            Some(Entity {
                state: EntityState::Orb(state),
                ..
            }) => {
                state.count -= 1;
                state.count == 0
            }
            _ => false,
        };
        if emptied {
            self.remove_entity(
                id,
                RemovalReason::Discarded,
                Some(EntityRemoveCause::Pickup),
            );
        }
    }

    pub fn experience_orbs(&self) -> Vec<OrbSnapshot> {
        self.entities
            .store
            .iter()
            .filter_map(|e| match &e.state {
                EntityState::Orb(state) => Some(OrbSnapshot {
                    id: e.base.id,
                    position: e.base.position,
                    velocity: e.base.velocity,
                    value: orb::value(&e.base),
                    count: state.count,
                    age: state.age,
                }),
                _ => None,
            })
            .collect()
    }

    pub fn player_experience(&self, id: PlayerId) -> Option<(i32, f32, i32)> {
        self.players.get(&id).map(|p| {
            let xp = p.vitals.experience;
            (xp.level, xp.progress, xp.total)
        })
    }

    pub fn give_experience(&mut self, id: PlayerId, amount: i32) {
        if let Some(player) = self.players.get_mut(&id) {
            let tick_count = player.vitals.tick_count as i32;
            player.vitals.experience.give_points(amount, tick_count);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pickup_box_inflates_the_player_box() {
        let area = pickup_box(Vec3::new(0.0, 64.0, 0.0), 1.8);
        assert!((area.min_x + 1.3).abs() < 1e-6);
        assert!((area.max_y - 66.3).abs() < 1e-6);
        assert_eq!(area.min_y, 63.5);
    }
}
