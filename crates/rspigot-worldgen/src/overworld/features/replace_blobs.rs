use serde_json::Value;

use super::cells::within_manhattan;
use super::place::{Run, max_abs};
use super::prov::{Int, Pos};
use crate::datapack::block_state;
use rspigot_domain::BlockStateId;

#[derive(Debug, Clone)]
pub(crate) struct ReplaceBlobs {
    target: BlockStateId,
    state: BlockStateId,
    radius: Int,
}

impl ReplaceBlobs {
    pub(crate) fn compile(value: &Value) -> Self {
        Self {
            target: value
                .get("target")
                .and_then(block_state)
                .unwrap_or(BlockStateId::AIR),
            state: value
                .get("state")
                .and_then(block_state)
                .unwrap_or(BlockStateId::AIR),
            radius: Int::compile(value.get("radius").unwrap_or(&Value::Null)),
        }
    }

    pub(crate) fn reach(&self) -> i32 {
        max_abs(&self.radius)
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        let min_y = run.region.height().min_y;
        let max_y = run.region.height().max_y();
        let mut y = origin.1.clamp(min_y + 1, max_y);
        loop {
            if y <= min_y + 1 {
                return false;
            }
            if run.region.get(origin.0, y, origin.2).kind() == self.target.kind() {
                break;
            }
            y -= 1;
        }
        let at = (origin.0, y, origin.2);
        let rx = self.radius.sample(run.rng);
        let ry = self.radius.sample(run.rng);
        let rz = self.radius.sample(run.rng);
        let limit = rx.max(ry.max(rz));
        let mut placed = false;
        for pos in within_manhattan(at, (rx, ry, rz)) {
            let distance = (pos.0 - at.0).abs() + (pos.1 - at.1).abs() + (pos.2 - at.2).abs();
            if distance > limit {
                break;
            }
            if run.region.get(pos.0, pos.1, pos.2).kind() == self.target.kind() {
                run.region.set(pos.0, pos.1, pos.2, self.state);
                placed = true;
            }
        }
        placed
    }
}

#[cfg(test)]
mod tests;
