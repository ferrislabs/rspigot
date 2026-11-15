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
}

impl Xoroshiro {
    fn from_parts(lo: u64, hi: u64) -> Self {
        if lo == 0 && hi == 0 {
            Self {
                lo: GOLDEN,
                hi: SILVER,
            }
        } else {
            Self { lo, hi }
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
    pub(crate) fn from_hash_of(&self, name: &str) -> Xoroshiro {
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
