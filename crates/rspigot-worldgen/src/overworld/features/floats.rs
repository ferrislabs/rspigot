use serde_json::Value;

use super::prov::{number, short};
use crate::climate::Xoroshiro;

#[derive(Debug, Clone, Copy)]
pub(super) enum Float {
    Constant(f32),
    Uniform(f32, f32),
    ClampedNormal {
        mean: f32,
        deviation: f32,
        min: f32,
        max: f32,
    },
}

impl Float {
    pub(super) fn compile(value: &Value) -> Self {
        if let Some(constant) = value.as_f64() {
            return Self::Constant(constant as f32);
        }
        match short(value) {
            "uniform" => Self::Uniform(
                number(value, "min_inclusive", 0.0) as f32,
                number(value, "max_exclusive", 0.0) as f32,
            ),
            "clamped_normal" => Self::ClampedNormal {
                mean: number(value, "mean", 0.0) as f32,
                deviation: number(value, "deviation", 1.0) as f32,
                min: number(value, "min", 0.0) as f32,
                max: number(value, "max", 0.0) as f32,
            },
            _ => Self::Constant(number(value, "value", 0.0) as f32),
        }
    }

    pub(super) fn sample(&self, rng: &mut Xoroshiro) -> f32 {
        match *self {
            Self::Constant(value) => value,
            Self::Uniform(min, max) => rng.next_float() * (max - min) + min,
            Self::ClampedNormal {
                mean,
                deviation,
                min,
                max,
            } => (mean + rng.next_gaussian() as f32 * deviation).clamp(min, max),
        }
    }
}
