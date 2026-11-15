use rspigot_blocks::{Face, Support};
use serde_json::Value;

use super::bodies::Compiler;
use super::place::{Run, max_abs};
use super::prov::{Int, Pos, State};
use super::tree::replaceable_by_trees;
use super::treedeco::{self, Context, Decorator};

const HORIZONTAL: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

#[derive(Debug, Clone)]
pub(crate) struct Fallen {
    log: State,
    length: Int,
    stump_decorators: Vec<Decorator>,
    log_decorators: Vec<Decorator>,
}

fn open(run: &mut Run, (x, y, z): Pos) -> bool {
    replaceable_by_trees(run.region.get(x, y, z))
}

fn over_solid_ground(run: &mut Run, (x, y, z): Pos) -> bool {
    run.region
        .get(x, y - 1, z)
        .is_face_sturdy(Face::Up, Support::Full)
}

fn may_place_on(run: &mut Run, pos: Pos) -> bool {
    open(run, pos) && over_solid_ground(run, pos)
}

impl Fallen {
    pub(crate) fn compile(value: &Value, compiler: &mut Compiler) -> Self {
        Self {
            log: State::compile(value.get("trunk_provider").unwrap_or(&Value::Null)),
            length: Int::compile(value.get("log_length").unwrap_or(&Value::Null)),
            stump_decorators: treedeco::compile_list(value.get("stump_decorators"), compiler),
            log_decorators: treedeco::compile_list(value.get("log_decorators"), compiler),
        }
    }

    pub(crate) fn reach(&self) -> i32 {
        max_abs(&self.length) + 1
    }

    fn put_log(&self, run: &mut Run, pos: Pos, axis: Option<&str>) {
        let Some(mut state) = self.log.resolve(run.region, run.rng, pos) else {
            return;
        };
        if let Some(axis) = axis {
            state = state.with_property("axis", axis).unwrap_or(state);
        }
        run.region.set(pos.0, pos.1, pos.2, state);
    }

    fn decorate(run: &mut Run, logs: &[Pos], decorators: &[Decorator]) {
        if decorators.is_empty() {
            return;
        }
        let mut context = Context::new(run, logs, &[], &[]);
        treedeco::run(decorators, &mut context);
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        self.put_log(run, origin, None);
        Self::decorate(run, &[origin], &self.stump_decorators);
        let (dx, dz) = HORIZONTAL[run.rng.below(4) as usize];
        let length = self.length.sample(run.rng) - 2;
        let offset = 2 + run.rng.below(2);
        let mut start = (origin.0 + dx * offset, origin.1 + 1, origin.2 + dz * offset);
        for _ in 0..6 {
            if may_place_on(run, start) {
                break;
            }
            start.1 -= 1;
        }
        let mut at = start;
        let mut unsupported = 0;
        for _ in 0..length {
            if !open(run, at) {
                return true;
            }
            if over_solid_ground(run, at) {
                unsupported = 0;
            } else {
                unsupported += 1;
                if unsupported > 2 {
                    return true;
                }
            }
            at = (at.0 + dx, at.1, at.2 + dz);
        }
        let axis = if dx == 0 { "z" } else { "x" };
        let mut logs = Vec::new();
        let mut at = start;
        for _ in 0..length {
            self.put_log(run, at, Some(axis));
            logs.push(at);
            at = (at.0 + dx, at.1, at.2 + dz);
        }
        Self::decorate(run, &logs, &self.log_decorators);
        true
    }
}

#[cfg(test)]
mod tests;
