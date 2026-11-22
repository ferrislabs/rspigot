use rspigot_data::enchantments::{self, EnchantmentDef};
use rspigot_data::tags;
use rspigot_items::ItemKind;

use crate::enchant;
use crate::item::{ENCHANTMENTS, ItemStack};
use crate::random::{LegacyRandom, RandomSource};

pub const ITEM: usize = 0;
pub const LAPIS: usize = 1;
pub const OFFERS: usize = 3;
pub const DATA: usize = 10;
pub const MAX_BOOKSHELVES: i32 = 15;
const ITEM_REGISTRY: &str = "minecraft:item";
const BOOK_SHELF_RING: i32 = 2;
const MAX_ENCHANTING_CHANCE: i32 = 50;

pub const IN_ENCHANTING_TABLE: [&str; 36] = [
    "minecraft:protection",
    "minecraft:fire_protection",
    "minecraft:feather_falling",
    "minecraft:blast_protection",
    "minecraft:projectile_protection",
    "minecraft:respiration",
    "minecraft:aqua_affinity",
    "minecraft:thorns",
    "minecraft:depth_strider",
    "minecraft:sharpness",
    "minecraft:smite",
    "minecraft:bane_of_arthropods",
    "minecraft:knockback",
    "minecraft:fire_aspect",
    "minecraft:looting",
    "minecraft:sweeping_edge",
    "minecraft:efficiency",
    "minecraft:silk_touch",
    "minecraft:unbreaking",
    "minecraft:fortune",
    "minecraft:power",
    "minecraft:punch",
    "minecraft:flame",
    "minecraft:infinity",
    "minecraft:luck_of_the_sea",
    "minecraft:lure",
    "minecraft:loyalty",
    "minecraft:impaling",
    "minecraft:riptide",
    "minecraft:channeling",
    "minecraft:multishot",
    "minecraft:quick_charge",
    "minecraft:piercing",
    "minecraft:density",
    "minecraft:breach",
    "minecraft:lunge",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offers {
    pub costs: [i32; 3],
    pub clues: [i32; 3],
    pub levels: [i32; 3],
}

impl Offers {
    pub const NONE: Self = Self {
        costs: [0; 3],
        clues: [-1; 3],
        levels: [-1; 3],
    };
}

impl Default for Offers {
    fn default() -> Self {
        Self::NONE
    }
}

pub fn bookshelf_offsets() -> impl Iterator<Item = (i32, i32, i32)> {
    (-BOOK_SHELF_RING..=BOOK_SHELF_RING).flat_map(|x| {
        (0..=1).flat_map(move |y| {
            (-BOOK_SHELF_RING..=BOOK_SHELF_RING)
                .filter(move |z| x.abs() == BOOK_SHELF_RING || z.abs() == BOOK_SHELF_RING)
                .map(move |z| (x, y, z))
        })
    })
}

pub fn is_lapis(stack: &ItemStack) -> bool {
    stack.item.kind() == Some(ItemKind::LapisLazuli)
}

pub fn is_book(stack: &ItemStack) -> bool {
    stack.item.kind() == Some(ItemKind::Book)
}

fn enchantment_value(stack: &ItemStack) -> i32 {
    crate::stack_components::enchantable(stack)
        .map_or(0, |value| i32::try_from(value).unwrap_or(i32::MAX))
}

pub fn is_enchantable(stack: &ItemStack) -> bool {
    enchantment_value(stack) > 0 && enchant::levels_under(stack, ENCHANTMENTS).is_empty()
}

fn offer_cost(random: &mut LegacyRandom, slot: usize, bookshelves: i32, stack: &ItemStack) -> i32 {
    if enchantment_value(stack) <= 0 {
        return 0;
    }
    let bookshelves = bookshelves.min(MAX_BOOKSHELVES);
    let selected = random.next_int(8) + 1 + (bookshelves >> 1) + random.next_int(bookshelves + 1);
    match slot {
        0 => (selected / 3).max(1),
        1 => selected * 2 / 3 + 1,
        _ => selected.max(bookshelves * 2),
    }
}

fn in_tag(registry: &str, tag: &str, id: u32) -> bool {
    tags::tag(registry, tag.trim_start_matches('#'))
        .is_some_and(|ids| ids.binary_search(&id).is_ok())
}

fn is_primary(def: &EnchantmentDef, stack: &ItemStack) -> bool {
    match def.primary_items {
        Some(tag) => in_tag(ITEM_REGISTRY, tag, u32::from(stack.item.0)),
        None => def.supports(stack.item.0),
    }
}

fn weighted(
    random: &mut LegacyRandom,
    available: &[(&'static EnchantmentDef, u8)],
) -> Option<(&'static EnchantmentDef, u8)> {
    let total: i32 = available.iter().map(|(def, _)| i32::from(def.weight)).sum();
    if total <= 0 {
        return None;
    }
    let mut selection = random.next_int(total);
    for pick in available {
        selection -= i32::from(pick.0.weight);
        if selection < 0 {
            return Some(*pick);
        }
    }
    None
}

fn available(cost: i32, stack: &ItemStack) -> Vec<(&'static EnchantmentDef, u8)> {
    let book = is_book(stack);
    let mut found = Vec::new();
    for name in IN_ENCHANTING_TABLE {
        let Some(def) = enchantments::by_name(name) else {
            continue;
        };
        if !(book || is_primary(def, stack)) {
            continue;
        }
        for level in (1..=def.max_level).rev() {
            let level_value = i32::from(level);
            if cost >= def.min_cost.at(level_value) && cost <= def.max_cost.at(level_value) {
                found.push((def, level));
                break;
            }
        }
    }
    found
}

fn select(
    random: &mut LegacyRandom,
    stack: &ItemStack,
    mut cost: i32,
) -> Vec<(&'static EnchantmentDef, u8)> {
    let mut results = Vec::new();
    let value = enchantment_value(stack);
    if value <= 0 {
        return results;
    }
    cost += 1 + random.next_int(value / 4 + 1) + random.next_int(value / 4 + 1);
    let span = (random.next_float() + random.next_float() - 1.0) * 0.15;
    cost = ((cost as f32 + cost as f32 * span) + 0.5).floor().max(1.0) as i32;
    let mut pool = available(cost, stack);
    if pool.is_empty() {
        return results;
    }
    results.extend(weighted(random, &pool));
    while random.next_int(MAX_ENCHANTING_CHANCE) <= cost {
        if let Some((last, _)) = results.last().copied() {
            pool.retain(|(def, _)| last.compatible_with(def));
        }
        if pool.is_empty() {
            break;
        }
        results.extend(weighted(random, &pool));
        cost /= 2;
    }
    results
}

fn listed(
    random: &mut LegacyRandom,
    stack: &ItemStack,
    seed: i32,
    slot: usize,
    cost: i32,
) -> Vec<(&'static EnchantmentDef, u8)> {
    random.set_seed(i64::from(seed.wrapping_add(slot as i32)));
    let mut picks = select(random, stack, cost);
    if is_book(stack) && picks.len() > 1 {
        let removed = random.next_int(picks.len() as i32) as usize;
        picks.remove(removed);
    }
    picks
}

pub fn list(
    stack: &ItemStack,
    seed: i32,
    slot: usize,
    cost: i32,
) -> Vec<(&'static EnchantmentDef, u8)> {
    listed(&mut LegacyRandom::new(0), stack, seed, slot, cost)
}

pub fn offers(stack: &ItemStack, seed: i32, bookshelves: i32) -> Offers {
    let mut offers = Offers::NONE;
    if enchantment_value(stack) <= 0 {
        return offers;
    }
    let mut random = LegacyRandom::new(i64::from(seed));
    for slot in 0..3 {
        offers.costs[slot] = offer_cost(&mut random, slot, bookshelves, stack);
        if offers.costs[slot] < slot as i32 + 1 {
            offers.costs[slot] = 0;
        }
    }
    for slot in 0..3 {
        if offers.costs[slot] <= 0 {
            continue;
        }
        let picks = listed(&mut random, stack, seed, slot, offers.costs[slot]);
        if picks.is_empty() {
            continue;
        }
        let (def, level) = picks[random.next_int(picks.len() as i32) as usize];
        offers.clues[slot] = i32::try_from(def.id()).unwrap_or(-1);
        offers.levels[slot] = i32::from(level);
    }
    if !is_enchantable(stack) {
        return Offers::NONE;
    }
    offers
}

pub fn enchanted(stack: &ItemStack, picks: &[(String, u8)]) -> ItemStack {
    let mut result = stack.clone();
    if is_book(stack) {
        result.item = ItemKind::EnchantedBook.id();
    }
    let mut levels = enchant::levels(&result);
    for (name, level) in picks {
        match levels.iter_mut().find(|(known, _)| known == name) {
            Some(entry) => entry.1 = entry.1.max(*level),
            None => levels.push((name.clone(), *level)),
        }
    }
    enchant::set_levels(&mut result, &levels);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enchant::tests::stack;

    const ENCHANTMENT_REGISTRY: &str = "minecraft:enchantment";

    #[test]
    fn the_table_order_is_the_non_treasure_tag() {
        let mut ids: Vec<u32> = IN_ENCHANTING_TABLE
            .iter()
            .map(|name| enchantments::id(name).unwrap())
            .collect();
        ids.sort_unstable();
        assert_eq!(
            Some(ids.as_slice()),
            tags::tag(ENCHANTMENT_REGISTRY, "minecraft:in_enchanting_table")
        );
    }

    #[test]
    fn thirty_two_positions_surround_the_table_on_two_layers() {
        let offsets: Vec<_> = bookshelf_offsets().collect();
        assert_eq!(offsets.len(), 32);
        assert!(offsets.contains(&(-2, 0, -2)));
        assert!(offsets.contains(&(2, 1, 0)));
        assert!(!offsets.contains(&(1, 0, 1)));
        assert!(offsets.iter().all(|(_, y, _)| (0..=1).contains(y)));
    }

    #[test]
    fn an_empty_or_unenchantable_item_has_no_offers() {
        assert_eq!(offers(&stack("minecraft:stick"), 7, 15), Offers::NONE);
        let mut sword = stack("minecraft:diamond_sword");
        enchant::set_levels(&mut sword, &[("minecraft:sharpness".into(), 1)]);
        assert_eq!(offers(&sword, 7, 15), Offers::NONE);
    }

    #[test]
    fn offers_are_deterministic_for_a_seed_and_grow_with_bookshelves() {
        let sword = stack("minecraft:diamond_sword");
        assert_eq!(offers(&sword, 1234, 15), offers(&sword, 1234, 15));
        let none = offers(&sword, 1234, 0);
        assert!(none.costs[0] >= 1 && none.costs[0] <= 3);
        let full = offers(&sword, 1234, 15);
        assert!(full.costs[2] >= 30);
        assert!(full.costs[0] <= full.costs[2]);
    }

    #[test]
    fn a_bottom_offer_below_its_slot_number_is_dropped() {
        for seed in 0..64 {
            let offers = offers(&stack("minecraft:diamond_sword"), seed, 0);
            for slot in 0..3 {
                assert!(offers.costs[slot] == 0 || offers.costs[slot] > slot as i32);
            }
        }
    }

    #[test]
    fn each_offer_names_an_enchantment_the_item_supports() {
        let sword = stack("minecraft:diamond_sword");
        let table = offers(&sword, 99, 15);
        for slot in 0..3 {
            assert!(table.costs[slot] > 0);
            let def = &enchantments::all()[usize::try_from(table.clues[slot]).unwrap()];
            assert!(def.supports(sword.item.0), "{}", def.name);
            assert!(table.levels[slot] >= 1);
        }
    }

    #[test]
    fn the_list_for_an_offer_starts_with_the_clue() {
        let pickaxe = stack("minecraft:diamond_pickaxe");
        let table = offers(&pickaxe, 42, 15);
        for slot in 0..3 {
            let picks = list(&pickaxe, 42, slot, table.costs[slot]);
            assert!(!picks.is_empty());
            assert!(picks.iter().all(|(def, _)| def.supports(pickaxe.item.0)));
        }
    }

    #[test]
    fn a_book_never_gets_more_than_it_may() {
        let book = stack("minecraft:book");
        assert!(is_enchantable(&book));
        let table = offers(&book, 5, 15);
        assert!(table.costs[2] > 0);
        let picks = list(&book, 5, 2, table.costs[2]);
        assert!(!picks.is_empty());
    }

    #[test]
    fn enchanting_a_book_makes_an_enchanted_book_with_stored_levels() {
        let book = stack("minecraft:book");
        let made = enchanted(&book, &[("minecraft:sharpness".into(), 2)]);
        assert_eq!(made.item.kind(), Some(ItemKind::EnchantedBook));
        assert_eq!(
            enchant::levels(&made),
            vec![("minecraft:sharpness".to_owned(), 2)]
        );
    }

    #[test]
    fn enchanting_an_item_writes_the_enchantments_component() {
        let sword = stack("minecraft:diamond_sword");
        let made = enchanted(&sword, &[("minecraft:sharpness".into(), 3)]);
        assert_eq!(made.item, sword.item);
        assert_eq!(
            enchant::levels(&made),
            vec![("minecraft:sharpness".to_owned(), 3)]
        );
    }

    #[test]
    fn only_lapis_is_a_lapis() {
        assert!(is_lapis(&stack("minecraft:lapis_lazuli")));
        assert!(!is_lapis(&stack("minecraft:diamond")));
    }
}
