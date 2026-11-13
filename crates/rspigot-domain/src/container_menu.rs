use rspigot_items::{CookingKind, EquipmentSlot, ItemId, ItemKind};

use crate::click::{self, Classified, ClickView, Drag};
use crate::crafting;
use crate::entity::reward::max_damage;
use crate::entity::trading::{MerchantOffer, MerchantOffers};
use crate::furnace;
use crate::item::{
    HOTBAR_START, Inventory, ItemStack, Slot, count, max_stack, menu_slot, same_item,
    same_item_same_components, split, stacks_match, with_count,
};
use crate::menu::{
    ClickInput, Clicker, HashedSlot, Hold, MenuEffect, OUTSIDE, RemoteUpdate, place_back_or_drop,
};
use crate::recipes;
use crate::smithing;
use crate::stonecutting;

const CONTAINER_MAX_STACK: u32 = 99;
const OFFHAND_BUTTON: i32 = 40;
const MAIN_START: usize = 9;
const MAIN_SLOTS: usize = 27;
const HOTBAR_SLOTS: usize = 9;
const SHULKER_BOXES_TAG: &str = "minecraft:shulker_boxes";
const CUTTER_INPUT: usize = 0;
const CUTTER_RESULT: usize = 1;
const CRAFTING_RESULT: usize = 0;
const CRAFTING_GRID: std::ops::RangeInclusive<usize> = 1..=9;
const MERCHANT_RESULT: usize = 2;
const MERCHANT_PLAYER_START: usize = 3;
const MERCHANT_MAIN_END: usize = 30;
const MERCHANT_END: usize = 39;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKind {
    Generic {
        rows: u8,
    },
    Generic3x3,
    Hopper,
    ShulkerBox,
    Crafting,
    Furnace,
    BlastFurnace,
    Smoker,
    Anvil,
    Smithing,
    Grindstone,
    Stonecutter,
    CartographyTable,
    Loom,
    Lectern,
    Beacon,
    Merchant,
    Mount {
        columns: u8,
        saddle: bool,
        armor: bool,
    },
}

const MOUNT_SADDLE_SLOT: usize = 0;
const MOUNT_ARMOR_SLOT: usize = 1;
const MOUNT_EXTRA_SLOTS: usize = 2;
const MOUNT_ROWS: usize = 3;

impl MenuKind {
    pub fn protocol_id(self) -> i32 {
        match self {
            Self::Generic { rows } => i32::from(rows.clamp(1, 6)) - 1,
            Self::Generic3x3 => 6,
            Self::ShulkerBox => 20,
            Self::Hopper => 16,
            Self::Crafting => 12,
            Self::Furnace => 14,
            Self::BlastFurnace => 10,
            Self::Smoker => 22,
            Self::Anvil => 8,
            Self::Smithing => 21,
            Self::Grindstone => 15,
            Self::Stonecutter => 24,
            Self::CartographyTable => 23,
            Self::Loom => 18,
            Self::Lectern => 17,
            Self::Beacon => 9,
            Self::Merchant => 19,
            Self::Mount { .. } => 0,
        }
    }

    pub fn container_size(self) -> usize {
        match self {
            Self::Generic { rows } => usize::from(rows.clamp(1, 6)) * 9,
            Self::Generic3x3 => 9,
            Self::Hopper => 5,
            Self::ShulkerBox => 27,
            Self::Crafting => 10,
            Self::Furnace | Self::BlastFurnace | Self::Smoker => 3,
            Self::Grindstone | Self::CartographyTable => 3,
            Self::Anvil => 3,
            Self::Smithing | Self::Loom => 4,
            Self::Stonecutter => 2,
            Self::Lectern | Self::Beacon => 1,
            Self::Merchant => 3,
            Self::Mount { columns, .. } => MOUNT_EXTRA_SLOTS + MOUNT_ROWS * usize::from(columns),
        }
    }

    pub fn data_count(self) -> usize {
        match self {
            Self::Furnace | Self::BlastFurnace | Self::Smoker => 4,
            Self::Anvil | Self::Smithing | Self::Stonecutter | Self::Loom | Self::Lectern => 1,
            Self::Beacon => 3,
            _ => 0,
        }
    }

    fn station_inputs(self) -> Option<usize> {
        match self {
            Self::Anvil | Self::Grindstone | Self::CartographyTable => Some(2),
            Self::Smithing | Self::Loom => Some(3),
            Self::Stonecutter => Some(1),
            _ => None,
        }
    }

    pub fn is_furnace(self) -> bool {
        self.cooking().is_some()
    }

    pub fn cooking(self) -> Option<CookingKind> {
        match self {
            Self::Furnace => Some(CookingKind::Smelting),
            Self::BlastFurnace => Some(CookingKind::Blasting),
            Self::Smoker => Some(CookingKind::Smoking),
            _ => None,
        }
    }

    fn station_plan(
        self,
        inputs: usize,
        slot: usize,
        stack: &ItemStack,
        container: &[Slot],
    ) -> Vec<(usize, usize, bool)> {
        let total = self.slot_count();
        let main = inputs + 1;
        let hotbar = main + MAIN_SLOTS;
        if slot == inputs {
            return vec![(main, total, true)];
        }
        if slot < inputs {
            return vec![(main, total, false)];
        }
        let swap = || {
            if slot < hotbar {
                vec![(hotbar, total, false)]
            } else {
                vec![(main, hotbar, false)]
            }
        };
        let filled = |index: usize| container.get(index).is_some_and(Option::is_some);
        match self {
            Self::Anvil => vec![(0, inputs, false)],
            Self::Grindstone if filled(0) && filled(1) => swap(),
            Self::Grindstone => vec![(0, 2, false)],
            Self::Stonecutter if stonecutting::accepts(stack.item) => vec![(0, 1, false)],
            Self::Smithing
                if (0..inputs).any(|index| {
                    container.get(index).is_some_and(Option::is_none) && self.accepts(index, stack)
                }) =>
            {
                vec![(0, inputs, false)]
            }
            Self::CartographyTable if is_map(stack) => vec![(0, 1, false)],
            Self::CartographyTable if is_map_material(stack) => vec![(1, 2, false)],
            Self::Loom if is_banner(stack) => vec![(0, 1, false)],
            Self::Loom if is_loom_dye(stack) => vec![(1, 2, false)],
            Self::Loom if is_loom_pattern(stack) => vec![(2, 3, false)],
            _ => swap(),
        }
    }

    fn quick_move_plan(
        self,
        slot: usize,
        stack: &ItemStack,
        container: &[Slot],
    ) -> Vec<(usize, usize, bool)> {
        let total = self.slot_count();
        if let Some(inputs) = self.station_inputs() {
            return self.station_plan(inputs, slot, stack, container);
        }
        match self {
            Self::Mount { .. } => self.mount_plan(slot, stack, container),
            Self::Merchant => match slot {
                MERCHANT_RESULT => vec![(MERCHANT_PLAYER_START, MERCHANT_END, true)],
                0 | 1 => vec![(MERCHANT_PLAYER_START, MERCHANT_END, false)],
                MERCHANT_PLAYER_START..MERCHANT_MAIN_END => {
                    vec![(MERCHANT_MAIN_END, MERCHANT_END, false)]
                }
                _ => vec![(MERCHANT_PLAYER_START, MERCHANT_MAIN_END, false)],
            },
            Self::Beacon => match slot {
                0 => vec![(1, total, true)],
                _ if container.first().is_some_and(Option::is_none)
                    && self.accepts(0, stack)
                    && stack.count.get() == 1 =>
                {
                    vec![(0, 1, false)]
                }
                1..=27 => vec![(28, total, false)],
                _ => vec![(1, 28, false)],
            },
            Self::Crafting => match slot {
                0 => vec![(10, 46, true)],
                1..=9 => vec![(10, 46, false)],
                10..=36 => vec![(1, 10, false), (37, 46, false)],
                _ => vec![(1, 10, false), (10, 37, false)],
            },
            Self::Furnace | Self::BlastFurnace | Self::Smoker => {
                let cooking = self.cooking().unwrap_or(CookingKind::Smelting);
                match slot {
                    2 => vec![(3, total, true)],
                    0 | 1 => vec![(3, total, false)],
                    _ if furnace::can_smelt(cooking, stack) => vec![(0, 1, false)],
                    _ if furnace::is_fuel(stack) => vec![(1, 2, false)],
                    3..=29 => vec![(30, total, false)],
                    _ => vec![(3, 30, false)],
                }
            }
            _ => {
                let size = self.container_size();
                if slot < size {
                    vec![(size, total, true)]
                } else {
                    vec![(0, size, false)]
                }
            }
        }
    }

    fn mount_plan(
        self,
        slot: usize,
        stack: &ItemStack,
        container: &[Slot],
    ) -> Vec<(usize, usize, bool)> {
        let total = self.slot_count();
        let size = self.container_size();
        if slot < size {
            return vec![(size, total, true)];
        }
        let empty = |index: usize| container.get(index).is_some_and(Option::is_none);
        if self.accepts(MOUNT_ARMOR_SLOT, stack) && empty(MOUNT_ARMOR_SLOT) {
            return vec![(MOUNT_ARMOR_SLOT, MOUNT_ARMOR_SLOT + 1, false)];
        }
        if self.accepts(MOUNT_SADDLE_SLOT, stack) && empty(MOUNT_SADDLE_SLOT) {
            return vec![(MOUNT_SADDLE_SLOT, MOUNT_SADDLE_SLOT + 1, false)];
        }
        let mut plan = Vec::new();
        if size > MOUNT_EXTRA_SLOTS {
            plan.push((MOUNT_EXTRA_SLOTS, size, false));
        }
        let hotbar = size + MAIN_SLOTS;
        if slot >= hotbar {
            plan.push((size, hotbar, false));
        } else {
            plan.push((hotbar, total, false));
        }
        plan
    }

    fn clears_on_close(self, slot: usize) -> bool {
        if let Some(inputs) = self.station_inputs() {
            return slot < inputs;
        }
        if self == Self::Beacon {
            return slot == 0;
        }
        if self == Self::Merchant {
            return slot < MERCHANT_RESULT;
        }
        matches!(self, Self::Crafting) && (1..=9).contains(&slot)
    }

    pub fn slot_count(self) -> usize {
        match self {
            Self::Lectern => self.container_size(),
            _ => self.container_size() + MAIN_SLOTS + HOTBAR_SLOTS,
        }
    }

    fn accepts(self, slot: usize, stack: &ItemStack) -> bool {
        if let Some(inputs) = self.station_inputs()
            && slot == inputs
        {
            return false;
        }
        match self {
            Self::Smithing => match slot {
                smithing::TEMPLATE => recipes::contains(recipes::SMITHING_TEMPLATE, stack.item.0),
                smithing::BASE => recipes::contains(recipes::SMITHING_BASE, stack.item.0),
                smithing::ADDITION => recipes::contains(recipes::SMITHING_ADDITION, stack.item.0),
                _ => true,
            },
            Self::Merchant => slot != MERCHANT_RESULT,
            Self::Mount { saddle, armor, .. } => match slot {
                MOUNT_SADDLE_SLOT => {
                    saddle
                        && stack.item.kind().and_then(ItemKind::equippable)
                            == Some(EquipmentSlot::Saddle)
                }
                MOUNT_ARMOR_SLOT => {
                    armor
                        && stack.item.kind().and_then(ItemKind::equippable)
                            == Some(EquipmentSlot::Body)
                }
                _ => true,
            },
            Self::Beacon if slot == 0 => in_item_tag(stack, "minecraft:beacon_payment_items"),
            Self::Grindstone if slot < 2 => is_damageable(stack) || has_enchantments(stack),
            Self::CartographyTable if slot == 0 => is_map(stack),
            Self::CartographyTable if slot == 1 => is_map_material(stack),
            Self::Loom if slot == 0 => is_banner(stack),
            Self::Loom if slot == 1 => is_loom_dye(stack),
            Self::Loom if slot == 2 => is_loom_pattern(stack),
            Self::ShulkerBox if slot < self.container_size() => !is_shulker_box(stack),
            Self::Crafting => slot != 0,
            Self::Furnace | Self::BlastFurnace | Self::Smoker => match slot {
                1 => furnace::is_fuel(stack) || furnace::is_empty_bucket(stack),
                2 => false,
                _ => true,
            },
            _ => true,
        }
    }
}

fn in_item_tag(stack: &ItemStack, tag: &str) -> bool {
    rspigot_data::tags::tag("minecraft:item", tag)
        .is_some_and(|ids| ids.binary_search(&u32::from(stack.item.0)).is_ok())
}

fn is_shulker_box(stack: &ItemStack) -> bool {
    in_item_tag(stack, SHULKER_BOXES_TAG)
}

fn is_damageable(stack: &ItemStack) -> bool {
    max_damage(stack).is_some()
}

fn lists_enchantments(stack: &ItemStack, key: &str) -> bool {
    stack
        .components
        .get_compound(key)
        .is_some_and(|enchantments| {
            enchantments
                .get_compound("levels")
                .map_or(!enchantments.is_empty(), |levels| !levels.is_empty())
        })
}

fn has_enchantments(stack: &ItemStack) -> bool {
    lists_enchantments(stack, "minecraft:enchantments")
        || lists_enchantments(stack, "minecraft:stored_enchantments")
}

fn is_map(stack: &ItemStack) -> bool {
    stack.components.contains_key("minecraft:map_id")
}

fn is_map_material(stack: &ItemStack) -> bool {
    matches!(
        stack.item.kind(),
        Some(ItemKind::Paper | ItemKind::Map | ItemKind::GlassPane)
    )
}

fn is_banner(stack: &ItemStack) -> bool {
    in_item_tag(stack, "minecraft:banners")
}

fn is_loom_dye(stack: &ItemStack) -> bool {
    in_item_tag(stack, "minecraft:loom_dyes") || stack.components.contains_key("minecraft:dye")
}

fn is_loom_pattern(stack: &ItemStack) -> bool {
    in_item_tag(stack, "minecraft:loom_patterns")
        || stack
            .components
            .contains_key("minecraft:provides_banner_patterns")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cell {
    Container(usize),
    Player(usize),
}

#[derive(Debug, Clone, PartialEq)]
enum RemoteSlot {
    Known(Slot),
    Hashed(HashedSlot),
}

impl RemoteSlot {
    fn matches(&mut self, stack: &Slot) -> bool {
        match self {
            Self::Known(remote) => stacks_match(remote, stack),
            Self::Hashed(hash) if hash.matches(stack) => {
                *self = Self::Known(stack.clone());
                true
            }
            Self::Hashed(_) => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct QuickCraft {
    status: i32,
    kind: i32,
    slots: Vec<usize>,
}

impl QuickCraft {
    fn reset(&mut self) {
        self.status = 0;
        self.slots.clear();
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContainerMenu {
    kind: MenuKind,
    carried: Slot,
    quick_craft: QuickCraft,
    remote: Vec<RemoteSlot>,
    remote_carried: RemoteSlot,
    cutter: Cutter,
    trade: Trade,
    held: Option<Hold>,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct Trade {
    offers: MerchantOffers,
    hint: i32,
    active: Option<usize>,
    future_xp: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Cutter {
    selected: i32,
    input: Option<ItemId>,
    error: bool,
}

struct Session<'a> {
    kind: MenuKind,
    container: &'a mut Vec<Slot>,
    player: &'a mut Inventory,
    carried: &'a mut Slot,
    cutter: &'a mut Cutter,
    trade: &'a mut Trade,
    infinite: bool,
    effects: Vec<MenuEffect>,
}

fn refresh_cutter(container: &mut [Slot], cutter: &mut Cutter) {
    let input = container
        .get(CUTTER_INPUT)
        .cloned()
        .flatten()
        .map(|s| s.item);
    if input != cutter.input {
        cutter.input = input;
        cutter.selected = -1;
        if let Some(result) = container.get_mut(CUTTER_RESULT) {
            *result = None;
        }
    }
}

fn set_cutter_result(container: &mut [Slot], cutter: &Cutter) {
    let cuts = cutter
        .input
        .map(stonecutting::recipes_for)
        .unwrap_or_default();
    let result = usize::try_from(cutter.selected)
        .ok()
        .and_then(|index| cuts.get(index))
        .and_then(|cut| {
            Some(ItemStack::new(
                cut.result.id(),
                std::num::NonZeroU8::new(cut.count)?,
            ))
        });
    if let Some(slot) = container.get_mut(CUTTER_RESULT) {
        *slot = result;
    }
}

fn update_merchant(container: &mut [Slot], trade: &mut Trade, effects: &mut Vec<MenuEffect>) {
    trade.active = None;
    let (first, second) = if container[0].is_none() {
        (container[1].clone(), None)
    } else {
        (container[0].clone(), container[1].clone())
    };
    if first.is_none() {
        container[MERCHANT_RESULT] = None;
        trade.future_xp = 0;
        return;
    }
    if !trade.offers.is_empty() {
        let hint = trade.hint;
        let in_stock = |trade: &Trade, found: Option<usize>| {
            found.filter(|index| {
                trade
                    .offers
                    .get(*index)
                    .is_some_and(|offer| !offer.is_out_of_stock())
            })
        };
        let mut found = trade
            .offers
            .recipe_index_for(first.as_ref(), second.as_ref(), hint);
        if in_stock(trade, found).is_none() {
            trade.active = found;
            found = trade
                .offers
                .recipe_index_for(second.as_ref(), first.as_ref(), hint);
        }
        match in_stock(trade, found).and_then(|index| Some((index, trade.offers.get(index)?))) {
            Some((index, offer)) => {
                container[MERCHANT_RESULT] = Some(offer.result.clone());
                trade.future_xp = offer.xp;
                trade.active = Some(index);
            }
            None => {
                container[MERCHANT_RESULT] = None;
                trade.future_xp = 0;
            }
        }
    }
    effects.push(MenuEffect::TradeUpdated {
        valid: container[MERCHANT_RESULT].is_some(),
    });
}

fn cell(kind: MenuKind, slot: usize) -> Option<Cell> {
    let size = kind.container_size();
    if slot < size {
        Some(Cell::Container(slot))
    } else if slot < size + MAIN_SLOTS {
        Some(Cell::Player(MAIN_START + slot - size))
    } else if slot < size + MAIN_SLOTS + HOTBAR_SLOTS {
        Some(Cell::Player(HOTBAR_START + slot - size - MAIN_SLOTS))
    } else {
        None
    }
}

fn stack_limit(kind: MenuKind, slot: usize, stack: &ItemStack) -> u32 {
    if kind == MenuKind::Beacon && slot == 0 {
        return 1;
    }
    if matches!(kind, MenuKind::Mount { .. }) && slot < MOUNT_EXTRA_SLOTS {
        return 1;
    }
    if kind.is_furnace() && slot == furnace::FUEL_SLOT && furnace::is_empty_bucket(stack) {
        return 1;
    }
    CONTAINER_MAX_STACK.min(max_stack(stack))
}

struct Look<'a> {
    kind: MenuKind,
    container: &'a [Slot],
    player: &'a Inventory,
    carried: &'a Slot,
    instabuild: bool,
}

impl ClickView for Look<'_> {
    fn slot(&self, raw: i32) -> Option<usize> {
        usize::try_from(raw)
            .ok()
            .filter(|slot| *slot < self.kind.slot_count())
    }

    fn item(&self, slot: usize) -> Slot {
        match cell(self.kind, slot) {
            Some(Cell::Container(index)) => self.container.get(index).cloned().flatten(),
            Some(Cell::Player(index)) => self.player.get(index).cloned(),
            None => None,
        }
    }

    fn carried(&self) -> &Slot {
        self.carried
    }

    fn may_pickup(&self, _: usize) -> bool {
        true
    }

    fn may_place(&self, slot: usize, stack: &ItemStack) -> bool {
        self.kind.accepts(slot, stack)
    }

    fn slot_limit(&self, slot: usize) -> u32 {
        self.container_limit(slot)
    }

    fn stack_limit(&self, slot: usize, stack: &ItemStack) -> u32 {
        stack_limit(self.kind, slot, stack)
    }

    fn container_limit(&self, slot: usize) -> u32 {
        if (self.kind == MenuKind::Beacon && slot == 0)
            || (matches!(self.kind, MenuKind::Mount { .. }) && slot < MOUNT_EXTRA_SLOTS)
        {
            1
        } else {
            CONTAINER_MAX_STACK
        }
    }

    fn hotbar(&self, button: i32) -> Slot {
        usize::try_from(button)
            .ok()
            .and_then(menu_slot)
            .and_then(|slot| self.player.get(slot))
            .cloned()
    }

    fn in_player(&self, slot: usize) -> bool {
        slot >= self.kind.container_size()
    }

    fn holds(&self, item: ItemId) -> bool {
        self.container
            .iter()
            .chain(self.player.slots()[MAIN_START..HOTBAR_START + HOTBAR_SLOTS].iter())
            .flatten()
            .any(|stack| stack.item == item)
    }

    fn instabuild(&self) -> bool {
        self.instabuild
    }
}

impl Session<'_> {
    fn cell(&self, slot: usize) -> Option<Cell> {
        cell(self.kind, slot)
    }

    fn look(&self) -> Look<'_> {
        Look {
            kind: self.kind,
            container: self.container,
            player: self.player,
            carried: self.carried,
            instabuild: self.infinite,
        }
    }

    fn index(&self, slot: i32) -> Option<usize> {
        usize::try_from(slot)
            .ok()
            .filter(|slot| *slot < self.kind.slot_count())
    }

    fn item(&self, slot: usize) -> Slot {
        match self.cell(slot) {
            Some(Cell::Container(index)) => self.container.get(index).cloned().flatten(),
            Some(Cell::Player(index)) => self.player.get(index).cloned(),
            None => None,
        }
    }

    fn put(&mut self, slot: usize, stack: Slot) {
        match self.cell(slot) {
            Some(Cell::Container(index)) => {
                if let Some(entry) = self.container.get_mut(index) {
                    *entry = stack;
                }
                if self.kind == MenuKind::Crafting && CRAFTING_GRID.contains(&index) {
                    self.refresh_result();
                }
                if self.kind == MenuKind::Stonecutter && index == CUTTER_INPUT {
                    self.refresh_cutter();
                }
                if self.kind == MenuKind::Smithing {
                    self.refresh_smithing(index != smithing::RESULT);
                }
                if self.kind == MenuKind::Merchant && index < MERCHANT_RESULT {
                    self.update_sell_item();
                }
            }
            Some(Cell::Player(index)) => {
                self.player.set(index, stack);
            }
            None => {}
        }
    }

    fn remove(&mut self, slot: usize, amount: u32) -> Slot {
        if amount == 0 {
            return None;
        }
        let mut current = self.item(slot);
        let amount = if self.kind == MenuKind::Merchant && slot == MERCHANT_RESULT {
            count(&current)
        } else {
            amount
        };
        let removed = split(&mut current, amount);
        self.put(slot, current);
        removed
    }

    fn may_place(&self, slot: usize, stack: &Slot) -> bool {
        stack
            .as_ref()
            .is_none_or(|stack| self.kind.accepts(slot, stack))
    }

    fn slot_max_for(&self, slot: usize, stack: &ItemStack) -> u32 {
        stack_limit(self.kind, slot, stack)
    }

    fn try_remove(&mut self, slot: usize, amount: u32, decrement: u32) -> Slot {
        let current = self.item(slot);
        if !self.may_place(slot, &current) && decrement < count(&current) {
            return None;
        }
        self.remove(slot, amount.min(decrement))
    }

    fn grid(&self) -> [Slot; 9] {
        std::array::from_fn(|offset| {
            self.container
                .get(CRAFTING_GRID.start() + offset)
                .cloned()
                .flatten()
        })
    }

    fn refresh_result(&mut self) {
        let result = crafting::result(3, 3, &self.grid());
        if let Some(entry) = self.container.get_mut(CRAFTING_RESULT) {
            *entry = result;
        }
    }

    fn refresh_cutter(&mut self) {
        refresh_cutter(self.container, self.cutter);
    }

    fn update_sell_item(&mut self) {
        update_merchant(self.container, self.trade, &mut self.effects);
    }

    fn take_trade(&mut self) {
        let Some(active) = self.trade.active else {
            return;
        };
        let Some(offer) = self.trade.offers.get(active).cloned() else {
            return;
        };
        let mut first = self.container[0].clone();
        let mut second = self.container[1].clone();
        if !(offer.take(&mut first, &mut second) || offer.take(&mut second, &mut first)) {
            return;
        }
        if let Some(held) = self.trade.offers.get_mut(active) {
            held.increase_uses();
        }
        self.effects.push(MenuEffect::Traded { offer: active });
        self.put(0, first);
        self.put(1, second);
    }

    fn move_to_payment(&mut self, payment: usize, wanted: &crate::entity::trading::ItemCost) {
        for slot in MERCHANT_PLAYER_START..MERCHANT_END {
            let Some(held) = self.item(slot) else {
                continue;
            };
            if !wanted.test(&held) {
                continue;
            }
            let current = self.container[payment].clone();
            if let Some(existing) = &current
                && !same_item_same_components(&held, existing)
            {
                continue;
            }
            let max = max_stack(&held);
            let have = count(&current);
            let moved = max.saturating_sub(have).min(u32::from(held.count.get()));
            let merged = with_count(&held, have + moved);
            let left = with_count(&held, u32::from(held.count.get()) - moved);
            self.put(slot, left);
            self.put(payment, merged.clone());
            if count(&merged) >= max {
                return;
            }
        }
    }

    fn try_move_items(&mut self, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        if index >= self.trade.offers.len() {
            return;
        }
        for payment in 0..MERCHANT_RESULT {
            let Some(held) = self.container[payment].clone() else {
                continue;
            };
            let mut stack = Some(held);
            if !self.move_to(&mut stack, MERCHANT_PLAYER_START, MERCHANT_END, true) {
                return;
            }
            self.put(payment, stack);
        }
        if self.container[0].is_some() || self.container[1].is_some() {
            return;
        }
        let Some(offer) = self.trade.offers.get(index).cloned() else {
            return;
        };
        self.move_to_payment(0, &offer.cost_a);
        if let Some(cost) = &offer.cost_b {
            self.move_to_payment(1, cost);
        }
    }

    fn refresh_smithing(&mut self, inputs_changed: bool) {
        if inputs_changed {
            let result = smithing::result(
                &self.container[smithing::TEMPLATE],
                &self.container[smithing::BASE],
                &self.container[smithing::ADDITION],
            );
            self.container[smithing::RESULT] = result;
        }
        self.cutter.error = self.container[..smithing::INPUTS]
            .iter()
            .all(Option::is_some)
            && self.container[smithing::RESULT].is_none();
    }

    fn took(&mut self, slot: usize, taken: &Slot, amount: u32) {
        if self.kind == MenuKind::Merchant && slot == MERCHANT_RESULT && taken.is_some() {
            self.take_trade();
            return;
        }
        if self.kind == MenuKind::Smithing && slot == smithing::RESULT && taken.is_some() {
            for index in 0..smithing::INPUTS {
                if let Some(input) = self.container[index].take() {
                    self.container[index] = with_count(&input, u32::from(input.count.get()) - 1);
                }
            }
            self.refresh_smithing(true);
            if self.effects.last() != Some(&MenuEffect::SmithingTaken) {
                self.effects.push(MenuEffect::SmithingTaken);
            }
            return;
        }
        if self.kind == MenuKind::Stonecutter && slot == CUTTER_RESULT && taken.is_some() {
            if let Some(input) = self.container[CUTTER_INPUT].take() {
                self.container[CUTTER_INPUT] = with_count(&input, u32::from(input.count.get()) - 1);
            }
            self.refresh_cutter();
            set_cutter_result(self.container, self.cutter);
            if self.effects.last() != Some(&MenuEffect::StonecutterTaken) {
                self.effects.push(MenuEffect::StonecutterTaken);
            }
            return;
        }
        if self.kind.is_furnace() && slot == furnace::RESULT_SLOT {
            if let Some(stack) = taken {
                self.effects.push(MenuEffect::FurnaceOutputTaken {
                    item: stack.item,
                    amount,
                });
            }
            return;
        }
        if self.kind != MenuKind::Crafting || slot != CRAFTING_RESULT || taken.is_none() {
            return;
        }
        let mut cells = self.grid();
        let leftovers = crafting::take_result(3, 3, &mut cells);
        for (offset, cell) in cells.into_iter().enumerate() {
            if let Some(entry) = self.container.get_mut(CRAFTING_GRID.start() + offset) {
                *entry = cell;
            }
        }
        for leftover in leftovers {
            let mut rest = Some(leftover);
            if !self.player.add(&mut rest, self.infinite) {
                self.drop_stack(rest, false, false);
            }
        }
        self.refresh_result();
    }

    fn safe_insert(&mut self, slot: usize, stack: Slot, increment: u32) -> Slot {
        let Some(incoming) = stack.clone() else {
            return stack;
        };
        if !self.may_place(slot, &stack) {
            return stack;
        }
        let current = self.item(slot);
        let room = i64::from(self.slot_max_for(slot, &incoming)) - i64::from(count(&current));
        let placed = i64::from(increment.min(count(&stack))).min(room);
        let Ok(placed) = u32::try_from(placed) else {
            return stack;
        };
        if placed == 0 {
            return stack;
        }
        let mut remaining = stack;
        match &current {
            None => {
                let part = split(&mut remaining, placed);
                self.put(slot, part);
            }
            Some(existing) if same_item_same_components(existing, &incoming) => {
                split(&mut remaining, placed);
                self.put(slot, with_count(existing, count(&current) + placed));
            }
            Some(_) => {}
        }
        remaining
    }

    fn drop_stack(&mut self, stack: Slot, thrower: bool, predicted: bool) -> bool {
        match stack {
            Some(stack) => {
                self.effects.push(MenuEffect::Drop {
                    stack,
                    thrower,
                    predicted,
                });
                true
            }
            None => false,
        }
    }

    fn can_quick_replace(&self, slot: usize, stack: &ItemStack) -> bool {
        match self.item(slot) {
            None => true,
            Some(existing) => {
                same_item_same_components(stack, &existing)
                    && u32::from(existing.count.get()) <= max_stack(stack)
            }
        }
    }

    fn click(&mut self, craft: &mut QuickCraft, slot: i32, button: i32, input: ClickInput) {
        if input == ClickInput::QuickCraft {
            self.quick_craft(craft, slot, button);
            return;
        }
        if craft.status != 0 {
            craft.reset();
            return;
        }
        match input {
            ClickInput::Pickup | ClickInput::QuickMove if button == 0 || button == 1 => {
                self.pickup(slot, button == 0, input == ClickInput::QuickMove);
            }
            ClickInput::Swap if (0..9).contains(&button) || button == OFFHAND_BUTTON => {
                self.swap(slot, button);
            }
            ClickInput::Clone => self.clone_slot(slot),
            ClickInput::Throw => self.throw(slot, button),
            ClickInput::PickupAll => self.pickup_all(slot, button),
            _ => {}
        }
    }

    fn quick_craft(&mut self, craft: &mut QuickCraft, slot: i32, button: i32) {
        let previous = craft.status;
        craft.status = button & 3;
        if (previous != 1 || craft.status != 2) && previous != craft.status {
            craft.reset();
            return;
        }
        let Some(carried) = self.carried.clone() else {
            craft.reset();
            return;
        };
        match craft.status {
            0 => {
                craft.kind = (button >> 2) & 3;
                if craft.kind == 0 || craft.kind == 1 || (craft.kind == 2 && self.infinite) {
                    craft.status = 1;
                    craft.slots.clear();
                } else {
                    craft.reset();
                }
            }
            1 => {
                let Some(target) = self.index(slot) else {
                    return;
                };
                let wide = craft.kind == 2
                    || u32::from(carried.count.get())
                        > u32::try_from(craft.slots.len()).unwrap_or(u32::MAX);
                if self.can_quick_replace(target, &carried)
                    && self.may_place(target, &Some(carried.clone()))
                    && wide
                    && !craft.slots.contains(&target)
                {
                    craft.slots.push(target);
                }
            }
            2 => {
                if !craft.slots.is_empty() {
                    self.finish_quick_craft(craft, &carried);
                }
                craft.reset();
            }
            _ => craft.reset(),
        }
    }

    fn finish_quick_craft(&mut self, craft: &QuickCraft, copy: &ItemStack) {
        let drag = click::plan_drag(&self.look(), &craft.slots, craft.kind, copy);
        *self.carried = drag.remaining;
        for (target, stack) in drag.placed {
            self.put(target, Some(stack));
        }
        self.effects.push(MenuEffect::FullResync);
    }

    fn pickup(&mut self, slot: i32, primary: bool, quick_move: bool) {
        if slot == i32::from(OUTSIDE) {
            if self.carried.is_some() {
                let thrown = if primary {
                    self.carried.take()
                } else {
                    split(self.carried, 1)
                };
                self.drop_stack(thrown, true, true);
            }
            return;
        }
        let Some(target) = self.index(slot) else {
            return;
        };
        if quick_move {
            let mut moved = self.quick_move_stack(target);
            while moved.is_some() && same_item(&self.item(target), &moved) {
                moved = self.quick_move_stack(target);
            }
            return;
        }
        let in_slot = self.item(target);
        let carried = self.carried.clone();
        match (&in_slot, &carried) {
            (None, Some(_)) => {
                let amount = if primary { count(&carried) } else { 1 };
                *self.carried = self.safe_insert(target, carried, amount);
            }
            (None, None) => {}
            (Some(existing), _) => match &carried {
                None => {
                    let amount = if primary {
                        count(&in_slot)
                    } else {
                        count(&in_slot).div_ceil(2)
                    };
                    if let Some(taken) = self.try_remove(target, amount, u32::MAX) {
                        *self.carried = Some(taken.clone());
                        let amount = u32::from(taken.count.get());
                        self.took(target, &Some(taken), amount);
                    }
                }
                Some(held) if self.may_place(target, &carried) => {
                    if same_item_same_components(existing, held) {
                        let amount = if primary { count(&carried) } else { 1 };
                        *self.carried = self.safe_insert(target, carried, amount);
                    } else if count(&carried) <= self.slot_max_for(target, held) {
                        *self.carried = in_slot.clone();
                        self.put(target, carried);
                    }
                }
                Some(held) if same_item_same_components(existing, held) => {
                    let room = max_stack(held).saturating_sub(count(&carried));
                    if let Some(taken) = self.try_remove(target, count(&in_slot), room) {
                        let amount = u32::from(taken.count.get());
                        *self.carried = with_count(held, count(&carried) + amount);
                        self.took(target, &Some(taken), amount);
                    }
                }
                Some(_) => {}
            },
        }
    }

    fn quick_move_stack(&mut self, slot: usize) -> Slot {
        let copy = self.item(slot)?;
        let mut stack = Some(copy.clone());
        let mut moved = false;
        for (start, end, reverse) in self.kind.quick_move_plan(slot, &copy, self.container) {
            if self.move_to(&mut stack, start, end, reverse) {
                moved = true;
                break;
            }
        }
        if !moved {
            return None;
        }
        self.put(slot, stack.clone());
        if count(&stack) == u32::from(copy.count.get()) {
            return None;
        }
        let moved_count = u32::from(copy.count.get()) - count(&stack);
        if self.kind == MenuKind::Merchant && slot == MERCHANT_RESULT {
            self.took(slot, &Some(copy.clone()), moved_count);
        } else if (self.kind == MenuKind::Crafting && slot == CRAFTING_RESULT)
            || (self.kind == MenuKind::Stonecutter && slot == CUTTER_RESULT)
        {
            self.took(slot, &Some(copy.clone()), moved_count);
            self.drop_stack(stack, false, true);
        } else if (self.kind.is_furnace() && slot == furnace::RESULT_SLOT)
            || (self.kind == MenuKind::Smithing && slot == smithing::RESULT)
        {
            self.took(slot, &Some(copy.clone()), moved_count);
        }
        Some(copy)
    }

    fn move_to(&mut self, stack: &mut Slot, start: usize, end: usize, reverse: bool) -> bool {
        let order: Vec<usize> = if reverse {
            (start..end).rev().collect()
        } else {
            (start..end).collect()
        };
        let mut moved = false;
        if stack.as_ref().is_some_and(|s| max_stack(s) > 1) {
            for &target in &order {
                let Some(incoming) = stack.clone() else {
                    break;
                };
                let Some(existing) = self.item(target) else {
                    continue;
                };
                if !same_item_same_components(&incoming, &existing) {
                    continue;
                }
                let total = u32::from(existing.count.get()) + u32::from(incoming.count.get());
                let limit = self.slot_max_for(target, &existing);
                if total <= limit {
                    *stack = None;
                    self.put(target, with_count(&existing, total));
                    moved = true;
                } else if u32::from(existing.count.get()) < limit {
                    split(stack, limit - u32::from(existing.count.get()));
                    self.put(target, with_count(&existing, limit));
                    moved = true;
                }
            }
        }
        if let Some(incoming) = stack.clone() {
            for &target in &order {
                if self.item(target).is_none() && self.may_place(target, stack) {
                    let limit = self.slot_max_for(target, &incoming);
                    let part = split(stack, u32::from(incoming.count.get()).min(limit));
                    self.put(target, part);
                    moved = true;
                    break;
                }
            }
        }
        moved
    }

    fn swap(&mut self, slot: i32, button: i32) {
        let Some(target) = self.index(slot) else {
            return;
        };
        let Some(own) = usize::try_from(button).ok().and_then(menu_slot) else {
            return;
        };
        let held = self.player.get(own).cloned();
        let in_slot = self.item(target);
        match (&held, &in_slot) {
            (None, None) => {}
            (None, Some(_)) => {
                self.player.set(own, in_slot.clone());
                self.put(target, None);
                self.took(target, &in_slot, 0);
            }
            (Some(stack), None) => {
                if self.may_place(target, &held) {
                    let limit = self.slot_max_for(target, stack);
                    if count(&held) > limit {
                        let mut rest = held.clone();
                        let part = split(&mut rest, limit);
                        self.player.set(own, rest);
                        self.put(target, part);
                    } else {
                        self.player.set(own, None);
                        self.put(target, held.clone());
                    }
                }
            }
            (Some(stack), Some(_)) => {
                if self.may_place(target, &held) {
                    let limit = self.slot_max_for(target, stack);
                    if count(&held) > limit {
                        let mut rest = held.clone();
                        let part = split(&mut rest, limit);
                        self.player.set(own, rest);
                        self.put(target, part);
                        let mut displaced = in_slot.clone();
                        if !self.player.add(&mut displaced, self.infinite) {
                            self.drop_stack(displaced, true, true);
                        }
                    } else {
                        self.player.set(own, in_slot.clone());
                        self.put(target, held.clone());
                    }
                }
            }
        }
    }

    fn clone_slot(&mut self, slot: i32) {
        if !self.infinite || self.carried.is_some() {
            return;
        }
        let Some(target) = self.index(slot) else {
            return;
        };
        if let Some(stack) = self.item(target) {
            *self.carried = with_count(&stack, max_stack(&stack));
        }
    }

    fn throw(&mut self, slot: i32, button: i32) {
        if self.carried.is_some() {
            return;
        }
        let Some(target) = self.index(slot) else {
            return;
        };
        let amount = if button == 0 {
            1
        } else {
            count(&self.item(target))
        };
        let mut dropped = self.try_remove(target, amount, u32::MAX);
        self.took(target, &dropped, count(&dropped));
        self.drop_stack(dropped.clone(), true, true);
        if button == 1 {
            while dropped.is_some() && same_item(&self.item(target), &dropped) {
                dropped = self.try_remove(target, amount, u32::MAX);
                self.took(target, &dropped, count(&dropped));
                if !self.drop_stack(dropped.clone(), true, true) {
                    break;
                }
            }
        }
    }

    fn pickup_all(&mut self, slot: i32, button: i32) {
        if self.kind == MenuKind::Merchant {
            return;
        }
        let Some(target) = self.index(slot) else {
            return;
        };
        if self.carried.is_none() || self.item(target).is_some() {
            return;
        }
        let total = self.kind.slot_count();
        let order: Vec<usize> = if button == 0 {
            (0..total).collect()
        } else {
            (0..total).rev().collect()
        };
        for pass in 0..2 {
            for &source in &order {
                let Some(carried) = self.carried.clone() else {
                    return;
                };
                if u32::from(carried.count.get()) >= max_stack(&carried) {
                    break;
                }
                let Some(existing) = self.item(source) else {
                    continue;
                };
                if !self.can_quick_replace(source, &carried) {
                    continue;
                }
                if pass == 0 && u32::from(existing.count.get()) == max_stack(&existing) {
                    continue;
                }
                let room = max_stack(&carried) - u32::from(carried.count.get());
                let taken = self.try_remove(source, u32::from(existing.count.get()), room);
                *self.carried =
                    with_count(&carried, u32::from(carried.count.get()) + count(&taken));
            }
        }
    }
}

impl ContainerMenu {
    pub fn new(kind: MenuKind) -> Self {
        Self {
            kind,
            carried: None,
            quick_craft: QuickCraft::default(),
            remote: vec![RemoteSlot::Known(None); kind.slot_count()],
            remote_carried: RemoteSlot::Known(None),
            cutter: Cutter::default(),
            trade: Trade::default(),
            held: None,
        }
    }

    pub fn kind(&self) -> MenuKind {
        self.kind
    }

    pub fn carried(&self) -> &Slot {
        &self.carried
    }

    pub fn click(
        &mut self,
        container: &mut Vec<Slot>,
        player: &mut Inventory,
        slot: i16,
        button: i8,
        input: ClickInput,
        clicker: Clicker,
    ) -> Vec<MenuEffect> {
        container.resize(self.kind.container_size(), None);
        let mut session = Session {
            kind: self.kind,
            container,
            player,
            carried: &mut self.carried,
            cutter: &mut self.cutter,
            trade: &mut self.trade,
            infinite: clicker.infinite_materials,
            effects: Vec::new(),
        };
        session.click(
            &mut self.quick_craft,
            i32::from(slot),
            i32::from(button),
            input,
        );
        session.effects
    }

    pub fn classify(
        &self,
        container: &[Slot],
        player: &Inventory,
        slot: i16,
        button: i8,
        input: ClickInput,
        clicker: Clicker,
    ) -> Classified {
        let look = Look {
            kind: self.kind,
            container,
            player,
            carried: &self.carried,
            instabuild: clicker.infinite_materials,
        };
        click::classify(&look, slot, button, input)
    }

    pub fn quick_crafting(&self) -> bool {
        self.quick_craft.status != 0
    }

    pub fn pending_drag(&self, container: &[Slot], player: &Inventory, button: i8) -> Option<Drag> {
        let ends = self.quick_craft.status == 1 && i32::from(button) & 3 == 2;
        let carried = self.carried.as_ref()?;
        if !ends || self.quick_craft.slots.is_empty() {
            return None;
        }
        let look = Look {
            kind: self.kind,
            container,
            player,
            carried: &self.carried,
            instabuild: false,
        };
        Some(click::plan_drag(
            &look,
            &self.quick_craft.slots,
            self.quick_craft.kind,
            carried,
        ))
    }

    pub fn drag_holds(&self, container: &[Slot], player: &Inventory, drag: &Drag) -> bool {
        self.quick_craft.status == 1
            && self.carried.as_ref() == Some(&drag.cursor)
            && drag
                .before
                .iter()
                .all(|(slot, stack)| stacks_match(&self.view(*slot, container, player), stack))
    }

    pub fn finish_drag(
        &mut self,
        container: &mut Vec<Slot>,
        player: &mut Inventory,
        drag: &Drag,
        cursor: Option<Slot>,
    ) -> Vec<MenuEffect> {
        self.quick_craft.reset();
        container.resize(self.kind.container_size(), None);
        let mut session = Session {
            kind: self.kind,
            container,
            player,
            carried: &mut self.carried,
            cutter: &mut self.cutter,
            trade: &mut self.trade,
            infinite: false,
            effects: Vec::new(),
        };
        if let Some(cursor) = cursor {
            *session.carried = cursor;
            for (target, stack) in &drag.placed {
                session.put(*target, Some(stack.clone()));
            }
        }
        session.effects.push(MenuEffect::FullResync);
        session.effects
    }

    pub fn set_slot(
        &mut self,
        container: &mut Vec<Slot>,
        player: &mut Inventory,
        slot: usize,
        stack: Slot,
    ) -> Vec<MenuEffect> {
        container.resize(self.kind.container_size(), None);
        let mut session = Session {
            kind: self.kind,
            container,
            player,
            carried: &mut self.carried,
            cutter: &mut self.cutter,
            trade: &mut self.trade,
            infinite: false,
            effects: Vec::new(),
        };
        session.put(slot, stack);
        session.effects
    }

    pub fn quick_move_once(
        &mut self,
        container: &mut Vec<Slot>,
        player: &mut Inventory,
        slot: usize,
        clicker: Clicker,
    ) -> (Slot, Vec<MenuEffect>) {
        container.resize(self.kind.container_size(), None);
        let mut session = Session {
            kind: self.kind,
            container,
            player,
            carried: &mut self.carried,
            cutter: &mut self.cutter,
            trade: &mut self.trade,
            infinite: clicker.infinite_materials,
            effects: Vec::new(),
        };
        let moved = session.quick_move_stack(slot);
        (moved, session.effects)
    }

    pub fn hold(&mut self, slot: usize, container: &[Slot], player: &Inventory) {
        let shown = match self.remote.get(slot) {
            Some(RemoteSlot::Known(shown)) => shown.clone(),
            _ => self.view(slot, container, player),
        };
        Hold::take(&mut self.held, slot, shown);
    }

    pub fn release(&mut self) {
        Hold::release(&mut self.held);
    }

    pub fn set_remote(&mut self, slot: usize, stack: Slot) {
        if let Some(remote) = self.remote.get_mut(slot) {
            *remote = RemoteSlot::Known(stack);
        }
    }

    pub fn set_carried(&mut self, stack: Slot) {
        self.carried = stack.clone();
        self.remote_carried = RemoteSlot::Known(stack);
    }

    pub fn item(&self, slot: usize, container: &[Slot], player: &Inventory) -> Slot {
        if slot >= self.kind.slot_count() {
            return None;
        }
        self.view(slot, container, player)
    }

    pub fn merchant_offers(&self) -> &MerchantOffers {
        &self.trade.offers
    }

    pub fn future_trade_xp(&self) -> i32 {
        self.trade.future_xp
    }

    pub fn trade_offer(&self, index: usize) -> Option<&MerchantOffer> {
        self.trade.offers.get(index)
    }

    pub fn set_offers(
        &mut self,
        container: &mut Vec<Slot>,
        player: &mut Inventory,
        offers: MerchantOffers,
    ) -> Vec<MenuEffect> {
        container.resize(self.kind.container_size(), None);
        self.trade.offers = offers;
        let mut session = Session {
            kind: self.kind,
            container,
            player,
            carried: &mut self.carried,
            cutter: &mut self.cutter,
            trade: &mut self.trade,
            infinite: false,
            effects: Vec::new(),
        };
        session.update_sell_item();
        session.effects
    }

    pub fn select_trade(
        &mut self,
        container: &mut Vec<Slot>,
        player: &mut Inventory,
        index: i32,
    ) -> Vec<MenuEffect> {
        container.resize(self.kind.container_size(), None);
        self.trade.hint = index;
        let mut session = Session {
            kind: self.kind,
            container,
            player,
            carried: &mut self.carried,
            cutter: &mut self.cutter,
            trade: &mut self.trade,
            infinite: false,
            effects: Vec::new(),
        };
        session.update_sell_item();
        session.try_move_items(index);
        session.effects
    }

    pub fn data(&self) -> Vec<i16> {
        match self.kind {
            MenuKind::Stonecutter => vec![i16::try_from(self.cutter.selected).unwrap_or(-1)],
            MenuKind::Smithing => vec![i16::from(self.cutter.error)],
            _ => Vec::new(),
        }
    }

    pub fn press(&mut self, container: &mut Vec<Slot>, button: i32) -> bool {
        if self.kind != MenuKind::Stonecutter {
            return false;
        }
        container.resize(self.kind.container_size(), None);
        refresh_cutter(container, &mut self.cutter);
        let cuts = self
            .cutter
            .input
            .map(stonecutting::recipes_for)
            .unwrap_or_default();
        if self.cutter.selected == button {
            return false;
        }
        if usize::try_from(button).is_ok_and(|index| index < cuts.len()) {
            self.cutter.selected = button;
            set_cutter_result(container, &self.cutter);
        }
        true
    }

    pub fn close(&mut self, player: &mut Inventory) -> Vec<MenuEffect> {
        let mut effects = Vec::new();
        if let Some(carried) = self.carried.take() {
            place_back_or_drop(player, carried, &mut effects);
        }
        effects
    }

    pub fn close_transient(
        &mut self,
        container: &mut [Slot],
        player: &mut Inventory,
    ) -> Vec<MenuEffect> {
        let mut effects = self.close(player);
        for (slot, entry) in container.iter_mut().enumerate() {
            if self.kind.clears_on_close(slot)
                && let Some(stack) = entry.take()
            {
                if self.kind == MenuKind::Beacon {
                    effects.push(MenuEffect::Drop {
                        stack,
                        thrower: false,
                        predicted: false,
                    });
                } else {
                    place_back_or_drop(player, stack, &mut effects);
                }
            }
        }
        effects
    }

    pub fn receive_remote(&mut self, slot: i16, hash: HashedSlot) -> bool {
        match usize::try_from(slot)
            .ok()
            .and_then(|slot| self.remote.get_mut(slot))
        {
            Some(remote) => {
                *remote = RemoteSlot::Hashed(hash);
                true
            }
            None => false,
        }
    }

    pub fn receive_remote_carried(&mut self, hash: HashedSlot) {
        self.remote_carried = RemoteSlot::Hashed(hash);
    }

    fn view(&self, slot: usize, container: &[Slot], player: &Inventory) -> Slot {
        let size = self.kind.container_size();
        if slot < size {
            container.get(slot).cloned().flatten()
        } else if slot < size + MAIN_SLOTS {
            player.get(MAIN_START + slot - size).cloned()
        } else {
            player.get(HOTBAR_START + slot - size - MAIN_SLOTS).cloned()
        }
    }

    pub fn changes(&mut self, container: &[Slot], player: &Inventory) -> Vec<RemoteUpdate> {
        let mut updates = Vec::new();
        let held = self.held.as_ref().map(|hold| hold.slot);
        for slot in 0..self.remote.len() {
            if held == Some(slot) {
                continue;
            }
            let stack = self.view(slot, container, player);
            if !self.remote[slot].matches(&stack) {
                self.remote[slot] = RemoteSlot::Known(stack.clone());
                updates.push(RemoteUpdate::Slot(slot, stack));
            }
        }
        if !self.remote_carried.matches(&self.carried) {
            self.remote_carried = RemoteSlot::Known(self.carried.clone());
            updates.push(RemoteUpdate::Carried(self.carried.clone()));
        }
        updates
    }

    pub fn sync_all(&mut self, container: &[Slot], player: &Inventory) -> (Vec<Slot>, Slot) {
        let mut slots: Vec<Slot> = (0..self.remote.len())
            .map(|slot| self.view(slot, container, player))
            .collect();
        if let Some(hold) = &self.held
            && let Some(slot) = slots.get_mut(hold.slot)
        {
            slot.clone_from(&hold.shown);
        }
        for (remote, stack) in self.remote.iter_mut().zip(&slots) {
            *remote = RemoteSlot::Known(stack.clone());
        }
        self.remote_carried = RemoteSlot::Known(self.carried.clone());
        (slots, self.carried.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU8;

    use rspigot_nbt::Compound;

    use super::*;

    fn stack(name: &str, count: u8) -> ItemStack {
        let kind = ItemKind::from_name(name).expect("known item");
        ItemStack::new(kind.id(), NonZeroU8::new(count).expect("non zero"))
    }

    fn chest() -> (ContainerMenu, Vec<Slot>, Inventory) {
        (
            ContainerMenu::new(MenuKind::Generic { rows: 3 }),
            vec![None; 27],
            Inventory::default(),
        )
    }

    fn click(
        menu: &mut ContainerMenu,
        container: &mut Vec<Slot>,
        player: &mut Inventory,
        slot: i16,
        button: i8,
        input: ClickInput,
    ) -> Vec<MenuEffect> {
        menu.click(container, player, slot, button, input, Clicker::default())
    }

    #[test]
    fn the_lectern_slot_classifies_like_any_slot_spigot_can_pick_up_from() {
        use crate::click::{ClickAction, ClickType};
        let menu = ContainerMenu::new(MenuKind::Lectern);
        let container = vec![Some(stack("minecraft:written_book", 1))];
        let player = Inventory::default();
        let classify = |input: ClickInput, button: i8| {
            let found = menu.classify(&container, &player, 0, button, input, Clicker::default());
            (found.click, found.action)
        };
        assert_eq!(
            classify(ClickInput::Pickup, 0),
            (ClickType::Left, ClickAction::PickupAll)
        );
        assert_eq!(
            classify(ClickInput::QuickMove, 0),
            (ClickType::ShiftLeft, ClickAction::MoveToOtherInventory)
        );
        assert_eq!(
            classify(ClickInput::Swap, 0),
            (ClickType::NumberKey, ClickAction::HotbarSwap)
        );
        assert_eq!(
            classify(ClickInput::Throw, 0),
            (ClickType::Drop, ClickAction::DropOneSlot)
        );
    }

    #[test]
    fn the_menu_ids_match_the_registry() {
        let registry = |name: &str| {
            let index = rspigot_data::registries::static_index("minecraft:menu", name).expect(name);
            i32::try_from(index).expect("small id")
        };
        assert_eq!(
            MenuKind::Generic { rows: 1 }.protocol_id(),
            registry("minecraft:generic_9x1")
        );
        assert_eq!(
            MenuKind::Generic { rows: 3 }.protocol_id(),
            registry("minecraft:generic_9x3")
        );
        assert_eq!(
            MenuKind::Generic { rows: 6 }.protocol_id(),
            registry("minecraft:generic_9x6")
        );
        assert_eq!(
            MenuKind::Generic3x3.protocol_id(),
            registry("minecraft:generic_3x3")
        );
        assert_eq!(MenuKind::Hopper.protocol_id(), registry("minecraft:hopper"));
        assert_eq!(
            MenuKind::ShulkerBox.protocol_id(),
            registry("minecraft:shulker_box")
        );
        assert_eq!(
            MenuKind::Crafting.protocol_id(),
            registry("minecraft:crafting")
        );
        assert_eq!(
            MenuKind::Furnace.protocol_id(),
            registry("minecraft:furnace")
        );
        assert_eq!(
            MenuKind::BlastFurnace.protocol_id(),
            registry("minecraft:blast_furnace")
        );
        assert_eq!(MenuKind::Smoker.protocol_id(), registry("minecraft:smoker"));
    }

    fn workstation(kind: MenuKind) -> (ContainerMenu, Vec<Slot>, Inventory) {
        (
            ContainerMenu::new(kind),
            vec![None; kind.container_size()],
            Inventory::default(),
        )
    }

    #[test]
    fn the_crafting_menu_has_a_result_slot_that_takes_nothing() {
        let (mut menu, mut container, mut player) = workstation(MenuKind::Crafting);
        assert_eq!(MenuKind::Crafting.slot_count(), 46);
        player.set(MAIN_START, Some(stack("minecraft:stick", 4)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            10,
            0,
            ClickInput::Pickup,
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::Pickup,
        );
        assert!(container[0].is_none());
        assert!(menu.carried().is_some());
        click(
            &mut menu,
            &mut container,
            &mut player,
            5,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(container[5].as_ref().map(|s| s.count.get()), Some(4));
    }

    fn table() -> (ContainerMenu, Vec<Slot>, Inventory) {
        workstation(MenuKind::Crafting)
    }

    fn load(
        menu: &mut ContainerMenu,
        container: &mut Vec<Slot>,
        player: &mut Inventory,
        slot: i16,
        item: ItemStack,
    ) {
        player.set(MAIN_START, Some(item));
        click(menu, container, player, 10, 0, ClickInput::Pickup);
        click(menu, container, player, slot, 0, ClickInput::Pickup);
    }

    fn pile(player: &Inventory, name: &str) -> u32 {
        let kind = ItemKind::from_name(name).unwrap();
        (0..crate::item::PLAYER_INVENTORY_SIZE)
            .filter_map(|slot| player.get(slot))
            .filter(|stack| stack.item == kind.id())
            .map(|stack| u32::from(stack.count.get()))
            .sum()
    }

    #[test]
    fn the_crafting_menu_shows_the_result_of_what_is_in_the_grid() {
        let (mut menu, mut container, mut player) = table();
        player.set(MAIN_START, Some(stack("minecraft:oak_log", 2)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            10,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(container[0], None);
        click(
            &mut menu,
            &mut container,
            &mut player,
            5,
            1,
            ClickInput::Pickup,
        );
        assert_eq!(container[0], Some(stack("minecraft:oak_planks", 4)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            10,
            0,
            ClickInput::Pickup,
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            5,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(container[0], None);
    }

    #[test]
    fn the_whole_three_by_three_grid_is_used_to_match_a_pattern() {
        let (mut menu, mut container, mut player) = table();
        player.set(MAIN_START, Some(stack("minecraft:oak_planks", 8)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            10,
            0,
            ClickInput::Pickup,
        );
        for slot in [1, 2, 3, 4, 6, 7, 8] {
            click(
                &mut menu,
                &mut container,
                &mut player,
                slot,
                1,
                ClickInput::Pickup,
            );
        }
        assert_ne!(container[0], Some(stack("minecraft:chest", 1)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            9,
            1,
            ClickInput::Pickup,
        );
        assert_eq!(container[0], Some(stack("minecraft:chest", 1)));
    }

    #[test]
    fn taking_the_result_uses_a_craft_and_shows_the_next_one() {
        let (mut menu, mut container, mut player) = table();
        load(
            &mut menu,
            &mut container,
            &mut player,
            5,
            stack("minecraft:oak_log", 2),
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(menu.carried(), &Some(stack("minecraft:oak_planks", 4)));
        assert_eq!(container[5], Some(stack("minecraft:oak_log", 1)));
        assert_eq!(container[0], Some(stack("minecraft:oak_planks", 4)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(menu.carried(), &Some(stack("minecraft:oak_planks", 8)));
        assert_eq!(container[5], None);
        assert_eq!(container[0], None);
    }

    #[test]
    fn shift_click_on_the_result_crafts_until_the_grid_runs_out() {
        let (mut menu, mut container, mut player) = table();
        player.set(MAIN_START, Some(stack("minecraft:oak_log", 3)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            10,
            0,
            ClickInput::Pickup,
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            5,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(container[5], Some(stack("minecraft:oak_log", 3)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::QuickMove,
        );
        assert_eq!(container[5], None);
        assert_eq!(container[0], None);
        assert_eq!(pile(&player, "minecraft:oak_planks"), 12);
    }

    #[test]
    fn shift_click_on_the_result_stops_when_nothing_fits() {
        let (mut menu, mut container, mut player) = table();
        for slot in 9..45 {
            player.set(slot, Some(stack("minecraft:dirt", 64)));
        }
        container[5] = Some(stack("minecraft:oak_log", 5));
        container[0] = Some(stack("minecraft:oak_planks", 4));
        let effects = click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::QuickMove,
        );
        assert!(effects.is_empty());
        assert_eq!(container[5], Some(stack("minecraft:oak_log", 5)));
        assert_eq!(container[0], Some(stack("minecraft:oak_planks", 4)));
    }

    #[test]
    fn what_the_inventory_cannot_hold_of_a_shift_clicked_result_is_dropped() {
        let (mut menu, mut container, mut player) = table();
        for slot in 9..45 {
            player.set(slot, Some(stack("minecraft:dirt", 64)));
        }
        player.set(20, Some(stack("minecraft:oak_planks", 62)));
        load(
            &mut menu,
            &mut container,
            &mut player,
            5,
            stack("minecraft:oak_log", 1),
        );
        player.set(MAIN_START, Some(stack("minecraft:dirt", 64)));
        let effects = click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::QuickMove,
        );
        assert_eq!(player.get(20), Some(&stack("minecraft:oak_planks", 64)));
        assert_eq!(
            effects,
            vec![MenuEffect::Drop {
                stack: stack("minecraft:oak_planks", 2),
                thrower: false,
                predicted: true,
            }]
        );
        assert_eq!(container[5], None);
    }

    #[test]
    fn dropping_the_result_throws_what_was_taken_and_uses_a_craft() {
        let (mut menu, mut container, mut player) = table();
        player.set(MAIN_START, Some(stack("minecraft:oak_log", 3)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            10,
            0,
            ClickInput::Pickup,
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            5,
            0,
            ClickInput::Pickup,
        );
        let effects = click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::Throw,
        );
        assert_eq!(
            effects,
            vec![MenuEffect::Drop {
                stack: stack("minecraft:oak_planks", 1),
                thrower: true,
                predicted: true,
            }]
        );
        assert_eq!(container[5], Some(stack("minecraft:oak_log", 2)));
        assert_eq!(container[0], Some(stack("minecraft:oak_planks", 4)));
    }

    #[test]
    fn a_number_key_takes_the_result_only_into_an_empty_hotbar_slot() {
        let (mut menu, mut container, mut player) = table();
        player.set(MAIN_START, Some(stack("minecraft:oak_log", 2)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            10,
            0,
            ClickInput::Pickup,
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            5,
            0,
            ClickInput::Pickup,
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            2,
            ClickInput::Swap,
        );
        assert_eq!(
            player.get(HOTBAR_START + 2),
            Some(&stack("minecraft:oak_planks", 4))
        );
        assert_eq!(container[5], Some(stack("minecraft:oak_log", 1)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            2,
            ClickInput::Swap,
        );
        assert_eq!(
            player.get(HOTBAR_START + 2),
            Some(&stack("minecraft:oak_planks", 4))
        );
        assert_eq!(container[5], Some(stack("minecraft:oak_log", 1)));
    }

    #[test]
    fn the_result_never_comes_out_in_parts() {
        let (mut menu, mut container, mut player) = table();
        player.set(MAIN_START, Some(stack("minecraft:oak_log", 2)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            10,
            0,
            ClickInput::Pickup,
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            5,
            0,
            ClickInput::Pickup,
        );
        menu.carried = Some(stack("minecraft:oak_planks", 62));
        click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(menu.carried(), &Some(stack("minecraft:oak_planks", 62)));
        assert_eq!(container[5], Some(stack("minecraft:oak_log", 2)));
        menu.carried = Some(stack("minecraft:oak_planks", 60));
        click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(menu.carried(), &Some(stack("minecraft:oak_planks", 64)));
        assert_eq!(container[5], Some(stack("minecraft:oak_log", 1)));
    }

    #[test]
    fn a_crafting_remainder_goes_back_to_the_inventory_or_is_dropped() {
        let (mut menu, mut container, mut player) = table();
        player.set(MAIN_START, Some(stack("minecraft:honey_bottle", 2)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            10,
            0,
            ClickInput::Pickup,
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            5,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(container[0], Some(stack("minecraft:sugar", 3)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(container[5], Some(stack("minecraft:honey_bottle", 1)));
        assert_eq!(pile(&player, "minecraft:glass_bottle"), 1);
        for slot in 9..45 {
            player.set(slot, Some(stack("minecraft:dirt", 64)));
        }
        menu.carried = None;
        container[5] = Some(stack("minecraft:honey_bottle", 2));
        container[0] = Some(stack("minecraft:sugar", 3));
        let effects = click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::Pickup,
        );
        assert!(effects.contains(&MenuEffect::Drop {
            stack: stack("minecraft:glass_bottle", 1),
            thrower: false,
            predicted: false,
        }));
    }

    #[test]
    fn shift_click_in_the_crafting_menu_fills_the_grid_then_the_hotbar() {
        let (mut menu, mut container, mut player) = workstation(MenuKind::Crafting);
        player.set(MAIN_START, Some(stack("minecraft:stick", 2)));
        player.set(HOTBAR_START, Some(stack("minecraft:stone", 3)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            10,
            0,
            ClickInput::QuickMove,
        );
        assert_eq!(container[1].as_ref().map(|s| s.count.get()), Some(2));
        click(
            &mut menu,
            &mut container,
            &mut player,
            37,
            0,
            ClickInput::QuickMove,
        );
        assert_eq!(container[2].as_ref().map(|s| s.count.get()), Some(3));
        click(
            &mut menu,
            &mut container,
            &mut player,
            1,
            0,
            ClickInput::QuickMove,
        );
        assert!(container[1].is_none());
        assert_eq!(player.get(MAIN_START).map(|s| s.count.get()), Some(2));
    }

    #[test]
    fn closing_the_crafting_menu_gives_the_grid_and_the_stack_in_hand_back() {
        let (mut menu, mut container, mut player) = workstation(MenuKind::Crafting);
        container[3] = Some(stack("minecraft:diamond", 2));
        container[0] = Some(stack("minecraft:stick", 1));
        player.set(MAIN_START, Some(stack("minecraft:stone", 5)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            10,
            0,
            ClickInput::Pickup,
        );
        let effects = menu.close_transient(&mut container, &mut player);
        assert!(container[3].is_none());
        assert!(container[0].is_some(), "the result slot is not cleared");
        assert_eq!(effects.len(), 2);
        let held: u32 = (0..46)
            .filter_map(|slot| player.get(slot))
            .map(|s| u32::from(s.count.get()))
            .sum();
        assert_eq!(held, 7);
    }

    #[test]
    fn the_furnace_result_slot_refuses_items_and_shift_click_swaps_main_and_hotbar() {
        let (mut menu, mut container, mut player) = workstation(MenuKind::Furnace);
        assert_eq!(MenuKind::Furnace.slot_count(), 39);
        assert_eq!(MenuKind::Furnace.data_count(), 4);
        player.set(MAIN_START, Some(stack("minecraft:dirt", 3)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            3,
            0,
            ClickInput::Pickup,
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            2,
            0,
            ClickInput::Pickup,
        );
        assert!(container[2].is_none());
        click(
            &mut menu,
            &mut container,
            &mut player,
            3,
            0,
            ClickInput::Pickup,
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            3,
            0,
            ClickInput::QuickMove,
        );
        assert_eq!(player.get(HOTBAR_START).map(|s| s.count.get()), Some(3));
        click(
            &mut menu,
            &mut container,
            &mut player,
            30,
            0,
            ClickInput::QuickMove,
        );
        assert_eq!(player.get(MAIN_START).map(|s| s.count.get()), Some(3));
    }

    fn shift(
        menu: &mut ContainerMenu,
        container: &mut Vec<Slot>,
        player: &mut Inventory,
        slot: i16,
    ) -> Vec<MenuEffect> {
        click(menu, container, player, slot, 0, ClickInput::QuickMove)
    }

    #[test]
    fn shift_click_sends_cookable_items_to_the_input_and_fuel_to_the_fuel_slot() {
        let (mut menu, mut container, mut player) = workstation(MenuKind::Furnace);
        player.set(MAIN_START, Some(stack("minecraft:raw_iron", 3)));
        player.set(MAIN_START + 1, Some(stack("minecraft:coal", 2)));
        shift(&mut menu, &mut container, &mut player, 3);
        shift(&mut menu, &mut container, &mut player, 4);
        assert_eq!(container[0], Some(stack("minecraft:raw_iron", 3)));
        assert_eq!(container[1], Some(stack("minecraft:coal", 2)));
        assert!(player.get(MAIN_START).is_none() && player.get(MAIN_START + 1).is_none());
    }

    #[test]
    fn each_furnace_only_takes_what_it_can_cook_from_shift_click() {
        let cases = [
            (MenuKind::Furnace, "minecraft:beef", true),
            (MenuKind::BlastFurnace, "minecraft:beef", false),
            (MenuKind::BlastFurnace, "minecraft:raw_iron", true),
            (MenuKind::Smoker, "minecraft:raw_iron", false),
            (MenuKind::Smoker, "minecraft:beef", true),
        ];
        for (kind, item, accepted) in cases {
            let (mut menu, mut container, mut player) = workstation(kind);
            player.set(MAIN_START, Some(stack(item, 1)));
            shift(&mut menu, &mut container, &mut player, 3);
            assert_eq!(container[0].is_some(), accepted, "{kind:?} {item}");
            assert_eq!(
                player.get(HOTBAR_START).is_some(),
                !accepted,
                "{kind:?} {item}"
            );
        }
    }

    #[test]
    fn shift_click_never_falls_through_to_another_slot() {
        let (mut menu, mut container, mut player) = workstation(MenuKind::Furnace);
        container[0] = Some(stack("minecraft:beef", 1));
        player.set(MAIN_START, Some(stack("minecraft:raw_iron", 1)));
        shift(&mut menu, &mut container, &mut player, 3);
        assert_eq!(container[0], Some(stack("minecraft:beef", 1)));
        assert_eq!(container[1], None);
        assert!(player.get(MAIN_START).is_some());
        container[1] = Some(stack("minecraft:stick", 64));
        player.set(MAIN_START, Some(stack("minecraft:coal", 1)));
        shift(&mut menu, &mut container, &mut player, 3);
        assert_eq!(container[1], Some(stack("minecraft:stick", 64)));
        assert!(player.get(MAIN_START).is_some());
    }

    #[test]
    fn the_fuel_slot_takes_fuel_and_empty_buckets_one_at_a_time() {
        let (mut menu, mut container, mut player) = workstation(MenuKind::Furnace);
        for (item, accepted) in [
            ("minecraft:coal", true),
            ("minecraft:bucket", true),
            ("minecraft:water_bucket", false),
            ("minecraft:dirt", false),
        ] {
            container[1] = None;
            menu.carried = Some(stack(item, 2));
            click(
                &mut menu,
                &mut container,
                &mut player,
                1,
                0,
                ClickInput::Pickup,
            );
            assert_eq!(container[1].is_some(), accepted, "{item}");
        }
        container[1] = None;
        menu.carried = Some(stack("minecraft:bucket", 4));
        click(
            &mut menu,
            &mut container,
            &mut player,
            1,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(container[1], Some(stack("minecraft:bucket", 1)));
        assert_eq!(menu.carried(), &Some(stack("minecraft:bucket", 3)));
        container[1] = None;
        menu.carried = Some(stack("minecraft:coal", 100));
        click(
            &mut menu,
            &mut container,
            &mut player,
            1,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(container[1], Some(stack("minecraft:coal", 64)));
    }

    #[test]
    fn taking_from_the_output_slot_reports_the_amount_removed_whichever_way_it_is_taken() {
        let taken = |input: ClickInput, slot: i16, button: i8| {
            let (mut menu, mut container, mut player) = workstation(MenuKind::Furnace);
            container[2] = Some(stack("minecraft:iron_ingot", 5));
            click(&mut menu, &mut container, &mut player, slot, button, input)
                .into_iter()
                .filter_map(|effect| match effect {
                    MenuEffect::FurnaceOutputTaken { item, amount } => Some((item, amount)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let ingot = ItemKind::IronIngot.id();
        assert_eq!(taken(ClickInput::Pickup, 2, 0), [(ingot, 5)]);
        assert_eq!(taken(ClickInput::Pickup, 2, 1), [(ingot, 3)]);
        assert_eq!(taken(ClickInput::QuickMove, 2, 0), [(ingot, 5)]);
        assert_eq!(taken(ClickInput::Swap, 2, 3), [(ingot, 0)]);
        assert_eq!(taken(ClickInput::Throw, 2, 0), [(ingot, 1)]);
        assert_eq!(taken(ClickInput::Throw, 2, 1), [(ingot, 5)]);
        assert!(taken(ClickInput::Pickup, 0, 0).is_empty());
        assert!(taken(ClickInput::Pickup, 20, 0).is_empty());
    }

    #[test]
    fn an_empty_output_slot_reports_nothing_and_a_partial_take_is_refused() {
        let (mut menu, mut container, mut player) = workstation(MenuKind::Furnace);
        let effects = click(
            &mut menu,
            &mut container,
            &mut player,
            2,
            0,
            ClickInput::Pickup,
        );
        assert!(effects.is_empty());
        container[2] = Some(stack("minecraft:iron_ingot", 5));
        menu.carried = Some(stack("minecraft:iron_ingot", 62));
        let effects = click(
            &mut menu,
            &mut container,
            &mut player,
            2,
            0,
            ClickInput::Pickup,
        );
        assert!(effects.is_empty());
        assert_eq!(container[2], Some(stack("minecraft:iron_ingot", 5)));
        menu.carried = Some(stack("minecraft:iron_ingot", 59));
        let effects = click(
            &mut menu,
            &mut container,
            &mut player,
            2,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(
            effects,
            vec![MenuEffect::FurnaceOutputTaken {
                item: ItemKind::IronIngot.id(),
                amount: 5
            }]
        );
        assert_eq!(menu.carried(), &Some(stack("minecraft:iron_ingot", 64)));
    }

    #[test]
    fn the_furnace_slots_accept_items_by_hand_and_give_them_back_with_shift() {
        let (mut menu, mut container, mut player) = workstation(MenuKind::Smoker);
        player.set(MAIN_START, Some(stack("minecraft:beef", 2)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            3,
            0,
            ClickInput::Pickup,
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(container[0].as_ref().map(|s| s.count.get()), Some(2));
        click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::QuickMove,
        );
        assert!(container[0].is_none());
        assert_eq!(player.get(MAIN_START).map(|s| s.count.get()), Some(2));
    }

    #[test]
    fn slots_are_the_container_then_the_main_inventory_then_the_hotbar() {
        let kind = MenuKind::Generic { rows: 3 };
        assert_eq!(kind.container_size(), 27);
        assert_eq!(kind.slot_count(), 27 + 27 + 9);
        let (mut menu, mut container, mut player) = chest();
        player.set(HOTBAR_START, Some(stack("minecraft:diamond", 2)));
        player.set(MAIN_START, Some(stack("minecraft:stick", 3)));
        let (slots, _) = menu.sync_all(&container, &player);
        assert_eq!(slots.len(), 63);
        assert_eq!(slots[27].as_ref().map(|s| s.count.get()), Some(3));
        assert_eq!(slots[54].as_ref().map(|s| s.count.get()), Some(2));
        container[0] = Some(stack("minecraft:stone", 5));
        let (slots, _) = menu.sync_all(&container, &player);
        assert_eq!(slots[0].as_ref().map(|s| s.count.get()), Some(5));
    }

    #[test]
    fn a_click_moves_a_stack_into_the_container_and_back() {
        let (mut menu, mut container, mut player) = chest();
        player.set(MAIN_START, Some(stack("minecraft:cobblestone", 10)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            27,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(menu.carried().as_ref().map(|s| s.count.get()), Some(10));
        click(
            &mut menu,
            &mut container,
            &mut player,
            4,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(container[4].as_ref().map(|s| s.count.get()), Some(10));
        assert!(menu.carried().is_none());
        assert!(player.get(MAIN_START).is_none());
        click(
            &mut menu,
            &mut container,
            &mut player,
            4,
            1,
            ClickInput::Pickup,
        );
        assert_eq!(menu.carried().as_ref().map(|s| s.count.get()), Some(5));
        assert_eq!(container[4].as_ref().map(|s| s.count.get()), Some(5));
    }

    #[test]
    fn shift_click_fills_the_container_forward_and_the_inventory_backward() {
        let (mut menu, mut container, mut player) = chest();
        player.set(HOTBAR_START + 2, Some(stack("minecraft:apple", 4)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            54 + 2,
            0,
            ClickInput::QuickMove,
        );
        assert_eq!(container[0].as_ref().map(|s| s.count.get()), Some(4));
        assert!(player.get(HOTBAR_START + 2).is_none());
        click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::QuickMove,
        );
        assert!(container[0].is_none());
        assert_eq!(player.get(HOTBAR_START + 8).map(|s| s.count.get()), Some(4));
    }

    #[test]
    fn shift_click_tops_up_existing_stacks_first() {
        let (mut menu, mut container, mut player) = chest();
        container[3] = Some(stack("minecraft:cobblestone", 60));
        player.set(MAIN_START, Some(stack("minecraft:cobblestone", 10)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            27,
            0,
            ClickInput::QuickMove,
        );
        assert_eq!(container[3].as_ref().map(|s| s.count.get()), Some(64));
        assert_eq!(container[0].as_ref().map(|s| s.count.get()), Some(6));
    }

    #[test]
    fn number_keys_swap_with_the_hotbar_and_f_with_the_offhand() {
        let (mut menu, mut container, mut player) = chest();
        container[1] = Some(stack("minecraft:gold_ingot", 3));
        player.set(HOTBAR_START + 4, Some(stack("minecraft:iron_ingot", 2)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            1,
            4,
            ClickInput::Swap,
        );
        assert_eq!(
            container[1].as_ref().map(|s| s.item),
            Some(stack("minecraft:iron_ingot", 1).item)
        );
        assert_eq!(player.get(HOTBAR_START + 4).map(|s| s.count.get()), Some(3));
        click(
            &mut menu,
            &mut container,
            &mut player,
            1,
            40,
            ClickInput::Swap,
        );
        assert!(container[1].is_none());
        assert!(player.get(crate::item::OFFHAND_SLOT).is_some());
    }

    #[test]
    fn a_shulker_box_refuses_shulker_boxes_but_the_inventory_side_does_not() {
        let mut menu = ContainerMenu::new(MenuKind::ShulkerBox);
        let mut container = vec![None; 27];
        let mut player = Inventory::default();
        player.set(MAIN_START, Some(stack("minecraft:shulker_box", 1)));
        player.set(MAIN_START + 1, Some(stack("minecraft:stone", 1)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            27,
            0,
            ClickInput::Pickup,
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::Pickup,
        );
        assert!(container[0].is_none());
        assert!(menu.carried().is_some());
        click(
            &mut menu,
            &mut container,
            &mut player,
            28,
            0,
            ClickInput::QuickMove,
        );
        assert_eq!(
            container[0].as_ref().map(|s| s.item),
            Some(stack("minecraft:stone", 1).item)
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            29,
            0,
            ClickInput::Pickup,
        );
        assert!(container.iter().flatten().all(|s| !is_shulker_box(s)));
    }

    #[test]
    fn clicking_outside_throws_the_carried_stack_or_one_of_it() {
        let (mut menu, mut container, mut player) = chest();
        player.set(MAIN_START, Some(stack("minecraft:stone", 8)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            27,
            0,
            ClickInput::Pickup,
        );
        let effects = click(
            &mut menu,
            &mut container,
            &mut player,
            OUTSIDE,
            1,
            ClickInput::Pickup,
        );
        assert_eq!(
            effects,
            vec![MenuEffect::Drop {
                stack: stack("minecraft:stone", 1),
                thrower: true,
                predicted: true
            }]
        );
        let effects = click(
            &mut menu,
            &mut container,
            &mut player,
            OUTSIDE,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(
            effects,
            vec![MenuEffect::Drop {
                stack: stack("minecraft:stone", 7),
                thrower: true,
                predicted: true
            }]
        );
    }

    #[test]
    fn dragging_splits_the_carried_stack_over_container_slots() {
        let (mut menu, mut container, mut player) = chest();
        player.set(MAIN_START, Some(stack("minecraft:stone", 9)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            27,
            0,
            ClickInput::Pickup,
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            OUTSIDE,
            0,
            ClickInput::QuickCraft,
        );
        for slot in [0, 1, 2] {
            click(
                &mut menu,
                &mut container,
                &mut player,
                slot,
                1,
                ClickInput::QuickCraft,
            );
        }
        click(
            &mut menu,
            &mut container,
            &mut player,
            OUTSIDE,
            2,
            ClickInput::QuickCraft,
        );
        for (slot, stack) in container.iter().take(3).enumerate() {
            assert_eq!(stack.as_ref().map(|s| s.count.get()), Some(3), "{slot}");
        }
        assert!(menu.carried().is_none());
    }

    #[test]
    fn double_click_gathers_matching_stacks_from_both_sides() {
        let (mut menu, mut container, mut player) = chest();
        container[5] = Some(stack("minecraft:stone", 10));
        player.set(MAIN_START, Some(stack("minecraft:stone", 20)));
        player.set(MAIN_START + 1, Some(stack("minecraft:dirt", 1)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            28,
            0,
            ClickInput::Pickup,
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            28,
            0,
            ClickInput::Pickup,
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            5,
            0,
            ClickInput::Pickup,
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            6,
            0,
            ClickInput::PickupAll,
        );
        assert_eq!(menu.carried().as_ref().map(|s| s.count.get()), Some(30));
    }

    #[test]
    fn closing_returns_the_carried_stack_to_the_inventory() {
        let (mut menu, mut container, mut player) = chest();
        container[0] = Some(stack("minecraft:diamond", 4));
        click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::Pickup,
        );
        let effects = menu.close(&mut player);
        assert!(menu.carried().is_none());
        assert!(matches!(
            effects.as_slice(),
            [MenuEffect::PlacedBack { .. }]
        ));
        let held: u32 = (0..46)
            .filter_map(|slot| player.get(slot))
            .map(|s| u32::from(s.count.get()))
            .sum();
        assert_eq!(held, 4);
    }

    #[test]
    fn remote_slots_only_report_what_the_client_does_not_already_know() {
        let (mut menu, mut container, player) = chest();
        menu.sync_all(&container, &player);
        assert!(menu.changes(&container, &player).is_empty());
        container[2] = Some(stack("minecraft:stone", 1));
        let updates = menu.changes(&container, &player);
        assert_eq!(updates.len(), 1);
        assert!(menu.changes(&container, &player).is_empty());
        menu.receive_remote(2, HashedSlot::of(&None));
        assert_eq!(menu.changes(&container, &player).len(), 1);
    }

    const STATIONS: [(MenuKind, usize); 6] = [
        (MenuKind::Anvil, 2),
        (MenuKind::Smithing, 3),
        (MenuKind::Grindstone, 2),
        (MenuKind::Stonecutter, 1),
        (MenuKind::CartographyTable, 2),
        (MenuKind::Loom, 3),
    ];

    fn with_component(
        mut stack: ItemStack,
        key: &str,
        value: impl Into<rspigot_nbt::Tag>,
    ) -> ItemStack {
        stack.components.insert(key, value);
        stack
    }

    fn enchanted(name: &str, key: &str) -> ItemStack {
        with_component(
            stack(name, 1),
            key,
            Compound::new().with("minecraft:sharpness", 1),
        )
    }

    fn place_in(
        kind: MenuKind,
        container: &mut Vec<Slot>,
        player: &mut Inventory,
        menu: &mut ContainerMenu,
        slot: usize,
        item: ItemStack,
    ) -> bool {
        player.set(MAIN_START, Some(item));
        click(
            menu,
            container,
            player,
            kind.container_size() as i16,
            0,
            ClickInput::Pickup,
        );
        click(menu, container, player, slot as i16, 0, ClickInput::Pickup);
        let accepted = container[slot].is_some();
        if !accepted {
            click(
                menu,
                container,
                player,
                kind.container_size() as i16,
                0,
                ClickInput::Pickup,
            );
        }
        accepted
    }

    fn accepts_at(kind: MenuKind, slot: usize, item: ItemStack) -> bool {
        let (mut menu, mut container, mut player) = workstation(kind);
        place_in(kind, &mut container, &mut player, &mut menu, slot, item)
    }

    const DONKEY_MENU: MenuKind = MenuKind::Mount {
        columns: 5,
        saddle: true,
        armor: false,
    };

    #[test]
    fn the_mount_menu_holds_a_saddle_slot_a_body_armor_slot_and_three_rows_of_chest_slots() {
        assert_eq!(DONKEY_MENU.container_size(), 17);
        assert_eq!(DONKEY_MENU.slot_count(), 17 + 36);
        let bare = MenuKind::Mount {
            columns: 0,
            saddle: true,
            armor: true,
        };
        assert_eq!(bare.container_size(), 2);
        assert!(accepts_at(bare, 0, stack("minecraft:saddle", 1)));
        assert!(!accepts_at(bare, 0, stack("minecraft:iron_horse_armor", 1)));
        assert!(accepts_at(bare, 1, stack("minecraft:iron_horse_armor", 1)));
        assert!(!accepts_at(bare, 1, stack("minecraft:saddle", 1)));
        assert!(accepts_at(DONKEY_MENU, 2, stack("minecraft:apple", 1)));
        assert!(!accepts_at(
            DONKEY_MENU,
            1,
            stack("minecraft:iron_horse_armor", 1)
        ));
    }

    #[test]
    fn a_saddle_shift_clicked_from_the_inventory_goes_to_the_saddle_slot_and_other_items_to_the_chest()
     {
        let (mut menu, mut container, mut player) = workstation(DONKEY_MENU);
        player.set(MAIN_START, Some(stack("minecraft:saddle", 1)));
        player.set(MAIN_START + 1, Some(stack("minecraft:apple", 5)));
        let size = DONKEY_MENU.container_size();
        let (_, _) = menu.quick_move_once(&mut container, &mut player, size, Clicker::default());
        assert_eq!(container[0], Some(stack("minecraft:saddle", 1)));
        menu.quick_move_once(&mut container, &mut player, size + 1, Clicker::default());
        assert_eq!(container[2], Some(stack("minecraft:apple", 5)));
        assert!(player.get(MAIN_START + 1).is_none());
        menu.quick_move_once(&mut container, &mut player, 2, Clicker::default());
        assert!(container[2].is_none());
        assert_eq!(
            player.get(HOTBAR_START + HOTBAR_SLOTS - 1),
            Some(&stack("minecraft:apple", 5))
        );
    }

    #[test]
    fn the_mount_saddle_and_armor_slots_hold_one_item() {
        let (mut menu, mut container, mut player) = workstation(DONKEY_MENU);
        player.set(MAIN_START, Some(stack("minecraft:saddle", 1)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            17,
            0,
            ClickInput::Pickup,
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(container[0], Some(stack("minecraft:saddle", 1)));
    }

    #[test]
    fn the_station_menu_ids_match_the_registry() {
        let registry = |name: &str| {
            let index = rspigot_data::registries::static_index("minecraft:menu", name).expect(name);
            i32::try_from(index).expect("small id")
        };
        assert_eq!(MenuKind::Anvil.protocol_id(), registry("minecraft:anvil"));
        assert_eq!(
            MenuKind::Smithing.protocol_id(),
            registry("minecraft:smithing")
        );
        assert_eq!(
            MenuKind::Grindstone.protocol_id(),
            registry("minecraft:grindstone")
        );
        assert_eq!(
            MenuKind::Stonecutter.protocol_id(),
            registry("minecraft:stonecutter")
        );
        assert_eq!(
            MenuKind::CartographyTable.protocol_id(),
            registry("minecraft:cartography_table")
        );
        assert_eq!(MenuKind::Loom.protocol_id(), registry("minecraft:loom"));
    }

    #[test]
    fn each_station_has_its_vanilla_slots_and_data_slots() {
        let expected = [
            (MenuKind::Anvil, 3, 1),
            (MenuKind::Smithing, 4, 1),
            (MenuKind::Grindstone, 3, 0),
            (MenuKind::Stonecutter, 2, 1),
            (MenuKind::CartographyTable, 3, 0),
            (MenuKind::Loom, 4, 1),
        ];
        for (kind, container, data) in expected {
            assert_eq!(kind.container_size(), container, "{kind:?}");
            assert_eq!(kind.slot_count(), container + 36, "{kind:?}");
            assert_eq!(kind.data_count(), data, "{kind:?}");
        }
    }

    #[test]
    fn the_result_slot_of_a_station_takes_nothing() {
        for (kind, result) in STATIONS {
            let accepted = accepts_at(kind, result, stack("minecraft:stone", 1));
            assert!(!accepted, "{kind:?}");
        }
    }

    #[test]
    fn closing_a_station_gives_the_inputs_back_and_keeps_the_result() {
        for (kind, result) in STATIONS {
            let (mut menu, mut container, mut player) = workstation(kind);
            for entry in container.iter_mut().take(result + 1) {
                *entry = Some(stack("minecraft:stone", 1));
            }
            menu.close_transient(&mut container, &mut player);
            assert!(container[..result].iter().all(Option::is_none), "{kind:?}");
            assert!(container[result].is_some(), "{kind:?}");
            let held: u32 = (0..46)
                .filter_map(|slot| player.get(slot))
                .map(|s| u32::from(s.count.get()))
                .sum();
            assert_eq!(held, result as u32, "{kind:?}");
        }
    }

    fn shift_click(kind: MenuKind, container: &mut Vec<Slot>, player: &mut Inventory, slot: usize) {
        let mut menu = ContainerMenu::new(kind);
        click(
            &mut menu,
            container,
            player,
            slot as i16,
            0,
            ClickInput::QuickMove,
        );
    }

    #[test]
    fn shift_clicking_a_station_result_fills_the_inventory_from_the_end() {
        for (kind, result) in STATIONS {
            let (mut container, mut player) =
                (vec![None; kind.container_size()], Inventory::default());
            container[result] = Some(stack("minecraft:stone", 1));
            shift_click(kind, &mut container, &mut player, result);
            assert!(container[result].is_none(), "{kind:?}");
            assert_eq!(
                player.get(HOTBAR_START + 8).map(|s| s.count.get()),
                Some(1),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn the_anvil_inputs_take_any_item_and_shift_click_fills_them_before_anything_else() {
        let kind = MenuKind::Anvil;
        let (mut menu, mut container, mut player) = workstation(kind);
        assert!(place_in(
            kind,
            &mut container,
            &mut player,
            &mut menu,
            0,
            stack("minecraft:stone", 1)
        ));
        assert!(place_in(
            kind,
            &mut container,
            &mut player,
            &mut menu,
            1,
            stack("minecraft:stick", 1)
        ));

        let (mut container, mut player) = (vec![None; 3], Inventory::default());
        player.set(MAIN_START, Some(stack("minecraft:stone", 2)));
        player.set(HOTBAR_START, Some(stack("minecraft:stick", 3)));
        player.set(HOTBAR_START + 1, Some(stack("minecraft:dirt", 4)));
        shift_click(kind, &mut container, &mut player, 3);
        shift_click(kind, &mut container, &mut player, 30);
        assert_eq!(container[0].as_ref().map(|s| s.count.get()), Some(2));
        assert_eq!(container[1].as_ref().map(|s| s.count.get()), Some(3));
        shift_click(kind, &mut container, &mut player, 31);
        assert_eq!(player.get(HOTBAR_START + 1).map(|s| s.count.get()), Some(4));
        assert!(player.get(MAIN_START).is_none());
        shift_click(kind, &mut container, &mut player, 0);
        assert!(container[0].is_none());
        assert_eq!(player.get(MAIN_START).map(|s| s.count.get()), Some(2));
    }

    #[test]
    fn the_smithing_slots_take_only_what_a_smithing_recipe_uses() {
        let kind = MenuKind::Smithing;
        let template = "minecraft:netherite_upgrade_smithing_template";
        let trim = "minecraft:bolt_armor_trim_smithing_template";
        assert!(accepts_at(kind, 0, stack(template, 1)));
        assert!(accepts_at(kind, 0, stack(trim, 1)));
        assert!(!accepts_at(kind, 0, stack("minecraft:diamond_sword", 1)));
        assert!(accepts_at(kind, 1, stack("minecraft:diamond_sword", 1)));
        assert!(accepts_at(kind, 1, stack("minecraft:iron_helmet", 1)));
        assert!(!accepts_at(kind, 1, stack("minecraft:netherite_ingot", 1)));
        assert!(accepts_at(kind, 2, stack("minecraft:netherite_ingot", 1)));
        assert!(accepts_at(kind, 2, stack("minecraft:copper_ingot", 1)));
        assert!(!accepts_at(kind, 2, stack("minecraft:stick", 1)));
        assert!(!accepts_at(kind, 3, stack("minecraft:netherite_ingot", 1)));
    }

    #[test]
    fn the_grindstone_takes_only_damageable_or_enchanted_items() {
        let kind = MenuKind::Grindstone;
        for slot in [0, 1] {
            assert!(
                accepts_at(kind, slot, stack("minecraft:iron_pickaxe", 1)),
                "pickaxe {slot}"
            );
            assert!(
                accepts_at(
                    kind,
                    slot,
                    enchanted("minecraft:stone", "minecraft:enchantments")
                ),
                "enchanted {slot}"
            );
            assert!(
                accepts_at(
                    kind,
                    slot,
                    enchanted("minecraft:book", "minecraft:stored_enchantments")
                ),
                "stored {slot}"
            );
            assert!(
                !accepts_at(kind, slot, stack("minecraft:stick", 1)),
                "stick {slot}"
            );
            assert!(
                !accepts_at(
                    kind,
                    slot,
                    with_component(
                        stack("minecraft:stone", 1),
                        "minecraft:enchantments",
                        Compound::new()
                    )
                ),
                "empty {slot}"
            );
            assert!(
                !accepts_at(
                    kind,
                    slot,
                    with_component(
                        stack("minecraft:iron_pickaxe", 1),
                        "minecraft:unbreakable",
                        Compound::new()
                    )
                ),
                "unbreakable {slot}"
            );
        }
    }

    #[test]
    fn grindstone_shift_click_fills_the_inputs_then_swaps_main_and_hotbar() {
        let kind = MenuKind::Grindstone;
        let (mut container, mut player) = (vec![None; 3], Inventory::default());
        player.set(MAIN_START, Some(stack("minecraft:stick", 1)));
        shift_click(kind, &mut container, &mut player, 3);
        assert!(container.iter().all(Option::is_none));
        assert!(player.get(MAIN_START).is_some());

        player.set(MAIN_START + 1, Some(stack("minecraft:iron_pickaxe", 1)));
        shift_click(kind, &mut container, &mut player, 4);
        assert!(container[0].is_some());
        player.set(MAIN_START + 2, Some(stack("minecraft:iron_axe", 1)));
        shift_click(kind, &mut container, &mut player, 5);
        assert!(container[1].is_some());

        shift_click(kind, &mut container, &mut player, 3);
        assert!(player.get(MAIN_START).is_none());
        assert!(player.get(HOTBAR_START).is_some());
    }

    #[test]
    fn the_stonecutter_input_takes_any_item_and_shift_click_swaps_what_it_cannot_cut() {
        let kind = MenuKind::Stonecutter;
        assert!(accepts_at(kind, 0, stack("minecraft:stone", 1)));
        assert!(accepts_at(kind, 0, stack("minecraft:stick", 1)));
        let (mut container, mut player) = (vec![None; 2], Inventory::default());
        player.set(MAIN_START, Some(stack("minecraft:dirt", 5)));
        shift_click(kind, &mut container, &mut player, 2);
        assert!(container.iter().all(Option::is_none));
        assert_eq!(player.get(HOTBAR_START).map(|s| s.count.get()), Some(5));
        shift_click(kind, &mut container, &mut player, 29);
        assert_eq!(player.get(MAIN_START).map(|s| s.count.get()), Some(5));
    }

    fn cutter() -> (ContainerMenu, Vec<Slot>, Inventory) {
        workstation(MenuKind::Stonecutter)
    }

    fn load_cutter(
        menu: &mut ContainerMenu,
        container: &mut Vec<Slot>,
        player: &mut Inventory,
        item: ItemStack,
    ) {
        player.set(MAIN_START, Some(item));
        click(menu, container, player, 2, 0, ClickInput::Pickup);
        click(menu, container, player, 0, 0, ClickInput::Pickup);
    }

    #[test]
    fn shift_click_sends_what_can_be_cut_to_the_input_and_never_elsewhere() {
        let (mut menu, mut container, mut player) = cutter();
        player.set(MAIN_START, Some(stack("minecraft:stone", 5)));
        shift(&mut menu, &mut container, &mut player, 2);
        assert_eq!(container[0], Some(stack("minecraft:stone", 5)));
        player.set(MAIN_START, Some(stack("minecraft:cobblestone", 1)));
        shift(&mut menu, &mut container, &mut player, 2);
        assert_eq!(container[0], Some(stack("minecraft:stone", 5)));
        assert!(player.get(MAIN_START).is_some());
    }

    #[test]
    fn choosing_a_recipe_puts_its_result_in_the_output_slot() {
        let (mut menu, mut container, mut player) = cutter();
        assert_eq!(menu.data(), vec![0]);
        load_cutter(
            &mut menu,
            &mut container,
            &mut player,
            stack("minecraft:stone", 3),
        );
        assert_eq!(menu.data(), vec![-1]);
        assert_eq!(container[1], None);
        assert!(menu.press(&mut container, 0));
        assert_eq!(
            container[1],
            Some(stack("minecraft:chiseled_stone_bricks", 1))
        );
        assert_eq!(menu.data(), vec![0]);
        assert!(menu.press(&mut container, 5));
        assert_eq!(container[1], Some(stack("minecraft:stone_brick_slab", 2)));
        assert_eq!(menu.data(), vec![5]);
    }

    #[test]
    fn choosing_the_same_recipe_again_or_one_that_does_not_exist_changes_nothing() {
        let (mut menu, mut container, mut player) = cutter();
        assert!(menu.press(&mut container, 0) || container[1].is_none());
        load_cutter(
            &mut menu,
            &mut container,
            &mut player,
            stack("minecraft:stone", 3),
        );
        menu.press(&mut container, 2);
        assert!(!menu.press(&mut container, 2));
        let result = container[1].clone();
        assert!(menu.press(&mut container, 11));
        assert!(menu.press(&mut container, -1));
        assert!(menu.press(&mut container, 100));
        assert_eq!(container[1], result);
        assert_eq!(menu.data(), vec![2]);
    }

    #[test]
    fn nothing_can_be_chosen_without_an_input() {
        let (mut menu, mut container, _player) = cutter();
        assert!(!menu.press(&mut container, 0));
        assert!(menu.press(&mut container, 1));
        assert_eq!(container[1], None);
        assert_eq!(menu.data(), vec![0]);
        let mut other = ContainerMenu::new(MenuKind::Generic { rows: 3 });
        assert!(!other.press(&mut vec![None; 27], 0));
    }

    #[test]
    fn the_choice_is_forgotten_when_the_input_changes_kind_but_not_when_it_changes_count() {
        let (mut menu, mut container, mut player) = cutter();
        load_cutter(
            &mut menu,
            &mut container,
            &mut player,
            stack("minecraft:stone", 3),
        );
        menu.press(&mut container, 3);
        player.set(MAIN_START, Some(stack("minecraft:stone", 2)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            2,
            0,
            ClickInput::Pickup,
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(menu.data(), vec![3]);
        assert!(container[1].is_some());
        click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(menu.data(), vec![-1]);
        assert_eq!(container[1], None);
        player.set(MAIN_START, Some(stack("minecraft:cobblestone", 2)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::Pickup,
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            2,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(menu.data(), vec![-1]);
    }

    #[test]
    fn taking_the_result_uses_exactly_one_input_whatever_the_count() {
        let (mut menu, mut container, mut player) = cutter();
        load_cutter(
            &mut menu,
            &mut container,
            &mut player,
            stack("minecraft:stone", 3),
        );
        menu.press(&mut container, 10);
        assert_eq!(container[1], Some(stack("minecraft:stone_stairs", 1)));
        menu.press(&mut container, 9);
        assert_eq!(container[1], Some(stack("minecraft:stone_slab", 2)));
        let effects = click(
            &mut menu,
            &mut container,
            &mut player,
            1,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(effects, vec![MenuEffect::StonecutterTaken]);
        assert_eq!(menu.carried(), &Some(stack("minecraft:stone_slab", 2)));
        assert_eq!(container[0], Some(stack("minecraft:stone", 2)));
        assert_eq!(container[1], Some(stack("minecraft:stone_slab", 2)));
        assert_eq!(menu.data(), vec![9]);
    }

    #[test]
    fn taking_the_last_input_empties_the_output_and_the_choice() {
        let (mut menu, mut container, mut player) = cutter();
        load_cutter(
            &mut menu,
            &mut container,
            &mut player,
            stack("minecraft:stone", 1),
        );
        menu.press(&mut container, 0);
        click(
            &mut menu,
            &mut container,
            &mut player,
            1,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(container[0], None);
        assert_eq!(container[1], None);
        assert_eq!(menu.data(), vec![-1]);
        assert_eq!(
            menu.carried(),
            &Some(stack("minecraft:chiseled_stone_bricks", 1))
        );
    }

    #[test]
    fn shift_click_on_the_result_cuts_until_the_stack_is_gone() {
        let (mut menu, mut container, mut player) = cutter();
        load_cutter(
            &mut menu,
            &mut container,
            &mut player,
            stack("minecraft:stone", 3),
        );
        menu.press(&mut container, 9);
        click(
            &mut menu,
            &mut container,
            &mut player,
            1,
            0,
            ClickInput::QuickMove,
        );
        assert_eq!(container[0], None);
        assert_eq!(container[1], None);
        let slabs: u32 = (9..45)
            .filter_map(|slot| player.get(slot))
            .map(|stack| u32::from(stack.count.get()))
            .sum();
        assert_eq!(slabs, 6);
    }

    #[test]
    fn what_does_not_fit_of_a_shift_clicked_cut_is_dropped_and_the_input_is_still_used() {
        let (mut menu, mut container, mut player) = cutter();
        load_cutter(
            &mut menu,
            &mut container,
            &mut player,
            stack("minecraft:stone", 1),
        );
        menu.press(&mut container, 9);
        for slot in 9..45 {
            player.set(slot, Some(stack("minecraft:dirt", 64)));
        }
        player.set(20, Some(stack("minecraft:stone_slab", 63)));
        let effects = click(
            &mut menu,
            &mut container,
            &mut player,
            1,
            0,
            ClickInput::QuickMove,
        );
        assert!(effects.contains(&MenuEffect::Drop {
            stack: stack("minecraft:stone_slab", 1),
            thrower: false,
            predicted: true,
        }));
        assert_eq!(container[0], None);
    }

    #[test]
    fn the_output_slot_takes_nothing_by_hand_and_nothing_in_parts() {
        let (mut menu, mut container, mut player) = cutter();
        load_cutter(
            &mut menu,
            &mut container,
            &mut player,
            stack("minecraft:stone", 3),
        );
        menu.press(&mut container, 9);
        menu.carried = Some(stack("minecraft:stone_slab", 63));
        click(
            &mut menu,
            &mut container,
            &mut player,
            1,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(menu.carried(), &Some(stack("minecraft:stone_slab", 63)));
        assert_eq!(container[0], Some(stack("minecraft:stone", 3)));
        menu.carried = Some(stack("minecraft:dirt", 1));
        click(
            &mut menu,
            &mut container,
            &mut player,
            1,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(menu.carried(), &Some(stack("minecraft:dirt", 1)));
    }

    fn smithing() -> (ContainerMenu, Vec<Slot>, Inventory) {
        workstation(MenuKind::Smithing)
    }

    fn put_into(
        menu: &mut ContainerMenu,
        container: &mut Vec<Slot>,
        player: &mut Inventory,
        slot: i16,
        item: ItemStack,
    ) {
        player.set(MAIN_START, Some(item));
        click(menu, container, player, 4, 0, ClickInput::Pickup);
        click(menu, container, player, slot, 0, ClickInput::Pickup);
    }

    fn upgrade_inputs(
        menu: &mut ContainerMenu,
        container: &mut Vec<Slot>,
        player: &mut Inventory,
        templates: u8,
    ) {
        put_into(
            menu,
            container,
            player,
            0,
            stack("minecraft:netherite_upgrade_smithing_template", templates),
        );
        put_into(
            menu,
            container,
            player,
            1,
            stack("minecraft:diamond_sword", 1),
        );
        put_into(
            menu,
            container,
            player,
            2,
            stack("minecraft:netherite_ingot", 3),
        );
    }

    #[test]
    fn three_matching_inputs_show_the_smithed_item() {
        let (mut menu, mut container, mut player) = smithing();
        assert_eq!(menu.data(), vec![0]);
        upgrade_inputs(&mut menu, &mut container, &mut player, 1);
        assert_eq!(container[3], Some(stack("minecraft:netherite_sword", 1)));
        assert_eq!(menu.data(), vec![0]);
        click(
            &mut menu,
            &mut container,
            &mut player,
            1,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(container[3], None);
        assert_eq!(menu.data(), vec![0]);
    }

    #[test]
    fn three_inputs_that_make_nothing_raise_the_error_flag() {
        let (mut menu, mut container, mut player) = smithing();
        let mut helmet = stack("minecraft:iron_helmet", 1);
        helmet.components.insert(
            "minecraft:trim",
            Compound::new()
                .with("material", "minecraft:copper")
                .with("pattern", "minecraft:bolt"),
        );
        put_into(
            &mut menu,
            &mut container,
            &mut player,
            0,
            stack("minecraft:bolt_armor_trim_smithing_template", 1),
        );
        put_into(&mut menu, &mut container, &mut player, 1, helmet);
        assert_eq!(menu.data(), vec![0]);
        put_into(
            &mut menu,
            &mut container,
            &mut player,
            2,
            stack("minecraft:copper_ingot", 1),
        );
        assert_eq!(container[3], None);
        assert_eq!(menu.data(), vec![1]);
        put_into(
            &mut menu,
            &mut container,
            &mut player,
            2,
            stack("minecraft:gold_ingot", 1),
        );
        assert!(container[3].is_some());
        assert_eq!(menu.data(), vec![0]);
    }

    #[test]
    fn taking_the_result_uses_one_of_each_input_the_template_included() {
        let (mut menu, mut container, mut player) = smithing();
        upgrade_inputs(&mut menu, &mut container, &mut player, 1);
        let effects = click(
            &mut menu,
            &mut container,
            &mut player,
            3,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(effects, vec![MenuEffect::SmithingTaken]);
        assert_eq!(menu.carried(), &Some(stack("minecraft:netherite_sword", 1)));
        assert_eq!(container[0], None);
        assert_eq!(container[1], None);
        assert_eq!(container[2], Some(stack("minecraft:netherite_ingot", 2)));
        assert_eq!(container[3], None);
        assert_eq!(menu.data(), vec![0]);
    }

    #[test]
    fn the_result_is_made_again_while_every_input_remains() {
        let (mut menu, mut container, mut player) = smithing();
        put_into(
            &mut menu,
            &mut container,
            &mut player,
            0,
            stack("minecraft:netherite_upgrade_smithing_template", 2),
        );
        put_into(
            &mut menu,
            &mut container,
            &mut player,
            1,
            stack("minecraft:diamond_sword", 1),
        );
        put_into(
            &mut menu,
            &mut container,
            &mut player,
            2,
            stack("minecraft:netherite_ingot", 3),
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            3,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(
            container[0],
            Some(stack("minecraft:netherite_upgrade_smithing_template", 1))
        );
        assert_eq!(container[1], None);
        assert_eq!(container[3], None);
    }

    #[test]
    fn shift_click_sends_each_input_to_the_slot_it_belongs_to() {
        let (mut menu, mut container, mut player) = smithing();
        for (offset, name) in [
            "minecraft:netherite_ingot",
            "minecraft:diamond_sword",
            "minecraft:netherite_upgrade_smithing_template",
        ]
        .into_iter()
        .enumerate()
        {
            player.set(MAIN_START + offset, Some(stack(name, 1)));
        }
        for slot in [4, 5, 6] {
            shift(&mut menu, &mut container, &mut player, slot);
        }
        assert_eq!(
            container[0],
            Some(stack("minecraft:netherite_upgrade_smithing_template", 1))
        );
        assert_eq!(container[1], Some(stack("minecraft:diamond_sword", 1)));
        assert_eq!(container[2], Some(stack("minecraft:netherite_ingot", 1)));
        assert_eq!(container[3], Some(stack("minecraft:netherite_sword", 1)));
    }

    #[test]
    fn shift_click_does_not_fall_through_when_the_slot_is_busy_or_the_item_unrelated() {
        let (mut menu, mut container, mut player) = smithing();
        container[0] = Some(stack("minecraft:netherite_upgrade_smithing_template", 1));
        player.set(
            MAIN_START,
            Some(stack("minecraft:bolt_armor_trim_smithing_template", 1)),
        );
        shift(&mut menu, &mut container, &mut player, 4);
        assert_eq!(
            container[0],
            Some(stack("minecraft:netherite_upgrade_smithing_template", 1))
        );
        assert!(player.get(HOTBAR_START).is_some());
        player.set(MAIN_START + 1, Some(stack("minecraft:dirt", 2)));
        shift(&mut menu, &mut container, &mut player, 5);
        assert!(container[1].is_none());
        assert_eq!(player.get(HOTBAR_START + 1).map(|s| s.count.get()), Some(2));
        shift(&mut menu, &mut container, &mut player, 32);
        assert_eq!(player.get(MAIN_START).map(|s| s.count.get()), Some(2));
    }

    #[test]
    fn shift_click_on_the_result_takes_it_and_uses_the_inputs() {
        let (mut menu, mut container, mut player) = smithing();
        upgrade_inputs(&mut menu, &mut container, &mut player, 1);
        let effects = shift(&mut menu, &mut container, &mut player, 3);
        assert_eq!(effects, vec![MenuEffect::SmithingTaken]);
        assert_eq!(
            player.get(HOTBAR_START + 8),
            Some(&stack("minecraft:netherite_sword", 1))
        );
        assert_eq!(container[0], None);
        assert_eq!(container[2], Some(stack("minecraft:netherite_ingot", 2)));
    }

    #[test]
    fn the_cartography_table_takes_a_filled_map_then_paper_a_map_or_a_glass_pane() {
        let kind = MenuKind::CartographyTable;
        let filled = with_component(stack("minecraft:filled_map", 1), "minecraft:map_id", 3);
        assert!(accepts_at(kind, 0, filled.clone()));
        assert!(!accepts_at(kind, 0, stack("minecraft:filled_map", 1)));
        assert!(!accepts_at(kind, 0, stack("minecraft:paper", 1)));
        assert!(!accepts_at(kind, 1, filled));
        for name in ["minecraft:paper", "minecraft:map", "minecraft:glass_pane"] {
            assert!(accepts_at(kind, 1, stack(name, 1)), "{name}");
        }
        assert!(!accepts_at(kind, 1, stack("minecraft:stone", 1)));
    }

    #[test]
    fn cartography_shift_click_routes_maps_and_materials_to_their_slot() {
        let kind = MenuKind::CartographyTable;
        let (mut container, mut player) = (vec![None; 3], Inventory::default());
        player.set(
            MAIN_START,
            Some(with_component(
                stack("minecraft:filled_map", 1),
                "minecraft:map_id",
                3,
            )),
        );
        player.set(MAIN_START + 1, Some(stack("minecraft:paper", 2)));
        player.set(MAIN_START + 2, Some(stack("minecraft:stone", 2)));
        shift_click(kind, &mut container, &mut player, 3);
        shift_click(kind, &mut container, &mut player, 4);
        shift_click(kind, &mut container, &mut player, 5);
        assert!(container[0].is_some());
        assert_eq!(container[1].as_ref().map(|s| s.count.get()), Some(2));
        assert!(player.get(MAIN_START + 2).is_none());
        assert_eq!(player.get(HOTBAR_START).map(|s| s.count.get()), Some(2));
    }

    #[test]
    fn the_loom_takes_a_banner_a_dye_and_a_pattern_in_their_own_slot() {
        let kind = MenuKind::Loom;
        assert!(accepts_at(kind, 0, stack("minecraft:white_banner", 1)));
        assert!(accepts_at(kind, 0, stack("minecraft:red_banner", 1)));
        assert!(!accepts_at(kind, 0, stack("minecraft:red_dye", 1)));
        assert!(accepts_at(kind, 1, stack("minecraft:red_dye", 1)));
        assert!(accepts_at(kind, 1, stack("minecraft:black_dye", 1)));
        assert!(!accepts_at(kind, 1, stack("minecraft:white_banner", 1)));
        assert!(accepts_at(
            kind,
            2,
            stack("minecraft:creeper_banner_pattern", 1)
        ));
        assert!(!accepts_at(kind, 2, stack("minecraft:red_dye", 1)));
        assert!(!accepts_at(kind, 2, stack("minecraft:stone", 1)));
    }

    #[test]
    fn loom_shift_click_routes_each_item_to_its_slot_and_swaps_the_rest() {
        let kind = MenuKind::Loom;
        let (mut container, mut player) = (vec![None; 4], Inventory::default());
        player.set(MAIN_START, Some(stack("minecraft:white_banner", 1)));
        player.set(MAIN_START + 1, Some(stack("minecraft:red_dye", 3)));
        player.set(
            MAIN_START + 2,
            Some(stack("minecraft:creeper_banner_pattern", 1)),
        );
        player.set(MAIN_START + 3, Some(stack("minecraft:stone", 1)));
        for slot in 4..8 {
            shift_click(kind, &mut container, &mut player, slot);
        }
        assert!(container[0].is_some());
        assert_eq!(container[1].as_ref().map(|s| s.count.get()), Some(3));
        assert!(container[2].is_some());
        assert!(container[3].is_none());
        assert_eq!(player.get(HOTBAR_START).map(|s| s.count.get()), Some(1));
        assert!(player.get(MAIN_START + 3).is_none());
    }

    #[test]
    fn the_lectern_menu_id_matches_the_registry() {
        let index = rspigot_data::registries::static_index("minecraft:menu", "minecraft:lectern")
            .expect("lectern");
        assert_eq!(
            MenuKind::Lectern.protocol_id(),
            i32::try_from(index).expect("small id")
        );
    }

    #[test]
    fn the_lectern_menu_is_one_book_slot_with_a_page_and_no_player_inventory() {
        assert_eq!(MenuKind::Lectern.container_size(), 1);
        assert_eq!(MenuKind::Lectern.slot_count(), 1);
        assert_eq!(MenuKind::Lectern.data_count(), 1);
        let (mut menu, container, player) = workstation(MenuKind::Lectern);
        let mut container = container;
        container[0] = Some(stack("minecraft:written_book", 1));
        let (slots, _) = menu.sync_all(&container, &player);
        assert_eq!(slots.len(), 1);
        assert!(slots[0].is_some());
    }

    #[test]
    fn clicks_outside_the_single_lectern_slot_change_nothing() {
        let (mut menu, mut container, mut player) = workstation(MenuKind::Lectern);
        player.set(MAIN_START, Some(stack("minecraft:stone", 3)));
        for slot in [1, 2, 10, 36] {
            click(
                &mut menu,
                &mut container,
                &mut player,
                slot,
                0,
                ClickInput::Pickup,
            );
            click(
                &mut menu,
                &mut container,
                &mut player,
                slot,
                0,
                ClickInput::QuickMove,
            );
        }
        assert!(menu.carried().is_none());
        assert!(container[0].is_none());
        assert_eq!(player.get(MAIN_START).map(|s| s.count.get()), Some(3));
    }

    #[test]
    fn shift_click_on_the_lectern_book_moves_nothing() {
        let (mut menu, mut container, mut player) = workstation(MenuKind::Lectern);
        container[0] = Some(stack("minecraft:written_book", 1));
        click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::QuickMove,
        );
        assert!(container[0].is_some());
        assert!((0..46).all(|slot| player.get(slot).is_none()));
    }

    #[test]
    fn the_beacon_menu_id_matches_the_registry() {
        let index = rspigot_data::registries::static_index("minecraft:menu", "minecraft:beacon")
            .expect("beacon");
        assert_eq!(
            MenuKind::Beacon.protocol_id(),
            i32::try_from(index).expect("small id")
        );
    }

    #[test]
    fn the_beacon_menu_is_one_payment_slot_over_the_inventory_with_three_data_slots() {
        assert_eq!(MenuKind::Beacon.container_size(), 1);
        assert_eq!(MenuKind::Beacon.slot_count(), 37);
        assert_eq!(MenuKind::Beacon.data_count(), 3);
    }

    #[test]
    fn the_beacon_payment_slot_takes_only_payment_items() {
        let kind = MenuKind::Beacon;
        for name in [
            "minecraft:iron_ingot",
            "minecraft:gold_ingot",
            "minecraft:emerald",
            "minecraft:diamond",
            "minecraft:netherite_ingot",
        ] {
            assert!(accepts_at(kind, 0, stack(name, 1)), "{name}");
        }
        for name in [
            "minecraft:stick",
            "minecraft:iron_block",
            "minecraft:copper_ingot",
        ] {
            assert!(!accepts_at(kind, 0, stack(name, 1)), "{name}");
        }
    }

    #[test]
    fn the_beacon_payment_slot_holds_a_single_item() {
        let (mut menu, mut container, mut player) = workstation(MenuKind::Beacon);
        player.set(MAIN_START, Some(stack("minecraft:iron_ingot", 5)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            1,
            0,
            ClickInput::Pickup,
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(container[0].as_ref().map(|s| s.count.get()), Some(1));
        assert_eq!(menu.carried().as_ref().map(|s| s.count.get()), Some(4));
        click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(container[0].as_ref().map(|s| s.count.get()), Some(1));
        assert_eq!(menu.carried().as_ref().map(|s| s.count.get()), Some(4));
    }

    #[test]
    fn beacon_shift_click_fills_the_payment_slot_with_a_single_item_and_swaps_the_rest() {
        let kind = MenuKind::Beacon;
        let (mut container, mut player) = (vec![None; 1], Inventory::default());
        player.set(MAIN_START, Some(stack("minecraft:iron_ingot", 2)));
        player.set(MAIN_START + 1, Some(stack("minecraft:stick", 1)));
        player.set(MAIN_START + 2, Some(stack("minecraft:gold_ingot", 1)));
        player.set(MAIN_START + 3, Some(stack("minecraft:emerald", 1)));
        shift_click(kind, &mut container, &mut player, 1);
        assert!(container[0].is_none());
        assert_eq!(player.get(HOTBAR_START).map(|s| s.count.get()), Some(2));
        shift_click(kind, &mut container, &mut player, 2);
        assert!(container[0].is_none());
        shift_click(kind, &mut container, &mut player, 3);
        assert_eq!(
            container[0].as_ref().map(|s| s.item),
            Some(stack("minecraft:gold_ingot", 1).item)
        );
        shift_click(kind, &mut container, &mut player, 4);
        assert_eq!(
            container[0].as_ref().map(|s| s.item),
            Some(stack("minecraft:gold_ingot", 1).item)
        );
        assert!(player.get(MAIN_START + 3).is_none());
        shift_click(kind, &mut container, &mut player, 0);
        assert!(container[0].is_none());
        assert!(player.get(HOTBAR_START + 8).is_some());
    }

    #[test]
    fn closing_the_beacon_throws_the_payment_instead_of_giving_it_back() {
        let (mut menu, mut container, mut player) = workstation(MenuKind::Beacon);
        container[0] = Some(stack("minecraft:diamond", 1));
        let effects = menu.close_transient(&mut container, &mut player);
        assert!(container[0].is_none());
        assert!(matches!(
            effects.as_slice(),
            [MenuEffect::Drop {
                thrower: false,
                predicted: false,
                ..
            }]
        ));
        assert!((0..46).all(|slot| player.get(slot).is_none()));
    }

    fn offer(cost: (&str, u8), second: Option<(&str, u8)>, result: (&str, u8)) -> MerchantOffer {
        let item = |name: &str| ItemKind::from_name(name).expect("known item").id();
        MerchantOffer::new(
            crate::entity::trading::ItemCost::new(item(cost.0), i32::from(cost.1)),
            second.map(|(name, count)| {
                crate::entity::trading::ItemCost::new(item(name), i32::from(count))
            }),
            stack(result.0, result.1),
            3,
            2,
            0.05,
        )
    }

    fn trading() -> (ContainerMenu, Vec<Slot>, Inventory) {
        let mut menu = ContainerMenu::new(MenuKind::Merchant);
        let mut container = vec![None; 3];
        let mut player = Inventory::default();
        let mut offers = MerchantOffers::new();
        offers.push(offer(
            ("minecraft:emerald", 2),
            None,
            ("minecraft:bread", 3),
        ));
        offers.push(offer(
            ("minecraft:emerald", 5),
            Some(("minecraft:book", 1)),
            ("minecraft:iron_sword", 1),
        ));
        menu.set_offers(&mut container, &mut player, offers);
        (menu, container, player)
    }

    #[test]
    fn the_merchant_menu_is_three_trade_slots_over_the_inventory() {
        let registry =
            rspigot_data::registries::static_index("minecraft:menu", "minecraft:merchant").unwrap();
        assert_eq!(MenuKind::Merchant.protocol_id(), registry as i32);
        assert_eq!(MenuKind::Merchant.container_size(), 3);
        assert_eq!(MenuKind::Merchant.slot_count(), 39);
        assert_eq!(MenuKind::Merchant.data_count(), 0);
    }

    #[test]
    fn payment_that_satisfies_an_offer_shows_its_result_and_experience() {
        let (mut menu, mut container, mut player) = trading();
        player.set(MAIN_START, Some(stack("minecraft:emerald", 8)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            3,
            0,
            ClickInput::Pickup,
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(container[2], Some(stack("minecraft:bread", 3)));
        assert_eq!(menu.future_trade_xp(), 2);
        click(
            &mut menu,
            &mut container,
            &mut player,
            0,
            0,
            ClickInput::Pickup,
        );
        assert!(container[2].is_none());
        assert_eq!(menu.future_trade_xp(), 0);
    }

    #[test]
    fn the_result_cannot_be_placed_into_and_needs_a_matching_payment() {
        let (mut menu, mut container, mut player) = trading();
        menu.carried = Some(stack("minecraft:bread", 3));
        click(
            &mut menu,
            &mut container,
            &mut player,
            2,
            0,
            ClickInput::Pickup,
        );
        assert!(container[2].is_none());
        assert_eq!(menu.carried(), &Some(stack("minecraft:bread", 3)));
    }

    #[test]
    fn taking_the_result_uses_the_payment_counts_the_trade_and_shows_the_next_result() {
        let (mut menu, mut container, mut player) = trading();
        container[0] = Some(stack("minecraft:emerald", 5));
        menu.set_offers(&mut container, &mut player, menu.merchant_offers().clone());
        assert_eq!(container[2], Some(stack("minecraft:bread", 3)));
        let effects = click(
            &mut menu,
            &mut container,
            &mut player,
            2,
            0,
            ClickInput::Pickup,
        );
        assert_eq!(menu.carried(), &Some(stack("minecraft:bread", 3)));
        assert_eq!(container[0], Some(stack("minecraft:emerald", 3)));
        assert_eq!(menu.trade_offer(0).unwrap().uses, 1);
        assert!(effects.contains(&MenuEffect::Traded { offer: 0 }));
        assert_eq!(container[2], Some(stack("minecraft:bread", 3)));
    }

    #[test]
    fn an_exhausted_offer_gives_no_result() {
        let (mut menu, mut container, mut player) = trading();
        container[0] = Some(stack("minecraft:emerald", 20));
        let mut offers = menu.merchant_offers().clone();
        offers.get_mut(0).unwrap().set_to_out_of_stock();
        menu.set_offers(&mut container, &mut player, offers);
        assert!(container[2].is_none());
    }

    #[test]
    fn the_second_payment_slot_may_hold_the_first_cost() {
        let (mut menu, mut container, mut player) = trading();
        container[0] = Some(stack("minecraft:book", 1));
        container[1] = Some(stack("minecraft:emerald", 5));
        menu.set_offers(&mut container, &mut player, menu.merchant_offers().clone());
        assert_eq!(container[2], Some(stack("minecraft:iron_sword", 1)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            2,
            0,
            ClickInput::Pickup,
        );
        assert!(container[0].is_none());
        assert!(container[1].is_none());
        assert_eq!(menu.trade_offer(1).unwrap().uses, 1);
    }

    #[test]
    fn shift_clicking_the_result_trades_into_the_inventory_from_the_end() {
        let (mut menu, mut container, mut player) = trading();
        container[0] = Some(stack("minecraft:emerald", 2));
        menu.set_offers(&mut container, &mut player, menu.merchant_offers().clone());
        let effects = click(
            &mut menu,
            &mut container,
            &mut player,
            2,
            0,
            ClickInput::QuickMove,
        );
        assert!(effects.contains(&MenuEffect::Traded { offer: 0 }));
        assert_eq!(
            player.get(HOTBAR_START + 8),
            Some(&stack("minecraft:bread", 3))
        );
        assert!(container[0].is_none());
        assert!(container[2].is_none());
    }

    #[test]
    fn shift_clicking_inventory_items_never_fills_the_payment_slots() {
        let (mut menu, mut container, mut player) = trading();
        player.set(MAIN_START, Some(stack("minecraft:emerald", 8)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            3,
            0,
            ClickInput::QuickMove,
        );
        assert!(container[0].is_none());
        assert_eq!(
            player.get(HOTBAR_START),
            Some(&stack("minecraft:emerald", 8))
        );
        click(
            &mut menu,
            &mut container,
            &mut player,
            30,
            0,
            ClickInput::QuickMove,
        );
        assert_eq!(player.get(MAIN_START), Some(&stack("minecraft:emerald", 8)));
    }

    #[test]
    fn selecting_a_trade_moves_the_payments_in_and_gives_the_old_ones_back() {
        let (mut menu, mut container, mut player) = trading();
        player.set(MAIN_START, Some(stack("minecraft:emerald", 30)));
        player.set(MAIN_START + 1, Some(stack("minecraft:book", 4)));
        menu.select_trade(&mut container, &mut player, 1);
        assert_eq!(container[0], Some(stack("minecraft:emerald", 30)));
        assert_eq!(container[1], Some(stack("minecraft:book", 4)));
        assert_eq!(container[2], Some(stack("minecraft:iron_sword", 1)));
        menu.select_trade(&mut container, &mut player, 0);
        assert_eq!(container[0], Some(stack("minecraft:emerald", 30)));
        assert!(container[1].is_none());
        assert_eq!(container[2], Some(stack("minecraft:bread", 3)));
        assert_eq!(
            player.get(HOTBAR_START + 7),
            Some(&stack("minecraft:book", 4))
        );
    }

    #[test]
    fn selecting_a_trade_out_of_range_changes_nothing() {
        let (mut menu, mut container, mut player) = trading();
        player.set(MAIN_START, Some(stack("minecraft:emerald", 30)));
        menu.select_trade(&mut container, &mut player, 9);
        assert!(container[0].is_none());
        menu.select_trade(&mut container, &mut player, -1);
        assert!(container[0].is_none());
    }

    #[test]
    fn the_payment_stack_is_topped_up_to_the_item_maximum_only() {
        let (mut menu, mut container, mut player) = trading();
        player.set(MAIN_START, Some(stack("minecraft:emerald", 64)));
        player.set(MAIN_START + 1, Some(stack("minecraft:emerald", 10)));
        menu.select_trade(&mut container, &mut player, 0);
        assert_eq!(container[0], Some(stack("minecraft:emerald", 64)));
        assert_eq!(
            player.get(MAIN_START + 1),
            Some(&stack("minecraft:emerald", 10))
        );
        assert!(player.get(MAIN_START).is_none());
    }

    #[test]
    fn closing_gives_the_payments_back_and_double_click_gathers_nothing() {
        let (mut menu, mut container, mut player) = trading();
        container[0] = Some(stack("minecraft:emerald", 3));
        let effects = menu.close_transient(&mut container, &mut player);
        assert!(container[0].is_none());
        assert!(
            effects
                .iter()
                .all(|effect| !matches!(effect, MenuEffect::Drop { .. }))
        );
        assert!(player.get(HOTBAR_START).is_some() || player.get(MAIN_START).is_some());
        menu.carried = Some(stack("minecraft:emerald", 1));
        player.set(MAIN_START, Some(stack("minecraft:emerald", 5)));
        click(
            &mut menu,
            &mut container,
            &mut player,
            4,
            0,
            ClickInput::PickupAll,
        );
        assert_eq!(menu.carried(), &Some(stack("minecraft:emerald", 1)));
    }

    #[test]
    fn a_changed_payment_reports_whether_the_trade_is_valid() {
        let (mut menu, mut container, mut player) = trading();
        let effects = menu.set_offers(&mut container, &mut player, menu.merchant_offers().clone());
        assert!(effects.is_empty());
        container[0] = Some(stack("minecraft:emerald", 2));
        let effects = menu.set_offers(&mut container, &mut player, menu.merchant_offers().clone());
        assert_eq!(effects, vec![MenuEffect::TradeUpdated { valid: true }]);
    }
}
