use rspigot_blocks::{BlockKind, StateId};
use rspigot_data::compostable;

use crate::ItemId;
use crate::random::RandomSource;

pub const MAX_FILL_LEVEL: i32 = 7;
pub const READY_LEVEL: i32 = 8;
pub const READY_DELAY_TICKS: u64 = 20;
pub const FILL_EVENT: i32 = 1500;
pub const BONE_MEAL_EVENT: i32 = 1505;
pub const FILL_SOUND: &str = "minecraft:block.composter.fill";
pub const FILL_SUCCESS_SOUND: &str = "minecraft:block.composter.fill_success";
pub const EMPTY_SOUND: &str = "minecraft:block.composter.empty";
pub const READY_SOUND: &str = "minecraft:block.composter.ready";

pub fn is_composter(state: StateId) -> bool {
    state.kind() == Some(BlockKind::Composter)
}

pub fn level(state: StateId) -> Option<i32> {
    if !is_composter(state) {
        return None;
    }
    state.property("level")?.parse().ok()
}

pub fn with_level(state: StateId, level: i32) -> StateId {
    state
        .with_property("level", &level.to_string())
        .unwrap_or(state)
}

pub fn is_compostable(item: ItemId) -> bool {
    compostable::layers(item.0).is_some()
}

pub fn layer_count(item: ItemId, level: i32, random: &mut impl RandomSource) -> Option<i32> {
    let layers = compostable::layers(item.0)?;
    if level == 0 || layers.zero == 0 {
        return Some(1);
    }
    let roll = random.next_int(layers.total() as i32);
    Some(i32::from(roll < layers.one as i32))
}

pub fn add_layer(state: StateId, level: i32, count: i32) -> StateId {
    if count < 1 {
        return state;
    }
    with_level(state, (level + count).clamp(0, MAX_FILL_LEVEL))
}

pub fn emptied(state: StateId) -> StateId {
    with_level(state, 0)
}

pub fn made_ready(state: StateId) -> StateId {
    with_level(state, READY_LEVEL)
}

pub fn needs_ready_tick(state: StateId) -> bool {
    level(state) == Some(MAX_FILL_LEVEL)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rspigot_items::ItemKind;

    struct Script(Vec<i32>);

    impl RandomSource for Script {
        fn next_float(&mut self) -> f32 {
            0.0
        }

        fn next_int(&mut self, bound: i32) -> i32 {
            self.0.remove(0).min(bound - 1)
        }

        fn next_double(&mut self) -> f64 {
            0.0
        }

        fn next_boolean(&mut self) -> bool {
            false
        }

        fn next_gaussian(&mut self) -> f64 {
            0.0
        }
    }

    fn composter(level: i32) -> StateId {
        with_level(BlockKind::Composter.default_state(), level)
    }

    #[test]
    fn level_reads_and_writes_the_property() {
        assert_eq!(level(composter(0)), Some(0));
        assert_eq!(level(composter(5)), Some(5));
        assert_eq!(level(composter(8)), Some(8));
        assert_eq!(level(BlockKind::Stone.default_state()), None);
    }

    #[test]
    fn an_empty_composter_always_takes_one_layer_without_drawing() {
        let mut random = Script(Vec::new());
        assert_eq!(
            layer_count(ItemKind::WheatSeeds.id(), 0, &mut random),
            Some(1)
        );
    }

    #[test]
    fn a_partly_filled_composter_rolls_the_item_chance() {
        let seeds = ItemKind::WheatSeeds.id();
        assert_eq!(layer_count(seeds, 3, &mut Script(vec![29])), Some(1));
        assert_eq!(layer_count(seeds, 3, &mut Script(vec![30])), Some(0));
        assert_eq!(layer_count(seeds, 3, &mut Script(vec![99])), Some(0));
    }

    #[test]
    fn always_add_one_items_never_draw() {
        let mut random = Script(Vec::new());
        assert_eq!(layer_count(ItemKind::Cake.id(), 4, &mut random), Some(1));
    }

    #[test]
    fn items_without_the_component_have_no_layers() {
        assert_eq!(
            layer_count(ItemKind::Stone.id(), 0, &mut Script(Vec::new())),
            None
        );
        assert!(!is_compostable(ItemKind::Stone.id()));
        assert!(is_compostable(ItemKind::Bread.id()));
    }

    #[test]
    fn adding_layers_clamps_at_seven_and_skips_empty_counts() {
        assert_eq!(add_layer(composter(3), 3, 1), composter(4));
        assert_eq!(add_layer(composter(7), 7, 1), composter(7));
        assert_eq!(add_layer(composter(3), 3, 0), composter(3));
        assert_eq!(add_layer(composter(6), 6, 3), composter(7));
    }

    #[test]
    fn ready_and_empty_levels() {
        assert!(needs_ready_tick(composter(7)));
        assert!(!needs_ready_tick(composter(6)));
        assert!(!needs_ready_tick(composter(8)));
        assert_eq!(made_ready(composter(7)), composter(8));
        assert_eq!(emptied(composter(8)), composter(0));
    }
}
