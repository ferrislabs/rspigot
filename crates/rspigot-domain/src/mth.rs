use std::sync::OnceLock;

pub const DEG_TO_RAD: f32 = 0.017_453_292;
pub const RAD_TO_DEG: f64 = 57.295_776_367_187_5;

const SIN_SCALE: f64 = 10_430.378_350_470_453;
const SIN_COUNT: usize = 65_536;
const LUT_SIZE: usize = 257;
const FRAC_BIAS_BITS: u64 = 4_805_340_802_404_319_232;
const INV_SQRT_MAGIC: i64 = 6_910_469_410_427_058_090;
const ONE_SIXTH: f64 = 0.166_666_666_666_666_66;

fn sin_table() -> &'static [f32] {
    static TABLE: OnceLock<Vec<f32>> = OnceLock::new();
    TABLE.get_or_init(|| {
        (0..SIN_COUNT)
            .map(|index| (index as f64 / SIN_SCALE).sin() as f32)
            .collect()
    })
}

struct ArcTables {
    asin: [f64; LUT_SIZE],
    cos: [f64; LUT_SIZE],
}

fn arc_tables() -> &'static ArcTables {
    static TABLES: OnceLock<ArcTables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let mut tables = ArcTables {
            asin: [0.0; LUT_SIZE],
            cos: [0.0; LUT_SIZE],
        };
        for index in 0..LUT_SIZE {
            let asin = (index as f64 / 256.0).asin();
            tables.cos[index] = asin.cos();
            tables.asin[index] = asin;
        }
        tables
    })
}

pub fn sin(value: f64) -> f32 {
    sin_table()[((value * SIN_SCALE) as i64 & 65_535) as usize]
}

pub fn cos(value: f64) -> f32 {
    sin_table()[((value * SIN_SCALE + 16_384.0) as i64 & 65_535) as usize]
}

pub fn yaw_direction(yaw: f32) -> (f64, f64) {
    let radians = f64::from(yaw * DEG_TO_RAD);
    (f64::from(sin(radians)), f64::from(-cos(radians)))
}

pub fn wrap_degrees(value: f32) -> f32 {
    let mut wrapped = value % 360.0;
    if wrapped >= 180.0 {
        wrapped -= 360.0;
    }
    if wrapped < -180.0 {
        wrapped += 360.0;
    }
    wrapped
}

pub fn degrees_difference(from: f32, to: f32) -> f32 {
    wrap_degrees(to - from)
}

pub fn degrees_difference_abs(from: f32, to: f32) -> f32 {
    degrees_difference(from, to).abs()
}

pub fn approach(current: f32, target: f32, increment: f32) -> f32 {
    let increment = increment.abs();
    if current < target {
        clamp(current + increment, current, target)
    } else {
        clamp(current - increment, target, current)
    }
}

pub fn approach_degrees(current: f32, target: f32, increment: f32) -> f32 {
    let difference = degrees_difference(current, target);
    approach(current, current + difference, increment)
}

pub fn clamp(value: f32, min: f32, max: f32) -> f32 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

pub fn rotate_if_necessary(current: f32, target: f32, max: f32) -> f32 {
    let difference = degrees_difference(current, target);
    let limited = clamp(difference, -max, max);
    target - limited
}

pub fn floor(value: f64) -> i32 {
    value.floor() as i32
}

pub fn lerp(delta: f64, start: f64, end: f64) -> f64 {
    start + delta * (end - start)
}

pub fn frac(value: f64) -> f64 {
    value - value.floor()
}

pub fn sign(value: f64) -> i32 {
    if value == 0.0 {
        0
    } else if value > 0.0 {
        1
    } else {
        -1
    }
}

pub const fn positive_ceil_div(value: i32, divisor: i32) -> i32 {
    -(-value).div_euclid(divisor)
}

fn fast_inv_sqrt(value: f64) -> f64 {
    let half = 0.5 * value;
    let bits = INV_SQRT_MAGIC - ((value.to_bits() as i64) >> 1);
    let estimate = f64::from_bits(bits as u64);
    estimate * (1.5 - half * estimate * estimate)
}

pub fn atan2(y: f64, x: f64) -> f64 {
    let length = x * x + y * y;
    if length.is_nan() {
        return f64::NAN;
    }
    let negative_y = y < 0.0;
    let mut y = if negative_y { -y } else { y };
    let negative_x = x < 0.0;
    let mut x = if negative_x { -x } else { x };
    let steep = y > x;
    if steep {
        std::mem::swap(&mut x, &mut y);
    }
    let inverse = fast_inv_sqrt(length);
    x *= inverse;
    y *= inverse;
    let bias = f64::from_bits(FRAC_BIAS_BITS);
    let biased = bias + y;
    let index = biased.to_bits() as i32 as usize;
    let tables = arc_tables();
    let asin = tables.asin[index];
    let cos = tables.cos[index];
    let delta = biased - bias;
    let error = y * cos - x * delta;
    let correction = (6.0 + error * error) * error * ONE_SIXTH;
    let mut angle = asin + correction;
    if steep {
        angle = std::f64::consts::FRAC_PI_2 - angle;
    }
    if negative_x {
        angle = std::f64::consts::PI - angle;
    }
    if negative_y {
        angle = -angle;
    }
    angle
}

pub fn view_vector(pitch: f32, yaw: f32) -> crate::Vec3 {
    let x = f64::from(pitch * DEG_TO_RAD);
    let y = f64::from(-yaw * DEG_TO_RAD);
    let (cos_y, sin_y) = (cos(y), sin(y));
    let (cos_x, sin_x) = (cos(x), sin(x));
    crate::Vec3::new(
        f64::from(sin_y * cos_x),
        f64::from(-sin_x),
        f64::from(cos_y * cos_x),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn approach_moves_by_the_increment_without_overshooting() {
        assert_eq!(approach(0.1, 1.8, 0.5), 0.6);
        assert_eq!(approach(1.7, 1.8, 0.5), 1.8);
        assert_eq!(approach(1.0, 0.2, 0.025), 0.975);
        assert_eq!(approach(0.3, 0.2, -0.5), 0.2);
    }

    #[test]
    fn approach_degrees_takes_the_short_way_round() {
        assert_eq!(approach_degrees(170.0, -170.0, 4.0), 174.0);
        assert_eq!(approach_degrees(-170.0, 170.0, 4.0), -174.0);
        assert_eq!(approach_degrees(10.0, 12.0, 4.0), 12.0);
        assert_eq!(degrees_difference_abs(350.0, 10.0), 20.0);
    }

    use super::*;

    #[test]
    fn sin_table_matches_mth() {
        assert_eq!(sin(0.0), 0.0);
        assert_eq!(cos(0.0), 1.0);
        assert!((sin(std::f64::consts::FRAC_PI_2) - 1.0).abs() < 1.0e-6);
        assert_eq!(sin(f64::from(90.0 * DEG_TO_RAD)), sin_table()[16_383]);
    }

    #[test]
    fn atan2_is_close_to_libm() {
        for (y, x) in [
            (1.0, 1.0),
            (-2.0, 0.5),
            (0.3, -4.0),
            (-1.0, -1.0),
            (0.0, 1.0),
        ] {
            assert!((atan2(y, x) - f64::atan2(y, x)).abs() < 1.0e-6, "{y} {x}");
        }
        assert!(atan2(f64::NAN, 1.0).is_nan());
    }

    #[test]
    fn wrapping_and_rotation() {
        assert_eq!(wrap_degrees(190.0), -170.0);
        assert_eq!(wrap_degrees(-190.0), 170.0);
        assert_eq!(wrap_degrees(180.0), -180.0);
        assert_eq!(degrees_difference(350.0, 10.0), 20.0);
        assert_eq!(rotate_if_necessary(0.0, 100.0, 75.0), 25.0);
        assert_eq!(rotate_if_necessary(0.0, 30.0, 75.0), 0.0);
        assert_eq!(positive_ceil_div(120, 2), 60);
        assert_eq!(positive_ceil_div(7, 2), 4);
        assert_eq!(positive_ceil_div(0, 2), 0);
    }
}
