use rspigot_items::{EquipmentSlot, ModifierOperation};

use crate::effect::{
    ActiveEffects, AttributeModifier, AttributeOperation, AttributeState, effect_attributes,
    player_base,
};
use crate::ids::ItemId;
use crate::item::ItemStack;
use rspigot_nbt::Tag;

pub const ATTRIBUTE_MODIFIERS: &str = "minecraft:attribute_modifiers";
const ATTRIBUTE_REGISTRY: &str = "minecraft:attribute";

pub const MAX_HEALTH: &str = "minecraft:max_health";
pub const MAX_ABSORPTION: &str = "minecraft:max_absorption";
pub const ATTACK_DAMAGE: &str = "minecraft:attack_damage";
pub const ATTACK_SPEED: &str = "minecraft:attack_speed";
pub const ATTACK_KNOCKBACK: &str = "minecraft:attack_knockback";
pub const ARMOR: &str = "minecraft:armor";
pub const ARMOR_TOUGHNESS: &str = "minecraft:armor_toughness";
pub const SAFE_FALL_DISTANCE: &str = "minecraft:safe_fall_distance";
pub const FALL_DAMAGE_MULTIPLIER: &str = "minecraft:fall_damage_multiplier";
pub const KNOCKBACK_RESISTANCE: &str = "minecraft:knockback_resistance";
pub const MOVEMENT_SPEED: &str = "minecraft:movement_speed";
pub const LUCK: &str = "minecraft:luck";

const RANGES: [(&str, f64, f64); 11] = [
    (MAX_HEALTH, 1.0, 1024.0),
    (MAX_ABSORPTION, 0.0, 2048.0),
    (ATTACK_DAMAGE, 0.0, 2048.0),
    (ATTACK_SPEED, 0.0, 1024.0),
    (ATTACK_KNOCKBACK, 0.0, 5.0),
    (ARMOR, 0.0, 30.0),
    (ARMOR_TOUGHNESS, 0.0, 20.0),
    (SAFE_FALL_DISTANCE, -1024.0, 1024.0),
    (FALL_DAMAGE_MULTIPLIER, 0.0, 100.0),
    (KNOCKBACK_RESISTANCE, -2.0, 1.0),
    (MOVEMENT_SPEED, 0.0, 1024.0),
];

const SYNCABLE: [&str; 14] = [
    "minecraft:burning_time",
    "minecraft:explosion_knockback_resistance",
    "minecraft:mining_efficiency",
    "minecraft:oxygen_bonus",
    "minecraft:sneaking_speed",
    "minecraft:submerged_mining_speed",
    "minecraft:water_movement_efficiency",
    MAX_HEALTH,
    MAX_ABSORPTION,
    ATTACK_SPEED,
    ARMOR,
    ARMOR_TOUGHNESS,
    SAFE_FALL_DISTANCE,
    FALL_DAMAGE_MULTIPLIER,
];

pub fn is_syncable(attribute: &str) -> bool {
    SYNCABLE.contains(&attribute)
}

pub fn sanitize(attribute: &str, value: f64) -> f64 {
    match RANGES.iter().find(|(name, _, _)| *name == attribute) {
        Some((_, min, _)) if value.is_nan() => *min,
        Some((_, min, max)) => value.clamp(*min, *max),
        None => value,
    }
}

pub fn value(state: &AttributeState) -> f64 {
    let operation = |kind: AttributeOperation| {
        state
            .modifiers
            .iter()
            .filter(move |m| m.operation == kind)
            .map(|m| m.amount)
    };
    let base = state.base + operation(AttributeOperation::AddValue).sum::<f64>();
    let mut result = base;
    for amount in operation(AttributeOperation::AddMultipliedBase) {
        result += base * amount;
    }
    for amount in operation(AttributeOperation::AddMultipliedTotal) {
        result *= 1.0 + amount;
    }
    sanitize(state.attribute, result)
}

const fn operation(operation: ModifierOperation) -> AttributeOperation {
    match operation {
        ModifierOperation::AddValue => AttributeOperation::AddValue,
        ModifierOperation::AddMultipliedBase => AttributeOperation::AddMultipliedBase,
        ModifierOperation::AddMultipliedTotal => AttributeOperation::AddMultipliedTotal,
    }
}

pub fn equipment_modifiers(
    slot: EquipmentSlot,
    item: ItemId,
) -> impl Iterator<Item = (&'static str, AttributeModifier)> {
    item.kind()
        .map_or(&[][..], |kind| kind.attribute_modifiers())
        .iter()
        .filter(move |m| m.slot.contains(slot))
        .map(|m| {
            (
                m.attribute,
                AttributeModifier {
                    id: std::borrow::Cow::Borrowed(m.id),
                    amount: m.amount,
                    operation: operation(m.operation),
                },
            )
        })
}

fn registered_attribute(name: &str) -> Option<&'static str> {
    let name = if name.contains(':') {
        std::borrow::Cow::Borrowed(name)
    } else {
        std::borrow::Cow::Owned(format!("minecraft:{name}"))
    };
    rspigot_data::registries::static_entries(ATTRIBUTE_REGISTRY)?
        .iter()
        .copied()
        .find(|known| *known == name)
}

fn slot_group(name: &str) -> Option<rspigot_items::SlotGroup> {
    use rspigot_items::SlotGroup as Group;
    Some(match name {
        "any" => Group::Any,
        "mainhand" => Group::Mainhand,
        "offhand" => Group::Offhand,
        "hand" => Group::Hand,
        "feet" => Group::Feet,
        "legs" => Group::Legs,
        "chest" => Group::Chest,
        "head" => Group::Head,
        "armor" => Group::Armor,
        "body" => Group::Body,
        "saddle" => Group::Saddle,
        _ => return None,
    })
}

fn component_modifier(
    slot: EquipmentSlot,
    entry: &Tag,
) -> Option<(&'static str, AttributeModifier)> {
    let entry = entry.as_compound()?;
    let attribute = registered_attribute(entry.get_str("type")?)?;
    let group = slot_group(entry.get_str("slot").unwrap_or("any"))?;
    if !group.contains(slot) {
        return None;
    }
    let operation = match entry.get_str("operation")? {
        "add_value" => AttributeOperation::AddValue,
        "add_multiplied_base" => AttributeOperation::AddMultipliedBase,
        "add_multiplied_total" => AttributeOperation::AddMultipliedTotal,
        _ => return None,
    };
    Some((
        attribute,
        AttributeModifier {
            id: std::borrow::Cow::Owned(entry.get_str("id")?.to_owned()),
            amount: entry
                .get_f64("amount")
                .filter(|amount| amount.is_finite())?,
            operation,
        },
    ))
}

pub fn worn_modifiers(
    slot: EquipmentSlot,
    item: ItemId,
    component: Option<&Tag>,
) -> Vec<(&'static str, AttributeModifier)> {
    match component {
        Some(Tag::List(entries)) => entries
            .iter()
            .filter_map(|entry| component_modifier(slot, entry))
            .collect(),
        _ => equipment_modifiers(slot, item).collect(),
    }
}

#[derive(Debug, Clone, Copy)]
enum Amount {
    Linear {
        base: f32,
        per_level_above_first: f32,
    },
    LevelsSquared {
        added: f32,
    },
}

impl Amount {
    fn calculate(self, level: i32) -> f32 {
        match self {
            Self::Linear {
                base,
                per_level_above_first,
            } => base + per_level_above_first * (level - 1) as f32,
            Self::LevelsSquared { added } => (level * level) as f32 + added,
        }
    }
}

struct EnchantmentAttribute {
    enchantment: &'static str,
    slot: &'static str,
    attribute: &'static str,
    operation: AttributeOperation,
    amount: Amount,
}

const THIRD: f32 = 0.333_333_34;

const ENCHANTMENT_ATTRIBUTES: [EnchantmentAttribute; 7] = [
    EnchantmentAttribute {
        enchantment: "minecraft:aqua_affinity",
        slot: "head",
        attribute: "minecraft:submerged_mining_speed",
        operation: AttributeOperation::AddMultipliedTotal,
        amount: Amount::Linear {
            base: 4.0,
            per_level_above_first: 4.0,
        },
    },
    EnchantmentAttribute {
        enchantment: "minecraft:depth_strider",
        slot: "feet",
        attribute: "minecraft:water_movement_efficiency",
        operation: AttributeOperation::AddValue,
        amount: Amount::Linear {
            base: THIRD,
            per_level_above_first: THIRD,
        },
    },
    EnchantmentAttribute {
        enchantment: "minecraft:respiration",
        slot: "head",
        attribute: "minecraft:oxygen_bonus",
        operation: AttributeOperation::AddValue,
        amount: Amount::Linear {
            base: 1.0,
            per_level_above_first: 1.0,
        },
    },
    EnchantmentAttribute {
        enchantment: "minecraft:swift_sneak",
        slot: "legs",
        attribute: "minecraft:sneaking_speed",
        operation: AttributeOperation::AddValue,
        amount: Amount::Linear {
            base: 0.15,
            per_level_above_first: 0.15,
        },
    },
    EnchantmentAttribute {
        enchantment: "minecraft:fire_protection",
        slot: "armor",
        attribute: "minecraft:burning_time",
        operation: AttributeOperation::AddMultipliedBase,
        amount: Amount::Linear {
            base: -0.15,
            per_level_above_first: -0.15,
        },
    },
    EnchantmentAttribute {
        enchantment: "minecraft:blast_protection",
        slot: "armor",
        attribute: "minecraft:explosion_knockback_resistance",
        operation: AttributeOperation::AddValue,
        amount: Amount::Linear {
            base: 0.15,
            per_level_above_first: 0.15,
        },
    },
    EnchantmentAttribute {
        enchantment: "minecraft:efficiency",
        slot: "mainhand",
        attribute: "minecraft:mining_efficiency",
        operation: AttributeOperation::AddValue,
        amount: Amount::LevelsSquared { added: 1.0 },
    },
];

pub fn enchantment_modifiers(
    slot: EquipmentSlot,
    stack: &ItemStack,
) -> Vec<(&'static str, AttributeModifier)> {
    enchantment_modifiers_for(slot, &crate::enchant::levels(stack))
}

pub fn enchantment_modifiers_for(
    slot: EquipmentSlot,
    levels: &[(String, u8)],
) -> Vec<(&'static str, AttributeModifier)> {
    let mut out = Vec::new();
    for (name, level) in levels.iter().map(|(name, level)| (name.clone(), *level)) {
        for entry in ENCHANTMENT_ATTRIBUTES
            .iter()
            .filter(|entry| entry.enchantment == name)
        {
            if !slot_group(entry.slot).is_some_and(|group| group.contains(slot)) {
                continue;
            }
            let path = name.strip_prefix("minecraft:").unwrap_or(&name);
            out.push((
                entry.attribute,
                AttributeModifier {
                    id: std::borrow::Cow::Owned(format!(
                        "minecraft:enchantment.{path}/{}",
                        slot.name()
                    )),
                    amount: f64::from(entry.amount.calculate(i32::from(level))),
                    operation: entry.operation,
                },
            ));
        }
    }
    out
}

pub fn stack_modifiers(
    slot: EquipmentSlot,
    stack: &ItemStack,
) -> Vec<(&'static str, AttributeModifier)> {
    worn_modifiers(slot, stack.item, stack.components.get(ATTRIBUTE_MODIFIERS))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Attributes {
    pub states: Vec<AttributeState>,
}

impl Attributes {
    pub fn of(effects: &ActiveEffects, equipment: &[(EquipmentSlot, &ItemStack)]) -> Self {
        let mut states = effect_attributes(effects);
        for (slot, stack) in equipment {
            let modifiers = stack_modifiers(*slot, stack)
                .into_iter()
                .chain(enchantment_modifiers(*slot, stack));
            for (attribute, modifier) in modifiers {
                match states.iter_mut().find(|s| s.attribute == attribute) {
                    Some(state) => state.modifiers.push(modifier),
                    None => states.push(AttributeState {
                        attribute,
                        base: player_base(attribute),
                        modifiers: vec![modifier],
                    }),
                }
            }
        }
        Self { states }
    }

    pub fn with_base(mut self, attribute: &'static str, base: f64) -> Self {
        match self.states.iter_mut().find(|s| s.attribute == attribute) {
            Some(state) => state.base = base,
            None => self.states.push(AttributeState {
                attribute,
                base,
                modifiers: Vec::new(),
            }),
        }
        self
    }

    pub fn with_instance(mut self, edit: &crate::entity::living::AttributeInstance) -> Self {
        match self
            .states
            .iter_mut()
            .find(|s| s.attribute == edit.attribute)
        {
            Some(state) => {
                state.base = edit.base;
                state.modifiers.extend(edit.modifiers.iter().cloned());
            }
            None => self.states.push(AttributeState {
                attribute: edit.attribute,
                base: edit.base,
                modifiers: edit.modifiers.clone(),
            }),
        }
        self
    }

    pub fn get(&self, attribute: &str) -> f64 {
        self.states
            .iter()
            .find(|s| s.attribute == attribute)
            .map_or_else(|| sanitize(attribute, player_base(attribute)), value)
    }

    pub fn state(&self, attribute: &'static str) -> AttributeState {
        self.states
            .iter()
            .find(|s| s.attribute == attribute)
            .cloned()
            .unwrap_or(AttributeState {
                attribute,
                base: player_base(attribute),
                modifiers: Vec::new(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::{EffectDuration, EffectId, EffectInstance};
    use rspigot_nbt::{Compound, Tag};

    fn effect(name: &str, amplifier: u8) -> EffectInstance {
        EffectInstance::new(
            EffectId::from_name(name).unwrap(),
            EffectDuration::Ticks(100),
            amplifier,
            true,
        )
    }

    fn item(name: &str) -> ItemStack {
        ItemStack::new(
            rspigot_items::ItemKind::from_name(name).unwrap().id(),
            std::num::NonZeroU8::MIN,
        )
    }

    fn with_modifiers(mut stack: ItemStack, entries: Vec<Tag>) -> ItemStack {
        stack
            .components
            .insert(ATTRIBUTE_MODIFIERS, Tag::List(entries));
        stack
    }

    fn entry(attribute: &str, amount: f64, operation: &str, slot: &str) -> Tag {
        Tag::Compound(
            Compound::new()
                .with("type", attribute)
                .with("id", "test:mod")
                .with("amount", amount)
                .with("operation", operation)
                .with("slot", slot),
        )
    }

    fn of(effects: &ActiveEffects, worn: &[(EquipmentSlot, ItemStack)]) -> Attributes {
        let borrowed: Vec<(EquipmentSlot, &ItemStack)> =
            worn.iter().map(|(slot, stack)| (*slot, stack)).collect();
        Attributes::of(effects, &borrowed)
    }

    #[test]
    fn the_luck_effects_raise_and_lower_the_luck_attribute() {
        let mut effects = ActiveEffects::default();
        assert_eq!(of(&effects, &[]).get(LUCK), 0.0);
        let _ = effects.apply(effect("minecraft:luck", 2));
        assert_eq!(of(&effects, &[]).get(LUCK), 3.0);
        let mut unlucky = ActiveEffects::default();
        let _ = unlucky.apply(effect("minecraft:unluck", 1));
        assert_eq!(of(&unlucky, &[]).get(LUCK), -2.0);
    }

    #[test]
    fn a_stack_modifier_replaces_the_defaults_of_the_item_for_its_slot() {
        let relic = with_modifiers(
            item("diamond_sword"),
            vec![entry(ATTACK_DAMAGE, 5.0, "add_value", "mainhand")],
        );
        let held = [(EquipmentSlot::Mainhand, relic.clone())];
        let attributes = of(&ActiveEffects::default(), &held);
        assert_eq!(attributes.get(ATTACK_DAMAGE), 6.0);
        assert_eq!(attributes.get(ATTACK_SPEED), 4.0);
        let offhand = [(EquipmentSlot::Offhand, relic)];
        assert_eq!(
            of(&ActiveEffects::default(), &offhand).get(ATTACK_DAMAGE),
            1.0
        );
    }

    #[test]
    fn an_empty_list_removes_the_defaults() {
        let bare = with_modifiers(item("diamond_chestplate"), Vec::new());
        let worn = [(EquipmentSlot::Chest, bare)];
        let attributes = of(&ActiveEffects::default(), &worn);
        assert_eq!(attributes.get(ARMOR), 0.0);
        assert_eq!(attributes.get(ARMOR_TOUGHNESS), 0.0);
    }

    #[test]
    fn a_stack_replaces_the_default_armor_and_a_slot_mismatch_does_nothing() {
        let plated = with_modifiers(
            item("diamond_chestplate"),
            vec![entry(ARMOR, 2.0, "add_value", "chest")],
        );
        let worn = [(EquipmentSlot::Chest, plated)];
        let attributes = of(&ActiveEffects::default(), &worn);
        assert_eq!(attributes.get(ARMOR), 2.0);
        assert_eq!(attributes.get(ARMOR_TOUGHNESS), 0.0);
        let wrong = with_modifiers(
            item("diamond_chestplate"),
            vec![entry(ARMOR, 7.0, "add_value", "feet")],
        );
        let worn = [(EquipmentSlot::Chest, wrong)];
        assert_eq!(of(&ActiveEffects::default(), &worn).get(ARMOR), 0.0);
    }

    #[test]
    fn stack_modifiers_apply_their_operations_in_vanilla_order_and_groups_match() {
        let tool = with_modifiers(
            item("stick"),
            vec![
                entry(ATTACK_DAMAGE, 1.0, "add_value", "hand"),
                entry(ATTACK_DAMAGE, 0.5, "add_multiplied_base", "any"),
                entry(ATTACK_DAMAGE, 1.0, "add_multiplied_total", "mainhand"),
                entry(ATTACK_DAMAGE, 100.0, "add_value", "armor"),
            ],
        );
        let held = [(EquipmentSlot::Mainhand, tool)];
        let attributes = of(&ActiveEffects::default(), &held);
        assert_eq!(attributes.get(ATTACK_DAMAGE), 6.0);
    }

    #[test]
    fn invalid_stack_entries_are_skipped() {
        let junk = with_modifiers(
            item("stick"),
            vec![
                entry("minecraft:nope", 1.0, "add_value", "any"),
                entry(ATTACK_DAMAGE, 1.0, "bad", "any"),
                entry(ATTACK_DAMAGE, 1.0, "add_value", "bad"),
                Tag::Int(3),
                entry(ATTACK_DAMAGE, 2.0, "add_value", "any"),
            ],
        );
        let held = [(EquipmentSlot::Mainhand, junk)];
        assert_eq!(of(&ActiveEffects::default(), &held).get(ATTACK_DAMAGE), 3.0);
    }

    #[test]
    fn defaults_are_the_player_bases() {
        let attributes = Attributes::default();
        assert_eq!(attributes.get(MAX_HEALTH), 20.0);
        assert_eq!(attributes.get(ATTACK_DAMAGE), 1.0);
        assert_eq!(attributes.get(ATTACK_SPEED), 4.0);
        assert_eq!(attributes.get(SAFE_FALL_DISTANCE), 3.0);
        assert_eq!(attributes.get(FALL_DAMAGE_MULTIPLIER), 1.0);
        assert_eq!(attributes.get(ARMOR), 0.0);
    }

    #[test]
    fn held_sword_and_strength_add_attack_damage() {
        let mut effects = ActiveEffects::default();
        let _ = effects.apply(effect("strength", 1));
        let held = [(EquipmentSlot::Mainhand, item("diamond_sword"))];
        let attributes = of(&effects, &held);
        assert_eq!(attributes.get(ATTACK_DAMAGE), 1.0 + 6.0 + 6.0);
        assert!((attributes.get(ATTACK_SPEED) - 1.6).abs() < 1e-6);
        let offhand = [(EquipmentSlot::Offhand, item("diamond_sword"))];
        assert_eq!(
            of(&ActiveEffects::default(), &offhand).get(ATTACK_DAMAGE),
            1.0
        );
    }

    #[test]
    fn jump_boost_and_health_boost_raise_their_attributes() {
        let mut effects = ActiveEffects::default();
        let _ = effects.apply(effect("jump_boost", 1));
        let _ = effects.apply(effect("health_boost", 0));
        let attributes = Attributes::of(&effects, &[]);
        assert_eq!(attributes.get(SAFE_FALL_DISTANCE), 5.0);
        assert_eq!(attributes.get(MAX_HEALTH), 24.0);
    }

    #[test]
    fn armor_pieces_stack_and_operations_follow_vanilla_order() {
        let worn = [
            (EquipmentSlot::Head, item("diamond_helmet")),
            (EquipmentSlot::Chest, item("diamond_chestplate")),
            (EquipmentSlot::Mainhand, item("diamond_helmet")),
        ];
        let attributes = of(&ActiveEffects::default(), &worn);
        assert_eq!(attributes.get(ARMOR), 3.0 + 8.0);
        assert_eq!(attributes.get(ARMOR_TOUGHNESS), 4.0);
        let state = AttributeState {
            attribute: ATTACK_DAMAGE,
            base: 2.0,
            modifiers: vec![
                AttributeModifier {
                    id: "a".into(),
                    amount: 1.0,
                    operation: AttributeOperation::AddValue,
                },
                AttributeModifier {
                    id: "b".into(),
                    amount: 0.5,
                    operation: AttributeOperation::AddMultipliedBase,
                },
                AttributeModifier {
                    id: "c".into(),
                    amount: 1.0,
                    operation: AttributeOperation::AddMultipliedTotal,
                },
            ],
        };
        assert_eq!(value(&state), 9.0);
        assert_eq!(sanitize(MAX_HEALTH, 0.0), 1.0);
        assert_eq!(sanitize(ARMOR, 99.0), 30.0);
        assert_eq!(sanitize(ARMOR, f64::NAN), 0.0);
    }

    #[test]
    fn a_new_base_keeps_the_modifiers_and_its_range() {
        let mut effects = ActiveEffects::default();
        let _ = effects.apply(effect("speed", 1));
        let attributes = Attributes::of(&effects, &[]).with_base(MOVEMENT_SPEED, 0.2);
        let state = attributes.state(MOVEMENT_SPEED);
        assert_eq!(state.base, 0.2);
        assert_eq!(state.modifiers.len(), 1);
        assert!((attributes.get(MOVEMENT_SPEED) - 0.2 * 1.4).abs() < 1e-6);

        let still = Attributes::default().with_base(MOVEMENT_SPEED, -0.5);
        assert_eq!(still.state(MOVEMENT_SPEED).base, -0.5);
        assert!(still.state(MOVEMENT_SPEED).modifiers.is_empty());
        assert_eq!(still.get(MOVEMENT_SPEED), 0.0);
    }

    fn enchanted(name: &str, enchantment: &str, level: u8) -> ItemStack {
        let mut stack = item(name);
        crate::enchant::set_levels(&mut stack, &[(enchantment.to_owned(), level)]);
        stack
    }

    fn worn(slot: EquipmentSlot, stack: &ItemStack) -> Attributes {
        Attributes::of(&ActiveEffects::default(), &[(slot, stack)])
    }

    #[test]
    fn aqua_affinity_multiplies_submerged_mining_speed_by_five() {
        let helmet = enchanted("iron_helmet", "minecraft:aqua_affinity", 1);
        let attributes = worn(EquipmentSlot::Head, &helmet);
        assert!((attributes.get("minecraft:submerged_mining_speed") - 1.0).abs() < 1e-6);
        let bare = worn(EquipmentSlot::Head, &item("iron_helmet"));
        assert!((bare.get("minecraft:submerged_mining_speed") - 0.2).abs() < 1e-6);
    }

    #[test]
    fn enchantment_attributes_only_apply_in_their_slot() {
        let helmet = enchanted("iron_helmet", "minecraft:aqua_affinity", 1);
        let in_hand = worn(EquipmentSlot::Mainhand, &helmet);
        assert!((in_hand.get("minecraft:submerged_mining_speed") - 0.2).abs() < 1e-6);
    }

    #[test]
    fn depth_strider_respiration_and_swift_sneak_add_their_levels() {
        let boots = enchanted("iron_boots", "minecraft:depth_strider", 3);
        let water = worn(EquipmentSlot::Feet, &boots).get("minecraft:water_movement_efficiency");
        assert!((water - 1.0).abs() < 1e-5, "{water}");
        let helmet = enchanted("iron_helmet", "minecraft:respiration", 2);
        assert_eq!(
            worn(EquipmentSlot::Head, &helmet).get("minecraft:oxygen_bonus"),
            2.0
        );
        let legs = enchanted("iron_leggings", "minecraft:swift_sneak", 2);
        let sneak = worn(EquipmentSlot::Legs, &legs).get("minecraft:sneaking_speed");
        assert!((sneak - 0.6).abs() < 1e-6, "{sneak}");
    }

    #[test]
    fn fire_and_blast_protection_set_burning_time_and_explosion_resistance() {
        let fire = enchanted("iron_chestplate", "minecraft:fire_protection", 4);
        let burning = worn(EquipmentSlot::Chest, &fire).get("minecraft:burning_time");
        assert!((burning - 0.4).abs() < 1e-6, "{burning}");
        let blast = enchanted("iron_chestplate", "minecraft:blast_protection", 4);
        let resistance =
            worn(EquipmentSlot::Chest, &blast).get("minecraft:explosion_knockback_resistance");
        assert!((resistance - 0.6).abs() < 1e-6, "{resistance}");
    }

    #[test]
    fn efficiency_adds_level_squared_plus_one_to_mining_efficiency() {
        let pickaxe = enchanted("iron_pickaxe", "minecraft:efficiency", 3);
        assert_eq!(
            worn(EquipmentSlot::Mainhand, &pickaxe).get("minecraft:mining_efficiency"),
            10.0
        );
        assert_eq!(
            worn(EquipmentSlot::Offhand, &pickaxe).get("minecraft:mining_efficiency"),
            0.0
        );
    }
}
