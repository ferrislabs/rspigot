use rspigot_blocks::BlockKind;

use crate::random::RandomSource;
use crate::{BlockPos, BlockStateId};

pub const BASE_HEIGHT: i32 = 4;
pub const HEIGHT_RAND_A: i32 = 2;
pub const MIN_HEIGHT_FOR_LEAVES: i32 = 3;
pub const BEND_LENGTH_MIN: i32 = 1;
pub const BEND_LENGTH_MAX: i32 = 2;
pub const LEAF_ATTEMPTS: i32 = 50;
pub const LEAF_RADIUS: i32 = 3;
pub const FOLIAGE_HEIGHT: i32 = 2;
pub const BONE_MEAL_SUCCESS: f32 = 0.45;
const AZALEA_LEAVES_WEIGHT: i32 = 3;
const FLOWERING_LEAVES_WEIGHT: i32 = 1;
const MAX_LEAF_DISTANCE: i32 = 7;
const HORIZONTAL: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tree {
    pub blocks: Vec<(BlockPos, BlockStateId)>,
}

fn log() -> BlockStateId {
    BlockKind::OakLog
        .state_with(&[("axis", "y")])
        .unwrap_or_else(|| BlockKind::OakLog.default_state())
}

fn leaves(kind: BlockKind, distance: i32) -> BlockStateId {
    kind.state_with(&[
        ("distance", &distance.to_string()),
        ("persistent", "false"),
        ("waterlogged", "false"),
    ])
    .unwrap_or_else(|| kind.default_state())
}

fn leaf_distances(logs: &[BlockPos], leaf_cells: &[BlockPos]) -> Vec<i32> {
    let mut distances = vec![MAX_LEAF_DISTANCE; leaf_cells.len()];
    let faces = [
        (0, 1, 0),
        (0, -1, 0),
        (1, 0, 0),
        (-1, 0, 0),
        (0, 0, 1),
        (0, 0, -1),
    ];
    for _ in 0..MAX_LEAF_DISTANCE {
        for index in 0..leaf_cells.len() {
            let mut best = distances[index];
            for (dx, dy, dz) in faces {
                let neighbour = leaf_cells[index].offset(dx, dy, dz);
                if logs.contains(&neighbour) {
                    best = best.min(1);
                } else if let Some(other) = leaf_cells.iter().position(|cell| *cell == neighbour) {
                    best = best.min(distances[other] + 1);
                }
            }
            distances[index] = best;
        }
    }
    distances
}

pub fn plan(
    origin: BlockPos,
    max_y: i32,
    free: &impl Fn(BlockPos) -> bool,
    dirt_below: bool,
    random: &mut impl RandomSource,
) -> Option<Tree> {
    let height = BASE_HEIGHT + random.next_int(HEIGHT_RAND_A + 1) + random.next_int(1);
    if origin.y + height + 1 >= max_y {
        return None;
    }
    if (1..=height).any(|dy| !free(origin.offset(0, dy, 0))) {
        return None;
    }
    let (dx, dz) = HORIZONTAL[random.next_int(4) as usize];
    let last = height - 1;
    let mut logs: Vec<BlockPos> = Vec::new();
    let mut attachments: Vec<BlockPos> = Vec::new();
    let mut at = origin;
    for step in 0..=last {
        if step + 1 >= last + random.next_int(2) {
            at = at.offset(dx, 0, dz);
        }
        if at == origin || free(at) {
            logs.push(at);
        }
        if step >= MIN_HEIGHT_FOR_LEAVES {
            attachments.push(at);
        }
        at = at.offset(0, 1, 0);
    }
    let bend = BEND_LENGTH_MIN + random.next_int(BEND_LENGTH_MAX - BEND_LENGTH_MIN + 1);
    for _ in 0..=bend {
        if free(at) {
            logs.push(at);
        }
        attachments.push(at);
        at = at.offset(dx, 0, dz);
    }
    let mut leaf_cells: Vec<(BlockPos, BlockKind)> = Vec::new();
    for attachment in attachments {
        for _ in 0..LEAF_ATTEMPTS {
            let x = random.next_int(LEAF_RADIUS) - random.next_int(LEAF_RADIUS);
            let y = random.next_int(FOLIAGE_HEIGHT) - random.next_int(FOLIAGE_HEIGHT);
            let z = random.next_int(LEAF_RADIUS) - random.next_int(LEAF_RADIUS);
            let spot = attachment.offset(x, y, z);
            if !free(spot) || logs.contains(&spot) {
                continue;
            }
            let pick = random.next_int(AZALEA_LEAVES_WEIGHT + FLOWERING_LEAVES_WEIGHT);
            let kind = if pick < AZALEA_LEAVES_WEIGHT {
                BlockKind::AzaleaLeaves
            } else {
                BlockKind::FloweringAzaleaLeaves
            };
            match leaf_cells.iter_mut().find(|(cell, _)| *cell == spot) {
                Some(existing) => existing.1 = kind,
                None => leaf_cells.push((spot, kind)),
            }
        }
    }
    let cells: Vec<BlockPos> = leaf_cells.iter().map(|(cell, _)| *cell).collect();
    let distances = leaf_distances(&logs, &cells);
    let mut blocks: Vec<(BlockPos, BlockStateId)> = Vec::new();
    if !dirt_below {
        blocks.push((
            origin.offset(0, -1, 0),
            BlockKind::RootedDirt.default_state(),
        ));
    }
    blocks.extend(logs.iter().map(|pos| (*pos, log())));
    blocks.extend(
        leaf_cells
            .iter()
            .zip(distances)
            .map(|((pos, kind), distance)| (*pos, leaves(*kind, distance))),
    );
    Some(Tree { blocks })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Zero;

    impl RandomSource for Zero {
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

    struct Lcg(u64);

    impl RandomSource for Lcg {
        fn next_float(&mut self) -> f32 {
            0.0
        }
        fn next_int(&mut self, bound: i32) -> i32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((self.0 >> 33) % bound as u64) as i32
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

    const ORIGIN: BlockPos = BlockPos::new(0, 64, 0);

    fn count(tree: &Tree, kind: BlockKind) -> usize {
        tree.blocks
            .iter()
            .filter(|(_, state)| state.kind() == Some(kind))
            .count()
    }

    #[test]
    fn a_tree_has_a_bent_oak_trunk_rooted_in_dirt_and_azalea_leaves() {
        let tree = plan(ORIGIN, 320, &|_| true, false, &mut Lcg(5)).unwrap();
        assert_eq!(count(&tree, BlockKind::RootedDirt), 1);
        assert!(count(&tree, BlockKind::OakLog) >= 5);
        assert!(count(&tree, BlockKind::AzaleaLeaves) > 0);
        assert!(
            tree.blocks
                .iter()
                .any(|(pos, state)| *pos == ORIGIN && state.kind() == Some(BlockKind::OakLog))
        );
    }

    #[test]
    fn a_dirt_floor_is_left_alone() {
        let tree = plan(ORIGIN, 320, &|_| true, true, &mut Zero).unwrap();
        assert_eq!(count(&tree, BlockKind::RootedDirt), 0);
    }

    #[test]
    fn the_trunk_climbs_four_blocks_then_bends_sideways() {
        let tree = plan(ORIGIN, 320, &|_| true, true, &mut Zero).unwrap();
        let logs: Vec<BlockPos> = tree
            .blocks
            .iter()
            .filter(|(_, state)| state.kind() == Some(BlockKind::OakLog))
            .map(|(pos, _)| *pos)
            .collect();
        assert!(logs.contains(&ORIGIN.offset(0, 1, 0)));
        assert!(logs.iter().any(|pos| pos.z < 0));
        assert!(logs.iter().all(|pos| pos.y >= 64));
    }

    #[test]
    fn leaves_hold_a_distance_to_the_logs() {
        let tree = plan(ORIGIN, 320, &|_| true, true, &mut Lcg(9)).unwrap();
        assert!(count(&tree, BlockKind::AzaleaLeaves) > 0);
        for (_, state) in &tree.blocks {
            if state.kind() == Some(BlockKind::AzaleaLeaves) {
                let distance: i32 = state.property("distance").unwrap().parse().unwrap();
                assert!((1..=7).contains(&distance));
            }
        }
    }

    #[test]
    fn a_blocked_trunk_or_the_world_ceiling_cancels_the_tree() {
        let blocked = |pos: BlockPos| pos != ORIGIN.offset(0, 2, 0);
        assert!(plan(ORIGIN, 320, &blocked, true, &mut Zero).is_none());
        assert!(plan(ORIGIN, 66, &|_| true, true, &mut Zero).is_none());
    }
}
