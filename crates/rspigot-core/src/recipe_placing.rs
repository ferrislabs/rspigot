use rspigot_domain::container_menu::MenuKind;
use rspigot_domain::item::{Inventory, ItemStack, Slot, same_item_same_components, with_count};
use rspigot_domain::player::PlayerId;
use rspigot_items::ItemSet;

use crate::events::ClientEvent;
use crate::plugins::views::Window;
use crate::ports::ClientLink;
use crate::recipe_catalog::{CatalogRecipe, Shape};
use crate::server::{Audience, Server};

const STORAGE: std::ops::RangeInclusive<usize> = 9..=44;
const PLAYER_GRID: [usize; 4] = [1, 2, 3, 4];
const TABLE_GRID: [usize; 9] = [1, 2, 3, 4, 5, 6, 7, 8, 9];
const FURNACE_INPUT: [usize; 1] = [0];

struct Target {
    window: Window,
    slots: &'static [usize],
    width: usize,
    height: usize,
}

fn layout(recipe: &CatalogRecipe, target: &Target) -> Option<Vec<Option<ItemSet>>> {
    let mut cells = vec![None; target.width * target.height];
    match &recipe.shape {
        Shape::Shaped {
            width,
            height,
            cells: ingredients,
        } => {
            let (width, height) = (usize::from(*width), usize::from(*height));
            if width > target.width || height > target.height {
                return None;
            }
            for y in 0..height {
                for x in 0..width {
                    cells[y * target.width + x] = ingredients[y * width + x];
                }
            }
        }
        Shape::Shapeless(ingredients) => {
            if ingredients.len() > cells.len() {
                return None;
            }
            for (cell, ingredient) in cells.iter_mut().zip(ingredients) {
                *cell = Some(*ingredient);
            }
        }
        Shape::Cooking { ingredient, .. } => cells[0] = Some(*ingredient),
        Shape::Other => return None,
    }
    Some(cells)
}

fn available(inventory: &Inventory, template: &ItemStack) -> u32 {
    STORAGE
        .filter_map(|slot| inventory.get(slot))
        .filter(|stack| same_item_same_components(stack, template))
        .map(|stack| u32::from(stack.count.get()))
        .sum()
}

fn pick(inventory: &Inventory, set: ItemSet, preferred: Option<&ItemStack>) -> Option<ItemStack> {
    if let Some(preferred) = preferred
        && set.contains(preferred.item)
        && available(inventory, preferred) > 0
    {
        return Some(preferred.clone());
    }
    STORAGE
        .filter_map(|slot| inventory.get(slot))
        .find(|stack| set.contains(stack.item))
        .cloned()
}

fn take(inventory: &mut Inventory, template: &ItemStack, mut amount: u32) {
    for slot in STORAGE {
        if amount == 0 {
            return;
        }
        let Some(stack) = inventory.get(slot).cloned() else {
            continue;
        };
        if !same_item_same_components(&stack, template) {
            continue;
        }
        let taken = amount.min(u32::from(stack.count.get()));
        inventory.set(
            slot,
            with_count(&stack, u32::from(stack.count.get()) - taken),
        );
        amount -= taken;
    }
}

struct Plan {
    inventory: Inventory,
    cells: Vec<Slot>,
}

fn plan(
    mut inventory: Inventory,
    ingredients: &[Option<ItemSet>],
    previous: &[Slot],
    max: bool,
) -> Plan {
    let mut chosen: Vec<Option<usize>> = Vec::with_capacity(ingredients.len());
    let mut templates: Vec<(ItemStack, u32)> = Vec::new();
    for (index, ingredient) in ingredients.iter().enumerate() {
        let template = ingredient.and_then(|set| {
            pick(
                &inventory,
                set,
                previous.get(index).and_then(Option::as_ref),
            )
        });
        let slot = template.map(|template| {
            let position = templates
                .iter()
                .position(|(known, _)| same_item_same_components(known, &template))
                .unwrap_or_else(|| {
                    templates.push((template, 0));
                    templates.len() - 1
                });
            templates[position].1 += 1;
            position
        });
        chosen.push(slot);
    }
    let complete = ingredients
        .iter()
        .zip(&chosen)
        .all(|(ingredient, slot)| ingredient.is_none() || slot.is_some());
    let crafts = templates
        .iter()
        .map(|(template, cells)| available(&inventory, template) / cells)
        .min()
        .filter(|_| complete)
        .unwrap_or(0);
    let crafts = if max {
        templates
            .iter()
            .map(|(template, _)| u32::from(template.item.max_stack_size()))
            .fold(crafts, u32::min)
    } else {
        crafts.min(1)
    };
    if crafts == 0 {
        return Plan {
            inventory,
            cells: vec![None; ingredients.len()],
        };
    }
    for (template, cells) in &templates {
        take(&mut inventory, template, cells * crafts);
    }
    let cells = chosen
        .iter()
        .map(|slot| slot.and_then(|slot| with_count(&templates[slot].0, crafts)))
        .collect();
    Plan { inventory, cells }
}

impl<L: ClientLink> Server<L> {
    fn placing_target(&self, id: PlayerId, recipe: &CatalogRecipe) -> Option<Target> {
        let player = self.players.get(&id)?;
        let window = self.window_of(id)?;
        let grid = matches!(recipe.shape, Shape::Shaped { .. } | Shape::Shapeless(_));
        match window {
            Window::Inventory if grid => Some(Target {
                window,
                slots: &PLAYER_GRID,
                width: 2,
                height: 2,
            }),
            Window::Container(_) => {
                let kind = player.open_menu.as_ref()?.menu.kind();
                match (&recipe.shape, kind) {
                    (Shape::Shaped { .. } | Shape::Shapeless(_), MenuKind::Crafting) => {
                        Some(Target {
                            window,
                            slots: &TABLE_GRID,
                            width: 3,
                            height: 3,
                        })
                    }
                    (Shape::Cooking { kind: cooking, .. }, menu)
                        if menu.cooking() == Some(*cooking) =>
                    {
                        Some(Target {
                            window,
                            slots: &FURNACE_INPUT,
                            width: 1,
                            height: 1,
                        })
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn placing_grid(&self, id: PlayerId, target: &Target) -> Vec<Slot> {
        let Some(player) = self.players.get(&id) else {
            return Vec::new();
        };
        match target.window {
            Window::Container(_) => {
                let items = player
                    .open_menu
                    .as_ref()
                    .map(|open| self.open_items(open))
                    .unwrap_or_default();
                target
                    .slots
                    .iter()
                    .map(|slot| items.get(*slot).cloned().flatten())
                    .collect()
            }
            _ => target
                .slots
                .iter()
                .map(|slot| player.inventory.get(*slot).cloned())
                .collect(),
        }
    }

    pub(crate) fn place_recipe(
        &mut self,
        id: PlayerId,
        window: i32,
        recipe: &CatalogRecipe,
        max: bool,
    ) {
        if self.window_of(id).map(Window::id) != Some(window) {
            return;
        }
        let Some(target) = self.placing_target(id, recipe) else {
            return;
        };
        let Some(ingredients) = layout(recipe, &target) else {
            return;
        };
        let previous = self.placing_grid(id, &target);
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let mut inventory = player.inventory.clone();
        if matches!(target.window, Window::Inventory) {
            for slot in target.slots {
                inventory.set(*slot, None);
            }
        }
        for stack in previous.iter().flatten() {
            let mut rest = Some(stack.clone());
            inventory.add(&mut rest, false);
            if rest.is_some() {
                return;
            }
        }
        let planned = plan(inventory, &ingredients, &previous, max);
        let missing = planned.cells.iter().all(Option::is_none);
        if let Some(player) = self.players.get_mut(&id) {
            player.inventory = planned.inventory;
        }
        for (slot, stack) in target.slots.iter().zip(planned.cells) {
            self.write_window_slot(id, target.window, *slot, stack);
        }
        self.sync_inventory(id);
        if matches!(target.window, Window::Container(_)) {
            self.sync_open_menu(id, true);
        }
        if missing && let Some(display) = recipe.display() {
            self.push(
                Audience::One(id),
                ClientEvent::GhostRecipe { window, display },
            );
        }
    }
}
