use md5::{Digest, Md5};

const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;
const SILVER: u64 = 0x6A09_E667_F3BC_C909;

const fn mix_stafford13(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Seed128 {
    lo: u64,
    hi: u64,
}

impl Seed128 {
    fn upgrade(seed: i64) -> Self {
        let lo = (seed as u64) ^ SILVER;
        let hi = lo.wrapping_add(GOLDEN);
        Self {
            lo: mix_stafford13(lo),
            hi: mix_stafford13(hi),
        }
    }

    fn of_hash(name: &str) -> Self {
        let digest = Md5::digest(name.as_bytes());
        let lo = u64::from_be_bytes(digest[0..8].try_into().unwrap_or_default());
        let hi = u64::from_be_bytes(digest[8..16].try_into().unwrap_or_default());
        Self { lo, hi }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Xoroshiro {
    lo: u64,
    hi: u64,
    spare: Option<f64>,
}

impl Xoroshiro {
    fn from_parts(lo: u64, hi: u64) -> Self {
        if lo == 0 && hi == 0 {
            Self {
                lo: GOLDEN,
                hi: SILVER,
                spare: None,
            }
        } else {
            Self {
                lo,
                hi,
                spare: None,
            }
        }
    }

    pub(crate) fn from_seed(seed: i64) -> Self {
        let seed = Seed128::upgrade(seed);
        Self::from_parts(seed.lo, seed.hi)
    }

    pub(crate) fn next_long(&mut self) -> u64 {
        let (lo, mut hi) = (self.lo, self.hi);
        let result = lo.wrapping_add(hi).rotate_left(17).wrapping_add(lo);
        hi ^= lo;
        self.lo = lo.rotate_left(49) ^ hi ^ (hi << 21);
        self.hi = hi.rotate_left(28);
        result
    }

    pub(crate) fn reseed(&mut self, seed: i64) {
        *self = Self::from_seed(seed);
    }

    pub(crate) fn below(&mut self, bound: i32) -> i32 {
        if bound <= 0 {
            return 0;
        }
        self.next_int(bound as u32) as i32
    }

    pub(crate) fn next_gaussian(&mut self) -> f64 {
        if let Some(value) = self.spare.take() {
            return value;
        }
        loop {
            let a = 2.0 * self.next_double() - 1.0;
            let b = 2.0 * self.next_double() - 1.0;
            let s = a * a + b * b;
            if s < 1.0 && s != 0.0 {
                let factor = (-2.0 * s.ln() / s).sqrt();
                self.spare = Some(b * factor);
                return a * factor;
            }
        }
    }

    pub(crate) fn next_int(&mut self, bound: u32) -> u32 {
        let draw = |rng: &mut Self| u64::from(rng.next_long() as u32);
        let mut product = draw(self) * u64::from(bound);
        let mut low = product & 0xFFFF_FFFF;
        if low < u64::from(bound) {
            let threshold = u64::from(bound.wrapping_neg() % bound);
            while low < threshold {
                product = draw(self) * u64::from(bound);
                low = product & 0xFFFF_FFFF;
            }
        }
        (product >> 32) as u32
    }

    pub(crate) fn next_float(&mut self) -> f32 {
        (self.next_long() >> 40) as f32 * 5.960_464_5e-8_f32
    }

    pub(crate) fn next_boolean(&mut self) -> bool {
        self.next_long() & 1 != 0
    }

    pub(crate) fn between(&mut self, min: i32, max: i32) -> i32 {
        min + self.next_int((max - min + 1) as u32) as i32
    }

    pub(crate) fn next_double(&mut self) -> f64 {
        (self.next_long() >> 11) as f64 * 1.110_223_024_625_156_5e-16
    }

    pub(crate) fn fork_positional(&mut self) -> Positional {
        let lo = self.next_long();
        let hi = self.next_long();
        Positional { lo, hi }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Positional {
    lo: u64,
    hi: u64,
}

impl Positional {
    pub(crate) fn at(&self, x: i32, y: i32, z: i32) -> Xoroshiro {
        let mut l = i64::from(x.wrapping_mul(3_129_871))
            ^ i64::from(z).wrapping_mul(116_129_781)
            ^ i64::from(y);
        l = l
            .wrapping_mul(l)
            .wrapping_mul(42_317_861)
            .wrapping_add(l.wrapping_mul(11));
        Xoroshiro::from_parts(((l >> 16) as u64) ^ self.lo, self.hi)
    }

    pub(crate) fn named(&self, name: &str) -> Xoroshiro {
        let seed = Seed128::of_hash(name);
        Xoroshiro::from_parts(seed.lo ^ self.lo, seed.hi ^ self.hi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_state_is_replaced() {
        let mut rng = Xoroshiro::from_parts(0, 0);
        assert_ne!(rng.next_long(), 0);
    }

    #[test]
    fn next_int_stays_in_bound() {
        let mut rng = Xoroshiro::from_seed(5);
        for bound in 1..300 {
            assert!(rng.next_int(bound) < bound);
        }
    }

    #[test]
    fn next_double_is_half_open() {
        let mut rng = Xoroshiro::from_seed(-9);
        for _ in 0..1000 {
            assert!((0.0..1.0).contains(&rng.next_double()));
        }
    }
}
