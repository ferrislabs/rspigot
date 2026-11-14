use rspigot_domain::entity::save::component_from_tag;
use rspigot_domain::item::{HOTBAR_START, ItemStack, OFFHAND_SLOT};
use rspigot_domain::player::PlayerId;
use rspigot_items::ItemKind;
use rspigot_nbt::{Compound, Tag};

use crate::console::console_text;
use crate::plugins::events::{PlayerEditBook, PlayerEditBookPatch};
use crate::plugins::steps::Continuation;
use crate::plugins::{BookMeta, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

const WRITABLE: &str = "minecraft:writable_book_content";
const WRITTEN: &str = "minecraft:written_book_content";
const HOTBAR: i32 = 9;
const OFFHAND: i32 = 40;

#[derive(Debug)]
pub(crate) struct EditBookStep {
    pub(crate) player: PlayerId,
    pub(crate) slot: usize,
    pub(crate) before: ItemStack,
    pub(crate) pages: Vec<String>,
    pub(crate) title: Option<String>,
    pub(crate) author: String,
}

fn page_text(tag: &Tag) -> String {
    match tag {
        Tag::String(text) => text.clone(),
        Tag::Compound(page) => match page.get("raw") {
            Some(raw) => page_text(raw),
            None => component_from_tag(tag).map_or_else(String::new, |c| console_text(&c)),
        },
        _ => String::new(),
    }
}

fn pages_of(stack: &ItemStack) -> Vec<String> {
    [WRITABLE, WRITTEN]
        .into_iter()
        .find_map(|key| stack.components.get_compound(key))
        .and_then(|content| content.get_list("pages"))
        .map(|pages| pages.iter().map(page_text).collect())
        .unwrap_or_default()
}

fn meta_of(stack: &ItemStack) -> BookMeta {
    let written = stack.components.get_compound(WRITTEN);
    BookMeta {
        title: written
            .and_then(|content| content.get("title"))
            .map(page_text),
        author: written
            .and_then(|content| content.get_str("author"))
            .map(str::to_owned),
        pages: pages_of(stack),
    }
}

fn page_list(pages: &[String]) -> Tag {
    Tag::List(pages.iter().cloned().map(Tag::String).collect())
}

fn filtered(text: &str) -> String {
    text.chars()
        .filter(|c| *c == '\n' || (*c >= ' ' && *c != '\u{7f}'))
        .collect()
}

fn inventory_slot(slot: i32) -> Option<usize> {
    match slot {
        0..HOTBAR => usize::try_from(slot).ok().map(|slot| HOTBAR_START + slot),
        OFFHAND => Some(OFFHAND_SLOT),
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn handle_edit_book(
        &mut self,
        id: PlayerId,
        slot: i32,
        pages: Vec<String>,
        title: Option<String>,
    ) {
        let Some(slot) = inventory_slot(slot) else {
            return;
        };
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let Some(before) = player
            .inventory
            .get(slot)
            .filter(|stack| stack.item.kind() == Some(ItemKind::WritableBook))
            .cloned()
        else {
            return;
        };
        let author = player.profile.name.clone();
        let step = EditBookStep {
            player: id,
            slot,
            before,
            pages: pages.iter().map(|page| filtered(page)).collect(),
            title: title.as_deref().map(filtered),
            author,
        };
        let previous = meta_of(&step.before);
        let new = BookMeta {
            title: step.title.clone(),
            author: step.title.as_ref().map(|_| step.author.clone()),
            pages: step.pages.clone(),
        };
        let signing = step.title.is_some();
        self.fire(id, Continuation::EditBook(Box::new(step)), move |server| {
            server.snapshot_of(id).map(|player| {
                EventPayload::PlayerEditBook(PlayerEditBook {
                    new_book_meta: new,
                    player,
                    previous_book_meta: previous,
                    signing,
                })
            })
        });
    }

    pub(crate) fn finish_edit_book(
        &mut self,
        step: EditBookStep,
        cancelled: bool,
        patch: PlayerEditBookPatch,
    ) {
        let id = step.player;
        let unchanged = self
            .players
            .get(&id)
            .and_then(|player| player.inventory.get(step.slot))
            == Some(&step.before);
        if !unchanged {
            return;
        }
        if cancelled {
            self.send_inventory(id);
            return;
        }
        let signing = patch.signing.unwrap_or(step.title.is_some());
        let meta = patch.new_book_meta.unwrap_or_else(|| BookMeta {
            title: step.title.clone(),
            author: step.title.as_ref().map(|_| step.author.clone()),
            pages: step.pages.clone(),
        });
        let mut stack = step.before;
        stack.components.remove(WRITABLE);
        if signing {
            let content = Compound::new()
                .with("title", Tag::String(meta.title.unwrap_or_default()))
                .with("author", Tag::String(meta.author.unwrap_or(step.author)))
                .with("pages", page_list(&meta.pages));
            stack.item = ItemKind::WrittenBook.id();
            stack.components.insert(WRITTEN, Tag::Compound(content));
        } else {
            let content = Compound::new().with("pages", page_list(&meta.pages));
            stack.components.insert(WRITABLE, Tag::Compound(content));
        }
        if let Some(player) = self.players.get_mut(&id) {
            player.inventory.set(step.slot, Some(stack));
        }
        self.sync_inventory(id);
    }
}
