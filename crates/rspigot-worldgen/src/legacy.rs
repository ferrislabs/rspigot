#![allow(dead_code)]

const MULTIPLIER: i64 = 0x5_DEEC_E66D;
const MASK: i64 = (1 << 48) - 1;

#[derive(Debug, Clone)]
pub(crate) struct JavaRandom {
    seed: i64,
    spare_gaussian: Option<f64>,
}

impl JavaRandom {
    pub(crate) fn new(seed: i64) -> Self {
        Self {
            seed: (seed ^ MULTIPLIER) & MASK,
            spare_gaussian: None,
        }
    }

    pub(crate) fn set_seed(&mut self, seed: i64) {
        self.seed = (seed ^ MULTIPLIER) & MASK;
        self.spare_gaussian = None;
    }

    fn next(&mut self, bits: u32) -> i32 {
        self.seed = (self.seed.wrapping_mul(MULTIPLIER).wrapping_add(0xB)) & MASK;
        (self.seed >> (48 - bits)) as i32
    }

    pub(crate) fn next_int(&mut self, bound: i32) -> i32 {
        if bound & (bound - 1) == 0 {
            return ((i64::from(bound) * i64::from(self.next(31))) >> 31) as i32;
        }
        loop {
            let bits = self.next(31);
            let value = bits % bound;
            if bits.wrapping_sub(value).wrapping_add(bound - 1) >= 0 {
                return value;
            }
        }
    }

    pub(crate) fn next_i32(&mut self) -> i32 {
        self.next(32)
    }

    pub(crate) fn between(&mut self, min: i32, max: i32) -> i32 {
        min + self.next_int(max - min + 1)
    }

    pub(crate) fn next_long(&mut self) -> i64 {
        let high = i64::from(self.next(32));
        let low = i64::from(self.next(32));
        (high << 32).wrapping_add(low)
    }

    pub(crate) fn next_boolean(&mut self) -> bool {
        self.next(1) != 0
    }

    pub(crate) fn next_float(&mut self) -> f32 {
        self.next(24) as f32 * 5.960_464_5e-8_f32
    }

    pub(crate) fn next_double(&mut self) -> f64 {
        let high = i64::from(self.next(26));
        let low = i64::from(self.next(27));
        ((high << 27) + low) as f64 * 1.110_223_024_625_156_5e-16
    }

    pub(crate) fn next_gaussian(&mut self) -> f64 {
        if let Some(value) = self.spare_gaussian.take() {
            return value;
        }
        loop {
            let v1 = 2.0 * self.next_double() - 1.0;
            let v2 = 2.0 * self.next_double() - 1.0;
            let s = v1 * v1 + v2 * v2;
            if s < 1.0 && s != 0.0 {
                let multiplier = (-2.0 * s.ln() / s).sqrt();
                self.spare_gaussian = Some(v2 * multiplier);
                return v1 * multiplier;
            }
        }
    }

    pub(crate) fn skip(&mut self, count: u32) {
        for _ in 0..count {
            self.next(32);
        }
    }

    pub(crate) fn set_decoration_seed(&mut self, seed: i64, x: i32, z: i32) -> i64 {
        self.set_seed(seed);
        let a = self.next_long() | 1;
        let b = self.next_long() | 1;
        let population = (i64::from(x)
            .wrapping_mul(a)
            .wrapping_add(i64::from(z).wrapping_mul(b)))
            ^ seed;
        self.set_seed(population);
        population
    }

    pub(crate) fn set_feature_seed(&mut self, population: i64, index: i32, step: i32) {
        self.set_seed(
            population
                .wrapping_add(i64::from(index))
                .wrapping_add(10_000 * i64::from(step)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_java_util_random_sequence() {
        let mut random = JavaRandom::new(42);
        assert_eq!(random.next_int(100), 30);
        assert_eq!(random.next_int(100), 63);
        assert_eq!(random.next_int(100), 48);
        assert_eq!(random.next_int(16), 0);
        assert_eq!(random.next_long(), 5_694_868_678_511_409_995);
        assert_eq!(random.next_double(), 0.277_078_490_074_136_65);
        assert_eq!(random.next_float(), 0.665_548_9);
        assert_eq!(random.next_gaussian(), -0.876_115_483_998_606_3);
        let mut random = JavaRandom::new(0);
        assert_eq!(random.next_long(), -4_962_768_465_676_381_896);
    }
}
