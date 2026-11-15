use rspigot_blocks::{BlockKind as Kind, Face};
use serde_json::Value;

use super::cave::moved;
use super::floats::Float;
use super::place::Run;
use super::prov::{BlockSet, Int, Pos, integer, number};
use super::speleothem::{
    Column, circle_mostly_embedded, empty_or_water, empty_or_water_or_lava, is_base,
    speleothem_height, state_at,
};
use crate::climate::Xoroshiro;
use crate::overworld::region::{Heightmap, Region};

#[derive(Debug, Clone)]
pub(crate) struct Large {
    replaceable: BlockSet,
    search: i32,
    radius: Int,
    scale: Float,
    ratio: f32,
    stalactite: Float,
    stalagmite: Float,
    wind: Float,
    wind_radius: i32,
    wind_bluntness: f32,
}

#[derive(Debug, Clone, Copy)]
struct Wind {
    origin_y: i32,
    speed: Option<(f64, f64)>,
    limit: i32,
}

impl Wind {
    fn none() -> Self {
        Self {
            origin_y: 0,
            speed: None,
            limit: 0,
        }
    }

    fn blowing(origin_y: i32, rng: &mut Xoroshiro, range: &Float, limit: i32) -> Self {
        let strength = range.sample(rng);
        let angle = rng.next_float() * (std::f32::consts::PI - 0.0) + 0.0;
        let x = libm::cos(f64::from(angle)) as f32 * strength;
        let z = libm::sin(f64::from(angle)) as f32 * strength;
        Self {
            origin_y,
            speed: Some((f64::from(x), f64::from(z))),
            limit,
        }
    }

    fn offset(&self, (x, y, z): Pos) -> Pos {
        let Some((sx, sz)) = self.speed else {
            return (x, y, z);
        };
        let scale = f64::from(self.origin_y - y);
        let dx = ((sx * scale).floor() as i32).clamp(-self.limit, self.limit);
        let dz = ((sz * scale).floor() as i32).clamp(-self.limit, self.limit);
        (x + dx, y, z + dz)
    }
}

struct Stone {
    root: Pos,
    up: bool,
    radius: i32,
    bluntness: f64,
    scale: f64,
}

impl Stone {
    fn height_at(&self, distance: f32) -> i32 {
        speleothem_height(
            f64::from(distance),
            f64::from(self.radius),
            self.scale,
            self.bluntness,
        ) as i32
    }

    fn suitable_for_wind(&self, radius: i32, bluntness: f32) -> bool {
        radius <= self.radius && f64::from(bluntness) <= self.bluntness
    }

    fn settle(&mut self, region: &mut Region, wind: &Wind) -> bool {
        while self.radius > 1 {
            let mut at = self.root;
            let steps = 10.min(self.height_at(0.0));
            for _ in 0..steps {
                if state_at(region, at).kind() == Some(Kind::Lava) {
                    return false;
                }
                if circle_mostly_embedded(region, wind.offset(at), self.radius) {
                    self.root = at;
                    return true;
                }
                at = moved(at, if self.up { Face::Down } else { Face::Up });
            }
            self.radius /= 2;
        }
        false
    }

    fn build(&self, region: &mut Region, rng: &mut Xoroshiro, wind: &Wind) {
        let r = self.radius;
        for i in -r..=r {
            for j in -r..=r {
                let distance = ((i * i + j * j) as f32 as f64).sqrt() as f32;
                if distance > r as f32 {
                    continue;
                }
                let mut height = self.height_at(distance);
                if height <= 0 {
                    continue;
                }
                if f64::from(rng.next_float()) < 0.2 {
                    height = (height as f32 * (rng.next_float() * (1.0 - 0.8) + 0.8)) as i32;
                }
                let mut at = (self.root.0 + i, self.root.1, self.root.2 + j);
                let mut carved = false;
                let ceiling = if self.up {
                    region.height_at(Heightmap::WorldSurface, at.0, at.2)
                } else {
                    i32::MAX
                };
                let mut step = 0;
                while step < height && at.1 < ceiling {
                    let target = wind.offset(at);
                    let state = state_at(region, target);
                    if empty_or_water_or_lava(state) {
                        carved = true;
                        region.set(
                            target.0,
                            target.1,
                            target.2,
                            Kind::DripstoneBlock.default_state(),
                        );
                    } else if carved && stone(state) {
                        break;
                    }
                    at = moved(at, if self.up { Face::Up } else { Face::Down });
                    step += 1;
                }
            }
        }
    }
}

fn stone(state: rspigot_domain::BlockStateId) -> bool {
    use std::sync::OnceLock;
    static STONE: OnceLock<BlockSet> = OnceLock::new();
    STONE
        .get_or_init(|| BlockSet::tag("minecraft:base_stone_overworld"))
        .contains(state)
}

fn bounds(int: &Int) -> (i32, i32) {
    match int {
        Int::Constant(n) => (*n, *n),
        Int::Uniform(a, b) => (*a, *b),
        Int::Clamped(source, a, b) => {
            let (low, high) = bounds(source);
            ((*a).max(low), (*b).min(high))
        }
        _ => (0, 0),
    }
}

impl Large {
    pub(crate) fn compile(value: &Value) -> Self {
        let float = |key: &str| Float::compile(value.get(key).unwrap_or(&Value::Null));
        Self {
            replaceable: BlockSet::compile(value.get("replaceable_blocks").unwrap_or(&Value::Null)),
            search: integer(value, "floor_to_ceiling_search_range", 30),
            radius: Int::compile(value.get("column_radius").unwrap_or(&Value::Null)),
            scale: float("height_scale"),
            ratio: number(value, "max_column_radius_to_cave_height_ratio", 0.1) as f32,
            stalactite: float("stalactite_bluntness"),
            stalagmite: float("stalagmite_bluntness"),
            wind: float("wind_speed"),
            wind_radius: integer(value, "min_radius_for_wind", 0),
            wind_bluntness: number(value, "min_bluntness_for_wind", 0.0) as f32,
        }
    }

    pub(crate) fn reach(&self) -> i32 {
        16
    }

    fn stone(
        &self,
        rng: &mut Xoroshiro,
        root: Pos,
        up: bool,
        radius: i32,
        bluntness: &Float,
    ) -> Stone {
        let bluntness = f64::from(bluntness.sample(rng));
        let scale = f64::from(self.scale.sample(rng));
        Stone {
            root,
            up,
            radius,
            bluntness,
            scale,
        }
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        if !empty_or_water(state_at(run.region, origin)) {
            return false;
        }
        let replaceable = &self.replaceable;
        let found =
            super::speleothem::scan(run.region, origin, self.search, empty_or_water, &|state| {
                is_base(state, Kind::DripstoneBlock, replaceable)
                    || state.kind() == Some(Kind::Lava)
            });
        let Some(Column {
            floor: Some(floor),
            ceiling: Some(ceiling),
        }) = found
        else {
            return false;
        };
        let height = ceiling - floor - 1;
        if height < 4 {
            return false;
        }
        let (low, high) = bounds(&self.radius);
        let limit = ((height as f32 * self.ratio) as i32).clamp(low, high);
        let radius = run.rng.below(limit - low + 1) + low;
        let mut hanging = self.stone(
            run.rng,
            (origin.0, ceiling - 1, origin.2),
            false,
            radius,
            &self.stalactite,
        );
        let mut standing = self.stone(
            run.rng,
            (origin.0, floor + 1, origin.2),
            true,
            radius,
            &self.stalagmite,
        );
        let wind = if hanging.suitable_for_wind(self.wind_radius, self.wind_bluntness)
            && standing.suitable_for_wind(self.wind_radius, self.wind_bluntness)
        {
            Wind::blowing(origin.1, run.rng, &self.wind, 16 - radius)
        } else {
            Wind::none()
        };
        let lower = hanging.settle(run.region, &wind);
        let upper = standing.settle(run.region, &wind);
        if lower {
            hanging.build(run.region, run.rng, &wind);
        }
        if upper {
            standing.build(run.region, run.rng, &wind);
        }
        true
    }
}

#[cfg(test)]
mod tests;
