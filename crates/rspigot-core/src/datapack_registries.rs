use std::sync::Arc;

use rspigot_data::advancements::{self, Advancement};
use rspigot_domain::Identifier;
use rspigot_domain::plugin_recipes;

use crate::events::ClientEvent;
use crate::pack_data::recipe_files;
use crate::ports::{ClientLink, EnchantmentEntry};
use crate::recipe_book;
use crate::server::{Audience, Server};

impl<L: ClientLink> Server<L> {
    pub fn datapack_registry(&self, registry: &str) -> Vec<(String, String)> {
        self.functions
            .data
            .registries()
            .overlay()
            .entries(registry)
            .iter()
            .map(|entry| (entry.id.clone(), entry.json.clone()))
            .collect()
    }

    pub fn datapack_enchantments(&self) -> Vec<EnchantmentEntry> {
        self.functions
            .data
            .registries()
            .overlay()
            .entries("minecraft:enchantment")
            .iter()
            .filter_map(|entry| {
                Some(EnchantmentEntry {
                    id: Identifier::parse(&entry.id).ok()?,
                    json: entry.json.clone(),
                })
            })
            .collect()
    }

    pub(crate) fn apply_datapack_registries(
        &mut self,
        previous_advancements: &[Advancement],
        online: bool,
    ) {
        let files = self.functions.data.registries().recipe_files().to_vec();
        let mut compiled = Vec::new();
        for file in &files {
            match recipe_files::compile(file) {
                Ok(recipe) => compiled.push(recipe),
                Err(error) => tracing::warn!("Failed to load recipe {}: {error}", file.id),
            }
        }
        let before: Vec<(&'static str, i32)> = self
            .plugin_recipes
            .iter()
            .filter(|recipe| recipe.owner == recipe_files::OWNER)
            .map(|recipe| (recipe.id, recipe.display_id))
            .collect();
        let recipes = Arc::make_mut(&mut self.plugin_recipes);
        recipes.remove_owner(recipe_files::OWNER);
        for recipe in compiled {
            if recipes.contains(recipe.id) {
                tracing::warn!("Recipe {} is already defined by a plugin", recipe.id);
                continue;
            }
            recipes.add(recipe);
        }
        plugin_recipes::activate(&self.plugin_recipes);
        plugin_recipes::publish(Some(Arc::clone(&self.plugin_recipes)));
        if !online {
            return;
        }
        let gone: Vec<(&'static str, i32)> = before
            .into_iter()
            .filter(|(id, _)| !self.plugin_recipes.contains(id))
            .collect();
        let current = advancements::all();
        let ids: Vec<_> = self.players.keys().copied().collect();
        for id in &ids {
            let Some(player) = self.players.get_mut(id) else {
                continue;
            };
            let removed: Vec<i32> = gone
                .iter()
                .filter(|(recipe, _)| player.recipe_book.remove(recipe))
                .map(|(_, display)| *display)
                .collect();
            if !removed.is_empty() {
                player.link.send(ClientEvent::RecipeBookRemove(removed));
            }
            player.advancements.reindex(previous_advancements, current);
        }
        self.push(Audience::All, ClientEvent::RecipeSync);
        self.push(Audience::All, ClientEvent::UpdateTags);
        for id in ids {
            if let Some(player) = self.players.get(&id) {
                for event in recipe_book::initial_events(&player.recipe_book) {
                    player.link.send(event);
                }
            }
            self.flush_advancements(id, true, false);
        }
    }
}
