use std::sync::Arc;

use serde_json::Value;

use super::bodies::Feature;
use super::prov::{Height, Int, Pos, Pred, flag, integer, number, short, text};
use crate::climate::Xoroshiro;
use crate::overworld::region::{Heightmap, Region};
use crate::overworld::surface_rules::SurfaceSystem;

#[derive(Debug, Clone)]
pub(crate) enum Modifier {
    Count(Int),
    CountOnEveryLayer(Int),
    InSquare,
    Heightmap(Heightmap),
    HeightRange(Height),
    Rarity(f32),
    RandomChance(f32),
    Biome,
    SurfaceWaterDepth(i32),
    EnvironmentScan {
        up: bool,
        allowed: Pred,
        target: Pred,
        steps: i32,
    },
    Offset(Int, Int, Int),
    Filter(Pred),
    NoiseBasedCount {
        ratio: i32,
        factor: f64,
        offset: f64,
    },
    NoiseThresholdCount {
        level: f64,
        below: i32,
        above: i32,
    },
    SurfaceRelative {
        map: Heightmap,
        min: Option<i32>,
        max: Option<i32>,
    },
    RandomlySelected(Vec<Modifier>),
    Fixed(Vec<Pos>),
    Cuboid {
        xz: Int,
        y: Int,
        edges: bool,
        interior: bool,
    },
    Unsupported,
}

impl Modifier {
    pub(crate) fn compile(value: &Value) -> Self {
        let provider = |key: &str| Int::compile(value.get(key).unwrap_or(&Value::Null));
        match short(value) {
            "count" => Self::Count(provider("count")),
            "count_on_every_layer" => Self::CountOnEveryLayer(provider("count")),
            "in_square" => Self::InSquare,
            "heightmap" => Self::Heightmap(Heightmap::from_name(text(value, "heightmap"))),
            "height_range" => {
                Self::HeightRange(Height::compile(value.get("height").unwrap_or(&Value::Null)))
            }
            "rarity_filter" => Self::Rarity(1.0 / number(value, "chance", 1.0) as f32),
            "random_chance" => Self::RandomChance(number(value, "chance", 1.0) as f32),
            "biome" => Self::Biome,
            "surface_water_depth_filter" => {
                Self::SurfaceWaterDepth(integer(value, "max_water_depth", 0))
            }
            "environment_scan" => Self::EnvironmentScan {
                up: text(value, "direction_of_search") == "up",
                allowed: Pred::compile(
                    value
                        .get("allowed_search_condition")
                        .unwrap_or(&Value::Null),
                ),
                target: Pred::compile(value.get("target_condition").unwrap_or(&Value::Null)),
                steps: integer(value, "max_steps", 1),
            },
            "offset" => Self::Offset(provider("x"), provider("y"), provider("z")),
            "block_predicate_filter" => Self::Filter(Pred::compile(
                value.get("predicate").unwrap_or(&Value::Null),
            )),
            "noise_based_count" => Self::NoiseBasedCount {
                ratio: integer(value, "noise_to_count_ratio", 0),
                factor: number(value, "noise_factor", 1.0),
                offset: number(value, "noise_offset", 0.0),
            },
            "noise_threshold_count" => Self::NoiseThresholdCount {
                level: number(value, "noise_level", 0.0),
                below: integer(value, "below_noise", 0),
                above: integer(value, "above_noise", 0),
            },
            "surface_relative_threshold_filter" => Self::SurfaceRelative {
                map: Heightmap::from_name(text(value, "heightmap")),
                min: value
                    .get("min_inclusive")
                    .and_then(Value::as_i64)
                    .map(|n| n as i32),
                max: value
                    .get("max_inclusive")
                    .and_then(Value::as_i64)
                    .map(|n| n as i32),
            },
            "randomly_selected" => Self::RandomlySelected(
                value
                    .get("placements")
                    .and_then(Value::as_array)
                    .map(|items| items.iter().map(Self::compile).collect())
                    .unwrap_or_default(),
            ),
            "cuboid" => Self::Cuboid {
                xz: provider("xz_size"),
                y: provider("y_size"),
                edges: flag(value, "include_edges", true),
                interior: flag(value, "include_interior", true),
            },
            "fixed_placement" => Self::Fixed(
                value
                    .get("positions")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| {
                                let at = |i: usize| {
                                    item.get(i).and_then(Value::as_i64).map(|n| n as i32)
                                };
                                Some((at(0)?, at(1)?, at(2)?))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            ),
            _ => Self::Unsupported,
        }
    }

    pub(crate) fn is_supported(&self) -> bool {
        !matches!(self, Self::Unsupported)
    }

    fn max_offset(&self) -> i32 {
        match self {
            Self::Offset(x, _, z) => max_abs(x).max(max_abs(z)),
            Self::Cuboid { xz, .. } => max_abs(xz),
            _ => 0,
        }
    }
}

pub(super) fn max_abs(provider: &Int) -> i32 {
    match provider {
        Int::Constant(n) => n.abs(),
        Int::Uniform(a, b) | Int::BiasedToBottom(a, b) | Int::VeryBiasedToBottom(a, b) => {
            a.abs().max(b.abs())
        }
        Int::Trapezoid { min, max, .. } => min.abs().max(max.abs()),
        Int::Clamped(_, a, b) => a.abs().max(b.abs()),
        Int::ClampedNormal { min, max, .. } => min.abs().max(max.abs()),
        Int::Weighted(entries) => entries.iter().map(|(p, _)| max_abs(p)).max().unwrap_or(0),
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Placed {
    pub(crate) feature: Arc<Feature>,
    pub(crate) modifiers: Vec<Modifier>,
    pub(crate) margin: i32,
}

impl Placed {
    pub(crate) fn new(feature: Arc<Feature>, modifiers: Vec<Modifier>) -> Self {
        let margin = feature.reach() + modifiers.iter().map(Modifier::max_offset).sum::<i32>();
        Self {
            feature,
            modifiers,
            margin,
        }
    }
}

pub(crate) struct Run<'a, 'r> {
    pub(crate) region: &'a mut Region<'r>,
    pub(crate) rng: &'a mut Xoroshiro,
    pub(crate) surface: &'a SurfaceSystem,
    pub(crate) feature_id: usize,
    pub(crate) seed: i64,
    pub(crate) biome_has: &'a [Vec<bool>],
    pub(crate) universal: &'a [bool],
    pub(crate) interest: Option<(i32, i32, i32, i32)>,
}

impl Run<'_, '_> {
    pub(crate) fn outside(&self, x: i32, z: i32, margin: i32) -> bool {
        self.interest.is_some_and(|(x0, z0, x1, z1)| {
            x < x0 - margin || x > x1 + margin || z < z0 - margin || z > z1 + margin
        })
    }
}

fn is_empty(state: rspigot_domain::BlockStateId) -> bool {
    state.is_air() || super::prov::is_fluid(state)
}

impl Placed {
    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        let mut placed = false;
        self.step(run, 0, origin, &mut placed);
        placed
    }

    fn step(&self, run: &mut Run, index: usize, pos: Pos, placed: &mut bool) {
        let Some(modifier) = self.modifiers.get(index) else {
            if !run.outside(pos.0, pos.2, self.feature.reach()) {
                *placed |= self.feature.place(run, pos);
            }
            return;
        };
        let mut emitted: Vec<Pos> = Vec::new();
        apply(modifier, run, pos, self.margin, &mut |next| {
            emitted.push(next)
        });
        for next in emitted {
            self.step(run, index + 1, next, placed);
        }
    }
}

fn apply(modifier: &Modifier, run: &mut Run, pos: Pos, margin: i32, emit: &mut dyn FnMut(Pos)) {
    let (x, y, z) = pos;
    let (min_y, max_y) = (run.region.height().min_y, run.region.height().max_y() + 1);
    match modifier {
        Modifier::Count(count) => {
            for _ in 0..count.sample(run.rng) {
                emit(pos);
            }
        }
        Modifier::CountOnEveryLayer(count) => {
            let mut layer = 0;
            loop {
                let mut any = false;
                let mut index = 0;
                while index < count.sample(run.rng) {
                    index += 1;
                    let px = run.rng.below(16) + x;
                    let pz = run.rng.below(16) + z;
                    if run.outside(px, pz, margin) {
                        continue;
                    }
                    let top = run.region.height_at(Heightmap::MotionBlocking, px, pz);
                    if let Some(found) = ground_layer(run, px, top, pz, layer) {
                        emit((px, found, pz));
                        any = true;
                    }
                }
                layer += 1;
                if !any {
                    break;
                }
            }
        }
        Modifier::InSquare => {
            let px = run.rng.below(16) + x;
            let pz = run.rng.below(16) + z;
            if !run.outside(px, pz, margin) {
                emit((px, y, pz));
            }
        }
        Modifier::Heightmap(map) => {
            let top = run.region.height_at(*map, x, z);
            if top > min_y {
                emit((x, top, z));
            }
        }
        Modifier::HeightRange(height) => emit((x, height.sample(run.rng, min_y, max_y - 1), z)),
        Modifier::Rarity(chance) | Modifier::RandomChance(chance) => {
            if run.rng.next_float() < *chance {
                emit(pos);
            }
        }
        Modifier::Biome => {
            if run.universal.get(run.feature_id).copied().unwrap_or(false) {
                emit(pos);
                return;
            }
            let biome = run.region.biome(x, y, z);
            if run
                .biome_has
                .get(usize::from(biome))
                .is_some_and(|set| set.get(run.feature_id).copied().unwrap_or(false))
            {
                emit(pos);
            }
        }
        Modifier::SurfaceWaterDepth(max) => {
            let surface = run.region.height_at(Heightmap::WorldSurface, x, z);
            let floor = run.region.height_at(Heightmap::OceanFloor, x, z);
            if surface - floor <= *max {
                emit(pos);
            }
        }
        Modifier::EnvironmentScan {
            up,
            allowed,
            target,
            steps,
        } => {
            let step = if *up { 1 } else { -1 };
            let mut at = pos;
            if !allowed.test(run.region, at) {
                return;
            }
            for _ in 0..*steps {
                if target.test(run.region, at) {
                    emit(at);
                    return;
                }
                at.1 += step;
                if !run.region.height().contains(at.1) {
                    return;
                }
                if !allowed.test(run.region, at) {
                    break;
                }
            }
            if target.test(run.region, at) {
                emit(at);
            }
        }
        Modifier::Offset(ox, oy, oz) => {
            let dx = ox.sample(run.rng);
            let dy = oy.sample(run.rng);
            let dz = oz.sample(run.rng);
            if !run.outside(x + dx, z + dz, margin) {
                emit((x + dx, y + dy, z + dz));
            }
        }
        Modifier::Filter(predicate) => {
            if predicate.test(run.region, pos) {
                emit(pos);
            }
        }
        Modifier::NoiseBasedCount {
            ratio,
            factor,
            offset,
        } => {
            let noise = run
                .surface
                .info_noise(f64::from(x) / factor, f64::from(z) / factor);
            let count = ((noise + offset) * f64::from(*ratio)).ceil() as i32;
            for _ in 0..count {
                emit(pos);
            }
        }
        Modifier::NoiseThresholdCount {
            level,
            below,
            above,
        } => {
            let noise = run
                .surface
                .info_noise(f64::from(x) / 200.0, f64::from(z) / 200.0);
            let count = if noise < *level { *below } else { *above };
            for _ in 0..count {
                emit(pos);
            }
        }
        Modifier::SurfaceRelative { map, min, max } => {
            let height = i64::from(run.region.height_at(*map, x, z));
            let low = min.map_or(i64::MIN, |m| height + i64::from(m));
            let high = max.map_or(i64::MAX, |m| height + i64::from(m));
            if low <= i64::from(y) && i64::from(y) <= high {
                emit(pos);
            }
        }
        Modifier::RandomlySelected(options) => {
            if options.is_empty() {
                return;
            }
            let pick = run.rng.below(options.len() as i32) as usize;
            apply(&options[pick], run, pos, margin, emit);
        }
        Modifier::Fixed(positions) => positions.iter().for_each(|p| emit(*p)),
        Modifier::Cuboid {
            xz,
            y: ys,
            edges,
            interior,
        } => {
            let height = ys.sample(run.rng);
            let width = xz.sample(run.rng);
            let depth = xz.sample(run.rng);
            for dx in 0..=width {
                for dy in 0..=height {
                    for dz in 0..=depth {
                        let (end_x, end_y, end_z) = (
                            dx == 0 || dx == width,
                            dy == 0 || dy == height,
                            dz == 0 || dz == depth,
                        );
                        let ends = [end_x, end_y, end_z].iter().filter(|end| **end).count();
                        let kept = (*edges || ends < 2) && (*interior || ends > 0);
                        if kept {
                            emit((x + dx, y + dy, z + dz));
                        }
                    }
                }
            }
        }
        Modifier::Unsupported => {}
    }
}

fn ground_layer(run: &mut Run, x: i32, top: i32, z: i32, layer: i32) -> Option<i32> {
    let min_y = run.region.height().min_y;
    let mut remaining = layer;
    let mut state = run.region.get(x, top, z);
    let mut y = top;
    while y > min_y {
        let below = run.region.get(x, y - 1, z);
        let bedrock = below.kind() == Some(rspigot_blocks::BlockKind::Bedrock);
        if !is_empty(below) && !bedrock && is_empty(state) {
            if remaining == 0 {
                return Some(y);
            }
            remaining -= 1;
        }
        state = below;
        y -= 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overworld::features::tests::Bench;
    use crate::resources::BlockKind;

    fn cells(edges: bool, interior: bool) -> Vec<Pos> {
        let bench = Bench::new();
        let mut region = bench.floor(BlockKind::Stone, 64);
        let surface = SurfaceSystem::new(1, &bench.resources.blocks);
        let mut rng = Xoroshiro::from_seed(1);
        let mut run = Run {
            region: &mut region,
            rng: &mut rng,
            surface: &surface,
            feature_id: 0,
            seed: 1,
            biome_has: &[],
            universal: &[],
            interest: None,
        };
        let value = serde_json::json!({
            "type": "minecraft:cuboid",
            "xz_size": 2,
            "y_size": 2,
            "include_edges": edges,
            "include_interior": interior,
        });
        let modifier = Modifier::compile(&value);
        let mut found = Vec::new();
        apply(&modifier, &mut run, (10, 70, 20), 0, &mut |pos| {
            found.push(pos)
        });
        found
    }

    #[test]
    fn cuboid_spans_from_the_origin_corner() {
        let all = cells(true, true);
        assert_eq!(all.len(), 27);
        assert_eq!(all.first(), Some(&(10, 70, 20)));
        assert_eq!(all.last(), Some(&(12, 72, 22)));
    }

    #[test]
    fn cuboid_flags_pick_faces_edges_and_interior() {
        assert_eq!(cells(false, false).len(), 6);
        assert_eq!(cells(true, false).len(), 26);
        assert_eq!(cells(false, true).len(), 7);
    }
}
