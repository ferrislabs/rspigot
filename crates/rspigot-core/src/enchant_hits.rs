use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rspigot_data::enchantments::{EffectRole, EntityEffect};
use rspigot_domain::damage::DamageType;
use rspigot_domain::effect::{EffectDuration, EffectId, EffectInstance};
use rspigot_domain::{EntityId, player::PlayerId};
use rspigot_entities::EntityKind as Kind;

use crate::enchant_effects::{self, EquipSlot, Planned, Worn};
use crate::health::{CHEST_SLOT, DamageSource, FEET_SLOT, HEAD_SLOT, LEGS_SLOT, OFFHAND_SLOT};
use crate::loot::LootContext;
use crate::player::Player;
use crate::plugins::events::EntityPotionEffectCause;
use crate::ports::ClientLink;
use crate::server::Server;

const TICKS_PER_SECOND: f32 = 20.0;

pub(crate) fn worn_gear<L>(player: &Player<L>) -> Vec<Worn<'_>> {
    let inventory = &player.inventory;
    [
        (inventory.held(), EquipSlot::Mainhand),
        (inventory.get(OFFHAND_SLOT), EquipSlot::Offhand),
        (inventory.get(HEAD_SLOT), EquipSlot::Head),
        (inventory.get(CHEST_SLOT), EquipSlot::Chest),
        (inventory.get(LEGS_SLOT), EquipSlot::Legs),
        (inventory.get(FEET_SLOT), EquipSlot::Feet),
    ]
    .into_iter()
    .filter_map(|(stack, slot)| {
        Some(Worn {
            stack: stack?,
            slot,
        })
    })
    .collect()
}

fn between(rng: &mut impl Rng, min: f32, max: f32) -> f32 {
    rng.random::<f32>() * (max - min) + min
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn player_protection(&self, id: PlayerId, source: &DamageSource) -> f32 {
        let Some(player) = self.players.get(&id) else {
            return 0.0;
        };
        let this = crate::loot::LootEntity::of(Kind::Player);
        let damage = self.damage_facts(source);
        let context = LootContext {
            this: Some(&this),
            damage: Some(&damage),
            ..LootContext::default()
        };
        enchant_effects::damage_protection(
            &worn_gear(player),
            &context,
            &mut StdRng::seed_from_u64(0),
        )
    }

    pub(crate) fn run_post_attack(&mut self, source: &DamageSource, victim: EntityId) {
        if source.kind == DamageType::Thorns || source.causing.is_none() {
            return;
        }
        let Some(facts) = self.entity_loot_facts(victim) else {
            return;
        };
        let damage = self.damage_facts(source);
        let attacker = source.causing.as_ref().map(|c| c.entity);
        let direct = source.direct.as_ref().map(|d| d.entity);
        let planned = {
            let gear_of = |entity: Option<EntityId>| {
                entity
                    .and_then(|entity| self.players.values().find(|p| p.entity_id == entity))
                    .map(worn_gear)
                    .unwrap_or_default()
            };
            let attacker_gear = gear_of(attacker);
            let victim_gear = gear_of(Some(victim));
            if attacker_gear.is_empty() && victim_gear.is_empty() {
                return;
            }
            let context = LootContext {
                this: Some(&facts),
                damage: Some(&damage),
                ..LootContext::default()
            };
            enchant_effects::post_attack(&attacker_gear, &victim_gear, &context, &mut self.rng)
        };
        for plan in planned {
            self.apply_planned(&plan, victim, attacker, direct);
        }
    }

    fn entity_for(
        role: EffectRole,
        victim: EntityId,
        attacker: Option<EntityId>,
        direct: Option<EntityId>,
    ) -> Option<EntityId> {
        match role {
            EffectRole::Victim => Some(victim),
            EffectRole::Attacker => attacker,
            EffectRole::DamagingEntity => direct,
        }
    }

    fn apply_planned(
        &mut self,
        plan: &Planned,
        victim: EntityId,
        attacker: Option<EntityId>,
        direct: Option<EntityId>,
    ) {
        let target = Self::entity_for(plan.affected, victim, attacker, direct);
        let owner = Self::entity_for(plan.enchanted, victim, attacker, direct);
        if let (Some(target), Some(owner)) = (target, owner) {
            self.apply_entity_effect_plan(plan.effect, plan.level, target, owner);
        }
    }

    pub(crate) fn apply_entity_effect_plan(
        &mut self,
        effect: &EntityEffect,
        level: i32,
        target: EntityId,
        owner: EntityId,
    ) {
        match effect {
            EntityEffect::AllOf(effects) => {
                for effect in *effects {
                    self.apply_entity_effect_plan(effect, level, target, owner);
                }
            }
            EntityEffect::Ignite(seconds) => {
                let seconds = seconds.calculate(level);
                match self.player_of(target) {
                    Some(player) => self.ignite_player(player, seconds),
                    None => self.ignite_entity(target, seconds),
                }
            }
            EntityEffect::ApplyMobEffect {
                effect,
                min_duration,
                max_duration,
                min_amplifier,
                max_amplifier,
            } => {
                let Some(id) = EffectId::from_name(effect) else {
                    return;
                };
                let seconds = between(
                    &mut self.rng,
                    min_duration.calculate(level),
                    max_duration.calculate(level),
                );
                let ticks = (seconds * TICKS_PER_SECOND).round().max(0.0) as u32;
                let amplifier = between(
                    &mut self.rng,
                    min_amplifier.calculate(level),
                    max_amplifier.calculate(level),
                )
                .round()
                .clamp(0.0, 255.0) as u8;
                let instance =
                    EffectInstance::new(id, EffectDuration::Ticks(ticks), amplifier, true);
                self.add_effect_to(target, instance, EntityPotionEffectCause::Attack);
            }
            EntityEffect::DamageEntity {
                damage_type,
                min_damage,
                max_damage,
            } => {
                let Some(kind) = DamageType::from_name(damage_type) else {
                    return;
                };
                let amount = between(
                    &mut self.rng,
                    min_damage.calculate(level),
                    max_damage.calculate(level),
                );
                let source = match self.player_of(owner) {
                    Some(player) => self.source_of(player),
                    None => self.mob_source(owner),
                };
                let source = match source {
                    Some(source) => DamageSource::by(kind, source),
                    None => DamageSource::new(kind),
                };
                match self.player_of(target) {
                    Some(player) => {
                        self.hurt(player, &source, amount);
                    }
                    None => {
                        self.hurt_mob(target, &source, amount);
                    }
                }
            }
            EntityEffect::ChangeItemDamage(_) => {}
        }
    }

    fn player_of(&self, entity: EntityId) -> Option<PlayerId> {
        self.players
            .values()
            .find(|player| player.entity_id == entity)
            .map(|player| player.id)
    }
}
