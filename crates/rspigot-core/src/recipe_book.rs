use rspigot_domain::player::PlayerId;
use rspigot_nbt::Compound;

use crate::plugins::EventPayload;
use crate::plugins::events::{
    PlayerRecipeBookSettingsChange, PlayerRecipeBookSettingsChangeRecipeBookType,
};
use crate::plugins::steps::Continuation;
use crate::ports::ClientLink;
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
            player.recipe_book.set(step.kind, step.open, step.filtering);
        }
    }
}
