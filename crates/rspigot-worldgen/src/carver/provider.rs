use serde_json::Value;

use crate::legacy::JavaRandom;

fn number(value: &Value, key: &str) -> Result<f64, String> {
    value
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("missing number {key}"))
}

fn kind(value: &Value) -> &str {
    let text = value.get("type").and_then(Value::as_str).unwrap_or("");
    text.strip_prefix("minecraft:").unwrap_or(text)
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FloatProvider {
    Constant(f32),
    Uniform(f32, f32),
    Trapezoid { min: f32, max: f32, plateau: f32 },
}

impl FloatProvider {
    pub(crate) fn parse(value: &Value) -> Result<Self, String> {
        if let Some(n) = value.as_f64() {
            return Ok(Self::Constant(n as f32));
        }
        match kind(value) {
            "constant" => Ok(Self::Constant(number(value, "value")? as f32)),
            "uniform" => Ok(Self::Uniform(
                number(value, "min_inclusive")? as f32,
                number(value, "max_exclusive")? as f32,
            )),
            "trapezoid" => Ok(Self::Trapezoid {
                min: number(value, "min")? as f32,
                max: number(value, "max")? as f32,
                plateau: number(value, "plateau")? as f32,
            }),
            other => Err(format!("unsupported float provider {other:?}")),
        }
    }

    pub(crate) fn sample(&self, random: &mut JavaRandom) -> f32 {
        match *self {
            Self::Constant(value) => value,
            Self::Uniform(min, max) => random.next_float() * (max - min) + min,
            Self::Trapezoid { min, max, plateau } => {
                let range = max - min;
                let half = (range - plateau) / 2.0;
                let rest = range - half;
                min + random.next_float() * rest + random.next_float() * half
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum IntProvider {
    Constant(i32),
    Uniform(i32, i32),
    BiasedToBottom(i32, i32),
    VeryBiasedToBottom(i32, i32),
}

impl IntProvider {
    pub(crate) fn parse(value: &Value) -> Result<Self, String> {
        if let Some(n) = value.as_f64() {
            return Ok(Self::Constant(n as i32));
        }
        let bounds = || -> Result<(i32, i32), String> {
            Ok((
                number(value, "min_inclusive")? as i32,
                number(value, "max_inclusive")? as i32,
            ))
        };
        match kind(value) {
            "constant" => Ok(Self::Constant(number(value, "value")? as i32)),
            "uniform" => bounds().map(|(min, max)| Self::Uniform(min, max)),
            "biased_to_bottom" => bounds().map(|(min, max)| Self::BiasedToBottom(min, max)),
            "very_biased_to_bottom" => {
                bounds().map(|(min, max)| Self::VeryBiasedToBottom(min, max))
            }
            other => Err(format!("unsupported int provider {other:?}")),
        }
    }

    pub(crate) fn sample(&self, random: &mut JavaRandom) -> i32 {
        match *self {
            Self::Constant(value) => value,
            Self::Uniform(min, max) => min + random.next_int(max - min + 1),
            Self::BiasedToBottom(min, max) => {
                let bound = random.next_int(max - min + 1) + 1;
                min + random.next_int(bound)
            }
            Self::VeryBiasedToBottom(min, max) => {
                let first = random.next_int(max - min + 1) + 1;
                let second = random.next_int(first) + 1;
                min + random.next_int(second)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Anchor {
    Absolute(i32),
    AboveBottom(i32),
    BelowTop(i32),
}

impl Anchor {
    fn parse(value: &Value) -> Result<Self, String> {
        let find = |key: &str| value.get(key).and_then(Value::as_i64).map(|n| n as i32);
        find("absolute")
            .map(Self::Absolute)
            .or_else(|| find("above_bottom").map(Self::AboveBottom))
            .or_else(|| find("below_top").map(Self::BelowTop))
            .ok_or_else(|| "unsupported vertical anchor".to_owned())
    }

    pub(crate) fn resolve(self, min_y: i32, depth: i32) -> i32 {
        match self {
            Self::Absolute(n) => n,
            Self::AboveBottom(n) => min_y + n,
            Self::BelowTop(n) => min_y + depth - 1 - n,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum HeightProvider {
    Constant(Anchor),
    Uniform(Anchor, Anchor),
}

impl HeightProvider {
    pub(crate) fn parse(value: &Value) -> Result<Self, String> {
        let anchor = |key: &str| Anchor::parse(value.get(key).unwrap_or(&Value::Null));
        match kind(value) {
            "constant" => anchor("value").map(Self::Constant),
            "uniform" => Ok(Self::Uniform(
                anchor("min_inclusive")?,
                anchor("max_inclusive")?,
            )),
            other => Err(format!("unsupported height provider {other:?}")),
        }
    }

    pub(crate) fn sample(&self, random: &mut JavaRandom, min_y: i32, depth: i32) -> i32 {
        match *self {
            Self::Constant(anchor) => anchor.resolve(min_y, depth),
            Self::Uniform(min, max) => {
                let (low, high) = (min.resolve(min_y, depth), max.resolve(min_y, depth));
                if low > high {
                    low
                } else {
                    low + random.next_int(high - low + 1)
                }
            }
        }
    }
}
