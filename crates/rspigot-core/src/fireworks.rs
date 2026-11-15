use std::num::NonZeroU8;

use rand::Rng;
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::firework::{self, FireworkState};
use rspigot_domain::entity::physics::{self, MoveTraits};
use rspigot_domain::entity::{Entity, EntityState, RemovalReason, arrow, raycast, thrown};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_items::ItemKind;

use crate::events::{ClientEvent, Direction, EntityStatus, Hand};
use crate::health::DamageSource;
use crate::plugins::events::{CreatureSpawnReason, EntityRemoveCause, FireworkExplode};
use crate::plugins::steps::Birth;
use crate::plugins::{Continuation, EventPayload};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use crate::thrown::Strike;

fn single(stack: &ItemStack) -> ItemStack {
    ItemStack {
        count: NonZeroU8::MIN,
        ..stack.clone()
    }
}

fn is_rocket(stack: &ItemStack) -> bool {
    stack.item == ItemKind::FireworkRocket.id()
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn use_firework(&mut self, id: PlayerId, hand: Hand) -> bool {
        let Some(stack) = self.held(id, hand).filter(is_rocket) else {
            return false;
        };
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        if !player.fall_flying {
            return false;
        }
        let (owner, position, target) = (player.profile.id.0, player.position, player.entity_id);
        let mut base = self.new_entity_base(Kind::FireworkRocket);
        base.position = position;
        base.old_position = position;
        let seed = self.rng.random::<i64>();
        let mut state = FireworkState::new(&mut base, seed, &single(&stack));
        state.owner = Some(owner);
        firework::set_attached(&mut base, &mut state, Some(target));
        let rocket = Entity::new(base, EntityState::Firework(Box::new(state)));
        self.spawn_entity(rocket, Birth::of(CreatureSpawnReason::Default).by(id));
        self.consume_held(id, hand, 1);
        self.player_swing(id, hand, false);
        true
    }

    pub(crate) fn use_firework_on(
        &mut self,
        id: PlayerId,
        hand: Hand,
        pos: BlockPos,
        face: Direction,
        cursor: Vec3,
    ) -> bool {
        let Some(stack) = self.held(id, hand).filter(is_rocket) else {
            return false;
        };
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        if player.fall_flying {
            return false;
        }
        let owner = player.profile.id.0;
        let (dx, dy, dz) = face.offset();
        let at = Vec3::new(
            f64::from(pos.x) + cursor.x + f64::from(dx) * firework::FACE_OFFSET,
            f64::from(pos.y) + cursor.y + f64::from(dy) * firework::FACE_OFFSET,
            f64::from(pos.z) + cursor.z + f64::from(dz) * firework::FACE_OFFSET,
        );
        let mut base = self.new_entity_base(Kind::FireworkRocket);
        base.position = at;
        base.old_position = at;
        let seed = self.rng.random::<i64>();
        let mut state = FireworkState::new(&mut base, seed, &single(&stack));
        state.owner = Some(owner);
        let rocket = Entity::new(base, EntityState::Firework(Box::new(state)));
        self.spawn_entity(rocket, Birth::of(CreatureSpawnReason::Default).by(id));
        self.consume_held(id, hand, 1);
        true
    }

    fn rocket_waits(&self, id: EntityId) -> bool {
        self.decisions.anywhere(|then| match then {
            Continuation::Burst(waiting) => *waiting == id,
            Continuation::ProjectileHit { arrow, .. } => *arrow == id,
            _ => false,
        })
    }

    pub(crate) fn tick_firework(&mut self, id: EntityId) {
        if self.rocket_waits(id) {
            return;
        }
        self.rocket_move(id);
        self.rocket_strike(id);
        let Some(Entity {
            base,
            state: EntityState::Firework(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        thrown::update_rotation(base);
        let first = state.life == 0;
        state.life += 1;
        let expired = state.lifetime < state.life;
        let position = base.position;
        let silent = base.is_silent();
        if first && !silent {
            self.play_sound(
                None,
                position,
                firework::LAUNCH_SOUND,
                SoundSource::Ambient,
                firework::LAUNCH_VOLUME,
                1.0,
            );
        }
        if expired {
            self.ask_burst(id);
        }
    }

    fn rocket_move(&mut self, id: EntityId) {
        let attached = match self.entities.store.get(id) {
            Some(Entity {
                state: EntityState::Firework(state),
                ..
            }) => state.attached,
            _ => return,
        };
        let target = attached.and_then(|target| self.rocket_carrier(target));
        let chunks = &self.world.chunks;
        let Some(Entity {
            base,
            state: EntityState::Firework(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        if attached.is_some() {
            match target {
                Some((position, velocity)) => {
                    base.position = position;
                    base.velocity = velocity;
                }
                None => firework::set_attached(base, state, None),
            }
            base.needs_sync = true;
            return;
        }
        if !firework::shot_at_angle(base) {
            let factor = if base.horizontal_collision {
                0.0
            } else {
                firework::ACCELERATION
            };
            base.velocity =
                base.velocity.multiply(factor, 1.0, factor) + Vec3::new(0.0, firework::LIFT, 0.0);
        }
        let movement = base.velocity;
        physics::move_entity(base, chunks, movement, &MoveTraits::plain(0.0));
        physics::apply_inside_blocks(base, chunks, false);
        base.velocity = movement;
        base.needs_sync = true;
    }

    fn rocket_carrier(&self, target: EntityId) -> Option<(Vec3, Vec3)> {
        if let Some(player) = self.players.values().find(|p| p.entity_id == target) {
            return Some((player.position, player.known_movement));
        }
        let entity = self.entities.store.get(target)?;
        (!entity.base.is_removed()).then_some((entity.base.position, entity.base.velocity))
    }

    fn rocket_strike(&mut self, id: EntityId) {
        let Some(Entity {
            base,
            state: EntityState::Firework(state),
            ..
        }) = self.entities.store.get(id)
        else {
            return;
        };
        let from = base.position;
        let to = from + base.velocity;
        let margin = f64::from(arrow::hit_margin(base.tick_count));
        let area = arrow::search_box(base);
        let explosive = firework::explosion_count(&firework::item(base)) > 0;
        let owner = self.owner_entity(state.owner);
        let left_owner = state.left_owner
            || owner
                .and_then(|owner| self.entity_box(owner))
                .is_none_or(|(_, _, bounds)| !bounds.intersects(&area));
        let (strike, _) = self.find_strike(id, state.owner, left_owner, from, to, margin, &area);
        if let Some(Entity {
            state: EntityState::Firework(state),
            ..
        }) = self.entities.store.get_mut(id)
        {
            state.left_owner = left_owner;
        }
        match strike {
            Strike::Miss => {}
            Strike::Entity(target) => self.ask_projectile_hit(id, target),
            Strike::Block(hit) => {
                self.notice_block_hit(id, hit);
                if explosive {
                    self.ask_burst(id);
                }
            }
        }
    }

    pub(crate) fn rocket_hit_entity(&mut self, id: EntityId) {
        self.ask_burst(id);
    }

    fn ask_burst(&mut self, id: EntityId) {
        let lane = self.lane_for(None, id);
        self.fire(lane, Continuation::Burst(id), move |server| {
            let entity = server.entity_view(id)?;
            Some(EventPayload::FireworkExplode(FireworkExplode {
                entity_type: entity.kind.clone(),
                entity,
            }))
        });
    }

    pub(crate) fn finish_burst(&mut self, id: EntityId, cancelled: bool) {
        if cancelled || self.entities.store.get(id).is_none() {
            return;
        }
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityStatus {
                entity: id,
                status: EntityStatus::Event(firework::EXPLODE_STATUS),
            },
        );
        self.rocket_blast(id);
        self.remove_entity(
            id,
            RemovalReason::Discarded,
            Some(EntityRemoveCause::Explode),
        );
    }

    fn rocket_blast(&mut self, id: EntityId) {
        let Some(Entity {
            base,
            state: EntityState::Firework(state),
            ..
        }) = self.entities.store.get(id)
        else {
            return;
        };
        let count = firework::explosion_count(&firework::item(base));
        if count == 0 {
            return;
        }
        let damage = firework::blast_damage(count);
        let (position, attached, owner) = (base.position, state.attached, state.owner);
        let causing = self
            .owner_entity(owner)
            .and_then(|owner| self.source_for(owner));
        let Some(direct) = self.mob_source(id) else {
            return;
        };
        let victims: Vec<(EntityId, Vec3)> = self
            .blast_candidates(position)
            .into_iter()
            .filter(|(target, _)| Some(*target) != attached)
            .collect();
        let make_source = |target_position: Vec3| DamageSource {
            kind: DamageType::Fireworks,
            causing: causing.clone().or_else(|| Some(direct.clone())),
            direct: Some(direct.clone()),
            position: None,
            knockback: Some((
                target_position.x - position.x,
                target_position.z - position.z,
            )),
            direct_block: None,
        };
        if let Some(target) = attached
            && let Some((target_position, _, _)) = self.entity_box(target)
        {
            self.hurt_entity(target, &make_source(target_position), damage);
        }
        for (target, target_position) in victims {
            let reaches = [0.0, 0.5].into_iter().any(|lift| {
                let point = Vec3::new(
                    target_position.x,
                    target_position.y + lift,
                    target_position.z,
                );
                raycast::clip_blocks(&self.world.chunks, position, point).is_none()
            });
            if !reaches {
                continue;
            }
            let distance = position.distance_squared(target_position).sqrt();
            let hurt = firework::damage_at(damage, distance);
            self.hurt_entity(target, &make_source(target_position), hurt);
        }
    }

    fn blast_candidates(&self, position: Vec3) -> Vec<(EntityId, Vec3)> {
        let mut found: Vec<(EntityId, Vec3)> = self
            .entities
            .store
            .iter()
            .filter(|entity| entity.living().is_some() && !entity.base.is_removed())
            .map(|entity| (entity.base.id, entity.base.position))
            .collect();
        found.extend(
            self.players
                .values()
                .filter(|player| !player.vitals.is_dead())
                .map(|player| (player.entity_id, player.position)),
        );
        found
            .into_iter()
            .filter(|(_, at)| position.distance_squared(*at) <= firework::BLAST_RADIUS_SQR)
            .collect()
    }
}
