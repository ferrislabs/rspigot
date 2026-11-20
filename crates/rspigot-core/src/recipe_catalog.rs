use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;

use rspigot_data::recipe_book;
use rspigot_items::{CookingKind, Craft, ItemKind, ItemSet};

use rspigot_domain::plugin_recipes::{self, Body, Category, PluginRecipe};

use crate::events::{BookDisplay, BookStack};
use crate::plugins::RecipeKind;

#[derive(Debug, Clone)]
pub(crate) enum Shape {
    Shaped {
        width: u8,
        height: u8,
        cells: Vec<Option<ItemSet>>,
    },
    Shapeless(Vec<ItemSet>),
    Cooking {
        kind: CookingKind,
        ingredient: ItemSet,
        duration: u16,
        experience: f32,
    },
    Other,
}

#[derive(Debug, Clone)]
pub(crate) struct CatalogRecipe {
    pub(crate) id: &'static str,
    pub(crate) display_id: i32,
    pub(crate) result: BookStack,
    pub(crate) category: i32,
    pub(crate) group: i32,
    pub(crate) shape: Shape,
}

#[derive(Debug, Clone, Copy)]
struct VanillaRecipe {
    id: &'static str,
    display_id: i32,
    owner: ItemKind,
    craft: &'static Craft,
    category: i32,
    group: i32,
}

impl VanillaRecipe {
    fn recipe(&self) -> CatalogRecipe {
        let (shape, count) = match *self.craft {
            Craft::Shaped {
                width,
                height,
                cells,
                product,
                ..
            } => (
                Shape::Shaped {
                    width,
                    height,
                    cells: cells.to_vec(),
                },
                product.count,
            ),
            Craft::Shapeless {
                ingredients,
                product,
                ..
            } => (Shape::Shapeless(ingredients.to_vec()), product.count),
            Craft::Cooking {
                kind,
                ingredient,
                experience,
                cooking_time,
                ..
            } => (
                Shape::Cooking {
                    kind,
                    ingredient,
                    duration: cooking_time,
                    experience,
                },
                1,
            ),
            _ => (Shape::Other, 1),
        };
        CatalogRecipe {
            id: self.id,
            display_id: self.display_id,
            result: BookStack {
                item: self.owner.id(),
                count,
            },
            category: self.category,
            group: self.group,
            shape,
        }
    }
}

const PLUGIN_GROUP_BASE: i32 = 100_000;

pub(crate) fn plugin_group(group: &str) -> i32 {
    if group.is_empty() {
        return 0;
    }
    let hash = group.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    PLUGIN_GROUP_BASE + i32::try_from(hash % 1_000_000).unwrap_or(0)
}

fn plugin_category(recipe: &PluginRecipe) -> i32 {
    let building = recipe.category == Category::Building;
    match &recipe.body {
        Body::Shaped { .. } | Body::Shapeless { .. } => match recipe.category {
            Category::Building => 0,
            Category::Redstone => 1,
            Category::Equipment => 2,
            Category::Misc | Category::Food => 3,
        },
        Body::Cooking { kind, .. } => match kind {
            CookingKind::Smelting if recipe.category == Category::Food => 4,
            CookingKind::Smelting if building => 5,
            CookingKind::Smelting => 6,
            CookingKind::Blasting if building => 7,
            CookingKind::Blasting => 8,
            CookingKind::Smoking => 9,
            CookingKind::Campfire => 6,
        },
        Body::Stonecutting { .. } | Body::Smithing { .. } => 3,
    }
}

impl CatalogRecipe {
    pub(crate) fn of_plugin(recipe: &PluginRecipe) -> Self {
        let shape = match &recipe.body {
            Body::Shaped {
                width,
                height,
                cells,
                ..
            } => Shape::Shaped {
                width: u8::try_from(*width).unwrap_or(u8::MAX),
                height: u8::try_from(*height).unwrap_or(u8::MAX),
                cells: cells.clone(),
            },
            Body::Shapeless { ingredients } => Shape::Shapeless(ingredients.clone()),
            Body::Cooking {
                kind,
                ingredient,
                experience,
                time,
            } => Shape::Cooking {
                kind: *kind,
                ingredient: *ingredient,
                duration: *time,
                experience: *experience,
            },
            Body::Stonecutting { .. } | Body::Smithing { .. } => Shape::Other,
        };
        Self {
            id: recipe.id,
            display_id: recipe.display_id,
            result: BookStack {
                item: recipe.result.item,
                count: recipe.result.count.get(),
            },
            category: plugin_category(recipe),
            group: plugin_group(&recipe.group),
            shape,
        }
    }

    pub(crate) fn display(&self) -> Option<BookDisplay> {
        let result = self.result;
        Some(match &self.shape {
            Shape::Shaped {
                width,
                height,
                cells,
            } => BookDisplay::Shaped {
                width: *width,
                height: *height,
                cells: cells.clone(),
                result,
                station: ItemKind::CraftingTable.id(),
            },
            Shape::Shapeless(ingredients) => BookDisplay::Shapeless {
                ingredients: ingredients.clone(),
                result,
                station: ItemKind::CraftingTable.id(),
            },
            Shape::Cooking {
                kind,
                ingredient,
                duration,
                experience,
            } => BookDisplay::Furnace {
                ingredient: *ingredient,
                result,
                station: station_of(*kind)?.id(),
                duration: *duration,
                experience: *experience,
            },
            Shape::Other => return None,
        })
    }

    pub(crate) fn requirements(&self) -> Vec<ItemSet> {
        match &self.shape {
            Shape::Shaped { cells, .. } => cells.iter().flatten().copied().collect(),
            Shape::Shapeless(ingredients) => ingredients.clone(),
            Shape::Cooking { ingredient, .. } => vec![*ingredient],
            Shape::Other => Vec::new(),
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

fn catalog() -> &'static [VanillaRecipe] {
    static CATALOG: OnceLock<Vec<VanillaRecipe>> = OnceLock::new();
    CATALOG.get_or_init(build)
}

fn build() -> Vec<VanillaRecipe> {
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
            Some(VanillaRecipe {
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

pub(crate) fn by_id(id: &str) -> Option<CatalogRecipe> {
    let catalog = catalog();
    catalog
        .binary_search_by(|recipe| recipe.id.cmp(id))
        .ok()
        .map(|index| catalog[index].recipe())
        .or_else(|| plugin_recipes::with(|recipes| recipes.get(id).map(CatalogRecipe::of_plugin)))
}

pub(crate) fn by_display(display_id: i32) -> Option<CatalogRecipe> {
    let catalog = catalog();
    catalog
        .binary_search_by(|recipe| recipe.display_id.cmp(&display_id))
        .ok()
        .map(|index| catalog[index].recipe())
        .or_else(|| {
            plugin_recipes::with(|recipes| {
                recipes
                    .iter()
                    .find(|recipe| recipe.display_id == display_id)
                    .map(CatalogRecipe::of_plugin)
            })
        })
}

pub(crate) fn vanilla_kinds() -> &'static [(&'static str, RecipeKind)] {
    static KINDS: OnceLock<Vec<(&'static str, RecipeKind)>> = OnceLock::new();
    KINDS.get_or_init(|| {
        let mut kinds: Vec<(&'static str, RecipeKind)> = ItemKind::ALL
            .into_iter()
            .flat_map(|owner| owner.crafts().iter())
            .map(|craft| {
                let kind = match craft {
                    Craft::Shaped { .. } => RecipeKind::Shaped,
                    Craft::Shapeless { .. } => RecipeKind::Shapeless,
                    Craft::Cooking { kind, .. } => match kind {
                        CookingKind::Smelting => RecipeKind::Furnace,
                        CookingKind::Blasting => RecipeKind::BlastFurnace,
                        CookingKind::Smoking => RecipeKind::Smoker,
                        CookingKind::Campfire => RecipeKind::Campfire,
                    },
                    Craft::Stonecutting { .. } => RecipeKind::Stonecutting,
                    Craft::SmithingTransform { .. } | Craft::SmithingTrim { .. } => {
                        RecipeKind::Smithing
                    }
                    _ => RecipeKind::Special,
                };
                (craft.id(), kind)
            })
            .collect();
        kinds.sort_unstable_by_key(|(id, _)| *id);
        kinds.dedup_by_key(|(id, _)| *id);
        kinds
    })
}

pub(crate) fn is_vanilla(id: &str) -> bool {
    vanilla_kinds()
        .binary_search_by(|(known, _)| (*known).cmp(id))
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recipe_is_found_by_its_id_and_by_its_display_id() {
        let planks = by_id("minecraft:acacia_planks").expect("a recipe");
        assert_eq!(by_display(planks.display_id).map(|r| r.id), Some(planks.id));
        assert_eq!(planks.result.item, ItemKind::AcaciaPlanks.id());
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

    #[test]
    fn every_catalog_recipe_satisfies_the_wire_invariants() {
        let mut bad = Vec::new();
        for recipe in catalog().iter().map(VanillaRecipe::recipe) {
            for set in recipe.requirements() {
                if set.items().is_empty() {
                    bad.push(format!("{} empty set tag={:?}", recipe.id, set.tag()));
                }
                if let Some(tag) = set.tag()
                    && (tag.starts_with('#') || !tag.contains(':'))
                {
                    bad.push(format!("{} tag {tag}", recipe.id));
                }
            }
            if let Some(BookDisplay::Shaped {
                width,
                height,
                cells,
                ..
            }) = recipe.display()
                && cells.len() != usize::from(width) * usize::from(height)
            {
                bad.push(format!(
                    "{} shaped {width}x{height} with {} cells",
                    recipe.id,
                    cells.len()
                ));
            }
            let owner = i32::from(recipe.result.item.0);
            if owner <= 0 || owner >= 1658 {
                bad.push(format!("{} owner id {owner}", recipe.id));
            }
            for set in recipe.requirements() {
                for item in set.items() {
                    if item.0 == 0 || i32::from(item.0) >= 1658 {
                        bad.push(format!("{} item id {}", recipe.id, item.0));
                    }
                }
            }
            if recipe.result.count == 0 {
                bad.push(format!("{} zero count", recipe.id));
            }
            if recipe.category < 0 || recipe.category > 12 {
                bad.push(format!("{} category {}", recipe.id, recipe.category));
            }
        }
        assert!(bad.is_empty(), "{bad:#?}");
    }
}
