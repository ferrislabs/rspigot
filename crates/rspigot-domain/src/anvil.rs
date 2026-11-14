use crate::enchant::{self, definition};
use crate::entity::reward::{damage_value, max_damage, set_damage_value};
use crate::item::{ItemStack, Slot};
use crate::text::Component;

pub const INPUT: usize = 0;
pub const ADDITION: usize = 1;
pub const RESULT: usize = 2;
pub const MAXIMUM_COST: i32 = 40;
pub const MAXIMUM_NAME_LENGTH: usize = 50;
const BOOK_FEE_DIVISOR: i32 = 2;
const MERGE_BONUS_PERCENT: i32 = 12;
const REPAIR_QUARTER: i32 = 4;
const MULTIPLE_ITEMS_COST: i32 = 40;

#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub result: Slot,
    pub cost: i32,
    pub repair_items: u32,
    pub only_renaming: bool,
}

impl Outcome {
    pub fn unavailable() -> Self {
        Self {
            result: None,
            cost: -1,
            repair_items: 0,
            only_renaming: false,
        }
    }
}

impl Default for Outcome {
    fn default() -> Self {
        Self {
            result: None,
            cost: 0,
            repair_items: 0,
            only_renaming: false,
        }
    }
}

pub fn valid_name(name: &str) -> Option<&str> {
    (name.chars().count() <= MAXIMUM_NAME_LENGTH).then_some(name)
}

fn is_repair_material(input: &ItemStack, addition: &ItemStack) -> bool {
    input
        .item
        .kind()
        .and_then(|kind| kind.repairable())
        .is_some_and(|set| set.contains(addition.item))
}

fn blank(name: &str) -> bool {
    name.trim().is_empty()
}

pub fn compute(input: &Slot, addition: &Slot, name: Option<&str>, infinite: bool) -> Outcome {
    let Some(input) = input else {
        return Outcome::unavailable();
    };
    let mut result = input.clone();
    let mut prior = enchant::levels(&result);
    let mut base_cost = i64::from(enchant::repair_cost(input))
        + addition
            .as_ref()
            .map_or(0, |a| i64::from(enchant::repair_cost(a)));
    let mut total: i32 = 0;
    let mut naming: i32 = 0;
    let mut repair_items: u32 = 0;
    if let Some(addition) = addition {
        let using_book = enchant::has_stored_component(addition);
        let damageable = max_damage(&result).is_some();
        if damageable && is_repair_material(input, addition) {
            let max = max_damage(&result).unwrap_or(0);
            let mut amount = damage_value(&result).min(max / REPAIR_QUARTER);
            if amount <= 0 {
                return Outcome::unavailable();
            }
            let available = i32::from(addition.count.get());
            let mut used = 0;
            while amount > 0 && used < available {
                let damage = damage_value(&result);
                set_damage_value(&mut result, damage - amount);
                total += 1;
                amount = damage_value(&result).min(max / REPAIR_QUARTER);
                used += 1;
            }
            repair_items = u32::try_from(used).unwrap_or(0);
        } else {
            if !using_book && (result.item != addition.item || !damageable) {
                return Outcome::unavailable();
            }
            if damageable && !using_book {
                let max = max_damage(&result).unwrap_or(0);
                let first = max - damage_value(input);
                let second = max_damage(addition).unwrap_or(0) - damage_value(addition);
                let remaining = first + second + max * MERGE_BONUS_PERCENT / 100;
                let damage = (max - remaining).max(0);
                if damage < damage_value(&result) {
                    set_damage_value(&mut result, damage);
                    total += 2;
                }
            }
            let mut any_compatible = false;
            let mut any_incompatible = false;
            for (name, added) in enchant::levels(addition) {
                let Some(def) = definition(&name) else {
                    continue;
                };
                let current = prior
                    .iter()
                    .find(|(held, _)| *held == name)
                    .map_or(0, |(_, level)| *level);
                let mut level = if current == added {
                    added + 1
                } else {
                    added.max(current)
                };
                let mut compatible =
                    infinite || enchant::is_book(input) || def.supports(input.item.0);
                for (other, _) in &prior {
                    if *other == name {
                        continue;
                    }
                    if definition(other).is_some_and(|held| !def.compatible_with(held)) {
                        compatible = false;
                        total += 1;
                    }
                }
                if !compatible {
                    any_incompatible = true;
                    continue;
                }
                any_compatible = true;
                level = level.min(def.max_level);
                match prior.iter_mut().find(|(held, _)| *held == name) {
                    Some(entry) => entry.1 = level,
                    None => prior.push((name.clone(), level)),
                }
                let mut fee = i32::from(def.anvil_cost);
                if using_book {
                    fee = (fee / BOOK_FEE_DIVISOR).max(1);
                }
                total += fee * i32::from(level);
                if input.count.get() > 1 {
                    total = MULTIPLE_ITEMS_COST;
                }
            }
            if any_incompatible && !any_compatible {
                return Outcome::unavailable();
            }
        }
    }
    let typed = name.filter(|typed| !blank(typed));
    match typed {
        Some(typed) => {
            let current = input.custom_name().map(|held| held.plain_text());
            if current.as_deref() != Some(typed) {
                naming = 1;
                total += 1;
                result.set_custom_name(Some(&Component::text(typed)));
            }
        }
        None => {
            if input.custom_name().is_some() {
                naming = 1;
                total += 1;
                result.set_custom_name(None);
            }
        }
    }
    base_cost += i64::from(total);
    let mut cost = if total >= 1 {
        i32::try_from(base_cost.clamp(0, i64::from(i32::MAX))).unwrap_or(i32::MAX)
    } else {
        0
    };
    let mut only_renaming = false;
    let mut keep = total >= 1;
    if naming == total && naming > 0 {
        if cost >= MAXIMUM_COST {
            cost = MAXIMUM_COST - 1;
        }
        only_renaming = true;
    }
    if cost >= MAXIMUM_COST && !infinite {
        keep = false;
    }
    if !keep {
        return Outcome {
            result: None,
            cost,
            repair_items,
            only_renaming,
        };
    }
    let mut repair = enchant::repair_cost(&result);
    let added = addition.as_ref().map_or(0, enchant::repair_cost);
    if repair < added {
        repair = added;
    }
    if naming != total || naming == 0 {
        repair = enchant::increased_repair_cost(repair);
    }
    enchant::set_repair_cost(&mut result, repair);
    enchant::set_levels(&mut result, &prior);
    Outcome {
        result: Some(result),
        cost,
        repair_items,
        only_renaming,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enchant::tests::stack;

    fn enchanted(name: &str, levels: &[(&str, u8)]) -> ItemStack {
        let mut item = stack(name);
        let levels: Vec<(String, u8)> = levels.iter().map(|(n, l)| ((*n).to_owned(), *l)).collect();
        enchant::set_levels(&mut item, &levels);
        item
    }

    fn damaged(name: &str, damage: i32) -> ItemStack {
        let mut item = stack(name);
        set_damage_value(&mut item, damage);
        item
    }

    #[test]
    fn nothing_in_the_input_gives_no_result() {
        assert_eq!(compute(&None, &None, None, false), Outcome::unavailable());
    }

    #[test]
    fn a_lone_item_without_a_rename_has_no_result_and_zero_cost() {
        let outcome = compute(&Some(stack("minecraft:diamond_sword")), &None, None, false);
        assert_eq!(outcome.result, None);
        assert_eq!(outcome.cost, 0);
    }

    #[test]
    fn renaming_costs_one_level_and_keeps_the_work_penalty() {
        let outcome = compute(
            &Some(stack("minecraft:diamond_sword")),
            &None,
            Some("Excalibur"),
            false,
        );
        let result = outcome.result.unwrap();
        assert_eq!(outcome.cost, 1);
        assert!(outcome.only_renaming);
        assert_eq!(result.custom_name().unwrap().plain_text(), "Excalibur");
        assert_eq!(enchant::repair_cost(&result), 0);
    }

    #[test]
    fn clearing_a_custom_name_costs_one_level() {
        let mut named = stack("minecraft:diamond_sword");
        named.set_custom_name(Some(&Component::text("Old")));
        let outcome = compute(&Some(named), &None, None, false);
        assert_eq!(outcome.cost, 1);
        assert!(outcome.result.unwrap().custom_name().is_none());
    }

    #[test]
    fn a_blank_name_is_no_rename() {
        let outcome = compute(&Some(stack("minecraft:stick")), &None, Some("   "), false);
        assert_eq!(outcome.result, None);
    }

    #[test]
    fn repairing_with_a_material_restores_a_quarter_per_item() {
        let sword = damaged("minecraft:diamond_sword", 1200);
        let mut diamonds = stack("minecraft:diamond");
        diamonds.count = std::num::NonZeroU8::new(3).unwrap();
        let outcome = compute(&Some(sword), &Some(diamonds), None, false);
        let result = outcome.result.unwrap();
        assert_eq!(damage_value(&result), 1200 - 3 * (1561 / 4));
        assert_eq!(outcome.cost, 3);
        assert_eq!(outcome.repair_items, 3);
        assert_eq!(enchant::repair_cost(&result), 1);
    }

    #[test]
    fn repairing_stops_when_the_item_is_whole() {
        let sword = damaged("minecraft:diamond_sword", 100);
        let mut diamonds = stack("minecraft:diamond");
        diamonds.count = std::num::NonZeroU8::new(5).unwrap();
        let outcome = compute(&Some(sword), &Some(diamonds), None, false);
        assert_eq!(outcome.repair_items, 1);
        assert_eq!(damage_value(&outcome.result.unwrap()), 0);
    }

    #[test]
    fn an_undamaged_item_cannot_be_repaired_with_its_material() {
        let outcome = compute(
            &Some(stack("minecraft:diamond_sword")),
            &Some(stack("minecraft:diamond")),
            None,
            false,
        );
        assert_eq!(outcome, Outcome::unavailable());
    }

    #[test]
    fn merging_two_damaged_tools_adds_the_remaining_durability_and_twelve_percent() {
        let first = damaged("minecraft:diamond_pickaxe", 1000);
        let second = damaged("minecraft:diamond_pickaxe", 1000);
        let outcome = compute(&Some(first), &Some(second), None, false);
        let result = outcome.result.unwrap();
        let max = 1561;
        let remaining = (max - 1000) * 2 + max * 12 / 100;
        assert_eq!(damage_value(&result), max - remaining);
        assert_eq!(outcome.cost, 2);
    }

    #[test]
    fn a_book_adds_its_enchantments_at_half_the_fee() {
        let sword = stack("minecraft:diamond_sword");
        let book = enchanted("minecraft:enchanted_book", &[("minecraft:sharpness", 3)]);
        let outcome = compute(&Some(sword), &Some(book), None, false);
        let result = outcome.result.unwrap();
        assert_eq!(
            enchant::levels(&result),
            vec![("minecraft:sharpness".to_owned(), 3)]
        );
        assert_eq!(outcome.cost, 3);
        assert_eq!(enchant::repair_cost(&result), 1);
    }

    #[test]
    fn equal_levels_combine_into_the_next_one_up_to_the_maximum() {
        let sword = enchanted("minecraft:diamond_sword", &[("minecraft:sharpness", 5)]);
        let book = enchanted("minecraft:enchanted_book", &[("minecraft:sharpness", 5)]);
        let outcome = compute(&Some(sword), &Some(book), None, false);
        assert_eq!(
            enchant::levels(&outcome.result.unwrap()),
            vec![("minecraft:sharpness".to_owned(), 5)]
        );
    }

    #[test]
    fn exclusive_enchantments_are_dropped_and_cost_a_level() {
        let sword = enchanted("minecraft:diamond_sword", &[("minecraft:sharpness", 2)]);
        let book = enchanted(
            "minecraft:enchanted_book",
            &[("minecraft:smite", 2), ("minecraft:unbreaking", 1)],
        );
        let outcome = compute(&Some(sword), &Some(book), None, false);
        let levels = enchant::levels(&outcome.result.unwrap());
        assert!(levels.contains(&("minecraft:unbreaking".to_owned(), 1)));
        assert!(!levels.iter().any(|(name, _)| name == "minecraft:smite"));
        assert_eq!(outcome.cost, 1 + 1);
    }

    #[test]
    fn only_incompatible_enchantments_give_no_result() {
        let sword = enchanted("minecraft:diamond_sword", &[("minecraft:sharpness", 2)]);
        let book = enchanted("minecraft:enchanted_book", &[("minecraft:smite", 2)]);
        assert_eq!(
            compute(&Some(sword), &Some(book), None, false),
            Outcome::unavailable()
        );
    }

    #[test]
    fn an_enchantment_the_item_cannot_take_is_refused_unless_infinite() {
        let sword = stack("minecraft:diamond_sword");
        let book = enchanted("minecraft:enchanted_book", &[("minecraft:efficiency", 1)]);
        assert_eq!(
            compute(&Some(sword.clone()), &Some(book.clone()), None, false),
            Outcome::unavailable()
        );
        assert!(
            compute(&Some(sword), &Some(book), None, true)
                .result
                .is_some()
        );
    }

    #[test]
    fn a_total_of_forty_levels_is_too_expensive_unless_infinite() {
        let sword = enchanted("minecraft:diamond_sword", &[("minecraft:sharpness", 4)]);
        let mut penalised = sword;
        enchant::set_repair_cost(&mut penalised, 63);
        let book = enchanted("minecraft:enchanted_book", &[("minecraft:sharpness", 4)]);
        let blocked = compute(&Some(penalised.clone()), &Some(book.clone()), None, false);
        assert_eq!(blocked.result, None);
        assert!(blocked.cost >= MAXIMUM_COST);
        let allowed = compute(&Some(penalised), &Some(book), None, true);
        assert!(allowed.result.is_some());
    }

    #[test]
    fn renaming_alone_never_reaches_the_maximum() {
        let mut worn = stack("minecraft:diamond_sword");
        enchant::set_repair_cost(&mut worn, 127);
        let outcome = compute(&Some(worn), &None, Some("x"), false);
        assert_eq!(outcome.cost, MAXIMUM_COST - 1);
        assert!(outcome.result.is_some());
    }

    #[test]
    fn two_different_items_do_not_merge() {
        assert_eq!(
            compute(
                &Some(stack("minecraft:diamond_sword")),
                &Some(stack("minecraft:stick")),
                None,
                false
            ),
            Outcome::unavailable()
        );
    }

    #[test]
    fn names_longer_than_fifty_characters_are_ignored() {
        assert!(valid_name(&"a".repeat(51)).is_none());
        assert!(valid_name(&"a".repeat(50)).is_some());
    }
}
