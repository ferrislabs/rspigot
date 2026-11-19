use std::f64::consts::TAU;

use super::impact::{Cell, Vec3};

pub const DEBRIS_CRATER_PER_POWER: f64 = 0.6;
pub const DEBRIS_DEPTH: i32 = 2;
pub const DEBRIS_FRACTION: f64 = 0.3;
pub const DEBRIS_MAX_PER_EXPLOSION: usize = 12;
pub const DEBRIS_MAX_PER_PLAN: usize = 32;
pub const DEBRIS_HORIZONTAL_SPEED_MIN: f64 = 0.35;
pub const DEBRIS_HORIZONTAL_SPEED_MAX: f64 = 0.9;
pub const DEBRIS_VERTICAL_SPEED_MIN: f64 = 0.5;
pub const DEBRIS_VERTICAL_SPEED_MAX: f64 = 1.1;
pub const DEBRIS_DIRECTION_JITTER: f64 = 0.7;
pub const DEBRIS_SPAWN_LIFT: f64 = 0.1;
pub const DEBRIS_PARTICLES: i32 = 4;

const SEED_X: u64 = 73_856_093;
const SEED_Y: u64 = 19_349_663;
const SEED_Z: u64 = 83_492_791;
const SEED_FALLBACK: u64 = 0x9E37_79B9_7F4A_7C15;
const UNIT_BITS: u32 = 53;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng(u64);

impl Rng {
    pub const fn new(seed: u64) -> Self {
        Self(if seed == 0 { SEED_FALLBACK } else { seed })
    }

    pub const fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> (u64::BITS - UNIT_BITS)) as f64 / (1_u64 << UNIT_BITS) as f64
    }
}

pub fn seed_of(cell: Cell, salt: u64) -> u64 {
    (cell.x as u64).wrapping_mul(SEED_X)
        ^ (cell.y as u64).wrapping_mul(SEED_Y)
        ^ (cell.z as u64).wrapping_mul(SEED_Z)
        ^ salt.wrapping_mul(SEED_FALLBACK)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Throw {
    pub from: Cell,
    pub velocity: Vec3,
}

pub fn crater_radius(power: f32) -> i32 {
    ((f64::from(power) * DEBRIS_CRATER_PER_POWER).round() as i32).max(1)
}

pub fn plan_debris(center: Cell, power: f32, seed: u64) -> Vec<Throw> {
    let radius = crater_radius(power);
    let mut rng = Rng::new(seed);
    let mut throws: Vec<Throw> = Vec::new();
    for dx in -radius..=radius {
        for dz in -radius..=radius {
            if dx * dx + dz * dz > radius * radius {
                continue;
            }
            for dy in -DEBRIS_DEPTH..0 {
                if throws.len() >= DEBRIS_MAX_PER_EXPLOSION {
                    return throws;
                }
                if rng.unit() >= DEBRIS_FRACTION {
                    continue;
                }
                throws.push(Throw {
                    from: Cell {
                        x: center.x + dx,
                        y: center.y + dy,
                        z: center.z + dz,
                    },
                    velocity: launch(dx, dz, &mut rng),
                });
            }
        }
    }
    throws
}

fn launch(dx: i32, dz: i32, rng: &mut Rng) -> Vec3 {
    let jitter = (rng.unit() * 2.0 - 1.0) * DEBRIS_DIRECTION_JITTER;
    let angle = if dx == 0 && dz == 0 {
        TAU * rng.unit()
    } else {
        f64::from(dz).atan2(f64::from(dx)) + jitter
    };
    let horizontal = lerp(
        DEBRIS_HORIZONTAL_SPEED_MIN,
        DEBRIS_HORIZONTAL_SPEED_MAX,
        rng.unit(),
    );
    let vertical = lerp(
        DEBRIS_VERTICAL_SPEED_MIN,
        DEBRIS_VERTICAL_SPEED_MAX,
        rng.unit(),
    );
    Vec3 {
        x: angle.cos() * horizontal,
        y: vertical,
        z: angle.sin() * horizontal,
    }
}

fn lerp(min: f64, max: f64, unit: f64) -> f64 {
    min + (max - min) * unit
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    const CENTER: Cell = Cell {
        x: 10,
        y: 64,
        z: -3,
    };
    const DIRECTION_TOLERANCE: f64 = DEBRIS_DIRECTION_JITTER + 1e-9;
    const VELOCITY_CAP: f64 = 3.9;

    fn horizontal_squared(a: Cell, b: Cell) -> i32 {
        (a.x - b.x).pow(2) + (a.z - b.z).pow(2)
    }

    #[test]
    fn the_generator_is_deterministic_and_stays_in_the_unit_interval() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..200 {
            let value = a.unit();
            assert_eq!(value, b.unit());
            assert!((0.0..1.0).contains(&value));
        }
        assert_ne!(Rng::new(1).next_u64(), Rng::new(2).next_u64());
        assert_ne!(Rng::new(0).next_u64(), 0);
    }

    #[test]
    fn a_plan_is_deterministic_for_one_seed_and_differs_between_seeds() {
        assert_eq!(plan_debris(CENTER, 4.0, 7), plan_debris(CENTER, 4.0, 7));
        assert_ne!(plan_debris(CENTER, 4.0, 7), plan_debris(CENTER, 4.0, 8));
    }

    #[test]
    fn a_plan_is_never_empty_and_respects_the_cap() {
        for seed in 1..40 {
            let throws = plan_debris(CENTER, 6.0, seed);
            assert!(throws.len() <= DEBRIS_MAX_PER_EXPLOSION);
        }
        assert!(!plan_debris(CENTER, 4.0, 3).is_empty());
    }

    #[test]
    fn debris_comes_from_below_the_floor_top_inside_the_crater_without_repeats() {
        let radius = crater_radius(4.0);
        for seed in 1..40 {
            let mut origins = HashSet::new();
            for throw in plan_debris(CENTER, 4.0, seed) {
                assert!(horizontal_squared(throw.from, CENTER) <= radius * radius);
                assert!(throw.from.y < CENTER.y && throw.from.y >= CENTER.y - DEBRIS_DEPTH);
                assert!(origins.insert(throw.from));
            }
        }
    }

    #[test]
    fn every_launch_goes_up_and_stays_within_the_speed_ranges_and_the_cap() {
        for seed in 1..60 {
            for throw in plan_debris(CENTER, 6.0, seed) {
                let v = throw.velocity;
                let horizontal = v.x.hypot(v.z);
                assert!(v.y >= DEBRIS_VERTICAL_SPEED_MIN && v.y <= DEBRIS_VERTICAL_SPEED_MAX);
                assert!(v.y > 0.0);
                assert!(
                    (DEBRIS_HORIZONTAL_SPEED_MIN - 1e-9..=DEBRIS_HORIZONTAL_SPEED_MAX + 1e-9)
                        .contains(&horizontal)
                );
                assert!(v.x.abs() < VELOCITY_CAP);
                assert!(v.y.abs() < VELOCITY_CAP);
                assert!(v.z.abs() < VELOCITY_CAP);
            }
        }
    }

    #[test]
    fn the_launch_points_away_from_the_impact_centre_within_the_jitter() {
        let mut checked = 0;
        for seed in 1..60 {
            for throw in plan_debris(CENTER, 6.0, seed) {
                let (dx, dz) = (
                    f64::from(throw.from.x - CENTER.x),
                    f64::from(throw.from.z - CENTER.z),
                );
                if dx == 0.0 && dz == 0.0 {
                    continue;
                }
                let wanted = dz.atan2(dx);
                let actual = throw.velocity.z.atan2(throw.velocity.x);
                let mut gap = (actual - wanted).abs() % TAU;
                if gap > std::f64::consts::PI {
                    gap = TAU - gap;
                }
                assert!(gap <= DIRECTION_TOLERANCE, "gap {gap}");
                checked += 1;
            }
        }
        assert!(checked > 0);
    }

    #[test]
    fn the_jitter_keeps_the_launch_in_the_outward_half_plane() {
        const { assert!(DEBRIS_DIRECTION_JITTER < std::f64::consts::FRAC_PI_2) };
    }

    #[test]
    fn the_speed_caps_stay_below_the_server_clamp() {
        const { assert!(DEBRIS_HORIZONTAL_SPEED_MAX < VELOCITY_CAP) };
        const { assert!(DEBRIS_VERTICAL_SPEED_MAX < VELOCITY_CAP) };
        const { assert!(DEBRIS_HORIZONTAL_SPEED_MIN < DEBRIS_HORIZONTAL_SPEED_MAX) };
        const { assert!(DEBRIS_VERTICAL_SPEED_MIN < DEBRIS_VERTICAL_SPEED_MAX) };
    }

    #[test]
    fn the_crater_grows_with_the_power() {
        assert!(crater_radius(6.0) > crater_radius(2.0));
        assert!(crater_radius(0.1) >= 1);
    }
}
