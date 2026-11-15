use std::collections::HashMap;

use super::eval::Sampler;
use super::graph::Id;
use crate::climate::Positional;

pub(crate) const WAY_BELOW_MIN_Y: i32 = -2032 << 4;
const LAVA_LEVEL: i32 = -54;
const SURFACE_OFFSETS: [(i32, i32); 13] = [
    (0, 0),
    (-2, -1),
    (-1, -1),
    (0, -1),
    (1, -1),
    (-3, 0),
    (-2, 0),
    (-1, 0),
    (1, 0),
    (-2, 1),
    (-1, 1),
    (0, 1),
    (1, 1),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fluid {
    Air,
    Water,
    Lava,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Status {
    level: i32,
    fluid: Fluid,
}

impl Status {
    fn at(self, y: i32) -> Fluid {
        if y < self.level {
            self.fluid
        } else {
            Fluid::Air
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AquiferIds {
    pub(crate) barrier: Id,
    pub(crate) floodedness: Id,
    pub(crate) spread: Id,
    pub(crate) lava: Id,
    pub(crate) exclusion: Id,
    pub(crate) surface_level: Id,
}

fn similarity(first: i32, second: i32) -> f64 {
    1.0 - f64::from(second - first) / 25.0
}

fn flowing_update_similarity() -> f64 {
    similarity(100, 144)
}

fn grid_y(block: i32) -> i32 {
    block.div_euclid(12)
}

fn lerp_f64(t: f64, a: f64, b: f64) -> f64 {
    a + t * (b - a)
}

fn map(value: f64, in_a: f64, in_b: f64, out_a: f64, out_b: f64) -> f64 {
    lerp_f64((value - in_a) / (in_b - in_a), out_a, out_b)
}

fn clamped_map(value: f64, in_a: f64, in_b: f64, out_a: f64, out_b: f64) -> f64 {
    let t = ((value - in_a) / (in_b - in_a)).clamp(0.0, 1.0);
    lerp_f64(t, out_a, out_b)
}

#[derive(Debug)]
pub(crate) struct Aquifer {
    ids: AquiferIds,
    random: Positional,
    sea_level: i32,
    min_grid: [i32; 3],
    size_x: i32,
    size_z: i32,
    locations: Vec<Option<[i32; 3]>>,
    statuses: Vec<Option<Status>>,
    surface_cache: HashMap<(i32, i32), i32>,
    skip_above: i32,
    pub(crate) schedule_fluid_update: bool,
}

impl Aquifer {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        sampler: &mut Sampler<'_>,
        ids: AquiferIds,
        random: Positional,
        sea_level: i32,
        x0: i32,
        z0: i32,
        min_y: i32,
        height: i32,
    ) -> Self {
        let min_gx = (x0 - 5) >> 4;
        let max_gx = ((x0 + 15 - 5) >> 4) + 1;
        let min_gz = (z0 - 5) >> 4;
        let max_gz = ((z0 + 15 - 5) >> 4) + 1;
        let min_gy = grid_y(min_y + 1) - 1;
        let max_block_y = min_y + height - 1;
        let size_y = grid_y(max_block_y + 1) - min_gy + 2;
        let size_x = max_gx - min_gx + 1;
        let size_z = max_gz - min_gz + 1;
        let total = (size_x * size_y * size_z) as usize;
        let mut aquifer = Self {
            ids,
            random,
            sea_level,
            min_grid: [min_gx, min_gy, min_gz],
            size_x,
            size_z,
            locations: vec![None; total],
            statuses: vec![None; total],
            surface_cache: HashMap::new(),
            skip_above: 0,
            schedule_fluid_update: false,
        };
        let max_surface = aquifer.max_surface_level(
            sampler,
            min_gx * 16,
            min_gz * 16,
            max_gx * 16 + 9,
            max_gz * 16 + 9,
        );
        let adjusted = max_surface + 8;
        aquifer.skip_above = (grid_y(adjusted + 12) + 1) * 12 + 11 - 1;
        aquifer
    }

    fn global(&self, y: i32) -> Status {
        if y < LAVA_LEVEL.min(self.sea_level) {
            Status {
                level: LAVA_LEVEL,
                fluid: Fluid::Lava,
            }
        } else {
            Status {
                level: self.sea_level,
                fluid: Fluid::Water,
            }
        }
    }

    fn max_surface_level(
        &mut self,
        sampler: &mut Sampler<'_>,
        min_x: i32,
        min_z: i32,
        max_x: i32,
        max_z: i32,
    ) -> i32 {
        let mut best = i32::MIN;
        for qz in (min_z >> 2)..=(max_z >> 2) {
            for qx in (min_x >> 2)..=(max_x >> 2) {
                let level = sampler
                    .eval(self.ids.surface_level, qx << 2, 0, qz << 2)
                    .floor() as i32;
                self.surface_cache.insert((qx << 2, qz << 2), level);
                best = best.max(level);
            }
        }
        best
    }

    fn surface_level(&mut self, sampler: &mut Sampler<'_>, x: i32, z: i32) -> i32 {
        let key = ((x >> 2) << 2, (z >> 2) << 2);
        if let Some(level) = self.surface_cache.get(&key) {
            return *level;
        }
        let level = sampler
            .eval(self.ids.surface_level, key.0, 0, key.1)
            .floor() as i32;
        self.surface_cache.insert(key, level);
        level
    }

    fn index(&self, gx: i32, gy: i32, gz: i32) -> usize {
        (((gy - self.min_grid[1]) * self.size_z + (gz - self.min_grid[2])) * self.size_x
            + (gx - self.min_grid[0])) as usize
    }

    fn location(&mut self, gx: i32, gy: i32, gz: i32) -> [i32; 3] {
        let index = self.index(gx, gy, gz);
        if let Some(found) = self.locations[index] {
            return found;
        }
        let mut random = self.random.at(gx, gy, gz);
        let lx = gx * 16 + random.next_int(10) as i32;
        let ly = gy * 12 + random.next_int(9) as i32;
        let lz = gz * 16 + random.next_int(10) as i32;
        let found = [lx, ly, lz];
        self.locations[index] = Some(found);
        found
    }

    fn status(&mut self, sampler: &mut Sampler<'_>, index: usize, at: [i32; 3]) -> Status {
        if let Some(found) = self.statuses[index] {
            return found;
        }
        let status = self.compute_fluid(sampler, at[0], at[1], at[2]);
        self.statuses[index] = Some(status);
        status
    }

    fn compute_fluid(&mut self, sampler: &mut Sampler<'_>, x: i32, y: i32, z: i32) -> Status {
        let global = self.global(y);
        let mut lowest = i32::MAX;
        let top = y + 12;
        let bottom = y - 12;
        let mut center_under = false;
        for (ox, oz) in SURFACE_OFFSETS {
            let sx = x + (ox << 4);
            let sz = z + (oz << 4);
            let preliminary = self.surface_level(sampler, sx, sz);
            let adjusted = preliminary + 8;
            let center = ox == 0 && oz == 0;
            if center && adjusted < bottom {
                return global;
            }
            let below = adjusted < top;
            if below || center {
                let status = self.global(adjusted);
                if status.at(adjusted) != Fluid::Air {
                    if center {
                        center_under = true;
                    }
                    if below {
                        return status;
                    }
                }
            }
            lowest = lowest.min(preliminary);
        }
        let level = self.compute_surface_level(sampler, x, y, z, global, lowest, center_under);
        let fluid = self.compute_fluid_type(sampler, x, y, z, global, level);
        Status { level, fluid }
    }

    #[allow(clippy::too_many_arguments)]
    fn compute_surface_level(
        &mut self,
        sampler: &mut Sampler<'_>,
        x: i32,
        y: i32,
        z: i32,
        global: Status,
        lowest: i32,
        center_under: bool,
    ) -> i32 {
        let exclusion = sampler.eval(self.ids.exclusion, x, y, z);
        let (partial, full) = if exclusion <= 0.0 {
            let adjusted = lowest + 8;
            let weight = if center_under {
                clamped_map(f64::from(adjusted - y), 0.0, 64.0, 1.0, 0.0)
            } else {
                0.0
            };
            let flood = f64::from(sampler.eval(self.ids.floodedness, x, y, z)).clamp(-1.0, 1.0);
            let full_threshold = map(weight, 1.0, 0.0, -0.3, 0.8);
            let partial_threshold = map(weight, 1.0, 0.0, -0.8, 0.4);
            (flood - partial_threshold, flood - full_threshold)
        } else {
            (-1.0, -1.0)
        };
        if full > 0.0 {
            global.level
        } else if partial > 0.0 {
            self.randomized_level(sampler, x, y, z, lowest)
        } else {
            WAY_BELOW_MIN_Y
        }
    }

    fn randomized_level(
        &mut self,
        sampler: &mut Sampler<'_>,
        x: i32,
        y: i32,
        z: i32,
        lowest: i32,
    ) -> i32 {
        let gx = x.div_euclid(16);
        let gy = y.div_euclid(40);
        let gz = z.div_euclid(16);
        let center = gy * 40 + 20;
        let spread = sampler.eval(self.ids.spread, gx, gy, gz);
        let quantized = ((f64::from(spread * 10.0)) / 3.0).floor() as i32 * 3;
        lowest.min(center + quantized)
    }

    fn compute_fluid_type(
        &mut self,
        sampler: &mut Sampler<'_>,
        x: i32,
        y: i32,
        z: i32,
        global: Status,
        level: i32,
    ) -> Fluid {
        if level < -9 && level != WAY_BELOW_MIN_Y && global.fluid != Fluid::Lava {
            let value = sampler.eval(
                self.ids.lava,
                x.div_euclid(64),
                y.div_euclid(40),
                z.div_euclid(64),
            );
            if f64::from(value).abs() > 0.3 {
                return Fluid::Lava;
            }
        }
        global.fluid
    }

    #[allow(clippy::too_many_arguments)]
    fn pressure(
        &self,
        sampler: &mut Sampler<'_>,
        x: i32,
        y: i32,
        z: i32,
        barrier: &mut f64,
        first: Status,
        second: Status,
    ) -> f64 {
        let a = first.at(y);
        let b = second.at(y);
        if (a == Fluid::Lava && b == Fluid::Water) || (a == Fluid::Water && b == Fluid::Lava) {
            return 2.0;
        }
        let difference = (first.level - second.level).abs();
        if difference == 0 {
            return 0.0;
        }
        let dy = (f64::from(y) + 0.5) - f64::from(first.level + second.level) * 0.5;
        let half = f64::from(difference) / 2.0 - dy.abs();
        let scaled = if dy > 0.0 {
            let lifted = half + 0.0;
            if lifted > 0.0 {
                lifted / 1.5
            } else {
                lifted / 2.5
            }
        } else {
            let shifted = half + 3.0;
            if shifted > 0.0 {
                shifted / 3.0
            } else {
                shifted / 10.0
            }
        };
        let noise = if !(-2.0..=2.0).contains(&scaled) {
            0.0
        } else {
            if barrier.is_nan() {
                *barrier = f64::from(sampler.eval(self.ids.barrier, x, y, z));
            }
            *barrier
        };
        (noise + scaled) * 2.0
    }

    pub(crate) fn substance(
        &mut self,
        sampler: &mut Sampler<'_>,
        x: i32,
        y: i32,
        z: i32,
        density: f64,
    ) -> Option<Fluid> {
        if density > 0.0 {
            self.schedule_fluid_update = false;
            return None;
        }
        let global = self.global(y);
        if self.skip_above < y {
            self.schedule_fluid_update = false;
            return Some(global.at(y));
        }
        if global.at(y) == Fluid::Lava {
            self.schedule_fluid_update = false;
            return Some(Fluid::Lava);
        }
        let gx = (x - 5) >> 4;
        let gy = grid_y(y + 1);
        let gz = (z - 5) >> 4;
        let mut distances = [i32::MAX; 4];
        let mut indices = [0usize; 4];
        let mut positions = [[0i32; 3]; 4];
        for xo in 0..2 {
            for yo in -1..2 {
                for zo in 0..2 {
                    let at = self.location(gx + xo, gy + yo, gz + zo);
                    let index = self.index(gx + xo, gy + yo, gz + zo);
                    let (dx, dy, dz) = (at[0] - x, at[1] - y, at[2] - z);
                    let distance = dx * dx + dy * dy + dz * dz;
                    let slot = if distances[0] >= distance {
                        0
                    } else if distances[1] >= distance {
                        1
                    } else if distances[2] >= distance {
                        2
                    } else if distances[3] >= distance {
                        3
                    } else {
                        continue;
                    };
                    for k in (slot + 1..4).rev() {
                        distances[k] = distances[k - 1];
                        indices[k] = indices[k - 1];
                        positions[k] = positions[k - 1];
                    }
                    distances[slot] = distance;
                    indices[slot] = index;
                    positions[slot] = at;
                }
            }
        }
        let flow = flowing_update_similarity();
        let s1 = self.status(sampler, indices[0], positions[0]);
        let sim12 = similarity(distances[0], distances[1]);
        let state = s1.at(y);
        if sim12 <= 0.0 {
            self.schedule_fluid_update = if sim12 < flow {
                false
            } else {
                let s2 = self.status(sampler, indices[1], positions[1]);
                s1 != s2
            };
            return Some(state);
        }
        if state == Fluid::Water && self.global(y - 1).at(y - 1) == Fluid::Lava {
            self.schedule_fluid_update = true;
            return Some(state);
        }
        let mut barrier = f64::NAN;
        let s2 = self.status(sampler, indices[1], positions[1]);
        let pressure = self.pressure(sampler, x, y, z, &mut barrier, s1, s2);
        if density + sim12 * pressure > 0.0 {
            self.schedule_fluid_update = false;
            return None;
        }
        let s3 = self.status(sampler, indices[2], positions[2]);
        let sim13 = similarity(distances[0], distances[2]);
        if sim13 > 0.0 {
            let pressure = self.pressure(sampler, x, y, z, &mut barrier, s1, s3);
            if density + sim12 * sim13 * pressure > 0.0 {
                self.schedule_fluid_update = false;
                return None;
            }
        }
        let sim23 = similarity(distances[1], distances[2]);
        if sim23 > 0.0 {
            let pressure = self.pressure(sampler, x, y, z, &mut barrier, s2, s3);
            if density + sim12 * sim23 * pressure > 0.0 {
                self.schedule_fluid_update = false;
                return None;
            }
        }
        let first = s1 != s2;
        let second = sim23 >= flow && s2 != s3;
        let third = sim13 >= flow && s1 != s3;
        if first || second || third {
            self.schedule_fluid_update = true;
            return Some(state);
        }
        self.schedule_fluid_update = if sim13 >= flow {
            let sim14 = similarity(distances[0], distances[3]);
            if sim14 >= flow {
                let s4 = self.status(sampler, indices[3], positions[3]);
                s1 != s4
            } else {
                false
            }
        } else {
            false
        };
        Some(state)
    }
}
