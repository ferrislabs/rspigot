use rspigot_ai::{BrainMessage, MobView};
use rspigot_domain::EntityId;
use rspigot_domain::Vec3;
use rspigot_domain::attribute::{ATTACK_DAMAGE, ATTACK_KNOCKBACK, KNOCKBACK_RESISTANCE};
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::Entity;
use rspigot_domain::entity::living;
use rspigot_domain::entity::species::Species;
use rspigot_domain::entity::species::hoglin::{
    self, ATTACK_ANIMATION_TICKS, CONVERSION_TICKS, HoglinState,
};
use rspigot_domain::mth;
use rspigot_domain::random::RandomSource;
use rspigot_entities::EntityKind as Kind;

use crate::commands::Target;
use crate::health::{DamageSource, attributes};
use crate::mobs::ServerRandom;
use crate::plugins::steps::{AfterHurt, MobAttackFollow, Struck};
use crate::ports::ClientLink;
use crate::server::Server;

const EVENT_ATTACK: i8 = 4;
const HOGLIN_ATTACK_SOUND: &str = "minecraft:entity.hoglin.attack";
const ZOGLIN_ATTACK_SOUND: &str = "minecraft:entity.zoglin.attack";
const CONVERTED_SOUND: &str = "minecraft:entity.hoglin.converted_to_zombified";
const THROW_ROTATION_SPAN: i32 = 21;
const THROW_ROTATION_OFFSET: i32 = 10;
const THROW_STRENGTH_SCALE: f32 = 0.5;
const THROW_STRENGTH_BASE: f32 = 0.2;
const THROW_LIFT_SCALE: f64 = 0.5;

fn hoglin_state(entity: &mut Entity) -> Option<&mut HoglinState> {
    match entity
        .living_mut()
        .and_then(|state| state.mob.as_mut())
        .map(|mob| &mut mob.species)
    {
        Some(Species::Hoglin(state)) => Some(state),
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn nether_mob_converting(&self, entity: &Entity, immune: bool) -> bool {
        !immune && !living::is_no_ai(&entity.base) && self.world.dimension.piglins_zombify
    }

    pub(crate) fn hoglin_view(&self, entity: &Entity, view: &mut MobView) {
        let immune = hoglin::is_immune_to_zombification(&entity.base);
        view.species.hoglin.converting =
            entity.base.kind == Kind::Hoglin && self.nether_mob_converting(entity, immune);
    }

    pub(crate) fn hoglin_post_tick(&mut self, id: EntityId) {
        if let Some(state) = self.entities.store.get_mut(id).and_then(hoglin_state)
            && state.attack_animation_remaining > 0
        {
            state.attack_animation_remaining -= 1;
        }
    }

    pub(crate) fn hoglin_custom_ai_step(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let kind = entity.base.kind;
        let baby = entity.is_baby();
        if kind == Kind::Hoglin {
            let immune = hoglin::is_immune_to_zombification(&entity.base);
            let converting = self.nether_mob_converting(entity, immune);
            let mut convert = false;
            if let Some(living) = self.entities.store.get_mut(id).and_then(Entity::living_mut) {
                hoglin::age_boundary_reached(&mut living.attributes, baby);
            }
            if let Some(state) = self.entities.store.get_mut(id).and_then(hoglin_state) {
                if converting {
                    state.time_in_overworld += 1;
                    convert = state.time_in_overworld > CONVERSION_TICKS;
                } else {
                    state.time_in_overworld = 0;
                }
            }
            if convert {
                self.play_mob_voice(id, |_| Some(CONVERTED_SOUND));
                self.convert_mob(id, Kind::Zoglin, None);
            }
        }
    }

    pub(crate) fn hoglin_hurt_target(&mut self, attacker: EntityId, target: Target) -> bool {
        let target_entity = match target {
            Target::Player(player) => self.players.get(&player).map(|p| p.entity_id),
            Target::Entity(entity) => Some(entity),
        };
        let living = match target {
            Target::Player(_) => true,
            Target::Entity(entity) => self
                .entities
                .store
                .get(entity)
                .is_some_and(|e| e.living().is_some()),
        };
        let Some(target_entity) = target_entity.filter(|_| living) else {
            return false;
        };
        let Some(source_entity) = self.mob_source(attacker) else {
            return false;
        };
        let kind = source_entity.kind;
        if let Some(state) = self.entities.store.get_mut(attacker).and_then(hoglin_state) {
            state.attack_animation_remaining = ATTACK_ANIMATION_TICKS;
        }
        self.mob_entity_event(attacker, EVENT_ATTACK);
        let sound = if kind == Kind::Hoglin {
            HOGLIN_ATTACK_SOUND
        } else {
            ZOGLIN_ATTACK_SOUND
        };
        self.play_mob_voice(attacker, move |_| Some(sound));
        if kind == Kind::Hoglin {
            self.brain_message(
                attacker,
                BrainMessage::HoglinHitTarget {
                    target: target_entity,
                },
            );
        }
        let baby = self
            .entities
            .store
            .get(attacker)
            .is_some_and(Entity::is_baby);
        let Some(attack) = self
            .entities
            .store
            .get_mut(attacker)
            .and_then(Entity::living_mut)
            .map(|state| state.attribute(ATTACK_DAMAGE) as f32)
        else {
            return false;
        };
        let rounded = attack.round() as i32;
        let damage = if !baby && rounded > 0 {
            attack / 2.0 + ServerRandom(&mut self.rng).next_int(rounded) as f32
        } else {
            attack
        };
        let source = DamageSource::by(DamageType::MobAttack, source_entity);
        let victim = match target {
            Target::Player(player) => Struck::Player(player),
            Target::Entity(entity) => Struck::Mob(entity),
        };
        let follow = MobAttackFollow {
            attacker,
            target,
            knockback: 0.0,
            yaw: 0.0,
            golem: false,
            hoglin: true,
        };
        self.strike(None, victim, &source, damage, AfterHurt::MobAttack(follow))
    }

    pub(crate) fn hoglin_finish_attack(&mut self, attacker: EntityId, target: Target, hurt: bool) {
        let baby = self
            .entities
            .store
            .get(attacker)
            .is_some_and(Entity::is_baby);
        if hurt && !baby {
            self.hoglin_throw_target(attacker, target);
        }
    }

    fn hoglin_throw_target(&mut self, body: EntityId, target: Target) {
        let Some(origin) = self.entities.store.get(body).map(|e| e.base.position) else {
            return;
        };
        let Some(strength) = self
            .entities
            .store
            .get_mut(body)
            .and_then(Entity::living_mut)
            .map(|state| state.attribute(ATTACK_KNOCKBACK))
        else {
            return;
        };
        let (position, resistance) = match target {
            Target::Player(player) => match self.players.get(&player) {
                Some(player) => (
                    player.position,
                    attributes(player).get(KNOCKBACK_RESISTANCE),
                ),
                None => return,
            },
            Target::Entity(entity) => match self.entities.store.get_mut(entity) {
                Some(found) => (
                    found.base.position,
                    found
                        .living_mut()
                        .map_or(0.0, |state| state.attribute(KNOCKBACK_RESISTANCE)),
                ),
                None => return,
            },
        };
        let knockback = strength - resistance;
        if knockback <= 0.0 {
            return;
        }
        let mut random = ServerRandom(&mut self.rng);
        let rotation = (random.next_int(THROW_ROTATION_SPAN) - THROW_ROTATION_OFFSET) as f32;
        let power =
            knockback * f64::from(random.next_float() * THROW_STRENGTH_SCALE + THROW_STRENGTH_BASE);
        let lift = knockback * f64::from(random.next_float()) * THROW_LIFT_SCALE;
        let direction = Vec3::new(position.x - origin.x, 0.0, position.z - origin.z)
            .normalize()
            .scale(power);
        let (sin, cos) = (
            f64::from(mth::sin(f64::from(rotation))),
            f64::from(mth::cos(f64::from(rotation))),
        );
        let push = Vec3::new(
            direction.x * cos + direction.z * sin,
            lift,
            direction.z * cos - direction.x * sin,
        );
        match target {
            Target::Player(player) => {
                if let Some(player) = self.players.get_mut(&player) {
                    player.vitals.velocity = player.vitals.velocity + push;
                    player.vitals.sync_velocity = true;
                }
            }
            Target::Entity(entity) => {
                if let Some(entity) = self.entities.store.get_mut(entity) {
                    entity.base.velocity = entity.base.velocity + push;
                    entity.base.needs_sync = true;
                }
            }
        }
    }

    pub(crate) fn hoglin_after_hurt(&mut self, id: EntityId, source: &DamageSource, dead: bool) {
        if dead {
            return;
        }
        let Some(attacker) = source
            .causing
            .as_ref()
            .filter(|cause| cause.kind.living())
            .map(|cause| cause.entity)
        else {
            return;
        };
        let kind = self.kind_of(id);
        let message = if kind == Some(Kind::Hoglin) {
            BrainMessage::HoglinHurtBy { attacker }
        } else {
            BrainMessage::ZoglinHurtBy { attacker }
        };
        self.brain_message(id, message);
    }
}
