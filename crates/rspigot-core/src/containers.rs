use rand::Rng;
use rspigot_blocks::{BlockKind, ShapeId};
use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::container_menu::{ContainerMenu, MenuKind};
use rspigot_domain::entity::physics::has_tag;
use rspigot_domain::entity::save::{item_stack_from_nbt, item_stack_to_nbt};
use rspigot_domain::item::{Inventory, ItemStack, Slot, split};
use rspigot_domain::menu::{Clicker, MenuEffect, RemoteUpdate};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::spawn::placement::is_redstone_conductor;
use rspigot_domain::text::Component;
use rspigot_domain::{BlockPos, BlockStateId, Identifier, Vec3};
use rspigot_entities::SoundSource;
use rspigot_nbt::{Compound, Tag};

use crate::chests::{self, Oxidation};
use crate::events::{ClientEvent, ContainerClick};
use crate::game_events::GameEventContext;
use crate::menu::Move;
use crate::player::Player;
use crate::plugins::views::Window;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use crate::{beacon, campfire, cooking, lectern};
use rspigot_domain::game_event::GameEvent;

const SMITHING_TABLE_USE: &str = "minecraft:block.smithing_table.use";
const STONECUTTER_TAKE_RESULT: &str = "minecraft:ui.stonecutter.take_result";
const WINDOWS: i32 = 100;
const MAX_VIEW_DISTANCE_SQUARED: f64 = 64.0;
const ITEMS: &str = "Items";
const BLOCK_EVENT_RANGE: f64 = 64.0;
const LID_ACTION: u8 = 1;
const LID_VOLUME: f32 = 0.5;
const ITEM_WIDTH: f64 = 0.25;
const MIN_DROPPED_PIECE: u32 = 10;
const MAX_DROPPED_PIECE: u32 = 30;
const DROP_SPREAD: f64 = 0.114_850_001_711_397_36;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lid {
    None,
    Chest,
    CopperChest(Oxidation),
    Ender,
    Shulker,
    Barrel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Backing {
    pub pos: BlockPos,
    pub second: Option<BlockPos>,
    pub entity: Option<&'static str>,
    pub ender: bool,
    pub block: BlockKind,
    pub lid: Lid,
}

impl Backing {
    pub(crate) fn positions(&self) -> impl Iterator<Item = BlockPos> {
        std::iter::once(self.pos).chain(self.second)
    }

    fn includes(&self, pos: BlockPos) -> bool {
        self.positions().any(|candidate| candidate == pos)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OpenMenu {
    pub window: i32,
    pub inventory: u64,
    pub title: &'static str,
    pub menu: ContainerMenu,
    pub backing: Backing,
    pub state_id: i32,
    pub local: Vec<Slot>,
    pub data: Vec<i16>,
    pub trader: Option<rspigot_domain::EntityId>,
}

pub(crate) fn next_window<L>(player: &mut Player<L>) -> i32 {
    player.window_counter = player.window_counter % WINDOWS as u8 + 1;
    i32::from(player.window_counter)
}

fn next_state(state_id: &mut i32) -> i32 {
    *state_id = state_id.wrapping_add(1) & 0x7FFF;
    *state_id
}

pub(crate) fn sync_menu_view<L: ClientLink>(
    link: &L,
    window: i32,
    state_id: &mut i32,
    menu: &mut ContainerMenu,
    items: &[Slot],
    inventory: &Inventory,
    full: bool,
) {
    if full {
        let (slots, carried) = menu.sync_all(items, inventory);
        link.send(ClientEvent::ContainerContent {
            window,
            state_id: next_state(state_id),
            slots,
            carried,
        });
        return;
    }
    for update in menu.changes(items, inventory) {
        match update {
            RemoteUpdate::Slot(slot, stack) => link.send(ClientEvent::ContainerSlot {
                window,
                state_id: next_state(state_id),
                slot: slot as i16,
                stack,
            }),
            RemoteUpdate::Carried(stack) => link.send(ClientEvent::CursorItem(stack)),
        }
    }
}

struct Opening {
    kind: MenuKind,
    entity: Option<&'static str>,
    ender: bool,
    title: &'static str,
    block: BlockKind,
    lid: Lid,
}

pub(crate) fn opens_menu(state: BlockStateId) -> bool {
    opening_of(state).is_some()
}

fn opening_of(state: BlockStateId) -> Option<Opening> {
    let block = state.kind()?;
    let storage = |kind, entity: &'static str, title, lid| {
        Some(Opening {
            kind,
            entity: Some(entity),
            ender: false,
            title,
            block,
            lid,
        })
    };
    let station = |kind, entity: Option<&'static str>, title| {
        Some(Opening {
            kind,
            entity,
            ender: false,
            title,
            block,
            lid: Lid::None,
        })
    };
    let rows = MenuKind::Generic { rows: 3 };
    if let Some(oxidation) = chests::oxidation_of(block) {
        return storage(
            rows,
            "chest",
            "container.chest",
            Lid::CopperChest(oxidation),
        );
    }
    match block {
        BlockKind::Chest => storage(rows, "chest", "container.chest", Lid::Chest),
        BlockKind::EnderChest => Some(Opening {
            kind: rows,
            entity: None,
            ender: true,
            title: "container.enderchest",
            block,
            lid: Lid::Ender,
        }),
        BlockKind::TrappedChest => storage(rows, "trapped_chest", "container.chest", Lid::Chest),
        BlockKind::Barrel => storage(rows, "barrel", "container.barrel", Lid::Barrel),
        BlockKind::Hopper => storage(MenuKind::Hopper, "hopper", "container.hopper", Lid::None),
        BlockKind::Dispenser => storage(
            MenuKind::Generic3x3,
            "dispenser",
            "container.dispenser",
            Lid::None,
        ),
        BlockKind::Dropper => storage(
            MenuKind::Generic3x3,
            "dropper",
            "container.dropper",
            Lid::None,
        ),
        BlockKind::CraftingTable => station(MenuKind::Crafting, None, "container.crafting"),
        BlockKind::Grindstone => station(MenuKind::Grindstone, None, "container.grindstone_title"),
        BlockKind::Stonecutter => station(MenuKind::Stonecutter, None, "container.stonecutter"),
        BlockKind::CartographyTable => station(
            MenuKind::CartographyTable,
            None,
            "container.cartography_table",
        ),
        BlockKind::Loom => station(MenuKind::Loom, None, "container.loom"),
        BlockKind::Beacon => station(MenuKind::Beacon, None, "container.beacon"),
        BlockKind::Lectern if lectern::has_book(state) => Some(Opening {
            kind: MenuKind::Lectern,
            entity: Some("lectern"),
            ender: false,
            title: "container.lectern",
            block,
            lid: Lid::None,
        }),
        BlockKind::SmithingTable => station(MenuKind::Smithing, None, "container.upgrade"),
        _ if has_tag(state, "minecraft:anvil") => {
            station(MenuKind::Anvil, None, "container.repair")
        }
        BlockKind::Furnace => station(MenuKind::Furnace, Some("furnace"), "container.furnace"),
        BlockKind::BlastFurnace => station(
            MenuKind::BlastFurnace,
            Some("blast_furnace"),
            "container.blast_furnace",
        ),
        BlockKind::Smoker => station(MenuKind::Smoker, Some("smoker"), "container.smoker"),
        _ if has_tag(state, "minecraft:shulker_boxes") => storage(
            MenuKind::ShulkerBox,
            "shulker_box",
            "container.shulkerBox",
            Lid::Shulker,
        ),
        _ => None,
    }
}

pub(crate) fn container_entity(state: BlockStateId) -> Option<&'static str> {
    if campfire::is_campfire(state) {
        return Some("campfire");
    }
    if lectern::is_lectern(state) {
        return Some("lectern");
    }
    if beacon::is_beacon(state) {
        return Some("beacon");
    }
    opening_of(state).and_then(|opening| {
        if opening.ender {
            Some("ender_chest")
        } else {
            opening.entity
        }
    })
}

fn facing_step(state: BlockStateId) -> (i32, i32, i32) {
    match state.property("facing") {
        Some("down") => (0, -1, 0),
        Some("north") => (0, 0, -1),
        Some("south") => (0, 0, 1),
        Some("west") => (-1, 0, 0),
        Some("east") => (1, 0, 0),
        _ => (0, 1, 0),
    }
}

fn lid_sounds(lid: Lid) -> Option<(&'static str, &'static str)> {
    match lid {
        Lid::Chest => Some(("minecraft:block.chest.open", "minecraft:block.chest.close")),
        Lid::CopperChest(Oxidation::Weathered) => Some((
            "minecraft:block.copper_chest_weathered.open",
            "minecraft:block.copper_chest_weathered.close",
        )),
        Lid::CopperChest(Oxidation::Oxidized) => Some((
            "minecraft:block.copper_chest_oxidized.open",
            "minecraft:block.copper_chest_oxidized.close",
        )),
        Lid::CopperChest(Oxidation::Unaffected | Oxidation::Exposed) => Some((
            "minecraft:block.copper_chest.open",
            "minecraft:block.copper_chest.close",
        )),
        Lid::Ender => Some((
            "minecraft:block.ender_chest.open",
            "minecraft:block.ender_chest.close",
        )),
        Lid::Shulker => Some((
            "minecraft:block.shulker_box.open",
            "minecraft:block.shulker_box.close",
        )),
        Lid::Barrel => Some((
            "minecraft:block.barrel.open",
            "minecraft:block.barrel.close",
        )),
        Lid::None => None,
    }
}

pub(crate) fn read_items(data: &Compound, size: usize) -> Vec<Slot> {
    read_items_at(data, ITEMS, size)
}

pub(crate) fn read_items_at(data: &Compound, key: &str, size: usize) -> Vec<Slot> {
    let mut items = vec![None; size];
    for entry in data.get_list(key).unwrap_or_default() {
        let Some(entry) = entry.as_compound() else {
            continue;
        };
        let slot = entry
            .get_i64("Slot")
            .and_then(|slot| usize::try_from(slot).ok())
            .filter(|slot| *slot < size);
        if let (Some(slot), Some(stack)) = (slot, item_stack_from_nbt(entry)) {
            items[slot] = Some(stack);
        }
    }
    items
}

pub(crate) fn write_items(data: &mut Compound, items: &[Slot]) {
    write_items_at(data, ITEMS, items);
}

pub(crate) fn write_items_at(data: &mut Compound, key: &str, items: &[Slot]) {
    let list = items
        .iter()
        .enumerate()
        .filter_map(|(slot, stack)| {
            let mut entry = item_stack_to_nbt(stack.as_ref()?);
            entry.insert("Slot", slot as i8);
            Some(Tag::Compound(entry))
        })
        .collect();
    data.insert(key, Tag::List(list));
}

fn blocked_above(state_above: Option<BlockStateId>) -> bool {
    state_above.is_some_and(is_redstone_conductor)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn ensure_container_entity(&mut self, pos: BlockPos, state: BlockStateId) {
        let Some(entity) = container_entity(state) else {
            return;
        };
        let present = self
            .world
            .chunks
            .get(pos.chunk())
            .is_some_and(|chunk| chunk.block_entity(pos).is_some());
        if present {
            return;
        }
        if let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) {
            chunk.set_block_entity(BlockEntity {
                pos,
                kind: Identifier::minecraft(entity),
                data: Compound::new(),
            });
        }
    }

    pub(crate) fn contents_lost_by(
        &self,
        pos: BlockPos,
        old: BlockStateId,
        new: BlockStateId,
    ) -> Vec<ItemStack> {
        if old.kind() == new.kind() {
            return Vec::new();
        }
        if campfire::is_campfire(old) {
            return self.campfire_items_lost_by(pos, old, new);
        }
        let Some(opening) = opening_of(old).filter(|opening| opening.entity.is_some()) else {
            return Vec::new();
        };
        self.half_items(pos, opening.kind.container_size())
            .into_iter()
            .flatten()
            .collect()
    }

    pub(crate) fn drop_contents(&mut self, pos: BlockPos, stacks: Vec<ItemStack>) {
        for stack in stacks {
            self.drop_item_stack(pos, stack);
        }
    }

    fn drop_item_stack(&mut self, pos: BlockPos, stack: ItemStack) {
        let room = 1.0 - ITEM_WIDTH;
        let position = Vec3::new(
            f64::from(pos.x) + self.rng.random::<f64>() * room + ITEM_WIDTH / 2.0,
            f64::from(pos.y) + self.rng.random::<f64>() * room,
            f64::from(pos.z) + self.rng.random::<f64>() * room + ITEM_WIDTH / 2.0,
        );
        let mut rest = Some(stack);
        while rest.is_some() {
            let amount = self.rng.random_range(MIN_DROPPED_PIECE..=MAX_DROPPED_PIECE);
            let Some(piece) = split(&mut rest, amount) else {
                break;
            };
            let mut triangle = |center: f64| {
                center + DROP_SPREAD * (self.rng.random::<f64>() - self.rng.random::<f64>())
            };
            let velocity = Vec3::new(triangle(0.0), triangle(0.2), triangle(0.0));
            self.spawn_item_entity(position, velocity, 0, piece);
        }
    }

    fn half_items(&self, pos: BlockPos, size: usize) -> Vec<Slot> {
        self.world
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .map_or_else(|| vec![None; size], |entity| read_items(&entity.data, size))
    }

    fn container_items(&self, backing: Backing, size: usize) -> Vec<Slot> {
        if backing.block == BlockKind::Lectern {
            return vec![self.lectern_book(backing.pos)];
        }
        match backing.second {
            Some(second) => {
                let half = size / 2;
                let mut items = self.half_items(backing.pos, half);
                items.extend(self.half_items(second, half));
                items
            }
            None => self.half_items(backing.pos, size),
        }
    }

    pub(crate) fn open_items(&self, open: &OpenMenu) -> Vec<Slot> {
        let size = open.menu.kind().container_size();
        if open.backing.entity.is_some() {
            self.container_items(open.backing, size)
        } else {
            let mut local = open.local.clone();
            local.resize(size, None);
            local
        }
    }

    fn store_half(&mut self, pos: BlockPos, entity: &'static str, items: &[Slot]) {
        let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) else {
            return;
        };
        let mut data = chunk
            .block_entity(pos)
            .map_or_else(Compound::new, |existing| existing.data.clone());
        write_items(&mut data, items);
        chunk.set_block_entity(BlockEntity {
            pos,
            kind: Identifier::minecraft(entity),
            data,
        });
    }

    fn store_container(&mut self, backing: Backing, items: &[Slot]) {
        let Some(entity) = backing.entity else {
            return;
        };
        match backing.second {
            Some(second) => {
                let half = items.len() / 2;
                self.store_half(backing.pos, entity, &items[..half]);
                self.store_half(second, entity, &items[half..]);
            }
            None => self.store_half(backing.pos, entity, items),
        }
    }

    pub(crate) fn open_block_menu(
        &mut self,
        id: PlayerId,
        pos: BlockPos,
        state: BlockStateId,
    ) -> bool {
        let Some(opening) = self.opening_at(pos, state) else {
            return false;
        };
        let Some(mut open) = opening else {
            return true;
        };
        self.close_open_menu(id, true);
        open.inventory = self.inventory_of(open.backing);
        let guarded = open.backing.lid != Lid::None;
        self.ask_open(id, pos, open);
        if guarded {
            self.anger_nearby_piglins(id, true);
        }
        true
    }

    pub(crate) fn opening_at(
        &self,
        pos: BlockPos,
        state: BlockStateId,
    ) -> Option<Option<OpenMenu>> {
        let mut opening = opening_of(state)?;
        let viewers = self.viewer_count(pos);
        let shut = match opening.lid {
            Lid::Chest | Lid::CopperChest(_) | Lid::Ender => {
                blocked_above(self.world.chunks.block(pos.offset(0, 1, 0)))
            }
            Lid::Shulker => viewers == 0 && self.shulker_is_obstructed(pos, state),
            Lid::Barrel | Lid::None => false,
        };
        if shut {
            return Some(None);
        }
        let (mut first, mut second) = (pos, None);
        let chunks = &self.world.chunks;
        if let Some((right, left)) = chests::pair_of(pos, state, |at| chunks.block(at)) {
            let partner = if pos == right { left } else { right };
            if !blocked_above(chunks.block(partner.offset(0, 1, 0))) {
                (first, second) = (right, Some(left));
                opening.kind = MenuKind::Generic { rows: 6 };
                opening.title = "container.chestDouble";
            }
        }
        let backing = Backing {
            pos: first,
            second,
            entity: opening.entity,
            ender: opening.ender,
            block: opening.block,
            lid: opening.lid,
        };
        Some(Some(OpenMenu {
            window: 0,
            inventory: 0,
            title: opening.title,
            menu: ContainerMenu::new(opening.kind),
            backing,
            state_id: 0,
            local: Vec::new(),
            data: Vec::new(),
            trader: None,
        }))
    }

    fn inventory_of(&mut self, backing: Backing) -> u64 {
        let shared = backing.entity.is_some().then(|| {
            self.players.values().find_map(|player| {
                player
                    .open_menu
                    .as_ref()
                    .filter(|open| open.backing.entity.is_some() && open.backing.pos == backing.pos)
                    .map(|open| open.inventory)
            })
        });
        shared.flatten().unwrap_or_else(|| self.next_inventory_id())
    }

    pub(crate) fn opening_in_reach(&self, id: PlayerId, pos: BlockPos, open: &OpenMenu) -> bool {
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let current = self
            .world
            .chunks
            .block(pos)
            .and_then(|state| self.opening_at(pos, state))
            .flatten();
        let same = current.is_some_and(|current| {
            current.backing == open.backing && current.menu.kind() == open.menu.kind()
        });
        same && !player.vitals.is_dead()
            && player.open_menu.is_none()
            && player.plugin_menu.is_none()
            && open.backing.positions().all(|at| {
                player.position.distance_squared(at.center()) <= MAX_VIEW_DISTANCE_SQUARED
            })
    }

    pub(crate) fn show_block_menu(&mut self, id: PlayerId, pos: BlockPos, mut open: OpenMenu) {
        let backing = open.backing;
        let kind = open.menu.kind();
        let size = kind.container_size();
        let items = if backing.entity.is_some() {
            self.container_items(backing, size)
        } else if backing.ender {
            let mut own = self
                .players
                .get(&id)
                .map_or_else(Vec::new, |player| player.ender_items.clone());
            own.resize(size, None);
            own
        } else {
            vec![None; size]
        };
        let initial_data = match kind {
            MenuKind::Lectern => vec![i16::try_from(self.lectern_page(pos)).unwrap_or(0)],
            MenuKind::Beacon => self.beacon_menu_data(pos),
            kind if kind.is_furnace() => self.furnace_data(pos),
            kind => vec![0; kind.data_count()],
        };
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let window = next_window(player);
        open.window = window;
        open.local = if backing.entity.is_some() {
            Vec::new()
        } else {
            items.clone()
        };
        open.data = initial_data.clone();
        player.link.send(ClientEvent::OpenScreen {
            window,
            menu: kind.protocol_id(),
            title: Component::translatable(open.title, Vec::new()),
        });
        sync_menu_view(
            &player.link,
            window,
            &mut open.state_id,
            &mut open.menu,
            &items,
            &player.inventory,
            true,
        );
        for (property, value) in initial_data.into_iter().enumerate() {
            player.link.send(ClientEvent::ContainerData {
                window,
                id: property as i16,
                value,
            });
        }
        player.open_menu = Some(open);
        let count = self.viewer_count(pos);
        self.viewers_changed(backing, count, true);
    }

    fn viewer_count(&self, pos: BlockPos) -> u8 {
        let count = self
            .players
            .values()
            .filter(|player| {
                player.open_menu.as_ref().is_some_and(|open| {
                    open.backing.includes(pos)
                        && (open.backing.entity.is_some() || open.backing.ender)
                })
            })
            .count();
        u8::try_from(count).unwrap_or(u8::MAX)
    }

    fn shulker_is_obstructed(&self, pos: BlockPos, state: BlockStateId) -> bool {
        let (dx, dy, dz) = facing_step(state);
        self.world
            .chunks
            .block(pos.offset(dx, dy, dz))
            .is_some_and(|neighbour| neighbour.collision_shape_id() != ShapeId::EMPTY)
    }

    fn viewers_changed(&mut self, backing: Backing, count: u8, opened: bool) {
        let Some(state) = self.world.chunks.block(backing.pos) else {
            return;
        };
        if state.kind() != Some(backing.block) {
            return;
        }
        let first_or_last = if opened { count == 1 } else { count == 0 };
        if first_or_last {
            let event = if opened {
                GameEvent::ContainerOpen
            } else {
                GameEvent::ContainerClose
            };
            self.block_game_event(event, backing.pos, GameEventContext::block(state));
        }
        let center = backing.pos.center() + rspigot_domain::Vec3::new(0.0, 0.5, 0.0);
        let mut position = center;
        if backing.lid == Lid::Barrel {
            let (dx, dy, dz) = facing_step(state);
            position = center
                + rspigot_domain::Vec3::new(
                    f64::from(dx) / 2.0,
                    f64::from(dy) / 2.0,
                    f64::from(dz) / 2.0,
                );
            if first_or_last
                && let Some(next) =
                    state.with_property("open", if opened { "true" } else { "false" })
            {
                self.set_block(backing.pos, next);
            }
        } else if backing.lid != Lid::None {
            if backing.second.is_some()
                && let Some(dir) = chests::connected_direction(state)
            {
                let (dx, dz) = dir.step();
                position = center
                    + rspigot_domain::Vec3::new(f64::from(dx) / 2.0, 0.0, f64::from(dz) / 2.0);
            }
            for half in backing.positions() {
                self.push(
                    Audience::Nearby {
                        position: half.center(),
                        range: BLOCK_EVENT_RANGE,
                        except: None,
                    },
                    ClientEvent::BlockEvent {
                        pos: half,
                        action: LID_ACTION,
                        data: count,
                        block: backing.block.id().0,
                    },
                );
            }
        }
        if first_or_last && let Some((open, close)) = lid_sounds(backing.lid) {
            let pitch = self.rng.random::<f32>() * 0.1 + 0.9;
            let sound = if opened { open } else { close };
            self.play_sound(
                None,
                position,
                sound,
                SoundSource::Blocks,
                LID_VOLUME,
                pitch,
            );
        }
    }

    pub(crate) fn apply_container_click(
        &mut self,
        id: PlayerId,
        click: &ContainerClick,
        act: Option<Move>,
        full: bool,
    ) -> Slot {
        let player = self.players.get(&id)?;
        let open = player.open_menu.as_ref()?;
        let backing = open.backing;
        let lectern = backing.block == BlockKind::Lectern;
        let act = act.filter(|_| !lectern);
        let clicker = Clicker {
            infinite_materials: player.game_mode == GameMode::Creative,
        };
        let full = full || click.state_id != open.state_id || lectern;
        let mut items = self.open_items(open);
        let before = items.clone();
        let player = self.players.get_mut(&id)?;
        let open = player.open_menu.as_mut()?;
        let (moved, effects) = match act {
            Some(Move::Click) => (
                None,
                open.menu.click(
                    &mut items,
                    &mut player.inventory,
                    click.slot,
                    click.button,
                    click.input,
                    clicker,
                ),
            ),
            Some(Move::Once(slot)) => {
                open.menu
                    .quick_move_once(&mut items, &mut player.inventory, slot, clicker)
            }
            None => (None, Vec::new()),
        };
        for (slot, hash) in &click.changed {
            open.menu.receive_remote(*slot, hash.clone());
        }
        open.menu.receive_remote_carried(click.carried.clone());
        let resync = full
            || effects
                .iter()
                .any(|effect| matches!(effect, MenuEffect::FullResync));
        if act.is_some() {
            self.store_open_items(id, &before, items);
        }
        self.container_effects(id, backing, effects);
        self.sync_open_menu(id, resync);
        self.sync_menu_data();
        self.sync_viewers(backing.pos, Some(id));
        self.sync_inventory(id);
        moved
    }

    pub(crate) fn container_effects(
        &mut self,
        id: PlayerId,
        backing: Backing,
        effects: Vec<MenuEffect>,
    ) {
        let cut = effects.contains(&MenuEffect::StonecutterTaken);
        let smithed = effects.contains(&MenuEffect::SmithingTaken);
        let mut drops = Vec::new();
        let mut taken = Vec::new();
        let mut traded = Vec::new();
        let mut updated = Vec::new();
        for effect in effects {
            match effect {
                MenuEffect::Drop { .. } => drops.push(effect),
                MenuEffect::FurnaceOutputTaken { item, amount } => taken.push((item, amount)),
                MenuEffect::Traded { offer } => traded.push(offer),
                MenuEffect::TradeUpdated { valid } => updated.push(valid),
                _ => {}
            }
        }
        for (item, amount) in taken {
            self.pay_furnace_experience(id, backing.pos, item, amount);
        }
        self.apply_menu_effects(id, drops);
        for offer in traded {
            self.trade_taken(id, offer);
        }
        for valid in updated {
            self.trade_updated_voice(id, valid);
        }
        if cut {
            self.play_sound(
                None,
                backing.pos.center(),
                STONECUTTER_TAKE_RESULT,
                SoundSource::Blocks,
                1.0,
                1.0,
            );
        }
        if smithed {
            let pitch = 0.9 + self.rng.random::<f32>() * 0.1;
            self.play_sound(
                None,
                backing.pos.center(),
                SMITHING_TABLE_USE,
                SoundSource::Blocks,
                1.0,
                pitch,
            );
        }
    }

    pub(crate) fn store_open_items(&mut self, id: PlayerId, before: &[Slot], items: Vec<Slot>) {
        let Some(backing) = self
            .players
            .get(&id)
            .and_then(|player| player.open_menu.as_ref())
            .map(|open| open.backing)
        else {
            return;
        };
        if backing.entity.is_some() {
            self.store_container(backing, &items);
            if let Some(kind) = cooking::cooking_of(backing.block) {
                self.furnace_clicked(backing.pos, kind, before, &items);
            }
        } else if let Some(player) = self.players.get_mut(&id) {
            if backing.ender {
                player.ender_items = items.clone();
            }
            if let Some(open) = player.open_menu.as_mut() {
                open.local = items;
            }
        }
    }

    pub(crate) fn open_menu_items(&self, id: PlayerId) -> Option<Vec<Slot>> {
        let open = self.players.get(&id)?.open_menu.as_ref()?;
        Some(self.open_items(open))
    }

    pub(crate) fn sync_open_menu(&mut self, id: PlayerId, full: bool) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let Some(open) = player.open_menu.as_ref() else {
            return;
        };
        let items = self.open_items(open);
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let Some(open) = player.open_menu.as_mut() else {
            return;
        };
        sync_menu_view(
            &player.link,
            open.window,
            &mut open.state_id,
            &mut open.menu,
            &items,
            &player.inventory,
            full,
        );
    }

    pub(crate) fn sync_viewers(&mut self, pos: BlockPos, except: Option<PlayerId>) {
        let viewers: Vec<PlayerId> = self
            .players
            .iter()
            .filter(|(id, player)| {
                Some(**id) != except
                    && player.open_menu.as_ref().is_some_and(|open| {
                        open.backing.includes(pos) && open.backing.entity.is_some()
                    })
            })
            .map(|(id, _)| *id)
            .collect();
        for viewer in viewers {
            self.sync_open_menu(viewer, false);
        }
    }

    pub(crate) fn close_open_menu(&mut self, id: PlayerId, notify_client: bool) -> bool {
        self.close_open_menu_by(id, notify_client, false)
    }

    fn close_open_menu_by(&mut self, id: PlayerId, notify_client: bool, by_player: bool) -> bool {
        if self.close_plugin_menu(id, notify_client, by_player) {
            return true;
        }
        let Some(window) = self
            .players
            .get(&id)
            .and_then(|player| player.open_menu.as_ref())
            .map(|open| open.window)
        else {
            return false;
        };
        self.notify_close(id, Window::Container(window), by_player);
        let Some(player) = self.players.get_mut(&id) else {
            return false;
        };
        let Some(mut open) = player.open_menu.take() else {
            return false;
        };
        let before = Self::equipment_of(player);
        let effects = if open.backing.entity.is_some() || open.backing.ender {
            open.menu.close(&mut player.inventory)
        } else {
            open.menu
                .close_transient(&mut open.local, &mut player.inventory)
        };
        if notify_client {
            player.link.send(ClientEvent::ContainerClose(open.window));
        }
        self.apply_menu_effects(id, effects);
        self.broadcast_equipment_changes(id, before);
        let count = self.viewer_count(open.backing.pos);
        self.viewers_changed(open.backing, count, false);
        if let Some(trader) = open.trader {
            self.merchant_stop_trading(trader);
        }
        true
    }

    pub(crate) fn handle_open_menu_close(&mut self, id: PlayerId, window: i32) -> bool {
        let matches = self.players.get(&id).is_some_and(|player| {
            player
                .plugin_menu
                .as_ref()
                .is_some_and(|open| open.window == window)
                || player
                    .open_menu
                    .as_ref()
                    .is_some_and(|open| open.window == window)
        });
        if matches {
            self.close_open_menu_by(id, false, true);
        }
        matches
    }

    pub(crate) fn send_menu_data(&self, pos: BlockPos, id: i16, value: i16) {
        for player in self.players.values() {
            if let Some(open) = player
                .open_menu
                .as_ref()
                .filter(|open| open.backing.pos == pos)
            {
                player.link.send(ClientEvent::ContainerData {
                    window: open.window,
                    id,
                    value,
                });
            }
        }
    }

    pub(crate) fn use_up_payment(&mut self, id: PlayerId) {
        if let Some(slot) = self
            .players
            .get_mut(&id)
            .and_then(|player| player.open_menu.as_mut())
            .and_then(|open| open.local.first_mut())
        {
            *slot = None;
        }
        self.sync_open_menu(id, false);
    }

    pub(crate) fn sync_menu_data(&mut self) {
        let viewers: Vec<PlayerId> = self
            .players
            .iter()
            .filter(|(_, player)| {
                player.open_menu.as_ref().is_some_and(|open| {
                    open.menu.kind().is_furnace()
                        || matches!(open.menu.kind(), MenuKind::Stonecutter | MenuKind::Smithing)
                })
            })
            .map(|(id, _)| *id)
            .collect();
        for id in viewers {
            let Some((pos, furnace)) = self
                .players
                .get(&id)
                .and_then(|player| player.open_menu.as_ref())
                .map(|open| (open.backing.pos, open.menu.kind().is_furnace()))
            else {
                continue;
            };
            let furnace_data = furnace.then(|| self.furnace_data(pos));
            let Some(player) = self.players.get_mut(&id) else {
                continue;
            };
            let Some(open) = player.open_menu.as_mut() else {
                continue;
            };
            let current = furnace_data.unwrap_or_else(|| open.menu.data());
            for (index, value) in current.iter().enumerate() {
                if open.data.get(index) != Some(value) {
                    open.data.resize(current.len(), 0);
                    open.data[index] = *value;
                    player.link.send(ClientEvent::ContainerData {
                        window: open.window,
                        id: index as i16,
                        value: *value,
                    });
                }
            }
        }
    }

    pub(crate) fn press_menu_button(&mut self, id: PlayerId, window: i32, button: i32) -> bool {
        let Some(player) = self.players.get_mut(&id) else {
            return false;
        };
        if player.game_mode == GameMode::Spectator {
            return false;
        }
        let Some(open) = player
            .open_menu
            .as_mut()
            .filter(|open| open.window == window && open.menu.kind() == MenuKind::Stonecutter)
        else {
            return false;
        };
        let mut items = open.local.clone();
        open.menu.press(&mut items, button);
        open.local = items;
        self.sync_open_menu(id, false);
        self.sync_menu_data();
        true
    }

    pub(crate) fn tick_open_menus(&mut self) {
        self.recheck_beacon_levels();
        self.sync_menu_data();
        let ids: Vec<PlayerId> = self
            .players
            .iter()
            .filter(|(_, player)| player.open_menu.is_some())
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            if self.open_menu_is_valid(id) {
                self.sync_open_menu(id, false);
            } else {
                self.close_open_menu(id, true);
            }
        }
    }

    pub(crate) fn open_menu_is_valid(&self, id: PlayerId) -> bool {
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let Some(open) = player.open_menu.as_ref() else {
            return false;
        };
        if let Some(trader) = open.trader {
            return !player.vitals.is_dead() && self.trader_menu_valid(id, trader);
        }
        let backing = open.backing;
        let present = backing.positions().all(|at| {
            self.world.chunks.block(at).and_then(BlockStateId::kind) == Some(backing.block)
        });
        let paired = backing.second.is_none_or(|left| {
            self.world.chunks.block(backing.pos).is_some_and(|state| {
                chests::pair_of(backing.pos, state, |at| self.world.chunks.block(at))
                    == Some((backing.pos, left))
            })
        });
        let reads = backing.block != BlockKind::Lectern
            || self
                .world
                .chunks
                .block(backing.pos)
                .is_some_and(lectern::has_book);
        present
            && paired
            && reads
            && !player.vitals.is_dead()
            && backing.positions().all(|at| {
                player.position.distance_squared(at.center()) <= MAX_VIEW_DISTANCE_SQUARED
            })
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU8;

    use rspigot_items::ItemKind;

    use super::*;

    fn stack(name: &str, count: u8) -> rspigot_domain::item::ItemStack {
        let kind = ItemKind::from_name(name).expect("known item");
        rspigot_domain::item::ItemStack::new(kind.id(), NonZeroU8::new(count).expect("non zero"))
    }

    #[test]
    fn items_round_trip_through_the_block_entity_nbt() {
        let mut items = vec![None; 27];
        items[0] = Some(stack("minecraft:diamond", 3));
        items[26] = Some(stack("minecraft:stone", 64));
        let mut data = Compound::new();
        write_items(&mut data, &items);
        assert_eq!(read_items(&data, 27), items);
    }

    #[test]
    fn reading_ignores_slots_outside_the_container_and_unknown_items() {
        let mut data = Compound::new();
        data.insert(
            ITEMS,
            Tag::List(vec![
                Tag::Compound(
                    Compound::new()
                        .with("Slot", 30i8)
                        .with("id", "minecraft:stone")
                        .with("count", 1),
                ),
                Tag::Compound(
                    Compound::new()
                        .with("Slot", 1i8)
                        .with("id", "minecraft:not_an_item")
                        .with("count", 1),
                ),
            ]),
        );
        assert!(read_items(&data, 27).iter().all(Option::is_none));
    }

    #[test]
    fn storage_blocks_map_to_their_menu_and_title() {
        let menu = |block: BlockKind| {
            opening_of(block.default_state()).map(|o| (o.kind, o.entity.unwrap_or(""), o.title))
        };
        assert_eq!(
            menu(BlockKind::Chest),
            Some((MenuKind::Generic { rows: 3 }, "chest", "container.chest"))
        );
        assert_eq!(
            menu(BlockKind::TrappedChest),
            Some((
                MenuKind::Generic { rows: 3 },
                "trapped_chest",
                "container.chest"
            ))
        );
        assert_eq!(
            menu(BlockKind::Barrel),
            Some((MenuKind::Generic { rows: 3 }, "barrel", "container.barrel"))
        );
        assert_eq!(
            menu(BlockKind::Hopper),
            Some((MenuKind::Hopper, "hopper", "container.hopper"))
        );
        assert_eq!(
            menu(BlockKind::Dispenser),
            Some((MenuKind::Generic3x3, "dispenser", "container.dispenser"))
        );
        assert_eq!(
            menu(BlockKind::Dropper),
            Some((MenuKind::Generic3x3, "dropper", "container.dropper"))
        );
        assert!(opening_of(BlockKind::Stone.default_state()).is_none());
    }

    #[test]
    fn every_shulker_box_colour_opens_the_shulker_menu() {
        let boxes: Vec<BlockKind> = BlockKind::ALL
            .into_iter()
            .filter(|block| block.name().ends_with("shulker_box"))
            .collect();
        assert_eq!(boxes.len(), 17);
        for block in boxes {
            let opening =
                opening_of(block.default_state()).unwrap_or_else(|| panic!("{}", block.name()));
            assert_eq!(opening.kind, MenuKind::ShulkerBox, "{}", block.name());
            assert_eq!(opening.entity, Some("shulker_box"), "{}", block.name());
        }
    }

    #[test]
    fn every_copper_chest_opens_a_three_row_chest_menu_on_the_chest_block_entity() {
        for block in chests::COPPER_CHESTS {
            let opening =
                opening_of(block.default_state()).unwrap_or_else(|| panic!("{}", block.name()));
            assert_eq!(
                opening.kind,
                MenuKind::Generic { rows: 3 },
                "{}",
                block.name()
            );
            assert_eq!(opening.entity, Some("chest"), "{}", block.name());
            assert_eq!(opening.title, "container.chest", "{}", block.name());
            assert!(!opening.ender, "{}", block.name());
        }
    }

    #[test]
    fn the_hinge_sounds_follow_the_oxidation_and_ignore_the_wax() {
        let sounds = |block: BlockKind| {
            let opening =
                opening_of(block.default_state()).unwrap_or_else(|| panic!("{}", block.name()));
            lid_sounds(opening.lid)
        };
        let plain = Some((
            "minecraft:block.copper_chest.open",
            "minecraft:block.copper_chest.close",
        ));
        let weathered = Some((
            "minecraft:block.copper_chest_weathered.open",
            "minecraft:block.copper_chest_weathered.close",
        ));
        let oxidized = Some((
            "minecraft:block.copper_chest_oxidized.open",
            "minecraft:block.copper_chest_oxidized.close",
        ));
        assert_eq!(sounds(BlockKind::CopperChest), plain);
        assert_eq!(sounds(BlockKind::ExposedCopperChest), plain);
        assert_eq!(sounds(BlockKind::WaxedCopperChest), plain);
        assert_eq!(sounds(BlockKind::WaxedExposedCopperChest), plain);
        assert_eq!(sounds(BlockKind::WeatheredCopperChest), weathered);
        assert_eq!(sounds(BlockKind::WaxedWeatheredCopperChest), weathered);
        assert_eq!(sounds(BlockKind::OxidizedCopperChest), oxidized);
        assert_eq!(sounds(BlockKind::WaxedOxidizedCopperChest), oxidized);
        assert_eq!(
            sounds(BlockKind::Chest),
            Some(("minecraft:block.chest.open", "minecraft:block.chest.close"))
        );
    }

    #[test]
    fn a_facing_property_gives_the_step_of_the_block() {
        let state = BlockKind::Barrel
            .default_state()
            .with_property("facing", "east")
            .expect("facing");
        assert_eq!(facing_step(state), (1, 0, 0));
        assert_eq!(facing_step(BlockKind::Stone.default_state()), (0, 1, 0));
    }

    #[test]
    fn a_solid_block_above_blocks_the_chest() {
        assert!(blocked_above(Some(BlockKind::Stone.default_state())));
        assert!(!blocked_above(Some(BlockKind::Air.default_state())));
        assert!(!blocked_above(None));
    }
}
