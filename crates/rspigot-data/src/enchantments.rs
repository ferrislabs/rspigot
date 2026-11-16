use crate::generated::enchantment_effects::EFFECTS;
use crate::generated::enchantments::ENCHANTMENTS;
use crate::loot::{Condition, LevelBasedValue};
use crate::registries;
use crate::tags;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cost {
    pub base: i32,
    pub per_level_above_first: i32,
}

impl Cost {
    pub fn at(self, level: i32) -> i32 {
        self.base + self.per_level_above_first * (level - 1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnchantmentDef {
    pub name: &'static str,
    pub max_level: u8,
    pub min_cost: Cost,
    pub max_cost: Cost,
    pub anvil_cost: u8,
    pub weight: u8,
    pub exclusive_set: Option<&'static str>,
    pub supported_items: &'static str,
    pub primary_items: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValueOp {
    Add(LevelBasedValue),
    Multiply(LevelBasedValue),
    Set(LevelBasedValue),
    RemoveBinomial(LevelBasedValue),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValueEffect {
    pub op: ValueOp,
    pub requirements: Option<Condition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectRole {
    Attacker,
    Victim,
    DamagingEntity,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EntityEffect {
    Ignite(LevelBasedValue),
    ApplyMobEffect {
        effect: &'static str,
        min_duration: LevelBasedValue,
        max_duration: LevelBasedValue,
        min_amplifier: LevelBasedValue,
        max_amplifier: LevelBasedValue,
    },
    DamageEntity {
        damage_type: &'static str,
        min_damage: LevelBasedValue,
        max_damage: LevelBasedValue,
    },
    ChangeItemDamage(LevelBasedValue),
    AllOf(&'static [EntityEffect]),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TargetedEffect {
    pub enchanted: EffectRole,
    pub affected: EffectRole,
    pub effect: EntityEffect,
    pub requirements: Option<Condition>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConditionalEffect {
    pub effect: EntityEffect,
    pub requirements: Option<Condition>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttributeEffect {
    pub attribute: &'static str,
    pub amount: LevelBasedValue,
    pub operation: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnchantmentEffects {
    pub slots: &'static [&'static str],
    pub damage: &'static [ValueEffect],
    pub damage_protection: &'static [ValueEffect],
    pub knockback: &'static [ValueEffect],
    pub armor_effectiveness: &'static [ValueEffect],
    pub smash_damage_per_fallen_block: &'static [ValueEffect],
    pub equipment_drops: &'static [ValueEffect],
    pub ammo_use: &'static [ValueEffect],
    pub projectile_count: &'static [ValueEffect],
    pub projectile_spread: &'static [ValueEffect],
    pub projectile_piercing: &'static [ValueEffect],
    pub crossbow_charge_time: Option<ValueOp>,
    pub post_attack: &'static [TargetedEffect],
    pub projectile_spawned: &'static [ConditionalEffect],
    pub attributes: &'static [AttributeEffect],
}

pub fn effects(name: &str) -> Option<&'static EnchantmentEffects> {
    EFFECTS.get(id(name)? as usize)
}

const REGISTRY: &str = "minecraft:enchantment";
const ITEM_REGISTRY: &str = "minecraft:item";
const CURSE_TAG: &str = "minecraft:curse";

pub fn all() -> &'static [EnchantmentDef] {
    &ENCHANTMENTS
}

pub fn id(name: &str) -> Option<u32> {
    registries::synced_index(REGISTRY, name)
}

pub fn by_name(name: &str) -> Option<&'static EnchantmentDef> {
    ENCHANTMENTS.get(id(name)? as usize)
}

impl EnchantmentDef {
    pub fn id(&self) -> u32 {
        id(self.name).unwrap_or(u32::MAX)
    }

    pub fn is_curse(&self) -> bool {
        tags::tag(REGISTRY, CURSE_TAG).is_some_and(|ids| ids.contains(&self.id()))
    }

    pub fn supports(&self, item: u16) -> bool {
        tags::tag(ITEM_REGISTRY, self.supported_items.trim_start_matches('#'))
            .is_some_and(|ids| ids.binary_search(&u32::from(item)).is_ok())
    }

    pub fn compatible_with(&self, other: &EnchantmentDef) -> bool {
        if self.name == other.name {
            return false;
        }
        let excludes = |from: &EnchantmentDef, to: &EnchantmentDef| {
            from.exclusive_set
                .and_then(|set| tags::tag(REGISTRY, set.trim_start_matches('#')))
                .is_some_and(|ids| ids.contains(&to.id()))
        };
        !excludes(self, other) && !excludes(other, self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str) -> u16 {
        registries::static_index("minecraft:item", name).unwrap() as u16
    }

    #[test]
    fn every_synced_enchantment_has_a_definition_in_registry_order() {
        assert_eq!(all().len(), 43);
        for (index, def) in all().iter().enumerate() {
            assert_eq!(id(def.name), Some(index as u32));
        }
    }

    #[test]
    fn sharpness_matches_the_datapack() {
        let sharpness = by_name("minecraft:sharpness").unwrap();
        assert_eq!(sharpness.max_level, 5);
        assert_eq!(sharpness.anvil_cost, 1);
        assert_eq!(sharpness.weight, 10);
        assert_eq!(sharpness.min_cost.at(3), 23);
        assert_eq!(sharpness.max_cost.at(1), 21);
        assert!(sharpness.supports(item("minecraft:diamond_sword")));
        assert!(!sharpness.supports(item("minecraft:diamond_pickaxe")));
    }

    #[test]
    fn exclusive_sets_and_curses() {
        let sharpness = by_name("minecraft:sharpness").unwrap();
        let smite = by_name("minecraft:smite").unwrap();
        let unbreaking = by_name("minecraft:unbreaking").unwrap();
        assert!(!sharpness.compatible_with(smite));
        assert!(sharpness.compatible_with(unbreaking));
        assert!(!sharpness.compatible_with(sharpness));
        assert!(by_name("minecraft:vanishing_curse").unwrap().is_curse());
        assert!(!sharpness.is_curse());
    }

    #[test]
    fn mending_is_a_book_and_tool_enchantment() {
        let mending = by_name("minecraft:mending").unwrap();
        assert_eq!(mending.anvil_cost, 4);
        assert_eq!(mending.max_level, 1);
        assert!(mending.supports(item("minecraft:diamond_pickaxe")));
    }

    #[test]
    fn the_damage_family_carries_its_value_effects() {
        let sharpness = effects("minecraft:sharpness").unwrap();
        assert_eq!(sharpness.slots, ["mainhand"]);
        let [effect] = sharpness.damage else {
            panic!("one damage effect")
        };
        assert_eq!(
            effect.op,
            ValueOp::Add(LevelBasedValue::Linear {
                base: 1.0,
                per_level_above_first: 0.5
            })
        );
        assert!(effect.requirements.is_none());
        assert!(
            effects("minecraft:smite").unwrap().damage[0]
                .requirements
                .is_some()
        );
        assert_eq!(effects("minecraft:protection").unwrap().slots, ["armor"]);
        assert_eq!(
            effects("minecraft:protection")
                .unwrap()
                .damage_protection
                .len(),
            1
        );
        assert_eq!(effects("minecraft:knockback").unwrap().knockback.len(), 1);
        assert_eq!(
            effects("minecraft:density")
                .unwrap()
                .smash_damage_per_fallen_block
                .len(),
            1
        );
        assert_eq!(
            effects("minecraft:breach")
                .unwrap()
                .armor_effectiveness
                .len(),
            1
        );
        assert_eq!(effects("minecraft:infinity").unwrap().ammo_use.len(), 1);
        assert_eq!(
            effects("minecraft:multishot")
                .unwrap()
                .projectile_count
                .len(),
            1
        );
        assert_eq!(
            effects("minecraft:piercing")
                .unwrap()
                .projectile_piercing
                .len(),
            1
        );
        assert!(
            effects("minecraft:quick_charge")
                .unwrap()
                .crossbow_charge_time
                .is_some()
        );
        assert_eq!(
            effects("minecraft:flame").unwrap().projectile_spawned.len(),
            1
        );
        assert_eq!(
            effects("minecraft:looting").unwrap().equipment_drops.len(),
            1
        );
    }

    #[test]
    fn post_attack_effects_keep_their_roles() {
        let fire = effects("minecraft:fire_aspect").unwrap();
        let [effect] = fire.post_attack else {
            panic!("one post attack effect")
        };
        assert_eq!(effect.enchanted, EffectRole::Attacker);
        assert_eq!(effect.affected, EffectRole::Victim);
        assert!(matches!(effect.effect, EntityEffect::Ignite(_)));
        let thorns = effects("minecraft:thorns").unwrap();
        assert_eq!(thorns.post_attack[0].enchanted, EffectRole::Victim);
        assert_eq!(thorns.post_attack[0].affected, EffectRole::Attacker);
        assert!(matches!(
            thorns.post_attack[0].effect,
            EntityEffect::AllOf(_)
        ));
        assert!(
            effects("minecraft:wind_burst")
                .unwrap()
                .post_attack
                .is_empty()
        );
    }

    #[test]
    fn sweeping_edge_gives_a_fraction_of_the_damage() {
        let sweeping = effects("minecraft:sweeping_edge").unwrap();
        let [attribute] = sweeping.attributes else {
            panic!("one attribute")
        };
        assert_eq!(attribute.attribute, "minecraft:sweeping_damage_ratio");
        assert_eq!(attribute.amount.calculate(1), 0.5);
        assert!((attribute.amount.calculate(3) - 0.75).abs() < 1e-6);
        assert!(
            effects("minecraft:efficiency")
                .unwrap()
                .attributes
                .is_empty()
        );
    }
}
