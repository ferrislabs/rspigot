use rspigot_blocks::BlockKind;

use crate::random::RandomSource;
use crate::{BlockPos, BlockStateId};

pub const MAX_PICKLES: i32 = 4;
const COLUMNS: i32 = 5;
const SPREAD_ODDS: i32 = 6;

pub fn pickles(state: BlockStateId) -> Option<i32> {
    if state.kind() != Some(BlockKind::SeaPickle) {
        return None;
    }
    state.property("pickles")?.parse().ok()
}

pub fn is_alive(state: BlockStateId) -> bool {
    state.property("waterlogged") == Some("true")
}

pub fn with_pickles(state: BlockStateId, count: i32) -> BlockStateId {
    state
        .with_property("pickles", &count.min(MAX_PICKLES).to_string())
        .unwrap_or(state)
}

pub fn merged(state: BlockStateId) -> Option<BlockStateId> {
    pickles(state)
        .filter(|count| *count < MAX_PICKLES)
        .map(|count| with_pickles(state, count + 1))
}

pub fn plan_bone_meal(
    pos: BlockPos,
    state: BlockStateId,
    block: &impl Fn(BlockPos) -> BlockStateId,
    is_coral: &impl Fn(BlockStateId) -> bool,
    random: &mut impl RandomSource,
) -> Vec<(BlockPos, BlockStateId)> {
    if !is_alive(state) || !is_coral(block(pos.offset(0, -1, 0))) {
        return Vec::new();
    }
    let mut planned: Vec<(BlockPos, BlockStateId)> = Vec::new();
    let mut width = 1;
    let mut shift = 0;
    for column in 0..COLUMNS {
        for across in 0..width {
            let top = pos.y + 1;
            for y in (top - 2)..top {
                let at = BlockPos::new(pos.x - 2 + column, y, pos.z - shift + across);
                if at != pos
                    && random.next_int(SPREAD_ODDS) == 0
                    && block(at).kind() == Some(BlockKind::Water)
                    && is_coral(block(at.offset(0, -1, 0)))
                {
                    let count = random.next_int(MAX_PICKLES) + 1;
                    let fresh = BlockKind::SeaPickle
                        .default_state()
                        .with_property("pickles", &count.to_string())
                        .unwrap_or_else(|| BlockKind::SeaPickle.default_state());
                    planned.push((at, fresh));
                }
            }
        }
        if width < 2 {
            width += 2;
            shift += 1;
        } else {
            width -= 2;
            shift -= 1;
        }
    }
    planned.push((pos, with_pickles(state, MAX_PICKLES)));
    planned
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    struct Always;

    impl RandomSource for Always {
        fn next_float(&mut self) -> f32 {
            0.0
        }
        fn next_int(&mut self, _: i32) -> i32 {
            0
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

    struct Never;

    impl RandomSource for Never {
        fn next_float(&mut self) -> f32 {
            0.0
        }
        fn next_int(&mut self, bound: i32) -> i32 {
            bound - 1
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

    const POS: BlockPos = BlockPos::new(0, 64, 0);

    fn pickle(count: &str, waterlogged: &str) -> BlockStateId {
        BlockKind::SeaPickle
            .state_with(&[("pickles", count), ("waterlogged", waterlogged)])
            .unwrap()
    }

    fn reef() -> HashMap<BlockPos, BlockStateId> {
        let mut cells = HashMap::new();
        for x in -3..=3 {
            for z in -3..=3 {
                cells.insert(
                    BlockPos::new(x, 63, z),
                    BlockKind::BrainCoralBlock.default_state(),
                );
                for y in 64..=65 {
                    cells.insert(BlockPos::new(x, y, z), BlockKind::Water.default_state());
                }
            }
        }
        cells.insert(POS, pickle("1", "true"));
        cells
    }

    fn coral(state: BlockStateId) -> bool {
        state.kind() == Some(BlockKind::BrainCoralBlock)
    }

    #[test]
    fn placing_a_pickle_on_a_pickle_adds_one_up_to_four() {
        assert_eq!(merged(pickle("1", "true")), Some(pickle("2", "true")));
        assert_eq!(merged(pickle("3", "false")), Some(pickle("4", "false")));
        assert_eq!(merged(pickle("4", "true")), None);
        assert_eq!(merged(BlockKind::Stone.default_state()), None);
    }

    #[test]
    fn bone_meal_on_a_live_pickle_over_coral_fills_four_and_spreads() {
        let cells = reef();
        let block = |pos: BlockPos| cells.get(&pos).copied().unwrap_or(BlockStateId::AIR);
        let planned = plan_bone_meal(POS, block(POS), &block, &coral, &mut Always);
        assert_eq!(planned.last(), Some(&(POS, pickle("4", "true"))));
        let spread = planned.len() - 1;
        assert!(spread > 4);
        assert!(
            planned[..spread]
                .iter()
                .all(|(_, state)| pickles(*state) == Some(1))
        );
        assert!(planned[..spread].iter().all(|(at, _)| at.x.abs() <= 2));
    }

    #[test]
    fn spread_follows_the_one_three_one_three_one_columns() {
        let cells = reef();
        let block = |pos: BlockPos| cells.get(&pos).copied().unwrap_or(BlockStateId::AIR);
        let planned = plan_bone_meal(POS, block(POS), &block, &coral, &mut Always);
        let in_column = |x: i32| planned.iter().filter(|(at, _)| at.x == x).count();
        assert_eq!(in_column(-2), 1);
        assert_eq!(in_column(-1), 3);
        assert_eq!(in_column(0), 1);
        assert_eq!(in_column(1), 3);
        assert_eq!(in_column(2), 1);
    }

    #[test]
    fn a_dead_pickle_or_one_off_the_coral_does_nothing() {
        let mut cells = reef();
        cells.insert(POS, pickle("1", "false"));
        let block = |pos: BlockPos| cells.get(&pos).copied().unwrap_or(BlockStateId::AIR);
        assert!(plan_bone_meal(POS, block(POS), &block, &coral, &mut Always).is_empty());
        let mut cells = reef();
        cells.insert(POS.offset(0, -1, 0), BlockKind::Stone.default_state());
        let block = |pos: BlockPos| cells.get(&pos).copied().unwrap_or(BlockStateId::AIR);
        assert!(plan_bone_meal(POS, block(POS), &block, &coral, &mut Always).is_empty());
    }

    #[test]
    fn with_bad_luck_only_the_clicked_pickle_grows() {
        let cells = reef();
        let block = |pos: BlockPos| cells.get(&pos).copied().unwrap_or(BlockStateId::AIR);
        let planned = plan_bone_meal(POS, block(POS), &block, &coral, &mut Never);
        assert_eq!(planned, vec![(POS, pickle("4", "true"))]);
    }
}
