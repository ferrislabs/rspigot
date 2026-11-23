use rspigot_blocks::{BlockKind, Face, StateFlags};

use crate::random::RandomSource;
use crate::{BlockPos, BlockStateId};

pub const GROWTH_CHANCE: f32 = 0.011_377_778;
pub const SULFUR_MAX_GROWTH_LENGTH: i32 = 2;
pub const STALAGMITE_SEARCH: i32 = 10;

const SPIKE: BlockKind = BlockKind::SulfurSpike;

fn step(pos: BlockPos, face: Face) -> BlockPos {
    let (dx, dy, dz) = face.offset();
    pos.offset(dx, dy, dz)
}

fn is_spike(state: BlockStateId) -> bool {
    state.kind() == Some(SPIKE)
}

fn is_tip(state: BlockStateId) -> bool {
    is_spike(state) && state.is_speleothem_tip(false)
}

fn is_unmerged_tip(state: BlockStateId, direction: Face) -> bool {
    is_tip(state) && state.is_speleothem_toward(direction)
}

fn has_fluid(state: BlockStateId) -> bool {
    state.flags().has_fluid()
}

fn has_water(state: BlockStateId) -> bool {
    state.flags().contains(StateFlags::WATER)
}

fn sturdy(state: BlockStateId) -> bool {
    let flags = state.flags();
    flags.contains(StateFlags::SOLID_RENDER) || flags.contains(StateFlags::FULL_COLLISION_CUBE)
}

pub fn sulfur_spike_state(direction: Face, thickness: &str, waterlogged: bool) -> BlockStateId {
    let vertical = if direction == Face::Up { "up" } else { "down" };
    SPIKE
        .state_with(&[
            ("vertical_direction", vertical),
            ("thickness", thickness),
            ("waterlogged", if waterlogged { "true" } else { "false" }),
        ])
        .unwrap_or(BlockStateId::AIR)
}

pub fn is_stalactite_start(state: BlockStateId, above: BlockStateId) -> bool {
    is_spike(state) && state.is_speleothem_toward(Face::Down) && !is_spike(above)
}

fn is_free_hanging(state: BlockStateId) -> bool {
    is_spike(state)
        && state.is_speleothem_toward(Face::Down)
        && state.property("thickness") == Some("tip")
        && state.property("waterlogged") != Some("true")
}

fn find_tip(
    block: &impl Fn(BlockPos) -> BlockStateId,
    pos: BlockPos,
    state: BlockStateId,
    max: i32,
) -> Option<BlockPos> {
    if is_tip(state) {
        return Some(pos);
    }
    let direction = state.tip_direction()?;
    let mut at = pos;
    for _ in 0..max {
        let current = block(at);
        if !is_spike(current) || current.tip_direction() != Some(direction) {
            return None;
        }
        if is_tip(current) {
            return Some(at);
        }
        at = step(at, direction);
    }
    None
}

fn can_tip_grow(
    block: &impl Fn(BlockPos) -> BlockStateId,
    tip: BlockPos,
    state: BlockStateId,
) -> bool {
    let Some(direction) = state.tip_direction() else {
        return false;
    };
    let target = block(step(tip, direction));
    if has_fluid(target) {
        return false;
    }
    target.is_air() || is_unmerged_tip(target, direction.opposite())
}

fn grow(
    block: &impl Fn(BlockPos) -> BlockStateId,
    tip: BlockPos,
    direction: Face,
) -> Vec<(BlockPos, BlockStateId)> {
    let target = step(tip, direction);
    let existing = block(target);
    if is_unmerged_tip(existing, direction.opposite()) {
        let (stalactite, stalagmite) = if direction == Face::Up {
            (target, tip)
        } else {
            (tip, target)
        };
        return vec![
            (
                stalactite,
                sulfur_spike_state(Face::Down, "tip_merge", false),
            ),
            (stalagmite, sulfur_spike_state(Face::Up, "tip_merge", false)),
        ];
    }
    if existing.is_air() || existing.kind() == Some(BlockKind::Water) {
        return vec![(
            target,
            sulfur_spike_state(direction, "tip", has_water(existing)),
        )];
    }
    Vec::new()
}

fn valid_placement(block: &impl Fn(BlockPos) -> BlockStateId, pos: BlockPos) -> bool {
    let support = block(step(pos, Face::Down));
    sturdy(support) || (is_spike(support) && support.is_speleothem_toward(Face::Up))
}

fn grow_stalagmite(
    block: &impl Fn(BlockPos) -> BlockStateId,
    tip: BlockPos,
) -> Vec<(BlockPos, BlockStateId)> {
    let mut at = tip;
    for _ in 0..STALAGMITE_SEARCH {
        at = step(at, Face::Down);
        let state = block(at);
        if has_fluid(state) {
            return Vec::new();
        }
        if is_unmerged_tip(state, Face::Up) && can_tip_grow(block, at, state) {
            return grow(block, at, Face::Up);
        }
        let below = step(at, Face::Down);
        if valid_placement(block, at) && !has_water(block(below)) {
            return grow(block, below, Face::Up);
        }
    }
    Vec::new()
}

pub fn plan_sulfur_growth(
    block: &impl Fn(BlockPos) -> BlockStateId,
    pos: BlockPos,
    state: BlockStateId,
    random: &mut impl RandomSource,
) -> Vec<(BlockPos, BlockStateId)> {
    if block(step(pos, Face::Up)).kind() != Some(BlockKind::Sulfur) {
        return Vec::new();
    }
    let Some(tip) = find_tip(block, pos, state, SULFUR_MAX_GROWTH_LENGTH) else {
        return Vec::new();
    };
    let tip_state = block(tip);
    if !is_free_hanging(tip_state) || !can_tip_grow(block, tip, tip_state) {
        return Vec::new();
    }
    if random.next_boolean() {
        grow(block, tip, Face::Down)
    } else {
        grow_stalagmite(block, tip)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rspigot_blocks::{BlockKind, Face};

    use super::*;
    use crate::random::RandomSource;
    use crate::{BlockPos, BlockStateId};

    struct Script(Vec<bool>);

    impl RandomSource for Script {
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
            self.0.remove(0)
        }
        fn next_gaussian(&mut self) -> f64 {
            0.0
        }
    }

    const START: BlockPos = BlockPos::new(0, 70, 0);

    fn spike(direction: Face, thickness: &str) -> BlockStateId {
        sulfur_spike_state(direction, thickness, false)
    }

    fn world() -> HashMap<BlockPos, BlockStateId> {
        [
            (START.offset(0, 1, 0), BlockKind::Sulfur.default_state()),
            (START, spike(Face::Down, "tip")),
        ]
        .into_iter()
        .collect()
    }

    fn lookup(map: &HashMap<BlockPos, BlockStateId>) -> impl Fn(BlockPos) -> BlockStateId + '_ {
        |pos| map.get(&pos).copied().unwrap_or(BlockStateId::AIR)
    }

    #[test]
    fn a_spike_hanging_from_sulfur_grows_one_block_down() {
        let w = world();
        let block = lookup(&w);
        let grown = plan_sulfur_growth(&block, START, block(START), &mut Script(vec![true]));
        assert_eq!(
            grown,
            vec![(START.offset(0, -1, 0), spike(Face::Down, "tip"))]
        );
    }

    #[test]
    fn it_only_grows_from_a_sulfur_block_not_potent_sulfur_or_stone() {
        for other in [
            BlockKind::PotentSulfur,
            BlockKind::Stone,
            BlockKind::DripstoneBlock,
        ] {
            let mut w = world();
            w.insert(START.offset(0, 1, 0), other.default_state());
            let block = lookup(&w);
            assert!(
                plan_sulfur_growth(&block, START, block(START), &mut Script(vec![true])).is_empty(),
                "{other:?}"
            );
        }
    }

    #[test]
    fn it_needs_no_water_above_unlike_dripstone() {
        let w = world();
        let block = lookup(&w);
        assert!(
            !plan_sulfur_growth(&block, START, block(START), &mut Script(vec![true])).is_empty()
        );
    }

    #[test]
    fn only_the_top_spike_of_a_column_is_a_start() {
        let mut w = world();
        w.insert(START.offset(0, 1, 0), spike(Face::Down, "base"));
        let block = lookup(&w);
        assert!(
            plan_sulfur_growth(&block, START, block(START), &mut Script(vec![true])).is_empty()
        );
    }

    #[test]
    fn a_column_stops_growing_at_two_blocks() {
        let mut w = world();
        w.insert(START, spike(Face::Down, "base"));
        w.insert(START.offset(0, -1, 0), spike(Face::Down, "tip"));
        let block = lookup(&w);
        let grown = plan_sulfur_growth(&block, START, block(START), &mut Script(vec![true]));
        assert!(!grown.is_empty());
        let mut long = world();
        long.insert(START, spike(Face::Down, "base"));
        long.insert(START.offset(0, -1, 0), spike(Face::Down, "middle"));
        long.insert(START.offset(0, -2, 0), spike(Face::Down, "tip"));
        let block = lookup(&long);
        assert!(
            plan_sulfur_growth(&block, START, block(START), &mut Script(vec![true])).is_empty()
        );
    }

    #[test]
    fn a_waterlogged_tip_does_not_grow() {
        let mut w = world();
        w.insert(START, sulfur_spike_state(Face::Down, "tip", true));
        let block = lookup(&w);
        assert!(
            plan_sulfur_growth(&block, START, block(START), &mut Script(vec![true])).is_empty()
        );
    }

    #[test]
    fn the_other_coin_face_grows_a_stalagmite_from_the_floor() {
        let mut w = world();
        let floor = START.offset(0, -4, 0);
        w.insert(floor, BlockKind::Stone.default_state());
        let block = lookup(&w);
        let grown = plan_sulfur_growth(&block, START, block(START), &mut Script(vec![false]));
        assert_eq!(grown, vec![(floor.offset(0, 1, 0), spike(Face::Up, "tip"))]);
    }

    #[test]
    fn a_stalactite_meets_a_sulfur_stalagmite_and_both_merge() {
        let mut w = world();
        let below = START.offset(0, -1, 0);
        w.insert(below, spike(Face::Up, "tip"));
        let block = lookup(&w);
        let grown = plan_sulfur_growth(&block, START, block(START), &mut Script(vec![true]));
        assert_eq!(
            grown,
            vec![
                (START, spike(Face::Down, "tip_merge")),
                (below, spike(Face::Up, "tip_merge")),
            ]
        );
    }

    #[test]
    fn a_dripstone_tip_below_is_not_merged_with_a_sulfur_spike() {
        let mut w = world();
        let below = START.offset(0, -1, 0);
        w.insert(
            below,
            BlockKind::PointedDripstone
                .state_with(&[("vertical_direction", "up"), ("thickness", "tip")])
                .unwrap(),
        );
        let block = lookup(&w);
        let grown = plan_sulfur_growth(&block, START, block(START), &mut Script(vec![true]));
        assert!(grown.is_empty());
    }

    #[test]
    fn the_stalagmite_scan_goes_through_any_block_but_fluid_for_ten_steps() {
        let mut w = world();
        for dy in 2..=4 {
            w.insert(START.offset(0, -dy, 0), BlockKind::Cobweb.default_state());
        }
        w.insert(START.offset(0, -6, 0), BlockKind::Stone.default_state());
        let block = lookup(&w);
        let grown = plan_sulfur_growth(&block, START, block(START), &mut Script(vec![false]));
        assert_eq!(
            grown,
            vec![(START.offset(0, -5, 0), spike(Face::Up, "tip"))]
        );
    }
}
