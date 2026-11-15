use std::sync::Arc;

use rspigot_blocks::{BlockKind as Kind, Face, StateFlags, Support};
use rspigot_domain::BlockStateId;
use serde_json::Value;

use super::bodies::Feature;
use super::place::{Placed, Run};
use super::prov::{BlockSet, Pos, Pred, State, integer, is_lava, is_water};
use crate::overworld::region::Heightmap;

#[derive(Debug, Clone)]
pub(crate) struct Root {
    tree: Arc<Placed>,
    space: i32,
    level_distance: i32,
    level_deviation: i32,
    radius: i32,
    replaceable: BlockSet,
    state: State,
    attempts: i32,
    column: i32,
    hanging_radius: i32,
    hanging_span: i32,
    hanging_state: State,
    hanging_attempts: i32,
    water: i32,
    allowed: Pred,
}

const HORIZONTAL: [(i32, i32); 4] = [(0, 1), (-1, 0), (0, -1), (1, 0)];

impl Root {
    pub(crate) fn compile(value: &Value, tree: Arc<Placed>) -> Self {
        Self {
            tree,
            space: integer(value, "required_vertical_space_for_tree", 0),
            level_distance: integer(value, "level_test_distance", 0),
            level_deviation: integer(value, "max_level_deviation", 0),
            radius: integer(value, "root_radius", 1),
            replaceable: BlockSet::compile(value.get("root_replaceable").unwrap_or(&Value::Null)),
            state: State::compile(value.get("root_state_provider").unwrap_or(&Value::Null)),
            attempts: integer(value, "root_placement_attempts", 0),
            column: integer(value, "root_column_max_height", 0),
            hanging_radius: integer(value, "hanging_root_radius", 1),
            hanging_span: integer(value, "hanging_roots_vertical_span", 1),
            hanging_state: State::compile(
                value
                    .get("hanging_root_state_provider")
                    .unwrap_or(&Value::Null),
            ),
            hanging_attempts: integer(value, "hanging_root_placement_attempts", 0),
            water: integer(value, "allowed_vertical_water_for_tree", 0),
            allowed: Pred::compile(value.get("allowed_tree_position").unwrap_or(&Value::Null)),
        }
    }

    pub(crate) fn reach(&self) -> i32 {
        (self.radius - 1)
            .max(self.hanging_radius - 1)
            .max(self.tree.margin)
    }

    fn allowed_space(&self, state: BlockStateId, above: i32) -> bool {
        state.is_air() || (above < self.water && is_water(state))
    }

    fn space_for_tree(&self, run: &mut Run, pos: Pos) -> bool {
        for step in 1..=self.space {
            let state = run.region.get(pos.0, pos.1 + step, pos.2);
            if !self.allowed_space(state, step) {
                return false;
            }
        }
        if self.level_distance > 0 {
            let mut at = pos;
            for (dx, dz) in HORIZONTAL {
                at = (
                    at.0 + dx * self.level_distance,
                    at.1,
                    at.2 + dz * self.level_distance,
                );
                let below = run.region.get(at.0, at.1 - self.level_deviation, at.2);
                let above = run.region.get(at.0, at.1 + self.level_deviation, at.2);
                if below.is_air() || !above.is_air() {
                    return false;
                }
                at = pos;
            }
        }
        true
    }

    fn rooted_dirt(&self, run: &mut Run, x: i32, y: i32, z: i32) {
        let mut at = (x, y, z);
        for _ in 0..self.attempts {
            let a = run.rng.below(self.radius);
            let b = run.rng.below(self.radius);
            let c = run.rng.below(self.radius);
            let d = run.rng.below(self.radius);
            at = (at.0 + a - b, at.1, at.2 + c - d);
            let state = run.region.get(at.0, at.1, at.2);
            if self.replaceable.contains(state)
                && let Some(placed) = self.state.resolve(run.region, run.rng, at)
            {
                run.region.set(at.0, at.1, at.2, placed);
            }
            at = (x, at.1, z);
        }
    }

    fn place_dirt(&self, run: &mut Run, origin: Pos, target: i32) {
        for y in origin.1..target {
            self.rooted_dirt(run, origin.0, y, origin.2);
        }
    }

    fn dirt_and_tree(&self, run: &mut Run, origin: Pos) -> bool {
        let mut at = origin;
        for step in 0..self.column {
            at.1 += 1;
            if run.region.height_at(Heightmap::WorldSurface, at.0, at.2) < at.1 {
                return false;
            }
            if self.allowed.test(run.region, at) && self.space_for_tree(run, at) {
                let below = run.region.get(at.0, at.1 - 1, at.2);
                if is_lava(below) || !below.flags().contains(StateFlags::LEGACY_SOLID) {
                    return false;
                }
                if self.tree.place(run, at) {
                    self.place_dirt(run, origin, origin.1 + step);
                    return true;
                }
            }
        }
        false
    }

    fn hanging_survives(&self, run: &mut Run, state: BlockStateId, at: Pos) -> bool {
        if state.kind() != Some(Kind::HangingRoots) {
            return true;
        }
        run.region
            .get(at.0, at.1 + 1, at.2)
            .is_face_sturdy(Face::Down, Support::Full)
    }

    fn place_roots(&self, run: &mut Run, origin: Pos) {
        for _ in 0..self.hanging_attempts {
            let r = self.hanging_radius;
            let s = self.hanging_span;
            let dx = run.rng.below(r) - run.rng.below(r);
            let dy = run.rng.below(s) - run.rng.below(s);
            let dz = run.rng.below(r) - run.rng.below(r);
            let at = (origin.0 + dx, origin.1 + dy, origin.2 + dz);
            if !run.region.get(at.0, at.1, at.2).is_air() {
                continue;
            }
            let Some(state) = self.hanging_state.resolve(run.region, run.rng, at) else {
                continue;
            };
            if self.hanging_survives(run, state, at)
                && run
                    .region
                    .get(at.0, at.1 + 1, at.2)
                    .is_face_sturdy(Face::Down, Support::Full)
            {
                run.region.set(at.0, at.1, at.2, state);
            }
        }
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        if !run.region.get(origin.0, origin.1, origin.2).is_air() {
            return false;
        }
        if self.dirt_and_tree(run, origin) {
            self.place_roots(run, origin);
        }
        true
    }
}

pub(crate) fn feature(value: &Value, tree: Arc<Placed>) -> Feature {
    Feature::Root(Box::new(Root::compile(value, tree)))
}

#[cfg(test)]
mod tests;
