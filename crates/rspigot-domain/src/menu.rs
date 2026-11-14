use rspigot_items::{EquipmentSlot, ItemId};

use crate::click::{self, Classified, ClickView, Drag};
use crate::crafting;
use crate::item::{
    HOTBAR_START, Inventory, ItemStack, OFFHAND_SLOT, PLAYER_INVENTORY_SIZE, Slot, count,
    max_stack, menu_slot, same_item, same_item_same_components, split, stacks_match, with_count,
};

pub const RESULT_SLOT: usize = 0;
pub const OUTSIDE: i16 = -999;
const GRID: std::ops::RangeInclusive<usize> = 1..=4;
const CONTAINER_MAX_STACK: u32 = 99;
const ARMOR_MAX_STACK: u32 = 1;
const OFFHAND_BUTTON: i32 = 40;
const STORAGE: std::ops::Range<usize> = 9..OFFHAND_SLOT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickInput {
    Pickup,
    QuickMove,
    Swap,
    Clone,
    Throw,
    QuickCraft,
    PickupAll,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashedItem {
    pub item: i32,
    pub count: i32,
    pub patched: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HashedSlot(pub Option<HashedItem>);

impl HashedSlot {
    pub const EMPTY: Self = Self(None);

    pub fn of(stack: &Slot) -> Self {
        Self(stack.as_ref().map(|s| HashedItem {
            item: i32::from(s.item.0),
            count: i32::from(s.count.get()),
            patched: false,
        }))
    }

    pub fn matches(&self, stack: &Slot) -> bool {
        match (&self.0, stack) {
            (None, None) => true,
            (Some(hash), Some(stack)) => {
                !hash.patched
                    && hash.count == i32::from(stack.count.get())
                    && hash.item == i32::from(stack.item.0)
            }
            _ => false,
        }
    }
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

#[derive(Debug, Clone, PartialEq)]
pub enum MenuEffect {
    Drop {
        stack: ItemStack,
        thrower: bool,
        predicted: bool,
    },
    ResultUpdated,
    FurnaceOutputTaken {
        item: ItemId,
        amount: u32,
    },
    StonecutterTaken,
    SmithingTaken,
    Traded {
        offer: usize,
    },
    TradeUpdated {
        valid: bool,
    },
    SlotResent(usize),
    FullResync,
    PlacedBack {
        slot: usize,
        stack: ItemStack,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum RemoteUpdate {
    Slot(usize, Slot),
    Carried(Slot),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Clicker {
    pub infinite_materials: bool,
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
pub struct InventoryMenu {
    carried: Slot,
    quick_craft: QuickCraft,
    remote: Vec<RemoteSlot>,
    remote_carried: RemoteSlot,
    suppressed: bool,
    held: Option<Hold>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Hold {
    pub(crate) slot: usize,
    pub(crate) shown: Slot,
    pub(crate) count: u32,
}

impl Hold {
    pub(crate) fn take(held: &mut Option<Self>, slot: usize, shown: Slot) {
        match held {
            Some(hold) if hold.slot == slot => hold.count += 1,
            _ => {
                *held = Some(Self {
                    slot,
                    shown,
                    count: 1,
                });
            }
        }
    }

    pub(crate) fn release(held: &mut Option<Self>) {
        if let Some(hold) = held {
            hold.count = hold.count.saturating_sub(1);
            if hold.count == 0 {
                *held = None;
            }
        }
    }
}

impl Default for InventoryMenu {
    fn default() -> Self {
        Self {
            carried: None,
            quick_craft: QuickCraft::default(),
            remote: vec![RemoteSlot::Known(None); PLAYER_INVENTORY_SIZE],
            remote_carried: RemoteSlot::Known(None),
            suppressed: false,
            held: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotKind {
    Result,
    Grid,
    Armor(EquipmentSlot),
    Storage,
}

fn kind(slot: usize) -> SlotKind {
    match slot {
        RESULT_SLOT => SlotKind::Result,
        1..=4 => SlotKind::Grid,
        5 => SlotKind::Armor(EquipmentSlot::Head),
        6 => SlotKind::Armor(EquipmentSlot::Chest),
        7 => SlotKind::Armor(EquipmentSlot::Legs),
        8 => SlotKind::Armor(EquipmentSlot::Feet),
        _ => SlotKind::Storage,
    }
}

fn equippable(stack: &ItemStack) -> Option<EquipmentSlot> {
    stack.item.kind().and_then(|kind| kind.equippable())
}

fn armor_menu_slot(slot: EquipmentSlot) -> Option<usize> {
    match slot {
        EquipmentSlot::Feet => Some(8),
        EquipmentSlot::Legs => Some(7),
        EquipmentSlot::Chest => Some(6),
        EquipmentSlot::Head => Some(5),
        _ => None,
    }
}

fn index(slot: i32) -> Option<usize> {
    usize::try_from(slot)
        .ok()
        .filter(|slot| *slot < PLAYER_INVENTORY_SIZE)
}

struct Session<'a> {
    inventory: &'a mut Inventory,
    carried: &'a mut Slot,
    infinite: bool,
    effects: Vec<MenuEffect>,
}

impl Session<'_> {
    fn item(&self, slot: usize) -> Slot {
        self.inventory.get(slot).cloned()
    }

    fn put(&mut self, slot: usize, stack: Slot) {
        self.inventory.set(slot, stack);
    }

    fn set(&mut self, slot: usize, stack: Slot) {
        self.put(slot, stack);
        if GRID.contains(&slot) {
            self.refresh_result();
        }
    }

    fn remove(&mut self, slot: usize, amount: u32) -> Slot {
        let removed = self.inventory.remove(slot, amount);
        if removed.is_some() && GRID.contains(&slot) {
            self.refresh_result();
        }
        removed
    }

    fn grid(&self) -> [Slot; 4] {
        std::array::from_fn(|offset| self.item(GRID.start() + offset))
    }

    fn refresh_result(&mut self) {
        let result = crafting::result(2, 2, &self.grid());
        self.put(RESULT_SLOT, result);
        if self.effects.last() != Some(&MenuEffect::ResultUpdated) {
            self.effects.push(MenuEffect::ResultUpdated);
        }
    }

    fn took(&mut self, slot: usize, taken: &Slot) {
        if slot != RESULT_SLOT || taken.is_none() {
            return;
        }
        let mut cells = self.grid();
        let leftovers = crafting::take_result(2, 2, &mut cells);
        for (offset, cell) in cells.into_iter().enumerate() {
            self.put(GRID.start() + offset, cell);
        }
        for leftover in leftovers {
            let mut rest = Some(leftover);
            if !self.inventory.add(&mut rest, self.infinite) {
                self.drop(rest, false, false);
            }
        }
        self.refresh_result();
    }

    fn may_place(&self, slot: usize, stack: &Slot) -> bool {
        match kind(slot) {
            SlotKind::Result => false,
            SlotKind::Armor(piece) => stack.as_ref().and_then(equippable) == Some(piece),
            SlotKind::Grid | SlotKind::Storage => true,
        }
    }

    fn slot_max(&self, slot: usize) -> u32 {
        match kind(slot) {
            SlotKind::Armor(_) => ARMOR_MAX_STACK,
            _ => CONTAINER_MAX_STACK,
        }
    }

    fn slot_max_for(&self, slot: usize, stack: &ItemStack) -> u32 {
        self.slot_max(slot).min(max_stack(stack))
    }

    fn try_remove(&mut self, slot: usize, amount: u32, decrement: u32) -> Slot {
        let current = self.item(slot);
        if !self.may_place(slot, &current) && decrement < count(&current) {
            return None;
        }
        let removed = self.remove(slot, amount.min(decrement))?;
        if self.item(slot).is_none() {
            self.set(slot, None);
        }
        Some(removed)
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
                self.set(slot, part);
            }
            Some(existing) if same_item_same_components(existing, &incoming) => {
                split(&mut remaining, placed);
                self.set(slot, with_count(existing, count(&current) + placed));
            }
            Some(_) => {}
        }
        remaining
    }

    fn drop(&mut self, stack: Slot, thrower: bool, predicted: bool) -> bool {
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
                self.pickup(slot, button == 0, input == ClickInput::QuickMove)
            }
            ClickInput::Swap if (0..9).contains(&button) || button == OFFHAND_BUTTON => {
                self.swap(slot, button)
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
                let Some(target) = index(slot) else {
                    return;
                };
                let wide = craft.kind == 2
                    || u32::from(carried.count.get())
                        > u32::try_from(craft.slots.len()).unwrap_or(u32::MAX);
                if self.can_quick_replace(target, &carried)
                    && self.may_place(target, &self.carried.clone())
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

    fn look(&self) -> Look<'_> {
        Look {
            inventory: self.inventory,
            carried: self.carried,
            instabuild: self.infinite,
        }
    }

    fn finish_quick_craft(&mut self, craft: &QuickCraft, copy: &ItemStack) {
        let drag = click::plan_drag(&self.look(), &craft.slots, craft.kind, copy);
        *self.carried = drag.remaining;
        for (target, stack) in drag.placed {
            self.set(target, Some(stack));
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
                self.drop(thrown, true, true);
            }
            return;
        }
        let Some(target) = index(slot) else {
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
                        self.took(target, &Some(taken));
                    }
                }
                Some(held) if self.may_place(target, &carried) => {
                    if same_item_same_components(existing, held) {
                        let amount = if primary { count(&carried) } else { 1 };
                        *self.carried = self.safe_insert(target, carried, amount);
                    } else if count(&carried) <= self.slot_max_for(target, held) {
                        *self.carried = in_slot.clone();
                        self.set(target, carried);
                    }
                }
                Some(held) if same_item_same_components(existing, held) => {
                    let room = max_stack(held).saturating_sub(count(&carried));
                    if let Some(taken) = self.try_remove(target, count(&in_slot), room) {
                        *self.carried =
                            with_count(held, count(&carried) + count(&Some(taken.clone())));
                        self.took(target, &Some(taken));
                    }
                }
                Some(_) => {}
            },
        }
        if self.slot_max(target) != CONTAINER_MAX_STACK {
            self.effects.push(MenuEffect::SlotResent(target));
        }
    }

    fn equipment_slot_for(stack: &ItemStack) -> EquipmentSlot {
        equippable(stack).unwrap_or(EquipmentSlot::Mainhand)
    }

    fn quick_move_stack(&mut self, slot: usize) -> Slot {
        let copy = self.item(slot)?;
        let mut stack = Some(copy.clone());
        let equipment = Self::equipment_slot_for(&copy);
        let armor_target = armor_menu_slot(equipment).filter(|t| self.item(*t).is_none());
        let moved = match slot {
            RESULT_SLOT => self.move_to(&mut stack, 9, 45, true),
            1..=8 => self.move_to(&mut stack, 9, 45, false),
            _ if armor_target.is_some() => {
                let target = armor_target.unwrap_or(slot);
                self.move_to(&mut stack, target, target + 1, false)
            }
            _ if equipment == EquipmentSlot::Offhand && self.item(OFFHAND_SLOT).is_none() => {
                self.move_to(&mut stack, OFFHAND_SLOT, OFFHAND_SLOT + 1, false)
            }
            9..=35 => self.move_to(&mut stack, HOTBAR_START, 45, false),
            36..=44 => self.move_to(&mut stack, 9, HOTBAR_START, false),
            _ => self.move_to(&mut stack, 9, 45, false),
        };
        if !moved {
            return None;
        }
        if stack.is_none() {
            self.set(slot, None);
        } else {
            self.put(slot, stack.clone());
        }
        if count(&stack) == u32::from(copy.count.get()) {
            return None;
        }
        if slot == RESULT_SLOT {
            self.put(slot, None);
            self.took(slot, &Some(copy.clone()));
            self.drop(stack, false, true);
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
                    self.set(target, part);
                    moved = true;
                    break;
                }
            }
        }
        moved
    }

    fn swap(&mut self, slot: i32, button: i32) {
        let Some(target) = index(slot) else {
            return;
        };
        let Some(hotbar) = usize::try_from(button).ok().and_then(menu_slot) else {
            return;
        };
        let held = self.item(hotbar);
        let in_slot = self.item(target);
        match (&held, &in_slot) {
            (None, None) => {}
            (None, Some(_)) => {
                self.put(hotbar, in_slot.clone());
                self.set(target, None);
                self.took(target, &in_slot);
            }
            (Some(stack), None) => {
                if self.may_place(target, &held) {
                    let limit = self.slot_max_for(target, stack);
                    if count(&held) > limit {
                        let mut rest = held.clone();
                        let part = split(&mut rest, limit);
                        self.put(hotbar, rest);
                        self.set(target, part);
                    } else {
                        self.put(hotbar, None);
                        self.set(target, held.clone());
                    }
                }
            }
            (Some(stack), Some(_)) => {
                if self.may_place(target, &held) {
                    let limit = self.slot_max_for(target, stack);
                    if count(&held) > limit {
                        let mut rest = held.clone();
                        let part = split(&mut rest, limit);
                        self.put(hotbar, rest);
                        self.set(target, part);
                        let mut displaced = in_slot.clone();
                        if !self.inventory.add(&mut displaced, self.infinite) {
                            self.drop(displaced, true, true);
                        }
                    } else {
                        self.put(hotbar, in_slot.clone());
                        self.set(target, held.clone());
                    }
                }
            }
        }
    }

    fn clone_slot(&mut self, slot: i32) {
        if !self.infinite || self.carried.is_some() {
            return;
        }
        let Some(target) = index(slot) else {
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
        let Some(target) = index(slot) else {
            return;
        };
        let amount = if button == 0 {
            1
        } else {
            count(&self.item(target))
        };
        let mut dropped = self.try_remove(target, amount, u32::MAX);
        self.took(target, &dropped);
        self.drop(dropped.clone(), true, true);
        if button == 1 {
            while dropped.is_some() && same_item(&self.item(target), &dropped) {
                dropped = self.try_remove(target, amount, u32::MAX);
                self.took(target, &dropped);
                if !self.drop(dropped.clone(), true, true) {
                    break;
                }
            }
        }
    }

    fn pickup_all(&mut self, slot: i32, button: i32) {
        let Some(target) = index(slot) else {
            return;
        };
        if self.carried.is_none() || self.item(target).is_some() {
            return;
        }
        let order: Vec<usize> = if button == 0 {
            (0..PLAYER_INVENTORY_SIZE).collect()
        } else {
            (0..PLAYER_INVENTORY_SIZE).rev().collect()
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
                if source == RESULT_SLOT || !self.can_quick_replace(source, &carried) {
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

struct Look<'a> {
    inventory: &'a Inventory,
    carried: &'a Slot,
    instabuild: bool,
}

impl ClickView for Look<'_> {
    fn slot(&self, raw: i32) -> Option<usize> {
        index(raw)
    }

    fn item(&self, slot: usize) -> Slot {
        self.inventory.get(slot).cloned()
    }

    fn carried(&self) -> &Slot {
        self.carried
    }

    fn may_pickup(&self, _: usize) -> bool {
        true
    }

    fn may_place(&self, slot: usize, stack: &ItemStack) -> bool {
        match kind(slot) {
            SlotKind::Result => false,
            SlotKind::Armor(piece) => equippable(stack) == Some(piece),
            SlotKind::Grid | SlotKind::Storage => true,
        }
    }

    fn slot_limit(&self, slot: usize) -> u32 {
        match kind(slot) {
            SlotKind::Armor(_) => ARMOR_MAX_STACK,
            _ => CONTAINER_MAX_STACK,
        }
    }

    fn stack_limit(&self, slot: usize, stack: &ItemStack) -> u32 {
        self.slot_limit(slot).min(max_stack(stack))
    }

    fn container_limit(&self, _: usize) -> u32 {
        CONTAINER_MAX_STACK
    }

    fn hotbar(&self, button: i32) -> Slot {
        usize::try_from(button)
            .ok()
            .and_then(menu_slot)
            .and_then(|slot| self.inventory.get(slot))
            .cloned()
    }

    fn in_player(&self, slot: usize) -> bool {
        slot > *GRID.end()
    }

    fn holds(&self, item: ItemId) -> bool {
        (RESULT_SLOT..=*GRID.end()).chain(STORAGE).any(|slot| {
            self.inventory
                .get(slot)
                .is_some_and(|stack| stack.item == item)
        })
    }

    fn instabuild(&self) -> bool {
        self.instabuild
    }
}

impl InventoryMenu {
    pub fn carried(&self) -> &Slot {
        &self.carried
    }

    pub fn quick_crafting(&self) -> bool {
        self.quick_craft.status != 0
    }

    pub fn click(
        &mut self,
        inventory: &mut Inventory,
        slot: i16,
        button: i8,
        input: ClickInput,
        clicker: Clicker,
    ) -> Vec<MenuEffect> {
        let mut session = Session {
            inventory,
            carried: &mut self.carried,
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
        inventory: &Inventory,
        slot: i16,
        button: i8,
        input: ClickInput,
        clicker: Clicker,
    ) -> Classified {
        let look = Look {
            inventory,
            carried: &self.carried,
            instabuild: clicker.infinite_materials,
        };
        click::classify(&look, slot, button, input)
    }

    pub fn pending_drag(&self, inventory: &Inventory, button: i8) -> Option<Drag> {
        let ends = self.quick_craft.status == 1 && i32::from(button) & 3 == 2;
        let carried = self.carried.as_ref()?;
        if !ends || self.quick_craft.slots.is_empty() {
            return None;
        }
        let look = Look {
            inventory,
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

    pub fn drag_holds(&self, inventory: &Inventory, drag: &Drag) -> bool {
        self.quick_craft.status == 1
            && self.carried.as_ref() == Some(&drag.cursor)
            && drag
                .before
                .iter()
                .all(|(slot, stack)| stacks_match(&inventory.get(*slot).cloned(), stack))
    }

    pub fn finish_drag(
        &mut self,
        inventory: &mut Inventory,
        drag: &Drag,
        cursor: Option<Slot>,
    ) -> Vec<MenuEffect> {
        self.quick_craft.reset();
        let mut session = Session {
            inventory,
            carried: &mut self.carried,
            infinite: false,
            effects: Vec::new(),
        };
        if let Some(cursor) = cursor {
            *session.carried = cursor;
            for (target, stack) in &drag.placed {
                session.set(*target, Some(stack.clone()));
            }
        }
        session.effects.push(MenuEffect::FullResync);
        session.effects
    }

    pub fn quick_move_once(
        &mut self,
        inventory: &mut Inventory,
        slot: usize,
        clicker: Clicker,
    ) -> (Slot, Vec<MenuEffect>) {
        let mut session = Session {
            inventory,
            carried: &mut self.carried,
            infinite: clicker.infinite_materials,
            effects: Vec::new(),
        };
        let moved = session.quick_move_stack(slot);
        (moved, session.effects)
    }

    pub fn set_remote_result(&mut self, inventory: &Inventory) {
        self.set_remote(RESULT_SLOT, inventory.get(RESULT_SLOT).cloned());
    }

    pub fn set_carried(&mut self, stack: Slot) {
        self.carried = stack.clone();
        self.remote_carried = RemoteSlot::Known(stack);
    }

    pub fn hold_result(&mut self, inventory: &Inventory) {
        let shown = match self.remote.get(RESULT_SLOT) {
            Some(RemoteSlot::Known(shown)) => shown.clone(),
            _ => inventory.get(RESULT_SLOT).cloned(),
        };
        Hold::take(&mut self.held, RESULT_SLOT, shown);
    }

    pub fn release_result(&mut self) {
        Hold::release(&mut self.held);
    }

    pub fn result_held(&self) -> bool {
        self.held.is_some()
    }

    pub fn crafting_grid(inventory: &Inventory) -> [Slot; 4] {
        std::array::from_fn(|offset| inventory.get(GRID.start() + offset).cloned())
    }

    pub fn set_by_player(
        &mut self,
        inventory: &mut Inventory,
        slot: usize,
        stack: Slot,
    ) -> Vec<MenuEffect> {
        let mut session = Session {
            inventory,
            carried: &mut self.carried,
            infinite: true,
            effects: Vec::new(),
        };
        session.set(slot, stack);
        session.effects
    }

    pub fn close(&mut self, inventory: &mut Inventory) -> Vec<MenuEffect> {
        let mut effects = Vec::new();
        if let Some(carried) = self.carried.take() {
            place_back_or_drop(inventory, carried, &mut effects);
        }
        inventory.set(RESULT_SLOT, None);
        for slot in GRID {
            if let Some(stack) = inventory.get(slot).cloned() {
                inventory.set(slot, None);
                place_back_or_drop(inventory, stack, &mut effects);
            }
        }
        effects
    }

    pub fn suppress_remote_updates(&mut self) {
        self.suppressed = true;
    }

    pub fn resume_remote_updates(&mut self) {
        self.suppressed = false;
    }

    pub fn set_remote(&mut self, slot: usize, stack: Slot) {
        if let Some(remote) = self.remote.get_mut(slot) {
            *remote = RemoteSlot::Known(stack);
        }
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

    pub fn changes(&mut self, inventory: &Inventory) -> Vec<RemoteUpdate> {
        if self.suppressed {
            return Vec::new();
        }
        let mut updates = Vec::new();
        let held = self.held.as_ref().map(|hold| hold.slot);
        for (slot, remote) in self.remote.iter_mut().enumerate() {
            if held == Some(slot) {
                continue;
            }
            let stack = inventory.get(slot).cloned();
            if !remote.matches(&stack) {
                *remote = RemoteSlot::Known(stack.clone());
                updates.push(RemoteUpdate::Slot(slot, stack));
            }
        }
        if !self.remote_carried.matches(&self.carried) {
            self.remote_carried = RemoteSlot::Known(self.carried.clone());
            updates.push(RemoteUpdate::Carried(self.carried.clone()));
        }
        updates
    }

    pub fn sync_all(&mut self, inventory: &Inventory) -> (Vec<Slot>, Slot) {
        let mut slots = inventory.slots().to_vec();
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

    pub fn synced(inventory: &Inventory) -> Self {
        let mut menu = Self::default();
        menu.sync_all(inventory);
        menu
    }
}

pub(crate) fn place_back_or_drop(
    inventory: &mut Inventory,
    stack: ItemStack,
    effects: &mut Vec<MenuEffect>,
) {
    let outcome = inventory.place_back(stack);
    for (slot, stack) in outcome.placed {
        effects.push(MenuEffect::PlacedBack { slot, stack });
    }
    if let Some(stack) = outcome.leftover {
        effects.push(MenuEffect::Drop {
            stack,
            thrower: false,
            predicted: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU8;

    use rspigot_items::ItemKind;

    use super::*;

    fn stack(kind: ItemKind, count: u8) -> ItemStack {
        ItemStack::new(kind.id(), NonZeroU8::new(count).unwrap())
    }

    fn inventory(slots: &[(usize, ItemStack)]) -> Inventory {
        let mut inventory = Inventory::default();
        for (slot, stack) in slots {
            inventory.set(*slot, Some(stack.clone()));
        }
        inventory
    }

    const SURVIVAL: Clicker = Clicker {
        infinite_materials: false,
    };
    const CREATIVE: Clicker = Clicker {
        infinite_materials: true,
    };

    fn click(
        menu: &mut InventoryMenu,
        inventory: &mut Inventory,
        slot: i16,
        button: i8,
        input: ClickInput,
    ) -> Vec<MenuEffect> {
        menu.click(inventory, slot, button, input, SURVIVAL)
    }

    fn carrying(stack: ItemStack) -> InventoryMenu {
        InventoryMenu {
            carried: Some(stack),
            ..InventoryMenu::default()
        }
    }

    fn thrown(stack: ItemStack) -> MenuEffect {
        MenuEffect::Drop {
            stack,
            thrower: true,
            predicted: true,
        }
    }

    #[test]
    fn left_click_takes_the_stack_and_right_click_takes_half_rounded_up() {
        let mut inv = inventory(&[
            (36, stack(ItemKind::Stone, 5)),
            (37, stack(ItemKind::Dirt, 7)),
        ]);
        let mut menu = InventoryMenu::default();
        assert!(click(&mut menu, &mut inv, 36, 0, ClickInput::Pickup).is_empty());
        assert_eq!(menu.carried(), &Some(stack(ItemKind::Stone, 5)));
        assert_eq!(inv.get(36), None);
        let mut menu = InventoryMenu::default();
        click(&mut menu, &mut inv, 37, 1, ClickInput::Pickup);
        assert_eq!(menu.carried(), &Some(stack(ItemKind::Dirt, 4)));
        assert_eq!(inv.get(37), Some(&stack(ItemKind::Dirt, 3)));
    }

    #[test]
    fn placing_merges_places_one_and_swaps_different_items() {
        let mut inv = inventory(&[
            (36, stack(ItemKind::Stone, 60)),
            (37, stack(ItemKind::Dirt, 2)),
        ]);
        let mut menu = carrying(stack(ItemKind::Stone, 10));
        click(&mut menu, &mut inv, 36, 0, ClickInput::Pickup);
        assert_eq!(inv.get(36), Some(&stack(ItemKind::Stone, 64)));
        assert_eq!(menu.carried(), &Some(stack(ItemKind::Stone, 6)));
        click(&mut menu, &mut inv, 38, 1, ClickInput::Pickup);
        assert_eq!(inv.get(38), Some(&stack(ItemKind::Stone, 1)));
        assert_eq!(menu.carried(), &Some(stack(ItemKind::Stone, 5)));
        click(&mut menu, &mut inv, 37, 0, ClickInput::Pickup);
        assert_eq!(inv.get(37), Some(&stack(ItemKind::Stone, 5)));
        assert_eq!(menu.carried(), &Some(stack(ItemKind::Dirt, 2)));
    }

    #[test]
    fn clicking_outside_throws_the_whole_carried_stack_or_one() {
        let mut inv = Inventory::default();
        let mut menu = carrying(stack(ItemKind::Stone, 5));
        assert_eq!(
            click(&mut menu, &mut inv, OUTSIDE, 1, ClickInput::Pickup),
            vec![thrown(stack(ItemKind::Stone, 1))]
        );
        assert_eq!(
            click(&mut menu, &mut inv, OUTSIDE, 0, ClickInput::Pickup),
            vec![thrown(stack(ItemKind::Stone, 4))]
        );
        assert_eq!(menu.carried(), &None);
        assert!(click(&mut menu, &mut inv, OUTSIDE, 0, ClickInput::Pickup).is_empty());
    }

    #[test]
    fn shift_click_moves_between_hotbar_and_main_inventory() {
        let mut inv = inventory(&[
            (36, stack(ItemKind::Stone, 40)),
            (9, stack(ItemKind::Stone, 60)),
            (20, stack(ItemKind::Dirt, 3)),
        ]);
        let mut menu = InventoryMenu::default();
        assert!(click(&mut menu, &mut inv, 36, 0, ClickInput::QuickMove).is_empty());
        assert_eq!(inv.get(9), Some(&stack(ItemKind::Stone, 64)));
        assert_eq!(inv.get(10), Some(&stack(ItemKind::Stone, 36)));
        assert_eq!(inv.get(36), None);
        click(&mut menu, &mut inv, 20, 0, ClickInput::QuickMove);
        assert_eq!(inv.get(36), Some(&stack(ItemKind::Dirt, 3)));
        assert_eq!(inv.get(20), None);
    }

    #[test]
    fn shift_click_equips_armor_and_shields_and_unequips_back() {
        let mut inv = inventory(&[
            (12, stack(ItemKind::IronHelmet, 1)),
            (13, stack(ItemKind::Shield, 1)),
            (14, stack(ItemKind::DiamondBoots, 1)),
        ]);
        let mut menu = InventoryMenu::default();
        click(&mut menu, &mut inv, 12, 0, ClickInput::QuickMove);
        click(&mut menu, &mut inv, 13, 0, ClickInput::QuickMove);
        click(&mut menu, &mut inv, 14, 0, ClickInput::QuickMove);
        assert_eq!(inv.get(5), Some(&stack(ItemKind::IronHelmet, 1)));
        assert_eq!(inv.get(OFFHAND_SLOT), Some(&stack(ItemKind::Shield, 1)));
        assert_eq!(inv.get(8), Some(&stack(ItemKind::DiamondBoots, 1)));
        click(&mut menu, &mut inv, 5, 0, ClickInput::QuickMove);
        assert_eq!(inv.get(5), None);
        assert_eq!(inv.get(9), Some(&stack(ItemKind::IronHelmet, 1)));
    }

    #[test]
    fn armor_slots_refuse_other_items_and_spigot_resends_them() {
        let mut inv = Inventory::default();
        let mut menu = carrying(stack(ItemKind::Stone, 1));
        assert_eq!(
            click(&mut menu, &mut inv, 5, 0, ClickInput::Pickup),
            vec![MenuEffect::SlotResent(5)]
        );
        assert_eq!(inv.get(5), None);
        menu.carried = Some(stack(ItemKind::IronChestplate, 1));
        click(&mut menu, &mut inv, 5, 0, ClickInput::Pickup);
        assert_eq!(inv.get(5), None);
        click(&mut menu, &mut inv, 6, 0, ClickInput::Pickup);
        assert_eq!(inv.get(6), Some(&stack(ItemKind::IronChestplate, 1)));
        assert_eq!(menu.carried(), &None);
    }

    #[test]
    fn crafting_grid_is_storage_that_updates_the_result_on_every_change() {
        let mut inv = inventory(&[(36, stack(ItemKind::Dirt, 3))]);
        let mut menu = InventoryMenu::default();
        click(&mut menu, &mut inv, 36, 0, ClickInput::Pickup);
        assert_eq!(
            click(&mut menu, &mut inv, 2, 1, ClickInput::Pickup),
            vec![MenuEffect::ResultUpdated]
        );
        assert_eq!(inv.get(2), Some(&stack(ItemKind::Dirt, 1)));
        assert!(click(&mut menu, &mut inv, 0, 0, ClickInput::Pickup).is_empty());
        assert_eq!(inv.get(RESULT_SLOT), None);
        menu.carried = None;
        assert_eq!(
            click(&mut menu, &mut inv, 2, 0, ClickInput::Pickup),
            vec![MenuEffect::ResultUpdated]
        );
        assert_eq!(menu.carried(), &Some(stack(ItemKind::Dirt, 1)));
        click(&mut menu, &mut inv, 3, 0, ClickInput::Pickup);
        assert_eq!(
            click(&mut menu, &mut inv, 3, 0, ClickInput::QuickMove),
            vec![MenuEffect::ResultUpdated]
        );
        assert_eq!(inv.get(9), Some(&stack(ItemKind::Dirt, 1)));
    }

    fn crafting(slots: &[(usize, ItemStack)]) -> (InventoryMenu, Inventory) {
        let mut inv = inventory(slots);
        let mut menu = InventoryMenu::default();
        for (slot, stack) in slots {
            if GRID.contains(slot) {
                menu.set_by_player(&mut inv, *slot, Some(stack.clone()));
            }
        }
        menu.sync_all(&inv);
        (menu, inv)
    }

    #[test]
    fn a_recipe_in_the_grid_shows_its_result() {
        let (_, inv) = crafting(&[(1, stack(ItemKind::OakLog, 2))]);
        assert_eq!(inv.get(RESULT_SLOT), Some(&stack(ItemKind::OakPlanks, 4)));
        let (_, inv) = crafting(&[(1, stack(ItemKind::Dirt, 2))]);
        assert_eq!(inv.get(RESULT_SLOT), None);
    }

    #[test]
    fn placing_an_ingredient_by_hand_shows_the_result() {
        let mut inv = inventory(&[(36, stack(ItemKind::OakLog, 3))]);
        let mut menu = InventoryMenu::default();
        click(&mut menu, &mut inv, 36, 0, ClickInput::Pickup);
        let effects = click(&mut menu, &mut inv, 4, 0, ClickInput::Pickup);
        assert_eq!(effects, vec![MenuEffect::ResultUpdated]);
        assert_eq!(inv.get(RESULT_SLOT), Some(&stack(ItemKind::OakPlanks, 4)));
        click(&mut menu, &mut inv, 4, 0, ClickInput::Pickup);
        assert_eq!(inv.get(RESULT_SLOT), None);
    }

    #[test]
    fn taking_the_result_by_hand_uses_up_one_craft_and_shows_the_next() {
        let (mut menu, mut inv) = crafting(&[(1, stack(ItemKind::OakLog, 2))]);
        click(&mut menu, &mut inv, 0, 0, ClickInput::Pickup);
        assert_eq!(menu.carried(), &Some(stack(ItemKind::OakPlanks, 4)));
        assert_eq!(inv.get(1), Some(&stack(ItemKind::OakLog, 1)));
        assert_eq!(inv.get(RESULT_SLOT), Some(&stack(ItemKind::OakPlanks, 4)));
        click(&mut menu, &mut inv, 0, 0, ClickInput::Pickup);
        assert_eq!(menu.carried(), &Some(stack(ItemKind::OakPlanks, 8)));
        assert_eq!(inv.get(1), None);
        assert_eq!(inv.get(RESULT_SLOT), None);
    }

    #[test]
    fn the_result_only_stacks_onto_a_matching_hand_and_never_takes_part_of_it() {
        let (mut menu, mut inv) = crafting(&[(1, stack(ItemKind::OakLog, 2))]);
        menu.carried = Some(stack(ItemKind::OakPlanks, 62));
        click(&mut menu, &mut inv, 0, 0, ClickInput::Pickup);
        assert_eq!(menu.carried(), &Some(stack(ItemKind::OakPlanks, 62)));
        assert_eq!(inv.get(1), Some(&stack(ItemKind::OakLog, 2)));
        menu.carried = Some(stack(ItemKind::OakPlanks, 60));
        click(&mut menu, &mut inv, 0, 0, ClickInput::Pickup);
        assert_eq!(menu.carried(), &Some(stack(ItemKind::OakPlanks, 64)));
        assert_eq!(inv.get(1), Some(&stack(ItemKind::OakLog, 1)));
        menu.carried = Some(stack(ItemKind::Dirt, 1));
        click(&mut menu, &mut inv, 0, 0, ClickInput::Pickup);
        assert_eq!(menu.carried(), &Some(stack(ItemKind::Dirt, 1)));
        assert_eq!(inv.get(1), Some(&stack(ItemKind::OakLog, 1)));
    }

    #[test]
    fn a_right_click_takes_half_the_result_but_still_uses_a_whole_craft() {
        let (mut menu, mut inv) = crafting(&[(1, stack(ItemKind::OakLog, 2))]);
        click(&mut menu, &mut inv, 0, 1, ClickInput::Pickup);
        assert_eq!(menu.carried(), &Some(stack(ItemKind::OakPlanks, 2)));
        assert_eq!(inv.get(1), Some(&stack(ItemKind::OakLog, 1)));
        assert_eq!(inv.get(RESULT_SLOT), Some(&stack(ItemKind::OakPlanks, 4)));
    }

    #[test]
    fn shift_click_on_the_result_crafts_until_the_grid_runs_out() {
        let (mut menu, mut inv) = crafting(&[(1, stack(ItemKind::OakLog, 3))]);
        click(&mut menu, &mut inv, 0, 0, ClickInput::QuickMove);
        assert_eq!(inv.get(1), None);
        assert_eq!(inv.get(RESULT_SLOT), None);
        let planks: u32 = (9..45).map(|slot| count(&inv.get(slot).cloned())).sum();
        assert_eq!(planks, 12);
    }

    #[test]
    fn shift_click_on_the_result_stops_when_the_inventory_is_full() {
        let mut filled: Vec<(usize, ItemStack)> = (9..45)
            .map(|slot| (slot, stack(ItemKind::Dirt, 64)))
            .collect();
        filled.push((1, stack(ItemKind::OakLog, 5)));
        let (mut menu, mut inv) = crafting(&filled);
        let effects = click(&mut menu, &mut inv, 0, 0, ClickInput::QuickMove);
        assert!(effects.iter().all(|e| *e == MenuEffect::ResultUpdated) || effects.is_empty());
        assert_eq!(inv.get(1), Some(&stack(ItemKind::OakLog, 5)));
        assert_eq!(inv.get(RESULT_SLOT), Some(&stack(ItemKind::OakPlanks, 4)));
    }

    #[test]
    fn a_number_key_moves_the_result_to_the_hotbar_only_when_the_slot_is_empty() {
        let (mut menu, mut inv) = crafting(&[(1, stack(ItemKind::OakLog, 2))]);
        click(&mut menu, &mut inv, 0, 2, ClickInput::Swap);
        assert_eq!(inv.get(38), Some(&stack(ItemKind::OakPlanks, 4)));
        assert_eq!(inv.get(1), Some(&stack(ItemKind::OakLog, 1)));
        assert_eq!(inv.get(RESULT_SLOT), Some(&stack(ItemKind::OakPlanks, 4)));
        click(&mut menu, &mut inv, 0, 2, ClickInput::Swap);
        assert_eq!(inv.get(38), Some(&stack(ItemKind::OakPlanks, 4)));
        assert_eq!(inv.get(1), Some(&stack(ItemKind::OakLog, 1)));
    }

    #[test]
    fn dropping_the_result_throws_what_was_taken_and_uses_a_craft() {
        let (mut menu, mut inv) = crafting(&[(1, stack(ItemKind::OakLog, 3))]);
        let effects = click(&mut menu, &mut inv, 0, 0, ClickInput::Throw);
        assert!(effects.contains(&thrown(stack(ItemKind::OakPlanks, 1))));
        assert_eq!(inv.get(1), Some(&stack(ItemKind::OakLog, 2)));
        assert_eq!(inv.get(RESULT_SLOT), Some(&stack(ItemKind::OakPlanks, 4)));
        let effects = click(&mut menu, &mut inv, 0, 1, ClickInput::Throw);
        let thrown_planks: u32 = effects
            .iter()
            .filter_map(|effect| match effect {
                MenuEffect::Drop { stack, .. } => Some(u32::from(stack.count.get())),
                _ => None,
            })
            .sum();
        assert_eq!(thrown_planks, 8);
        assert_eq!(inv.get(1), None);
        assert_eq!(inv.get(RESULT_SLOT), None);
    }

    #[test]
    fn a_crafting_remainder_goes_back_to_the_inventory() {
        let (mut menu, mut inv) = crafting(&[(1, stack(ItemKind::HoneyBottle, 2))]);
        assert_eq!(inv.get(RESULT_SLOT), Some(&stack(ItemKind::Sugar, 3)));
        click(&mut menu, &mut inv, 0, 0, ClickInput::Pickup);
        assert_eq!(inv.get(1), Some(&stack(ItemKind::HoneyBottle, 1)));
        assert_eq!(inv.get(36), Some(&stack(ItemKind::GlassBottle, 1)));
        let (mut menu, mut inv) = crafting(&[(1, stack(ItemKind::HoneyBottle, 1))]);
        click(&mut menu, &mut inv, 0, 0, ClickInput::Pickup);
        assert_eq!(inv.get(1), Some(&stack(ItemKind::GlassBottle, 1)));
    }

    #[test]
    fn a_remainder_with_no_room_is_dropped() {
        let mut filled: Vec<(usize, ItemStack)> = (9..45)
            .map(|slot| (slot, stack(ItemKind::Dirt, 64)))
            .collect();
        filled.push((1, stack(ItemKind::HoneyBottle, 2)));
        let (mut menu, mut inv) = crafting(&filled);
        let effects = click(&mut menu, &mut inv, 0, 0, ClickInput::Pickup);
        assert!(effects.contains(&MenuEffect::Drop {
            stack: stack(ItemKind::GlassBottle, 1),
            thrower: false,
            predicted: false,
        }));
    }

    #[test]
    fn a_crafting_grid_change_in_creative_also_updates_the_result() {
        let mut inv = Inventory::default();
        let mut menu = InventoryMenu::default();
        let effects = menu.set_by_player(&mut inv, 2, Some(stack(ItemKind::OakLog, 1)));
        assert!(effects.contains(&MenuEffect::ResultUpdated));
        assert_eq!(inv.get(RESULT_SLOT), Some(&stack(ItemKind::OakPlanks, 4)));
    }

    #[test]
    fn number_keys_swap_with_the_hotbar_and_f_with_the_offhand() {
        let mut inv = inventory(&[
            (9, stack(ItemKind::Stone, 5)),
            (38, stack(ItemKind::Dirt, 2)),
        ]);
        let mut menu = InventoryMenu::default();
        click(&mut menu, &mut inv, 9, 2, ClickInput::Swap);
        assert_eq!(inv.get(9), Some(&stack(ItemKind::Dirt, 2)));
        assert_eq!(inv.get(38), Some(&stack(ItemKind::Stone, 5)));
        click(&mut menu, &mut inv, 9, 40, ClickInput::Swap);
        assert_eq!(inv.get(OFFHAND_SLOT), Some(&stack(ItemKind::Dirt, 2)));
        assert_eq!(inv.get(9), None);
        assert!(click(&mut menu, &mut inv, 9, 9, ClickInput::Swap).is_empty());
        assert_eq!(inv.get(OFFHAND_SLOT), Some(&stack(ItemKind::Dirt, 2)));
    }

    #[test]
    fn swapping_a_stack_into_an_armor_slot_keeps_the_rest_and_displaces_the_old_piece() {
        let mut inv = inventory(&[
            (36, stack(ItemKind::CarvedPumpkin, 3)),
            (5, stack(ItemKind::IronHelmet, 1)),
        ]);
        let mut menu = InventoryMenu::default();
        click(&mut menu, &mut inv, 5, 0, ClickInput::Swap);
        assert_eq!(inv.get(5), Some(&stack(ItemKind::CarvedPumpkin, 1)));
        assert_eq!(inv.get(36), Some(&stack(ItemKind::CarvedPumpkin, 2)));
        assert_eq!(inv.get(37), Some(&stack(ItemKind::IronHelmet, 1)));
    }

    #[test]
    fn middle_click_clones_a_full_stack_only_with_infinite_materials() {
        let mut inv = inventory(&[(36, stack(ItemKind::Stone, 2))]);
        let mut menu = InventoryMenu::default();
        click(&mut menu, &mut inv, 36, 2, ClickInput::Clone);
        assert_eq!(menu.carried(), &None);
        menu.click(&mut inv, 36, 2, ClickInput::Clone, CREATIVE);
        assert_eq!(menu.carried(), &Some(stack(ItemKind::Stone, 64)));
        assert_eq!(inv.get(36), Some(&stack(ItemKind::Stone, 2)));
    }

    #[test]
    fn throw_drops_one_or_the_whole_slot_and_only_with_an_empty_cursor() {
        let mut inv = inventory(&[(36, stack(ItemKind::Stone, 5))]);
        let mut menu = InventoryMenu::default();
        assert_eq!(
            click(&mut menu, &mut inv, 36, 0, ClickInput::Throw),
            vec![thrown(stack(ItemKind::Stone, 1))]
        );
        assert_eq!(
            click(&mut menu, &mut inv, 36, 1, ClickInput::Throw),
            vec![thrown(stack(ItemKind::Stone, 4))]
        );
        assert!(click(&mut menu, &mut inv, 36, 1, ClickInput::Throw).is_empty());
        let mut inv = inventory(&[(36, stack(ItemKind::Stone, 5))]);
        menu.carried = Some(stack(ItemKind::Dirt, 1));
        assert!(click(&mut menu, &mut inv, 36, 0, ClickInput::Throw).is_empty());
        assert!(click(&mut menu, &mut inv, OUTSIDE, 0, ClickInput::Throw).is_empty());
    }

    fn drag(
        menu: &mut InventoryMenu,
        inv: &mut Inventory,
        kind: i8,
        slots: &[i16],
        clicker: Clicker,
    ) -> Vec<MenuEffect> {
        let mut effects = menu.click(inv, OUTSIDE, kind << 2, ClickInput::QuickCraft, clicker);
        for slot in slots {
            effects.extend(menu.click(
                inv,
                *slot,
                (kind << 2) | 1,
                ClickInput::QuickCraft,
                clicker,
            ));
        }
        effects.extend(menu.click(
            inv,
            OUTSIDE,
            (kind << 2) | 2,
            ClickInput::QuickCraft,
            clicker,
        ));
        effects
    }

    #[test]
    fn left_drag_splits_evenly_and_spigot_resyncs_everything() {
        let mut inv = inventory(&[(37, stack(ItemKind::Stone, 2))]);
        let mut menu = carrying(stack(ItemKind::Stone, 10));
        let effects = drag(&mut menu, &mut inv, 0, &[36, 37, 38, 37], SURVIVAL);
        assert_eq!(effects, vec![MenuEffect::FullResync]);
        assert_eq!(inv.get(36), Some(&stack(ItemKind::Stone, 3)));
        assert_eq!(inv.get(37), Some(&stack(ItemKind::Stone, 5)));
        assert_eq!(inv.get(38), Some(&stack(ItemKind::Stone, 3)));
        assert_eq!(menu.carried(), &Some(stack(ItemKind::Stone, 1)));
        assert!(!menu.quick_crafting());
    }

    #[test]
    fn right_drag_places_one_per_slot_and_middle_drag_needs_creative() {
        let mut inv = Inventory::default();
        let mut menu = carrying(stack(ItemKind::Stone, 3));
        let effects = drag(&mut menu, &mut inv, 1, &[1, 9], SURVIVAL);
        assert_eq!(
            effects,
            vec![MenuEffect::ResultUpdated, MenuEffect::FullResync]
        );
        assert_eq!(inv.get(1), Some(&stack(ItemKind::Stone, 1)));
        assert_eq!(inv.get(9), Some(&stack(ItemKind::Stone, 1)));
        assert_eq!(menu.carried(), &Some(stack(ItemKind::Stone, 1)));
        assert!(drag(&mut menu, &mut inv, 2, &[10], SURVIVAL).is_empty());
        assert_eq!(inv.get(10), None);
        drag(&mut menu, &mut inv, 2, &[10, 11], CREATIVE);
        assert_eq!(inv.get(10), Some(&stack(ItemKind::Stone, 64)));
        assert_eq!(inv.get(11), Some(&stack(ItemKind::Stone, 64)));
    }

    #[test]
    fn a_click_in_the_middle_of_a_drag_cancels_it() {
        let mut inv = Inventory::default();
        let mut menu = carrying(stack(ItemKind::Stone, 4));
        menu.click(&mut inv, OUTSIDE, 0, ClickInput::QuickCraft, SURVIVAL);
        menu.click(&mut inv, 9, 1, ClickInput::QuickCraft, SURVIVAL);
        assert!(menu.quick_crafting());
        assert!(click(&mut menu, &mut inv, 10, 0, ClickInput::Pickup).is_empty());
        assert!(!menu.quick_crafting());
        assert_eq!(inv.get(10), None);
        assert_eq!(menu.carried(), &Some(stack(ItemKind::Stone, 4)));
    }

    #[test]
    fn double_click_gathers_partial_stacks_before_full_ones() {
        let mut inv = inventory(&[
            (9, stack(ItemKind::Stone, 64)),
            (20, stack(ItemKind::Stone, 30)),
            (40, stack(ItemKind::Stone, 20)),
            (RESULT_SLOT, stack(ItemKind::Stone, 5)),
        ]);
        let mut menu = carrying(stack(ItemKind::Stone, 10));
        click(&mut menu, &mut inv, 36, 0, ClickInput::PickupAll);
        assert_eq!(menu.carried(), &Some(stack(ItemKind::Stone, 64)));
        assert_eq!(inv.get(20), None);
        assert_eq!(inv.get(40), None);
        assert_eq!(inv.get(9), Some(&stack(ItemKind::Stone, 60)));
        assert_eq!(inv.get(RESULT_SLOT), Some(&stack(ItemKind::Stone, 5)));
    }

    #[test]
    fn closing_puts_carried_and_grid_back_and_drops_what_does_not_fit() {
        let mut inv = inventory(&[
            (37, stack(ItemKind::Stone, 60)),
            (1, stack(ItemKind::Dirt, 2)),
        ]);
        inv.selected_hotbar = 1;
        let mut menu = carrying(stack(ItemKind::Stone, 10));
        let effects = menu.close(&mut inv);
        assert_eq!(
            effects,
            vec![
                MenuEffect::PlacedBack {
                    slot: 1,
                    stack: stack(ItemKind::Stone, 64)
                },
                MenuEffect::PlacedBack {
                    slot: 0,
                    stack: stack(ItemKind::Stone, 6)
                },
                MenuEffect::PlacedBack {
                    slot: 2,
                    stack: stack(ItemKind::Dirt, 2)
                },
            ]
        );
        assert_eq!(menu.carried(), &None);
        assert_eq!(inv.get(1), None);
        let mut full = Inventory::default();
        for slot in 9..45 {
            full.set(slot, Some(stack(ItemKind::Dirt, 64)));
        }
        menu.carried = Some(stack(ItemKind::Stone, 3));
        assert_eq!(
            menu.close(&mut full),
            vec![MenuEffect::Drop {
                stack: stack(ItemKind::Stone, 3),
                thrower: false,
                predicted: false,
            }]
        );
    }

    #[test]
    fn remote_slots_only_report_what_the_client_does_not_already_know() {
        let mut inv = inventory(&[(36, stack(ItemKind::Stone, 5))]);
        let mut menu = InventoryMenu::default();
        assert_eq!(
            menu.changes(&inv),
            vec![RemoteUpdate::Slot(36, Some(stack(ItemKind::Stone, 5)))]
        );
        assert!(menu.changes(&inv).is_empty());
        click(&mut menu, &mut inv, 36, 0, ClickInput::Pickup);
        menu.suppress_remote_updates();
        assert!(menu.changes(&inv).is_empty());
        assert!(menu.receive_remote(36, HashedSlot::EMPTY));
        assert!(!menu.receive_remote(46, HashedSlot::EMPTY));
        menu.receive_remote_carried(HashedSlot::of(&Some(stack(ItemKind::Stone, 5))));
        menu.resume_remote_updates();
        assert!(menu.changes(&inv).is_empty());
        menu.receive_remote(9, HashedSlot::of(&Some(stack(ItemKind::Stone, 1))));
        menu.receive_remote_carried(HashedSlot(Some(HashedItem {
            item: i32::from(ItemKind::Stone.id().0),
            count: 5,
            patched: true,
        })));
        assert_eq!(
            menu.changes(&inv),
            vec![
                RemoteUpdate::Slot(9, None),
                RemoteUpdate::Carried(Some(stack(ItemKind::Stone, 5)))
            ]
        );
        let (slots, carried) = menu.sync_all(&inv);
        assert_eq!(slots.len(), PLAYER_INVENTORY_SIZE);
        assert_eq!(carried, Some(stack(ItemKind::Stone, 5)));
        inv.set(10, Some(stack(ItemKind::Dirt, 1)));
        menu.set_remote(10, Some(stack(ItemKind::Dirt, 1)));
        assert!(menu.changes(&inv).is_empty());
    }
}
