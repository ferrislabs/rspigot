use rspigot_data::enchantments::{
    self, ChargingSounds, EnchantmentDef, EnchantmentEffects, ValueEffect, ValueOp,
};
use rspigot_items::ItemKind;
use rspigot_nbt::{Compound, Tag};

use crate::item::{ENCHANTMENTS, ItemStack};

pub const STORED_ENCHANTMENTS: &str = "minecraft:stored_enchantments";
pub const REPAIR_COST: &str = "minecraft:repair_cost";
const LEVELS: &str = "levels";

pub fn is_book(stack: &ItemStack) -> bool {
    stack.item.kind() == Some(ItemKind::EnchantedBook)
}

pub fn component_key(stack: &ItemStack) -> &'static str {
    if is_book(stack) {
        STORED_ENCHANTMENTS
    } else {
        ENCHANTMENTS
    }
}

pub fn levels_under(stack: &ItemStack, key: &str) -> Vec<(String, u8)> {
    let Some(enchantments) = stack.components.get_compound(key) else {
        return Vec::new();
    };
    enchantments
        .get_compound(LEVELS)
        .unwrap_or(enchantments)
        .iter()
        .filter_map(|(name, level)| Some((name.to_owned(), u8::try_from(level.as_i64()?).ok()?)))
        .collect()
}

pub fn levels(stack: &ItemStack) -> Vec<(String, u8)> {
    levels_under(stack, component_key(stack))
}

pub fn set_levels(stack: &mut ItemStack, levels: &[(String, u8)]) {
    let key = component_key(stack);
    if levels.is_empty() {
        stack.components.remove(key);
        return;
    }
    let compound: Compound = levels
        .iter()
        .map(|(name, level)| (name.clone(), Tag::Int(i32::from(*level))))
        .collect();
    stack.components.insert(key, compound);
}

pub fn has_any(stack: &ItemStack) -> bool {
    !levels_under(stack, ENCHANTMENTS).is_empty()
        || !levels_under(stack, STORED_ENCHANTMENTS).is_empty()
}

pub fn has_stored_component(stack: &ItemStack) -> bool {
    is_book(stack) || stack.components.contains_key(STORED_ENCHANTMENTS)
}

pub fn definition(name: &str) -> Option<&'static EnchantmentDef> {
    enchantments::by_name(name)
}

pub fn repair_cost(stack: &ItemStack) -> i32 {
    stack.components.get_i32(REPAIR_COST).unwrap_or(0)
}

pub fn set_repair_cost(stack: &mut ItemStack, cost: i32) {
    if cost == 0 {
        stack.components.remove(REPAIR_COST);
    } else {
        stack.components.insert(REPAIR_COST, cost);
    }
}

pub fn increased_repair_cost(cost: i32) -> i32 {
    i32::try_from((i64::from(cost) * 2 + 1).min(i64::from(i32::MAX))).unwrap_or(i32::MAX)
}

fn apply_fixed(op: ValueOp, level: i32, value: f32) -> f32 {
    match op {
        ValueOp::Add(amount) => value + amount.calculate(level),
        ValueOp::Multiply(factor) => value * factor.calculate(level),
        ValueOp::Set(amount) => amount.calculate(level),
        ValueOp::RemoveBinomial(_) => value,
    }
}

fn held_effects(stack: &ItemStack) -> impl Iterator<Item = (&'static EnchantmentEffects, i32)> {
    levels(stack).into_iter().filter_map(|(name, level)| {
        let effects = enchantments::effects(&name)?;
        effects
            .slots
            .iter()
            .any(|slot| matches!(*slot, "mainhand" | "hand" | "any"))
            .then_some((effects, i32::from(level)))
    })
}

pub fn modify_unconditional(
    stack: &ItemStack,
    select: fn(&'static EnchantmentEffects) -> &'static [ValueEffect],
    base: f32,
) -> f32 {
    held_effects(stack).fold(base, |value, (effects, level)| {
        select(effects)
            .iter()
            .filter(|effect| effect.requirements.is_none())
            .fold(value, |value, effect| apply_fixed(effect.op, level, value))
    })
}

pub fn modify_crossbow_charge_time(stack: &ItemStack, base: f32) -> f32 {
    held_effects(stack).fold(base, |value, (effects, level)| {
        effects
            .crossbow_charge_time
            .map_or(value, |op| apply_fixed(op, level, value))
    })
}

pub fn modify_trident_spin_attack_strength(stack: &ItemStack, base: f32) -> f32 {
    levels(stack)
        .into_iter()
        .filter_map(|(name, level)| {
            let op = enchantments::effects(&name)?.trident_spin_attack_strength?;
            Some((op, i32::from(level)))
        })
        .fold(base, |value, (op, level)| apply_fixed(op, level, value))
}

fn highest_level_pick<T: Copy>(
    stack: &ItemStack,
    select: fn(&'static EnchantmentEffects) -> &'static [T],
) -> Option<T> {
    levels(stack)
        .into_iter()
        .filter_map(|(name, level)| {
            let list = select(enchantments::effects(&name)?);
            (!list.is_empty()).then_some((list, usize::from(level)))
        })
        .max_by_key(|(_, level)| *level)
        .and_then(|(list, level)| list.get(level.clamp(1, list.len()) - 1).copied())
}

pub fn trident_sound(stack: &ItemStack) -> Option<&'static str> {
    highest_level_pick(stack, |e| e.trident_sound)
}

pub fn crossbow_charging_sounds(stack: &ItemStack) -> Option<ChargingSounds> {
    highest_level_pick(stack, |e| e.crossbow_charging_sounds)
}

fn carries(stack: &ItemStack, select: fn(&'static EnchantmentEffects) -> bool) -> bool {
    levels(stack)
        .into_iter()
        .any(|(name, level)| level > 0 && enchantments::effects(&name).is_some_and(select))
}

pub fn prevents_armor_change(stack: &ItemStack) -> bool {
    carries(stack, |e| e.prevent_armor_change)
}

pub fn prevents_equipment_drop(stack: &ItemStack) -> bool {
    carries(stack, |e| e.prevent_equipment_drop)
}

#[cfg(test)]
pub(crate) mod tests {
    use std::num::NonZeroU8;

    use super::*;

    pub(crate) fn stack(name: &str) -> ItemStack {
        let id = rspigot_data::registries::static_index("minecraft:item", name).unwrap();
        ItemStack::new(crate::ids::ItemId(id as u16), NonZeroU8::new(1).unwrap())
    }

    #[test]
    fn books_keep_their_levels_under_the_stored_component() {
        let mut book = stack("minecraft:enchanted_book");
        set_levels(&mut book, &[("minecraft:sharpness".into(), 3)]);
        assert!(book.components.contains_key(STORED_ENCHANTMENTS));
        assert!(!book.components.contains_key(ENCHANTMENTS));
        assert_eq!(levels(&book), vec![("minecraft:sharpness".to_owned(), 3)]);
        assert!(has_any(&book));
    }

    #[test]
    fn repair_cost_doubles_and_saturates() {
        assert_eq!(increased_repair_cost(0), 1);
        assert_eq!(increased_repair_cost(3), 7);
        assert_eq!(increased_repair_cost(i32::MAX), i32::MAX);
        let mut sword = stack("minecraft:diamond_sword");
        set_repair_cost(&mut sword, 7);
        assert_eq!(repair_cost(&sword), 7);
        set_repair_cost(&mut sword, 0);
        assert!(!sword.components.contains_key(REPAIR_COST));
    }

    #[test]
    fn unconditional_effects_follow_the_datapack_values() {
        let bow = stack("minecraft:crossbow");
        let mut shotgun = bow.clone();
        set_levels(&mut shotgun, &[("minecraft:multishot".into(), 1)]);
        assert_eq!(modify_unconditional(&bow, |e| e.projectile_count, 1.0), 1.0);
        assert_eq!(
            modify_unconditional(&shotgun, |e| e.projectile_count, 1.0),
            3.0
        );
        assert_eq!(
            modify_unconditional(&shotgun, |e| e.projectile_spread, 0.0),
            10.0
        );
        let mut quick = bow.clone();
        set_levels(&mut quick, &[("minecraft:quick_charge".into(), 3)]);
        assert_eq!(modify_crossbow_charge_time(&quick, 1.25), 0.5);
        assert_eq!(modify_crossbow_charge_time(&bow, 1.25), 1.25);
    }

    #[test]
    fn riptide_adds_its_spin_attack_strength_and_picks_its_sound_by_level() {
        let mut trident = stack("minecraft:trident");
        assert_eq!(modify_trident_spin_attack_strength(&trident, 0.0), 0.0);
        assert_eq!(trident_sound(&trident), None);
        set_levels(&mut trident, &[("minecraft:riptide".into(), 3)]);
        assert_eq!(modify_trident_spin_attack_strength(&trident, 0.0), 3.0);
        assert_eq!(
            trident_sound(&trident),
            Some("minecraft:item.trident.riptide_3")
        );
        set_levels(&mut trident, &[("minecraft:riptide".into(), 1)]);
        assert_eq!(modify_trident_spin_attack_strength(&trident, 0.0), 1.5);
        assert_eq!(
            trident_sound(&trident),
            Some("minecraft:item.trident.riptide_1")
        );
    }

    #[test]
    fn quick_charge_picks_its_charging_sounds_by_level() {
        let mut crossbow = stack("minecraft:crossbow");
        assert_eq!(crossbow_charging_sounds(&crossbow), None);
        set_levels(&mut crossbow, &[("minecraft:quick_charge".into(), 2)]);
        let sounds = crossbow_charging_sounds(&crossbow).unwrap();
        assert_eq!(sounds.start, "minecraft:item.crossbow.quick_charge_2");
        assert_eq!(sounds.end, "minecraft:item.crossbow.loading_end");
    }

    #[test]
    fn the_curses_carry_their_prevent_components() {
        let mut boots = stack("minecraft:diamond_boots");
        assert!(!prevents_armor_change(&boots));
        assert!(!prevents_equipment_drop(&boots));
        set_levels(&mut boots, &[("minecraft:binding_curse".into(), 1)]);
        assert!(prevents_armor_change(&boots));
        assert!(!prevents_equipment_drop(&boots));
        set_levels(&mut boots, &[("minecraft:vanishing_curse".into(), 1)]);
        assert!(!prevents_armor_change(&boots));
        assert!(prevents_equipment_drop(&boots));
    }
}
