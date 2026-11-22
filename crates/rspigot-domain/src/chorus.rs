use rspigot_blocks::BlockKind;

use crate::random::RandomSource;
use crate::{BlockPos, BlockStateId};

pub const GROWN_EVENT: i32 = 0x409;
pub const DEAD_EVENT: i32 = 0x40a;
pub const MAX_LIVING_AGE: i32 = 4;
pub const DEAD_AGE: i32 = 5;
const HORIZONTAL: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];
const SUPPORT_SEARCH: i32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Change {
    pub pos: BlockPos,
    pub state: BlockStateId,
    pub event: Option<i32>,
}

pub fn flower(age: i32) -> BlockStateId {
    BlockKind::ChorusFlower
        .default_state()
        .with_property("age", &age.to_string())
        .unwrap_or_else(|| BlockKind::ChorusFlower.default_state())
}

fn age_of(state: BlockStateId) -> i32 {
    state
        .property("age")
        .and_then(|age| age.parse().ok())
        .unwrap_or(DEAD_AGE)
}

fn supports(state: BlockStateId) -> bool {
    state.kind() == Some(BlockKind::EndStone)
}

fn is_plant(state: BlockStateId) -> bool {
    state.kind() == Some(BlockKind::ChorusPlant)
}

fn is_flower(state: BlockStateId) -> bool {
    state.kind() == Some(BlockKind::ChorusFlower)
}

pub fn plant_state(block: &impl Fn(BlockPos) -> BlockStateId, pos: BlockPos) -> BlockStateId {
    let linked = |state: BlockStateId| is_plant(state) || is_flower(state);
    let faces = [
        ("down", (0, -1, 0)),
        ("up", (0, 1, 0)),
        ("north", (0, 0, -1)),
        ("east", (1, 0, 0)),
        ("south", (0, 0, 1)),
        ("west", (-1, 0, 0)),
    ];
    let mut state = BlockKind::ChorusPlant.default_state();
    for (name, (dx, dy, dz)) in faces {
        let neighbour = block(pos.offset(dx, dy, dz));
        let connected = linked(neighbour) || name == "down" && supports(neighbour);
        state = state
            .with_property(name, if connected { "true" } else { "false" })
            .unwrap_or(state);
    }
    state
}

fn all_neighbours_empty(
    block: &impl Fn(BlockPos) -> BlockStateId,
    pos: BlockPos,
    ignore: Option<(i32, i32)>,
) -> bool {
    HORIZONTAL
        .iter()
        .filter(|step| Some(**step) != ignore)
        .all(|(dx, dz)| block(pos.offset(*dx, 0, *dz)).is_air())
}

pub fn plan(
    block: &impl Fn(BlockPos) -> BlockStateId,
    pos: BlockPos,
    state: BlockStateId,
    max_y: i32,
    random: &mut impl RandomSource,
) -> Vec<Change> {
    let above = pos.offset(0, 1, 0);
    if !block(above).is_air() || above.y > max_y {
        return Vec::new();
    }
    let age = age_of(state);
    if age > MAX_LIVING_AGE {
        return Vec::new();
    }
    let mut can_grow_up = false;
    let mut ends_on_support = false;
    let below = block(pos.offset(0, -1, 0));
    if supports(below) {
        can_grow_up = true;
    } else if is_plant(below) {
        let mut height = 1;
        for _ in 0..SUPPORT_SEARCH {
            let lower = block(pos.offset(0, -(height + 1), 0));
            if !is_plant(lower) {
                ends_on_support = supports(lower);
                break;
            }
            height += 1;
        }
        let blocked = height > 1 && {
            let bound = if ends_on_support { 5 } else { 4 };
            random.next_int(bound) < height
        };
        can_grow_up = !blocked;
    } else if below.is_air() {
        can_grow_up = true;
    }
    if can_grow_up
        && all_neighbours_empty(block, above, None)
        && block(pos.offset(0, 2, 0)).is_air()
    {
        let grown = Change {
            pos: above,
            state: flower(age),
            event: Some(GROWN_EVENT),
        };
        let overlay = |at: BlockPos| if at == above { grown.state } else { block(at) };
        return vec![
            Change {
                pos,
                state: plant_state(&overlay, pos),
                event: None,
            },
            grown,
        ];
    }
    if age < MAX_LIVING_AGE {
        let mut tries = random.next_int(4);
        if ends_on_support {
            tries += 1;
        }
        let mut placed: Vec<Change> = Vec::new();
        for _ in 0..tries {
            let step = HORIZONTAL[random.next_int(4) as usize];
            let target = pos.offset(step.0, 0, step.1);
            let free = block(target).is_air()
                && block(target.offset(0, -1, 0)).is_air()
                && all_neighbours_empty(block, target, Some((-step.0, -step.1)));
            if free {
                placed.push(Change {
                    pos: target,
                    state: flower(age + 1),
                    event: Some(GROWN_EVENT),
                });
            }
        }
        if placed.is_empty() {
            return vec![Change {
                pos,
                state: flower(DEAD_AGE),
                event: Some(DEAD_EVENT),
            }];
        }
        let overlay = |at: BlockPos| {
            placed
                .iter()
                .find(|change| change.pos == at)
                .map_or_else(|| block(at), |change| change.state)
        };
        let mut changes = vec![Change {
            pos,
            state: plant_state(&overlay, pos),
            event: None,
        }];
        changes.extend(placed.iter().copied());
        return changes;
    }
    vec![Change {
        pos,
        state: flower(DEAD_AGE),
        event: Some(DEAD_EVENT),
    }]
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    struct Script(Vec<i32>);

    impl RandomSource for Script {
        fn next_float(&mut self) -> f32 {
            0.0
        }
        fn next_int(&mut self, bound: i32) -> i32 {
            let value = if self.0.is_empty() {
                0
            } else {
                self.0.remove(0)
            };
            assert!(value < bound, "{value} >= {bound}");
            value
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

    const POS: BlockPos = BlockPos::new(0, 70, 0);

    fn world(cells: &[(BlockPos, BlockStateId)]) -> HashMap<BlockPos, BlockStateId> {
        cells.iter().copied().collect()
    }

    fn run(cells: &HashMap<BlockPos, BlockStateId>, script: Vec<i32>) -> Vec<Change> {
        let block = |pos: BlockPos| cells.get(&pos).copied().unwrap_or(BlockStateId::AIR);
        plan(&block, POS, block(POS), 319, &mut Script(script))
    }

    fn on_end_stone(age: i32) -> HashMap<BlockPos, BlockStateId> {
        world(&[
            (POS.offset(0, -1, 0), BlockKind::EndStone.default_state()),
            (POS, flower(age)),
        ])
    }

    #[test]
    fn a_flower_on_end_stone_grows_a_stem_and_a_new_flower_of_the_same_age() {
        let changes = run(&on_end_stone(2), Vec::new());
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[1].pos, POS.offset(0, 1, 0));
        assert_eq!(changes[1].state, flower(2));
        assert_eq!(changes[1].event, Some(GROWN_EVENT));
        assert_eq!(changes[0].pos, POS);
        assert_eq!(changes[0].state.kind(), Some(BlockKind::ChorusPlant));
        assert_eq!(changes[0].state.property("up"), Some("true"));
        assert_eq!(changes[0].state.property("down"), Some("true"));
        assert_eq!(changes[0].state.property("north"), Some("false"));
    }

    #[test]
    fn a_blocked_top_grows_sideways_flowers_one_age_older() {
        let mut cells = on_end_stone(1);
        cells.insert(POS.offset(0, 2, 0), BlockKind::Stone.default_state());
        let changes = run(&cells, vec![1, 1]);
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[1].pos, POS.offset(1, 0, 0));
        assert_eq!(changes[1].state, flower(2));
        assert_eq!(changes[0].state.property("east"), Some("true"));
    }

    #[test]
    fn a_flower_that_cannot_spread_dies() {
        let mut cells = on_end_stone(1);
        cells.insert(POS.offset(0, 2, 0), BlockKind::Stone.default_state());
        let changes = run(&cells, vec![0]);
        assert_eq!(
            changes,
            vec![Change {
                pos: POS,
                state: flower(DEAD_AGE),
                event: Some(DEAD_EVENT)
            }]
        );
    }

    #[test]
    fn the_oldest_living_flower_dies_instead_of_spreading() {
        let mut cells = on_end_stone(4);
        cells.insert(POS.offset(0, 2, 0), BlockKind::Stone.default_state());
        let changes = run(&cells, Vec::new());
        assert_eq!(changes[0].state, flower(DEAD_AGE));
    }

    #[test]
    fn a_dead_flower_or_a_covered_one_does_nothing() {
        assert!(run(&on_end_stone(5), Vec::new()).is_empty());
        let mut cells = on_end_stone(1);
        cells.insert(POS.offset(0, 1, 0), BlockKind::Stone.default_state());
        assert!(run(&cells, Vec::new()).is_empty());
    }

    #[test]
    fn a_flower_floating_over_air_still_grows() {
        let cells = world(&[(POS, flower(0))]);
        assert_eq!(run(&cells, Vec::new()).len(), 2);
    }

    #[test]
    fn a_tall_stem_may_refuse_to_grow_higher() {
        let mut cells = world(&[(POS, flower(0))]);
        cells.insert(POS.offset(0, -1, 0), BlockKind::ChorusPlant.default_state());
        cells.insert(POS.offset(0, -2, 0), BlockKind::ChorusPlant.default_state());
        cells.insert(POS.offset(0, -3, 0), BlockKind::EndStone.default_state());
        let refused = run(&cells, vec![0, 0, 0]);
        assert!(
            refused
                .iter()
                .all(|change| change.pos != POS.offset(0, 1, 0))
        );
        assert_eq!(refused[1].pos, POS.offset(0, 0, -1));
        let allowed = run(&cells, vec![2]);
        assert_eq!(allowed[1].pos, POS.offset(0, 1, 0));
    }

    #[test]
    fn the_stem_connects_to_the_end_stone_below_and_to_neighbouring_stems() {
        let cells = world(&[
            (POS.offset(0, -1, 0), BlockKind::EndStone.default_state()),
            (POS.offset(1, 0, 0), BlockKind::ChorusPlant.default_state()),
        ]);
        let block = |pos: BlockPos| cells.get(&pos).copied().unwrap_or(BlockStateId::AIR);
        let state = plant_state(&block, POS);
        assert_eq!(state.property("down"), Some("true"));
        assert_eq!(state.property("east"), Some("true"));
        assert_eq!(state.property("west"), Some("false"));
    }
}
