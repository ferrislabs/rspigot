use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::generated::enchantment_effects::EFFECTS;
use crate::generated::enchantments::ENCHANTMENTS;
use crate::loot::{Condition, EntityTarget, LevelBasedValue};
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
pub enum LevelValue {
    Plain(LevelBasedValue),
    Lookup {
        values: &'static [f32],
        fallback: LevelBasedValue,
    },
    Clamped {
        value: &'static LevelValue,
        min: f32,
        max: f32,
    },
}

impl LevelValue {
    pub fn calculate(&self, level: i32) -> f32 {
        match *self {
            Self::Plain(value) => value.calculate(level),
            Self::Lookup { values, fallback } => usize::try_from(level - 1)
                .ok()
                .and_then(|index| values.get(index).copied())
                .unwrap_or_else(|| fallback.calculate(level)),
            Self::Clamped { value, min, max } => value.calculate(level).clamp(min, max),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EntityCheck {
    OnGround(bool),
    Flying(bool),
    FallFlying(bool),
    InWater(bool),
    Vehicle(bool),
    MovementAffectedBy(&'static str),
    PeriodicTick(i32),
    HorizontalSpeedMin(f32),
    FallDistanceMin(f32),
    IsPlayer(bool),
    Gamemode(&'static [&'static str]),
    FoodMin(i32),
    CanSeeSky(bool),
    EntityType(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Requirement {
    Loot(Condition),
    Not(&'static Requirement),
    AnyOf(&'static [Requirement]),
    AllOf(&'static [Requirement]),
    ActiveCheck(bool),
    Entity {
        target: EntityTarget,
        check: EntityCheck,
    },
    Location {
        block_tag: Option<&'static str>,
        can_see_sky: Option<bool>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlockCondition {
    AllOf(&'static [BlockCondition]),
    MatchingBlockTag {
        offset: [i32; 3],
        tag: &'static str,
    },
    MatchingBlocks {
        offset: [i32; 3],
        blocks: &'static [&'static str],
    },
    MatchingFluids {
        offset: [i32; 3],
        fluids: &'static [&'static str],
    },
    Unobstructed,
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
    ApplyImpulse {
        direction: [f32; 3],
        coordinate_scale: [f32; 3],
        magnitude: LevelValue,
    },
    ApplyExhaustion(LevelValue),
    Explode {
        block_interaction: &'static str,
        immune_blocks: Option<&'static str>,
        knockback_multiplier: LevelValue,
        radius: LevelValue,
        small_particle: Option<&'static str>,
        large_particle: Option<&'static str>,
        sound: &'static str,
        create_fire: bool,
        attribute_to_user: bool,
    },
    SummonEntity {
        entity: &'static str,
        join_team: bool,
    },
    ReplaceDisk {
        radius: LevelValue,
        height: LevelValue,
        offset: [i32; 3],
        block: &'static str,
        state: &'static [(&'static str, &'static str)],
        predicate: Option<BlockCondition>,
        trigger_game_event: Option<&'static str>,
    },
    SpawnParticles {
        particle: &'static str,
        speed: f32,
        horizontal_movement_scale: f32,
        vertical_base: f32,
        vertical_offset: f32,
    },
    PlaySound {
        sounds: &'static [&'static str],
        volume: f32,
        pitch_min: f32,
        pitch_max: f32,
    },
    Attribute {
        id: &'static str,
        attribute: &'static str,
        amount: LevelValue,
        operation: &'static str,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TargetedEffect {
    pub enchanted: EffectRole,
    pub affected: EffectRole,
    pub effect: EntityEffect,
    pub requirements: Option<Requirement>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConditionalEffect {
    pub effect: EntityEffect,
    pub requirements: Option<Requirement>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Immunity {
    pub requirements: Option<Condition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChargingSounds {
    pub start: &'static str,
    pub end: &'static str,
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
    pub block_experience: &'static [ValueEffect],
    pub mob_experience: &'static [ValueEffect],
    pub fishing_luck_bonus: &'static [ValueEffect],
    pub fishing_time_reduction: &'static [ValueEffect],
    pub item_damage: &'static [ValueEffect],
    pub repair_with_xp: &'static [ValueEffect],
    pub trident_return_acceleration: &'static [ValueEffect],
    pub trident_spin_attack_strength: Option<ValueOp>,
    pub trident_sound: &'static [&'static str],
    pub crossbow_charge_time: Option<ValueOp>,
    pub crossbow_charging_sounds: &'static [ChargingSounds],
    pub prevent_armor_change: bool,
    pub prevent_equipment_drop: bool,
    pub damage_immunity: &'static [Immunity],
    pub post_attack: &'static [TargetedEffect],
    pub post_piercing_attack: &'static [ConditionalEffect],
    pub projectile_spawned: &'static [ConditionalEffect],
    pub hit_block: &'static [ConditionalEffect],
    pub location_changed: &'static [ConditionalEffect],
    pub tick: &'static [ConditionalEffect],
    pub attributes: &'static [AttributeEffect],
}

#[derive(Debug, Default)]
pub struct EnchantmentOverlay {
    entries: BTreeMap<String, (&'static EnchantmentDef, &'static EnchantmentEffects)>,
}

impl EnchantmentOverlay {
    pub fn insert(
        &mut self,
        name: &str,
        def: &'static EnchantmentDef,
        effects: &'static EnchantmentEffects,
    ) {
        self.entries.insert(name.to_owned(), (def, effects));
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }
}

thread_local! {
    static ACTIVE: RefCell<Option<Arc<EnchantmentOverlay>>> = const { RefCell::new(None) };
}

pub fn activate(overlay: &Arc<EnchantmentOverlay>) {
    ACTIVE.with(|active| {
        *active.borrow_mut() = (!overlay.is_empty()).then(|| Arc::clone(overlay));
    });
}

fn overlaid(name: &str) -> Option<(&'static EnchantmentDef, &'static EnchantmentEffects)> {
    ACTIVE.with(|active| {
        active
            .borrow()
            .as_ref()
            .and_then(|overlay| overlay.entries.get(name).copied())
    })
}

pub fn effects(name: &str) -> Option<&'static EnchantmentEffects> {
    match overlaid(name) {
        Some((_, effects)) => Some(effects),
        None => EFFECTS.get(id(name)? as usize),
    }
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
    match overlaid(name) {
        Some((def, _)) => Some(def),
        None => ENCHANTMENTS.get(id(name)? as usize),
    }
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
    use crate::loot::LevelBasedValue;

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
        let wind = effects("minecraft:wind_burst").unwrap();
        let [burst] = wind.post_attack else {
            panic!("one wind burst effect")
        };
        let EntityEffect::Explode {
            knockback_multiplier,
            radius,
            ..
        } = burst.effect
        else {
            panic!("explode")
        };
        assert_eq!(radius.calculate(1), 3.5);
        assert_eq!(knockback_multiplier.calculate(1), 1.2);
        assert_eq!(knockback_multiplier.calculate(3), 2.2);
        assert!((knockback_multiplier.calculate(4) - 2.55).abs() < 1e-5);
    }

    #[test]
    fn the_value_components_are_read() {
        let one = |name: &str, select: fn(&EnchantmentEffects) -> &[ValueEffect]| {
            let list = select(effects(name).unwrap());
            assert_eq!(list.len(), 1, "{name}");
            list[0].op
        };
        assert_eq!(
            one("minecraft:silk_touch", |e| e.block_experience),
            ValueOp::Set(LevelBasedValue::Constant(0.0))
        );
        assert_eq!(
            one("minecraft:luck_of_the_sea", |e| e.fishing_luck_bonus),
            ValueOp::Add(LevelBasedValue::Linear {
                base: 1.0,
                per_level_above_first: 1.0
            })
        );
        assert_eq!(
            one("minecraft:lure", |e| e.fishing_time_reduction),
            ValueOp::Add(LevelBasedValue::Linear {
                base: 5.0,
                per_level_above_first: 5.0
            })
        );
        assert_eq!(
            one("minecraft:mending", |e| e.repair_with_xp),
            ValueOp::Multiply(LevelBasedValue::Constant(2.0))
        );
        assert_eq!(
            one("minecraft:loyalty", |e| e.trident_return_acceleration),
            ValueOp::Add(LevelBasedValue::Linear {
                base: 1.0,
                per_level_above_first: 1.0
            })
        );
        assert_eq!(
            effects("minecraft:unbreaking").unwrap().item_damage.len(),
            2
        );
        assert!(
            effects("minecraft:mending")
                .unwrap()
                .mob_experience
                .is_empty()
        );
    }

    #[test]
    fn the_trident_and_crossbow_components_are_read() {
        let riptide = effects("minecraft:riptide").unwrap();
        assert_eq!(riptide.trident_sound.len(), 3);
        assert_eq!(riptide.trident_sound[0], "minecraft:item.trident.riptide_1");
        let Some(ValueOp::Add(strength)) = riptide.trident_spin_attack_strength else {
            panic!("spin attack strength")
        };
        assert_eq!(strength.calculate(3), 3.0);
        let quick = effects("minecraft:quick_charge").unwrap();
        assert_eq!(quick.crossbow_charging_sounds.len(), 3);
        assert_eq!(
            quick.crossbow_charging_sounds[1].start,
            "minecraft:item.crossbow.quick_charge_2"
        );
        assert_eq!(
            quick.crossbow_charging_sounds[1].end,
            "minecraft:item.crossbow.loading_end"
        );
    }

    #[test]
    fn the_flag_components_and_immunity_are_read() {
        assert!(
            effects("minecraft:vanishing_curse")
                .unwrap()
                .prevent_equipment_drop
        );
        assert!(
            !effects("minecraft:vanishing_curse")
                .unwrap()
                .prevent_armor_change
        );
        assert!(
            effects("minecraft:binding_curse")
                .unwrap()
                .prevent_armor_change
        );
        assert!(!effects("minecraft:mending").unwrap().prevent_equipment_drop);
        assert_eq!(
            effects("minecraft:frost_walker")
                .unwrap()
                .damage_immunity
                .len(),
            1
        );
    }

    #[test]
    fn the_conditional_entity_components_carry_the_new_effects() {
        let frost = effects("minecraft:frost_walker").unwrap();
        let [walk] = frost.location_changed else {
            panic!("one location effect")
        };
        let EntityEffect::ReplaceDisk {
            radius,
            block,
            state,
            offset,
            predicate,
            ..
        } = walk.effect
        else {
            panic!("replace disk")
        };
        assert_eq!(radius.calculate(1), 3.0);
        assert_eq!(radius.calculate(2), 4.0);
        assert_eq!(radius.calculate(40), 16.0);
        assert_eq!(block, "minecraft:frosted_ice");
        assert_eq!(state, [("age", "0")]);
        assert_eq!(offset, [0, -1, 0]);
        assert!(matches!(predicate, Some(BlockCondition::AllOf(list)) if list.len() == 4));
        assert!(walk.requirements.is_some());
        let soul = effects("minecraft:soul_speed").unwrap();
        assert_eq!(soul.location_changed.len(), 2);
        assert_eq!(soul.tick.len(), 2);
        assert!(matches!(
            soul.tick[0].effect,
            EntityEffect::SpawnParticles {
                particle: "minecraft:soul",
                ..
            }
        ));
        assert!(matches!(
            soul.tick[1].effect,
            EntityEffect::PlaySound { .. }
        ));
        let channeling = effects("minecraft:channeling").unwrap();
        assert_eq!(channeling.hit_block.len(), 1);
        assert_eq!(channeling.post_attack.len(), 1);
        let lunge = effects("minecraft:lunge").unwrap();
        let EntityEffect::AllOf(parts) = lunge.post_piercing_attack[0].effect else {
            panic!("all of")
        };
        assert!(
            parts
                .iter()
                .any(|part| matches!(part, EntityEffect::ApplyImpulse { .. }))
        );
        assert!(
            parts
                .iter()
                .any(|part| matches!(part, EntityEffect::ApplyExhaustion(_)))
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
