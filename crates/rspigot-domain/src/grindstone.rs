use rspigot_items::ItemKind;

use crate::enchant::{self, definition};
use crate::entity::reward::{damage_value, max_damage, set_damage_value};
use crate::item::{ItemStack, Slot, max_stack, same_item_same_components};

pub const FIRST: usize = 0;
pub const SECOND: usize = 1;
pub const RESULT: usize = 2;
const MERGE_BONUS_PERCENT: i32 = 5;

fn max_of(stack: &ItemStack) -> i32 {
    max_damage(stack).unwrap_or(0)
}

fn is_curse(name: &str) -> bool {
    definition(name).is_some_and(|def| def.is_curse())
}

fn remove_non_curses(mut item: ItemStack) -> ItemStack {
    let kept: Vec<(String, u8)> = enchant::levels(&item)
        .into_iter()
        .filter(|(name, _)| is_curse(name))
        .collect();
    let was_book = enchant::is_book(&item);
    enchant::set_levels(&mut item, &kept);
    if was_book && kept.is_empty() {
        item.item = ItemKind::Book.id();
    }
    let mut cost = 0;
    for _ in 0..kept.len() {
        cost = enchant::increased_repair_cost(cost);
    }
    enchant::set_repair_cost(&mut item, cost);
    item
}

fn merge_enchantments(target: &mut ItemStack, source: &ItemStack) {
    let mut merged = enchant::levels(target);
    for (name, level) in enchant::levels(source) {
        let held = merged.iter().position(|(held, _)| *held == name);
        if is_curse(&name) && held.is_some() {
            continue;
        }
        match held {
            Some(index) => merged[index].1 = merged[index].1.max(level),
            None => merged.push((name, level)),
        }
    }
    enchant::set_levels(target, &merged);
}

fn merge(first: &ItemStack, second: &ItemStack) -> Slot {
    if first.item != second.item {
        return None;
    }
    let max = max_of(first).max(max_of(second));
    let total = (max_of(first) - damage_value(first))
        + (max_of(second) - damage_value(second))
        + max * MERGE_BONUS_PERCENT / 100;
    let damageable = max_damage(first).is_some();
    let count = if damageable {
        1
    } else if max_stack(first) > 1 && same_item_same_components(first, second) {
        2
    } else {
        return None;
    };
    let mut result = crate::item::with_count(first, count)?;
    if damageable {
        set_damage_value(&mut result, (max - total).max(0));
    }
    merge_enchantments(&mut result, second);
    Some(remove_non_curses(result))
}

pub fn compute(first: &Slot, second: &Slot) -> Slot {
    match (first, second) {
        (None, None) => None,
        (Some(a), _) if a.count.get() > 1 => None,
        (_, Some(b)) if b.count.get() > 1 => None,
        (Some(a), Some(b)) => merge(a, b),
        (Some(only), None) | (None, Some(only)) => {
            enchant::has_any(only).then(|| remove_non_curses(only.clone()))
        }
    }
}

fn experience_of(stack: &ItemStack) -> i32 {
    enchant::levels(stack)
        .iter()
        .filter(|(name, _)| !is_curse(name))
        .filter_map(|(name, level)| Some(definition(name)?.min_cost.at(i32::from(*level))))
        .sum()
}

pub fn experience(first: &Slot, second: &Slot) -> i32 {
    first.as_ref().map_or(0, experience_of) + second.as_ref().map_or(0, experience_of)
}

pub fn award(base: i32, roll: impl FnOnce(i32) -> i32) -> i32 {
    if base < 1 {
        return 0;
    }
    let half = (base + 1) / 2;
    half + roll(half)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enchant::tests::stack;

    fn enchanted(name: &str, levels: &[(&str, u8)]) -> ItemStack {
        let mut item = stack(name);
        let levels: Vec<(String, u8)> = levels
            .iter()
            .map(|(name, level)| ((*name).to_owned(), *level))
            .collect();
        enchant::set_levels(&mut item, &levels);
        item
    }

    #[test]
    fn an_empty_grindstone_has_no_result() {
        assert_eq!(compute(&None, &None), None);
    }

    #[test]
    fn a_single_plain_item_has_no_result() {
        assert_eq!(
            compute(&Some(stack("minecraft:diamond_sword")), &None),
            None
        );
    }

    #[test]
    fn a_single_enchanted_item_is_stripped_but_keeps_its_curses() {
        let sword = enchanted(
            "minecraft:diamond_sword",
            &[("minecraft:sharpness", 3), ("minecraft:vanishing_curse", 1)],
        );
        let result = compute(&None, &Some(sword)).unwrap();
        assert_eq!(
            enchant::levels(&result),
            vec![("minecraft:vanishing_curse".to_owned(), 1)]
        );
        assert_eq!(enchant::repair_cost(&result), 1);
    }

    #[test]
    fn an_enchanted_book_with_nothing_left_becomes_a_book() {
        let book = enchanted("minecraft:enchanted_book", &[("minecraft:mending", 1)]);
        let result = compute(&Some(book), &None).unwrap();
        assert_eq!(result.item.kind(), Some(ItemKind::Book));
        assert!(!enchant::has_any(&result));
        assert_eq!(enchant::repair_cost(&result), 0);
    }

    #[test]
    fn stacks_of_more_than_one_are_refused() {
        let mut sword = enchanted("minecraft:diamond_sword", &[("minecraft:sharpness", 3)]);
        sword.count = std::num::NonZeroU8::new(2).unwrap();
        assert_eq!(compute(&Some(sword), &None), None);
    }

    #[test]
    fn two_damaged_tools_merge_with_a_five_percent_bonus() {
        let mut first = stack("minecraft:diamond_pickaxe");
        let mut second = stack("minecraft:diamond_pickaxe");
        set_damage_value(&mut first, 1000);
        set_damage_value(&mut second, 1200);
        let result = compute(&Some(first), &Some(second)).unwrap();
        let max = 1561;
        let total = (max - 1000) + (max - 1200) + max * 5 / 100;
        assert_eq!(damage_value(&result), max - total);
    }

    #[test]
    fn different_items_do_not_merge() {
        let mut pick = stack("minecraft:diamond_pickaxe");
        set_damage_value(&mut pick, 10);
        let mut axe = stack("minecraft:diamond_axe");
        set_damage_value(&mut axe, 10);
        assert_eq!(compute(&Some(pick), &Some(axe)), None);
    }

    #[test]
    fn merging_carries_enchantments_and_curses_over_once() {
        let first = enchanted(
            "minecraft:diamond_pickaxe",
            &[("minecraft:efficiency", 2), ("minecraft:binding_curse", 1)],
        );
        let second = enchanted(
            "minecraft:diamond_pickaxe",
            &[("minecraft:unbreaking", 3), ("minecraft:binding_curse", 1)],
        );
        let result = compute(&Some(first), &Some(second)).unwrap();
        let levels = enchant::levels(&result);
        assert_eq!(levels, vec![("minecraft:binding_curse".to_owned(), 1)]);
        assert_eq!(enchant::repair_cost(&result), 1);
    }

    #[test]
    fn experience_sums_the_minimum_costs_of_the_non_curse_enchantments() {
        let sword = enchanted(
            "minecraft:diamond_sword",
            &[("minecraft:sharpness", 3), ("minecraft:vanishing_curse", 1)],
        );
        assert_eq!(experience(&Some(sword), &None), 1 + 11 * 2);
        assert_eq!(experience(&None, &None), 0);
    }

    #[test]
    fn the_award_is_half_rounded_up_plus_a_roll_below_that() {
        assert_eq!(award(0, |_| 99), 0);
        assert_eq!(award(23, |bound| bound - 1), 12 + 11);
        assert_eq!(award(23, |_| 0), 12);
    }
}
