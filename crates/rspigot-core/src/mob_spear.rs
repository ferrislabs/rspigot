use rspigot_domain::attribute::ATTACK_DAMAGE;
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::raycast::{clip_blocks, clip_box};
use rspigot_domain::spear::{self, Spear};
use rspigot_domain::{EntityId, Vec3, mth};
use rspigot_entities::SoundSource;

use crate::commands::Target;
use crate::events::{ClientEvent, Hand, SwingStyle};
use crate::health::DamageSource;
use crate::plugins::AfterHurt;
use crate::plugins::events::EntityKnockbackCause;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

fn dot(a: Vec3, b: Vec3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

pub(crate) fn mob_reach() -> (f64, f64) {
    (
        spear::MIN_REACH * spear::MOB_REACH_FACTOR,
        spear::MAX_REACH * spear::MOB_REACH_FACTOR,
    )
}

struct Candidate {
    entity: EntityId,
    distance: f64,
    velocity: Vec3,
}

impl<L: ClientLink> Server<L> {
    fn mob_spear_candidates(&self, id: EntityId, eye: Vec3, look: Vec3) -> Vec<Candidate> {
        let (min, max) = mob_reach();
        let mut end = eye + look.scale(max);
        if let Some(block) = clip_blocks(&self.world.chunks, eye, end) {
            end = block.location;
        }
        let start = eye + look.scale(min);
        let mut found: Vec<Candidate> = Vec::new();
        let mut consider =
            |entity: EntityId, velocity: Vec3, bounds: &rspigot_domain::entity::Aabb| {
                if let Some(hit) = clip_box(&bounds.inflate(spear::HITBOX_MARGIN), start, end) {
                    found.push(Candidate {
                        entity,
                        distance: hit.distance_squared(eye),
                        velocity,
                    });
                }
            };
        for player in self.players.values() {
            if player.vitals.is_dead()
                || player.game_mode == rspigot_domain::player::GameMode::Spectator
            {
                continue;
            }
            if let Some((_, _, bounds)) = self.entity_box(player.entity_id) {
                consider(player.entity_id, player.known_movement, &bounds);
            }
        }
        for entity in self.entities.store.iter() {
            if entity.base.id == id
                || entity.base.is_removed()
                || !self.living_entity_alive(entity.base.id)
            {
                continue;
            }
            consider(
                entity.base.id,
                entity.base.velocity,
                &entity.base.bounding_box(),
            );
        }
        found.sort_by(|a, b| a.distance.total_cmp(&b.distance));
        found
    }

    pub(crate) fn mob_spear_tick(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(living) = entity.living() else {
            return;
        };
        let Some((slot, ticks)) = living.using_item else {
            self.entities.mobs.stabbed.remove(&id);
            return;
        };
        let Some(weapon) = living
            .equipment(slot)
            .and_then(|stack| stack.item.kind().and_then(spear::of))
        else {
            return;
        };
        let position = entity.base.position;
        let look = mth::view_vector(entity.base.pitch, entity.base.yaw);
        let eye = position + Vec3::new(0.0, f64::from(entity.base.eye_height()), 0.0);
        let own_velocity = entity.base.velocity;
        let yaw = entity.base.yaw;
        let source = entity
            .base
            .kind
            .sounds()
            .map_or(SoundSource::Hostile, |sounds| sounds.source);
        let base_damage = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
            .map_or(0.0, |living| living.attribute(ATTACK_DAMAGE) as f32);
        if ticks == 1 {
            self.play_sound(None, position, weapon.use_sound(), source, 1.0, 1.0);
        }
        let Some(charged) = weapon.charging(ticks) else {
            return;
        };
        let tick = self.current_tick() as i64;
        let attacker_speed = dot(look, own_velocity.scale(spear::MOTION_TO_SPEED));
        let candidates = self.mob_spear_candidates(id, eye, look);
        let mut hit_any = false;
        for candidate in candidates {
            let seen = self.entities.mobs.stabbed.entry(id).or_default();
            seen.retain(|(_, at)| tick - *at < i64::from(spear::CONTACT_COOLDOWN_TICKS));
            if seen.iter().any(|(entity, _)| *entity == candidate.entity) {
                continue;
            }
            seen.push((candidate.entity, tick));
            let target_speed = dot(look, candidate.velocity.scale(spear::MOTION_TO_SPEED));
            let relative = spear::relative_speed(attacker_speed, target_speed);
            let contact = spear::contact(
                &weapon,
                charged,
                attacker_speed,
                relative,
                spear::MOB_FACTOR,
            );
            if !contact.any() {
                continue;
            }
            let bonus = if contact.deals_damage {
                weapon.bonus_damage(relative)
            } else {
                0.0
            };
            hit_any |= self.mob_stab(
                id,
                &weapon,
                candidate.entity,
                base_damage + bonus,
                contact,
                yaw,
            );
        }
        if hit_any {
            self.play_sound(None, position, weapon.hit_sound(), source, 1.0, 1.0);
            self.play_sound(None, position, weapon.attack_sound(), source, 1.0, 1.0);
            self.push(
                Audience::Trackers(id),
                ClientEvent::EntitySwing {
                    entity: id,
                    hand: Hand::Main,
                    style: SwingStyle::Stab,
                    duration: weapon.attack_ticks,
                },
            );
        }
    }

    fn mob_stab(
        &mut self,
        attacker: EntityId,
        _weapon: &Spear,
        target: EntityId,
        damage: f32,
        contact: spear::Contact,
        yaw: f32,
    ) -> bool {
        let Some(source_entity) = self.mob_source(attacker) else {
            return false;
        };
        let mut hit = false;
        if contact.deals_damage {
            let source = DamageSource::by(DamageType::Spear, source_entity);
            let struck = self.struck(target);
            hit = self.strike(None, struck, &source, damage, AfterHurt::Nothing);
        }
        if contact.knocks_back {
            let victim = self
                .player_of_entity(target)
                .map_or(Target::Entity(target), Target::Player);
            self.knockback_target(
                victim,
                f64::from(spear::STAB_KNOCKBACK),
                mth::yaw_direction(yaw),
                EntityKnockbackCause::EntityAttack,
                attacker,
            );
            hit = true;
        }
        if contact.dismounts && self.entities.rides.vehicle_of(target).is_some() {
            self.stop_riding(target);
            hit = true;
        }
        hit
    }
}
