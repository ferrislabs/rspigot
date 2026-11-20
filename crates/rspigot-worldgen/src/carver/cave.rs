use std::f32::consts::{FRAC_PI_2, PI};

use rspigot_domain::mth;
use serde_json::Value;

use super::Context;
use super::ellipsoid::{RANGE, can_reach, carve_ellipsoid};
use super::mask::Mask;
use super::provider::{FloatProvider, HeightProvider, IntProvider};
use crate::legacy::JavaRandom;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Cave {
    pub(crate) probability: f32,
    pub(crate) y: HeightProvider,
    pub(crate) count: IntProvider,
    pub(crate) thickness: FloatProvider,
    pub(crate) weird_thickness_bias: bool,
    pub(crate) room_vertical_radius: FloatProvider,
    pub(crate) horizontal_radius: FloatProvider,
    pub(crate) vertical_radius: FloatProvider,
    pub(crate) start_vertical_radius: FloatProvider,
    pub(crate) floor_level: FloatProvider,
}

fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value, String> {
    value.get(key).ok_or_else(|| format!("missing {key}"))
}

impl Cave {
    pub(crate) fn parse(value: &Value) -> Result<Self, String> {
        let float = |key: &str| FloatProvider::parse(field(value, key)?);
        Ok(Self {
            probability: field(value, "probability")?
                .as_f64()
                .ok_or("probability is not a number")? as f32,
            y: HeightProvider::parse(field(value, "y")?)?,
            count: IntProvider::parse(field(value, "count")?)?,
            thickness: float("thickness")?,
            weird_thickness_bias: value
                .get("weird_thickness_bias")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            room_vertical_radius: float("room_vertical_radius_multiplier")?,
            horizontal_radius: float("horizontal_radius_multiplier")?,
            vertical_radius: float("vertical_radius_multiplier")?,
            start_vertical_radius: match value.get("start_vertical_radius_multiplier") {
                Some(provider) => FloatProvider::parse(provider)?,
                None => FloatProvider::Constant(1.0),
            },
            floor_level: float("floor_level")?,
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
        let count = self.count.sample(random);
        for _ in 0..count {
            let x = f64::from(source.0 * 16 + random.next_int(16));
            let y = f64::from(self.y.sample(random, context.min_y, context.depth));
            let z = f64::from(source.1 * 16 + random.next_int(16));
            let horizontal = f64::from(self.horizontal_radius.sample(random));
            let vertical = f64::from(self.vertical_radius.sample(random));
            let start_scale = f64::from(self.start_vertical_radius.sample(random));
            let floor_level = f64::from(self.floor_level.sample(random));
            let skip = move |xd: f64, yd: f64, zd: f64, _: i32| {
                yd <= floor_level || xd * xd + yd * yd + zd * zd >= 1.0
            };
            let mut tunnels = 1;
            if random.next_int(4) == 0 {
                let room_scale = f64::from(self.room_vertical_radius.sample(random));
                let thickness = random.next_float() * 6.0 + 1.0;
                create_room(chunk, (x, y, z), thickness, room_scale, mask, &skip);
                tunnels += random.next_int(4);
            }
            for _ in 0..tunnels {
                let horizontal_rotation = random.next_float() * 6.283_185_5;
                let vertical_rotation = (random.next_float() - 0.5) / 4.0;
                let thickness = self.thickness_for(random);
                let distance = span - random.next_int(span / 4);
                let seed = random.next_long();
                let tunnel = Tunnel {
                    chunk,
                    horizontal,
                    vertical,
                    distance,
                    skip: &skip,
                };
                tunnel.run(
                    seed,
                    (x, y, z),
                    (thickness, horizontal_rotation, vertical_rotation),
                    0,
                    start_scale,
                    mask,
                );
            }
        }
    }

    fn thickness_for(&self, random: &mut JavaRandom) -> f32 {
        let mut thickness = self.thickness.sample(random);
        if self.weird_thickness_bias && random.next_int(10) == 0 {
            let first = random.next_float();
            let second = random.next_float();
            thickness *= first * second * 3.0 + 1.0;
        }
        thickness
    }
}

fn create_room(
    chunk: (i32, i32),
    (x, y, z): (f64, f64, f64),
    thickness: f32,
    scale: f64,
    mask: &mut Mask,
    skip: &dyn Fn(f64, f64, f64, i32) -> bool,
) {
    let radius = f64::from(mth::sin(f64::from(FRAC_PI_2)) * thickness) + 1.5;
    carve_ellipsoid(chunk, (x + 1.0, y, z), (radius, radius * scale), mask, skip);
}

struct Tunnel<'a> {
    chunk: (i32, i32),
    horizontal: f64,
    vertical: f64,
    distance: i32,
    skip: &'a dyn Fn(f64, f64, f64, i32) -> bool,
}

impl Tunnel<'_> {
    fn run(
        &self,
        seed: i64,
        from: (f64, f64, f64),
        angles: (f32, f32, f32),
        step: i32,
        scale: f64,
        mask: &mut Mask,
    ) {
        let (mut x, mut y, mut z) = from;
        let (thickness, mut yaw, mut pitch) = angles;
        let mut random = JavaRandom::new(seed);
        let distance = self.distance;
        let split = random.next_int(distance / 2) + distance / 4;
        let steep = random.next_int(6) == 0;
        let mut yaw_drift = 0.0f32;
        let mut pitch_drift = 0.0f32;
        for j in step..distance {
            let radius =
                1.5 + f64::from(mth::sin(f64::from(PI * j as f32 / distance as f32)) * thickness);
            let flat = radius * scale;
            let cos_pitch = mth::cos(f64::from(pitch));
            x += f64::from(mth::cos(f64::from(yaw)) * cos_pitch);
            y += f64::from(mth::sin(f64::from(pitch)));
            z += f64::from(mth::sin(f64::from(yaw)) * cos_pitch);
            pitch *= if steep { 0.92 } else { 0.7 };
            pitch += pitch_drift * 0.1;
            yaw += yaw_drift * 0.1;
            pitch_drift *= 0.9;
            yaw_drift *= 0.75;
            pitch_drift += (random.next_float() - random.next_float()) * random.next_float() * 2.0;
            yaw_drift += (random.next_float() - random.next_float()) * random.next_float() * 4.0;
            if j == split && thickness > 1.0 {
                for turn in [-FRAC_PI_2, FRAC_PI_2] {
                    let child_seed = random.next_long();
                    let child_thickness = random.next_float() * 0.5 + 0.5;
                    self.run(
                        child_seed,
                        (x, y, z),
                        (child_thickness, yaw + turn, pitch / 3.0),
                        j,
                        1.0,
                        mask,
                    );
                }
                return;
            }
            if random.next_int(4) != 0 {
                if !can_reach(self.chunk, x, z, j, distance, thickness) {
                    return;
                }
                carve_ellipsoid(
                    self.chunk,
                    (x, y, z),
                    (radius * self.horizontal, flat * self.vertical),
                    mask,
                    self.skip,
                );
            }
        }
    }
}
