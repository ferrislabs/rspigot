use rspigot_blocks::BlockKind as Kind;
use rspigot_data::multi_noise::NoiseParameters;
use rspigot_domain::BlockStateId;
use serde_json::Value;

use super::place::Run;
use super::prov::{BlockSet, Int, Pos, State, flag, integer, number};
use crate::climate::NoiseStack;
use crate::datapack::block_state;

const DIRECTIONS: [(&str, (i32, i32, i32)); 6] = [
    ("down", (0, -1, 0)),
    ("up", (0, 1, 0)),
    ("north", (0, 0, -1)),
    ("south", (0, 0, 1)),
    ("west", (-1, 0, 0)),
    ("east", (1, 0, 0)),
];

const NOISE_BASE_AMPLITUDE: f64 = 0.955_388_882_960_065;

#[derive(Debug, Clone)]
struct Layers {
    filling: State,
    inner: State,
    alternate: State,
    middle: State,
    outer: State,
}

#[derive(Debug, Clone, Copy)]
struct Thresholds {
    filling: f64,
    inner: f64,
    middle: f64,
    outer: f64,
}

#[derive(Debug, Clone)]
pub(crate) struct Geode {
    layers: Layers,
    placements: Vec<BlockStateId>,
    cannot_replace: BlockSet,
    invalid: BlockSet,
    filling: f64,
    inner: f64,
    middle: f64,
    outer: f64,
    crack_chance: f64,
    crack_size: f64,
    crack_offset: i32,
    potential_chance: f64,
    alternate_chance: f64,
    require_alternate: bool,
    wall: Int,
    points: Int,
    offset: Int,
    min_gen: i32,
    max_gen: i32,
    noise_scale: f64,
    threshold: i32,
}

fn uniform(value: &Value, key: &str, min: i32, max: i32) -> Int {
    value.get(key).map_or(Int::Uniform(min, max), Int::compile)
}

fn provider(blocks: &Value, key: &str) -> State {
    State::compile(blocks.get(key).unwrap_or(&Value::Null))
}

const NOISE: NoiseParameters = NoiseParameters {
    id: "geode",
    base_octave: -4,
    octave_count: 1,
    base_amplitude: NOISE_BASE_AMPLITUDE,
    amplitude_modifiers: &[],
};

fn can_cluster_grow(state: BlockStateId) -> bool {
    state.is_air()
        || (state.kind() == Some(Kind::Water)
            && state
                .property("level")
                .and_then(|level| level.parse::<i32>().ok())
                .is_some_and(|level| level == 0 || level >= 8))
}

fn is_source(state: BlockStateId) -> bool {
    match state.kind() {
        Some(Kind::Water) => state.property("level") == Some("0"),
        Some(Kind::Seagrass | Kind::TallSeagrass | Kind::Kelp | Kind::KelpPlant) => true,
        _ => state.property("waterlogged") == Some("true"),
    }
}

fn distance_squared((ax, ay, az): Pos, (bx, by, bz): Pos) -> f64 {
    let (dx, dy, dz) = (f64::from(ax - bx), f64::from(ay - by), f64::from(az - bz));
    dx * dx + dy * dy + dz * dz
}

fn inverse_root(value: f64) -> f64 {
    1.0 / value.sqrt()
}

impl Geode {
    pub(crate) fn compile(value: &Value) -> Self {
        let empty = Value::Null;
        let blocks = value.get("blocks").unwrap_or(&empty);
        let layers = value.get("layers").unwrap_or(&empty);
        let crack = value.get("crack").unwrap_or(&empty);
        Self {
            layers: Layers {
                filling: provider(blocks, "filling_provider"),
                inner: provider(blocks, "inner_layer_provider"),
                alternate: provider(blocks, "alternate_inner_layer_provider"),
                middle: provider(blocks, "middle_layer_provider"),
                outer: provider(blocks, "outer_layer_provider"),
            },
            placements: blocks
                .get("inner_placements")
                .and_then(Value::as_array)
                .map(|items| items.iter().filter_map(block_state).collect())
                .unwrap_or_default(),
            cannot_replace: BlockSet::compile(blocks.get("cannot_replace").unwrap_or(&empty)),
            invalid: BlockSet::compile(blocks.get("invalid_blocks").unwrap_or(&empty)),
            filling: number(layers, "filling", 1.7),
            inner: number(layers, "inner_layer", 2.2),
            middle: number(layers, "middle_layer", 3.2),
            outer: number(layers, "outer_layer", 4.2),
            crack_chance: number(crack, "generate_crack_chance", 1.0),
            crack_size: number(crack, "base_crack_size", 2.0),
            crack_offset: integer(crack, "crack_point_offset", 2),
            potential_chance: number(value, "use_potential_placements_chance", 0.35),
            alternate_chance: number(value, "use_alternate_layer0_chance", 0.0),
            require_alternate: flag(value, "placements_require_layer0_alternate", true),
            wall: uniform(value, "outer_wall_distance", 4, 6),
            points: uniform(value, "distribution_points", 3, 4),
            offset: uniform(value, "point_offset", 1, 2),
            min_gen: integer(value, "min_gen_offset", -16),
            max_gen: integer(value, "max_gen_offset", 16),
            noise_scale: number(value, "noise_multiplier", 0.05),
            threshold: integer(value, "invalid_blocks_threshold", 0),
        }
    }

    pub(crate) fn reach(&self) -> i32 {
        self.min_gen.abs().max(self.max_gen.abs()) + 1
    }

    fn thresholds(&self, points: i32) -> Thresholds {
        let ratio = f64::from(points) / f64::from(self.wall.max_inclusive());
        Thresholds {
            filling: inverse_root(self.filling),
            inner: inverse_root(self.inner + ratio),
            middle: inverse_root(self.middle + ratio),
            outer: inverse_root(self.outer + ratio),
        }
    }

    fn safe_set(&self, run: &mut Run, (x, y, z): Pos, state: BlockStateId) {
        if !self.cannot_replace.contains(run.region.get(x, y, z)) {
            run.region.set(x, y, z, state);
        }
    }

    fn draw(&self, run: &mut Run, state: &State, pos: Pos) -> BlockStateId {
        state
            .resolve(run.region, run.rng, pos)
            .unwrap_or(BlockStateId::AIR)
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        let count = self.points.sample(run.rng);
        let noise = NoiseStack::create_legacy(&NOISE, run.seed);
        let limits = self.thresholds(count);
        let ratio = f64::from(count) / f64::from(self.wall.max_inclusive());
        let crack_bias = if count > 3 { ratio } else { 0.0 };
        let crack_roll = run.rng.next_double();
        let crack_limit = inverse_root(self.crack_size + crack_roll / 2.0 + crack_bias);
        let cracked = f64::from(run.rng.next_float()) < self.crack_chance;
        let mut points: Vec<(Pos, i32)> = Vec::new();
        let mut invalid = 0;
        for _ in 0..count {
            let dx = self.wall.sample(run.rng);
            let dy = self.wall.sample(run.rng);
            let dz = self.wall.sample(run.rng);
            let at = (origin.0 + dx, origin.1 + dy, origin.2 + dz);
            let state = run.region.get(at.0, at.1, at.2);
            if state.is_air() || self.invalid.contains(state) {
                invalid += 1;
                if invalid > self.threshold {
                    return false;
                }
            }
            points.push((at, self.offset.sample(run.rng)));
        }
        let mut cracks: Vec<Pos> = Vec::new();
        if cracked {
            let side = run.rng.below(4);
            let far = count * 2 + 1;
            let (dx, dz) = match side {
                0 => (far, 0),
                1 => (0, far),
                2 => (far, far),
                _ => (0, 0),
            };
            for dy in [7, 5, 1] {
                cracks.push((origin.0 + dx, origin.1 + dy, origin.2 + dz));
            }
        }
        let mut potential: Vec<Pos> = Vec::new();
        let (lo, hi) = (self.min_gen, self.max_gen);
        for z in lo..=hi {
            for y in lo..=hi {
                for x in lo..=hi {
                    let at = (origin.0 + x, origin.1 + y, origin.2 + z);
                    let bonus =
                        f64::from(noise.get(f64::from(at.0), f64::from(at.1), f64::from(at.2)))
                            * self.noise_scale;
                    let mut sum = 0.0;
                    for (point, offset) in &points {
                        sum +=
                            inverse_root(distance_squared(at, *point) + f64::from(*offset)) + bonus;
                    }
                    if sum < limits.outer {
                        continue;
                    }
                    if sum >= limits.filling {
                        let state = self.draw(run, &self.layers.filling, at);
                        self.safe_set(run, at, state);
                        continue;
                    }
                    let mut crack_sum = 0.0;
                    for crack in &cracks {
                        crack_sum += inverse_root(
                            distance_squared(at, *crack) + f64::from(self.crack_offset),
                        ) + bonus;
                    }
                    if cracked && crack_sum >= crack_limit {
                        self.safe_set(run, at, BlockStateId::AIR);
                    } else if sum >= limits.inner {
                        let alternate = f64::from(run.rng.next_float()) < self.alternate_chance;
                        let layer = if alternate {
                            &self.layers.alternate
                        } else {
                            &self.layers.inner
                        };
                        let state = self.draw(run, layer, at);
                        self.safe_set(run, at, state);
                        if (!self.require_alternate || alternate)
                            && f64::from(run.rng.next_float()) < self.potential_chance
                        {
                            potential.push(at);
                        }
                    } else if sum >= limits.middle {
                        let state = self.draw(run, &self.layers.middle, at);
                        self.safe_set(run, at, state);
                    } else {
                        let state = self.draw(run, &self.layers.outer, at);
                        self.safe_set(run, at, state);
                    }
                }
            }
        }
        self.grow(run, &potential);
        true
    }

    fn grow(&self, run: &mut Run, potential: &[Pos]) {
        if self.placements.is_empty() {
            return;
        }
        for at in potential {
            let mut state = self.placements[run.rng.below(self.placements.len() as i32) as usize];
            for (name, (dx, dy, dz)) in DIRECTIONS {
                if let Some(facing) = state.with_property("facing", name) {
                    state = facing;
                }
                let next = (at.0 + dx, at.1 + dy, at.2 + dz);
                let neighbor = run.region.get(next.0, next.1, next.2);
                if state.property("waterlogged").is_some()
                    && let Some(wet) = state.with_property(
                        "waterlogged",
                        if is_source(neighbor) { "true" } else { "false" },
                    )
                {
                    state = wet;
                }
                if can_cluster_grow(neighbor) {
                    self.safe_set(run, next, state);
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
