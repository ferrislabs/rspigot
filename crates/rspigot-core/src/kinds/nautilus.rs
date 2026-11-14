use rspigot_ai::{BrainMessage, MobAi, NautilusAction};
use rspigot_domain::EntityId;
use rspigot_domain::effect::EffectId;
use rspigot_domain::entity::living;
use rspigot_domain::entity::species::Species;
use rspigot_domain::entity::species::nautilus;
use rspigot_domain::entity::tamable;
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::random::LegacyRandom;
use rspigot_domain::random::RandomSource;
use rspigot_entities::EntityKind as Kind;
use rspigot_items::EquipmentSlot;

use crate::commands::Target;
use crate::events::Hand;
use crate::health::DamageSource;
use crate::interact::{Interaction, item_tag};
use crate::ports::ClientLink;
use crate::server::Server;

const TAME_FAILURE_EVENT: i8 = 6;
const EAT_SOUND: &str = "minecraft:entity.nautilus.eat";
const ZOMBIE_EAT_SOUND: &str = "minecraft:entity.zombie_nautilus.eat";
const POISON: &str = "minecraft:poison";
const SPEED: &str = "minecraft:speed";
const SLOWNESS: &str = "minecraft:slowness";
const MOVEMENT_SPEED: &str = "minecraft:movement_speed";
const MIN_KNOCKBACK: f32 = 0.2;
const MAX_KNOCKBACK: f32 = 2.0;
const EFFECT_STEP: f32 = 0.25;

fn amplifier_of(state: &living::LivingState, name: &str) -> i32 {
    EffectId::from_name(name)
        .and_then(|id| state.effects.get(id))
        .map_or(0, |instance| i32::from(instance.amplifier) + 1)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn nautilus_registered(&mut self, id: EntityId, ai: &mut MobAi) {
        let pending = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| match &mut entity.state {
                EntityState::Living(living) => {
                    match living.mob.as_mut().map(|mob| &mut mob.species) {
                        Some(Species::Nautilus(state)) => state.pending_attack_cooldown.take(),
                        _ => None,
                    }
                }
                _ => None,
            });
        if let Some(cooldown) = pending {
            ai.init_nautilus_memories(cooldown);
        }
    }

    pub(crate) fn nautilus_action(&mut self, id: EntityId, action: NautilusAction) {
        match action {
            NautilusAction::ChargeHit {
                target,
                speed,
                knockback,
                velocity,
            } => self.nautilus_charge_hit(id, target, speed, knockback, velocity),
        }
    }

    fn nautilus_charge_hit(
        &mut self,
        id: EntityId,
        target: EntityId,
        speed: f32,
        force: f32,
        velocity: rspigot_domain::Vec3,
    ) {
        let Some(state) = self.entities.store.get_mut(id).and_then(Entity::living_mut) else {
            return;
        };
        let boost =
            (amplifier_of(state, SPEED) - amplifier_of(state, SLOWNESS)) as f32 * EFFECT_STEP;
        let attribute = state.attribute(MOVEMENT_SPEED) as f32;
        let strength = (speed * attribute).clamp(MIN_KNOCKBACK, MAX_KNOCKBACK) + boost;
        let yaw = velocity.x.atan2(-velocity.z).to_degrees() as f32;
        let victim = match self.players.values().find(|p| p.entity_id == target) {
            Some(player) => Target::Player(player.id),
            None => Target::Entity(target),
        };
        self.hurt_target_with(id, victim, Some((strength * force, yaw)));
    }

    pub(crate) fn nautilus_after_hurt(&mut self, id: EntityId, source: &DamageSource) {
        let Some(attacker) = source.causing.as_ref().map(|cause| cause.entity) else {
            return;
        };
        let uuid = self
            .entities
            .store
            .get(attacker)
            .filter(|entity| entity.living().is_some())
            .map(|entity| entity.base.uuid)
            .or_else(|| {
                self.players
                    .values()
                    .find(|player| player.entity_id == attacker)
                    .map(|player| player.profile.id.0)
            });
        let Some(uuid) = uuid else {
            return;
        };
        self.brain_message(id, BrainMessage::NautilusAnger { attacker, uuid });
    }

    pub(crate) fn nautilus_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        let entity = self.entities.store.get_mut(target)?;
        if let Some(mob) = entity.living_mut().and_then(|living| living.mob.as_mut()) {
            mob.persistence_required = true;
        }
        let baby = entity.is_baby();
        let tame = tamable::is_tame(&entity.base);
        let kind = entity.base.kind;
        if baby || tame {
            return None;
        }
        let stack = held?;
        if !item_tag(stack.item, nautilus::TAMING_ITEMS_TAG) {
            return None;
        }
        self.use_animal_item(player, hand, kind, stack);
        let mut random = self.mob_random(target);
        let tamed = random.next_int(3) == 0;
        self.store_mob_random(target, random);
        if tamed {
            self.ask_tame(player, target);
        } else {
            self.mob_entity_event(target, TAME_FAILURE_EVENT);
        }
        let sound = if kind == Kind::ZombieNautilus {
            ZOMBIE_EAT_SOUND
        } else {
            EAT_SOUND
        };
        self.entity_sound(target, sound, 1.0, 1.0);
        Some(Interaction::Predicted)
    }

    pub(crate) fn nautilus_refuses_effect(&self, id: EntityId, effect: EffectId) -> bool {
        matches!(
            self.kind_of(id),
            Some(Kind::Nautilus | Kind::ZombieNautilus)
        ) && EffectId::from_name(POISON) == Some(effect)
    }

    pub(crate) fn nautilus_custom_ai_step(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let leashed = entity.extra.contains_key(crate::spawning::LEASH);
        let tame = tamable::is_tame(&entity.base);
        let position = entity.base.position.block();
        let baby = entity.is_baby();
        let Some(mob) = entity.living_mut().and_then(|living| {
            let saddled = living.equipment(EquipmentSlot::Saddle).is_some();
            living.mob.as_mut().map(|mob| (mob, saddled))
        }) else {
            return;
        };
        let (mob, saddled) = mob;
        if leashed || !tame {
            return;
        }
        let radius = nautilus::restriction_radius(baby, saddled);
        let near = mob.home_radius != -1 && {
            let reach = f64::from(radius + 8);
            let (dx, dy, dz) = (
                f64::from(mob.home_pos.x - position.x),
                f64::from(mob.home_pos.y - position.y),
                f64::from(mob.home_pos.z - position.z),
            );
            dx * dx + dy * dy + dz * dz < reach * reach
        };
        if near && mob.home_radius == radius {
            return;
        }
        mob.home_pos = position;
        mob.home_radius = radius;
    }

    pub(crate) fn nautilus_offspring(
        &mut self,
        child: &mut Entity,
        parent: EntityId,
        _random: &mut LegacyRandom,
    ) {
        let owner = self
            .entities
            .store
            .get(parent)
            .and_then(|entity| rspigot_domain::entity::tamable::owner(&entity.base));
        if let Some(owner) = owner {
            rspigot_domain::entity::tamable::tame(&mut child.base, owner);
        }
    }
}
