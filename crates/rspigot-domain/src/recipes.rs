use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use rspigot_items::{CookingKind, Craft, ItemKind, ItemSet};

pub const SMITHING_BASE: &str = "minecraft:smithing_base";
pub const SMITHING_TEMPLATE: &str = "minecraft:smithing_template";
pub const SMITHING_ADDITION: &str = "minecraft:smithing_addition";
pub const FURNACE_INPUT: &str = "minecraft:furnace_input";
pub const BLAST_FURNACE_INPUT: &str = "minecraft:blast_furnace_input";
pub const SMOKER_INPUT: &str = "minecraft:smoker_input";
pub const CAMPFIRE_INPUT: &str = "minecraft:campfire_input";
pub const BREWING_INPUT: &str = "minecraft:brewing_input";
pub const BREWING_REAGENT: &str = "minecraft:brewing_reagent";

pub(crate) fn cooking_key(kind: CookingKind) -> &'static str {
    match kind {
        CookingKind::Smelting => FURNACE_INPUT,
        CookingKind::Blasting => BLAST_FURNACE_INPUT,
        CookingKind::Smoking => SMOKER_INPUT,
        CookingKind::Campfire => CAMPFIRE_INPUT,
    }
}

pub fn property_sets() -> &'static [(&'static str, Vec<u16>)] {
    static SETS: OnceLock<Vec<(&'static str, Vec<u16>)>> = OnceLock::new();
    SETS.get_or_init(build)
}

pub fn contains(key: &str, item: u16) -> bool {
    property_sets()
        .iter()
        .find(|(candidate, _)| *candidate == key)
        .is_some_and(|(_, items)| items.binary_search(&item).is_ok())
        || crate::plugin_recipes::with(|plugin| {
            plugin.property_items(key).binary_search(&item).is_ok()
        })
}

pub fn merged_property_sets(
    plugin: &crate::plugin_recipes::PluginRecipes,
) -> Vec<(&'static str, Vec<u16>)> {
    property_sets()
        .iter()
        .map(|(key, items)| {
            let mut merged = items.clone();
            merged.extend(plugin.property_items(key));
            merged.sort_unstable();
            merged.dedup();
            (*key, merged)
        })
        .collect()
}

fn build() -> Vec<(&'static str, Vec<u16>)> {
    let mut sets: BTreeMap<&'static str, BTreeSet<u16>> = [
        SMITHING_BASE,
        SMITHING_TEMPLATE,
        SMITHING_ADDITION,
        FURNACE_INPUT,
        BLAST_FURNACE_INPUT,
        SMOKER_INPUT,
        CAMPFIRE_INPUT,
        BREWING_INPUT,
        BREWING_REAGENT,
    ]
    .into_iter()
    .map(|key| (key, BTreeSet::new()))
    .collect();
    let mut add = |key: &'static str, set: ItemSet| {
        sets.entry(key)
            .or_default()
            .extend(set.items().iter().map(|item| item.0));
    };
    for owner in ItemKind::ALL {
        for craft in owner.crafts() {
            match *craft {
                Craft::Cooking {
                    kind, ingredient, ..
                } => add(cooking_key(kind), ingredient),
                Craft::SmithingTransform {
                    template,
                    base,
                    addition,
                    ..
                }
                | Craft::SmithingTrim {
                    template,
                    base,
                    addition,
                    ..
                } => {
                    add(SMITHING_TEMPLATE, template);
                    add(SMITHING_BASE, base);
                    add(SMITHING_ADDITION, addition);
                }
                _ => {}
            }
        }
    }
    sets.into_iter()
        .map(|(key, items)| (key, items.into_iter().collect()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(key: &str) -> Vec<u16> {
        property_sets()
            .iter()
            .find(|(candidate, _)| *candidate == key)
            .map(|(_, items)| items.clone())
            .unwrap()
    }

    #[test]
    fn every_set_the_client_asks_for_is_present() {
        let keys: Vec<_> = property_sets().iter().map(|(key, _)| *key).collect();
        assert_eq!(keys.len(), 9);
        assert!(keys.contains(&BREWING_INPUT) && keys.contains(&BREWING_REAGENT));
        assert!(contains(SMITHING_ADDITION, ItemKind::Diamond.id().0));
        assert!(!contains(SMITHING_ADDITION, ItemKind::Stick.id().0));
        assert!(!contains("minecraft:nope", ItemKind::Diamond.id().0));
    }

    #[test]
    fn the_sets_hold_what_the_recipes_take() {
        assert_eq!(set(FURNACE_INPUT).len(), 163);
        assert_eq!(set(BLAST_FURNACE_INPUT).len(), 62);
        assert_eq!(set(SMOKER_INPUT).len(), 9);
        assert_eq!(set(CAMPFIRE_INPUT).len(), 9);
        assert_eq!(set(SMITHING_TEMPLATE).len(), 19);
        assert_eq!(set(SMITHING_BASE).len(), 37);
        assert_eq!(set(SMITHING_ADDITION).len(), 11);
        assert!(set(BREWING_INPUT).is_empty());
    }

    #[test]
    fn a_set_is_sorted_and_holds_the_expected_items() {
        let base = set(SMITHING_BASE);
        assert!(base.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(base.contains(&ItemKind::DiamondSword.id().0));
        assert!(base.contains(&ItemKind::LeatherHelmet.id().0));
        assert!(set(SMITHING_ADDITION).contains(&ItemKind::NetheriteIngot.id().0));
        assert!(
            set(SMITHING_TEMPLATE).contains(&ItemKind::NetheriteUpgradeSmithingTemplate.id().0)
        );
        assert!(set(FURNACE_INPUT).contains(&ItemKind::RawIron.id().0));
        assert!(!set(SMOKER_INPUT).contains(&ItemKind::RawIron.id().0));
    }
}
