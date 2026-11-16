use std::collections::HashMap;

use serde_json::Value;

pub(crate) type Id = u32;
pub(crate) const NONE: Id = u32::MAX;

#[derive(Debug, thiserror::Error)]
pub(crate) enum GraphError {
    #[error("density function {0} is missing from the datapack")]
    MissingFunction(String),
    #[error("unsupported density function type {0}")]
    Unsupported(String),
    #[error("density function field {0} is missing or malformed")]
    Field(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Binary {
    Add,
    Sub,
    Mul,
    Div,
    Min,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unary {
    Abs,
    Square,
    Cube,
    HalfNegative,
    QuarterNegative,
    Negate,
    Squeeze,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BlendedParameters {
    pub(crate) xz_scale: f64,
    pub(crate) y_scale: f64,
    pub(crate) xz_factor: f64,
    pub(crate) y_factor: f64,
    pub(crate) smear: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Node {
    Constant(f32),
    Noise {
        noise: usize,
        xz_scale: f64,
        y_scale: f64,
        shift_x: Id,
        shift_z: Id,
    },
    ShiftA(usize),
    ShiftB(usize),
    Gradient {
        from: i32,
        min: i32,
        max: i32,
        from_value: f32,
        factor: f32,
    },
    Binary(Binary, Id, Id),
    Unary(Unary, Id),
    Clamp {
        input: Id,
        min: f32,
        max: f32,
    },
    Lerp {
        alpha: Id,
        first: Id,
        second: Id,
    },
    Spline(usize),
    RangeChoice {
        input: Id,
        min: f32,
        max: f32,
        inside: Id,
        outside: Id,
    },
    IntervalSelect {
        input: Id,
        thresholds: Vec<f32>,
        branches: Vec<Id>,
    },
    Interpolated {
        input: Id,
        cell_xz: i32,
        cell_y: i32,
    },
    FindTopSurface {
        density: Id,
        upper: Id,
        lower: i32,
        cell: i32,
    },
    Blended(usize),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum SplineValue {
    Constant(f32),
    Nested(usize),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SplinePoint {
    pub(crate) location: f32,
    pub(crate) derivative: f32,
    pub(crate) value: SplineValue,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Spline {
    pub(crate) coordinate: Id,
    pub(crate) points: Vec<SplinePoint>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Graph {
    pub(crate) nodes: Vec<Node>,
    pub(crate) splines: Vec<Spline>,
    pub(crate) noise_names: Vec<String>,
    pub(crate) blended: Vec<BlendedParameters>,
}

pub(crate) struct Compiler<'a> {
    functions: &'a Value,
    graph: Graph,
    named: HashMap<String, Id>,
    noise_index: HashMap<String, usize>,
}

fn f32_of(value: &Value, key: &'static str) -> Result<f32, GraphError> {
    value
        .get(key)
        .and_then(Value::as_f64)
        .map(|v| v as f32)
        .ok_or(GraphError::Field(key))
}

fn f64_of(value: &Value, key: &'static str) -> Result<f64, GraphError> {
    value
        .get(key)
        .and_then(Value::as_f64)
        .ok_or(GraphError::Field(key))
}

fn i32_of(value: &Value, key: &'static str) -> Result<i32, GraphError> {
    value
        .get(key)
        .and_then(Value::as_i64)
        .map(|v| v as i32)
        .ok_or(GraphError::Field(key))
}

fn field<'v>(value: &'v Value, key: &'static str) -> Result<&'v Value, GraphError> {
    value.get(key).ok_or(GraphError::Field(key))
}

impl<'a> Compiler<'a> {
    pub(crate) fn new(functions: &'a Value) -> Self {
        Self {
            functions,
            graph: Graph::default(),
            named: HashMap::new(),
            noise_index: HashMap::new(),
        }
    }

    pub(crate) fn finish(self) -> Graph {
        self.graph
    }

    fn push(&mut self, node: Node) -> Id {
        self.graph.nodes.push(node);
        (self.graph.nodes.len() - 1) as Id
    }

    fn noise(&mut self, name: &str) -> usize {
        if let Some(index) = self.noise_index.get(name) {
            return *index;
        }
        let index = self.graph.noise_names.len();
        self.graph.noise_names.push(name.to_owned());
        self.noise_index.insert(name.to_owned(), index);
        index
    }

    pub(crate) fn build(&mut self, value: &Value) -> Result<Id, GraphError> {
        match value {
            Value::String(name) => self.reference(name),
            Value::Number(number) => Ok(self.push(Node::Constant(
                number.as_f64().ok_or(GraphError::Field("constant"))? as f32,
            ))),
            Value::Object(_) => self.object(value),
            _ => Err(GraphError::Field("function")),
        }
    }

    fn reference(&mut self, name: &str) -> Result<Id, GraphError> {
        if let Some(id) = self.named.get(name) {
            return Ok(*id);
        }
        let body = self
            .functions
            .get(name)
            .ok_or_else(|| GraphError::MissingFunction(name.to_owned()))?;
        let id = self.build(body)?;
        self.named.insert(name.to_owned(), id);
        Ok(id)
    }

    fn binary(&mut self, op: Binary, value: &Value) -> Result<Id, GraphError> {
        let left = self.build(field(value, "left")?)?;
        let right = self.build(field(value, "right")?)?;
        Ok(self.push(Node::Binary(op, left, right)))
    }

    fn unary(&mut self, op: Unary, value: &Value) -> Result<Id, GraphError> {
        let input = self.build(field(value, "input")?)?;
        Ok(self.push(Node::Unary(op, input)))
    }

    fn object(&mut self, value: &Value) -> Result<Id, GraphError> {
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .ok_or(GraphError::Field("type"))?;
        let kind = kind.strip_prefix("minecraft:").unwrap_or(kind);
        match kind {
            "cache" | "blend_density" => self.build(field(value, "input")?),
            "constant" => Ok(self.push(Node::Constant(f32_of(value, "argument")?))),
            "blend_alpha" => Ok(self.push(Node::Constant(1.0))),
            "blend_offset" | "beardifier" => Ok(self.push(Node::Constant(0.0))),
            "noise" => {
                let name = field(value, "noise")?
                    .as_str()
                    .ok_or(GraphError::Field("noise"))?;
                let noise = self.noise(name);
                let shift_x = match value.get("shift_x") {
                    Some(shift) => self.build(shift)?,
                    None => NONE,
                };
                let shift_z = match value.get("shift_z") {
                    Some(shift) => self.build(shift)?,
                    None => NONE,
                };
                Ok(self.push(Node::Noise {
                    noise,
                    xz_scale: f64_of(value, "xz_scale")?,
                    y_scale: f64_of(value, "y_scale")?,
                    shift_x,
                    shift_z,
                }))
            }
            "shift_a" | "shift_b" => {
                let name = field(value, "noise")?
                    .as_str()
                    .ok_or(GraphError::Field("noise"))?;
                let noise = self.noise(name);
                Ok(self.push(if kind == "shift_a" {
                    Node::ShiftA(noise)
                } else {
                    Node::ShiftB(noise)
                }))
            }
            "gradient" => {
                let from = i32_of(value, "from_coordinate")?;
                let to = i32_of(value, "to_coordinate")?;
                let from_value = f32_of(value, "from_value")?;
                let to_value = f32_of(value, "to_value")?;
                Ok(self.push(Node::Gradient {
                    from,
                    min: from.min(to),
                    max: from.max(to),
                    from_value,
                    factor: (to_value - from_value) / (to - from) as f32,
                }))
            }
            "add" => self.binary(Binary::Add, value),
            "sub" => self.binary(Binary::Sub, value),
            "mul" => self.binary(Binary::Mul, value),
            "div" => self.binary(Binary::Div, value),
            "min" => self.binary(Binary::Min, value),
            "max" => self.binary(Binary::Max, value),
            "abs" => self.unary(Unary::Abs, value),
            "square" => self.unary(Unary::Square, value),
            "cube" => self.unary(Unary::Cube, value),
            "half_negative" => self.unary(Unary::HalfNegative, value),
            "quarter_negative" => self.unary(Unary::QuarterNegative, value),
            "negate" => self.unary(Unary::Negate, value),
            "squeeze" => self.unary(Unary::Squeeze, value),
            "clamp" => {
                let input = self.build(field(value, "input")?)?;
                Ok(self.push(Node::Clamp {
                    input,
                    min: f32_of(value, "min")?,
                    max: f32_of(value, "max")?,
                }))
            }
            "lerp" => {
                let alpha = self.build(field(value, "alpha")?)?;
                let first = self.build(field(value, "first")?)?;
                let second = self.build(field(value, "second")?)?;
                Ok(self.push(Node::Lerp {
                    alpha,
                    first,
                    second,
                }))
            }
            "spline" => {
                let spline = self.spline(field(value, "spline")?)?;
                Ok(self.push(Node::Spline(spline)))
            }
            "range_choice" => {
                let input = self.build(field(value, "input")?)?;
                let inside = self.build(field(value, "when_in_range")?)?;
                let outside = self.build(field(value, "when_out_of_range")?)?;
                Ok(self.push(Node::RangeChoice {
                    input,
                    min: f32_of(value, "min_inclusive")?,
                    max: f32_of(value, "max_exclusive")?,
                    inside,
                    outside,
                }))
            }
            "interval_select" => {
                let input = self.build(field(value, "input")?)?;
                let thresholds = field(value, "thresholds")?
                    .as_array()
                    .ok_or(GraphError::Field("thresholds"))?
                    .iter()
                    .map(|v| v.as_f64().map(|v| v as f32))
                    .collect::<Option<Vec<f32>>>()
                    .ok_or(GraphError::Field("thresholds"))?;
                let mut branches = Vec::new();
                for function in field(value, "functions")?
                    .as_array()
                    .ok_or(GraphError::Field("functions"))?
                {
                    branches.push(self.build(function)?);
                }
                Ok(self.push(Node::IntervalSelect {
                    input,
                    thresholds,
                    branches,
                }))
            }
            "interpolated" => {
                let input = self.build(field(value, "input")?)?;
                Ok(self.push(Node::Interpolated {
                    input,
                    cell_xz: i32_of(value, "cell_size_xz")?,
                    cell_y: i32_of(value, "cell_size_y")?,
                }))
            }
            "find_top_surface" => {
                let density = self.build(field(value, "density")?)?;
                let upper = self.build(field(value, "upper_bound")?)?;
                Ok(self.push(Node::FindTopSurface {
                    density,
                    upper,
                    lower: i32_of(value, "lower_bound")?,
                    cell: i32_of(value, "cell_height")?,
                }))
            }
            "old_blended_noise" => {
                let index = self.graph.blended.len();
                self.graph.blended.push(BlendedParameters {
                    xz_scale: f64_of(value, "xz_scale")?,
                    y_scale: f64_of(value, "y_scale")?,
                    xz_factor: f64_of(value, "xz_factor")?,
                    y_factor: f64_of(value, "y_factor")?,
                    smear: f64_of(value, "smear_scale_multiplier")?,
                });
                Ok(self.push(Node::Blended(index)))
            }
            other => Err(GraphError::Unsupported(other.to_owned())),
        }
    }

    fn spline(&mut self, value: &Value) -> Result<usize, GraphError> {
        let coordinate = self.build(field(value, "coordinate")?)?;
        let raw = field(value, "points")?
            .as_array()
            .ok_or(GraphError::Field("points"))?;
        let mut points = Vec::with_capacity(raw.len());
        for point in raw {
            let value = field(point, "value")?;
            let value = match value {
                Value::Object(_) => SplineValue::Nested(self.spline(value)?),
                other => {
                    SplineValue::Constant(other.as_f64().ok_or(GraphError::Field("value"))? as f32)
                }
            };
            points.push(SplinePoint {
                location: f32_of(point, "location")?,
                derivative: f32_of(point, "derivative")?,
                value,
            });
        }
        self.graph.splines.push(Spline { coordinate, points });
        Ok(self.graph.splines.len() - 1)
    }
}

impl Graph {
    pub(crate) fn y_dependence(&self) -> Vec<bool> {
        let mut flags = vec![false; self.nodes.len()];
        let spline_flag = |flags: &[bool], splines: &[Spline], spline: usize| -> bool {
            fn walk(flags: &[bool], splines: &[Spline], spline: usize) -> bool {
                let spec = &splines[spline];
                flags[spec.coordinate as usize]
                    || spec.points.iter().any(|p| match p.value {
                        SplineValue::Constant(_) => false,
                        SplineValue::Nested(inner) => walk(flags, splines, inner),
                    })
            }
            walk(flags, splines, spline)
        };
        for index in 0..self.nodes.len() {
            let any = |flags: &[bool], ids: &[Id]| {
                ids.iter().any(|id| *id != NONE && flags[*id as usize])
            };
            let value = match &self.nodes[index] {
                Node::Constant(_) | Node::ShiftA(_) | Node::ShiftB(_) => false,
                Node::Noise {
                    y_scale,
                    shift_x,
                    shift_z,
                    ..
                } => *y_scale != 0.0 || any(&flags, &[*shift_x, *shift_z]),
                Node::Gradient { .. } | Node::Interpolated { .. } | Node::Blended(_) => true,
                Node::Binary(_, a, b) => any(&flags, &[*a, *b]),
                Node::Unary(_, a) => any(&flags, &[*a]),
                Node::Clamp { input, .. } => any(&flags, &[*input]),
                Node::Lerp {
                    alpha,
                    first,
                    second,
                } => any(&flags, &[*alpha, *first, *second]),
                Node::Spline(spline) => spline_flag(&flags, &self.splines, *spline),
                Node::RangeChoice {
                    input,
                    inside,
                    outside,
                    ..
                } => any(&flags, &[*input, *inside, *outside]),
                Node::IntervalSelect {
                    input, branches, ..
                } => any(&flags, &[*input]) || any(&flags, branches),
                Node::FindTopSurface { upper, .. } => any(&flags, &[*upper]),
            };
            flags[index] = value;
        }
        flags
    }

    pub(crate) fn parent_counts(&self, roots: &[Id]) -> Vec<u32> {
        let mut counts = vec![0u32; self.nodes.len()];
        let mut bump = |id: Id| {
            if id != NONE {
                counts[id as usize] += 1;
            }
        };
        for root in roots {
            bump(*root);
        }
        for node in &self.nodes {
            match node {
                Node::Constant(_)
                | Node::ShiftA(_)
                | Node::ShiftB(_)
                | Node::Gradient { .. }
                | Node::Blended(_) => {}
                Node::Noise {
                    shift_x, shift_z, ..
                } => {
                    bump(*shift_x);
                    bump(*shift_z);
                }
                Node::Binary(_, a, b) => {
                    bump(*a);
                    bump(*b);
                }
                Node::Unary(_, a) => bump(*a),
                Node::Clamp { input, .. } | Node::Interpolated { input, .. } => bump(*input),
                Node::Lerp {
                    alpha,
                    first,
                    second,
                } => {
                    bump(*alpha);
                    bump(*first);
                    bump(*second);
                }
                Node::Spline(_) => {}
                Node::RangeChoice {
                    input,
                    inside,
                    outside,
                    ..
                } => {
                    bump(*input);
                    bump(*inside);
                    bump(*outside);
                }
                Node::IntervalSelect {
                    input, branches, ..
                } => {
                    bump(*input);
                    for branch in branches {
                        bump(*branch);
                    }
                }
                Node::FindTopSurface { density, upper, .. } => {
                    bump(*density);
                    bump(*upper);
                }
            }
        }
        for spline in &self.splines {
            bump(spline.coordinate);
        }
        counts
    }
}
