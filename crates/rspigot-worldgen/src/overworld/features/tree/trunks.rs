use rspigot_domain::BlockStateId;
use serde_json::Value;

use super::{Attachment, Site};
use crate::overworld::features::prov::{BlockSet, Int, Pos, integer, number};

fn axis_of(direction: (i32, i32)) -> &'static str {
    if direction.0 != 0 { "x" } else { "z" }
}

fn step(pos: Pos, direction: (i32, i32)) -> Pos {
    (pos.0 + direction.0, pos.1, pos.2 + direction.1)
}

fn attachment(pos: Pos) -> Attachment {
    Attachment {
        pos,
        radius_offset: 0,
        double: false,
    }
}

fn manhattan(a: Pos, b: Pos) -> i32 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs() + (a.2 - b.2).abs()
}

#[derive(Debug, Clone)]
pub(super) struct Cherry {
    count: Int,
    length: Int,
    start: (i32, i32),
    end: Int,
}

impl Cherry {
    pub(super) fn compile(value: &Value) -> Self {
        let start = value.get("branch_start_offset_from_top");
        let bound = |key: &str| {
            start
                .map(|range| integer(range, key, 0))
                .unwrap_or_default()
        };
        Self {
            count: Int::compile(value.get("branch_count").unwrap_or(&Value::Null)),
            length: Int::compile(
                value
                    .get("branch_horizontal_length")
                    .unwrap_or(&Value::Null),
            ),
            start: (bound("min_inclusive"), bound("max_inclusive")),
            end: Int::compile(
                value
                    .get("branch_end_offset_from_top")
                    .unwrap_or(&Value::Null),
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct Bending {
    min_height_for_leaves: i32,
    bend: Int,
}

impl Bending {
    pub(super) fn compile(value: &Value) -> Self {
        Self {
            min_height_for_leaves: integer(value, "min_height_for_leaves", 1),
            bend: Int::compile(value.get("bend_length").unwrap_or(&Value::Null)),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct Upwards {
    steps: Int,
    probability: f32,
    length: Int,
    through: BlockSet,
}

impl Upwards {
    pub(super) fn compile(value: &Value) -> Self {
        Self {
            steps: Int::compile(value.get("extra_branch_steps").unwrap_or(&Value::Null)),
            probability: number(value, "place_branch_per_log_probability", 0.0) as f32,
            length: Int::compile(value.get("extra_branch_length").unwrap_or(&Value::Null)),
            through: BlockSet::compile(value.get("can_grow_through").unwrap_or(&Value::Null)),
        }
    }

    pub(super) fn grows_through(&self, state: BlockStateId) -> bool {
        self.through.contains(state)
    }
}

impl Site<'_, '_, '_> {
    fn uniform(&mut self, (min, max): (i32, i32)) -> i32 {
        self.run.rng.between(min, max)
    }

    pub(super) fn cherry(&mut self, config: &Cherry, height: i32, origin: Pos) -> Vec<Attachment> {
        self.dirt((origin.0, origin.1 - 1, origin.2));
        let first = 0.max(height - 1 + self.uniform(config.start));
        let second_range = (config.start.0, config.start.1 - 1);
        let mut second = 0.max(height - 1 + self.uniform(second_range));
        if second >= first {
            second += 1;
        }
        let count = config.count.sample(self.run.rng);
        let three = count == 3;
        let several = count >= 2;
        let log_height = if three {
            height
        } else if several {
            first.max(second) + 1
        } else {
            first + 1
        };
        for i in 0..log_height {
            self.log((origin.0, origin.1 + i, origin.2));
        }
        let mut result = Vec::new();
        if three {
            result.push(attachment((origin.0, origin.1 + log_height, origin.2)));
        }
        let direction = self.random_horizontal();
        result.push(self.cherry_branch(
            config,
            height,
            origin,
            (direction, first, first < log_height - 1),
        ));
        if several {
            let opposite = (-direction.0, -direction.1);
            result.push(self.cherry_branch(
                config,
                height,
                origin,
                (opposite, second, second < log_height - 1),
            ));
        }
        result
    }

    fn cherry_branch(
        &mut self,
        config: &Cherry,
        height: i32,
        origin: Pos,
        (direction, start, near_top): ((i32, i32), i32, bool),
    ) -> Attachment {
        let axis = axis_of(direction);
        let mut at = (origin.0, origin.1 + start, origin.2);
        let end_offset = height - 1 + config.end.sample(self.run.rng);
        let long = near_top || end_offset < start;
        let length = config.length.sample(self.run.rng) + i32::from(long);
        let end = (
            origin.0 + direction.0 * length,
            origin.1 + end_offset,
            origin.2 + direction.1 * length,
        );
        for _ in 0..if long { 2 } else { 1 } {
            at = step(at, direction);
            self.put_log(at, Some(axis));
        }
        let vertical = if end.1 > at.1 { 1 } else { -1 };
        loop {
            let distance = manhattan(at, end);
            if distance == 0 {
                break;
            }
            let share = (end.1 - at.1).abs() as f32 / distance as f32;
            let climb = self.run.rng.next_float() < share;
            at = if climb {
                (at.0, at.1 + vertical, at.2)
            } else {
                step(at, direction)
            };
            self.put_log(at, if climb { None } else { Some(axis) });
        }
        attachment((end.0, end.1 + 1, end.2))
    }

    pub(super) fn bending(
        &mut self,
        config: &Bending,
        height: i32,
        origin: Pos,
    ) -> Vec<Attachment> {
        let direction = self.random_horizontal();
        let top = height - 1;
        let mut at = origin;
        self.dirt((origin.0, origin.1 - 1, origin.2));
        let mut result = Vec::new();
        for i in 0..=top {
            if i + 1 >= top + self.run.rng.below(2) {
                at = step(at, direction);
            }
            self.log(at);
            if i >= config.min_height_for_leaves {
                result.push(attachment(at));
            }
            at.1 += 1;
        }
        let bend = config.bend.sample(self.run.rng);
        for _ in 0..=bend {
            self.log(at);
            result.push(attachment(at));
            at = step(at, direction);
        }
        result
    }

    pub(super) fn upwards(
        &mut self,
        config: &Upwards,
        height: i32,
        origin: Pos,
    ) -> Vec<Attachment> {
        let mut result = Vec::new();
        for i in 0..height {
            let y = origin.1 + i;
            let placed = self.log((origin.0, y, origin.2));
            if placed && i < height - 1 && self.run.rng.next_float() < config.probability {
                let direction = self.random_horizontal();
                let length = config.length.sample(self.run.rng);
                let start = 0.max(length - config.length.sample(self.run.rng) - 1);
                let steps = config.steps.sample(self.run.rng);
                self.upwards_branch(
                    height,
                    &mut result,
                    (origin.0, y, origin.2),
                    (direction, start, steps),
                );
            }
            if i == height - 1 {
                result.push(attachment((origin.0, y + 1, origin.2)));
            }
        }
        result
    }

    fn upwards_branch(
        &mut self,
        height: i32,
        result: &mut Vec<Attachment>,
        (x0, y, z0): Pos,
        (direction, start, steps): ((i32, i32), i32, i32),
    ) {
        let mut last = y + start;
        let (mut x, mut z) = (x0, z0);
        let mut remaining = steps;
        let mut i = start;
        while i < height && remaining > 0 {
            if i >= 1 {
                let at_y = y + i;
                x += direction.0;
                z += direction.1;
                last = at_y;
                if self.log((x, at_y, z)) {
                    last += 1;
                }
                result.push(attachment((x, at_y, z)));
            }
            i += 1;
            remaining -= 1;
        }
        if last - y > 1 {
            result.push(attachment((x, last, z)));
            result.push(attachment((x, last - 2, z)));
        }
    }
}
