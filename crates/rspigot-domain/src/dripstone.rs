use rspigot_blocks::{BlockKind, Face, StateFlags};

use crate::random::RandomSource;
use crate::{BlockPos, BlockStateId};

pub const GROWTH_CHANCE: f32 = 0.011_377_778;
pub const WATER_DRIP_CHANCE: f32 = 0.175_781_25;
pub const LAVA_DRIP_CHANCE: f32 = 0.058_593_75;
pub const DRIP_SEARCH: i32 = 11;
pub const GROWTH_SEARCH: i32 = 7;
pub const STALAGMITE_SEARCH: i32 = 10;
pub const DRIP_BASE_DELAY: u64 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fluid {
    Water,
    Lava,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drip {
    pub fluid: Fluid,
    pub source: BlockPos,
    pub tip: BlockPos,
}

fn step(pos: BlockPos, face: Face) -> BlockPos {
    let (dx, dy, dz) = face.offset();
    pos.offset(dx, dy, dz)
}

fn is_pointed(state: BlockStateId) -> bool {
    state.kind() == Some(BlockKind::PointedDripstone)
}

pub fn is_stalactite(state: BlockStateId) -> bool {
    is_pointed(state) && state.is_speleothem_toward(Face::Down)
}

pub fn is_stalactite_start(state: BlockStateId, above: BlockStateId) -> bool {
    is_stalactite(state) && !is_pointed(above)
}

fn is_unmerged_tip(state: BlockStateId, direction: Face) -> bool {
    is_pointed(state)
        && state.is_speleothem_toward(direction)
        && state.property("thickness") == Some("tip")
}

fn is_tip(state: BlockStateId) -> bool {
    is_pointed(state) && state.is_speleothem_tip(false)
}

fn has_water(state: BlockStateId) -> bool {
    state.flags().contains(StateFlags::WATER)
}

fn is_water_source(state: BlockStateId) -> bool {
    match state.kind() {
        Some(BlockKind::Water) => state.property("level") == Some("0"),
        _ => false,
    }
}

fn sturdy(state: BlockStateId) -> bool {
    let flags = state.flags();
    flags.contains(StateFlags::SOLID_RENDER) || flags.contains(StateFlags::FULL_COLLISION_CUBE)
}

pub fn tip_state(direction: Face, thickness: &str, waterlogged: bool) -> BlockStateId {
    let vertical = if direction == Face::Up { "up" } else { "down" };
    BlockKind::PointedDripstone
        .state_with(&[
            ("vertical_direction", vertical),
            ("thickness", thickness),
            ("waterlogged", if waterlogged { "true" } else { "false" }),
        ])
        .unwrap_or(BlockStateId::AIR)
}

pub fn find_tip(
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
        if !is_pointed(current) || current.tip_direction() != Some(direction) {
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
    if target.flags().has_fluid() {
        return false;
    }
    target.is_air() || is_unmerged_tip(target, direction.opposite())
}

fn can_drip(state: BlockStateId) -> bool {
    is_unmerged_tip(state, Face::Down) && state.property("waterlogged") != Some("true")
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
            (stalactite, tip_state(Face::Down, "tip_merge", false)),
            (stalagmite, tip_state(Face::Up, "tip_merge", false)),
        ];
    }
    if existing.is_air() || existing.kind() == Some(BlockKind::Water) {
        return vec![(target, tip_state(direction, "tip", has_water(existing)))];
    }
    Vec::new()
}

fn drips_through(state: BlockStateId) -> bool {
    state.is_air()
        || (!sturdy(state)
            && !state.flags().has_fluid()
            && !state.flags().contains(StateFlags::HAS_COLLISION))
}

fn grow_stalagmite(
    block: &impl Fn(BlockPos) -> BlockStateId,
    tip: BlockPos,
) -> Vec<(BlockPos, BlockStateId)> {
    let mut at = tip;
    for _ in 0..STALAGMITE_SEARCH {
        at = step(at, Face::Down);
        let state = block(at);
        if state.flags().has_fluid() {
            return Vec::new();
        }
        if is_unmerged_tip(state, Face::Up) && can_tip_grow(block, at, state) {
            return grow(block, at, Face::Up);
        }
        let support_pos = step(at, Face::Down);
        let support = block(support_pos);
        let supported =
            sturdy(support) || support.is_speleothem_toward(Face::Up) && is_pointed(support);
        if supported && !has_water(support) {
            return grow(block, support_pos, Face::Up);
        }
        if !drips_through(state) {
            return Vec::new();
        }
    }
    Vec::new()
}

pub fn plan_growth(
    block: &impl Fn(BlockPos) -> BlockStateId,
    pos: BlockPos,
    state: BlockStateId,
    random: &mut impl RandomSource,
) -> Vec<(BlockPos, BlockStateId)> {
    let support = block(step(pos, Face::Up));
    let above = block(pos.offset(0, 2, 0));
    if support.kind() != Some(BlockKind::DripstoneBlock) || !is_water_source(above) {
        return Vec::new();
    }
    let Some(tip) = find_tip(block, pos, state, GROWTH_SEARCH) else {
        return Vec::new();
    };
    let tip_state = block(tip);
    if !can_drip(tip_state) || !can_tip_grow(block, tip, tip_state) {
        return Vec::new();
    }
    if random.next_boolean() {
        grow(block, tip, Face::Down)
    } else {
        grow_stalagmite(block, tip)
    }
}

pub fn find_drip(
    block: &impl Fn(BlockPos) -> BlockStateId,
    pos: BlockPos,
    state: BlockStateId,
) -> Option<Drip> {
    if !is_stalactite(state) {
        return None;
    }
    let mut root = pos;
    let mut found = false;
    for _ in 0..DRIP_SEARCH {
        if !is_pointed(block(root)) {
            found = true;
            break;
        }
        root = step(root, Face::Up);
    }
    if !found {
        return None;
    }
    let source = step(root, Face::Up);
    let above = block(source);
    let fluid = if above.kind() == Some(BlockKind::Mud) || has_water(above) {
        Fluid::Water
    } else if above.flags().contains(StateFlags::LAVA) {
        Fluid::Lava
    } else {
        return None;
    };
    let tip = find_tip(block, pos, state, DRIP_SEARCH)?;
    Some(Drip { fluid, source, tip })
}

pub fn drip_chance(fluid: Fluid) -> f32 {
    match fluid {
        Fluid::Water => WATER_DRIP_CHANCE,
        Fluid::Lava => LAVA_DRIP_CHANCE,
    }
}

pub fn cauldron_below(
    block: &impl Fn(BlockPos) -> BlockStateId,
    tip: BlockPos,
    accepts: impl Fn(BlockStateId) -> bool,
) -> Option<BlockPos> {
    let mut at = tip;
    for _ in 0..DRIP_SEARCH {
        at = step(at, Face::Down);
        let state = block(at);
        if accepts(state) {
            return Some(at);
        }
        if !drips_through(state) {
            return None;
        }
    }
    None
}

pub fn drip_delay(tip: BlockPos, cauldron: BlockPos) -> u64 {
    DRIP_BASE_DELAY + u64::try_from(tip.y - cauldron.y).unwrap_or(0)
}

pub fn find_stalactite_above(
    block: &impl Fn(BlockPos) -> BlockStateId,
    cauldron: BlockPos,
) -> Option<(BlockPos, BlockStateId)> {
    let mut at = cauldron;
    for _ in 0..DRIP_SEARCH {
        at = step(at, Face::Up);
        let state = block(at);
        if is_stalactite(state) {
            return Some((at, state));
        }
        if !drips_through(state) {
            return None;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

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

    fn cells(list: &[(BlockPos, BlockStateId)]) -> HashMap<BlockPos, BlockStateId> {
        list.iter().copied().collect()
    }

    fn lookup(map: &HashMap<BlockPos, BlockStateId>) -> impl Fn(BlockPos) -> BlockStateId + '_ {
        |pos| map.get(&pos).copied().unwrap_or(BlockStateId::AIR)
    }

    fn water_source() -> BlockStateId {
        BlockKind::Water.default_state()
    }

    fn stalactite_world() -> HashMap<BlockPos, BlockStateId> {
        cells(&[
            (START.offset(0, 2, 0), water_source()),
            (
                START.offset(0, 1, 0),
                BlockKind::DripstoneBlock.default_state(),
            ),
            (START, tip_state(Face::Down, "tip", false)),
        ])
    }

    #[test]
    fn a_stalactite_start_is_the_top_pointed_block() {
        let tip = tip_state(Face::Down, "tip", false);
        assert!(is_stalactite_start(tip, BlockStateId::AIR));
        assert!(!is_stalactite_start(tip, tip));
        assert!(!is_stalactite_start(
            tip_state(Face::Up, "tip", false),
            BlockStateId::AIR
        ));
    }

    #[test]
    fn a_stalactite_under_dripstone_and_water_grows_one_block_down() {
        let world = stalactite_world();
        let block = lookup(&world);
        let state = block(START);
        let grown = plan_growth(&block, START, state, &mut Script(vec![true]));
        assert_eq!(
            grown,
            vec![(START.offset(0, -1, 0), tip_state(Face::Down, "tip", false))]
        );
    }

    #[test]
    fn growth_needs_a_dripstone_block_with_water_above_it() {
        let mut world = stalactite_world();
        world.insert(START.offset(0, 1, 0), BlockKind::Stone.default_state());
        let block = lookup(&world);
        assert!(plan_growth(&block, START, block(START), &mut Script(vec![true])).is_empty());
        let mut world = stalactite_world();
        world.remove(&START.offset(0, 2, 0));
        let block = lookup(&world);
        assert!(plan_growth(&block, START, block(START), &mut Script(vec![true])).is_empty());
    }

    #[test]
    fn a_waterlogged_tip_does_not_grow() {
        let mut world = stalactite_world();
        world.insert(START, tip_state(Face::Down, "tip", true));
        let block = lookup(&world);
        assert!(plan_growth(&block, START, block(START), &mut Script(vec![true])).is_empty());
    }

    #[test]
    fn a_stalactite_meets_a_stalagmite_tip_and_both_merge() {
        let mut world = stalactite_world();
        let below = START.offset(0, -1, 0);
        world.insert(below, tip_state(Face::Up, "tip", false));
        let block = lookup(&world);
        let grown = plan_growth(&block, START, block(START), &mut Script(vec![true]));
        assert_eq!(
            grown,
            vec![
                (START, tip_state(Face::Down, "tip_merge", false)),
                (below, tip_state(Face::Up, "tip_merge", false)),
            ]
        );
    }

    #[test]
    fn the_other_coin_face_grows_a_stalagmite_from_the_floor() {
        let mut world = stalactite_world();
        let floor = START.offset(0, -4, 0);
        world.insert(floor, BlockKind::Stone.default_state());
        let block = lookup(&world);
        let grown = plan_growth(&block, START, block(START), &mut Script(vec![false]));
        assert_eq!(
            grown,
            vec![(floor.offset(0, 1, 0), tip_state(Face::Up, "tip", false))]
        );
    }

    #[test]
    fn a_stalagmite_tip_below_grows_upward_instead() {
        let mut world = stalactite_world();
        let stalagmite = START.offset(0, -3, 0);
        world.insert(stalagmite, tip_state(Face::Up, "tip", false));
        let block = lookup(&world);
        let grown = plan_growth(&block, START, block(START), &mut Script(vec![false]));
        assert_eq!(
            grown,
            vec![(
                stalagmite.offset(0, 1, 0),
                tip_state(Face::Up, "tip", false)
            )]
        );
    }

    #[test]
    fn water_above_the_root_drips_from_the_tip() {
        let mut world = cells(&[
            (START.offset(0, 2, 0), water_source()),
            (START.offset(0, 1, 0), BlockKind::Stone.default_state()),
            (START, tip_state(Face::Down, "base", false)),
            (START.offset(0, -1, 0), tip_state(Face::Down, "tip", false)),
        ]);
        let drip = {
            let block = lookup(&world);
            find_drip(&block, START, block(START)).unwrap()
        };
        assert_eq!(drip.fluid, Fluid::Water);
        assert_eq!(drip.tip, START.offset(0, -1, 0));
        world.insert(START.offset(0, 2, 0), BlockKind::Lava.default_state());
        let lava = {
            let block = lookup(&world);
            find_drip(&block, START, block(START)).unwrap().fluid
        };
        assert_eq!(lava, Fluid::Lava);
        world.remove(&START.offset(0, 2, 0));
        let block = lookup(&world);
        assert!(find_drip(&block, START, block(START)).is_none());
    }

    #[test]
    fn the_drip_falls_through_air_to_the_first_accepting_block() {
        let world = cells(&[(START.offset(0, -4, 0), BlockKind::Cauldron.default_state())]);
        let block = lookup(&world);
        let tip = START.offset(0, -1, 0);
        let found = cauldron_below(&block, tip, |state| {
            state.kind() == Some(BlockKind::Cauldron)
        });
        assert_eq!(found, Some(START.offset(0, -4, 0)));
        assert_eq!(drip_delay(tip, START.offset(0, -4, 0)), 53);
        let blocked = cells(&[
            (START.offset(0, -2, 0), BlockKind::Stone.default_state()),
            (START.offset(0, -4, 0), BlockKind::Cauldron.default_state()),
        ]);
        let block = lookup(&blocked);
        assert_eq!(
            cauldron_below(&block, tip, |state| state.kind()
                == Some(BlockKind::Cauldron)),
            None
        );
    }

    #[test]
    fn a_cauldron_finds_the_stalactite_dripping_into_it() {
        let world = cells(&[
            (START, tip_state(Face::Down, "tip", false)),
            (START.offset(0, -3, 0), BlockKind::Cauldron.default_state()),
        ]);
        let block = lookup(&world);
        assert_eq!(
            find_stalactite_above(&block, START.offset(0, -3, 0)).map(|(pos, _)| pos),
            Some(START)
        );
    }
}
