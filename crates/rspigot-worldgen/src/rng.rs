const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;

const fn finalize(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

pub(crate) const fn mix(z: u64) -> u64 {
    finalize(z.wrapping_add(GOLDEN))
}

pub(crate) fn hash3(seed: u64, x: i32, y: i32, z: i32) -> u64 {
    let h = mix(seed ^ u64::from(x as u32));
    let h = mix(h ^ u64::from(y as u32).rotate_left(21));
    mix(h ^ u64::from(z as u32).rotate_left(42))
}

pub(crate) fn unit(hash: u64) -> f64 {
    (hash >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0)
}

#[derive(Debug, Clone)]
pub(crate) struct Rng {
    state: u64,
}

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Self { state: mix(seed) }
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GOLDEN);
        finalize(self.state)
    }

    #[cfg(test)]
    pub(crate) fn skip(&mut self, draws: u64) {
        self.state = self.state.wrapping_add(GOLDEN.wrapping_mul(draws));
    }

    pub(crate) fn below(&mut self, bound: u32) -> u32 {
        (((self.next_u64() >> 32) * u64::from(bound)) >> 32) as u32
    }

    #[cfg(test)]
    pub(crate) fn range(&mut self, low: i32, high: i32) -> i32 {
        low + self.below((high - low + 1).max(1) as u32) as i32
    }

    pub(crate) fn unit(&mut self) -> f64 {
        unit(self.next_u64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..64 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn skip_matches_drawing() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..13 {
            a.next_u64();
        }
        b.skip(13);
        assert_eq!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn below_stays_in_bound() {
        let mut rng = Rng::new(1);
        for bound in 1..50 {
            for _ in 0..50 {
                assert!(rng.below(bound) < bound);
            }
        }
        assert_eq!(rng.below(0), 0);
    }

    #[test]
    fn unit_is_half_open() {
        let mut rng = Rng::new(9);
        for _ in 0..1000 {
            let v = rng.unit();
            assert!((0.0..1.0).contains(&v));
        }
    }

    #[test]
    fn range_is_inclusive() {
        let mut rng = Rng::new(3);
        let mut seen = [false; 4];
        for _ in 0..200 {
            seen[(rng.range(-1, 2) + 1) as usize] = true;
        }
        assert!(seen.iter().all(|s| *s));
    }

    #[test]
    fn hashes_depend_on_every_input() {
        let base = hash3(1, 2, 3, 4);
        assert_ne!(base, hash3(2, 2, 3, 4));
        assert_ne!(base, hash3(1, 3, 3, 4));
        assert_ne!(base, hash3(1, 2, 4, 4));
        assert_ne!(base, hash3(1, 2, 3, 5));
    }
}
