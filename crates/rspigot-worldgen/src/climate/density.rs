use rspigot_data::multi_noise::{DensityGraph, DensityNode, NO_NODE, SplineValue};

use super::noise::NoiseStack;
use super::random::Xoroshiro;

#[derive(Debug, Clone)]
pub(crate) struct ClimateFunctions {
    graph: DensityGraph,
    noises: Vec<NoiseStack>,
    uses_y: Vec<bool>,
}

#[derive(Debug, Clone)]
pub(crate) struct Scratch {
    column: Vec<f32>,
    point: Vec<f32>,
    column_key: Option<(i32, i32)>,
}

impl Scratch {
    fn new(nodes: usize) -> Self {
        Self {
            column: vec![f32::NAN; nodes],
            point: vec![f32::NAN; nodes],
            column_key: None,
        }
    }

    fn enter(&mut self, x: i32, z: i32) {
        if self.column_key != Some((x, z)) {
            self.column.fill(f32::NAN);
            self.column_key = Some((x, z));
        }
        self.point.fill(f32::NAN);
    }
}

fn lerp(t: f32, a: f32, b: f32) -> f32 {
    a + t * (b - a)
}

impl ClimateFunctions {
    pub(crate) fn new(graph: DensityGraph, seed: i64) -> Self {
        let random = Xoroshiro::from_seed(seed).fork_positional();
        let noises = graph
            .noises
            .iter()
            .map(|parameters| {
                NoiseStack::create(parameters, &mut random.from_hash_of(parameters.id))
            })
            .collect();
        let mut uses_y = vec![false; graph.nodes.len()];
        for index in 0..graph.nodes.len() {
            uses_y[index] = Self::depends_on_y(&graph, index as u16);
        }
        Self {
            graph,
            noises,
            uses_y,
        }
    }

    fn depends_on_y(graph: &DensityGraph, node: u16) -> bool {
        let sub = |n: u16| n != NO_NODE && Self::depends_on_y(graph, n);
        match graph.nodes[usize::from(node)] {
            DensityNode::Constant(_) | DensityNode::ShiftB { .. } => false,
            DensityNode::ShiftA { .. } => false,
            DensityNode::Noise {
                y_scale,
                shift_x,
                shift_z,
                ..
            } => y_scale != 0.0 || sub(shift_x) || sub(shift_z),
            DensityNode::Gradient { .. } => true,
            DensityNode::Add { left, right } | DensityNode::Mul { left, right } => {
                sub(left) || sub(right)
            }
            DensityNode::Abs { input } => sub(input),
            DensityNode::Lerp {
                alpha,
                first,
                second,
            } => sub(alpha) || sub(first) || sub(second),
            DensityNode::Spline { spline } => {
                let spec = graph.splines[usize::from(spline)];
                sub(spec.coordinate)
                    || (spec.start..spec.start + spec.length).any(|i| {
                        match graph.spline_points[i as usize].value {
                            SplineValue::Constant(_) => false,
                            SplineValue::Nested(inner) => Self::spline_uses_y(graph, inner),
                        }
                    })
            }
        }
    }

    fn spline_uses_y(graph: &DensityGraph, spline: u16) -> bool {
        let spec = graph.splines[usize::from(spline)];
        Self::depends_on_y(graph, spec.coordinate)
            || (spec.start..spec.start + spec.length).any(|i| {
                match graph.spline_points[i as usize].value {
                    SplineValue::Constant(_) => false,
                    SplineValue::Nested(inner) => Self::spline_uses_y(graph, inner),
                }
            })
    }

    pub(crate) fn scratch(&self) -> Scratch {
        Scratch::new(self.graph.nodes.len())
    }

    pub(crate) fn sample(&self, scratch: &mut Scratch, x: i32, y: i32, z: i32) -> [f32; 6] {
        scratch.enter(x, z);
        self.graph
            .router
            .map(|node| self.eval(scratch, node, x, y, z))
    }

    fn eval(&self, scratch: &mut Scratch, node: u16, x: i32, y: i32, z: i32) -> f32 {
        let index = usize::from(node);
        let cached = if self.uses_y[index] {
            scratch.point[index]
        } else {
            scratch.column[index]
        };
        if !cached.is_nan() {
            return cached;
        }
        let value = self.compute(scratch, node, x, y, z);
        if self.uses_y[index] {
            scratch.point[index] = value;
        } else {
            scratch.column[index] = value;
        }
        value
    }

    fn compute(&self, scratch: &mut Scratch, node: u16, x: i32, y: i32, z: i32) -> f32 {
        match self.graph.nodes[usize::from(node)] {
            DensityNode::Constant(value) => value,
            DensityNode::Noise {
                noise,
                xz_scale,
                y_scale,
                shift_x,
                shift_z,
            } => {
                let mut nx = f64::from(x) * xz_scale;
                let mut nz = f64::from(z) * xz_scale;
                if shift_x != NO_NODE {
                    nx += f64::from(self.eval(scratch, shift_x, x, y, z));
                }
                if shift_z != NO_NODE {
                    nz += f64::from(self.eval(scratch, shift_z, x, y, z));
                }
                self.noises[usize::from(noise)].get(nx, f64::from(y) * y_scale, nz)
            }
            DensityNode::ShiftA { noise } => {
                self.noises[usize::from(noise)].get(
                    f64::from(x) * 0.25,
                    f64::from(y) * 0.0,
                    f64::from(z) * 0.25,
                ) * 4.0
            }
            DensityNode::ShiftB { noise } => {
                self.noises[usize::from(noise)].get(f64::from(z) * 0.25, f64::from(x) * 0.25, 0.0)
                    * 4.0
            }
            DensityNode::Gradient {
                from,
                min,
                max,
                from_value,
                factor,
            } => from_value + (y.clamp(min, max) - from) as f32 * factor,
            DensityNode::Add { left, right } => {
                self.eval(scratch, left, x, y, z) + self.eval(scratch, right, x, y, z)
            }
            DensityNode::Mul { left, right } => {
                self.eval(scratch, left, x, y, z) * self.eval(scratch, right, x, y, z)
            }
            DensityNode::Abs { input } => self.eval(scratch, input, x, y, z).abs(),
            DensityNode::Lerp {
                alpha,
                first,
                second,
            } => {
                let alpha = self.eval(scratch, alpha, x, y, z);
                if alpha == 0.0 {
                    self.eval(scratch, first, x, y, z)
                } else if alpha == 1.0 {
                    self.eval(scratch, second, x, y, z)
                } else {
                    let a = self.eval(scratch, first, x, y, z);
                    let b = self.eval(scratch, second, x, y, z);
                    lerp(alpha, a, b)
                }
            }
            DensityNode::Spline { spline } => self.spline(scratch, spline, x, y, z),
        }
    }

    fn spline_value(
        &self,
        scratch: &mut Scratch,
        value: SplineValue,
        x: i32,
        y: i32,
        z: i32,
    ) -> f32 {
        match value {
            SplineValue::Constant(v) => v,
            SplineValue::Nested(inner) => self.spline(scratch, inner, x, y, z),
        }
    }

    fn spline(&self, scratch: &mut Scratch, spline: u16, x: i32, y: i32, z: i32) -> f32 {
        let spec = self.graph.splines[usize::from(spline)];
        let points =
            &self.graph.spline_points[spec.start as usize..(spec.start + spec.length) as usize];
        let input = self.eval(scratch, spec.coordinate, x, y, z);
        let last = points.len() - 1;
        let start = points.partition_point(|p| !(input < p.location));
        let start = start as i32 - 1;
        let extend = |value: f32, index: usize| {
            let derivative = points[index].derivative;
            if derivative == 0.0 {
                value
            } else {
                value + derivative * (input - points[index].location)
            }
        };
        if start < 0 {
            let value = self.spline_value(scratch, points[0].value, x, y, z);
            return extend(value, 0);
        }
        let start = start as usize;
        if start == last {
            let value = self.spline_value(scratch, points[last].value, x, y, z);
            return extend(value, last);
        }
        let (x0, x1) = (points[start].location, points[start + 1].location);
        let t = (input - x0) / (x1 - x0);
        let v0 = self.spline_value(scratch, points[start].value, x, y, z);
        let v1 = self.spline_value(scratch, points[start + 1].value, x, y, z);
        let (d0, d1) = (points[start].derivative, points[start + 1].derivative);
        let a = d0 * (x1 - x0) - (v1 - v0);
        let b = -d1 * (x1 - x0) + (v1 - v0);
        lerp(t, v0, v1) + t * (1.0 - t) * lerp(t, a, b)
    }
}
