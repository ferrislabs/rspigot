use std::collections::BTreeSet;

use rspigot_domain::player::PlayerId;
use rspigot_nbt::{Compound, Tag};

use crate::events::{BookEntry, ClientEvent};
use crate::plugins::events::{
    PlayerRecipeBookClick, PlayerRecipeBookClickPatch, PlayerRecipeBookSettingsChange,
    PlayerRecipeBookSettingsChangeRecipeBookType, PlayerRecipeDiscover,
};
use crate::plugins::steps::Continuation;
use crate::plugins::{EventKind, EventPayload};
use crate::ports::ClientLink;
use crate::recipe_catalog::{self, CatalogRecipe};
use crate::server::Server;

pub const RECIPE_BOOK: &str = "recipeBook";

pub type BookKind = PlayerRecipeBookSettingsChangeRecipeBookType;

const KINDS: [BookKind; 4] = [
    BookKind::Crafting,
    BookKind::Furnace,
    BookKind::BlastFurnace,
    BookKind::Smoker,
];
const OPEN_KEYS: [&str; 4] = [
    "isGuiOpen",
    "isFurnaceGuiOpen",
    "isBlastingFurnaceGuiOpen",
    "isSmokerGuiOpen",
];
const FILTER_KEYS: [&str; 4] = [
    "isFilteringCraftable",
    "isFurnaceFilteringCraftable",
    "isBlastingFurnaceFilteringCraftable",
    "isSmokerFilteringCraftable",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RecipeBookSettings {
    states: [(bool, bool); 4],
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BookSettingStep {
    pub(crate) player: PlayerId,
    pub(crate) kind: BookKind,
    pub(crate) open: bool,
    pub(crate) filtering: bool,
}

fn index(kind: BookKind) -> usize {
    KINDS
        .iter()
        .position(|known| *known == kind)
        .unwrap_or_default()
}

impl RecipeBookSettings {
    pub fn set(&mut self, kind: BookKind, open: bool, filtering: bool) {
        self.states[index(kind)] = (open, filtering);
    }

    pub fn get(&self, kind: BookKind) -> (bool, bool) {
        self.states[index(kind)]
    }

    pub fn to_nbt(self) -> Compound {
        let mut out = Compound::new();
        for (slot, (open, filtering)) in self.states.into_iter().enumerate() {
            out.insert(OPEN_KEYS[slot], open);
            out.insert(FILTER_KEYS[slot], filtering);
        }
        out
    }

    pub fn from_nbt(data: Option<&Compound>) -> Self {
        let mut settings = Self::default();
        if let Some(data) = data {
            for slot in 0..KINDS.len() {
                settings.states[slot] = (
                    data.get_bool(OPEN_KEYS[slot]).unwrap_or(false),
                    data.get_bool(FILTER_KEYS[slot]).unwrap_or(false),
                );
            }
        }
        settings
    }
}

const KNOWN: &str = "recipes";
const HIGHLIGHTED: &str = "toBeDisplayed";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecipeBook {
    pub settings: RecipeBookSettings,
    known: BTreeSet<&'static str>,
    highlighted: BTreeSet<&'static str>,
}

impl RecipeBook {
    pub fn contains(&self, recipe: &str) -> bool {
        self.known.contains(recipe)
    }

    pub fn known(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.known.iter().copied()
    }

    pub fn is_highlighted(&self, recipe: &str) -> bool {
        self.highlighted.contains(recipe)
    }

    pub(crate) fn add(&mut self, recipe: &'static str) -> bool {
        let added = self.known.insert(recipe);
        if added {
            self.highlighted.insert(recipe);
        }
        added
    }

    pub(crate) fn remove(&mut self, recipe: &str) -> bool {
        self.highlighted.remove(recipe);
        self.known.remove(recipe)
    }

    pub(crate) fn see(&mut self, recipe: &str) {
        self.highlighted.remove(recipe);
    }

    pub fn to_nbt(&self) -> Compound {
        let mut out = self.settings.to_nbt();
        out.insert(KNOWN, names_to_nbt(&self.known));
        out.insert(HIGHLIGHTED, names_to_nbt(&self.highlighted));
        out
    }

    pub fn from_nbt(data: Option<&Compound>) -> Self {
        let mut book = Self {
            settings: RecipeBookSettings::from_nbt(data),
            ..Self::default()
        };
        let Some(data) = data else {
            return book;
        };
        for name in names_from_nbt(data, KNOWN) {
            book.known.insert(name);
        }
        for name in names_from_nbt(data, HIGHLIGHTED) {
            if book.known.contains(name) {
                book.highlighted.insert(name);
            }
        }
        book
    }
}

fn names_to_nbt(names: &BTreeSet<&'static str>) -> Tag {
    Tag::List(
        names
            .iter()
            .map(|name| Tag::String((*name).to_owned()))
            .collect(),
    )
}

fn names_from_nbt<'a>(data: &'a Compound, key: &str) -> impl Iterator<Item = &'static str> + 'a {
    data.get_list(key).into_iter().flatten().filter_map(|tag| {
        let Tag::String(name) = tag else {
            return None;
        };
        crate::recipe_catalog::by_id(name).map(|recipe| recipe.id)
    })
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn change_recipe_book_settings(
        &mut self,
        id: PlayerId,
        kind: BookKind,
        open: bool,
        filtering: bool,
    ) {
        let step = BookSettingStep {
            player: id,
            kind,
            open,
            filtering,
        };
        self.fire(id, Continuation::BookSetting(step), move |server| {
            server.snapshot_of(id).map(|player| {
                EventPayload::PlayerRecipeBookSettingsChange(PlayerRecipeBookSettingsChange {
                    filtering,
                    open,
                    player,
                    recipe_book_type: kind,
                })
            })
        });
    }

    pub(crate) fn finish_book_setting(&mut self, step: BookSettingStep) {
        if let Some(player) = self.players.get_mut(&step.player) {
            player
                .recipe_book
                .settings
                .set(step.kind, step.open, step.filtering);
        }
    }
}

#[derive(Debug)]
pub(crate) struct DiscoverStep {
    pub(crate) player: PlayerId,
    pub(crate) recipe: &'static str,
}

#[derive(Debug)]
pub(crate) struct BookClickStep {
    pub(crate) player: PlayerId,
    pub(crate) window: i32,
    pub(crate) original: &'static str,
    pub(crate) max: bool,
}

const NOTIFY_AND_HIGHLIGHT: (bool, bool) = (true, true);

fn entry_of(recipe: &CatalogRecipe, flags: (bool, bool)) -> Option<BookEntry> {
    Some(BookEntry {
        display_id: recipe.display_id,
        display: recipe.display()?,
        group: recipe.group,
        category: recipe.category,
        requirements: recipe.requirements(),
        notify: flags.0,
        highlight: flags.1,
    })
}

pub(crate) fn initial_events(book: &RecipeBook) -> [ClientEvent; 2] {
    let entries = book
        .known()
        .filter_map(recipe_catalog::by_id)
        .filter_map(|recipe| entry_of(recipe, (false, book.is_highlighted(recipe.id))))
        .collect();
    [
        ClientEvent::RecipeBookSettings(book.settings),
        ClientEvent::RecipeBookAdd {
            entries,
            replace: true,
        },
    ]
}

impl<L: ClientLink> Server<L> {
    pub fn award_recipes(&mut self, id: PlayerId, recipes: &[&str]) -> usize {
        let Some(player) = self.players.get(&id) else {
            return 0;
        };
        let fresh: Vec<&'static CatalogRecipe> = recipes
            .iter()
            .filter_map(|name| recipe_catalog::by_id(name))
            .filter(|recipe| !player.recipe_book.contains(recipe.id))
            .collect();
        let count = fresh.len();
        if !self.listened(EventKind::PlayerRecipeDiscover) {
            self.grant_recipes(id, &fresh);
            return count;
        }
        for recipe in fresh {
            let step = DiscoverStep {
                player: id,
                recipe: recipe.id,
            };
            self.fire(id, Continuation::Discover(step), move |server| {
                Some(EventPayload::PlayerRecipeDiscover(PlayerRecipeDiscover {
                    player: server.snapshot_of(id)?,
                    recipe: recipe.id.to_owned(),
                }))
            });
        }
        count
    }

    pub(crate) fn finish_discover(&mut self, step: DiscoverStep, cancelled: bool) {
        let Some(recipe) = recipe_catalog::by_id(step.recipe) else {
            return;
        };
        let known = self
            .players
            .get(&step.player)
            .is_some_and(|player| player.recipe_book.contains(recipe.id));
        if !cancelled && !known {
            self.grant_recipes(step.player, &[recipe]);
        }
    }

    fn grant_recipes(&mut self, id: PlayerId, recipes: &[&'static CatalogRecipe]) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let fresh: Vec<&'static CatalogRecipe> = recipes
            .iter()
            .copied()
            .filter(|recipe| player.recipe_book.add(recipe.id))
            .collect();
        let entries: Vec<BookEntry> = fresh
            .iter()
            .filter_map(|recipe| entry_of(recipe, NOTIFY_AND_HIGHLIGHT))
            .collect();
        if entries.is_empty() {
            return;
        }
        player.link.send(ClientEvent::RecipeBookAdd {
            entries,
            replace: false,
        });
        for recipe in fresh {
            self.trigger_recipe_unlocked(id, recipe.id);
        }
    }

    pub fn take_recipes(&mut self, id: PlayerId, recipes: &[&str]) -> usize {
        let Some(player) = self.players.get_mut(&id) else {
            return 0;
        };
        let removed: Vec<i32> = recipes
            .iter()
            .filter_map(|name| recipe_catalog::by_id(name))
            .filter(|recipe| player.recipe_book.remove(recipe.id))
            .map(|recipe| recipe.display_id)
            .collect();
        if !removed.is_empty() {
            player
                .link
                .send(ClientEvent::RecipeBookRemove(removed.clone()));
        }
        removed.len()
    }

    pub fn known_recipes(&self, id: PlayerId) -> Vec<&'static str> {
        self.players
            .get(&id)
            .map(|player| player.recipe_book.known().collect())
            .unwrap_or_default()
    }

    pub(crate) fn mark_recipe_seen(&mut self, id: PlayerId, display_id: i32) {
        let Some(recipe) = recipe_catalog::by_display(display_id) else {
            return;
        };
        if let Some(player) = self.players.get_mut(&id) {
            player.recipe_book.see(recipe.id);
        }
    }

    pub(crate) fn handle_place_recipe(
        &mut self,
        id: PlayerId,
        window: i32,
        display_id: i32,
        max: bool,
    ) {
        let Some(recipe) = recipe_catalog::by_display(display_id) else {
            return;
        };
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let spectating = player.game_mode == rspigot_domain::player::GameMode::Spectator;
        if spectating
            || player.vitals.is_dead()
            || !player.recipe_book.contains(recipe.id)
            || self.window_of(id).map(|current| current.id()) != Some(window)
        {
            return;
        }
        let step = BookClickStep {
            player: id,
            window,
            original: recipe.id,
            max,
        };
        self.fire(id, Continuation::BookClick(step), move |server| {
            Some(EventPayload::PlayerRecipeBookClick(PlayerRecipeBookClick {
                original_recipe: recipe.id.to_owned(),
                player: server.snapshot_of(id)?,
                recipe: recipe.id.to_owned(),
                shift_click: max,
            }))
        });
    }

    pub(crate) fn finish_book_click(
        &mut self,
        step: BookClickStep,
        patch: PlayerRecipeBookClickPatch,
    ) {
        let chosen = patch
            .recipe
            .as_deref()
            .and_then(recipe_catalog::by_id)
            .or_else(|| recipe_catalog::by_id(step.original));
        let Some(recipe) = chosen else {
            return;
        };
        let max = patch.shift_click.unwrap_or(step.max);
        self.place_recipe(step.player, step.window, recipe, max);
    }
}
