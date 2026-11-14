use rspigot_domain::attribute::{ATTACK_DAMAGE, ATTACK_KNOCKBACK, ATTACK_SPEED};
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::physics::has_tag;
use rspigot_domain::entity::{Aabb, EntityState};
use rspigot_domain::mth;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::SoundSource;

use crate::commands::Target;
use crate::events::{Animation, ClientEvent, Hand};
use crate::health::{DamageSource, attributes};
use crate::plugins::events::{EntityExhaustionReason, EntityKnockbackCause};
use crate::plugins::{AfterHurt, AttackFollow, Holder, Struck, SweepFollow};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

pub const DEFAULT_KNOCKBACK: f64 = crate::mobs::DEFAULT_KNOCKBACK;
const SPRINT_KNOCKBACK: f32 = 0.5;
const ATTACK_PARTIAL_TICK: f32 = 0.5;
const FULL_STRENGTH: f32 = 0.9;
const CRITICAL_MULTIPLIER: f32 = 1.5;
const ATTACK_EXHAUSTION: f32 = 0.1;
const ATTACK_RANGE_BUFFER: f64 = 3.0;
const SURVIVAL_REACH: f64 = 3.0;
const CREATIVE_REACH: f64 = 5.0;
const PLAYER_WIDTH: f32 = 0.6;
const DEGREES_PER_RADIAN: f64 = 57.295_776_367_187_5;
const ATTACKER_SLOWDOWN: f64 = 0.6;
const PUSH_LIFT: f64 = 0.1;
const SWEEP_SPEED_FACTOR: f64 = 2.5;
const SWEEP_INFLATE_XZ: f64 = 1.0;
const SWEEP_INFLATE_Y: f64 = 0.25;
const SWEEP_RANGE_SQR: f64 = 9.0;
const SWEEP_KNOCKBACK: f64 = 0.400_000_005_960_464_5;
const MOVEMENT_SPEED: &str = "minecraft:movement_speed";
const SWEEPING_DAMAGE_RATIO: &str = "minecraft:sweeping_damage_ratio";
const SWORDS: &str = "minecraft:swords";
const SOUND_KNOCKBACK: &str = "minecraft:entity.player.attack.knockback";
const SOUND_CRIT: &str = "minecraft:entity.player.attack.crit";
const SOUND_STRONG: &str = "minecraft:entity.player.attack.strong";
const SOUND_WEAK: &str = "minecraft:entity.player.attack.weak";
const SOUND_SWEEP: &str = "minecraft:entity.player.attack.sweep";
const SOUND_NO_DAMAGE: &str = "minecraft:entity.player.attack.nodamage";

pub fn attack_strength_delay(attack_speed: f64) -> f32 {
    (1.0 / attack_speed * 20.0) as f32
}

pub fn attack_strength_scale(ticker: u32, attack_speed: f64, partial: f32) -> f32 {
    ((ticker as f32 + partial) / attack_strength_delay(attack_speed)).clamp(0.0, 1.0)
}

pub fn base_damage_scale(scale: f32) -> f32 {
    0.2 + scale * scale * 0.8
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Strike {
    pub damage: f32,
    pub critical: bool,
    pub sprint_knockback: bool,
}

pub fn strike(base: f32, scale: f32, sprinting: bool, can_critical: bool) -> Option<Strike> {
    let damage = base * base_damage_scale(scale);
    if damage <= 0.0 {
        return None;
    }
    let full = scale > FULL_STRENGTH;
    let critical = full && can_critical && !sprinting;
    Some(Strike {
        damage: if critical {
            damage * CRITICAL_MULTIPLIER
        } else {
            damage
        },
        critical,
        sprint_knockback: sprinting && full,
    })
}

fn distance_to_box_squared(point: Vec3, bounds: &Aabb) -> f64 {
    let axis = |value: f64, min: f64, max: f64| (min - value).max(0.0).max(value - max);
    let dx = axis(point.x, bounds.min_x, bounds.max_x);
    let dy = axis(point.y, bounds.min_y, bounds.max_y);
    let dz = axis(point.z, bounds.min_z, bounds.max_z);
    dx * dx + dy * dy + dz * dz
}

fn is_sword(item: rspigot_domain::ItemId) -> bool {
    rspigot_data::tags::tag("minecraft:item", SWORDS)
        .is_some_and(|ids| ids.binary_search(&u32::from(item.0)).is_ok())
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Victim {
    pub(crate) target: Target,
    pub(crate) entity: EntityId,
    pub(crate) bounds: Aabb,
    pub(crate) living: bool,
    pub(crate) attackable: bool,
    pub(crate) velocity: Vec3,
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn indicate_damage(&mut self, id: PlayerId, dx: f64, dz: f64) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let yaw = (dz.atan2(dx) * DEGREES_PER_RADIAN - f64::from(player.rotation.yaw)) as f32;
        player.vitals.hurt_dir = yaw;
        let entity = player.entity_id;
        self.push(
            Audience::One(id),
            ClientEvent::HurtAnimation { entity, yaw },
        );
    }

    fn in_water(&self, position: Vec3) -> bool {
        self.world
            .chunks
            .block(position.block())
            .is_some_and(|state| state.flags().contains(rspigot_blocks::StateFlags::WATER))
    }

    pub(crate) fn feet_in_water(&self, id: PlayerId) -> bool {
        self.players
            .get(&id)
            .is_some_and(|p| self.in_water(p.position))
    }

    fn victim(&self, target: EntityId) -> Option<Victim> {
        if let Some(player) = self
            .players
            .values()
            .find(|p| p.entity_id == target && !p.vitals.removed)
        {
            return Some(Victim {
                target: Target::Player(player.id),
                entity: target,
                bounds: Aabb::of_size(player.position, PLAYER_WIDTH, player.height() as f32),
                living: true,
                attackable: true,
                velocity: player.vitals.velocity,
            });
        }
        if let Some((dragon, part)) = self.dragon_part_of(target) {
            let entity = self.entities.store.get(dragon)?;
            return Some(Victim {
                target: Target::Entity(dragon),
                entity: dragon,
                bounds: self.dragon_part_box(dragon, part)?,
                living: true,
                attackable: true,
                velocity: entity.base.velocity,
            });
        }
        let entity = self.entities.store.get(target)?;
        Some(Victim {
            target: Target::Entity(target),
            entity: target,
            bounds: entity.base.bounding_box(),
            living: entity.living().is_some(),
            attackable: !matches!(
                entity.state,
                EntityState::Item(_) | EntityState::Falling(_) | EntityState::Orb(_)
            ),
            velocity: entity.base.velocity,
        })
    }

    pub(crate) fn handle_attack(&mut self, id: PlayerId, target: EntityId) {
        let Some(attacker) = self.players.get(&id) else {
            return;
        };
        if attacker.vitals.is_dead()
            || !attacker.vitals.client_loaded()
            || attacker.game_mode == GameMode::Spectator
        {
            return;
        }
        let Some(victim) = self.victim(target) else {
            return;
        };
        let reach = if attacker.game_mode == GameMode::Creative {
            CREATIVE_REACH
        } else {
            SURVIVAL_REACH
        };
        let distance = distance_to_box_squared(attacker.eye_position(), &victim.bounds).sqrt();
        if distance < -ATTACK_RANGE_BUFFER || distance > reach + ATTACK_RANGE_BUFFER {
            return;
        }
        let invalid = target == attacker.entity_id
            || self.entities.store.get(target).is_some_and(|e| {
                matches!(
                    e.base.kind,
                    rspigot_entities::EntityKind::Item
                        | rspigot_entities::EntityKind::ExperienceOrb
                )
            });
        if invalid {
            tracing::warn!(
                "Player {} tried to attack an invalid entity",
                attacker.name()
            );
            self.disconnect_player(
                id,
                Component::translatable(
                    "multiplayer.disconnect.invalid_entity_attacked",
                    Vec::new(),
                ),
            );
            return;
        }
        if let Some((dragon, part)) = self.dragon_part_of(target) {
            self.dragon_note_part(dragon, part);
        }
        self.attack(id, victim);
    }

    fn is_deflectable(&self, entity: EntityId) -> bool {
        self.entities.store.get(entity).is_some_and(|entity| {
            matches!(
                entity.base.kind,
                rspigot_entities::EntityKind::Fireball
                    | rspigot_entities::EntityKind::ShulkerBullet
            )
        })
    }

    fn struck_target(target: Target) -> Struck {
        match target {
            Target::Player(player) => Struck::Player(player),
            Target::Entity(entity) => Struck::Mob(entity),
        }
    }

    pub(crate) fn knockback_target(
        &mut self,
        target: Target,
        strength: f64,
        direction: (f64, f64),
        cause: EntityKnockbackCause,
        source: EntityId,
    ) {
        let holder = match target {
            Target::Player(player) => Holder::Player(player),
            Target::Entity(entity) => Holder::Mob(entity),
        };
        self.knock(holder, strength, direction, cause, Some(source));
    }

    fn attacker_sound(&mut self, id: PlayerId, sound: &'static str) {
        if let Some(position) = self.players.get(&id).map(|p| p.position) {
            self.play_sound(None, position, sound, SoundSource::Players, 1.0, 1.0);
        }
    }

    fn attack(&mut self, id: PlayerId, victim: Victim) {
        if !victim.attackable {
            return;
        }
        let in_water = self.feet_in_water(id);
        let climbing = self.players.get(&id).is_some_and(|p| {
            self.world
                .chunks
                .block(p.position.block())
                .is_some_and(|state| has_tag(state, "minecraft:climbable"))
        });
        let Some(source) = self.source_of(id) else {
            return;
        };
        let Some(attacker) = self.players.get(&id) else {
            return;
        };
        let stats = attributes(attacker);
        let scale = attack_strength_scale(
            attacker.vitals.attack_ticker,
            stats.get(ATTACK_SPEED),
            ATTACK_PARTIAL_TICK,
        );
        let can_critical = attacker.vitals.fall_distance > 0.0
            && !attacker.on_ground
            && !climbing
            && !in_water
            && !attacker.effects.has("minecraft:blindness")
            && victim.living;
        let Some(strike) = strike(
            stats.get(ATTACK_DAMAGE) as f32,
            scale,
            attacker.sprinting,
            can_critical,
        ) else {
            return;
        };
        let full = scale > FULL_STRENGTH;
        let yaw = attacker.rotation.yaw;
        let attacker_entity = attacker.entity_id;
        let speed = stats.get(MOVEMENT_SPEED) as f32;
        let sweep_limit = f64::from(speed) * SWEEP_SPEED_FACTOR;
        let held = attacker.inventory.held().map(|s| s.item);
        let sweep = full
            && !strike.critical
            && !strike.sprint_knockback
            && attacker.on_ground
            && attacker.known_movement.horizontal_distance_sqr() < sweep_limit * sweep_limit
            && held.is_some_and(is_sword);
        let sweep_ratio = stats.get(SWEEPING_DAMAGE_RATIO) as f32;
        let knockback = stats.get(ATTACK_KNOCKBACK) as f32 / 2.0
            + if strike.sprint_knockback {
                SPRINT_KNOCKBACK
            } else {
                0.0
            };
        if strike.sprint_knockback {
            self.attacker_sound(id, SOUND_KNOCKBACK);
        }
        let source = DamageSource::by(DamageType::PlayerAttack, source);
        let follow = AttackFollow {
            attacker: id,
            attacker_entity,
            victim,
            strike,
            full,
            knockback,
            yaw,
            sweep: sweep.then_some((1.0 + sweep_ratio * strike.damage) * scale),
        };
        let struck = match victim.target {
            Target::Entity(entity) if self.is_deflectable(entity) => Struck::Projectile(entity),
            target => Self::struck_target(target),
        };
        self.strike(
            Some(id),
            struck,
            &source,
            strike.damage,
            AfterHurt::Attack(Box::new(follow)),
        );
    }

    pub(crate) fn finish_attack(&mut self, follow: AttackFollow, hurt: bool) {
        let AttackFollow {
            attacker: id,
            attacker_entity,
            victim,
            strike,
            full,
            knockback,
            yaw,
            sweep,
        } = follow;
        if !hurt {
            self.attacker_sound(id, SOUND_NO_DAMAGE);
            self.send_inventory(id);
            return;
        }
        let (dx, dz) = mth::yaw_direction(yaw);
        if knockback > 0.0 {
            if victim.living {
                self.knockback_target(
                    victim.target,
                    f64::from(knockback),
                    (dx, dz),
                    EntityKnockbackCause::EntityAttack,
                    attacker_entity,
                );
            } else if let Some(entity) = self.entities.store.get_mut(victim.entity) {
                let push = Vec3::new(
                    -dx * f64::from(knockback),
                    PUSH_LIFT,
                    -dz * f64::from(knockback),
                );
                let base = &mut entity.base;
                base.velocity = Vec3::new(
                    base.velocity.x + push.x,
                    base.velocity.y + push.y,
                    base.velocity.z + push.z,
                );
                base.needs_sync = true;
            }
            if let Some(attacker) = self.players.get_mut(&id) {
                attacker.vitals.velocity =
                    attacker
                        .vitals
                        .velocity
                        .multiply(ATTACKER_SLOWDOWN, 1.0, ATTACKER_SLOWDOWN);
                if attacker.sprinting {
                    attacker.sprinting = false;
                    self.broadcast_metadata(id);
                }
            }
        }
        if let Target::Player(victim_id) = victim.target
            && self
                .players
                .get(&victim_id)
                .is_some_and(|target| target.vitals.sync_velocity)
        {
            self.knock_velocity(victim_id, victim.velocity);
        }
        if let Some(damage) = sweep
            && let Some(source) = self.source_of(id)
        {
            let source = DamageSource::by(DamageType::PlayerAttack, source);
            self.sweep_attack(id, victim, source, damage, (dx, dz));
        }
        if strike.critical {
            self.attacker_sound(id, SOUND_CRIT);
            let event = ClientEvent::EntityAnimation {
                entity: victim.entity,
                animation: Animation::CriticalHit,
            };
            self.push(Audience::One(id), event.clone());
            self.push(Audience::Trackers(attacker_entity), event);
        }
        if !strike.critical && sweep.is_none() {
            self.attacker_sound(id, if full { SOUND_STRONG } else { SOUND_WEAK });
        }
        if let Some(attacker) = self.players.get_mut(&id) {
            attacker.vitals.last_hurt_mob = Some(victim.entity);
            attacker.vitals.last_hurt_mob_at = attacker.vitals.tick_count as i32;
        }
        if victim.living {
            self.damage_weapon(id);
        }
        self.exhaust(id, ATTACK_EXHAUSTION, EntityExhaustionReason::Unknown);
    }

    fn sweep_attack(
        &mut self,
        id: PlayerId,
        victim: Victim,
        source: DamageSource,
        damage: f32,
        direction: (f64, f64),
    ) {
        self.attacker_sound(id, SOUND_SWEEP);
        let Some(attacker) = self.players.get(&id) else {
            return;
        };
        let origin = attacker.position;
        let attacker_entity = attacker.entity_id;
        let b = victim.bounds;
        let area = Aabb::new(
            b.min_x - SWEEP_INFLATE_XZ,
            b.min_y - SWEEP_INFLATE_Y,
            b.min_z - SWEEP_INFLATE_XZ,
            b.max_x + SWEEP_INFLATE_XZ,
            b.max_y + SWEEP_INFLATE_Y,
            b.max_z + SWEEP_INFLATE_XZ,
        );
        let mut targets: Vec<Target> = self
            .entities
            .store
            .iter()
            .filter(|e| e.living().is_some() && e.base.id != victim.entity)
            .filter(|e| e.base.bounding_box().intersects(&area))
            .filter(|e| e.base.position.distance_squared(origin) < SWEEP_RANGE_SQR)
            .map(|e| Target::Entity(e.base.id))
            .collect();
        targets.extend(
            self.players
                .values()
                .filter(|p| p.entity_id != attacker_entity && p.entity_id != victim.entity)
                .filter(|p| !p.vitals.removed)
                .filter(|p| {
                    Aabb::of_size(p.position, PLAYER_WIDTH, p.height() as f32).intersects(&area)
                })
                .filter(|p| p.position.distance_squared(origin) < SWEEP_RANGE_SQR)
                .map(|p| Target::Player(p.id)),
        );
        for target in targets {
            let follow = SweepFollow {
                attacker: id,
                target,
                direction,
            };
            self.strike(
                Some(id),
                Self::struck_target(target),
                &source,
                damage,
                AfterHurt::Sweep(follow),
            );
        }
    }

    pub(crate) fn finish_sweep(&mut self, follow: SweepFollow, hurt: bool) {
        let attacker = self.players.get(&follow.attacker).map(|p| p.entity_id);
        if hurt && let Some(attacker) = attacker {
            self.knockback_target(
                follow.target,
                SWEEP_KNOCKBACK,
                follow.direction,
                EntityKnockbackCause::SweepAttack,
                attacker,
            );
        }
    }

    fn damage_weapon(&mut self, id: PlayerId) {
        let amount = self
            .players
            .get(&id)
            .and_then(|player| player.inventory.held())
            .and_then(|stack| stack.item.kind())
            .and_then(|kind| kind.weapon_damage())
            .and_then(|amount| i32::try_from(amount).ok());
        if let Some(amount) = amount {
            self.hurt_and_break(id, Hand::Main, amount);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::knockback_velocity;

    #[test]
    fn fist_cooldown_is_five_ticks_and_sword_cooldown_twelve_and_a_half() {
        assert_eq!(attack_strength_delay(4.0), 5.0);
        assert!((attack_strength_delay(1.6) - 12.5).abs() < 1e-5);
        assert_eq!(attack_strength_scale(0, 4.0, 0.5), 0.1);
        assert_eq!(attack_strength_scale(5, 4.0, 0.5), 1.0);
        assert!((attack_strength_scale(6, 1.6, 0.5) - 0.52).abs() < 1e-5);
        assert_eq!(attack_strength_scale(100, 1.6, 0.5), 1.0);
    }

    #[test]
    fn spamming_scales_damage_down_to_a_fifth() {
        assert_eq!(base_damage_scale(1.0), 1.0);
        assert!((base_damage_scale(0.0) - 0.2).abs() < 1e-6);
        let weak = strike(7.0, 0.1, false, true).unwrap();
        assert!((weak.damage - 7.0 * (0.2 + 0.01 * 0.8)).abs() < 1e-5);
        assert!(!weak.critical);
    }

    #[test]
    fn critical_hits_need_full_strength_and_no_sprint() {
        let critical = strike(7.0, 1.0, false, true).unwrap();
        assert_eq!(critical.damage, 10.5);
        assert!(critical.critical);
        assert!(!critical.sprint_knockback);
        let sprinting = strike(7.0, 1.0, true, true).unwrap();
        assert_eq!(sprinting.damage, 7.0);
        assert!(!sprinting.critical);
        assert!(sprinting.sprint_knockback);
        let grounded = strike(1.0, 1.0, false, false).unwrap();
        assert_eq!(grounded.damage, 1.0);
        assert!(strike(0.0, 1.0, false, true).is_none());
    }

    #[test]
    fn knockback_pushes_away_and_lifts_grounded_targets() {
        let pushed = knockback_velocity(Vec3::ZERO, 0.4, 1.0, 0.0, true);
        assert_eq!(pushed, Vec3::new(-0.4, 0.4, 0.0));
        let airborne = knockback_velocity(Vec3::new(0.0, -0.3, 0.0), 0.4, 0.0, 2.0, false);
        assert_eq!(airborne, Vec3::new(0.0, -0.3, -0.4));
        let stacked = knockback_velocity(pushed, 0.5, 0.0, -1.0, true);
        assert_eq!(stacked, Vec3::new(-0.2, 0.4, 0.5));
    }

    #[test]
    fn reach_is_measured_to_the_target_box() {
        let target = Aabb::of_size(Vec3::new(0.0, 64.0, 0.0), 0.6, 1.8);
        assert_eq!(
            distance_to_box_squared(Vec3::new(0.0, 65.0, 0.0), &target),
            0.0
        );
        let far = distance_to_box_squared(Vec3::new(3.3, 65.0, 0.0), &target);
        assert!((far - 9.0).abs() < 1e-6);
    }

    #[test]
    fn only_swords_sweep() {
        assert!(is_sword(rspigot_items::ItemKind::DiamondSword.id()));
        assert!(!is_sword(rspigot_items::ItemKind::DiamondAxe.id()));
    }
}
