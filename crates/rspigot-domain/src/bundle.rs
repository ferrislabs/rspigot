use std::num::NonZeroU8;

use rspigot_items::ItemKind;
use rspigot_nbt::{Compound, Tag};

use crate::item::{ItemStack, max_stack, same_item_same_components};
use crate::menu::BundleSound;
use crate::stack_components::{self, BUNDLE_CONTENTS, Slot};

const CONTAINER: &str = "minecraft:container";
const BEES: &str = "minecraft:bees";
const DAMAGE: &str = "minecraft:damage";
const SHULKER_SUFFIX: &str = "shulker_box";
const BUNDLE_SUFFIX: &str = "bundle";
const BUNDLE_IN_BUNDLE_WEIGHT: Fraction = Fraction { num: 1, den: 16 };
const FULL: Fraction = Fraction { num: 1, den: 1 };
const NO_SELECTION: i32 = -1;
pub const SELECTED: &str = "rspigot:bundle_selected";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fraction {
    num: i64,
    den: i64,
}

const fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a.abs() } else { gcd(b, a % b) }
}

impl Fraction {
    pub const ZERO: Self = Self { num: 0, den: 1 };

    fn new(num: i64, den: i64) -> Self {
        let divisor = gcd(num, den).max(1);
        Self {
            num: num / divisor,
            den: den / divisor,
        }
    }

    fn add(self, other: Self) -> Self {
        Self::new(
            self.num * other.den + other.num * self.den,
            self.den * other.den,
        )
    }

    fn sub(self, other: Self) -> Self {
        Self::new(
            self.num * other.den - other.num * self.den,
            self.den * other.den,
        )
    }

    fn times(self, count: i64) -> Self {
        Self::new(self.num * count, self.den)
    }

    fn whole_divisions_of(self, divisor: Self) -> i64 {
        (self.num * divisor.den) / (self.den * divisor.num)
    }

    pub fn as_f32(self) -> f32 {
        self.num as f32 / self.den as f32
    }
}

pub fn is_bundle(stack: &ItemStack) -> bool {
    match stack_components::slot(stack, &BUNDLE_CONTENTS) {
        Slot::Set(_) => true,
        Slot::Removed => false,
        Slot::Inherited => stack.item.kind().is_some_and(|kind| {
            let name = kind.name().trim_start_matches("minecraft:");
            name.ends_with(BUNDLE_SUFFIX)
        }),
    }
}

pub fn can_fit_inside(stack: &ItemStack) -> bool {
    stack
        .item
        .kind()
        .is_some_and(|kind: ItemKind| !kind.name().ends_with(SHULKER_SUFFIX))
}

fn is_stackable(stack: &ItemStack) -> bool {
    max_stack(stack) > 1
        && (stack_components::durability(stack).is_none()
            || stack.components.get_i32(DAMAGE).unwrap_or(0) == 0)
}

pub fn weight_of(stack: &ItemStack) -> Fraction {
    if is_bundle(stack) {
        return Contents::read(stack).weight.add(BUNDLE_IN_BUNDLE_WEIGHT);
    }
    let carries_bees = stack
        .components
        .get_list(BEES)
        .is_some_and(|bees| !bees.is_empty());
    if carries_bees {
        FULL
    } else {
        Fraction::new(1, i64::from(max_stack(stack)))
    }
}

fn stack_from(tag: &Tag) -> Option<ItemStack> {
    let entry = tag.as_compound()?;
    let item = ItemKind::from_name(entry.get_str("id")?)?.id();
    let count = u8::try_from(entry.get_i32("count").unwrap_or(1)).ok()?;
    Some(ItemStack {
        item,
        count: NonZeroU8::new(count)?,
        components: entry
            .get_compound("components")
            .cloned()
            .unwrap_or_default(),
    })
}

fn tag_of(stack: &ItemStack) -> Tag {
    let name = stack.item.kind().map_or("minecraft:air", ItemKind::name);
    let mut entry = Compound::new()
        .with("id", name)
        .with("count", i32::from(stack.count.get()));
    if !stack.components.is_empty() {
        entry.insert("components", stack.components.clone());
    }
    Tag::Compound(entry)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Contents {
    items: Vec<ItemStack>,
    weight: Fraction,
}

impl Contents {
    pub fn read(bundle: &ItemStack) -> Self {
        let items: Vec<ItemStack> = bundle
            .components
            .get_list(BUNDLE_CONTENTS.name)
            .unwrap_or_default()
            .iter()
            .filter_map(stack_from)
            .collect();
        let weight = items.iter().fold(Fraction::ZERO, |sum, stack| {
            sum.add(weight_of(stack).times(i64::from(stack.count.get())))
        });
        Self { items, weight }
    }

    pub fn write(&self, bundle: &mut ItemStack) {
        if self.items.is_empty() {
            bundle.components.remove(BUNDLE_CONTENTS.name);
        } else {
            bundle.components.insert(
                BUNDLE_CONTENTS.name,
                Tag::List(self.items.iter().map(tag_of).collect()),
            );
        }
    }

    pub fn items(&self) -> &[ItemStack] {
        &self.items
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn weight(&self) -> Fraction {
        self.weight
    }

    fn max_amount_to_add(&self, item_weight: Fraction) -> i64 {
        FULL.sub(self.weight).whole_divisions_of(item_weight).max(0)
    }

    pub fn try_insert(&mut self, stack: &mut ItemStack) -> u32 {
        if !can_fit_inside(stack) {
            return 0;
        }
        let weight = weight_of(stack);
        let amount = i64::from(stack.count.get()).min(self.max_amount_to_add(weight));
        if amount == 0 {
            return 0;
        }
        self.weight = self.weight.add(weight.times(amount));
        let moved = amount as u8;
        let existing = is_stackable(stack)
            .then(|| {
                self.items
                    .iter()
                    .position(|held| same_item_same_components(held, stack))
            })
            .flatten();
        match existing {
            None => {
                let mut part = stack.clone();
                part.count = NonZeroU8::new(moved).unwrap_or(NonZeroU8::MIN);
                self.items.insert(0, part);
            }
            Some(index) => {
                let mut merged = self.items.remove(index);
                merged.count = merged.count.saturating_add(moved);
                self.items.insert(0, merged);
            }
        }
        match NonZeroU8::new(stack.count.get() - moved) {
            Some(left) => stack.count = left,
            None => stack.count = NonZeroU8::MIN,
        }
        u32::from(moved)
    }

    pub fn max_amount_for(&self, stack: &ItemStack) -> u32 {
        if !can_fit_inside(stack) {
            return 0;
        }
        u32::try_from(self.max_amount_to_add(weight_of(stack))).unwrap_or(0)
    }

    pub fn remove_one(&mut self, selected: i32) -> Option<ItemStack> {
        if self.items.is_empty() {
            return None;
        }
        let index = usize::try_from(selected)
            .ok()
            .filter(|index| *index < self.items.len())
            .unwrap_or(0);
        let removed = self.items.remove(index);
        self.weight = self
            .weight
            .sub(weight_of(&removed).times(i64::from(removed.count.get())));
        Some(removed)
    }

    pub fn take_all(&mut self) -> Vec<ItemStack> {
        self.weight = Fraction::ZERO;
        std::mem::take(&mut self.items)
    }

    pub fn is_valid_selection(&self, index: i32) -> bool {
        usize::try_from(index).is_ok_and(|index| index < self.items.len())
    }
}

pub fn toggled_selection(current: i32, requested: i32, contents: &Contents) -> i32 {
    if current == requested || !contents.is_valid_selection(requested) {
        NO_SELECTION
    } else {
        requested
    }
}

pub fn selected(bundle: &ItemStack) -> i32 {
    bundle.components.get_i32(SELECTED).unwrap_or(NO_SELECTION)
}

pub fn set_selected(bundle: &mut ItemStack, index: i32) {
    if index < 0 {
        bundle.components.remove(SELECTED);
    } else {
        bundle.components.insert(SELECTED, index);
    }
}

pub fn toggle(bundle: &mut ItemStack, requested: i32) {
    let contents = Contents::read(bundle);
    let next = toggled_selection(selected(bundle), requested, &contents);
    set_selected(bundle, next);
}

pub trait BundleSession {
    fn slot_item(&self, slot: usize) -> Option<ItemStack>;
    fn take(&mut self, slot: usize, amount: u32, limit: u32) -> Option<ItemStack>;
    fn insert(&mut self, slot: usize, stack: Option<ItemStack>, amount: u32) -> Option<ItemStack>;
    fn store(&mut self, slot: usize, stack: Option<ItemStack>);
    fn cursor(&mut self) -> &mut Option<ItemStack>;
    fn sound(&mut self, sound: BundleSound);
    fn is_result(&self, slot: usize) -> bool;
}

pub fn click<S: BundleSession>(session: &mut S, target: usize, primary: bool) -> bool {
    let in_slot = session.slot_item(target);
    let carried = session.cursor().clone();
    if let Some(mut bundle) = carried.clone().filter(is_bundle) {
        if primary && in_slot.is_some() && !session.is_result(target) {
            let mut contents = Contents::read(&bundle);
            let mut inserted = 0;
            if let Some(slot_item) = &in_slot {
                let room = contents.max_amount_for(slot_item);
                if room > 0
                    && let Some(mut taken) =
                        session.take(target, u32::from(slot_item.count.get()), room)
                {
                    inserted = contents.try_insert(&mut taken);
                }
            }
            session.sound(if inserted > 0 {
                BundleSound::Insert
            } else {
                BundleSound::InsertFail
            });
            contents.write(&mut bundle);
            *session.cursor() = Some(bundle);
            return true;
        }
        if !primary && in_slot.is_none() {
            let mut contents = Contents::read(&bundle);
            if let Some(removed) = contents.remove_one(selected(&bundle)) {
                set_selected(&mut bundle, NO_SELECTION);
                let amount = u32::from(removed.count.get());
                match session.insert(target, Some(removed), amount) {
                    Some(mut rest) => {
                        contents.try_insert(&mut rest);
                    }
                    None => session.sound(BundleSound::RemoveOne),
                }
            }
            contents.write(&mut bundle);
            *session.cursor() = Some(bundle);
            return true;
        }
    }
    let Some(mut bundle) = in_slot.filter(is_bundle) else {
        return false;
    };
    let mut contents = Contents::read(&bundle);
    match (primary, carried) {
        (true, Some(mut other)) => {
            let original = u32::from(other.count.get());
            let moved = contents.try_insert(&mut other);
            session.sound(if moved > 0 {
                BundleSound::Insert
            } else {
                BundleSound::InsertFail
            });
            *session.cursor() = (moved < original).then_some(other);
            contents.write(&mut bundle);
            set_selected(&mut bundle, NO_SELECTION);
            session.store(target, Some(bundle));
            true
        }
        (false, None) => {
            if let Some(removed) = contents.remove_one(selected(&bundle)) {
                session.sound(BundleSound::RemoveOne);
                *session.cursor() = Some(removed);
            }
            contents.write(&mut bundle);
            set_selected(&mut bundle, NO_SELECTION);
            session.store(target, Some(bundle));
            true
        }
        _ => {
            if selected(&bundle) != NO_SELECTION {
                set_selected(&mut bundle, NO_SELECTION);
                session.store(target, Some(bundle));
            }
            false
        }
    }
}

pub fn destroyed_contents(stack: &ItemStack) -> Vec<ItemStack> {
    if is_bundle(stack) {
        return Contents::read(stack).items().to_vec();
    }
    if !stack
        .item
        .kind()
        .is_some_and(|kind| kind.name().ends_with(SHULKER_SUFFIX))
    {
        return Vec::new();
    }
    stack
        .components
        .get_list(CONTAINER)
        .unwrap_or_default()
        .iter()
        .filter_map(Tag::as_compound)
        .filter_map(|entry| stack_from(entry.get("item")?))
        .collect()
}

pub fn drop_contents(bundle: &mut ItemStack) -> Vec<ItemStack> {
    let mut contents = Contents::read(bundle);
    if contents.is_empty() {
        return Vec::new();
    }
    let dropped = contents.take_all();
    contents.write(bundle);
    dropped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(kind: ItemKind, count: u8) -> ItemStack {
        ItemStack::new(kind.id(), NonZeroU8::new(count).unwrap())
    }

    fn bundle() -> ItemStack {
        stack(ItemKind::Bundle, 1)
    }

    #[test]
    fn a_bundle_is_recognised_by_item_or_component() {
        assert!(is_bundle(&bundle()));
        assert!(is_bundle(&stack(ItemKind::RedBundle, 1)));
        assert!(!is_bundle(&stack(ItemKind::Stick, 1)));
    }

    #[test]
    fn items_weigh_one_over_their_stack_size() {
        assert_eq!(weight_of(&stack(ItemKind::Stone, 1)), Fraction::new(1, 64));
        assert_eq!(
            weight_of(&stack(ItemKind::EnderPearl, 1)),
            Fraction::new(1, 16)
        );
        assert_eq!(weight_of(&stack(ItemKind::DiamondSword, 1)), FULL);
    }

    #[test]
    fn a_bundle_holds_sixty_four_of_a_stack_of_sixty_four() {
        let mut contents = Contents::read(&bundle());
        let mut stones = stack(ItemKind::Stone, 64);
        assert_eq!(contents.try_insert(&mut stones), 64);
        assert_eq!(contents.weight(), FULL);
        let mut more = stack(ItemKind::Stone, 1);
        assert_eq!(contents.try_insert(&mut more), 0);
    }

    #[test]
    fn a_partial_insert_leaves_the_rest_in_the_source() {
        let mut contents = Contents::read(&bundle());
        let mut pearls = stack(ItemKind::EnderPearl, 16);
        assert_eq!(contents.try_insert(&mut pearls), 16);
        let mut swords = stack(ItemKind::Stone, 40);
        assert_eq!(contents.try_insert(&mut swords), 0);
        let mut cobble = stack(ItemKind::Stick, 64);
        let mut half = Contents::read(&bundle());
        assert_eq!(half.try_insert(&mut cobble), 64);
        let mut stones = stack(ItemKind::Stone, 40);
        assert_eq!(half.try_insert(&mut stones), 0);
        let mut third = Contents::read(&bundle());
        let mut gravel = stack(ItemKind::Stone, 30);
        assert_eq!(third.try_insert(&mut gravel), 30);
        let mut more = stack(ItemKind::Stone, 50);
        assert_eq!(third.try_insert(&mut more), 34);
        assert_eq!(more.count.get(), 16);
    }

    #[test]
    fn identical_stacks_merge_and_move_to_the_front() {
        let mut contents = Contents::read(&bundle());
        contents.try_insert(&mut stack(ItemKind::Stone, 3));
        contents.try_insert(&mut stack(ItemKind::Dirt, 2));
        contents.try_insert(&mut stack(ItemKind::Stone, 4));
        assert_eq!(contents.items()[0].count.get(), 7);
        assert_eq!(contents.items()[1].item, ItemKind::Dirt.id());
        assert_eq!(contents.items().len(), 2);
    }

    #[test]
    fn a_bundle_inside_a_bundle_weighs_a_sixteenth_plus_its_contents() {
        let mut inner = bundle();
        let mut contents = Contents::read(&inner);
        contents.try_insert(&mut stack(ItemKind::Stone, 32));
        contents.write(&mut inner);
        assert_eq!(
            weight_of(&inner),
            Fraction::new(1, 2).add(Fraction::new(1, 16))
        );
    }

    #[test]
    fn shulker_boxes_do_not_fit() {
        let mut contents = Contents::read(&bundle());
        assert_eq!(contents.try_insert(&mut stack(ItemKind::ShulkerBox, 1)), 0);
    }

    #[test]
    fn contents_round_trip_through_the_component() {
        let mut held = bundle();
        let mut contents = Contents::read(&held);
        contents.try_insert(&mut stack(ItemKind::Stone, 5));
        contents.try_insert(&mut stack(ItemKind::Apple, 2));
        contents.write(&mut held);
        let read = Contents::read(&held);
        assert_eq!(read, contents);
        assert_eq!(read.items().len(), 2);
    }

    #[test]
    fn removing_takes_the_selected_stack_or_the_first_one() {
        let mut contents = Contents::read(&bundle());
        contents.try_insert(&mut stack(ItemKind::Stone, 3));
        contents.try_insert(&mut stack(ItemKind::Dirt, 2));
        let selected = contents.remove_one(1).unwrap();
        assert_eq!(selected.item, ItemKind::Stone.id());
        let first = contents.remove_one(5).unwrap();
        assert_eq!(first.item, ItemKind::Dirt.id());
        assert!(contents.remove_one(0).is_none());
        assert_eq!(contents.weight(), Fraction::ZERO);
    }

    #[test]
    fn toggling_selects_deselects_and_rejects_out_of_range_indexes() {
        let mut contents = Contents::read(&bundle());
        contents.try_insert(&mut stack(ItemKind::Stone, 3));
        assert_eq!(toggled_selection(-1, 0, &contents), 0);
        assert_eq!(toggled_selection(0, 0, &contents), -1);
        assert_eq!(toggled_selection(-1, 4, &contents), -1);
        assert_eq!(toggled_selection(-1, -2, &contents), -1);
    }

    #[test]
    fn dropping_empties_the_bundle_in_order() {
        let mut held = bundle();
        let mut contents = Contents::read(&held);
        contents.try_insert(&mut stack(ItemKind::Stone, 3));
        contents.try_insert(&mut stack(ItemKind::Dirt, 2));
        contents.write(&mut held);
        let dropped = drop_contents(&mut held);
        assert_eq!(dropped.len(), 2);
        assert_eq!(dropped[0].item, ItemKind::Dirt.id());
        assert!(Contents::read(&held).is_empty());
        assert!(drop_contents(&mut held).is_empty());
    }
}

pub fn strip_runtime_selection(root: &mut Compound) {
    if let Some(Tag::Compound(components)) = root.get_mut("components") {
        components.remove(SELECTED);
    }
    for tag in root.values_mut() {
        strip_tag(tag);
    }
}

fn strip_tag(tag: &mut Tag) {
    match tag {
        Tag::Compound(compound) => strip_runtime_selection(compound),
        Tag::List(items) => {
            for item in items
                .iter_mut()
                .filter(|item| matches!(item, Tag::Compound(_) | Tag::List(_)))
            {
                strip_tag(item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod selection_save_tests {
    use super::*;
    use crate::entity::save::item_stack_to_nbt;

    #[test]
    fn a_saved_tree_loses_the_selected_index_at_any_depth() {
        let mut bundle = ItemStack::new(ItemKind::Bundle.id(), NonZeroU8::MIN);
        set_selected(&mut bundle, 0);
        let mut root = Compound::new().with(
            "Items",
            Tag::List(vec![Tag::Compound(item_stack_to_nbt(&bundle))]),
        );
        strip_runtime_selection(&mut root);
        let Some(Tag::List(items)) = root.get("Items") else {
            panic!("items");
        };
        let Tag::Compound(item) = &items[0] else {
            panic!("an item");
        };
        assert!(
            item.get_compound("components")
                .is_none_or(|components| !components.contains_key(SELECTED))
        );
    }
}
