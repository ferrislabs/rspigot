use serde_json::Value;

use super::place::{Run, max_abs};
use super::prov::{BlockSet, Int, Pos, Pred, State};

const FACES: [(i32, i32, i32); 6] = [
    (0, -1, 0),
    (0, 1, 0),
    (0, 0, -1),
    (0, 0, 1),
    (-1, 0, 0),
    (1, 0, 0),
];

#[derive(Debug, Clone)]
pub(crate) struct Spread {
    block: State,
    accepted: BlockSet,
    can_replace: Pred,
    attempts: Int,
    xz: Int,
    y: Int,
}

impl Spread {
    pub(crate) fn compile(value: &Value) -> Self {
        let provider = |key: &str| Int::compile(value.get(key).unwrap_or(&Value::Null));
        Self {
            block: State::compile(value.get("block").unwrap_or(&Value::Null)),
            accepted: BlockSet::compile(value.get("accepted_neighbors").unwrap_or(&Value::Null)),
            can_replace: Pred::compile(value.get("can_replace").unwrap_or(&Value::Null)),
            attempts: provider("attempts"),
            xz: provider("xz_offset"),
            y: provider("y_offset"),
        }
    }

    pub(crate) fn reach(&self) -> i32 {
        max_abs(&self.xz)
    }

    fn put(&self, run: &mut Run, at: Pos) {
        if let Some(state) = self.block.resolve(run.region, run.rng, at) {
            run.region.set(at.0, at.1, at.2, state);
        }
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        self.put(run, origin);
        let attempts = self.attempts.sample(run.rng);
        for _ in 0..attempts {
            let dx = self.xz.sample(run.rng);
            let dy = self.y.sample(run.rng);
            let dz = self.xz.sample(run.rng);
            let at = (origin.0 + dx, origin.1 + dy, origin.2 + dz);
            if !self.can_replace.test(run.region, at) {
                continue;
            }
            let mut neighbours = 0;
            for (fx, fy, fz) in FACES {
                if self
                    .accepted
                    .contains(run.region.get(at.0 + fx, at.1 + fy, at.2 + fz))
                {
                    neighbours += 1;
                }
                if neighbours > 1 {
                    break;
                }
            }
            if neighbours == 1 {
                self.put(run, at);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests;
