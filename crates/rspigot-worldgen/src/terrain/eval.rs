use super::graph::{Binary, BlendedParameters, Graph, Id, NONE, Node, SplineValue, Unary};
use crate::climate::{NoiseStack, SmearedStack, Xoroshiro};
use crate::datapack::noise_parameters;

const LIMIT_FACTOR: f64 = 0.999_984_741_210_937_5;
const MAIN_FACTOR: f64 = 12.75;
const MULTIPLIER: f64 = 684.412;

#[derive(Debug, Clone)]
pub(crate) struct Blended {
    min: SmearedStack,
    max: SmearedStack,
    main: SmearedStack,
    xz: f64,
    y: f64,
    xz_main: f64,
    y_main: f64,
}

impl Blended {
    fn new(random: &mut Xoroshiro, params: BlendedParameters) -> Self {
        let (xz, y) = (params.xz_scale * MULTIPLIER, params.y_scale * MULTIPLIER);
        let smear = y * params.smear;
        Self {
            min: SmearedStack::create(random, -15, smear, LIMIT_FACTOR),
            max: SmearedStack::create(random, -15, smear, LIMIT_FACTOR),
            main: SmearedStack::create(random, -7, smear / params.y_factor, MAIN_FACTOR),
            xz,
            y,
            xz_main: xz / params.xz_factor,
            y_main: y / params.y_factor,
        }
    }

    fn get(&self, x: f64, y: f64, z: f64) -> f32 {
        let alpha = (self
            .main
            .get(x * self.xz_main, y * self.y_main, z * self.xz_main)
            + 0.5)
            .clamp(0.0, 1.0);
        let low = self.min.get(x * self.xz, y * self.y, z * self.xz);
        let high = self.max.get(x * self.xz, y * self.y, z * self.xz);
        if alpha == 0.0 {
            low
        } else if alpha == 1.0 {
            high
        } else {
            lerp(alpha, low, high)
        }
    }
}

pub(crate) fn lerp(t: f32, a: f32, b: f32) -> f32 {
    a + t * (b - a)
}

fn lerp2(tx: f32, ty: f32, a: f32, b: f32, c: f32, d: f32) -> f32 {
    lerp(ty, lerp(tx, a, b), lerp(tx, c, d))
}

#[derive(Debug, Clone)]
pub(crate) struct Compiled {
    pub(crate) graph: Graph,
    noises: Vec<NoiseStack>,
    blended: Vec<Blended>,
    y_dependent: Vec<bool>,
    cached: Vec<bool>,
}

impl Compiled {
    pub(crate) fn new(graph: Graph, seed: i64, roots: &[Id]) -> Self {
        let random = Xoroshiro::from_seed(seed).fork_positional();
        let noises = graph
            .noise_names
            .iter()
            .map(|name| {
                let parameters = noise_parameters(name)
                    .unwrap_or_else(|| panic!("noise {name} missing from the worldgen data"));
                NoiseStack::create(&parameters, &mut random.named(name))
            })
            .collect();
        let blended = graph
            .blended
            .iter()
            .map(|params| Blended::new(&mut random.named("minecraft:terrain"), *params))
            .collect();
        let y_dependent = graph.y_dependence();
        let parents = graph.parent_counts(roots);
        let cached = graph
            .nodes
            .iter()
            .enumerate()
            .map(|(i, node)| {
                !matches!(node, Node::Constant(_) | Node::Gradient { .. })
                    && (parents[i] > 1
                        || matches!(
                            node,
                            Node::Noise { .. }
                                | Node::ShiftA(_)
                                | Node::ShiftB(_)
                                | Node::Spline(_)
                                | Node::Blended(_)
                                | Node::Interpolated { .. }
                                | Node::FindTopSurface { .. }
                        ))
            })
            .collect();
        Self {
            graph,
            noises,
            blended,
            y_dependent,
            cached,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Volume {
    x0: i32,
    z0: i32,
    min_y: i32,
    height: i32,
}

impl Volume {
    fn contains(&self, x: i32, y: i32, z: i32) -> bool {
        (self.x0..self.x0 + 16).contains(&x)
            && (self.z0..self.z0 + 16).contains(&z)
            && (self.min_y..self.min_y + self.height).contains(&y)
    }

    fn index(&self, x: i32, y: i32, z: i32) -> usize {
        (((y - self.min_y) * 16 + (z - self.z0)) * 16 + (x - self.x0)) as usize
    }
}

#[derive(Debug, Clone, Copy)]
struct PointEntry {
    x: i32,
    y: i32,
    z: i32,
    value: f32,
    valid: bool,
}

#[derive(Debug, Clone, Copy)]
struct ColumnEntry {
    x: i32,
    z: i32,
    value: f32,
    valid: bool,
}

#[derive(Debug)]
pub(crate) struct Sampler<'a> {
    compiled: &'a Compiled,
    point: Vec<PointEntry>,
    column: Vec<ColumnEntry>,
    volume: Option<Volume>,
    dense: Vec<Option<Box<[f32]>>>,
}

impl<'a> Sampler<'a> {
    pub(crate) fn new(compiled: &'a Compiled) -> Self {
        let n = compiled.graph.nodes.len();
        Self {
            compiled,
            point: vec![
                PointEntry {
                    x: 0,
                    y: 0,
                    z: 0,
                    value: 0.0,
                    valid: false
                };
                n
            ],
            column: vec![
                ColumnEntry {
                    x: 0,
                    z: 0,
                    value: 0.0,
                    valid: false
                };
                n
            ],
            volume: None,
            dense: vec![None; n],
        }
    }

    pub(crate) fn begin_chunk(&mut self, x0: i32, z0: i32, min_y: i32, height: i32) {
        self.volume = Some(Volume {
            x0,
            z0,
            min_y,
            height,
        });
        self.dense.iter_mut().for_each(|slot| *slot = None);
    }

    pub(crate) fn eval(&mut self, id: Id, x: i32, y: i32, z: i32) -> f32 {
        let i = id as usize;
        if !self.compiled.cached[i] {
            return self.compute(i, x, y, z);
        }
        if self.compiled.y_dependent[i] {
            let entry = self.point[i];
            if entry.valid && entry.x == x && entry.y == y && entry.z == z {
                return entry.value;
            }
            let value = self.compute(i, x, y, z);
            self.point[i] = PointEntry {
                x,
                y,
                z,
                value,
                valid: true,
            };
            value
        } else {
            let entry = self.column[i];
            if entry.valid && entry.x == x && entry.z == z {
                return entry.value;
            }
            let value = self.compute(i, x, y, z);
            self.column[i] = ColumnEntry {
                x,
                z,
                value,
                valid: true,
            };
            value
        }
    }

    fn compute(&mut self, i: usize, x: i32, y: i32, z: i32) -> f32 {
        let compiled = self.compiled;
        match &compiled.graph.nodes[i] {
            Node::Constant(value) => *value,
            Node::Noise {
                noise,
                xz_scale,
                y_scale,
                shift_x,
                shift_z,
            } => {
                let mut nx = f64::from(x) * xz_scale;
                let mut nz = f64::from(z) * xz_scale;
                if *shift_x != NONE {
                    nx += f64::from(self.eval(*shift_x, x, y, z));
                }
                if *shift_z != NONE {
                    nz += f64::from(self.eval(*shift_z, x, y, z));
                }
                compiled.noises[*noise].get(nx, f64::from(y) * y_scale, nz)
            }
            Node::ShiftA(noise) => {
                compiled.noises[*noise].get(
                    f64::from(x) * 0.25,
                    f64::from(y) * 0.0,
                    f64::from(z) * 0.25,
                ) * 4.0
            }
            Node::ShiftB(noise) => {
                compiled.noises[*noise].get(f64::from(z) * 0.25, f64::from(x) * 0.25, 0.0) * 4.0
            }
            Node::Gradient {
                from,
                min,
                max,
                from_value,
                factor,
            } => *from_value + (y.clamp(*min, *max) - *from) as f32 * *factor,
            Node::Binary(op, a, b) => {
                let left = self.eval(*a, x, y, z);
                let right = self.eval(*b, x, y, z);
                match op {
                    Binary::Add => left + right,
                    Binary::Sub => left - right,
                    Binary::Mul => left * right,
                    Binary::Div => left / right,
                    Binary::Min => left.min(right),
                    Binary::Max => left.max(right),
                }
            }
            Node::Unary(op, a) => {
                let v = self.eval(*a, x, y, z);
                match op {
                    Unary::Abs => v.abs(),
                    Unary::Square => v * v,
                    Unary::Cube => v * v * v,
                    Unary::HalfNegative => {
                        if v > 0.0 {
                            v
                        } else {
                            v * 0.5
                        }
                    }
                    Unary::QuarterNegative => {
                        if v > 0.0 {
                            v
                        } else {
                            v * 0.25
                        }
                    }
                    Unary::Negate => -v,
                    Unary::Squeeze => {
                        let c = v.clamp(-1.0, 1.0);
                        c / 2.0 - c * c * c / 24.0
                    }
                }
            }
            Node::Clamp { input, min, max } => self.eval(*input, x, y, z).clamp(*min, *max),
            Node::Lerp {
                alpha,
                first,
                second,
            } => {
                let a = self.eval(*alpha, x, y, z);
                if a == 0.0 {
                    self.eval(*first, x, y, z)
                } else if a == 1.0 {
                    self.eval(*second, x, y, z)
                } else {
                    let f = self.eval(*first, x, y, z);
                    let s = self.eval(*second, x, y, z);
                    lerp(a, f, s)
                }
            }
            Node::Spline(spline) => self.spline(*spline, x, y, z),
            Node::RangeChoice {
                input,
                min,
                max,
                inside,
                outside,
            } => {
                let v = self.eval(*input, x, y, z);
                if *min <= v && v < *max {
                    self.eval(*inside, x, y, z)
                } else {
                    self.eval(*outside, x, y, z)
                }
            }
            Node::IntervalSelect {
                input,
                thresholds,
                branches,
            } => {
                let v = self.eval(*input, x, y, z);
                let index = thresholds
                    .iter()
                    .position(|t| v < *t)
                    .unwrap_or(branches.len() - 1);
                self.eval(branches[index], x, y, z)
            }
            Node::Interpolated {
                input,
                cell_xz,
                cell_y,
            } => self.interpolated(i, *input, *cell_xz, *cell_y, x, y, z),
            Node::FindTopSurface {
                density,
                upper,
                lower,
                cell,
            } => {
                let bound = self.eval(*upper, x, y, z);
                let mut level = (bound / *cell as f32).floor() as i32 * *cell;
                if level <= *lower {
                    return *lower as f32;
                }
                loop {
                    if level < *lower {
                        return *lower as f32;
                    }
                    if self.eval(*density, x, level, z) > 0.0 {
                        return level as f32;
                    }
                    level -= *cell;
                }
            }
            Node::Blended(index) => {
                compiled.blended[*index].get(f64::from(x), f64::from(y), f64::from(z))
            }
        }
    }

    fn spline(&mut self, spline: usize, x: i32, y: i32, z: i32) -> f32 {
        let compiled = self.compiled;
        let spec = &compiled.graph.splines[spline];
        let input = self.eval(spec.coordinate, x, y, z);
        let points = &spec.points;
        let last = points.len() - 1;
        let start = points.partition_point(|p| p.location <= input) as i32 - 1;
        let extend = |value: f32, index: usize| {
            let derivative = points[index].derivative;
            if derivative == 0.0 {
                value
            } else {
                value + derivative * (input - points[index].location)
            }
        };
        if start < 0 {
            let value = self.spline_value(points[0].value, x, y, z);
            return extend(value, 0);
        }
        let start = start as usize;
        if start == last {
            let value = self.spline_value(points[last].value, x, y, z);
            return extend(value, last);
        }
        let (x0, x1) = (points[start].location, points[start + 1].location);
        let t = (input - x0) / (x1 - x0);
        let v0 = self.spline_value(points[start].value, x, y, z);
        let v1 = self.spline_value(points[start + 1].value, x, y, z);
        let (d0, d1) = (points[start].derivative, points[start + 1].derivative);
        let a = d0 * (x1 - x0) - (v1 - v0);
        let b = -d1 * (x1 - x0) + (v1 - v0);
        lerp(t, v0, v1) + t * (1.0 - t) * lerp(t, a, b)
    }

    fn spline_value(&mut self, value: SplineValue, x: i32, y: i32, z: i32) -> f32 {
        match value {
            SplineValue::Constant(v) => v,
            SplineValue::Nested(inner) => self.spline(inner, x, y, z),
        }
    }

    fn interpolated(
        &mut self,
        node: usize,
        input: Id,
        cell_xz: i32,
        cell_y: i32,
        x: i32,
        y: i32,
        z: i32,
    ) -> f32 {
        let (mx, my, mz) = (
            x.rem_euclid(cell_xz),
            y.rem_euclid(cell_y),
            z.rem_euclid(cell_xz),
        );
        if mx == 0 && my == 0 && mz == 0 {
            return self.eval(input, x, y, z);
        }
        if let Some(volume) = self.volume
            && volume.contains(x, y, z)
        {
            if self.dense[node].is_none() {
                let grid = self.fill_dense(input, cell_xz, cell_y, volume);
                self.dense[node] = Some(grid);
            }
            if let Some(grid) = &self.dense[node] {
                return grid[volume.index(x, y, z)];
            }
        }
        let (bx, by, bz) = (x - mx, y - my, z - mz);
        let mut corner = |dx: i32, dy: i32, dz: i32| {
            self.eval(
                input,
                bx + dx * cell_xz,
                by + dy * cell_y,
                bz + dz * cell_xz,
            )
        };
        let c000 = corner(0, 0, 0);
        let c100 = corner(1, 0, 0);
        let c010 = corner(0, 1, 0);
        let c110 = corner(1, 1, 0);
        let c001 = corner(0, 0, 1);
        let c101 = corner(1, 0, 1);
        let c011 = corner(0, 1, 1);
        let c111 = corner(1, 1, 1);
        let (fx, fy, fz) = (
            mx as f32 / cell_xz as f32,
            my as f32 / cell_y as f32,
            mz as f32 / cell_xz as f32,
        );
        lerp(
            fz,
            lerp2(fx, fy, c000, c100, c010, c110),
            lerp2(fx, fy, c001, c101, c011, c111),
        )
    }

    fn fill_dense(&mut self, input: Id, cell_xz: i32, cell_y: i32, volume: Volume) -> Box<[f32]> {
        let nx = (16 / cell_xz + 1) as usize;
        let ny = (volume.height / cell_y + 1) as usize;
        let mut corners = vec![0.0f32; nx * nx * ny];
        for k in 0..nx {
            for i in 0..nx {
                for j in 0..ny {
                    corners[(j * nx + k) * nx + i] = self.eval(
                        input,
                        volume.x0 + i as i32 * cell_xz,
                        volume.min_y + j as i32 * cell_y,
                        volume.z0 + k as i32 * cell_xz,
                    );
                }
            }
        }
        let at = |i: usize, j: usize, k: usize| corners[(j * nx + k) * nx + i];
        let inv_xz = 1.0f32 / cell_xz as f32;
        let inv_y = 1.0f32 / cell_y as f32;
        let mut out = vec![0.0f32; (16 * 16 * volume.height) as usize].into_boxed_slice();
        for k in 0..nx - 1 {
            for i in 0..nx - 1 {
                for j in 0..ny - 1 {
                    let v000 = at(i, j, k);
                    let v100 = at(i + 1, j, k);
                    let v010 = at(i, j + 1, k);
                    let v110 = at(i + 1, j + 1, k);
                    let v001 = at(i, j, k + 1);
                    let v101 = at(i + 1, j, k + 1);
                    let v011 = at(i, j + 1, k + 1);
                    let v111 = at(i + 1, j + 1, k + 1);
                    for zz in 0..cell_xz {
                        let tz = zz as f32 * inv_xz;
                        let a00 = lerp(tz, v000, v001);
                        let a01 = lerp(tz, v010, v011);
                        let a10 = lerp(tz, v100, v101);
                        let a11 = lerp(tz, v110, v111);
                        for xx in 0..cell_xz {
                            let tx = xx as f32 * inv_xz;
                            let bottom = lerp(tx, a00, a10);
                            let top = lerp(tx, a01, a11);
                            let step = (top - bottom) * inv_y;
                            let mut value = bottom;
                            for yy in 0..cell_y {
                                let wx = i as i32 * cell_xz + xx;
                                let wy = j as i32 * cell_y + yy;
                                let wz = k as i32 * cell_xz + zz;
                                out[((wy * 16 + wz) * 16 + wx) as usize] = value;
                                value += step;
                            }
                        }
                    }
                }
            }
        }
        out
    }
}
