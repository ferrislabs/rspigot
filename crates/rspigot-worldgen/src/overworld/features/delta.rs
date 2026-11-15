use serde_json::Value;

use super::cells::within_manhattan;
use super::place::{Run, max_abs};
use super::prov::{Int, Pos};
use crate::datapack::block_state;
use rspigot_blocks::BlockKind as Kind;
use rspigot_domain::BlockStateId;

const CANNOT_REPLACE: [Kind; 7] = [
    Kind::Bedrock,
    Kind::NetherBricks,
    Kind::NetherBrickFence,
    Kind::NetherBrickStairs,
    Kind::NetherWart,
    Kind::Chest,
    Kind::Spawner,
];

const DIRECTIONS: [(i32, i32, i32); 6] = [
    (0, -1, 0),
    (0, 1, 0),
    (0, 0, -1),
    (0, 0, 1),
    (-1, 0, 0),
    (1, 0, 0),
];

#[derive(Debug, Clone)]
pub(crate) struct Delta {
    contents: BlockStateId,
    rim: BlockStateId,
    size: Int,
    rim_size: Int,
}

impl Delta {
    pub(crate) fn compile(value: &Value) -> Self {
        let state = |key: &str| {
            value
                .get(key)
                .and_then(block_state)
                .unwrap_or(BlockStateId::AIR)
        };
        Self {
            contents: state("contents"),
            rim: state("rim"),
            size: Int::compile(value.get("size").unwrap_or(&Value::Null)),
            rim_size: Int::compile(value.get("rim_size").unwrap_or(&Value::Null)),
        }
    }

    pub(crate) fn reach(&self) -> i32 {
        max_abs(&self.size) + max_abs(&self.rim_size)
    }

    fn clear(&self, run: &mut Run, (x, y, z): Pos) -> bool {
        let here = run.region.get(x, y, z);
        if here.kind() == self.contents.kind() {
            return false;
        }
        if here
            .kind()
            .is_some_and(|kind| CANNOT_REPLACE.contains(&kind))
        {
            return false;
        }
        for (dx, dy, dz) in DIRECTIONS {
            let air = run.region.get(x + dx, y + dy, z + dz).is_air();
            let up = dy == 1;
            if air && !up {
                return false;
            }
            if !air && up {
                return false;
            }
        }
        true
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        let rimmed = run.rng.next_double() < 0.9;
        let rim_x = if rimmed {
            self.rim_size.sample(run.rng)
        } else {
            0
        };
        let rim_z = if rimmed {
            self.rim_size.sample(run.rng)
        } else {
            0
        };
        let use_rim = rimmed && rim_x != 0 && rim_z != 0;
        let size_x = self.size.sample(run.rng);
        let size_z = self.size.sample(run.rng);
        let limit = size_x.max(size_z);
        let mut placed = false;
        for pos in within_manhattan(origin, (size_x, 0, size_z)) {
            let distance =
                (pos.0 - origin.0).abs() + (pos.1 - origin.1).abs() + (pos.2 - origin.2).abs();
            if distance > limit {
                break;
            }
            if !self.clear(run, pos) {
                continue;
            }
            if use_rim {
                placed = true;
                run.region.set(pos.0, pos.1, pos.2, self.rim);
            }
            let moved = (pos.0 + rim_x, pos.1, pos.2 + rim_z);
            if self.clear(run, moved) {
                placed = true;
                run.region.set(moved.0, moved.1, moved.2, self.contents);
            }
        }
        placed
    }
}

#[cfg(test)]
mod tests;
