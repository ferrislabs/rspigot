use std::sync::OnceLock;

use std::num::NonZeroU8;

use rspigot_items::{Craft, ItemId, ItemKind, ItemSet};

use crate::item::ItemStack;
use crate::plugin_recipes::{self, Body};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cut {
    pub id: &'static str,
    pub ingredient: ItemSet,
    pub result: ItemKind,
    pub count: u8,
}

pub fn recipes() -> &'static [Cut] {
    static CUTS: OnceLock<Vec<Cut>> = OnceLock::new();
    CUTS.get_or_init(|| {
        let mut cuts: Vec<Cut> = ItemKind::ALL
            .into_iter()
            .flat_map(|owner| {
                owner.crafts().iter().filter_map(move |craft| match *craft {
                    Craft::Stonecutting {
                        id,
                        ingredient,
                        count,
                    } => Some(Cut {
                        id,
                        ingredient,
                        result: owner,
                        count,
                    }),
                    _ => None,
                })
            })
            .collect();
        cuts.sort_by(|left, right| left.id.cmp(right.id));
        cuts
    })
}

pub fn recipes_for(item: ItemId) -> Vec<&'static Cut> {
    recipes()
        .iter()
        .filter(|cut| cut.ingredient.contains(item))
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Offer {
    pub id: &'static str,
    pub result: ItemStack,
}

pub fn offers_for(item: ItemId) -> Vec<Offer> {
    let vanilla = recipes_for(item).into_iter().filter_map(|cut| {
        Some(Offer {
            id: cut.id,
            result: ItemStack::new(cut.result.id(), NonZeroU8::new(cut.count)?),
        })
    });
    let plugin = plugin_recipes::with(|recipes| {
        recipes
            .iter()
            .filter(|recipe| {
                matches!(&recipe.body, Body::Stonecutting { ingredient } if ingredient.contains(item))
            })
            .map(|recipe| Offer {
                id: recipe.id,
                result: recipe.result.clone(),
            })
            .collect::<Vec<_>>()
    });
    vanilla.chain(plugin).collect()
}

pub fn accepts(item: ItemId) -> bool {
    recipes().iter().any(|cut| cut.ingredient.contains(item))
        || plugin_recipes::with(|recipes| {
            recipes.iter().any(|recipe| {
                matches!(&recipe.body, Body::Stonecutting { ingredient } if ingredient.contains(item))
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(item: ItemKind) -> Vec<&'static str> {
        recipes_for(item.id()).iter().map(|cut| cut.id).collect()
    }

    #[test]
    fn every_recipe_of_the_datapack_is_listed_once() {
        assert_eq!(recipes().len(), 351);
        let mut ids: Vec<_> = recipes().iter().map(|cut| cut.id).collect();
        ids.dedup();
        assert_eq!(ids.len(), 351);
    }

    #[test]
    fn the_list_follows_the_recipe_name_order_of_the_game() {
        assert_eq!(
            names(ItemKind::Stone),
            [
                "minecraft:chiseled_stone_bricks_from_stone_stonecutting",
                "minecraft:cobblestone_from_stone_stonecutting",
                "minecraft:cobblestone_slab_from_stone_stonecutting",
                "minecraft:cobblestone_stairs_from_stone_stonecutting",
                "minecraft:cobblestone_wall_from_stone_stonecutting",
                "minecraft:stone_brick_slab_from_stone_stonecutting",
                "minecraft:stone_brick_stairs_from_stone_stonecutting",
                "minecraft:stone_brick_wall_from_stone_stonecutting",
                "minecraft:stone_bricks_from_stone_stonecutting",
                "minecraft:stone_slab_from_stone_stonecutting",
                "minecraft:stone_stairs_from_stone_stonecutting",
            ]
        );
    }

    #[test]
    fn a_recipe_makes_the_count_of_its_result() {
        let slab = recipes_for(ItemKind::Stone.id())
            .into_iter()
            .find(|cut| cut.result == ItemKind::StoneSlab)
            .unwrap();
        assert_eq!(slab.count, 2);
        assert!(accepts(ItemKind::Stone.id()));
        assert!(!accepts(ItemKind::Dirt.id()));
        assert!(recipes_for(ItemKind::Dirt.id()).is_empty());
    }

    #[test]
    fn plugin_cuts_follow_the_vanilla_ones_and_vanish_with_the_recipe() {
        let (id, display_id) = plugin_recipes::intern_id("t:cut");
        let mut set = plugin_recipes::PluginRecipes::default();
        set.add(plugin_recipes::PluginRecipe {
            id,
            display_id,
            owner: "test".to_owned(),
            body: Body::Stonecutting {
                ingredient: plugin_recipes::intern_items(&[ItemKind::Stone.id()]),
            },
            result: ItemStack::new(ItemKind::Diamond.id(), NonZeroU8::new(3).unwrap()),
            group: String::new(),
            category: plugin_recipes::Category::Misc,
            override_vanilla: false,
            auto_discover: false,
            exact: Vec::new(),
        });
        plugin_recipes::activate(&std::sync::Arc::new(set));
        let offers = offers_for(ItemKind::Stone.id());
        let last = offers.last().unwrap();
        assert_eq!(last.id, "t:cut");
        assert_eq!(last.result.count.get(), 3);
        assert_eq!(offers.len(), names(ItemKind::Stone).len() + 1);
        assert!(offers_for(ItemKind::Dirt.id()).is_empty());
        plugin_recipes::activate(&std::sync::Arc::default());
        assert_eq!(
            offers_for(ItemKind::Stone.id()).len(),
            names(ItemKind::Stone).len()
        );
    }
}
