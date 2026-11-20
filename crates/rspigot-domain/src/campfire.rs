use std::num::NonZeroU8;

use rspigot_items::CookingKind;

use crate::furnace::{CookingRecipe, recipe_for};
use crate::item::{ItemStack, Slot};

pub const SLOTS: usize = 4;
const COOLING_PER_TICK: i32 = 2;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Campfire {
    pub items: [Slot; SLOTS],
    pub progress: [i32; SLOTS],
    pub total: [i32; SLOTS],
}

impl Campfire {
    pub fn can_cook(stack: &ItemStack) -> bool {
        recipe_for(CookingKind::Campfire, stack).is_some()
    }

    pub fn recipe(stack: &ItemStack) -> Option<CookingRecipe> {
        recipe_for(CookingKind::Campfire, stack)
    }

    pub fn free_slot(&self) -> Option<usize> {
        self.items.iter().position(Option::is_none)
    }

    pub fn put(&mut self, slot: usize, stack: &ItemStack, total: i32) {
        self.total[slot] = total;
        self.progress[slot] = 0;
        self.items[slot] = Some(ItemStack {
            count: NonZeroU8::MIN,
            ..stack.clone()
        });
    }

    pub fn place(&mut self, stack: &ItemStack) -> bool {
        let Some(recipe) = Self::recipe(stack) else {
            return false;
        };
        let Some(slot) = self.free_slot() else {
            return false;
        };
        self.put(slot, stack, i32::from(recipe.cooking_time));
        true
    }

    pub fn advance(&mut self, slot: usize) -> Option<(ItemStack, ItemStack)> {
        let stack = self.items[slot].as_ref()?;
        self.progress[slot] += 1;
        if self.progress[slot] < self.total[slot] {
            return None;
        }
        Some((stack.clone(), Self::result_of(stack)))
    }

    pub fn result_of(stack: &ItemStack) -> ItemStack {
        Self::recipe(stack).map_or_else(|| stack.clone(), |recipe| recipe.result)
    }

    pub fn cook(&mut self) -> Vec<ItemStack> {
        let mut done = Vec::new();
        for slot in 0..SLOTS {
            if let Some((_, result)) = self.advance(slot) {
                done.push(result);
                self.items[slot] = None;
            }
        }
        done
    }

    pub fn cool(&mut self) -> bool {
        let mut changed = false;
        for slot in 0..SLOTS {
            if self.progress[slot] > 0 {
                changed = true;
                self.progress[slot] =
                    (self.progress[slot] - COOLING_PER_TICK).clamp(0, self.total[slot]);
            }
        }
        changed
    }

    pub fn is_empty(&self) -> bool {
        self.items.iter().all(Option::is_none)
    }
}

#[cfg(test)]
mod tests {
    use rspigot_items::ItemKind;

    use super::*;

    fn raw(kind: ItemKind, amount: u8) -> ItemStack {
        ItemStack::new(kind.id(), NonZeroU8::new(amount).unwrap())
    }

    #[test]
    fn food_goes_in_the_first_free_slot_one_at_a_time() {
        let mut fire = Campfire::default();
        assert!(fire.place(&raw(ItemKind::Beef, 5)));
        assert!(fire.place(&raw(ItemKind::Potato, 1)));
        assert_eq!(fire.items[0], Some(raw(ItemKind::Beef, 1)));
        assert_eq!(fire.items[1], Some(raw(ItemKind::Potato, 1)));
        assert_eq!(fire.total[0], 600);
        fire.items[0] = None;
        assert!(fire.place(&raw(ItemKind::Cod, 1)));
        assert_eq!(fire.items[0], Some(raw(ItemKind::Cod, 1)));
    }

    #[test]
    fn only_campfire_food_is_taken_and_a_full_fire_takes_nothing() {
        let mut fire = Campfire::default();
        assert!(!fire.place(&raw(ItemKind::Dirt, 1)));
        assert!(!fire.place(&raw(ItemKind::RawIron, 1)));
        for _ in 0..SLOTS {
            assert!(fire.place(&raw(ItemKind::Beef, 1)));
        }
        assert!(!fire.place(&raw(ItemKind::Beef, 1)));
        assert!(!Campfire::can_cook(&raw(ItemKind::Dirt, 1)));
        assert!(Campfire::can_cook(&raw(ItemKind::Kelp, 1)));
    }

    #[test]
    fn an_item_is_done_on_the_600th_lit_tick() {
        let mut fire = Campfire::default();
        fire.place(&raw(ItemKind::Beef, 1));
        for _ in 0..599 {
            assert!(fire.cook().is_empty());
        }
        assert_eq!(fire.cook(), vec![raw(ItemKind::CookedBeef, 1)]);
        assert!(fire.is_empty());
    }

    #[test]
    fn each_slot_cooks_on_its_own_clock() {
        let mut fire = Campfire::default();
        fire.place(&raw(ItemKind::Beef, 1));
        for _ in 0..100 {
            fire.cook();
        }
        fire.place(&raw(ItemKind::Potato, 1));
        for _ in 0..500 {
            fire.cook();
        }
        assert_eq!(fire.items[0], None);
        assert_eq!(fire.items[1], Some(raw(ItemKind::Potato, 1)));
        assert_eq!(fire.progress[1], 500);
    }

    #[test]
    fn progress_fades_by_two_per_tick_when_the_fire_is_out_and_items_stay() {
        let mut fire = Campfire::default();
        fire.place(&raw(ItemKind::Beef, 1));
        for _ in 0..10 {
            fire.cook();
        }
        assert!(fire.cool());
        assert_eq!(fire.progress[0], 8);
        for _ in 0..10 {
            fire.cool();
        }
        assert_eq!(fire.progress[0], 0);
        assert!(!fire.cool());
        assert_eq!(fire.items[0], Some(raw(ItemKind::Beef, 1)));
    }

    #[test]
    fn a_new_item_starts_from_zero_whatever_the_slot_held_before() {
        let mut fire = Campfire::default();
        fire.place(&raw(ItemKind::Dirt, 1));
        fire.place(&raw(ItemKind::Beef, 1));
        for _ in 0..600 {
            fire.cook();
        }
        assert_eq!(fire.progress[0], 600);
        fire.place(&raw(ItemKind::Potato, 1));
        assert_eq!(fire.progress[0], 0);
        assert_eq!(fire.total[0], 600);
    }

    #[test]
    fn an_item_with_no_recipe_left_in_a_slot_comes_back_as_it_is() {
        let mut fire = Campfire::default();
        fire.items[2] = Some(raw(ItemKind::Dirt, 1));
        fire.total[2] = 3;
        fire.cook();
        fire.cook();
        assert_eq!(fire.cook(), vec![raw(ItemKind::Dirt, 1)]);
    }

    #[test]
    fn a_stale_progress_does_not_count_for_an_empty_slot() {
        let mut fire = Campfire {
            progress: [50, 0, 0, 0],
            total: [600, 0, 0, 0],
            ..Campfire::default()
        };
        assert!(fire.cook().is_empty());
        assert_eq!(fire.progress[0], 50);
    }
}
