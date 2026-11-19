use std::f64::consts::TAU;

use super::impact::{Cell, Vec3};

pub const DEBRIS_CRATER_PER_POWER: f64 = 0.6;
pub const DEBRIS_DEPTH: i32 = 2;
pub const DEBRIS_FRACTION: f64 = 0.3;
pub const DEBRIS_MAX_PER_EXPLOSION: usize = 12;
pub const DEBRIS_MAX_PER_PLAN: usize = 32;
pub const DEBRIS_MIN_GAP: f64 = 1.5;
pub const DEBRIS_SPREAD: f64 = 6.0;
pub const DEBRIS_LANDING_DELAY_TICKS: u32 = 12;
pub const DEBRIS_DELAY_JITTER_TICKS: u32 = 8;
pub const DEBRIS_PLAYER_CLEARANCE: f64 = 1.2;
pub const DEBRIS_PLAYER_HEIGHT: f64 = 1.8;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Throw {
    pub from: Cell,
    pub to: Cell,
    pub delay_ticks: u32,
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
                let to = landing(center, radius, &mut rng);
                if throws.iter().any(|throw| throw.to == to) {
                    continue;
                }
                throws.push(Throw {
                    from: Cell {
                        x: center.x + dx,
                        y: center.y + dy,
                        z: center.z + dz,
                    },
                    to,
                    delay_ticks: DEBRIS_LANDING_DELAY_TICKS
                        + (rng.unit() * f64::from(DEBRIS_DELAY_JITTER_TICKS + 1)) as u32,
                });
            }
        }
    }
    throws
}

fn landing(center: Cell, radius: i32, rng: &mut Rng) -> Cell {
    let angle = TAU * rng.unit();
    let distance = f64::from(radius) + DEBRIS_MIN_GAP + rng.unit() * DEBRIS_SPREAD;
    Cell {
        x: center.x + (angle.cos() * distance).round() as i32,
        y: center.y,
        z: center.z + (angle.sin() * distance).round() as i32,
    }
}

pub fn blocked_by_player(cell: Cell, players: &[Vec3]) -> bool {
    let (x, z) = (f64::from(cell.x) + 0.5, f64::from(cell.z) + 0.5);
    players.iter().any(|player| {
        (player.x - x).hypot(player.z - z) <= DEBRIS_PLAYER_CLEARANCE
            && player.y - f64::from(cell.y) > -DEBRIS_PLAYER_HEIGHT
            && player.y - f64::from(cell.y) < 1.0
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct Schedule<T> {
    pending: Vec<(u32, Cell, T)>,
}

impl<T> Default for Schedule<T> {
    fn default() -> Self {
        Self {
            pending: Vec::new(),
        }
    }
}

impl<T> Schedule<T> {
    pub fn push(&mut self, delay_ticks: u32, at: Cell, item: T) {
        self.pending.push((delay_ticks, at, item));
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn advance(&mut self) -> Vec<(Cell, T)> {
        let mut due = Vec::new();
        let mut waiting = Vec::with_capacity(self.pending.len());
        for (remaining, at, item) in self.pending.drain(..) {
            let remaining = remaining.saturating_sub(1);
            if remaining == 0 {
                due.push((at, item));
            } else {
                waiting.push((remaining, at, item));
            }
        }
        self.pending = waiting;
        due
    }
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
    fn debris_comes_from_below_the_floor_top_inside_the_crater_and_lands_outside_it() {
        let radius = crater_radius(4.0);
        assert!(radius >= 1);
        for seed in 1..40 {
            let throws = plan_debris(CENTER, 4.0, seed);
            let mut landings = HashSet::new();
            let mut origins = HashSet::new();
            for throw in &throws {
                assert!(horizontal_squared(throw.from, CENTER) <= radius * radius);
                assert!(throw.from.y < CENTER.y && throw.from.y >= CENTER.y - DEBRIS_DEPTH);
                assert!(horizontal_squared(throw.to, CENTER) > radius * radius);
                assert_eq!(throw.to.y, CENTER.y);
                assert!(throw.delay_ticks >= DEBRIS_LANDING_DELAY_TICKS);
                assert!(
                    throw.delay_ticks <= DEBRIS_LANDING_DELAY_TICKS + DEBRIS_DELAY_JITTER_TICKS
                );
                assert!(landings.insert(throw.to));
                assert!(origins.insert(throw.from));
            }
        }
    }

    #[test]
    fn landings_stay_within_the_spread() {
        let radius = f64::from(crater_radius(4.0));
        for seed in 1..40 {
            for throw in plan_debris(CENTER, 4.0, seed) {
                let reach = f64::from(horizontal_squared(throw.to, CENTER)).sqrt();
                assert!(reach <= radius + DEBRIS_MIN_GAP + DEBRIS_SPREAD + 1.0);
            }
        }
    }

    #[test]
    fn the_crater_grows_with_the_power() {
        assert!(crater_radius(6.0) > crater_radius(2.0));
        assert!(crater_radius(0.1) >= 1);
    }

    #[test]
    fn a_landing_next_to_a_player_is_blocked() {
        let cell = Cell { x: 5, y: 64, z: 5 };
        let near = Vec3 {
            x: 5.5,
            y: 64.0,
            z: 5.5,
        };
        let far = Vec3 {
            x: 9.5,
            y: 64.0,
            z: 5.5,
        };
        assert!(blocked_by_player(cell, &[far, near]));
        assert!(!blocked_by_player(cell, &[far]));
        assert!(!blocked_by_player(cell, &[]));
    }

    #[test]
    fn the_schedule_releases_each_item_after_its_delay_exactly_once() {
        let mut schedule = Schedule::default();
        schedule.push(1, CENTER, 'a');
        schedule.push(3, Cell { x: 0, ..CENTER }, 'b');
        assert_eq!(schedule.len(), 2);
        assert_eq!(schedule.advance(), vec![(CENTER, 'a')]);
        assert!(schedule.advance().is_empty());
        assert_eq!(schedule.advance(), vec![(Cell { x: 0, ..CENTER }, 'b')]);
        assert!(schedule.is_empty());
        assert!(schedule.advance().is_empty());
    }
}
