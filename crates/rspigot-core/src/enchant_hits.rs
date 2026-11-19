use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rspigot_data::enchantments::{EffectRole, EntityEffect};
use rspigot_domain::Vec3;
use rspigot_domain::damage::DamageType;
use rspigot_domain::effect::{EffectDuration, EffectId, EffectInstance};
use rspigot_domain::entity::{Entities, Entity, EntityState, arrow};
use rspigot_domain::{EntityId, player::PlayerId};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::EquipmentSlot;
use rustc_hash::FxHashMap;

use crate::enchant_effects::{self, EquipSlot, Planned, Worn};
use crate::events::Hand;
use crate::health::{CHEST_SLOT, DamageSource, FEET_SLOT, HEAD_SLOT, LEGS_SLOT, OFFHAND_SLOT};
use crate::loot::{LootContext, LootDamage};
use crate::player::Player;
use crate::plugins::events::EntityPotionEffectCause;
use crate::plugins::{Combuster, Holder};
use crate::ports::ClientLink;
use crate::server::Server;

const TICKS_PER_SECOND: f32 = 20.0;
const PUNCH_SCALE: f64 = 0.6;
const PUNCH_LIFT: f64 = 0.1;

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

const MOB_GEAR: [(EquipmentSlot, EquipSlot); 6] = [
    (EquipmentSlot::Mainhand, EquipSlot::Mainhand),
    (EquipmentSlot::Offhand, EquipSlot::Offhand),
    (EquipmentSlot::Head, EquipSlot::Head),
    (EquipmentSlot::Chest, EquipSlot::Chest),
    (EquipmentSlot::Legs, EquipSlot::Legs),
    (EquipmentSlot::Feet, EquipSlot::Feet),
];

fn gear_in<'a, L>(
    players: &'a FxHashMap<PlayerId, Player<L>>,
    entities: &'a Entities,
    entity: EntityId,
) -> Vec<Worn<'a>> {
    if let Some(player) = players.values().find(|p| p.entity_id == entity) {
        return worn_gear(player);
    }
    let Some(living) = entities.get(entity).and_then(Entity::living) else {
        return Vec::new();
    };
    MOB_GEAR
        .iter()
        .filter_map(|(slot, equip)| {
            Some(Worn {
                stack: living.equipment(*slot)?,
                slot: *equip,
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

    pub(crate) fn mob_protection(&self, id: EntityId, source: &DamageSource) -> f32 {
        let Some(facts) = self.entity_loot_facts(id) else {
            return 0.0;
        };
        let damage = self.damage_facts(source);
        let context = LootContext {
            this: Some(&facts),
            damage: Some(&damage),
            ..LootContext::default()
        };
        enchant_effects::damage_protection(
            &gear_in(&self.players, &self.entities.store, id),
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
                    .map(|entity| gear_in(&self.players, &self.entities.store, entity))
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
            self.apply_entity_effect_plan(plan.effect, plan.level, target, owner, Some(plan.slot));
        }
    }

    pub(crate) fn apply_entity_effect_plan(
        &mut self,
        effect: &EntityEffect,
        level: i32,
        target: EntityId,
        owner: EntityId,
        worn: Option<EquipSlot>,
    ) {
        match effect {
            EntityEffect::AllOf(effects) => {
                for effect in *effects {
                    self.apply_entity_effect_plan(effect, level, target, owner, worn);
                }
            }
            EntityEffect::Ignite(seconds) => {
                let seconds = seconds.calculate(level);
                let holder = self
                    .player_of(target)
                    .map_or(Holder::Mob(target), Holder::Player);
                let combuster = self
                    .subject_view(owner)
                    .map_or(Combuster::Nothing, Combuster::Entity);
                self.combust(holder, seconds, combuster);
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
            EntityEffect::ChangeItemDamage(amount) => {
                if let Some(slot) = worn {
                    self.wear_enchanted(owner, slot, amount.calculate(level) as i32);
                }
            }
        }
    }

    fn wear_enchanted(&mut self, owner: EntityId, slot: EquipSlot, amount: i32) {
        if let Some(player) = self.player_of(owner) {
            let armor = match slot {
                EquipSlot::Head => HEAD_SLOT,
                EquipSlot::Chest => CHEST_SLOT,
                EquipSlot::Legs => LEGS_SLOT,
                EquipSlot::Feet => FEET_SLOT,
                EquipSlot::Mainhand => return self.wear(player, Hand::Main, amount),
                EquipSlot::Offhand => return self.wear(player, Hand::Off, amount),
            };
            self.wear_slot(player, Hand::Main, armor, amount);
        } else if let Some((worn, _)) = MOB_GEAR.iter().find(|(_, equip)| *equip == slot) {
            self.damage_mob_equipment(owner, *worn, amount);
        }
    }

    pub(crate) fn equipment_drop_bonus(&mut self, victim: EntityId, source: &DamageSource) -> f32 {
        let Some(attacker) = source.causing.as_ref().map(|cause| cause.entity) else {
            return 0.0;
        };
        let Some(facts) = self.entity_loot_facts(victim) else {
            return 0.0;
        };
        let damage = self.damage_facts(source);
        let gear = gear_in(&self.players, &self.entities.store, attacker);
        let context = LootContext {
            this: Some(&facts),
            damage: Some(&damage),
            ..LootContext::default()
        };
        enchant_effects::equipment_drop_bonus(&gear, 0.0, &context, &mut self.rng)
    }

    pub(crate) fn arrow_enchantments(&mut self, arrow: EntityId, target: EntityId) -> (f64, f32) {
        let Some(Entity {
            base,
            state: EntityState::Arrow(state),
            ..
        }) = self.entities.store.get(arrow)
        else {
            return (0.0, 0.0);
        };
        let trident = base.kind == Kind::Trident;
        let base_damage = if trident {
            f64::from(arrow::TRIDENT_DAMAGE)
        } else {
            state.base_damage
        };
        let kind = if trident {
            DamageType::Trident
        } else {
            DamageType::Arrow
        };
        let owner = state.owner;
        let Some(weapon) = state.weapon.clone() else {
            return (base_damage, 0.0);
        };
        let owner_id = self.owner_entity(owner);
        let damage = LootDamage {
            kind,
            source: owner_id.and_then(|owner| self.entity_loot_facts(owner)),
            direct: self.entity_loot_facts(arrow),
            is_direct: false,
        };
        let facts = self.entity_loot_facts(target);
        let context = LootContext {
            this: facts.as_ref(),
            damage: Some(&damage),
            ..LootContext::default()
        };
        let worn = [Worn {
            stack: &weapon,
            slot: EquipSlot::Mainhand,
        }];
        let damage =
            enchant_effects::modify_damage(&worn, base_damage as f32, &context, &mut self.rng);
        let knockback = enchant_effects::modify_knockback(&worn, 0.0, &context, &mut self.rng);
        (f64::from(damage), knockback)
    }

    pub(crate) fn push_by_arrow(&mut self, target: EntityId, heading: Vec3, punch: f32) {
        if punch <= 0.0 {
            return;
        }
        let horizontal = (heading.x * heading.x + heading.z * heading.z).sqrt();
        if horizontal <= 0.0 {
            return;
        }
        let scale = (1.0 - self.knockback_resistance_of(target)).max(0.0);
        let strength = f64::from(punch) * PUNCH_SCALE * scale / horizontal;
        let push = Vec3::new(heading.x * strength, PUNCH_LIFT, heading.z * strength);
        if let Some(player) = self.players.values_mut().find(|p| p.entity_id == target) {
            player.vitals.velocity = player.vitals.velocity + push;
            player.vitals.sync_velocity = true;
        } else if let Some(entity) = self.entities.store.get_mut(target) {
            entity.base.velocity = entity.base.velocity + push;
            entity.base.needs_sync = true;
        }
    }

    fn player_of(&self, entity: EntityId) -> Option<PlayerId> {
        self.players
            .values()
            .find(|player| player.entity_id == entity)
            .map(|player| player.id)
    }
}
