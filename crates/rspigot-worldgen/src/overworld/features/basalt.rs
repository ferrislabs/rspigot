use std::sync::Arc;

use serde_json::Value;

use super::bodies::Compiler;
use super::place::{Placed, Run};
use super::prov::{BlockSet, Int, Pos, Pred, State};

fn direction(name: &str) -> Pos {
    match name {
        "up" => (0, 1, 0),
        _ => (0, -1, 0),
    }
}

fn offset((x, y, z): Pos, (dx, dy, dz): Pos) -> Pos {
    (x + dx, y + dy, z + dz)
}

fn predicate(value: &Value, key: &str) -> Pred {
    value.get(key).map_or_else(
        || Pred::Blocks(BlockSet::tag("minecraft:replaceable"), (0, 0, 0)),
        Pred::compile,
    )
}

#[derive(Debug, Clone)]
pub(crate) struct Pillar {
    block: State,
    can_replace: Pred,
    direction: Pos,
    chance: f32,
    cap: Option<Arc<Placed>>,
}

impl Pillar {
    pub(crate) fn compile(value: &Value, compiler: &mut Compiler) -> Self {
        Self {
            block: State::compile(value.get("block").unwrap_or(&Value::Null)),
            can_replace: predicate(value, "can_replace"),
            direction: direction(value.get("direction").and_then(Value::as_str).unwrap_or("")),
            chance: value
                .get("chance_to_continue")
                .and_then(Value::as_f64)
                .map_or(1.0, |n| n as f32),
            cap: value.get("cap_feature").map(|cap| compiler.placed_ref(cap)),
        }
    }

    pub(crate) fn reach(&self) -> i32 {
        self.cap.as_ref().map_or(1, |cap| cap.margin.max(1))
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        let mut at = origin;
        while self.can_replace.test(run.region, at)
            && run.rng.next_float() < self.chance
            && run.region.height().contains(at.1)
        {
            if let Some(state) = self.block.resolve(run.region, run.rng, at) {
                run.region.set(at.0, at.1, at.2, state);
            }
            at = offset(at, self.direction);
        }
        let back = (-self.direction.0, -self.direction.1, -self.direction.2);
        at = offset(at, back);
        if let Some(cap) = &self.cap {
            cap.place(run, at);
        }
        true
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Patchy {
    block: State,
    project_through: Pred,
    size: Int,
    max_projection: i32,
}

impl Patchy {
    pub(crate) fn compile(value: &Value) -> Self {
        Self {
            block: State::compile(value.get("block").unwrap_or(&Value::Null)),
            project_through: predicate(value, "project_through"),
            size: Int::compile(value.get("size").unwrap_or(&Value::Null)),
            max_projection: value
                .get("max_projection_height")
                .and_then(Value::as_i64)
                .map_or(0, |n| n as i32),
        }
    }

    pub(crate) fn reach(&self) -> i32 {
        self.size.max_inclusive().max(1) + self.max_projection
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        let size = self.size.sample(run.rng);
        let bound = size * size + 1;
        for dx in -size..=size {
            for dz in -size..=size {
                let weight = dx.abs() * dz.abs();
                if run.rng.below(bound) >= bound - weight {
                    continue;
                }
                let mut at = (origin.0 + dx, origin.1, origin.2 + dz);
                let mut remaining = self.max_projection;
                loop {
                    let below = (at.0, at.1 - 1, at.2);
                    if !self.project_through.test(run.region, below) {
                        break;
                    }
                    at = below;
                    remaining -= 1;
                    if remaining <= 0 {
                        break;
                    }
                }
                if let Some(state) = self.block.resolve(run.region, run.rng, at) {
                    run.region.set(at.0, at.1, at.2, state);
                }
            }
        }
        true
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Cluster {
    block: State,
    continue_through: Pred,
    can_replace: Pred,
    cannot_place_on: BlockSet,
    cluster_reach: Int,
    column_count: Int,
    column_reach: Int,
    height: Int,
}

impl Cluster {
    pub(crate) fn compile(value: &Value) -> Self {
        let int = |key: &str| Int::compile(value.get(key).unwrap_or(&Value::Null));
        Self {
            block: State::compile(value.get("block").unwrap_or(&Value::Null)),
            continue_through: predicate(value, "continue_through"),
            can_replace: predicate(value, "can_replace"),
            cannot_place_on: BlockSet::compile(
                value.get("cannot_place_on").unwrap_or(&Value::Null),
            ),
            cluster_reach: int("cluster_reach"),
            column_count: int("column_count"),
            column_reach: int("column_reach"),
            height: int("height"),
        }
    }

    pub(crate) fn reach(&self) -> i32 {
        self.cluster_reach.max_inclusive().max(0) + self.column_reach.max_inclusive().max(0) + 1
    }

    fn can_place_at(&self, run: &mut Run, at: Pos) -> bool {
        if !self.can_replace.test(run.region, at) {
            return false;
        }
        let below = run.region.get(at.0, at.1 - 1, at.2);
        !(below.is_air() || self.cannot_place_on.contains(below))
    }

    fn find_surface(&self, run: &mut Run, mut at: Pos, mut limit: i32) -> Option<Pos> {
        while at.1 > run.region.height().min_y + 1 && limit >= 1 {
            limit -= 1;
            if self.can_place_at(run, at) {
                return Some(at);
            }
            at.1 -= 1;
        }
        None
    }

    fn find_air(&self, run: &mut Run, mut at: Pos, mut limit: i32) -> Option<Pos> {
        while at.1 <= run.region.height().max_y() && limit >= 1 {
            limit -= 1;
            let state = run.region.get(at.0, at.1, at.2);
            if self.cannot_place_on.contains(state) {
                return None;
            }
            if state.is_air() {
                return Some(at);
            }
            at.1 += 1;
        }
        None
    }

    fn place_column(&self, run: &mut Run, origin: Pos, column_height: i32, reach: i32) -> bool {
        let mut placed = false;
        for z in origin.2 - reach..=origin.2 + reach {
            for x in origin.0 - reach..=origin.0 + reach {
                let at = (x, origin.1, z);
                let distance = (x - origin.0).abs() + (z - origin.2).abs();
                let found = if self.can_replace.test(run.region, at) {
                    self.find_surface(run, at, distance)
                } else {
                    self.find_air(run, at, distance)
                };
                let Some(mut cursor) = found else {
                    continue;
                };
                let mut steps = column_height - distance / 2;
                while steps >= 0 {
                    if self.can_replace.test(run.region, cursor) {
                        if let Some(state) = self.block.resolve(run.region, run.rng, cursor) {
                            run.region.set(cursor.0, cursor.1, cursor.2, state);
                        }
                        cursor.1 += 1;
                        placed = true;
                    } else if self.continue_through.test(run.region, cursor) {
                        cursor.1 += 1;
                    } else {
                        break;
                    }
                    steps -= 1;
                }
            }
        }
        placed
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        if !self.can_place_at(run, origin) {
            return false;
        }
        let height = self.height.sample(run.rng);
        let reach = height.min(self.cluster_reach.sample(run.rng));
        let count = self.column_count.sample(run.rng);
        let width = 2 * reach + 1;
        let mut placed = false;
        for _ in 0..count {
            let x = origin.0 - reach + run.rng.below(width);
            let y = origin.1 + run.rng.below(1);
            let z = origin.2 - reach + run.rng.below(width);
            let at = (x, y, z);
            let distance = (x - origin.0).abs() + (y - origin.1).abs() + (z - origin.2).abs();
            if height - distance >= 0 {
                let column_reach = self.column_reach.sample(run.rng);
                placed |= self.place_column(run, at, height - distance, column_reach);
            }
        }
        placed
    }
}
