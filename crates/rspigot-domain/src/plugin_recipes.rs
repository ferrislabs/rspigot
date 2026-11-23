use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use rspigot_items::{CookingKind, ItemId, ItemSet};

use crate::item::ItemStack;

const FIRST_DISPLAY_ID: i32 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Building,
    Redstone,
    Equipment,
    Misc,
    Food,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    Shaped {
        width: usize,
        height: usize,
        mirror: bool,
        cells: Vec<Option<ItemSet>>,
    },
    Shapeless {
        ingredients: Vec<ItemSet>,
    },
    Cooking {
        kind: CookingKind,
        ingredient: ItemSet,
        experience: f32,
        time: u16,
    },
    Stonecutting {
        ingredient: ItemSet,
    },
    Smithing {
        template: ItemSet,
        base: ItemSet,
        addition: ItemSet,
    },
    SmithingTrim {
        template: ItemSet,
        base: ItemSet,
        addition: ItemSet,
        pattern: String,
    },
    Transmute {
        input: ItemSet,
        material: ItemSet,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PluginRecipe {
    pub exact: Vec<Option<ItemStack>>,
    pub id: &'static str,
    pub display_id: i32,
    pub owner: String,
    pub body: Body,
    pub result: ItemStack,
    pub group: String,
    pub category: Category,
    pub override_vanilla: bool,
    pub auto_discover: bool,
}

impl PluginRecipe {
    pub fn uses_crafting_grid(&self) -> bool {
        matches!(
            self.body,
            Body::Shaped { .. } | Body::Shapeless { .. } | Body::Transmute { .. }
        )
    }

    pub fn ingredients(&self) -> Vec<ItemSet> {
        match &self.body {
            Body::Shaped { cells, .. } => cells.iter().flatten().copied().collect(),
            Body::Shapeless { ingredients } => ingredients.clone(),
            Body::Cooking { ingredient, .. } | Body::Stonecutting { ingredient } => {
                vec![*ingredient]
            }
            Body::Smithing {
                template,
                base,
                addition,
            }
            | Body::SmithingTrim {
                template,
                base,
                addition,
                ..
            } => vec![*template, *base, *addition],
            Body::Transmute { input, material } => vec![*input, *material],
        }
    }

    pub fn accepts_exact(&self, index: usize, stack: &ItemStack) -> bool {
        exact_matches(self.exact.get(index).and_then(Option::as_ref), stack)
    }
}

pub fn exact_matches(wanted: Option<&ItemStack>, stack: &ItemStack) -> bool {
    wanted.is_none_or(|wanted| crate::item::same_item_same_components(wanted, stack))
}

#[derive(Debug, Clone, Default)]
pub struct PluginRecipes {
    recipes: Vec<Arc<PluginRecipe>>,
}

impl PluginRecipes {
    pub fn add(&mut self, recipe: PluginRecipe) {
        self.recipes.push(Arc::new(recipe));
    }

    pub fn remove(&mut self, id: &str) -> Option<Arc<PluginRecipe>> {
        let position = self.recipes.iter().position(|recipe| recipe.id == id)?;
        Some(self.recipes.remove(position))
    }

    pub fn remove_owner(&mut self, owner: &str) -> Vec<Arc<PluginRecipe>> {
        let (gone, kept) = std::mem::take(&mut self.recipes)
            .into_iter()
            .partition(|recipe| recipe.owner == owner);
        self.recipes = kept;
        gone
    }

    pub fn get(&self, id: &str) -> Option<&PluginRecipe> {
        self.recipes
            .iter()
            .find(|recipe| recipe.id == id)
            .map(Arc::as_ref)
    }

    pub fn contains(&self, id: &str) -> bool {
        self.get(id).is_some()
    }

    pub fn iter(&self) -> impl Iterator<Item = &PluginRecipe> {
        self.recipes.iter().map(Arc::as_ref)
    }

    pub fn len(&self) -> usize {
        self.recipes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.recipes.is_empty()
    }

    pub fn count_of(&self, owner: &str) -> usize {
        self.iter().filter(|recipe| recipe.owner == owner).count()
    }

    pub fn precedence(&self, overriding: bool) -> impl Iterator<Item = &PluginRecipe> {
        self.iter()
            .filter(move |recipe| recipe.override_vanilla == overriding)
    }

    pub fn property_items(&self, key: &str) -> Vec<u16> {
        let mut items: Vec<u16> = Vec::new();
        for recipe in self.iter() {
            let sets: Vec<ItemSet> = match &recipe.body {
                Body::Cooking {
                    kind, ingredient, ..
                } if crate::recipes::cooking_key(*kind) == key => vec![*ingredient],
                Body::Smithing { template, .. } | Body::SmithingTrim { template, .. }
                    if key == crate::recipes::SMITHING_TEMPLATE =>
                {
                    vec![*template]
                }
                Body::Smithing { base, .. } | Body::SmithingTrim { base, .. }
                    if key == crate::recipes::SMITHING_BASE =>
                {
                    vec![*base]
                }
                Body::Smithing { addition, .. } | Body::SmithingTrim { addition, .. }
                    if key == crate::recipes::SMITHING_ADDITION =>
                {
                    vec![*addition]
                }
                _ => Vec::new(),
            };
            items.extend(sets.iter().flat_map(|set| set.items().iter().map(|i| i.0)));
        }
        items.sort_unstable();
        items.dedup();
        items
    }
}

thread_local! {
    static ACTIVE: RefCell<Arc<PluginRecipes>> = RefCell::new(Arc::default());
}

static PUBLISHED: RwLock<Option<Arc<PluginRecipes>>> = RwLock::new(None);

pub fn activate(recipes: &Arc<PluginRecipes>) {
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        if !Arc::ptr_eq(&active, recipes) {
            *active = Arc::clone(recipes);
        }
    });
}

pub fn with<R>(read: impl FnOnce(&PluginRecipes) -> R) -> R {
    let active = ACTIVE.with(|active| Arc::clone(&active.borrow()));
    read(&active)
}

pub fn publish(recipes: Option<Arc<PluginRecipes>>) {
    *PUBLISHED.write().unwrap_or_else(PoisonError::into_inner) = recipes;
}

pub fn published() -> Option<Arc<PluginRecipes>> {
    PUBLISHED
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

pub fn intern_id(id: &str) -> (&'static str, i32) {
    static NAMES: Mutex<Option<HashMap<String, (&'static str, i32)>>> = Mutex::new(None);
    let mut names = NAMES.lock().unwrap_or_else(PoisonError::into_inner);
    let names = names.get_or_insert_with(HashMap::new);
    if let Some(found) = names.get(id) {
        return *found;
    }
    let display = FIRST_DISPLAY_ID.saturating_add(i32::try_from(names.len()).unwrap_or(i32::MAX));
    let leaked: &'static str = Box::leak(id.to_owned().into_boxed_str());
    names.insert(id.to_owned(), (leaked, display));
    (leaked, display)
}

pub fn intern_items(items: &[ItemId]) -> ItemSet {
    static SETS: Mutex<Option<HashMap<Vec<ItemId>, &'static [ItemId]>>> = Mutex::new(None);
    let mut sorted = items.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut sets = SETS.lock().unwrap_or_else(PoisonError::into_inner);
    let sets = sets.get_or_insert_with(HashMap::new);
    if let Some(found) = sets.get(&sorted) {
        return ItemSet::Direct(found);
    }
    let leaked: &'static [ItemId] = Box::leak(sorted.clone().into_boxed_slice());
    sets.insert(sorted, leaked);
    ItemSet::Direct(leaked)
}

pub fn by_display(display_id: i32) -> Option<&'static str> {
    if display_id < FIRST_DISPLAY_ID {
        return None;
    }
    with(|recipes| {
        recipes
            .iter()
            .find(|recipe| recipe.display_id == display_id)
            .map(|recipe| recipe.id)
    })
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU8;

    use rspigot_items::ItemKind;

    use super::*;

    fn recipe(id: &str, owner: &str) -> PluginRecipe {
        let (id, display_id) = intern_id(id);
        PluginRecipe {
            id,
            display_id,
            owner: owner.to_owned(),
            body: Body::Shapeless {
                ingredients: vec![intern_items(&[ItemKind::Stick.id()])],
            },
            result: ItemStack::new(ItemKind::Diamond.id(), NonZeroU8::MIN),
            group: String::new(),
            category: Category::Misc,
            override_vanilla: false,
            auto_discover: false,
            exact: Vec::new(),
        }
    }

    #[test]
    fn interned_ids_and_sets_are_stable_and_sorted() {
        assert_eq!(intern_id("t:a"), intern_id("t:a"));
        assert_ne!(intern_id("t:a").1, intern_id("t:b").1);
        let set = intern_items(&[
            ItemKind::Stick.id(),
            ItemKind::Dirt.id(),
            ItemKind::Stick.id(),
        ]);
        assert_eq!(set.items().len(), 2);
        assert!(set.contains(ItemKind::Dirt.id()));
        assert_eq!(
            set,
            intern_items(&[ItemKind::Dirt.id(), ItemKind::Stick.id()])
        );
    }

    #[test]
    fn recipes_are_added_removed_and_dropped_with_their_owner() {
        let mut set = PluginRecipes::default();
        set.add(recipe("t:one", "a"));
        set.add(recipe("t:two", "b"));
        set.add(recipe("t:three", "a"));
        assert_eq!(set.count_of("a"), 2);
        assert!(set.contains("t:two"));
        assert_eq!(set.remove("t:two").map(|r| r.id), Some("t:two"));
        assert!(set.remove("t:two").is_none());
        let gone: Vec<_> = set.remove_owner("a").iter().map(|r| r.id).collect();
        assert_eq!(gone, ["t:one", "t:three"]);
        assert!(set.is_empty());
    }

    #[test]
    fn the_active_set_is_what_the_matchers_read() {
        let mut set = PluginRecipes::default();
        set.add(recipe("t:active", "a"));
        activate(&Arc::new(set));
        assert!(with(|recipes| recipes.contains("t:active")));
        assert_eq!(by_display(intern_id("t:active").1), Some("t:active"));
        activate(&Arc::default());
        assert!(with(PluginRecipes::is_empty));
    }
}
