use std::sync::Arc;

use rspigot_domain::Identifier;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::plugin_recipes::{self, Body, Category, PluginRecipe, PluginRecipes};
pub use rspigot_items::CookingKind;
use rspigot_items::{ItemId, ItemKind, ItemSet};

use crate::events::ClientEvent;
use crate::plugins::items::stack_of;
use crate::plugins::{ActionError, ActionErrorKind, ItemSpec};
use crate::ports::ClientLink;
use crate::recipe_catalog;
use crate::server::{Audience, Server};

type Cell = Option<(ItemSet, Option<ItemStack>)>;

pub const MAX_PLUGIN_RECIPES: usize = 1024;
pub const MAX_RECIPE_LIST: usize = 8192;
const MAX_ID_LENGTH: usize = 256;
const MAX_GROUP_LENGTH: usize = 64;
const MAX_GRID: usize = 3;
const MAX_SHAPELESS: usize = 9;
const MAX_INGREDIENT_ITEMS: usize = 512;
const ITEM_REGISTRY: &str = "minecraft:item";

#[derive(Debug, Clone, PartialEq)]
pub enum IngredientSpec {
    Items(Vec<String>),
    Tag(String),
    Exact(ItemSpec),
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecipeShape {
    Shaped {
        pattern: Vec<String>,
        keys: Vec<(char, IngredientSpec)>,
        mirror: bool,
    },
    Shapeless(Vec<IngredientSpec>),
    Cooking {
        kind: CookingKind,
        ingredient: IngredientSpec,
        experience: f32,
        time: u32,
    },
    Stonecutting(IngredientSpec),
    Smithing {
        template: IngredientSpec,
        base: IngredientSpec,
        addition: IngredientSpec,
    },
    SmithingTrim {
        template: IngredientSpec,
        base: IngredientSpec,
        addition: IngredientSpec,
        pattern: String,
    },
    Transmute {
        input: IngredientSpec,
        material: IngredientSpec,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecipeSpec {
    pub id: String,
    pub shape: RecipeShape,
    pub result: ItemSpec,
    pub group: String,
    pub category: Category,
    pub override_vanilla: bool,
    pub auto_discover: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecipeKind {
    Shaped,
    Shapeless,
    Furnace,
    BlastFurnace,
    Smoker,
    Campfire,
    Stonecutting,
    Smithing,
    Special,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipeView {
    pub id: String,
    pub kind: RecipeKind,
    pub plugin: bool,
}

fn invalid(reason: impl Into<String>) -> ActionError {
    ActionError::new(ActionErrorKind::InvalidArgument, reason)
}

fn valid_id(id: &str) -> Result<(), ActionError> {
    let parsed = Identifier::parse(id).map_err(|_| invalid(format!("{id} is not a valid id")))?;
    if !id.contains(':') || parsed.as_str() != id || id.len() > MAX_ID_LENGTH {
        return Err(invalid(format!(
            "{id} is not a valid namespace:key id of at most {MAX_ID_LENGTH} characters"
        )));
    }
    Ok(())
}

fn ingredient(spec: &IngredientSpec) -> Result<ItemSet, ActionError> {
    let ids: Vec<ItemId> = match spec {
        IngredientSpec::Items(keys) => {
            if keys.is_empty() {
                return Err(invalid("an ingredient lists no item"));
            }
            if keys.len() > MAX_INGREDIENT_ITEMS {
                return Err(invalid(format!(
                    "an ingredient lists more than {MAX_INGREDIENT_ITEMS} items"
                )));
            }
            keys.iter()
                .map(|key| match ItemKind::from_name(key) {
                    Some(kind) if kind != ItemKind::Air => Ok(kind.id()),
                    _ => Err(invalid(format!("unknown item {key}"))),
                })
                .collect::<Result<_, _>>()?
        }
        IngredientSpec::Exact(item) => {
            let stack = stack_of(item)
                .map_err(invalid)?
                .ok_or_else(|| invalid("an exact ingredient cannot be air"))?;
            vec![stack.item]
        }
        IngredientSpec::Tag(name) => {
            let bare = name.trim_start_matches('#');
            let key = if bare.contains(':') {
                bare.to_owned()
            } else {
                format!("minecraft:{bare}")
            };
            let ids = rspigot_data::tags::tag(ITEM_REGISTRY, &key)
                .ok_or_else(|| invalid(format!("unknown item tag {key}")))?;
            ids.iter()
                .filter_map(|id| u16::try_from(*id).ok().map(ItemId))
                .collect()
        }
    };
    if ids.is_empty() {
        return Err(invalid("an ingredient matches no item"));
    }
    Ok(plugin_recipes::intern_items(&ids))
}

fn exact_of(spec: &IngredientSpec) -> Result<Option<ItemStack>, ActionError> {
    match spec {
        IngredientSpec::Exact(item) => {
            Ok(Some(stack_of(item).map_err(invalid)?.ok_or_else(|| {
                invalid("an exact ingredient cannot be air")
            })?))
        }
        _ => Ok(None),
    }
}

fn exacts(specs: &[&IngredientSpec]) -> Result<Vec<Option<ItemStack>>, ActionError> {
    let found = specs
        .iter()
        .map(|spec| exact_of(spec))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(if found.iter().all(Option::is_none) {
        Vec::new()
    } else {
        found
    })
}

fn pattern_id(pattern: &str) -> Result<(), ActionError> {
    let parsed = Identifier::parse(pattern)
        .map_err(|_| invalid(format!("{pattern} is not a valid trim pattern id")))?;
    if !pattern.contains(':') || parsed.as_str() != pattern {
        return Err(invalid(format!(
            "{pattern} is not a namespace:key trim pattern id"
        )));
    }
    Ok(())
}

fn result(spec: &ItemSpec) -> Result<ItemStack, ActionError> {
    stack_of(spec)
        .map_err(invalid)?
        .ok_or_else(|| invalid("a recipe result cannot be air"))
}

fn shaped(
    pattern: &[String],
    keys: &[(char, IngredientSpec)],
    mirror: bool,
) -> Result<(Body, Vec<Option<ItemStack>>), ActionError> {
    if pattern.is_empty() {
        return Err(invalid("the pattern is empty"));
    }
    if pattern.len() > MAX_GRID {
        return Err(invalid(format!(
            "the pattern has more than {MAX_GRID} rows"
        )));
    }
    let rows: Vec<Vec<char>> = pattern.iter().map(|row| row.chars().collect()).collect();
    let width = rows[0].len();
    if width == 0 || width > MAX_GRID {
        return Err(invalid(format!(
            "a pattern row is not 1 to {MAX_GRID} wide"
        )));
    }
    if rows.iter().any(|row| row.len() != width) {
        return Err(invalid("the pattern rows are not all the same width"));
    }
    let mut sets: Vec<(char, ItemSet, Option<ItemStack>)> = Vec::new();
    for (symbol, spec) in keys {
        if *symbol == ' ' {
            return Err(invalid("the space cannot be an ingredient key"));
        }
        if sets.iter().any(|(known, _, _)| known == symbol) {
            return Err(invalid(format!("the key '{symbol}' is set twice")));
        }
        if !rows.iter().flatten().any(|cell| cell == symbol) {
            return Err(invalid(format!("the key '{symbol}' is not in the pattern")));
        }
        sets.push((*symbol, ingredient(spec)?, exact_of(spec)?));
    }
    let mut cells: Vec<Vec<Cell>> = Vec::with_capacity(rows.len());
    for row in &rows {
        let mut cell_row = Vec::with_capacity(width);
        for symbol in row {
            if *symbol == ' ' {
                cell_row.push(None);
                continue;
            }
            let set = sets
                .iter()
                .find(|(known, _, _)| known == symbol)
                .map(|(_, set, exact)| (*set, exact.clone()))
                .ok_or_else(|| {
                    invalid(format!("the pattern uses '{symbol}' with no ingredient"))
                })?;
            cell_row.push(Some(set));
        }
        cells.push(cell_row);
    }
    let filled = |row: &Vec<Cell>| row.iter().any(Option::is_some);
    let top = cells
        .iter()
        .position(filled)
        .ok_or_else(|| invalid("the pattern has no ingredient"))?;
    let bottom = cells
        .iter()
        .rposition(filled)
        .ok_or_else(|| invalid("the pattern has no ingredient"))?;
    let column = |x: usize| cells.iter().any(|row| row[x].is_some());
    let left = (0..width)
        .find(|x| column(*x))
        .ok_or_else(|| invalid("the pattern has no ingredient"))?;
    let right = (0..width)
        .rfind(|x| column(*x))
        .ok_or_else(|| invalid("the pattern has no ingredient"))?;
    let trimmed: Vec<Cell> = cells[top..=bottom]
        .iter()
        .flat_map(|row| row[left..=right].iter().cloned())
        .collect();
    let exact: Vec<Option<ItemStack>> = trimmed
        .iter()
        .flatten()
        .map(|(_, exact)| exact.clone())
        .collect();
    let exact = if exact.iter().all(Option::is_none) {
        Vec::new()
    } else {
        exact
    };
    Ok((
        Body::Shaped {
            width: right - left + 1,
            height: bottom - top + 1,
            mirror,
            cells: trimmed
                .iter()
                .map(|cell| cell.as_ref().map(|(set, _)| *set))
                .collect(),
        },
        exact,
    ))
}

pub(crate) fn compile(spec: &RecipeSpec, owner: &str) -> Result<PluginRecipe, ActionError> {
    valid_id(&spec.id)?;
    if spec.group.len() > MAX_GROUP_LENGTH {
        return Err(invalid(format!(
            "the group is longer than {MAX_GROUP_LENGTH} characters"
        )));
    }
    let mut exact = Vec::new();
    let body = match &spec.shape {
        RecipeShape::Shaped {
            pattern,
            keys,
            mirror,
        } => {
            let (body, found) = shaped(pattern, keys, *mirror)?;
            exact = found;
            body
        }
        RecipeShape::Shapeless(ingredients) => {
            if ingredients.is_empty() || ingredients.len() > MAX_SHAPELESS {
                return Err(invalid(format!(
                    "a shapeless recipe takes 1 to {MAX_SHAPELESS} ingredients"
                )));
            }
            exact = exacts(&ingredients.iter().collect::<Vec<_>>())?;
            Body::Shapeless {
                ingredients: ingredients
                    .iter()
                    .map(ingredient)
                    .collect::<Result<_, _>>()?,
            }
        }
        RecipeShape::Cooking {
            kind,
            ingredient: choice,
            experience,
            time,
        } => {
            if !experience.is_finite() || *experience < 0.0 {
                return Err(invalid("the experience is negative or not a number"));
            }
            let time = u16::try_from(*time)
                .ok()
                .filter(|time| *time > 0)
                .ok_or_else(|| invalid("the cooking time is not 1 to 65535 ticks"))?;
            exact = exacts(&[choice])?;
            Body::Cooking {
                kind: *kind,
                ingredient: ingredient(choice)?,
                experience: *experience,
                time,
            }
        }
        RecipeShape::Stonecutting(choice) => {
            if matches!(choice, IngredientSpec::Exact(_)) {
                return Err(invalid("a stonecutting recipe cannot take an exact item"));
            }
            Body::Stonecutting {
                ingredient: ingredient(choice)?,
            }
        }
        RecipeShape::Smithing {
            template,
            base,
            addition,
        } => {
            exact = exacts(&[template, base, addition])?;
            Body::Smithing {
                template: ingredient(template)?,
                base: ingredient(base)?,
                addition: ingredient(addition)?,
            }
        }
        RecipeShape::SmithingTrim {
            template,
            base,
            addition,
            pattern,
        } => {
            pattern_id(pattern)?;
            exact = exacts(&[template, base, addition])?;
            Body::SmithingTrim {
                template: ingredient(template)?,
                base: ingredient(base)?,
                addition: ingredient(addition)?,
                pattern: pattern.clone(),
            }
        }
        RecipeShape::Transmute { input, material } => {
            exact = exacts(&[input, material])?;
            Body::Transmute {
                input: ingredient(input)?,
                material: ingredient(material)?,
            }
        }
    };
    let (id, display_id) = plugin_recipes::intern_id(&spec.id);
    Ok(PluginRecipe {
        id,
        display_id,
        owner: owner.to_owned(),
        body,
        result: result(&spec.result)?,
        group: spec.group.clone(),
        category: spec.category,
        override_vanilla: spec.override_vanilla,
        auto_discover: spec.auto_discover,
        exact,
    })
}

fn kind_of(recipe: &PluginRecipe) -> RecipeKind {
    match &recipe.body {
        Body::Shaped { .. } => RecipeKind::Shaped,
        Body::Shapeless { .. } => RecipeKind::Shapeless,
        Body::Cooking { kind, .. } => match kind {
            CookingKind::Smelting => RecipeKind::Furnace,
            CookingKind::Blasting => RecipeKind::BlastFurnace,
            CookingKind::Smoking => RecipeKind::Smoker,
            CookingKind::Campfire => RecipeKind::Campfire,
        },
        Body::Stonecutting { .. } => RecipeKind::Stonecutting,
        Body::Smithing { .. } | Body::SmithingTrim { .. } => RecipeKind::Smithing,
        Body::Transmute { .. } => RecipeKind::Special,
    }
}

impl<L: ClientLink> Server<L> {
    fn commit_plugin_recipes(&mut self, change: impl FnOnce(&mut PluginRecipes)) {
        change(Arc::make_mut(&mut self.plugin_recipes));
        plugin_recipes::activate(&self.plugin_recipes);
        plugin_recipes::publish(Some(Arc::clone(&self.plugin_recipes)));
    }

    pub(crate) fn add_plugin_recipe(
        &mut self,
        connection: u64,
        spec: &RecipeSpec,
    ) -> Result<(), ActionError> {
        let owner = self.plugin_name(connection);
        let recipe = compile(spec, &owner)?;
        if self.plugin_recipes.contains(recipe.id) || recipe_catalog::is_vanilla(recipe.id) {
            return Err(invalid(format!("the recipe {} already exists", recipe.id)));
        }
        if self.plugin_recipes.count_of(&owner) >= MAX_PLUGIN_RECIPES {
            return Err(ActionError::new(
                ActionErrorKind::LimitReached,
                format!("the plugin already holds {MAX_PLUGIN_RECIPES} recipes"),
            ));
        }
        let grid = recipe.uses_crafting_grid();
        self.commit_plugin_recipes(|recipes| recipes.add(recipe));
        if !grid {
            self.push(Audience::All, ClientEvent::RecipeSync);
        }
        Ok(())
    }

    pub(crate) fn remove_plugin_recipe(&mut self, connection: u64, id: &str) -> Result<(), String> {
        let owner = self.plugin_name(connection);
        match self.plugin_recipes.get(id) {
            Some(recipe) if recipe.owner == owner => {}
            Some(_) => return Err(format!("the recipe {id} belongs to another plugin")),
            None => return Err(format!("there is no plugin recipe {id}")),
        }
        let mut gone = Vec::new();
        self.commit_plugin_recipes(|recipes| gone.extend(recipes.remove(id)));
        self.recipes_gone(&gone);
        Ok(())
    }

    pub(crate) fn forget_plugin_recipes(&mut self, owner: &str) {
        if self.plugin_recipes.count_of(owner) == 0 {
            return;
        }
        let mut gone = Vec::new();
        self.commit_plugin_recipes(|recipes| gone = recipes.remove_owner(owner));
        self.recipes_gone(&gone);
    }

    fn recipes_gone(&mut self, gone: &[Arc<PluginRecipe>]) {
        if gone.iter().any(|recipe| !recipe.uses_crafting_grid()) {
            self.push(Audience::All, ClientEvent::RecipeSync);
        }
        for player in self.players.values_mut() {
            let removed: Vec<i32> = gone
                .iter()
                .filter(|recipe| player.recipe_book.remove(recipe.id))
                .map(|recipe| recipe.display_id)
                .collect();
            if !removed.is_empty() {
                player.link.send(ClientEvent::RecipeBookRemove(removed));
            }
        }
    }

    pub(crate) fn recipe_views(&self) -> Vec<RecipeView> {
        let plugin: Vec<RecipeView> = self
            .plugin_recipes
            .iter()
            .map(|recipe| RecipeView {
                id: recipe.id.to_owned(),
                kind: kind_of(recipe),
                plugin: true,
            })
            .collect();
        let room = MAX_RECIPE_LIST.saturating_sub(plugin.len());
        recipe_catalog::vanilla_kinds()
            .iter()
            .take(room)
            .map(|(id, kind)| RecipeView {
                id: (*id).to_owned(),
                kind: *kind,
                plugin: false,
            })
            .chain(plugin)
            .collect()
    }

    pub(crate) fn plugin_discover(&mut self, id: PlayerId, recipe: &str) -> Result<(), String> {
        if recipe_catalog::by_id(recipe).is_none() && !recipe_catalog::is_vanilla(recipe) {
            return Err(format!("there is no recipe {recipe}"));
        }
        self.award_recipes(id, &[recipe]);
        Ok(())
    }

    pub(crate) fn plugin_undiscover(&mut self, id: PlayerId, recipe: &str) -> Result<(), String> {
        if recipe_catalog::by_id(recipe).is_none() && !recipe_catalog::is_vanilla(recipe) {
            return Err(format!("there is no recipe {recipe}"));
        }
        self.take_recipes(id, &[recipe]);
        Ok(())
    }

    pub(crate) fn discovered_recipes(&self, id: PlayerId) -> Option<Vec<String>> {
        self.players.get(&id)?;
        Some(
            self.known_recipes(id)
                .into_iter()
                .map(str::to_owned)
                .collect(),
        )
    }

    pub(crate) fn auto_discover_recipes(&mut self, id: PlayerId) {
        if self.plugin_recipes.is_empty() {
            return;
        }
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let held: Vec<ItemId> = player
            .inventory
            .slots()
            .iter()
            .flatten()
            .map(|stack| stack.item)
            .collect();
        let wanted: Vec<&'static str> = self
            .plugin_recipes
            .iter()
            .filter(|recipe| recipe.auto_discover && !player.recipe_book.contains(recipe.id))
            .filter(|recipe| {
                recipe
                    .ingredients()
                    .iter()
                    .any(|set| held.iter().any(|item| set.contains(*item)))
            })
            .map(|recipe| recipe.id)
            .collect();
        if !wanted.is_empty() {
            self.award_recipes(id, &wanted);
        }
    }
}
