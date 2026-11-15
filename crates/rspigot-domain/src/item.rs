use std::num::NonZeroU8;

use rspigot_nbt::{Compound, Tag};

use crate::entity::save::{component_from_tag, component_to_tag};
use crate::ids::ItemId;
use crate::text::Component;

pub const CUSTOM_NAME: &str = "minecraft:custom_name";
pub const LORE: &str = "minecraft:lore";
pub const CUSTOM_MODEL_DATA: &str = "minecraft:custom_model_data";
pub const MAX_STACK_SIZE: &str = "minecraft:max_stack_size";
const MAX_STACK_COMPONENT: u32 = 99;
pub const ENCHANTMENTS: &str = "minecraft:enchantments";
pub const MAX_LORE_LINES: usize = 256;
const ENCHANTMENT_LEVELS: &str = "levels";

#[derive(Debug, Clone, Default)]
pub struct CustomModelData {
    pub floats: Vec<f32>,
    pub flags: Vec<bool>,
    pub strings: Vec<String>,
    pub colors: Vec<i32>,
}

impl PartialEq for CustomModelData {
    fn eq(&self, other: &Self) -> bool {
        self.floats.len() == other.floats.len()
            && self
                .floats
                .iter()
                .zip(&other.floats)
                .all(|(a, b)| a.to_bits() == b.to_bits())
            && self.flags == other.flags
            && self.strings == other.strings
            && self.colors == other.colors
    }
}

impl Eq for CustomModelData {}

impl CustomModelData {
    pub fn is_empty(&self) -> bool {
        self.floats.is_empty()
            && self.flags.is_empty()
            && self.strings.is_empty()
            && self.colors.is_empty()
    }

    fn to_tag(&self) -> Tag {
        let mut out = Compound::new();
        if !self.floats.is_empty() {
            out.insert(
                "floats",
                Tag::List(self.floats.iter().map(|value| Tag::Float(*value)).collect()),
            );
        }
        if !self.flags.is_empty() {
            out.insert(
                "flags",
                Tag::List(self.flags.iter().map(|value| Tag::from(*value)).collect()),
            );
        }
        if !self.strings.is_empty() {
            out.insert(
                "strings",
                Tag::List(
                    self.strings
                        .iter()
                        .map(|value| Tag::from(value.as_str()))
                        .collect(),
                ),
            );
        }
        if !self.colors.is_empty() {
            out.insert(
                "colors",
                Tag::List(self.colors.iter().map(|value| Tag::Int(*value)).collect()),
            );
        }
        Tag::Compound(out)
    }

    fn from_tag(tag: &Tag) -> Option<Self> {
        let data = tag.as_compound()?;
        let list = |key: &str| data.get_list(key).unwrap_or_default();
        Some(Self {
            floats: list("floats")
                .iter()
                .filter_map(|value| value.as_f64().map(|value| value as f32))
                .collect(),
            flags: list("flags")
                .iter()
                .filter_map(|value| value.as_i64().map(|value| value != 0))
                .collect(),
            strings: list("strings")
                .iter()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect(),
            colors: list("colors")
                .iter()
                .filter_map(|value| value.as_i64().and_then(|value| i32::try_from(value).ok()))
                .collect(),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItemStack {
    pub item: ItemId,
    pub count: NonZeroU8,
    pub components: Compound,
}

impl ItemStack {
    pub fn new(item: ItemId, count: NonZeroU8) -> Self {
        Self {
            item,
            count,
            components: Compound::new(),
        }
    }

    pub fn custom_name(&self) -> Option<Component> {
        self.components
            .get(CUSTOM_NAME)
            .and_then(component_from_tag)
    }

    pub fn set_custom_name(&mut self, name: Option<&Component>) {
        match name {
            Some(name) => self.components.insert(CUSTOM_NAME, component_to_tag(name)),
            None => self.components.remove(CUSTOM_NAME),
        };
    }

    pub fn lore(&self) -> Vec<Component> {
        self.components
            .get_list(LORE)
            .map(|lines| lines.iter().filter_map(component_from_tag).collect())
            .unwrap_or_default()
    }

    pub fn set_lore(&mut self, lines: &[Component]) {
        if lines.is_empty() {
            self.components.remove(LORE);
        } else {
            let lines = lines.iter().map(component_to_tag).collect();
            self.components.insert(LORE, Tag::List(lines));
        }
    }

    pub fn custom_model_data(&self) -> Option<CustomModelData> {
        self.components
            .get(CUSTOM_MODEL_DATA)
            .and_then(CustomModelData::from_tag)
    }

    pub fn set_custom_model_data(&mut self, data: Option<&CustomModelData>) {
        match data {
            Some(data) => self.components.insert(CUSTOM_MODEL_DATA, data.to_tag()),
            None => self.components.remove(CUSTOM_MODEL_DATA),
        };
    }

    pub fn enchantments(&self) -> Vec<(String, u8)> {
        let Some(enchantments) = self.components.get_compound(ENCHANTMENTS) else {
            return Vec::new();
        };
        enchantments
            .get_compound(ENCHANTMENT_LEVELS)
            .unwrap_or(enchantments)
            .iter()
            .filter_map(|(key, level)| Some((key.to_owned(), u8::try_from(level.as_i64()?).ok()?)))
            .collect()
    }

    pub fn set_enchantments(&mut self, levels: &[(String, u8)]) {
        if levels.is_empty() {
            self.components.remove(ENCHANTMENTS);
        } else {
            let levels: Compound = levels
                .iter()
                .map(|(key, level)| (key.clone(), Tag::Int(i32::from(*level))))
                .collect();
            self.components.insert(ENCHANTMENTS, levels);
        }
    }
}

pub type Slot = Option<ItemStack>;

pub const HOTBAR_START: usize = 36;
pub const OFFHAND_SLOT: usize = 45;
pub const PLAYER_INVENTORY_SIZE: usize = 46;
const CONTAINER_MAX_STACK: u32 = 99;
const STORAGE_SLOTS: usize = 36;
const OFFHAND_INDEX: usize = 40;

pub fn count(slot: &Slot) -> u32 {
    slot.as_ref().map_or(0, |s| u32::from(s.count.get()))
}

pub fn max_stack(stack: &ItemStack) -> u32 {
    stack
        .components
        .get_i32(MAX_STACK_SIZE)
        .and_then(|size| u32::try_from(size).ok())
        .filter(|size| (1..=MAX_STACK_COMPONENT).contains(size))
        .unwrap_or_else(|| u32::from(stack.item.max_stack_size()))
}

pub fn with_count(stack: &ItemStack, count: u32) -> Slot {
    let count = NonZeroU8::new(u8::try_from(count).unwrap_or(u8::MAX))?;
    Some(ItemStack {
        count,
        ..stack.clone()
    })
}

pub fn split(slot: &mut Slot, amount: u32) -> Slot {
    let stack = slot.as_ref()?;
    let taken = amount.min(count(slot));
    let part = with_count(stack, taken);
    *slot = with_count(stack, count(slot) - taken);
    part
}

pub fn same_item_same_components(a: &ItemStack, b: &ItemStack) -> bool {
    a.item == b.item && a.components == b.components
}

pub fn same_item(a: &Slot, b: &Slot) -> bool {
    a.as_ref().map(|s| s.item) == b.as_ref().map(|s| s.item)
}

pub fn stacks_match(a: &Slot, b: &Slot) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.count == b.count && same_item_same_components(a, b),
        _ => false,
    }
}

pub const fn menu_slot(inventory_slot: usize) -> Option<usize> {
    match inventory_slot {
        0..=8 => Some(HOTBAR_START + inventory_slot),
        9..=35 => Some(inventory_slot),
        36..=39 => Some(44 - inventory_slot),
        OFFHAND_INDEX => Some(OFFHAND_SLOT),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlaceBack {
    pub placed: Vec<(usize, ItemStack)>,
    pub leftover: Slot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Inventory {
    slots: Vec<Slot>,
    pub selected_hotbar: u8,
}

impl Default for Inventory {
    fn default() -> Self {
        Self {
            slots: vec![None; PLAYER_INVENTORY_SIZE],
            selected_hotbar: 0,
        }
    }
}

impl Inventory {
    pub fn get(&self, slot: usize) -> Option<&ItemStack> {
        self.slots.get(slot)?.as_ref()
    }

    pub fn set(&mut self, slot: usize, stack: Slot) -> bool {
        match self.slots.get_mut(slot) {
            Some(entry) => {
                *entry = stack;
                true
            }
            None => false,
        }
    }

    pub fn held(&self) -> Option<&ItemStack> {
        self.get(HOTBAR_START + usize::from(self.selected_hotbar))
    }

    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }

    pub fn remove(&mut self, slot: usize, amount: u32) -> Slot {
        if amount == 0 {
            return None;
        }
        split(self.slots.get_mut(slot)?, amount)
    }

    fn storage(&self, inventory_slot: usize) -> &Slot {
        menu_slot(inventory_slot)
            .and_then(|slot| self.slots.get(slot))
            .unwrap_or(&None)
    }

    fn has_space_for(&self, inventory_slot: usize, stack: &ItemStack) -> bool {
        self.storage(inventory_slot).as_ref().is_some_and(|dest| {
            same_item_same_components(dest, stack)
                && max_stack(dest) > 1
                && u32::from(dest.count.get()) < max_stack(dest).min(CONTAINER_MAX_STACK)
        })
    }

    pub fn find_matching(&self, stack: &ItemStack) -> Option<usize> {
        (0..STORAGE_SLOTS).find(|slot| {
            self.storage(*slot)
                .as_ref()
                .is_some_and(|held| same_item_same_components(held, stack))
        })
    }

    pub fn suitable_hotbar_slot(&self) -> u8 {
        let selected = usize::from(self.selected_hotbar);
        let rotated = || (0..9).map(|offset| (offset + selected) % 9);
        let slot = rotated()
            .find(|slot| self.storage(*slot).is_none())
            .or_else(|| {
                rotated().find(|slot| {
                    self.storage(*slot)
                        .as_ref()
                        .is_some_and(|stack| stack.enchantments().is_empty())
                })
            })
            .unwrap_or(selected);
        slot as u8
    }

    pub fn pick_slot(&mut self, inventory_slot: usize) {
        self.selected_hotbar = self.suitable_hotbar_slot();
        let (Some(from), Some(to)) = (
            menu_slot(inventory_slot),
            menu_slot(usize::from(self.selected_hotbar)),
        ) else {
            return;
        };
        self.slots.swap(from, to);
    }

    pub fn add_and_pick(&mut self, stack: ItemStack) {
        self.selected_hotbar = self.suitable_hotbar_slot();
        let Some(selected) = menu_slot(usize::from(self.selected_hotbar)) else {
            return;
        };
        if self.slots[selected].is_some()
            && let Some(free) = self.free_slot().and_then(menu_slot)
        {
            self.slots[free] = self.slots[selected].take();
        }
        self.slots[selected] = Some(stack);
    }

    pub fn slot_with_remaining_space(&self, stack: &ItemStack) -> Option<usize> {
        let selected = usize::from(self.selected_hotbar);
        [selected, OFFHAND_INDEX]
            .into_iter()
            .chain(0..STORAGE_SLOTS)
            .find(|slot| self.has_space_for(*slot, stack))
    }

    pub fn free_slot(&self) -> Option<usize> {
        (0..STORAGE_SLOTS).find(|slot| self.storage(*slot).is_none())
    }

    fn add_resource(&mut self, inventory_slot: usize, stack: &ItemStack) -> u32 {
        let amount = u32::from(stack.count.get());
        let Some(slot) = menu_slot(inventory_slot) else {
            return amount;
        };
        let existing = count(&self.slots[slot]);
        let template = self.slots[slot].clone().unwrap_or_else(|| stack.clone());
        let space = max_stack(&template)
            .min(CONTAINER_MAX_STACK)
            .saturating_sub(existing);
        let added = amount.min(space);
        if added == 0 {
            return amount;
        }
        self.slots[slot] = with_count(&template, existing + added);
        amount - added
    }

    pub fn add(&mut self, stack: &mut Slot, infinite_materials: bool) -> bool {
        if stack.is_none() {
            return false;
        }
        let mut last = count(stack);
        while let Some(current) = stack.clone() {
            last = u32::from(current.count.get());
            let remaining = match self
                .slot_with_remaining_space(&current)
                .or_else(|| self.free_slot())
            {
                Some(slot) => self.add_resource(slot, &current),
                None => last,
            };
            *stack = with_count(&current, remaining);
            if remaining >= last {
                break;
            }
        }
        if count(stack) == last && infinite_materials {
            *stack = None;
            return true;
        }
        count(stack) < last
    }

    fn add_at(&mut self, inventory_slot: usize, stack: &mut Slot) -> bool {
        let mut last = count(stack);
        while let Some(current) = stack.clone() {
            last = u32::from(current.count.get());
            let remaining = self.add_resource(inventory_slot, &current);
            *stack = with_count(&current, remaining);
            if remaining >= last {
                break;
            }
        }
        count(stack) < last
    }

    pub fn place_back(&mut self, stack: ItemStack) -> PlaceBack {
        let mut outcome = PlaceBack::default();
        let mut stack = Some(stack);
        while let Some(current) = stack.clone() {
            let Some(slot) = self
                .slot_with_remaining_space(&current)
                .or_else(|| self.free_slot())
            else {
                outcome.leftover = stack;
                break;
            };
            let room = max_stack(&current).saturating_sub(count(self.storage(slot)));
            let mut part = split(&mut stack, room);
            if self.add_at(slot, &mut part)
                && let Some(placed) = self.storage(slot).clone()
            {
                outcome.placed.push((slot, placed));
            }
        }
        outcome
    }

    pub fn insert(&mut self, item: ItemId, count: u32, max_stack: u8) -> InsertOutcome {
        let max = u32::from(max_stack.max(1));
        let mut remaining = count;
        let mut changed = Vec::new();
        let order: Vec<usize> = (HOTBAR_START..HOTBAR_START + 9)
            .chain(9..HOTBAR_START)
            .collect();
        for &slot in &order {
            if remaining == 0 {
                break;
            }
            let Some(existing) = self.slots[slot].as_mut() else {
                continue;
            };
            if existing.item != item || !existing.components.is_empty() {
                continue;
            }
            let added = max
                .saturating_sub(u32::from(existing.count.get()))
                .min(remaining);
            if added > 0 {
                existing.count = NonZeroU8::new((u32::from(existing.count.get()) + added) as u8)
                    .unwrap_or(existing.count);
                remaining -= added;
                changed.push(slot);
            }
        }
        for &slot in &order {
            if remaining == 0 {
                break;
            }
            if self.slots[slot].is_none() {
                let added = max.min(remaining);
                if let Some(count) = NonZeroU8::new(added as u8) {
                    self.slots[slot] = Some(ItemStack::new(item, count));
                    remaining -= added;
                    changed.push(slot);
                }
            }
        }
        InsertOutcome { remaining, changed }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsertOutcome {
    pub remaining: u32,
    pub changed: Vec<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::NamedColor;

    #[test]
    fn insert_merges_hotbar_first_then_fills_empty_slots() {
        let mut inventory = Inventory::default();
        inventory.set(
            HOTBAR_START + 2,
            Some(ItemStack::new(ItemId(5), NonZeroU8::new(60).unwrap())),
        );
        let outcome = inventory.insert(ItemId(5), 10, 64);
        assert_eq!(outcome.remaining, 0);
        assert_eq!(outcome.changed, vec![HOTBAR_START + 2, HOTBAR_START]);
        assert_eq!(
            inventory.get(HOTBAR_START + 2).map(|s| s.count.get()),
            Some(64)
        );
        assert_eq!(inventory.get(HOTBAR_START).map(|s| s.count.get()), Some(6));
    }

    fn stone(count: u8) -> ItemStack {
        ItemStack::new(ItemId(1), NonZeroU8::new(count).unwrap())
    }

    #[test]
    fn custom_model_data_round_trips_through_the_component() {
        let mut stack = stone(1);
        assert_eq!(stack.custom_model_data(), None);
        let data = CustomModelData {
            floats: vec![1.5, 2.0],
            flags: vec![true, false],
            strings: vec!["a".to_owned()],
            colors: vec![0xFF0000],
        };
        stack.set_custom_model_data(Some(&data));
        assert_eq!(stack.custom_model_data(), Some(data));
        let stored = stack.components.get_compound(CUSTOM_MODEL_DATA).unwrap();
        assert_eq!(stored.get_list("colors").unwrap().len(), 1);
        stack.set_custom_model_data(None);
        assert!(stack.components.is_empty());
    }

    #[test]
    fn a_max_stack_size_component_replaces_the_item_default_within_vanilla_bounds() {
        let mut stack = stone(1);
        assert_eq!(max_stack(&stack), 64);
        stack.components.insert(MAX_STACK_SIZE, 16);
        assert_eq!(max_stack(&stack), 16);
        stack.components.insert(MAX_STACK_SIZE, 99);
        assert_eq!(max_stack(&stack), 99);
        stack.components.insert(MAX_STACK_SIZE, 0);
        assert_eq!(max_stack(&stack), 64);
        stack.components.insert(MAX_STACK_SIZE, 100);
        assert_eq!(max_stack(&stack), 64);
    }

    #[test]
    fn add_fills_the_selected_slot_then_the_offhand_then_storage_in_order() {
        let mut inventory = Inventory {
            selected_hotbar: 4,
            ..Inventory::default()
        };
        inventory.set(HOTBAR_START + 4, Some(stone(60)));
        inventory.set(OFFHAND_SLOT, Some(stone(62)));
        inventory.set(20, Some(stone(63)));
        let mut stack = Some(stone(10));
        assert!(inventory.add(&mut stack, false));
        assert_eq!(stack, None);
        assert_eq!(inventory.get(HOTBAR_START + 4), Some(&stone(64)));
        assert_eq!(inventory.get(OFFHAND_SLOT), Some(&stone(64)));
        assert_eq!(inventory.get(20), Some(&stone(64)));
        assert_eq!(inventory.get(HOTBAR_START), Some(&stone(3)));
    }

    #[test]
    fn add_reports_partial_fits_and_creative_voids_the_rest() {
        let mut inventory = Inventory::default();
        for slot in 9..HOTBAR_START + 9 {
            inventory.set(slot, Some(stone(64)));
        }
        inventory.set(9, Some(stone(60)));
        let mut stack = Some(stone(10));
        assert!(!inventory.add(&mut stack, false));
        assert_eq!(stack, Some(stone(6)));
        assert!(inventory.add(&mut stack, true));
        assert_eq!(stack, None);
        assert!(!inventory.add(&mut None, true));
    }

    fn enchanted(count: u8) -> ItemStack {
        let mut stack = stone(count);
        stack.set_enchantments(&[("minecraft:sharpness".to_owned(), 1)]);
        stack
    }

    #[test]
    fn a_matching_stack_is_found_in_the_hotbar_before_the_storage() {
        let mut inventory = Inventory::default();
        inventory.set(20, Some(stone(1)));
        inventory.set(HOTBAR_START + 3, Some(stone(2)));
        assert_eq!(inventory.find_matching(&stone(9)), Some(3));
        assert_eq!(inventory.find_matching(&enchanted(1)), None);
        inventory.set(HOTBAR_START + 3, None);
        assert_eq!(inventory.find_matching(&stone(9)), Some(20));
    }

    #[test]
    fn the_suitable_hotbar_slot_is_the_first_empty_one_after_the_selected_then_a_plain_item() {
        let mut inventory = Inventory {
            selected_hotbar: 7,
            ..Inventory::default()
        };
        for slot in 0..9 {
            inventory.set(HOTBAR_START + slot, Some(enchanted(1)));
        }
        assert_eq!(inventory.suitable_hotbar_slot(), 7);
        inventory.set(HOTBAR_START + 1, Some(stone(1)));
        assert_eq!(inventory.suitable_hotbar_slot(), 1);
        inventory.set(HOTBAR_START + 8, None);
        assert_eq!(inventory.suitable_hotbar_slot(), 8);
    }

    #[test]
    fn picking_a_storage_slot_swaps_it_with_the_suitable_hotbar_slot() {
        let mut inventory = Inventory {
            selected_hotbar: 2,
            ..Inventory::default()
        };
        inventory.set(20, Some(stone(5)));
        inventory.set(HOTBAR_START + 2, Some(enchanted(1)));
        inventory.pick_slot(20);
        assert_eq!(inventory.selected_hotbar, 3);
        assert_eq!(inventory.get(HOTBAR_START + 3), Some(&stone(5)));
        assert_eq!(inventory.get(20), None);
        assert_eq!(inventory.get(HOTBAR_START + 2), Some(&enchanted(1)));
        inventory.set(HOTBAR_START + 4, Some(enchanted(1)));
        for slot in [0, 1, 2, 3, 5, 6, 7, 8] {
            inventory.set(HOTBAR_START + slot, Some(enchanted(1)));
        }
        inventory.set(21, Some(stone(7)));
        inventory.pick_slot(21);
        assert_eq!(inventory.selected_hotbar, 3);
        assert_eq!(inventory.get(HOTBAR_START + 3), Some(&stone(7)));
        assert_eq!(inventory.get(21), Some(&enchanted(1)));
    }

    #[test]
    fn adding_and_picking_moves_the_displaced_item_to_the_first_free_slot() {
        let mut inventory = Inventory::default();
        for slot in 0..9 {
            inventory.set(HOTBAR_START + slot, Some(enchanted(slot as u8 + 1)));
        }
        inventory.add_and_pick(stone(1));
        assert_eq!(inventory.selected_hotbar, 0);
        assert_eq!(inventory.get(HOTBAR_START), Some(&stone(1)));
        assert_eq!(inventory.get(9), Some(&enchanted(1)));
    }

    #[test]
    fn place_back_reports_each_slot_and_the_leftover() {
        let mut inventory = Inventory::default();
        for slot in 9..HOTBAR_START + 9 {
            inventory.set(slot, Some(stone(64)));
        }
        inventory.set(HOTBAR_START + 2, Some(stone(61)));
        let outcome = inventory.place_back(stone(5));
        assert_eq!(outcome.placed, vec![(2, stone(64))]);
        assert_eq!(outcome.leftover, Some(stone(2)));
    }

    #[test]
    fn menu_slots_map_vanilla_inventory_indices() {
        assert_eq!(menu_slot(0), Some(HOTBAR_START));
        assert_eq!(menu_slot(8), Some(44));
        assert_eq!(menu_slot(9), Some(9));
        assert_eq!(menu_slot(36), Some(8));
        assert_eq!(menu_slot(39), Some(5));
        assert_eq!(menu_slot(40), Some(OFFHAND_SLOT));
        assert_eq!(menu_slot(41), None);
    }

    #[test]
    fn name_lore_and_enchantments_are_vanilla_components() {
        let mut stack = stone(1);
        stack.set_custom_name(Some(&Component::text("A").color(NamedColor::Gold)));
        assert_eq!(
            stack.components.get(CUSTOM_NAME),
            Some(&Tag::Compound(
                Compound::new().with("text", "A").with("color", "gold")
            ))
        );
        stack.set_lore(&[Component::text("A"), Component::text("B")]);
        assert_eq!(
            stack.components.get(LORE),
            Some(&Tag::List(vec![
                Tag::String("A".to_owned()),
                Tag::String("B".to_owned())
            ]))
        );
        stack.set_enchantments(&[("minecraft:sharpness".into(), 5)]);
        assert_eq!(
            stack.components.get(ENCHANTMENTS),
            Some(&Tag::Compound(
                Compound::new().with("minecraft:sharpness", 5)
            ))
        );
        assert_eq!(
            stack.custom_name(),
            Some(Component::text("A").color(NamedColor::Gold))
        );
        assert_eq!(stack.lore(), [Component::text("A"), Component::text("B")]);
        assert_eq!(
            stack.enchantments(),
            [("minecraft:sharpness".to_owned(), 5)]
        );

        let mut legacy = stone(1);
        legacy.components.insert(
            ENCHANTMENTS,
            Compound::new().with("levels", Compound::new().with("minecraft:sharpness", 5)),
        );
        assert_eq!(
            legacy.enchantments(),
            [("minecraft:sharpness".to_owned(), 5)]
        );

        stack.set_custom_name(None);
        stack.set_lore(&[]);
        stack.set_enchantments(&[]);
        assert!(stack.components.is_empty());
        assert_eq!(stack.custom_name(), None);
        assert!(stack.lore().is_empty());
        assert!(stack.enchantments().is_empty());
    }

    #[test]
    fn insert_reports_leftover_when_full() {
        let mut inventory = Inventory::default();
        for slot in 9..HOTBAR_START + 9 {
            inventory.set(
                slot,
                Some(ItemStack::new(ItemId(1), NonZeroU8::new(64).unwrap())),
            );
        }
        assert_eq!(inventory.insert(ItemId(2), 3, 64).remaining, 3);
        assert_eq!(inventory.insert(ItemId(1), 3, 64).remaining, 3);
    }
}
