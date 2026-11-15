use rspigot_data::multi_noise::NoiseParameters;

use crate::climate::{NoiseStack, Positional, Xoroshiro};
use crate::legacy::JavaRandom;

fn java_hash(name: &str) -> i64 {
    i64::from(name.encode_utf16().fold(0i32, |hash, unit| {
        hash.wrapping_mul(31).wrapping_add(i32::from(unit))
    }))
}

fn position_seed(x: i32, y: i32, z: i32) -> i64 {
    let mut l = i64::from(x.wrapping_mul(3_129_871))
        ^ i64::from(z).wrapping_mul(116_129_781)
        ^ i64::from(y);
    l = l
        .wrapping_mul(l)
        .wrapping_mul(42_317_861)
        .wrapping_add(l.wrapping_mul(11));
    l >> 16
}

#[derive(Debug, Clone)]
pub(crate) enum SurfaceRandom {
    Modern(Positional),
    Legacy(i64),
}

pub(crate) enum Drawn {
    Modern(Xoroshiro),
    Legacy(JavaRandom),
}

impl Drawn {
    pub(crate) fn next_double(&mut self) -> f64 {
        match self {
            Self::Modern(random) => random.next_double(),
            Self::Legacy(random) => random.next_double(),
        }
    }

    pub(crate) fn next_float(&mut self) -> f32 {
        match self {
            Self::Modern(random) => random.next_float(),
            Self::Legacy(random) => random.next_float(),
        }
    }
}

impl SurfaceRandom {
    pub(crate) fn modern(seed: i64) -> Self {
        Self::Modern(Xoroshiro::from_seed(seed).fork_positional())
    }

    pub(crate) fn legacy(seed: i64) -> Self {
        Self::Legacy(JavaRandom::new(seed).next_long())
    }

    pub(crate) fn at(&self, x: i32, y: i32, z: i32) -> Drawn {
        match self {
            Self::Modern(positional) => Drawn::Modern(positional.at(x, y, z)),
            Self::Legacy(seed) => Drawn::Legacy(JavaRandom::new(position_seed(x, y, z) ^ seed)),
        }
    }

    pub(crate) fn named_positional(&self, name: &str) -> Self {
        match self {
            Self::Modern(positional) => Self::Modern(positional.named(name).fork_positional()),
            Self::Legacy(seed) => Self::legacy(java_hash(name) ^ seed),
        }
    }

    pub(crate) fn noise(&self, parameters: &NoiseParameters, name: &str) -> NoiseStack {
        match self {
            Self::Modern(positional) => NoiseStack::create(parameters, &mut positional.named(name)),
            Self::Legacy(seed) => NoiseStack::create_legacy(parameters, java_hash(name) ^ seed),
        }
    }
}
