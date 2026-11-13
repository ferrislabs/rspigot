use rspigot_blocks::StateId;
use rspigot_domain::BlockPos;
use rspigot_domain::item::{self, ItemStack, Slot};
use rspigot_domain::spawn::placement::is_redstone_conductor;

use crate::world::{AiWorld, TargetKind};

pub const MAX_PICKUP: u32 = 16;
const CAT_CLASS: &str = "Cat";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChestSide {
    Single,
    Left,
    Right,
}

pub fn chest_side(state: StateId) -> ChestSide {
    match state.property("type") {
        Some("left") => ChestSide::Left,
        Some("right") => ChestSide::Right,
        _ => ChestSide::Single,
    }
}

pub fn connected_pos(pos: BlockPos, state: StateId) -> BlockPos {
    let (dx, dz) = match (state.property("facing"), chest_side(state)) {
        (Some("north"), ChestSide::Right) => (-1, 0),
        (Some("north"), _) => (1, 0),
        (Some("east"), ChestSide::Right) => (0, -1),
        (Some("east"), _) => (0, 1),
        (Some("south"), ChestSide::Right) => (1, 0),
        (Some("south"), _) => (-1, 0),
        (Some("west"), ChestSide::Right) => (0, 1),
        (Some("west"), _) => (0, -1),
        _ => (0, 0),
    };
    pos.offset(dx, 0, dz)
}

pub fn is_blocked_by_block(world: &impl AiWorld, pos: BlockPos) -> bool {
    world
        .state(pos.offset(0, 1, 0))
        .is_some_and(is_redstone_conductor)
}

pub fn is_cat_sitting_on(world: &impl AiWorld, pos: BlockPos) -> bool {
    let area = rspigot_domain::entity::Aabb::new(
        f64::from(pos.x),
        f64::from(pos.y + 1),
        f64::from(pos.z),
        f64::from(pos.x + 1),
        f64::from(pos.y + 2),
        f64::from(pos.z + 1),
    );
    world
        .living_of(TargetKind::Class(CAT_CLASS), &area)
        .iter()
        .any(|cat| cat.facts.sitting)
}

pub fn is_chest_blocked_at(world: &impl AiWorld, pos: BlockPos) -> bool {
    is_blocked_by_block(world, pos) || is_cat_sitting_on(world, pos)
}

pub fn is_empty(slots: &[Slot]) -> bool {
    slots.iter().all(Option::is_none)
}

pub fn has_item_matching(slots: &[Slot], hand: &ItemStack) -> bool {
    slots.iter().flatten().any(|stack| stack.item == hand.item)
}

pub fn pick_up(slots: &mut [Slot]) -> Slot {
    let index = slots.iter().position(Option::is_some)?;
    let amount = item::count(&slots[index]).min(MAX_PICKUP);
    item::split(&mut slots[index], amount)
}

pub fn add_items(slots: &mut [Slot], held: Slot) -> Slot {
    let mut held = held?;
    for slot in slots.iter_mut() {
        match slot {
            None => {
                *slot = Some(held);
                return None;
            }
            Some(inside) if item::same_item_same_components(inside, &held) => {
                let max = item::max_stack(inside);
                let count = u32::from(inside.count.get());
                if count < max {
                    let room = max - count;
                    let moved = room.min(u32::from(held.count.get()));
                    *inside = item::with_count(inside, count + moved)?;
                    held =
                        item::with_count(&held, u32::from(held.count.get()).saturating_sub(room))?;
                }
            }
            Some(_) => {}
        }
    }
    Some(held)
}

pub fn changed(before: &[Slot], after: &[Slot]) -> Vec<(usize, Slot)> {
    before
        .iter()
        .zip(after)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(index, (_, b))| (index, b.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU8;

    use rspigot_blocks::BlockKind;
    use rspigot_items::ItemKind;

    use super::*;

    fn stack(kind: ItemKind, count: u8) -> Slot {
        Some(ItemStack::new(kind.id(), NonZeroU8::new(count).unwrap()))
    }

    #[test]
    fn picking_up_takes_at_most_sixteen_from_the_first_non_empty_slot() {
        let mut slots = vec![None, stack(ItemKind::Apple, 40), stack(ItemKind::Stick, 3)];
        let taken = pick_up(&mut slots);
        assert_eq!(taken, stack(ItemKind::Apple, 16));
        assert_eq!(slots[1], stack(ItemKind::Apple, 24));
        assert_eq!(slots[2], stack(ItemKind::Stick, 3));
    }

    #[test]
    fn picking_up_a_small_stack_empties_its_slot() {
        let mut slots = vec![stack(ItemKind::Stick, 5), None];
        assert_eq!(pick_up(&mut slots), stack(ItemKind::Stick, 5));
        assert!(is_empty(&slots));
        assert_eq!(pick_up(&mut slots), None);
    }

    #[test]
    fn placing_fills_matching_stacks_in_order_before_an_empty_slot_is_used() {
        let mut slots = vec![stack(ItemKind::Apple, 60), stack(ItemKind::Apple, 62), None];
        let rest = add_items(&mut slots, stack(ItemKind::Apple, 10));
        assert_eq!(rest, None);
        assert_eq!(slots[0], stack(ItemKind::Apple, 64));
        assert_eq!(slots[1], stack(ItemKind::Apple, 64));
        assert_eq!(slots[2], stack(ItemKind::Apple, 4));
    }

    #[test]
    fn placing_into_an_empty_slot_moves_the_whole_stack() {
        let mut slots = vec![stack(ItemKind::Stick, 1), None, None];
        let rest = add_items(&mut slots, stack(ItemKind::Apple, 16));
        assert_eq!(rest, None);
        assert_eq!(slots[1], stack(ItemKind::Apple, 16));
    }

    #[test]
    fn placing_into_a_full_container_hands_back_the_remainder() {
        let mut slots = vec![stack(ItemKind::Apple, 60), stack(ItemKind::Stick, 3)];
        let rest = add_items(&mut slots, stack(ItemKind::Apple, 10));
        assert_eq!(rest, stack(ItemKind::Apple, 6));
        assert_eq!(slots[0], stack(ItemKind::Apple, 64));
        let mut full = vec![stack(ItemKind::Stick, 3)];
        assert_eq!(
            add_items(&mut full, stack(ItemKind::Apple, 2)),
            stack(ItemKind::Apple, 2)
        );
    }

    #[test]
    fn components_must_match_to_merge() {
        let mut named = ItemStack::new(ItemKind::Apple.id(), NonZeroU8::new(5).unwrap());
        named.set_custom_name(Some(&rspigot_domain::text::Component::text("x")));
        let mut slots = vec![Some(named.clone()), None];
        let rest = add_items(&mut slots, stack(ItemKind::Apple, 3));
        assert_eq!(rest, None);
        assert_eq!(slots[0], Some(named));
        assert_eq!(slots[1], stack(ItemKind::Apple, 3));
    }

    #[test]
    fn only_the_item_must_match_for_the_leaving_requirement() {
        let slots = vec![stack(ItemKind::Apple, 1)];
        let hand = ItemStack::new(ItemKind::Apple.id(), NonZeroU8::new(9).unwrap());
        assert!(has_item_matching(&slots, &hand));
        let other = ItemStack::new(ItemKind::Stick.id(), NonZeroU8::new(1).unwrap());
        assert!(!has_item_matching(&slots, &other));
    }

    #[test]
    fn changed_lists_only_the_modified_slots() {
        let before = vec![stack(ItemKind::Apple, 3), None, stack(ItemKind::Stick, 1)];
        let after = vec![stack(ItemKind::Apple, 3), stack(ItemKind::Stick, 2), None];
        assert_eq!(
            changed(&before, &after),
            vec![(1, stack(ItemKind::Stick, 2)), (2, None)]
        );
    }

    fn chest(facing: &str, side: &str) -> StateId {
        BlockKind::Chest
            .default_state()
            .with_property("facing", facing)
            .and_then(|state| state.with_property("type", side))
            .unwrap()
    }

    #[test]
    fn the_connected_half_is_found_like_the_vanilla_direction() {
        let at = BlockPos::new(0, 64, 0);
        assert_eq!(
            connected_pos(at, chest("north", "left")),
            BlockPos::new(1, 64, 0)
        );
        assert_eq!(
            connected_pos(at, chest("north", "right")),
            BlockPos::new(-1, 64, 0)
        );
        assert_eq!(
            connected_pos(at, chest("east", "left")),
            BlockPos::new(0, 64, 1)
        );
        assert_eq!(
            connected_pos(at, chest("south", "left")),
            BlockPos::new(-1, 64, 0)
        );
        assert_eq!(
            connected_pos(at, chest("west", "right")),
            BlockPos::new(0, 64, 1)
        );
        assert_eq!(chest_side(chest("north", "single")), ChestSide::Single);
    }
}
