use rand::Rng;
use rspigot_blocks::BlockKind;
use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::entity::save::{item_stack_from_nbt, item_stack_to_nbt};
use rspigot_domain::item::{ItemStack, Slot};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, BlockStateId, Identifier, Vec3};
use rspigot_entities::SoundSource;
use rspigot_nbt::Compound;

use crate::chests::Dir;
use crate::ports::ClientLink;
use crate::server::Server;

const BOOK: &str = "Book";
const PAGE: &str = "Page";
const BOOK_PUT: &str = "minecraft:item.book.put";
const LECTERN_BOOKS: &str = "minecraft:lectern_books";
const WRITTEN_BOOK_CONTENT: &str = "minecraft:written_book_content";
const WRITABLE_BOOK_CONTENT: &str = "minecraft:writable_book_content";
const PREVIOUS_PAGE: i32 = 1;
const NEXT_PAGE: i32 = 2;
const TAKE_BOOK: i32 = 3;
const PAGE_JUMP_START: i32 = 100;
const POP_OFFSET: f64 = 0.25;
const POP_PICKUP_DELAY: u16 = 10;
const PAGE_DATA: i16 = 0;

pub(crate) fn is_lectern(state: BlockStateId) -> bool {
    state.kind() == Some(BlockKind::Lectern)
}

pub(crate) fn has_book(state: BlockStateId) -> bool {
    is_lectern(state) && state.property("has_book") == Some("true")
}

fn is_lectern_book(stack: &ItemStack) -> bool {
    rspigot_data::tags::tag("minecraft:item", LECTERN_BOOKS)
        .is_some_and(|ids| ids.binary_search(&u32::from(stack.item.0)).is_ok())
}

fn page_count(book: &ItemStack) -> i32 {
    [WRITTEN_BOOK_CONTENT, WRITABLE_BOOK_CONTENT]
        .into_iter()
        .find_map(|key| book.components.get_compound(key))
        .and_then(|content| content.get_list("pages"))
        .map_or(0, |pages| i32::try_from(pages.len()).unwrap_or(i32::MAX))
}

fn clamp_page(page: i32, count: i32) -> i32 {
    if page < 0 { 0 } else { page.min(count - 1) }
}

impl<L: ClientLink> Server<L> {
    fn lectern_data(&self, pos: BlockPos) -> Option<&Compound> {
        self.world
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .map(|entity| &entity.data)
    }

    pub(crate) fn lectern_book(&self, pos: BlockPos) -> Slot {
        self.lectern_data(pos)
            .and_then(|data| data.get_compound(BOOK))
            .and_then(item_stack_from_nbt)
    }

    pub(crate) fn lectern_page(&self, pos: BlockPos) -> i32 {
        self.lectern_data(pos)
            .and_then(|data| data.get_i32(PAGE))
            .unwrap_or(0)
    }

    fn store_lectern(&mut self, pos: BlockPos, book: Option<&ItemStack>, page: i32) {
        let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) else {
            return;
        };
        let mut data = Compound::new();
        if let Some(book) = book {
            data.insert(BOOK, item_stack_to_nbt(book));
            data.insert(PAGE, page);
        }
        chunk.set_block_entity(BlockEntity {
            pos,
            kind: Identifier::minecraft("lectern"),
            data,
        });
    }

    fn turn_to_page(&mut self, pos: BlockPos, requested: i32) {
        let Some(book) = self.lectern_book(pos) else {
            return;
        };
        let page = clamp_page(requested, page_count(&book));
        if page == self.lectern_page(pos) {
            return;
        }
        self.store_lectern(pos, Some(&book), page);
        self.send_menu_data(pos, PAGE_DATA, i16::try_from(page).unwrap_or(i16::MAX));
    }

    fn take_book(&mut self, id: PlayerId, pos: BlockPos, window: i32) {
        if !self.may_build(id) {
            return;
        }
        let Some(book) = self.lectern_book(pos) else {
            return;
        };
        self.ask_take_book(id, pos, window, book);
    }

    fn may_build(&self, id: PlayerId) -> bool {
        self.players.get(&id).is_some_and(|player| {
            matches!(player.game_mode, GameMode::Survival | GameMode::Creative)
        })
    }

    pub(crate) fn take_book_now(&mut self, id: PlayerId, pos: BlockPos, book: ItemStack) {
        if !self.may_build(id) {
            return;
        }
        self.store_lectern(pos, None, 0);
        if let Some(state) = self.world.chunks.block(pos)
            && let Some(next) = state
                .with_property("powered", "false")
                .and_then(|state| state.with_property("has_book", "false"))
        {
            self.set_block(pos, next);
        }
        self.give_or_drop(id, book);
    }

    pub(crate) fn give_or_drop(&mut self, id: PlayerId, book: ItemStack) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let mut rest = Some(book);
        player.inventory.add(&mut rest, false);
        self.sync_inventory(id);
        self.broadcast_held_item(id);
        if let Some(rest) = rest {
            self.drop_from_player(id, rest, false, true);
        }
    }

    pub(crate) fn handle_container_button(&mut self, id: PlayerId, window: i32, button: i32) {
        if self.press_menu_button(id, window, button) {
            return;
        }
        let Some(pos) = self
            .players
            .get(&id)
            .and_then(|player| player.open_menu.as_ref())
            .filter(|open| open.window == window && open.backing.block == BlockKind::Lectern)
            .map(|open| open.backing.pos)
        else {
            return;
        };
        let page = self.lectern_page(pos);
        match button {
            PREVIOUS_PAGE => self.turn_to_page(pos, page - 1),
            NEXT_PAGE => self.turn_to_page(pos, page + 1),
            TAKE_BOOK => self.take_book(id, pos, window),
            jump if jump >= PAGE_JUMP_START => self.turn_to_page(pos, jump - PAGE_JUMP_START),
            _ => {}
        }
    }

    pub(crate) fn place_book_on_lectern(
        &mut self,
        id: PlayerId,
        slot: usize,
        pos: BlockPos,
        clicked: BlockStateId,
    ) -> bool {
        if !is_lectern(clicked) || has_book(clicked) {
            return false;
        }
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let mode = player.game_mode;
        let Some(held) = player
            .inventory
            .get(slot)
            .filter(|stack| is_lectern_book(stack))
            .cloned()
        else {
            return false;
        };
        if mode == GameMode::Spectator {
            return false;
        }
        self.store_lectern(pos, Some(&held), 0);
        if let Some(next) = clicked
            .with_property("powered", "false")
            .and_then(|state| state.with_property("has_book", "true"))
        {
            self.set_block(pos, next);
        }
        if mode != GameMode::Creative {
            self.consume_one(id, slot, held);
        }
        self.play_sound(
            None,
            pos.center() + Vec3::new(0.0, 0.5, 0.0),
            BOOK_PUT,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
        true
    }

    pub(crate) fn lectern_book_lost_by(
        &self,
        pos: BlockPos,
        old: BlockStateId,
        new: BlockStateId,
    ) -> Option<(Vec3, ItemStack)> {
        if old.kind() == new.kind() || !has_book(old) {
            return None;
        }
        let book = self.lectern_book(pos)?;
        let (dx, dz) = old
            .property("facing")
            .and_then(Dir::parse)
            .map_or((0, 0), Dir::step);
        let at = Vec3::new(
            f64::from(pos.x) + 0.5 + POP_OFFSET * f64::from(dx),
            f64::from(pos.y) + 1.0,
            f64::from(pos.z) + 0.5 + POP_OFFSET * f64::from(dz),
        );
        Some((at, book))
    }

    pub(crate) fn pop_book(&mut self, at: Vec3, book: ItemStack) {
        let velocity = Vec3::new(
            self.rng.random::<f64>() * 0.2 - 0.1,
            0.2,
            self.rng.random::<f64>() * 0.2 - 0.1,
        );
        self.spawn_item_entity(at, velocity, POP_PICKUP_DELAY, book);
    }
}
