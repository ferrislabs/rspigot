use rspigot_data::enchantments::{self, EnchantmentDef};
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

pub fn damage_after_unbreaking(
    level: i32,
    armor: bool,
    amount: i32,
    mut roll: impl FnMut() -> f32,
) -> i32 {
    if level <= 0 || amount <= 0 {
        return amount;
    }
    let level = level as f32;
    let removed = if armor {
        (2.0 * level) / (10.0 + 5.0 * (level - 1.0))
    } else {
        level / (level + 1.0)
    };
    (0..amount).filter(|_| roll() >= removed).count() as i32
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
    fn unbreaking_removes_damage_by_the_enchantment_odds() {
        assert_eq!(damage_after_unbreaking(0, false, 3, || 0.0), 3);
        assert_eq!(damage_after_unbreaking(1, false, 4, || 0.49), 0);
        assert_eq!(damage_after_unbreaking(1, false, 4, || 0.51), 4);
        assert_eq!(damage_after_unbreaking(3, false, 4, || 0.74), 0);
        assert_eq!(damage_after_unbreaking(3, false, 4, || 0.76), 4);
        assert_eq!(damage_after_unbreaking(1, true, 4, || 0.19), 0);
        assert_eq!(damage_after_unbreaking(1, true, 4, || 0.21), 4);
        assert_eq!(damage_after_unbreaking(5, true, 0, || 0.0), 0);
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
}
