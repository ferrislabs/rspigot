use crate::{BlockPos, BlockStateId};
use rspigot_blocks::{BlockKind, Face};

use super::reactions::{BLOCK_ENTITIES, DESTROYED, IMMOVABLE, PUSH_ONLY};
use crate::spawn::block_within_border;

pub const PUSH_LIMIT: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reaction {
    Normal,
    Destroy,
    Immovable,
    PushOnly,
}

pub fn reaction_of(state: BlockStateId) -> Reaction {
    let Some(kind) = state.kind() else {
        return Reaction::Normal;
    };
    let name = kind.name();
    if DESTROYED.binary_search(&name).is_ok() {
        Reaction::Destroy
    } else if IMMOVABLE.binary_search(&name).is_ok() {
        Reaction::Immovable
    } else if PUSH_ONLY.binary_search(&name).is_ok() {
        Reaction::PushOnly
    } else {
        Reaction::Normal
    }
}

fn has_block_entity(state: BlockStateId) -> bool {
    state
        .kind()
        .is_some_and(|kind| BLOCK_ENTITIES.binary_search(&kind.name()).is_ok())
}

fn is_piston(state: BlockStateId) -> bool {
    matches!(
        state.kind(),
        Some(BlockKind::Piston | BlockKind::StickyPiston)
    )
}

pub fn is_sticky(state: BlockStateId) -> bool {
    matches!(
        state.kind(),
        Some(BlockKind::SlimeBlock | BlockKind::HoneyBlock)
    )
}

pub fn can_stick_to_each_other(one: BlockStateId, other: BlockStateId) -> bool {
    match (one.kind(), other.kind()) {
        (Some(BlockKind::HoneyBlock), Some(BlockKind::SlimeBlock))
        | (Some(BlockKind::SlimeBlock), Some(BlockKind::HoneyBlock)) => false,
        _ => is_sticky(one) || is_sticky(other),
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub min_y: i32,
    pub max_y: i32,
}

pub fn is_pushable(
    state: BlockStateId,
    limits: Limits,
    pos: BlockPos,
    moving: Face,
    destroy: bool,
    piston_facing: Face,
) -> bool {
    if pos.y < limits.min_y || pos.y > limits.max_y || !block_within_border(pos) {
        return false;
    }
    if state.is_air() {
        return true;
    }
    if matches!(
        state.kind(),
        Some(
            BlockKind::Obsidian
                | BlockKind::CryingObsidian
                | BlockKind::RespawnAnchor
                | BlockKind::ReinforcedDeepslate
        )
    ) {
        return false;
    }
    if moving == Face::Down && pos.y == limits.min_y {
        return false;
    }
    if moving == Face::Up && pos.y == limits.max_y {
        return false;
    }
    if is_piston(state) {
        if state.property("extended") == Some("true") {
            return false;
        }
    } else {
        match reaction_of(state) {
            Reaction::Immovable => return false,
            Reaction::Destroy => return destroy,
            Reaction::PushOnly => return moving == piston_facing,
            Reaction::Normal => {}
        }
    }
    !has_block_entity(state)
}

pub struct Resolver<'a, F: Fn(BlockPos) -> BlockStateId> {
    block: &'a F,
    limits: Limits,
    piston: BlockPos,
    extending: bool,
    facing: Face,
    push: Face,
    start: BlockPos,
    pub to_push: Vec<BlockPos>,
    pub to_destroy: Vec<BlockPos>,
}

impl<'a, F: Fn(BlockPos) -> BlockStateId> Resolver<'a, F> {
    pub fn new(
        block: &'a F,
        limits: Limits,
        piston: BlockPos,
        direction: Face,
        extending: bool,
    ) -> Self {
        let (push, start) = if extending {
            (direction, step(piston, direction))
        } else {
            (
                direction.opposite(),
                step(step(piston, direction), direction),
            )
        };
        Self {
            block,
            limits,
            piston,
            extending,
            facing: direction,
            push,
            start,
            to_push: Vec::new(),
            to_destroy: Vec::new(),
        }
    }

    fn state(&self, pos: BlockPos) -> BlockStateId {
        (self.block)(pos)
    }

    fn pushable(&self, state: BlockStateId, pos: BlockPos, destroy: bool, facing: Face) -> bool {
        is_pushable(state, self.limits, pos, self.push, destroy, facing)
    }

    pub fn resolve(&mut self) -> bool {
        self.to_push.clear();
        self.to_destroy.clear();
        let state = self.state(self.start);
        if !self.pushable(state, self.start, false, self.facing) {
            if self.extending && reaction_of(state) == Reaction::Destroy && !state.is_air() {
                self.to_destroy.push(self.start);
                return true;
            }
            return false;
        }
        if !self.add_block_line(self.start, self.push) {
            return false;
        }
        let mut index = 0;
        while index < self.to_push.len() {
            let pos = self.to_push[index];
            if is_sticky(self.state(pos)) && !self.add_branching(pos) {
                return false;
            }
            index += 1;
        }
        true
    }

    fn add_block_line(&mut self, start: BlockPos, direction: Face) -> bool {
        let mut state = self.state(start);
        if state.is_air()
            || !self.pushable(state, start, false, direction)
            || start == self.piston
            || self.to_push.contains(&start)
        {
            return true;
        }
        let back = self.push.opposite();
        let mut count = 1;
        if count + self.to_push.len() > PUSH_LIMIT {
            return false;
        }
        while is_sticky(state) {
            let pos = offset(start, back, count);
            let previous = state;
            state = self.state(pos);
            if state.is_air()
                || !can_stick_to_each_other(previous, state)
                || !self.pushable(state, pos, false, back)
                || pos == self.piston
            {
                break;
            }
            count += 1;
            if count + self.to_push.len() > PUSH_LIMIT {
                return false;
            }
        }
        let mut added = 0;
        for back_step in (0..count).rev() {
            self.to_push.push(offset(start, back, back_step));
            added += 1;
        }
        let mut ahead = 1;
        loop {
            let pos = offset(start, self.push, ahead);
            if let Some(index) = self.to_push.iter().position(|pushed| *pushed == pos) {
                self.reorder_at_collision(added, index);
                for at in 0..=(index + added) {
                    let pushed = self.to_push[at];
                    if is_sticky(self.state(pushed)) && !self.add_branching(pushed) {
                        return false;
                    }
                }
                return true;
            }
            let state = self.state(pos);
            if state.is_air() {
                return true;
            }
            if !self.pushable(state, pos, true, self.push) || pos == self.piston {
                return false;
            }
            if reaction_of(state) == Reaction::Destroy {
                self.to_destroy.push(pos);
                return true;
            }
            if self.to_push.len() >= PUSH_LIMIT {
                return false;
            }
            self.to_push.push(pos);
            added += 1;
            ahead += 1;
        }
    }

    fn reorder_at_collision(&mut self, added: usize, collision: usize) {
        let len = self.to_push.len();
        let mut reordered = Vec::with_capacity(len);
        reordered.extend_from_slice(&self.to_push[..collision]);
        reordered.extend_from_slice(&self.to_push[len - added..]);
        reordered.extend_from_slice(&self.to_push[collision..len - added]);
        self.to_push = reordered;
    }

    fn add_branching(&mut self, from: BlockPos) -> bool {
        let state = self.state(from);
        for face in Face::ALL {
            if axis(face) == axis(self.push) {
                continue;
            }
            let pos = step(from, face);
            if can_stick_to_each_other(self.state(pos), state) && !self.add_block_line(pos, face) {
                return false;
            }
        }
        true
    }
}

fn step(pos: BlockPos, face: Face) -> BlockPos {
    let (dx, dy, dz) = face.offset();
    pos.offset(dx, dy, dz)
}

fn axis(face: Face) -> u8 {
    match face {
        Face::Down | Face::Up => 1,
        Face::North | Face::South => 2,
        Face::West | Face::East => 0,
    }
}

fn offset(pos: BlockPos, face: Face, count: usize) -> BlockPos {
    let (dx, dy, dz) = face.offset();
    let count = count as i32;
    pos.offset(dx * count, dy * count, dz * count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustc_hash::FxHashMap;

    const LIMITS: Limits = Limits {
        min_y: -64,
        max_y: 319,
    };
    const ORIGIN: BlockPos = BlockPos::new(0, 64, 0);

    fn at(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    fn world(blocks: &[(BlockPos, BlockStateId)]) -> impl Fn(BlockPos) -> BlockStateId {
        let map: FxHashMap<BlockPos, BlockStateId> = blocks.iter().copied().collect();
        move |pos| map.get(&pos).copied().unwrap_or(BlockStateId::AIR)
    }

    fn kinds(blocks: &[(BlockPos, BlockKind)]) -> Vec<(BlockPos, BlockStateId)> {
        blocks
            .iter()
            .map(|(pos, kind)| (*pos, kind.default_state()))
            .collect()
    }

    fn line(count: i32, kind: BlockKind) -> Vec<(BlockPos, BlockKind)> {
        (1..=count).map(|x| (at(x, 64, 0), kind)).collect()
    }

    struct Outcome {
        ok: bool,
        pushed: Vec<BlockPos>,
        destroyed: Vec<BlockPos>,
    }

    fn resolve_states(
        piston: BlockPos,
        blocks: &[(BlockPos, BlockStateId)],
        direction: Face,
        extending: bool,
    ) -> Outcome {
        let lookup = world(blocks);
        let mut resolver = Resolver::new(&lookup, LIMITS, piston, direction, extending);
        let ok = resolver.resolve();
        Outcome {
            ok,
            pushed: resolver.to_push,
            destroyed: resolver.to_destroy,
        }
    }

    fn resolve(blocks: &[(BlockPos, BlockKind)], direction: Face, extending: bool) -> Outcome {
        resolve_states(ORIGIN, &kinds(blocks), direction, extending)
    }

    fn extend(blocks: &[(BlockPos, BlockKind)]) -> Outcome {
        resolve(blocks, Face::East, true)
    }

    fn retract(blocks: &[(BlockPos, BlockKind)]) -> Outcome {
        resolve(blocks, Face::East, false)
    }

    fn sorted(mut positions: Vec<BlockPos>) -> Vec<BlockPos> {
        positions.sort_by_key(|pos| (pos.x, pos.y, pos.z));
        positions
    }

    fn pushable_east(kind: BlockKind) -> bool {
        is_pushable(
            kind.default_state(),
            LIMITS,
            at(1, 64, 0),
            Face::East,
            false,
            Face::East,
        )
    }

    #[test]
    fn an_empty_space_in_front_resolves_to_nothing() {
        let outcome = extend(&[]);
        assert!(outcome.ok);
        assert!(outcome.pushed.is_empty());
        assert!(outcome.destroyed.is_empty());
    }

    #[test]
    fn a_single_block_is_pushed() {
        let outcome = extend(&[(at(1, 64, 0), BlockKind::Stone)]);
        assert!(outcome.ok);
        assert_eq!(outcome.pushed, vec![at(1, 64, 0)]);
    }

    #[test]
    fn a_line_keeps_its_order_from_the_piston_outwards() {
        let outcome = extend(&line(4, BlockKind::Stone));
        assert!(outcome.ok);
        assert_eq!(
            outcome.pushed,
            vec![at(1, 64, 0), at(2, 64, 0), at(3, 64, 0), at(4, 64, 0)]
        );
    }

    #[test]
    fn a_piston_pushes_a_line_of_twelve_blocks() {
        let outcome = extend(&line(12, BlockKind::Stone));
        assert!(outcome.ok);
        assert_eq!(outcome.pushed.len(), PUSH_LIMIT);
    }

    #[test]
    fn a_line_of_thirteen_blocks_is_too_heavy() {
        assert!(!extend(&line(13, BlockKind::Stone)).ok);
    }

    #[test]
    fn a_gap_in_the_line_ends_it() {
        let mut blocks = line(3, BlockKind::Stone);
        blocks.extend(line(20, BlockKind::Stone).into_iter().skip(4));
        let outcome = extend(&blocks);
        assert!(outcome.ok);
        assert_eq!(outcome.pushed.len(), 3);
    }

    #[test]
    fn twelve_blocks_then_a_breakable_block_still_resolves() {
        let mut blocks = line(12, BlockKind::Stone);
        blocks.push((at(13, 64, 0), BlockKind::Torch));
        let outcome = extend(&blocks);
        assert!(outcome.ok);
        assert_eq!(outcome.pushed.len(), 12);
        assert_eq!(outcome.destroyed, vec![at(13, 64, 0)]);
    }

    #[test]
    fn obsidian_blocks_the_push() {
        let mut blocks = line(2, BlockKind::Stone);
        blocks.push((at(3, 64, 0), BlockKind::Obsidian));
        assert!(!extend(&blocks).ok);
    }

    #[test]
    fn an_immovable_block_right_in_front_blocks_the_push() {
        for kind in [
            BlockKind::Bedrock,
            BlockKind::Barrier,
            BlockKind::CommandBlock,
            BlockKind::EndPortalFrame,
            BlockKind::Lodestone,
            BlockKind::ReinforcedDeepslate,
            BlockKind::RespawnAnchor,
            BlockKind::CryingObsidian,
            BlockKind::MovingPiston,
            BlockKind::PistonHead,
        ] {
            assert!(!extend(&[(at(1, 64, 0), kind)]).ok, "{kind:?}");
        }
    }

    #[test]
    fn a_block_entity_cannot_be_pushed() {
        for kind in [
            BlockKind::Chest,
            BlockKind::Furnace,
            BlockKind::Hopper,
            BlockKind::Dispenser,
            BlockKind::Beacon,
            BlockKind::EnderChest,
            BlockKind::Jukebox,
            BlockKind::Spawner,
            BlockKind::EnchantingTable,
        ] {
            assert!(!extend(&[(at(1, 64, 0), kind)]).ok, "{kind:?}");
        }
    }

    #[test]
    fn a_destroyable_block_ahead_is_broken_not_pushed() {
        let outcome = extend(&[
            (at(1, 64, 0), BlockKind::Stone),
            (at(2, 64, 0), BlockKind::Torch),
        ]);
        assert!(outcome.ok);
        assert_eq!(outcome.pushed, vec![at(1, 64, 0)]);
        assert_eq!(outcome.destroyed, vec![at(2, 64, 0)]);
    }

    #[test]
    fn a_destroyable_block_right_in_front_is_broken_with_nothing_pushed() {
        let outcome = extend(&[(at(1, 64, 0), BlockKind::Torch)]);
        assert!(outcome.ok);
        assert!(outcome.pushed.is_empty());
        assert_eq!(outcome.destroyed, vec![at(1, 64, 0)]);
    }

    #[test]
    fn a_retraction_never_breaks_what_is_in_front() {
        assert!(!retract(&[(at(2, 64, 0), BlockKind::Torch)]).ok);
    }

    #[test]
    fn a_retraction_with_nothing_to_pull_resolves_empty() {
        let outcome = retract(&[]);
        assert!(outcome.ok);
        assert!(outcome.pushed.is_empty());
    }

    #[test]
    fn a_retracting_sticky_piston_pulls_the_block_in_front_of_its_head() {
        let outcome = retract(&[(at(2, 64, 0), BlockKind::Stone)]);
        assert!(outcome.ok);
        assert_eq!(outcome.pushed, vec![at(2, 64, 0)]);
    }

    #[test]
    fn a_retraction_cannot_pull_an_immovable_block() {
        assert!(!retract(&[(at(2, 64, 0), BlockKind::Obsidian)]).ok);
    }

    #[test]
    fn a_retraction_pulls_a_slime_block_and_what_sticks_behind_it() {
        let outcome = retract(&[
            (at(2, 64, 0), BlockKind::SlimeBlock),
            (at(3, 64, 0), BlockKind::Stone),
        ]);
        assert!(outcome.ok);
        assert_eq!(outcome.pushed, vec![at(3, 64, 0), at(2, 64, 0)]);
    }

    #[test]
    fn a_retraction_pulls_the_blocks_stuck_beside_slime() {
        let outcome = retract(&[
            (at(2, 64, 0), BlockKind::SlimeBlock),
            (at(2, 65, 0), BlockKind::Stone),
        ]);
        assert!(outcome.ok);
        assert_eq!(sorted(outcome.pushed), vec![at(2, 64, 0), at(2, 65, 0)]);
    }

    #[test]
    fn slime_drags_the_blocks_beside_it() {
        let outcome = extend(&[
            (at(1, 64, 0), BlockKind::SlimeBlock),
            (at(1, 65, 0), BlockKind::Stone),
        ]);
        assert!(outcome.ok);
        assert_eq!(outcome.pushed.len(), 2);
        assert!(outcome.pushed.contains(&at(1, 65, 0)));
    }

    #[test]
    fn slime_drags_blocks_on_every_side_but_the_push_axis() {
        let blocks = [
            (at(1, 64, 0), BlockKind::SlimeBlock),
            (at(1, 65, 0), BlockKind::Stone),
            (at(1, 63, 0), BlockKind::Stone),
            (at(1, 64, 1), BlockKind::Stone),
            (at(1, 64, -1), BlockKind::Stone),
        ];
        let outcome = extend(&blocks);
        assert!(outcome.ok);
        assert_eq!(outcome.pushed.len(), 5);
    }

    #[test]
    fn slime_drags_a_chain_of_slime_and_the_stone_hanging_from_it() {
        let outcome = extend(&[
            (at(1, 64, 0), BlockKind::SlimeBlock),
            (at(1, 65, 0), BlockKind::SlimeBlock),
            (at(1, 66, 0), BlockKind::Stone),
        ]);
        assert!(outcome.ok);
        assert_eq!(outcome.pushed.len(), 3);
    }

    #[test]
    fn a_plain_block_does_not_drag_its_neighbours() {
        let outcome = extend(&[
            (at(1, 64, 0), BlockKind::Stone),
            (at(1, 65, 0), BlockKind::Stone),
        ]);
        assert_eq!(outcome.pushed, vec![at(1, 64, 0)]);
    }

    #[test]
    fn honey_drags_the_blocks_beside_it() {
        let outcome = extend(&[
            (at(1, 64, 0), BlockKind::HoneyBlock),
            (at(1, 65, 0), BlockKind::Stone),
        ]);
        assert!(outcome.ok);
        assert_eq!(outcome.pushed.len(), 2);
    }

    #[test]
    fn honey_and_slime_do_not_stick_together() {
        let outcome = extend(&[
            (at(1, 64, 0), BlockKind::SlimeBlock),
            (at(1, 65, 0), BlockKind::HoneyBlock),
        ]);
        assert!(outcome.ok);
        assert_eq!(outcome.pushed, vec![at(1, 64, 0)]);
    }

    #[test]
    fn honey_beside_slime_stays_when_a_stone_sits_on_the_honey() {
        let outcome = extend(&[
            (at(1, 64, 0), BlockKind::SlimeBlock),
            (at(1, 65, 0), BlockKind::HoneyBlock),
            (at(1, 66, 0), BlockKind::Stone),
        ]);
        assert_eq!(outcome.pushed, vec![at(1, 64, 0)]);
    }

    #[test]
    fn a_slime_wall_of_twelve_moves_and_thirteen_does_not() {
        let wall = |count: i32| -> Vec<(BlockPos, BlockKind)> {
            (0..count)
                .map(|index| (at(1, 64 + index / 4, index % 4), BlockKind::SlimeBlock))
                .collect()
        };
        assert!(extend(&wall(12)).ok);
        assert!(!extend(&wall(13)).ok);
    }

    #[test]
    fn a_branch_that_hits_an_immovable_block_blocks_everything() {
        let outcome = extend(&[
            (at(1, 64, 0), BlockKind::SlimeBlock),
            (at(1, 65, 0), BlockKind::SlimeBlock),
            (at(2, 65, 0), BlockKind::Obsidian),
        ]);
        assert!(!outcome.ok);
    }

    #[test]
    fn a_branch_pushing_a_block_into_the_pistons_own_line_resolves_without_duplicates() {
        let outcome = extend(&[
            (at(1, 64, 0), BlockKind::SlimeBlock),
            (at(1, 65, 0), BlockKind::SlimeBlock),
            (at(2, 65, 0), BlockKind::Stone),
            (at(2, 64, 0), BlockKind::Stone),
        ]);
        assert!(outcome.ok);
        let unique = sorted(outcome.pushed.clone());
        let mut deduped = unique.clone();
        deduped.dedup();
        assert_eq!(unique, deduped);
        assert_eq!(unique.len(), 4);
    }

    #[test]
    fn a_block_ahead_of_a_branch_that_is_already_queued_is_reordered_not_repeated() {
        let outcome = extend(&[
            (at(1, 64, 0), BlockKind::Stone),
            (at(2, 64, 0), BlockKind::SlimeBlock),
            (at(2, 65, 0), BlockKind::Stone),
            (at(3, 65, 0), BlockKind::Stone),
            (at(3, 64, 0), BlockKind::Stone),
        ]);
        assert!(outcome.ok);
        let mut seen = outcome.pushed.clone();
        seen.sort_by_key(|pos| (pos.x, pos.y, pos.z));
        seen.dedup();
        assert_eq!(seen.len(), outcome.pushed.len());
        assert_eq!(seen.len(), 5);
    }

    #[test]
    fn the_block_a_pushed_block_moves_into_is_listed_after_it() {
        let outcome = extend(&[
            (at(1, 64, 0), BlockKind::SlimeBlock),
            (at(1, 65, 0), BlockKind::Stone),
            (at(2, 65, 0), BlockKind::Stone),
            (at(2, 64, 0), BlockKind::Stone),
        ]);
        assert!(outcome.ok);
        for (index, pos) in outcome.pushed.iter().enumerate() {
            let ahead = at(pos.x + 1, pos.y, pos.z);
            if let Some(ahead_index) = outcome.pushed.iter().position(|other| *other == ahead) {
                assert!(ahead_index > index, "{pos:?} moves into {ahead:?}");
            }
        }
    }

    #[test]
    fn a_retracted_piston_is_pushed_like_a_block_and_an_extended_one_is_not() {
        let retracted = BlockKind::Piston
            .default_state()
            .with_property("extended", "false")
            .expect("extended");
        let extended = retracted
            .with_property("extended", "true")
            .expect("extended");
        assert!(extend_with(retracted).ok);
        assert!(!extend_with(extended).ok);
    }

    fn extend_with(state: BlockStateId) -> Outcome {
        resolve_states(ORIGIN, &[(at(1, 64, 0), state)], Face::East, true)
    }

    #[test]
    fn a_sticky_piston_follows_the_same_rule() {
        let extended = BlockKind::StickyPiston
            .default_state()
            .with_property("extended", "true")
            .expect("extended");
        assert!(!extend_with(extended).ok);
        assert!(extend_with(BlockKind::StickyPiston.default_state()).ok);
    }

    #[test]
    fn glazed_terracotta_only_moves_along_the_piston_axis() {
        assert!(extend(&[(at(1, 64, 0), BlockKind::WhiteGlazedTerracotta)]).ok);
        let state = BlockKind::WhiteGlazedTerracotta.default_state();
        assert!(!is_pushable(
            state,
            LIMITS,
            at(1, 64, 0),
            Face::Up,
            false,
            Face::East
        ));
        assert!(!is_pushable(
            state,
            LIMITS,
            at(1, 64, 0),
            Face::South,
            false,
            Face::East
        ));
        assert!(is_pushable(
            state,
            LIMITS,
            at(1, 64, 0),
            Face::East,
            false,
            Face::East
        ));
    }

    #[test]
    fn slime_does_not_drag_glazed_terracotta_sideways() {
        let outcome = extend(&[
            (at(1, 64, 0), BlockKind::SlimeBlock),
            (at(1, 65, 0), BlockKind::WhiteGlazedTerracotta),
        ]);
        assert!(outcome.ok);
        assert_eq!(outcome.pushed, vec![at(1, 64, 0)]);
    }

    #[test]
    fn a_push_only_block_is_pushed_along_the_axis_even_when_it_follows_a_line() {
        let outcome = extend(&[
            (at(1, 64, 0), BlockKind::Stone),
            (at(2, 64, 0), BlockKind::WhiteGlazedTerracotta),
        ]);
        assert!(outcome.ok);
        assert_eq!(outcome.pushed.len(), 2);
    }

    #[test]
    fn destroyable_blocks_are_only_destroyed_when_the_caller_allows_it() {
        let torch = BlockKind::Torch.default_state();
        assert!(!is_pushable(
            torch,
            LIMITS,
            at(1, 64, 0),
            Face::East,
            false,
            Face::East
        ));
        assert!(is_pushable(
            torch,
            LIMITS,
            at(1, 64, 0),
            Face::East,
            true,
            Face::East
        ));
    }

    #[test]
    fn air_is_always_pushable_inside_the_world() {
        assert!(pushable_east(BlockKind::Air));
    }

    #[test]
    fn chests_and_a_bedrock_floor_cannot_be_pushed() {
        assert!(!pushable_east(BlockKind::Chest));
        let stone = BlockKind::Stone.default_state();
        assert!(!is_pushable(
            stone,
            LIMITS,
            at(1, -64, 0),
            Face::Down,
            false,
            Face::Down
        ));
        assert!(is_pushable(
            stone,
            LIMITS,
            at(1, -64, 0),
            Face::Up,
            false,
            Face::Up
        ));
    }

    #[test]
    fn nothing_is_pushed_above_the_build_limit() {
        let outcome = resolve_states(
            at(0, 318, 0),
            &[(at(0, 319, 0), BlockKind::Stone.default_state())],
            Face::Up,
            true,
        );
        assert!(!outcome.ok);
    }

    #[test]
    fn nothing_is_pushed_below_the_world_floor() {
        let outcome = resolve_states(
            at(0, -63, 0),
            &[(at(0, -64, 0), BlockKind::Stone.default_state())],
            Face::Down,
            true,
        );
        assert!(!outcome.ok);
    }

    #[test]
    fn a_block_may_move_into_the_last_layer_of_the_world() {
        let outcome = resolve_states(
            at(0, 317, 0),
            &[(at(0, 318, 0), BlockKind::Stone.default_state())],
            Face::Up,
            true,
        );
        assert!(outcome.ok);
        assert_eq!(outcome.pushed, vec![at(0, 318, 0)]);
    }

    #[test]
    fn nothing_is_pushed_outside_the_world_border() {
        let edge = 29_999_984;
        let outcome = resolve_states(
            at(edge - 1, 64, 0),
            &[(at(edge, 64, 0), BlockKind::Stone.default_state())],
            Face::East,
            true,
        );
        assert!(!outcome.ok);
        let inside = resolve_states(
            at(edge - 2, 64, 0),
            &[(at(edge - 1, 64, 0), BlockKind::Stone.default_state())],
            Face::East,
            true,
        );
        assert!(inside.ok);
    }

    #[test]
    fn the_resolver_works_in_every_direction() {
        for direction in Face::ALL {
            let (dx, dy, dz) = direction.offset();
            let front = ORIGIN.offset(dx, dy, dz);
            let behind = front.offset(dx, dy, dz);
            let outcome = resolve_states(
                ORIGIN,
                &[
                    (front, BlockKind::Stone.default_state()),
                    (behind, BlockKind::Dirt.default_state()),
                ],
                direction,
                true,
            );
            assert!(outcome.ok, "{direction:?}");
            assert_eq!(outcome.pushed, vec![front, behind], "{direction:?}");
        }
    }

    #[test]
    fn sticky_blocks_are_slime_and_honey_only() {
        assert!(is_sticky(BlockKind::SlimeBlock.default_state()));
        assert!(is_sticky(BlockKind::HoneyBlock.default_state()));
        assert!(!is_sticky(BlockKind::Stone.default_state()));
        assert!(!is_sticky(BlockKind::Air.default_state()));
    }

    #[test]
    fn stickiness_pairs_follow_the_vanilla_table() {
        let slime = BlockKind::SlimeBlock.default_state();
        let honey = BlockKind::HoneyBlock.default_state();
        let stone = BlockKind::Stone.default_state();
        assert!(can_stick_to_each_other(slime, stone));
        assert!(can_stick_to_each_other(stone, slime));
        assert!(can_stick_to_each_other(honey, stone));
        assert!(can_stick_to_each_other(slime, slime));
        assert!(can_stick_to_each_other(honey, honey));
        assert!(!can_stick_to_each_other(slime, honey));
        assert!(!can_stick_to_each_other(honey, slime));
        assert!(!can_stick_to_each_other(stone, stone));
    }

    #[test]
    fn reactions_classify_the_reference_blocks() {
        assert_eq!(
            reaction_of(BlockKind::Stone.default_state()),
            Reaction::Normal
        );
        assert_eq!(
            reaction_of(BlockKind::Air.default_state()),
            Reaction::Normal
        );
        assert_eq!(
            reaction_of(BlockKind::Torch.default_state()),
            Reaction::Destroy
        );
        assert_eq!(
            reaction_of(BlockKind::Obsidian.default_state()),
            Reaction::Immovable
        );
        assert_eq!(
            reaction_of(BlockKind::BlackGlazedTerracotta.default_state()),
            Reaction::PushOnly
        );
    }

    #[test]
    fn every_unbreakable_block_is_immovable() {
        for index in 0..BlockKind::ALL.len() {
            let kind = BlockKind::ALL[index];
            if kind.settings().hardness < 0.0 {
                assert_eq!(
                    reaction_of(kind.default_state()),
                    Reaction::Immovable,
                    "{kind:?}"
                );
            }
        }
    }

    #[test]
    fn the_reaction_tables_name_real_blocks_and_are_sorted_and_disjoint() {
        for table in [DESTROYED, IMMOVABLE, PUSH_ONLY, BLOCK_ENTITIES] {
            assert!(table.windows(2).all(|pair| pair[0] < pair[1]));
            for name in table {
                assert!(BlockKind::from_name(name).is_some(), "{name}");
            }
        }
        for name in DESTROYED {
            assert!(IMMOVABLE.binary_search(name).is_err(), "{name}");
            assert!(PUSH_ONLY.binary_search(name).is_err(), "{name}");
        }
        for name in IMMOVABLE {
            assert!(PUSH_ONLY.binary_search(name).is_err(), "{name}");
        }
    }

    #[test]
    fn a_block_with_a_block_entity_is_never_normal_pushable() {
        for name in BLOCK_ENTITIES {
            let kind = BlockKind::from_name(name).expect("known block");
            if matches!(kind, BlockKind::Piston | BlockKind::StickyPiston) {
                continue;
            }
            assert!(
                !is_pushable(
                    kind.default_state(),
                    LIMITS,
                    at(1, 64, 0),
                    Face::East,
                    true,
                    Face::East
                ) || reaction_of(kind.default_state()) == Reaction::Destroy,
                "{name}"
            );
        }
    }
}
