use std::num::NonZeroU8;

use rspigot_blocks::BlockKind;
use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::entity::save::{component_from_tag, component_to_tag, read_uuid, uuid_tag};
use rspigot_domain::item::{Slot, max_stack, same_item_same_components};
use rspigot_domain::spawner::SpawnData;
use rspigot_domain::{BlockPos, BlockStateId, Identifier};
use rspigot_entities::EntityKind;
use rspigot_nbt::{Compound, Tag};
use uuid::Uuid;

use crate::containers::{Backing, opening_of};
use crate::events::ClientEvent;
use crate::plugins::resolve;
use crate::plugins::{ItemSpec, spec_of, stack_of};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use crate::sign;

const FRONT_TEXT: &str = "front_text";
const BACK_TEXT: &str = "back_text";
const MESSAGES: &str = "messages";
const WAXED: &str = "is_waxed";
const COLOR: &str = "color";
const GLOWING: &str = "has_glowing_text";
const PROFILE: &str = "profile";
const PATTERNS: &str = "patterns";
const SHERDS: &str = "sherds";
const COMMAND: &str = "Command";
const CUSTOM_NAME: &str = "CustomName";
const PRIMARY: &str = "primary_effect";
const SECONDARY: &str = "secondary_effect";
const LEVELS: &str = "Levels";
const EXIT_PORTAL: &str = "exit_portal";
const EXACT_TELEPORT: &str = "ExactTeleport";
const FURNACE_SLOTS: usize = 3;
const BREWING_SLOTS: usize = 5;
const BOOKSHELF_SLOTS: usize = 6;
const SHELF_SLOTS: usize = rspigot_domain::shelf::SLOTS;
const MAX_SIGN_LINES: usize = 4;
const POT_SHERDS: usize = 4;
const DYES: [&str; 16] = [
    "white",
    "orange",
    "magenta",
    "light_blue",
    "yellow",
    "lime",
    "pink",
    "gray",
    "light_gray",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
];

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FurnaceEdit {
    pub burn_time: Option<i32>,
    pub cook_time: Option<i32>,
    pub cook_time_total: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SpawnerEdit {
    pub entity: Option<String>,
    pub delay: Option<i32>,
    pub min_delay: Option<i32>,
    pub max_delay: Option<i32>,
    pub spawn_count: Option<i32>,
    pub max_nearby: Option<i32>,
    pub required_player_range: Option<i32>,
    pub spawn_range: Option<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TileEdit {
    SignLines {
        front: bool,
        lines: Vec<String>,
    },
    SignStyle {
        front: bool,
        color: Option<String>,
        glowing: Option<bool>,
    },
    SignWaxed(bool),
    InventorySet {
        slot: i32,
        item: Option<ItemSpec>,
    },
    InventoryAdd(ItemSpec),
    InventoryClear,
    Furnace(FurnaceEdit),
    Spawner(SpawnerEdit),
    SkullOwner {
        name: Option<String>,
        uuid: Option<Uuid>,
    },
    BannerPatterns(Vec<(String, String)>),
    CommandBlock {
        command: Option<String>,
        name: Option<String>,
    },
    Beacon {
        primary: Option<String>,
        secondary: Option<String>,
    },
    JukeboxDisc(ItemSpec),
    JukeboxClear,
    EndGateway {
        exit: BlockPos,
        exact: bool,
    },
    LecternBook(ItemSpec),
    LecternClear,
    DecoratedPot(Vec<String>),
}

impl TileEdit {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::SignLines { .. } => "block.sign.set_lines",
            Self::SignStyle { .. } => "block.sign.set_style",
            Self::SignWaxed(_) => "block.sign.set_waxed",
            Self::InventorySet { .. } => "block.inventory.set",
            Self::InventoryAdd(_) => "block.inventory.add",
            Self::InventoryClear => "block.inventory.clear",
            Self::Furnace(_) => "block.furnace.set",
            Self::Spawner(_) => "block.spawner.set",
            Self::SkullOwner { .. } => "block.skull.set_owner",
            Self::BannerPatterns(_) => "block.banner.set_patterns",
            Self::CommandBlock { .. } => "block.command.set",
            Self::Beacon { .. } => "block.beacon.set_effects",
            Self::JukeboxDisc(_) => "block.jukebox.set_disc",
            Self::JukeboxClear => "block.jukebox.clear",
            Self::EndGateway { .. } => "block.end_gateway.set_exit",
            Self::LecternBook(_) => "block.lectern.set_book",
            Self::LecternClear => "block.lectern.clear",
            Self::DecoratedPot(_) => "block.decorated_pot.set_sherds",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignView {
    pub front: Vec<String>,
    pub back: Vec<String>,
    pub waxed: bool,
    pub front_color: String,
    pub back_color: String,
    pub front_glowing: bool,
    pub back_glowing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentsView {
    pub size: usize,
    pub slots: Vec<(usize, ItemSpec)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FurnaceView {
    pub burn_time: i32,
    pub cook_time: i32,
    pub cook_time_total: i32,
    pub lit: bool,
    pub contents: ContentsView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnerView {
    pub entity: String,
    pub delay: i32,
    pub min_delay: i32,
    pub max_delay: i32,
    pub spawn_count: i32,
    pub max_nearby: i32,
    pub required_player_range: i32,
    pub spawn_range: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TileData {
    Sign(SignView),
    Inventory(ContentsView),
    Furnace(FurnaceView),
    Spawner(SpawnerView),
    Skull {
        name: Option<String>,
        uuid: Option<Uuid>,
    },
    Banner(Vec<(String, String)>),
    CommandBlock {
        command: String,
        name: String,
    },
    Beacon {
        levels: i32,
        primary: Option<String>,
        secondary: Option<String>,
    },
    Jukebox(Option<ItemSpec>),
    Lectern(Option<ItemSpec>),
    EndGateway {
        exit: Option<BlockPos>,
        exact: bool,
    },
    DecoratedPot(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileView {
    pub kind: String,
    pub data: TileData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockInfo {
    pub light: i32,
    pub sky_light: i32,
    pub block_light: i32,
    pub power: i32,
    pub biome: String,
}

#[derive(Debug, Clone, Copy)]
enum Tile {
    Sign(&'static str),
    Storage(Backing, usize),
    Plain(&'static str, usize),
    Furnace,
    Spawner,
    Skull,
    Banner,
    Command,
    Beacon,
    Jukebox,
    Lectern,
    Gateway,
    Pot,
}

fn is_player_head(state: BlockStateId) -> bool {
    state
        .kind()
        .is_some_and(|kind| kind.name().contains("player_"))
}

fn tile_name(tile: Tile) -> &'static str {
    match tile {
        Tile::Sign(name) => name,
        Tile::Storage(backing, _) => backing.entity.unwrap_or("chest"),
        Tile::Plain(name, _) => name,
        Tile::Furnace => "furnace",
        Tile::Spawner => "mob_spawner",
        Tile::Skull => "skull",
        Tile::Banner => "banner",
        Tile::Command => "command_block",
        Tile::Beacon => "beacon",
        Tile::Jukebox => "jukebox",
        Tile::Lectern => "lectern",
        Tile::Gateway => "end_gateway",
        Tile::Pot => "decorated_pot",
    }
}

fn dye(name: &str) -> Result<&str, String> {
    DYES.contains(&name)
        .then_some(name)
        .ok_or_else(|| format!("{name} is not a dye colour"))
}

fn effect_key(key: &Option<String>) -> Result<Option<&str>, String> {
    match key {
        Some(key) => resolve::effect_id(key).map(|_| Some(key.as_str())),
        None => Ok(None),
    }
}

fn spec_slots(items: &[Slot]) -> Vec<(usize, ItemSpec)> {
    items
        .iter()
        .enumerate()
        .filter_map(|(slot, stack)| Some((slot, spec_of(stack.as_ref()?))))
        .collect()
}

fn stackable(
    into: &rspigot_domain::item::ItemStack,
    stack: &rspigot_domain::item::ItemStack,
) -> bool {
    same_item_same_components(into, stack) && u32::from(into.count.get()) < max_stack(into)
}

impl<L: ClientLink> Server<L> {
    fn tile_at(&self, pos: BlockPos) -> Option<Tile> {
        let state = self.world.chunks.block(pos)?;
        let kind = state.kind()?;
        if sign::is_sign(state) {
            return Some(Tile::Sign(sign::entity_kind(state)));
        }
        if let Some((backing, size)) = self.storage_backing(pos) {
            return Some(Tile::Storage(backing, size));
        }
        let name = kind.name();
        Some(match kind {
            BlockKind::Furnace | BlockKind::BlastFurnace | BlockKind::Smoker => Tile::Furnace,
            BlockKind::BrewingStand => Tile::Plain("brewing_stand", BREWING_SLOTS),
            BlockKind::ChiseledBookshelf => Tile::Plain("chiseled_bookshelf", BOOKSHELF_SLOTS),
            kind if crate::shelf::is_shelf_kind(kind) => Tile::Plain("shelf", SHELF_SLOTS),
            BlockKind::Spawner => Tile::Spawner,
            BlockKind::CommandBlock
            | BlockKind::ChainCommandBlock
            | BlockKind::RepeatingCommandBlock => Tile::Command,
            BlockKind::Beacon => Tile::Beacon,
            BlockKind::Jukebox => Tile::Jukebox,
            BlockKind::Lectern => Tile::Lectern,
            BlockKind::EndGateway => Tile::Gateway,
            BlockKind::DecoratedPot => Tile::Pot,
            _ if name.ends_with("_head") || name.ends_with("_skull") => Tile::Skull,
            _ if name.ends_with("_banner") => Tile::Banner,
            _ => return None,
        })
    }

    fn tile_data_at(&self, pos: BlockPos) -> Compound {
        self.world
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .map(|entity| entity.data.clone())
            .unwrap_or_default()
    }

    fn put_tile(&mut self, pos: BlockPos, tile: Tile, data: Compound, send: bool) {
        let name = tile_name(tile);
        if let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) {
            chunk.set_block_entity(BlockEntity {
                pos,
                kind: Identifier::minecraft(name),
                data: data.clone(),
            });
        }
        if send {
            let mut data = data;
            data.remove("SpawnPotentials");
            self.push(
                Audience::ChunkViewers(pos.chunk()),
                ClientEvent::BlockEntityUpdate {
                    pos,
                    kind: name,
                    data,
                },
            );
        }
    }

    fn tile_items(&self, pos: BlockPos, tile: Tile) -> Option<Vec<Slot>> {
        match tile {
            Tile::Storage(backing, size) => Some(self.container_items(backing, size)),
            Tile::Plain(_, size) => Some(self.half_items(pos, size)),
            Tile::Furnace => Some(self.half_items(pos, FURNACE_SLOTS)),
            _ => None,
        }
    }

    fn store_tile_items(&mut self, pos: BlockPos, tile: Tile, items: &[Slot]) {
        match tile {
            Tile::Storage(backing, _) => self.store_container(backing, items),
            Tile::Plain(name, _) => self.store_half(pos, name, items),
            Tile::Furnace => {
                let name = self
                    .world
                    .chunks
                    .block(pos)
                    .and_then(opening_of)
                    .and_then(|opening| opening.entity)
                    .unwrap_or("furnace");
                self.store_half(pos, name, items);
            }
            _ => return,
        }
        if let Tile::Plain("chiseled_bookshelf", _) = tile {
            self.mark_bookshelf_slots(pos, items);
        }
        self.sync_viewers(pos, None);
    }

    fn mark_bookshelf_slots(&mut self, pos: BlockPos, items: &[Slot]) {
        let Some(mut state) = self.world.chunks.block(pos) else {
            return;
        };
        for (slot, stack) in items.iter().enumerate() {
            let value = if stack.is_some() { "true" } else { "false" };
            if let Some(next) = state.with_property(&format!("slot_{slot}_occupied"), value) {
                state = next;
            }
        }
        if self.world.chunks.block(pos) != Some(state) {
            self.set_block_quiet(pos, state);
        }
    }

    pub(super) fn apply_tile(&mut self, pos: BlockPos, edit: TileEdit) -> Result<(), String> {
        let tile = self
            .tile_at(pos)
            .ok_or_else(|| "the block has no tile entity this edit applies to".to_owned())?;
        let wrong = || format!("the block is not a {}", edit_target(&edit));
        match (&edit, tile) {
            (
                TileEdit::SignLines { .. } | TileEdit::SignStyle { .. } | TileEdit::SignWaxed(_),
                Tile::Sign(_),
            )
            | (
                TileEdit::InventorySet { .. }
                | TileEdit::InventoryAdd(_)
                | TileEdit::InventoryClear,
                Tile::Storage(..) | Tile::Plain(..) | Tile::Furnace,
            )
            | (TileEdit::Furnace(_), Tile::Furnace)
            | (TileEdit::Spawner(_), Tile::Spawner)
            | (TileEdit::SkullOwner { .. }, Tile::Skull)
            | (TileEdit::BannerPatterns(_), Tile::Banner)
            | (TileEdit::CommandBlock { .. }, Tile::Command)
            | (TileEdit::Beacon { .. }, Tile::Beacon)
            | (TileEdit::JukeboxDisc(_) | TileEdit::JukeboxClear, Tile::Jukebox)
            | (TileEdit::EndGateway { .. }, Tile::Gateway)
            | (TileEdit::LecternBook(_) | TileEdit::LecternClear, Tile::Lectern)
            | (TileEdit::DecoratedPot(_), Tile::Pot) => {}
            _ => return Err(wrong()),
        }
        match edit {
            TileEdit::SignLines { front, lines } => self.edit_sign_lines(pos, tile, front, lines),
            TileEdit::SignStyle {
                front,
                color,
                glowing,
            } => self.edit_sign_style(pos, tile, front, color, glowing),
            TileEdit::SignWaxed(waxed) => {
                let mut data = self.tile_data_at(pos);
                data.insert(WAXED, Tag::Byte(i8::from(waxed)));
                self.put_tile(pos, tile, data, true);
                Ok(())
            }
            TileEdit::InventorySet { slot, item } => {
                let mut items = self.tile_items(pos, tile).unwrap_or_default();
                let index = usize::try_from(slot)
                    .ok()
                    .filter(|index| *index < items.len())
                    .ok_or_else(|| format!("slot {slot} is outside 0..{}", items.len()))?;
                items[index] = match item {
                    Some(spec) => stack_of(&spec)?,
                    None => None,
                };
                self.store_tile_items(pos, tile, &items);
                Ok(())
            }
            TileEdit::InventoryAdd(spec) => {
                let stack = stack_of(&spec)?.ok_or_else(|| "air cannot be added".to_owned())?;
                let mut left = u32::from(stack.count.get());
                let mut items = self.tile_items(pos, tile).unwrap_or_default();
                for held in items.iter_mut().flatten() {
                    if left > 0 && stackable(held, &stack) {
                        let have = u32::from(held.count.get());
                        let moved = (max_stack(held) - have).min(left);
                        held.count = NonZeroU8::new((have + moved) as u8).unwrap_or(held.count);
                        left -= moved;
                    }
                }
                for slot in items.iter_mut().filter(|slot| slot.is_none()) {
                    if left > 0 {
                        let put = left.min(max_stack(&stack));
                        let mut placed = stack.clone();
                        placed.count = NonZeroU8::new(put as u8).unwrap_or(placed.count);
                        *slot = Some(placed);
                        left -= put;
                    }
                }
                self.store_tile_items(pos, tile, &items);
                if left > 0 {
                    return Err(format!("{left} items did not fit"));
                }
                Ok(())
            }
            TileEdit::InventoryClear => {
                let size = self.tile_items(pos, tile).map_or(0, |items| items.len());
                self.store_tile_items(pos, tile, &vec![None; size]);
                Ok(())
            }
            TileEdit::Furnace(edit) => self.edit_furnace(pos, &edit),
            TileEdit::Spawner(edit) => self.edit_spawner(pos, &edit),
            TileEdit::SkullOwner { name, uuid } => {
                let state = self.world.chunks.block(pos).unwrap_or_default();
                if !is_player_head(state) {
                    return Err("only a player head has an owner".to_owned());
                }
                let mut data = self.tile_data_at(pos);
                data.remove(PROFILE);
                if name.is_some() || uuid.is_some() {
                    let mut profile = Compound::new();
                    if let Some(name) = name {
                        if name.chars().count() > 16 {
                            return Err("a profile name is at most 16 characters".to_owned());
                        }
                        profile.insert("name", Tag::String(name));
                    }
                    if let Some(uuid) = uuid {
                        profile.insert("id", uuid_tag(uuid));
                    }
                    data.insert(PROFILE, Tag::Compound(profile));
                }
                self.put_tile(pos, tile, data, true);
                Ok(())
            }
            TileEdit::BannerPatterns(patterns) => {
                let mut list = Vec::with_capacity(patterns.len());
                for (pattern, color) in &patterns {
                    dye(color)?;
                    if rspigot_data::registries::synced_entries("minecraft:banner_pattern")
                        .is_some_and(|known| !known.contains(&pattern.as_str()))
                    {
                        return Err(format!("{pattern} is not a banner pattern"));
                    }
                    list.push(Tag::Compound(
                        Compound::new()
                            .with("pattern", Tag::String(pattern.clone()))
                            .with(COLOR, Tag::String(color.clone())),
                    ));
                }
                let mut data = self.tile_data_at(pos);
                data.insert(PATTERNS, Tag::List(list));
                self.put_tile(pos, tile, data, true);
                Ok(())
            }
            TileEdit::CommandBlock { command, name } => {
                let mut data = self.command_block_data(pos).cloned().unwrap_or_default();
                if let Some(command) = command {
                    data.insert(COMMAND, Tag::String(command));
                }
                if let Some(name) = name {
                    if name.is_empty() {
                        data.remove(CUSTOM_NAME);
                    } else {
                        data.insert(CUSTOM_NAME, component_to_tag(&resolve::text(&name)));
                    }
                }
                self.store_command_block(pos, data);
                self.send_command_block(pos);
                Ok(())
            }
            TileEdit::Beacon { primary, secondary } => {
                let (primary, secondary) = (effect_key(&primary)?, effect_key(&secondary)?);
                let mut data = self.tile_data_at(pos);
                for (key, effect) in [(PRIMARY, primary), (SECONDARY, secondary)] {
                    match effect {
                        Some(effect) => {
                            data.insert(key, Tag::String(effect.to_owned()));
                        }
                        None => {
                            data.remove(key);
                        }
                    }
                }
                self.put_tile(pos, tile, data, false);
                Ok(())
            }
            TileEdit::JukeboxDisc(spec) => {
                let stack = stack_of(&spec)?.ok_or_else(|| "air is not a disc".to_owned())?;
                self.set_jukebox_record(pos, Some(stack));
                Ok(())
            }
            TileEdit::JukeboxClear => {
                self.set_jukebox_record(pos, None);
                Ok(())
            }
            TileEdit::EndGateway { exit, exact } => {
                let mut data = self.tile_data_at(pos);
                data.insert(EXIT_PORTAL, Tag::IntArray(vec![exit.x, exit.y, exit.z]));
                data.insert(EXACT_TELEPORT, Tag::Byte(i8::from(exact)));
                self.put_tile(pos, tile, data, true);
                Ok(())
            }
            TileEdit::LecternBook(spec) => {
                let stack = stack_of(&spec)?.ok_or_else(|| "air is not a book".to_owned())?;
                self.set_lectern_contents(pos, Some(&stack));
                Ok(())
            }
            TileEdit::LecternClear => {
                self.set_lectern_contents(pos, None);
                Ok(())
            }
            TileEdit::DecoratedPot(sherds) => {
                if sherds.len() != POT_SHERDS {
                    return Err(format!(
                        "a pot has {POT_SHERDS} sides, got {}",
                        sherds.len()
                    ));
                }
                for sherd in &sherds {
                    if rspigot_items::ItemKind::from_name(sherd).is_none() {
                        return Err(format!("unknown item {sherd}"));
                    }
                }
                let mut data = self.tile_data_at(pos);
                data.insert(
                    SHERDS,
                    Tag::List(sherds.into_iter().map(Tag::String).collect()),
                );
                self.put_tile(pos, tile, data, true);
                Ok(())
            }
        }
    }

    fn set_lectern_contents(
        &mut self,
        pos: BlockPos,
        book: Option<&rspigot_domain::item::ItemStack>,
    ) {
        self.store_lectern(pos, book, 0);
        let state = self.world.chunks.block(pos).unwrap_or_default();
        let value = if book.is_some() { "true" } else { "false" };
        if let Some(next) = state.with_property("has_book", value)
            && next != state
        {
            self.set_block_quiet(pos, next);
        }
        self.update_neighbour_for_output_signal(pos);
    }

    fn edit_sign_lines(
        &mut self,
        pos: BlockPos,
        tile: Tile,
        front: bool,
        lines: Vec<String>,
    ) -> Result<(), String> {
        if lines.len() > MAX_SIGN_LINES {
            return Err(format!(
                "a sign has {MAX_SIGN_LINES} lines, got {}",
                lines.len()
            ));
        }
        let key = if front { FRONT_TEXT } else { BACK_TEXT };
        let mut data = self.tile_data_at(pos);
        let mut text = data.get_compound(key).cloned().unwrap_or_default();
        let old: Vec<Tag> = text
            .get_list(MESSAGES)
            .map(<[Tag]>::to_vec)
            .unwrap_or_default();
        let messages = (0..MAX_SIGN_LINES)
            .map(|index| {
                sign::line_tag(old.get(index), lines.get(index).map_or("", String::as_str))
            })
            .collect();
        text.insert(MESSAGES, Tag::List(messages));
        text.remove("filtered_messages");
        data.insert(key, Tag::Compound(text));
        self.put_tile(pos, tile, data, true);
        Ok(())
    }

    fn edit_sign_style(
        &mut self,
        pos: BlockPos,
        tile: Tile,
        front: bool,
        color: Option<String>,
        glowing: Option<bool>,
    ) -> Result<(), String> {
        let key = if front { FRONT_TEXT } else { BACK_TEXT };
        let mut data = self.tile_data_at(pos);
        let mut text = data.get_compound(key).cloned().unwrap_or_default();
        if let Some(color) = color {
            text.insert(COLOR, Tag::String(dye(&color)?.to_owned()));
        }
        if let Some(glowing) = glowing {
            text.insert(GLOWING, Tag::Byte(i8::from(glowing)));
        }
        data.insert(key, Tag::Compound(text));
        self.put_tile(pos, tile, data, true);
        Ok(())
    }

    fn edit_furnace(&mut self, pos: BlockPos, edit: &FurnaceEdit) -> Result<(), String> {
        for (what, value) in [
            ("burn time", edit.burn_time),
            ("cook time", edit.cook_time),
            ("cook time total", edit.cook_time_total),
        ] {
            if let Some(value) = value {
                resolve::not_negative(what, value)?;
            }
        }
        let mut furnace = self
            .furnace_at(pos)
            .ok_or_else(|| "the furnace has no data".to_owned())?;
        if let Some(burn) = edit.burn_time {
            furnace.lit_time_remaining = burn;
            furnace.lit_total_time = furnace.lit_total_time.max(burn);
        }
        if let Some(cook) = edit.cook_time {
            furnace.cooking_time_spent = cook;
        }
        if let Some(total) = edit.cook_time_total {
            furnace.cooking_total_time = total;
        }
        self.store_furnace(pos, &furnace);
        Ok(())
    }

    fn edit_spawner(&mut self, pos: BlockPos, edit: &SpawnerEdit) -> Result<(), String> {
        let mut spawner = self.spawner_at(pos).ok_or("the block is not a spawner")?;
        if let Some(key) = &edit.entity {
            EntityKind::from_name(key).ok_or_else(|| format!("unknown entity type {key}"))?;
            spawner.next = Some(SpawnData::of_entity(key));
            spawner.potentials.clear();
        }
        for (what, value, slot) in [
            ("delay", edit.delay, &mut spawner.delay),
            ("minimum delay", edit.min_delay, &mut spawner.min_delay),
            ("maximum delay", edit.max_delay, &mut spawner.max_delay),
            ("spawn count", edit.spawn_count, &mut spawner.count),
            ("maximum nearby", edit.max_nearby, &mut spawner.max_nearby),
            (
                "required player range",
                edit.required_player_range,
                &mut spawner.player_range,
            ),
            ("spawn range", edit.spawn_range, &mut spawner.range),
        ] {
            if let Some(value) = value {
                resolve::not_negative(what, value)?;
                *slot = value;
            }
        }
        if spawner.max_delay <= 0 || spawner.min_delay > spawner.max_delay {
            return Err("the minimum spawn delay must not exceed the maximum, above 0".to_owned());
        }
        self.store_spawner(pos, &spawner);
        self.send_spawner_update(pos);
        Ok(())
    }

    pub(super) fn tile_view(&self, pos: BlockPos) -> Result<Option<TileView>, String> {
        let Some(state) = self.world.chunks.block(pos) else {
            return Ok(None);
        };
        let Some(tile) = self.tile_at(pos) else {
            return Err("the block has no tile entity the SDK knows".to_owned());
        };
        let data = self.tile_data_at(pos);
        let name = tile_name(tile);
        let view = match tile {
            Tile::Sign(_) => TileData::Sign(SignView {
                front: sign::plain_lines(data.get_compound(FRONT_TEXT)),
                back: sign::plain_lines(data.get_compound(BACK_TEXT)),
                waxed: data.get_bool(WAXED).unwrap_or(false),
                front_color: side_color(data.get_compound(FRONT_TEXT)),
                back_color: side_color(data.get_compound(BACK_TEXT)),
                front_glowing: side_glowing(data.get_compound(FRONT_TEXT)),
                back_glowing: side_glowing(data.get_compound(BACK_TEXT)),
            }),
            Tile::Storage(..) | Tile::Plain(..) => {
                let items = self.tile_items(pos, tile).unwrap_or_default();
                TileData::Inventory(ContentsView {
                    size: items.len(),
                    slots: spec_slots(&items),
                })
            }
            Tile::Furnace => {
                let furnace = self.furnace_at(pos).unwrap_or_default();
                let items = self.tile_items(pos, tile).unwrap_or_default();
                TileData::Furnace(FurnaceView {
                    burn_time: furnace.lit_time_remaining,
                    cook_time: furnace.cooking_time_spent,
                    cook_time_total: furnace.cooking_total_time,
                    lit: state.property("lit") == Some("true"),
                    contents: ContentsView {
                        size: items.len(),
                        slots: spec_slots(&items),
                    },
                })
            }
            Tile::Spawner => {
                let spawner = self.spawner_at(pos).unwrap_or_default();
                let entity = spawner
                    .next
                    .as_ref()
                    .or_else(|| spawner.potentials.first().map(|(data, _)| data))
                    .and_then(SpawnData::entity_id)
                    .unwrap_or_default()
                    .to_owned();
                TileData::Spawner(SpawnerView {
                    entity,
                    delay: spawner.delay,
                    min_delay: spawner.min_delay,
                    max_delay: spawner.max_delay,
                    spawn_count: spawner.count,
                    max_nearby: spawner.max_nearby,
                    required_player_range: spawner.player_range,
                    spawn_range: spawner.range,
                })
            }
            Tile::Skull => {
                let profile = data.get_compound(PROFILE);
                TileData::Skull {
                    name: profile
                        .and_then(|profile| profile.get_str("name"))
                        .map(str::to_owned),
                    uuid: profile
                        .and_then(|profile| profile.get("id"))
                        .and_then(read_uuid),
                }
            }
            Tile::Banner => TileData::Banner(
                data.get_list(PATTERNS)
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|entry| {
                        let entry = entry.as_compound()?;
                        Some((
                            entry.get_str("pattern")?.to_owned(),
                            entry.get_str(COLOR)?.to_owned(),
                        ))
                    })
                    .collect(),
            ),
            Tile::Command => {
                let data = self.command_block_data(pos).cloned().unwrap_or_default();
                TileData::CommandBlock {
                    command: data.get_str(COMMAND).unwrap_or_default().to_owned(),
                    name: data
                        .get(CUSTOM_NAME)
                        .and_then(component_from_tag)
                        .map(|name| name.to_legacy())
                        .unwrap_or_default(),
                }
            }
            Tile::Beacon => TileData::Beacon {
                levels: data.get_i32(LEVELS).unwrap_or(0),
                primary: data.get_str(PRIMARY).map(str::to_owned),
                secondary: data.get_str(SECONDARY).map(str::to_owned),
            },
            Tile::Jukebox => TileData::Jukebox(self.jukebox_record(pos).as_ref().map(spec_of)),
            Tile::Lectern => TileData::Lectern(self.lectern_book(pos).as_ref().map(spec_of)),
            Tile::Gateway => TileData::EndGateway {
                exit: match data.get_int_array(EXIT_PORTAL) {
                    Some(&[x, y, z]) => Some(BlockPos::new(x, y, z)),
                    _ => None,
                },
                exact: data.get_bool(EXACT_TELEPORT).unwrap_or(false),
            },
            Tile::Pot => TileData::DecoratedPot(
                data.get_list(SHERDS)
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|tag| match tag {
                        Tag::String(item) => Some(item.clone()),
                        _ => None,
                    })
                    .collect(),
            ),
        };
        Ok(Some(TileView {
            kind: format!("minecraft:{name}"),
            data: view,
        }))
    }

    pub(super) fn block_info(&self, pos: BlockPos) -> Option<BlockInfo> {
        self.world.chunks.block(pos)?;
        let chunk = self.world.chunks.get(pos.chunk())?;
        let sky = if self.world.dimension.has_skylight {
            crate::lighting::light_level(chunk, pos, true)
        } else {
            0
        };
        let block = crate::lighting::light_level(chunk, pos, false);
        Some(BlockInfo {
            light: sky.max(block),
            sky_light: sky,
            block_light: block,
            power: self.best_neighbor_signal(pos, true),
            biome: rspigot_data::registries::biome_name(self.biome_at(pos).0)
                .unwrap_or("minecraft:plains")
                .to_owned(),
        })
    }
}

fn edit_target(edit: &TileEdit) -> &'static str {
    match edit {
        TileEdit::SignLines { .. } | TileEdit::SignStyle { .. } | TileEdit::SignWaxed(_) => "sign",
        TileEdit::InventorySet { .. } | TileEdit::InventoryAdd(_) | TileEdit::InventoryClear => {
            "container"
        }
        TileEdit::Furnace(_) => "furnace",
        TileEdit::Spawner(_) => "spawner",
        TileEdit::SkullOwner { .. } => "skull",
        TileEdit::BannerPatterns(_) => "banner",
        TileEdit::CommandBlock { .. } => "command block",
        TileEdit::Beacon { .. } => "beacon",
        TileEdit::JukeboxDisc(_) | TileEdit::JukeboxClear => "jukebox",
        TileEdit::EndGateway { .. } => "end gateway",
        TileEdit::LecternBook(_) | TileEdit::LecternClear => "lectern",
        TileEdit::DecoratedPot(_) => "decorated pot",
    }
}

fn side_color(text: Option<&Compound>) -> String {
    text.and_then(|text| text.get_str(COLOR))
        .unwrap_or("black")
        .to_owned()
}

fn side_glowing(text: Option<&Compound>) -> bool {
    text.and_then(|text| text.get_bool(GLOWING))
        .unwrap_or(false)
}
