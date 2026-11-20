use std::f32::consts::PI;

use rspigot_domain::mth;
use serde_json::Value;

use super::Context;
use super::ellipsoid::{RANGE, can_reach, carve_ellipsoid};
use super::mask::Mask;
use super::provider::{FloatProvider, HeightProvider};
use crate::legacy::JavaRandom;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Canyon {
    pub(crate) probability: f32,
    pub(crate) y: HeightProvider,
    pub(crate) vertical_rotation: FloatProvider,
    pub(crate) distance_factor: FloatProvider,
    pub(crate) horizontal_radius_factor: FloatProvider,
    pub(crate) thickness: FloatProvider,
    pub(crate) vertical_radius_center_factor: f32,
    pub(crate) vertical_radius_default_factor: f32,
    pub(crate) width_smoothness: i32,
    pub(crate) y_scale: FloatProvider,
}

fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value, String> {
    value.get(key).ok_or_else(|| format!("missing {key}"))
}

fn float_of(value: &Value, key: &str) -> Result<f32, String> {
    field(value, key)?
        .as_f64()
        .map(|n| n as f32)
        .ok_or_else(|| format!("{key} is not a number"))
}

impl Canyon {
    pub(crate) fn parse(value: &Value) -> Result<Self, String> {
        let shape = field(value, "shape")?;
        let provider = |source: &Value, key: &str| FloatProvider::parse(field(source, key)?);
        Ok(Self {
            probability: float_of(value, "probability")?,
            y: HeightProvider::parse(field(value, "y")?)?,
            vertical_rotation: provider(value, "vertical_rotation")?,
            distance_factor: provider(shape, "distance_factor")?,
            horizontal_radius_factor: provider(shape, "horizontal_radius_factor")?,
            thickness: provider(shape, "thickness")?,
            vertical_radius_center_factor: float_of(shape, "vertical_radius_center_factor")?,
            vertical_radius_default_factor: float_of(shape, "vertical_radius_default_factor")?,
            width_smoothness: float_of(shape, "width_smoothness")? as i32,
            y_scale: provider(shape, "y_scale")?,
        })
    }

    pub(crate) fn carve(
        &self,
        context: Context,
        random: &mut JavaRandom,
        chunk: (i32, i32),
        source: (i32, i32),
        mask: &mut Mask,
    ) {
        let span = (RANGE * 2 - 1) * 16;
        let x = f64::from(source.0 * 16 + random.next_int(16));
        let y = f64::from(self.y.sample(random, context.min_y, context.depth));
        let z = f64::from(source.1 * 16 + random.next_int(16));
        let yaw = random.next_float() * 6.283_185_5;
        let pitch = self.vertical_rotation.sample(random);
        let scale = f64::from(self.y_scale.sample(random));
        let thickness = self.thickness.sample(random);
        let distance = (span as f32 * self.distance_factor.sample(random)) as i32;
        let seed = random.next_long();
        self.walk(
            context,
            chunk,
            seed,
            (x, y, z),
            (thickness, yaw, pitch),
            distance,
            scale,
            mask,
        );
    }

    fn widths(&self, context: Context, random: &mut JavaRandom) -> Vec<f32> {
        let mut factor = 1.0f32;
        (0..context.depth)
            .map(|index| {
                if index == 0 || random.next_int(self.width_smoothness) == 0 {
                    factor = 1.0 + random.next_float() * random.next_float();
                }
                factor * factor
            })
            .collect()
    }

    fn vertical_radius(
        &self,
        random: &mut JavaRandom,
        radius: f64,
        distance: f32,
        step: f32,
    ) -> f64 {
        let progress = 1.0 - (0.5 - step / distance).abs() * 2.0;
        let factor =
            self.vertical_radius_default_factor + self.vertical_radius_center_factor * progress;
        let jitter = random.next_float() * (1.0 - 0.75) + 0.75;
        f64::from(factor) * radius * f64::from(jitter)
    }

    #[allow(clippy::too_many_arguments)]
    fn walk(
        &self,
        context: Context,
        chunk: (i32, i32),
        seed: i64,
        from: (f64, f64, f64),
        angles: (f32, f32, f32),
        distance: i32,
        scale: f64,
        mask: &mut Mask,
    ) {
        let (mut x, mut y, mut z) = from;
        let (thickness, mut yaw, mut pitch) = angles;
        let mut random = JavaRandom::new(seed);
        let widths = self.widths(context, &mut random);
        let min_y = context.min_y;
        let skip = |xd: f64, yd: f64, zd: f64, by: i32| {
            let width = widths
                .get((by - min_y - 1) as usize)
                .copied()
                .unwrap_or(1.0);
            (xd * xd + zd * zd) * f64::from(width) + yd * yd / 6.0 >= 1.0
        };
        let mut yaw_drift = 0.0f32;
        let mut pitch_drift = 0.0f32;
        for j in 0..distance {
            let base =
                1.5 + f64::from(mth::sin(f64::from(j as f32 * PI / distance as f32)) * thickness);
            let flat = base * scale;
            let horizontal = base * f64::from(self.horizontal_radius_factor.sample(&mut random));
            let vertical = self.vertical_radius(&mut random, flat, distance as f32, j as f32);
            let cos_pitch = mth::cos(f64::from(pitch));
            x += f64::from(mth::cos(f64::from(yaw)) * cos_pitch);
            y += f64::from(mth::sin(f64::from(pitch)));
            z += f64::from(mth::sin(f64::from(yaw)) * cos_pitch);
            pitch = pitch * 0.7 + pitch_drift * 0.05;
            yaw += yaw_drift * 0.05;
            yaw_drift *= 0.5;
            pitch_drift = pitch_drift * 0.8
                + (random.next_float() - random.next_float()) * random.next_float() * 2.0;
            yaw_drift += (random.next_float() - random.next_float()) * random.next_float() * 4.0;
            if random.next_int(4) != 0 {
                if !can_reach(chunk, x, z, j, distance, thickness) {
                    return;
                }
                carve_ellipsoid(chunk, (x, y, z), (horizontal, vertical), mask, &skip);
            }
        }
    }
}
