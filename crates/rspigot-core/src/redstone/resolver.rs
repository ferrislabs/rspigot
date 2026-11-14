use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::{BlockPos, BlockStateId};

use super::reactions::{BLOCK_ENTITIES, DESTROYED, IMMOVABLE, PUSH_ONLY};
use super::step;

pub(crate) const PUSH_LIMIT: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reaction {
    Normal,
    Destroy,
    Immovable,
    PushOnly,
}

pub(crate) fn reaction_of(state: BlockStateId) -> Reaction {
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

fn is_sticky(state: BlockStateId) -> bool {
    matches!(
        state.kind(),
        Some(BlockKind::SlimeBlock | BlockKind::HoneyBlock)
    )
}

fn can_stick_to_each_other(one: BlockStateId, other: BlockStateId) -> bool {
    match (one.kind(), other.kind()) {
        (Some(BlockKind::HoneyBlock), Some(BlockKind::SlimeBlock))
        | (Some(BlockKind::SlimeBlock), Some(BlockKind::HoneyBlock)) => false,
        _ => is_sticky(one) || is_sticky(other),
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Limits {
    pub(crate) min_y: i32,
    pub(crate) max_y: i32,
}

pub(crate) fn is_pushable(
    state: BlockStateId,
    limits: Limits,
    pos: BlockPos,
    moving: Face,
    destroy: bool,
    piston_facing: Face,
) -> bool {
    if pos.y < limits.min_y || pos.y > limits.max_y {
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

pub(crate) struct Resolver<'a, F: Fn(BlockPos) -> BlockStateId> {
    block: &'a F,
    limits: Limits,
    piston: BlockPos,
    extending: bool,
    facing: Face,
    push: Face,
    start: BlockPos,
    pub(crate) to_push: Vec<BlockPos>,
    pub(crate) to_destroy: Vec<BlockPos>,
}

impl<'a, F: Fn(BlockPos) -> BlockStateId> Resolver<'a, F> {
    pub(crate) fn new(
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

    pub(crate) fn resolve(&mut self) -> bool {
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

    fn world(blocks: &[(BlockPos, BlockKind)]) -> impl Fn(BlockPos) -> BlockStateId {
        let map: FxHashMap<BlockPos, BlockStateId> = blocks
            .iter()
            .map(|(pos, kind)| (*pos, kind.default_state()))
            .collect();
        move |pos| map.get(&pos).copied().unwrap_or(BlockStateId::AIR)
    }

    fn line(count: i32, kind: BlockKind) -> Vec<(BlockPos, BlockKind)> {
        (1..=count)
            .map(|x| (BlockPos::new(x, 64, 0), kind))
            .collect()
    }

    fn resolved(
        blocks: &[(BlockPos, BlockKind)],
        direction: Face,
        extending: bool,
    ) -> (bool, Vec<BlockPos>, Vec<BlockPos>) {
        let lookup = world(blocks);
        let mut resolver = Resolver::new(
            &lookup,
            LIMITS,
            BlockPos::new(0, 64, 0),
            direction,
            extending,
        );
        let ok = resolver.resolve();
        (ok, resolver.to_push, resolver.to_destroy)
    }

    #[test]
    fn a_piston_pushes_a_line_of_twelve_blocks() {
        let (ok, pushed, _) = resolved(&line(12, BlockKind::Stone), Face::East, true);
        assert!(ok);
        assert_eq!(pushed.len(), 12);
    }

    #[test]
    fn a_line_of_thirteen_blocks_is_too_heavy() {
        let (ok, _, _) = resolved(&line(13, BlockKind::Stone), Face::East, true);
        assert!(!ok);
    }

    #[test]
    fn obsidian_blocks_the_push() {
        let mut blocks = line(2, BlockKind::Stone);
        blocks.push((BlockPos::new(3, 64, 0), BlockKind::Obsidian));
        let (ok, _, _) = resolved(&blocks, Face::East, true);
        assert!(!ok);
    }

    #[test]
    fn a_destroyable_block_ahead_is_broken_not_pushed() {
        let blocks = vec![
            (BlockPos::new(1, 64, 0), BlockKind::Stone),
            (BlockPos::new(2, 64, 0), BlockKind::Torch),
        ];
        let (ok, pushed, destroyed) = resolved(&blocks, Face::East, true);
        assert!(ok);
        assert_eq!(pushed, vec![BlockPos::new(1, 64, 0)]);
        assert_eq!(destroyed, vec![BlockPos::new(2, 64, 0)]);
    }

    #[test]
    fn slime_drags_the_blocks_beside_it() {
        let blocks = vec![
            (BlockPos::new(1, 64, 0), BlockKind::SlimeBlock),
            (BlockPos::new(1, 65, 0), BlockKind::Stone),
        ];
        let (ok, pushed, _) = resolved(&blocks, Face::East, true);
        assert!(ok);
        assert!(pushed.contains(&BlockPos::new(1, 65, 0)));
        assert_eq!(pushed.len(), 2);
    }

    #[test]
    fn honey_and_slime_do_not_stick_together() {
        let blocks = vec![
            (BlockPos::new(1, 64, 0), BlockKind::SlimeBlock),
            (BlockPos::new(1, 65, 0), BlockKind::HoneyBlock),
        ];
        let (ok, pushed, _) = resolved(&blocks, Face::East, true);
        assert!(ok);
        assert_eq!(pushed, vec![BlockPos::new(1, 64, 0)]);
    }

    #[test]
    fn a_retracting_sticky_piston_pulls_the_block_in_front_of_its_head() {
        let blocks = vec![(BlockPos::new(2, 64, 0), BlockKind::Stone)];
        let (ok, pushed, _) = resolved(&blocks, Face::East, false);
        assert!(ok);
        assert_eq!(pushed, vec![BlockPos::new(2, 64, 0)]);
    }

    #[test]
    fn glazed_terracotta_only_moves_along_the_piston_axis() {
        let blocks = vec![(BlockPos::new(1, 64, 0), BlockKind::WhiteGlazedTerracotta)];
        assert!(resolved(&blocks, Face::East, true).0);
        let lookup = world(&blocks);
        let state = lookup(BlockPos::new(1, 64, 0));
        assert!(!is_pushable(
            state,
            LIMITS,
            BlockPos::new(1, 64, 0),
            Face::Up,
            false,
            Face::East
        ));
    }

    #[test]
    fn chests_and_a_bedrock_floor_cannot_be_pushed() {
        let chest = BlockKind::Chest.default_state();
        assert!(!is_pushable(
            chest,
            LIMITS,
            BlockPos::new(1, 64, 0),
            Face::East,
            false,
            Face::East
        ));
        let stone = BlockKind::Stone.default_state();
        assert!(!is_pushable(
            stone,
            LIMITS,
            BlockPos::new(1, -64, 0),
            Face::Down,
            false,
            Face::Down
        ));
    }
}
