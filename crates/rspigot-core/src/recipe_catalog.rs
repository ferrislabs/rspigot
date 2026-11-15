use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;

use rspigot_data::recipe_book;
use rspigot_items::{CookingKind, Craft, ItemKind, ItemSet};

use crate::events::{BookDisplay, BookStack};

#[derive(Debug, Clone, Copy)]
pub(crate) struct CatalogRecipe {
    pub(crate) id: &'static str,
    pub(crate) display_id: i32,
    pub(crate) owner: ItemKind,
    pub(crate) craft: &'static Craft,
    pub(crate) category: i32,
    pub(crate) group: i32,
}

impl CatalogRecipe {
    pub(crate) fn display(&self) -> Option<BookDisplay> {
        let result = BookStack {
            item: self.owner.id(),
            count: self.count(),
        };
        Some(match *self.craft {
            Craft::Shaped {
                width,
                height,
                cells,
                ..
            } => BookDisplay::Shaped {
                width,
                height,
                cells: cells.to_vec(),
                result,
                station: ItemKind::CraftingTable.id(),
            },
            Craft::Shapeless { ingredients, .. } => BookDisplay::Shapeless {
                ingredients: ingredients.to_vec(),
                result,
                station: ItemKind::CraftingTable.id(),
            },
            Craft::Cooking {
                kind,
                ingredient,
                experience,
                cooking_time,
                ..
            } => BookDisplay::Furnace {
                ingredient,
                result,
                station: station_of(kind)?.id(),
                duration: cooking_time,
                experience,
            },
            _ => return None,
        })
    }

    pub(crate) fn requirements(&self) -> Vec<ItemSet> {
        match *self.craft {
            Craft::Shaped { cells, .. } => cells.iter().flatten().copied().collect(),
            Craft::Shapeless { ingredients, .. } => ingredients.to_vec(),
            Craft::Cooking { ingredient, .. } => vec![ingredient],
            _ => Vec::new(),
        }
    }

    fn count(&self) -> u8 {
        match *self.craft {
            Craft::Shaped { product, .. } | Craft::Shapeless { product, .. } => product.count,
            _ => 1,
        }
    }
}

fn station_of(kind: CookingKind) -> Option<ItemKind> {
    match kind {
        CookingKind::Smelting => Some(ItemKind::Furnace),
        CookingKind::Blasting => Some(ItemKind::BlastFurnace),
        CookingKind::Smoking => Some(ItemKind::Smoker),
        CookingKind::Campfire => None,
    }
}

fn catalog() -> &'static [CatalogRecipe] {
    static CATALOG: OnceLock<Vec<CatalogRecipe>> = OnceLock::new();
    CATALOG.get_or_init(build)
}

fn build() -> Vec<CatalogRecipe> {
    let mut crafts: HashMap<&'static str, (ItemKind, &'static Craft)> = HashMap::new();
    for owner in ItemKind::ALL {
        for craft in owner.crafts() {
            if matches!(
                craft,
                Craft::Shaped { .. } | Craft::Shapeless { .. } | Craft::Cooking { .. }
            ) {
                crafts.insert(craft.id(), (owner, craft));
            }
        }
    }
    let groups: BTreeSet<&str> = recipe_book::all()
        .iter()
        .map(|recipe| recipe.group)
        .filter(|group| !group.is_empty())
        .collect();
    recipe_book::all()
        .iter()
        .enumerate()
        .filter_map(|(index, recipe)| {
            let (owner, craft) = *crafts.get(recipe.id)?;
            let group = groups
                .iter()
                .position(|candidate| *candidate == recipe.group)
                .map_or(0, |position| position as i32 + 1);
            Some(CatalogRecipe {
                id: recipe.id,
                display_id: index as i32,
                owner,
                craft,
                category: i32::from(recipe.category),
                group,
            })
        })
        .collect()
}

pub(crate) fn by_id(id: &str) -> Option<&'static CatalogRecipe> {
    let catalog = catalog();
    catalog
        .binary_search_by(|recipe| recipe.id.cmp(id))
        .ok()
        .map(|index| &catalog[index])
}

pub(crate) fn by_display(display_id: i32) -> Option<&'static CatalogRecipe> {
    let catalog = catalog();
    catalog
        .binary_search_by(|recipe| recipe.display_id.cmp(&display_id))
        .ok()
        .map(|index| &catalog[index])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recipe_is_found_by_its_id_and_by_its_display_id() {
        let planks = by_id("minecraft:acacia_planks").expect("a recipe");
        assert_eq!(by_display(planks.display_id).map(|r| r.id), Some(planks.id));
        assert_eq!(planks.owner, ItemKind::AcaciaPlanks);
        assert!(planks.group > 0);
    }

    #[test]
    fn crafting_and_cooking_recipes_have_a_display_and_requirements() {
        let furnace = by_id("minecraft:iron_ingot_from_smelting_iron_ore").expect("a recipe");
        assert!(matches!(
            furnace.display(),
            Some(BookDisplay::Furnace { .. })
        ));
        assert_eq!(furnace.requirements().len(), 1);
        let stick = by_id("minecraft:stick").expect("a recipe");
        assert!(matches!(
            stick.display(),
            Some(BookDisplay::Shaped {
                width: 1,
                height: 2,
                ..
            })
        ));
    }

    #[test]
    fn campfire_recipes_are_not_in_the_catalog() {
        assert!(by_id("minecraft:cooked_beef_from_campfire_cooking").is_none());
    }
}
