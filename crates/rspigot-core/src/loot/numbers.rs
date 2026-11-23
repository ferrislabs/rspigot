use rand::Rng;
use rspigot_data::loot::{
    self, Aggregate, AttributeValue, Binary, FloatExpr, IntExpr, ScoreHolder, Unary,
};
use rspigot_nbt::{NbtPath, Tag};

use super::{MAX_NESTING, Roller};

const SIN_SCALE: f32 = 10_430.378;
const SIN_STEPS: i32 = 65_536;
const QUARTER_TURN: f32 = 16_384.0;

fn sine_of(index: i32) -> f32 {
    let step = f64::from(index & (SIN_STEPS - 1));
    (step * std::f64::consts::PI * 2.0 / f64::from(SIN_STEPS)).sin() as f32
}

fn sin(value: f32) -> f32 {
    sine_of((value * SIN_SCALE) as i32)
}

fn cos(value: f32) -> f32 {
    sine_of((value * SIN_SCALE + QUARTER_TURN) as i32)
}

fn rounded(value: f32) -> Option<i32> {
    if !value.is_finite() {
        return None;
    }
    i32::try_from((f64::from(value) + 0.5).floor() as i64).ok()
}

fn floor_quotient(left: i32, right: i32) -> Option<i32> {
    if right == 0 {
        return None;
    }
    let (left, right) = (i64::from(left), i64::from(right));
    let mut quotient = left / right;
    if left % right != 0 && ((left < 0) != (right < 0)) {
        quotient -= 1;
    }
    i32::try_from(quotient).ok()
}

fn floor_modulus(left: i32, right: i32) -> Option<i32> {
    let quotient = i64::from(floor_quotient(left, right)?);
    i32::try_from(i64::from(left) - quotient * i64::from(right)).ok()
}

fn power(base: i32, exponent: i32) -> Option<i32> {
    if base == 0 && exponent == 0 {
        return None;
    }
    if exponent < 0 {
        return match base {
            1 => Some(1),
            -1 => Some(if exponent % 2 == 0 { 1 } else { -1 }),
            _ => None,
        };
    }
    let mut result: i32 = 1;
    for _ in 0..exponent {
        result = result.checked_mul(base)?;
        if result == 0 || result == 1 && base == 1 {
            break;
        }
    }
    Some(result)
}

fn narrowed(value: i64) -> Option<i32> {
    i32::try_from(value).ok()
}

fn stored(tag: &Tag) -> Option<f64> {
    Some(match tag {
        Tag::Byte(value) => f64::from(*value),
        Tag::Short(value) => f64::from(*value),
        Tag::Int(value) => f64::from(*value),
        Tag::Long(value) => *value as f64,
        Tag::Float(value) => f64::from(*value),
        Tag::Double(value) => *value,
        _ => return None,
    })
}

fn int_of_tag(tag: &Tag) -> Option<i32> {
    Some(match tag {
        Tag::Byte(value) => i32::from(*value),
        Tag::Short(value) => i32::from(*value),
        Tag::Int(value) => *value,
        Tag::Long(value) => *value as i32,
        Tag::Float(value) => *value as i32,
        Tag::Double(value) => *value as i32,
        _ => return None,
    })
}

impl<R: Rng + ?Sized> Roller<'_, '_, R> {
    fn stored_number(&self, storage: &str, path: &str) -> Option<Tag> {
        let world = self.context.world?;
        let data = world
            .storages
            .iter()
            .find(|(name, _)| *name == storage)
            .map(|(_, data)| (*data).clone())?;
        let path = NbtPath::parse(path).ok()?;
        let root = Tag::Compound(data);
        let found = path.get(&root);
        match found.as_slice() {
            [single] => stored(single).map(|_| (*single).clone()),
            _ => None,
        }
    }

    pub(super) fn attribute_value(&self, attribute: &str) -> Option<AttributeValue> {
        let world = self.context.world;
        let dimension = world.map_or("", |world| world.dimension);
        let fallback = [(
            rspigot_domain::world_clock::OVERWORLD,
            self.context.day_time.unwrap_or(0),
        )];
        let clocks = match world {
            Some(world) => world.clocks.as_slice(),
            None => &fallback[..],
        };
        crate::environment_attributes::value(attribute, dimension, clocks)
    }

    fn score_of(&self, holder: ScoreHolder, objective: &str) -> Option<i32> {
        let world = self.context.world;
        let in_world = |name: &str| {
            world.and_then(|world| {
                world
                    .scores
                    .iter()
                    .find(|(holder, found, _)| holder == name && found == objective)
                    .map(|(_, _, value)| *value)
            })
        };
        match holder {
            ScoreHolder::Fixed(name) => in_world(name),
            ScoreHolder::Context(target) => {
                let entity = self.context.target(target)?;
                entity
                    .scores
                    .iter()
                    .find(|(found, _)| found == objective)
                    .map(|(_, value)| *value)
                    .or_else(|| entity.name.as_deref().and_then(in_world))
            }
        }
    }

    fn deeper<T>(&mut self, evaluate: impl FnOnce(&mut Self) -> Option<T>) -> Option<T> {
        if self.depth >= MAX_NESTING {
            tracing::warn!("Detected infinite loop in number provider references");
            return None;
        }
        self.depth += 1;
        let found = evaluate(self);
        self.depth -= 1;
        found
    }

    pub(super) fn int_expr(&mut self, expr: &IntExpr) -> i32 {
        match self.int_unsafe(expr) {
            Some(value) => value,
            None => {
                tracing::warn!("A loot number provider failed arithmetic, using 0");
                0
            }
        }
    }

    pub(super) fn float_expr(&mut self, expr: &FloatExpr) -> f32 {
        self.float_unsafe(expr)
            .filter(|value| value.is_finite())
            .unwrap_or(0.0)
    }

    fn inputs_int(&mut self, inputs: &[IntExpr]) -> Option<Vec<i32>> {
        inputs.iter().map(|input| self.int_unsafe(input)).collect()
    }

    fn inputs_float(&mut self, inputs: &[FloatExpr]) -> Option<Vec<f32>> {
        inputs
            .iter()
            .map(|input| self.float_unsafe(input))
            .collect()
    }

    fn pick_weight(&mut self, weights: impl Iterator<Item = i32>) -> Option<usize> {
        let weights: Vec<i32> = weights.collect();
        let total: i64 = weights.iter().map(|weight| i64::from(*weight)).sum();
        if total <= 0 {
            return None;
        }
        let mut pick = self.rng.random_range(0..total);
        weights.iter().position(|weight| {
            pick -= i64::from(*weight);
            pick < 0
        })
    }

    fn int_unsafe(&mut self, expr: &IntExpr) -> Option<i32> {
        match *expr {
            IntExpr::Constant(value) => Some(value),
            IntExpr::Uniform { min, max } => {
                let low = self.int_unsafe(min)?;
                let high = self.int_unsafe(max)?;
                if low >= high {
                    Some(low)
                } else {
                    let span = i64::from(high) - i64::from(low) + 1;
                    narrowed(i64::from(low) + self.rng.random_range(0..span))
                }
            }
            IntExpr::Binomial { n, p } => {
                let trials = self.int_unsafe(n)?;
                let chance = self.float_unsafe(p).filter(|chance| chance.is_finite())?;
                let mut successes = 0;
                for _ in 0..trials.max(0) {
                    if self.rng.random::<f32>() < chance {
                        successes += 1;
                    }
                }
                Some(successes)
            }
            IntExpr::Aggregate { op, inputs } => {
                let values = self.inputs_int(inputs)?;
                match op {
                    Aggregate::Sum => narrowed(values.iter().map(|v| i64::from(*v)).sum()),
                    Aggregate::Product => {
                        let mut product: i64 = 1;
                        for value in values {
                            product = product.checked_mul(i64::from(value))?;
                        }
                        narrowed(product)
                    }
                    Aggregate::Average => {
                        let count = i64::try_from(values.len()).ok().filter(|c| *c > 0)?;
                        narrowed(values.iter().map(|v| i64::from(*v)).sum::<i64>() / count)
                    }
                    Aggregate::Minimum => Some(values.into_iter().fold(i32::MAX, i32::min)),
                    Aggregate::Maximum => Some(values.into_iter().fold(i32::MIN, i32::max)),
                    Aggregate::Length => None,
                }
            }
            IntExpr::Binary { op, left, right } => {
                let left = self.int_unsafe(left)?;
                let right = self.int_unsafe(right)?;
                match op {
                    Binary::Difference => left.checked_sub(right),
                    Binary::Quotient => (right != 0).then(|| left.wrapping_div(right)),
                    Binary::FloorQuotient => floor_quotient(left, right),
                    Binary::Modulus => (right != 0).then(|| left.wrapping_rem(right)),
                    Binary::FloorModulus => floor_modulus(left, right),
                }
            }
            IntExpr::Unary { op, input } => {
                let value = self.int_unsafe(input)?;
                match op {
                    Unary::Negate => value.checked_neg(),
                    _ => value.checked_abs(),
                }
            }
            IntExpr::Power { base, exponent } => {
                let base = self.int_unsafe(base)?;
                let exponent = self.int_unsafe(exponent)?;
                power(base, exponent)
            }
            IntExpr::FromFloat(input) => rounded(self.float_unsafe(input)?),
            IntExpr::Conditional {
                condition,
                on_true,
                on_false,
            } => {
                let chosen = if self.test(condition) {
                    on_true
                } else {
                    on_false
                };
                self.int_unsafe(chosen)
            }
            IntExpr::Dispatch { cases, default } => {
                for (condition, value) in cases {
                    if self.test(condition) {
                        return self.int_unsafe(value);
                    }
                }
                self.int_unsafe(default)
            }
            IntExpr::Weighted(entries) => {
                let index = self.pick_weight(entries.iter().map(|(_, weight)| *weight))?;
                self.int_unsafe(&entries[index].0)
            }
            IntExpr::Score {
                holder,
                objective,
                fallback,
            } => match self.score_of(holder, objective) {
                Some(value) => Some(value),
                None => self.int_unsafe(fallback),
            },
            IntExpr::Storage {
                storage,
                path,
                fallback,
            } => match self.stored_number(storage, path) {
                Some(tag) => int_of_tag(&tag),
                None => self.int_unsafe(fallback),
            },
            IntExpr::Attribute(attribute) => match self.attribute_value(attribute)? {
                AttributeValue::Bool(flag) => Some(i32::from(flag)),
                AttributeValue::Float(value) => rounded(value),
                AttributeValue::Text(_) => None,
            },
            IntExpr::Reference(name) => {
                let found = loot::int_provider(name);
                if found.is_none() {
                    tracing::warn!("Unknown int provider {name} referenced by a loot number");
                }
                self.deeper(|roller| roller.int_unsafe(found?))
            }
        }
    }

    fn float_unsafe(&mut self, expr: &FloatExpr) -> Option<f32> {
        match *expr {
            FloatExpr::Constant(value) => Some(value),
            FloatExpr::Uniform { min, max } => {
                let low = self.float_unsafe(min)?;
                let high = self.float_unsafe(max)?;
                if low >= high {
                    Some(low)
                } else {
                    Some(self.rng.random::<f32>() * (high - low) + low)
                }
            }
            FloatExpr::Aggregate { op, inputs } => {
                let values = self.inputs_float(inputs)?;
                Some(match op {
                    Aggregate::Sum => values.iter().sum(),
                    Aggregate::Product => values.iter().product(),
                    Aggregate::Average => values.iter().sum::<f32>() / values.len() as f32,
                    Aggregate::Minimum => values.into_iter().fold(f32::MAX, f32::min),
                    Aggregate::Maximum => values.into_iter().fold(f32::MIN, f32::max),
                    Aggregate::Length => values.iter().map(|v| v * v).sum::<f32>().sqrt(),
                })
            }
            FloatExpr::Binary { op, left, right } => match op {
                Binary::Modulus => {
                    let right = self.float_unsafe(right)?;
                    if right == 0.0 {
                        return Some(f32::NAN);
                    }
                    Some(self.float_unsafe(left)? % right)
                }
                Binary::Quotient => Some(self.float_unsafe(left)? / self.float_unsafe(right)?),
                _ => Some(self.float_unsafe(left)? - self.float_unsafe(right)?),
            },
            FloatExpr::Unary { op, input } => {
                let value = self.float_unsafe(input)?;
                Some(match op {
                    Unary::Absolute => value.abs(),
                    Unary::Negate => -value,
                    Unary::Ceiling => value.ceil() as i32 as f32,
                    Unary::Floor => f64::from(value).floor() as f32,
                    Unary::Round => rounded(value).map_or(value, |rounded| rounded as f32),
                    Unary::Truncate => value.trunc(),
                    Unary::Sine => sin(value),
                    Unary::Cosine => cos(value),
                    Unary::SquareRoot => value.sqrt(),
                })
            }
            FloatExpr::Power { base, exponent } => {
                let base = self.float_unsafe(base)?;
                let exponent = self.float_unsafe(exponent)?;
                if base == 0.0 && exponent == 0.0 {
                    return Some(f32::NAN);
                }
                Some(f64::from(base).powf(f64::from(exponent)) as f32)
            }
            FloatExpr::FromInt(input) => Some(self.int_unsafe(input)? as f32),
            FloatExpr::EnchantmentLevel(amount) => {
                Some(amount.calculate(self.context.enchantment_level))
            }
            FloatExpr::Conditional {
                condition,
                on_true,
                on_false,
            } => {
                let chosen = if self.test(condition) {
                    on_true
                } else {
                    on_false
                };
                self.float_unsafe(chosen)
            }
            FloatExpr::Dispatch { cases, default } => {
                for (condition, value) in cases {
                    if self.test(condition) {
                        return self.float_unsafe(value);
                    }
                }
                self.float_unsafe(default)
            }
            FloatExpr::Weighted(entries) => {
                let index = self.pick_weight(entries.iter().map(|(_, weight)| *weight))?;
                self.float_unsafe(&entries[index].0)
            }
            FloatExpr::Storage {
                storage,
                path,
                fallback,
            } => match self.stored_number(storage, path) {
                Some(tag) => stored(&tag).map(|value| value as f32),
                None => self.float_unsafe(fallback),
            },
            FloatExpr::Attribute(attribute) => match self.attribute_value(attribute)? {
                AttributeValue::Bool(flag) => Some(f32::from(u8::from(flag))),
                AttributeValue::Float(value) => Some(value),
                AttributeValue::Text(_) => None,
            },
            FloatExpr::Reference(name) => {
                let found = loot::float_provider(name);
                if found.is_none() {
                    tracing::warn!("Unknown float provider {name} referenced by a loot number");
                }
                self.deeper(|roller| roller.float_unsafe(found?))
            }
        }
    }
}
