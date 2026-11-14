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
}
