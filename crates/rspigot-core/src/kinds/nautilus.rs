use rspigot_ai::{BrainMessage, MobAi, NautilusAction};
use rspigot_domain::EntityId;
use rspigot_domain::effect::EffectId;
use rspigot_domain::entity::living;
use rspigot_domain::entity::species::Species;
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::random::LegacyRandom;

use crate::commands::Target;
use crate::health::DamageSource;
use crate::ports::ClientLink;
use crate::server::Server;

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
