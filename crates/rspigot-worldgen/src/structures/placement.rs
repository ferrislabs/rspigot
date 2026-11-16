use std::sync::OnceLock;

use serde_json::Value;

use super::data::short;
use crate::legacy::JavaRandom;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Spread {
    Linear,
    Triangular,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reduction {
    Default,
    LegacyOne,
    LegacyTwo,
    LegacyThree,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Kind {
    RandomSpread {
        spacing: i32,
        separation: i32,
        spread: Spread,
    },
    Rings {
        distance: i32,
        spread: i32,
        count: i32,
        preferred: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Exclusion {
    pub(crate) other: String,
    pub(crate) chunks: i32,
}

#[derive(Debug)]
pub(crate) struct Placement {
    pub(crate) kind: Kind,
    pub(crate) reduction: Reduction,
    pub(crate) frequency: f32,
    pub(crate) salt: i32,
    pub(crate) exclusion: Option<Exclusion>,
    rings: OnceLock<Vec<(i32, i32)>>,
}

pub(crate) trait BiomeProbe {
    fn at(&self, qx: i32, qy: i32, qz: i32) -> u8;
    fn id(&self, name: &str) -> Option<u8>;
}

pub(crate) trait Neighbours {
    fn has_structure_chunk(&self, set: &str, x: i32, z: i32, range: i32) -> bool;
}

fn large_feature_with_salt(random: &mut JavaRandom, seed: i64, x: i32, z: i32, salt: i32) {
    random.set_seed(
        i64::from(x)
            .wrapping_mul(341_873_128_712)
            .wrapping_add(i64::from(z).wrapping_mul(132_897_987_541))
            .wrapping_add(seed)
            .wrapping_add(i64::from(salt)),
    );
}

pub(crate) fn large_feature_seed(random: &mut JavaRandom, seed: i64, x: i32, z: i32) {
    random.set_seed(seed);
    let a = random.next_long();
    let b = random.next_long();
    random.set_seed(i64::from(x).wrapping_mul(a) ^ i64::from(z).wrapping_mul(b) ^ seed);
}

impl Reduction {
    fn parse(name: &str) -> Self {
        match name {
            "legacy_type_1" => Self::LegacyOne,
            "legacy_type_2" => Self::LegacyTwo,
            "legacy_type_3" => Self::LegacyThree,
            _ => Self::Default,
        }
    }

    fn should_generate(self, seed: i64, salt: i32, x: i32, z: i32, probability: f32) -> bool {
        let mut random = JavaRandom::new(0);
        match self {
            Self::Default => {
                large_feature_with_salt(&mut random, seed, x, z, salt);
                random.next_float() < probability
            }
            Self::LegacyOne => {
                let (i, j) = (x >> 4, z >> 4);
                random.set_seed(i64::from(i ^ (j << 4)) ^ seed);
                random.skip(1);
                random.next_int((1.0_f32 / probability) as i32) == 0
            }
            Self::LegacyTwo => {
                large_feature_with_salt(&mut random, seed, x, z, 10_387_320);
                random.next_float() < probability
            }
            Self::LegacyThree => {
                large_feature_seed(&mut random, seed, x, z);
                random.next_double() < f64::from(probability)
            }
        }
    }
}

fn int(value: &Value, key: &str) -> i32 {
    value
        .get(key)
        .and_then(Value::as_i64)
        .and_then(|n| i32::try_from(n).ok())
        .unwrap_or(0)
}

impl Placement {
    pub(crate) fn parse(value: &Value) -> Option<Self> {
        let kind = match short(value.get("type")?.as_str()?) {
            "random_spread" => Kind::RandomSpread {
                spacing: int(value, "spacing"),
                separation: int(value, "separation"),
                spread: match value.get("spread_type").and_then(Value::as_str) {
                    Some("triangular") => Spread::Triangular,
                    _ => Spread::Linear,
                },
            },
            "concentric_rings" => Kind::Rings {
                distance: int(value, "distance"),
                spread: int(value, "spread"),
                count: int(value, "count"),
                preferred: value
                    .get("preferred_biomes")
                    .and_then(Value::as_array)
                    .map(|names| {
                        names
                            .iter()
                            .filter_map(Value::as_str)
                            .map(|name| short(name).to_owned())
                            .collect()
                    })
                    .unwrap_or_default(),
            },
            _ => return None,
        };
        let exclusion = value.get("exclusion_zone").map(|zone| Exclusion {
            other: short(zone.get("other_set").and_then(Value::as_str).unwrap_or("")).to_owned(),
            chunks: int(zone, "chunk_count"),
        });
        Some(Self {
            kind,
            reduction: Reduction::parse(
                value
                    .get("frequency_reduction_method")
                    .and_then(Value::as_str)
                    .unwrap_or("default"),
            ),
            frequency: value
                .get("frequency")
                .and_then(Value::as_f64)
                .map_or(1.0, |f| f as f32),
            salt: int(value, "salt"),
            exclusion,
            rings: OnceLock::new(),
        })
    }

    pub(crate) fn potential_chunk(
        &self,
        seed: i64,
        chunk_x: i32,
        chunk_z: i32,
    ) -> Option<(i32, i32)> {
        let Kind::RandomSpread {
            spacing,
            separation,
            spread,
        } = self.kind
        else {
            return None;
        };
        let (x, z) = (chunk_x.div_euclid(spacing), chunk_z.div_euclid(spacing));
        let mut random = JavaRandom::new(0);
        large_feature_with_salt(&mut random, seed, x, z, self.salt);
        let limit = spacing - separation;
        let evaluate = |random: &mut JavaRandom| match spread {
            Spread::Linear => random.next_int(limit),
            Spread::Triangular => (random.next_int(limit) + random.next_int(limit)) / 2,
        };
        let offset_x = evaluate(&mut random);
        let offset_z = evaluate(&mut random);
        Some((x * spacing + offset_x, z * spacing + offset_z))
    }

    pub(crate) fn rings(&self, seed: i64, biomes: &impl BiomeProbe) -> &[(i32, i32)] {
        self.rings.get_or_init(|| self.compute_rings(seed, biomes))
    }

    fn compute_rings(&self, seed: i64, biomes: &impl BiomeProbe) -> Vec<(i32, i32)> {
        let Kind::Rings {
            distance,
            spread,
            count,
            ref preferred,
        } = self.kind
        else {
            return Vec::new();
        };
        if count == 0 {
            return Vec::new();
        }
        let wanted: Vec<u8> = preferred
            .iter()
            .filter_map(|name| biomes.id(name))
            .collect();
        let mut random = JavaRandom::new(seed);
        let mut angle = random.next_double() * std::f64::consts::PI * 2.0;
        let (mut in_ring, mut ring, mut spread) = (0, 0, spread);
        let mut out = Vec::with_capacity(count as usize);
        for placed in 0..count {
            let base = f64::from(distance * 4 + distance * ring * 6);
            let radius = base + (random.next_double() - 0.5) * f64::from(distance) * 2.5;
            let x = java_round(angle.cos() * radius);
            let z = java_round(angle.sin() * radius);
            let mut forked = JavaRandom::new(random.next_long());
            out.push(find_ring_chunk(biomes, &wanted, x, z, &mut forked));
            angle += std::f64::consts::PI * 2.0 / f64::from(spread);
            in_ring += 1;
            if in_ring == spread {
                ring += 1;
                in_ring = 0;
                spread = (spread + (spread * 2) / (ring + 1)).min(count - placed);
                angle += random.next_double() * std::f64::consts::PI * 2.0;
            }
        }
        out
    }

    pub(crate) fn is_structure_chunk(
        &self,
        seed: i64,
        biomes: &impl BiomeProbe,
        others: &impl Neighbours,
        x: i32,
        z: i32,
    ) -> bool {
        let placement = match self.kind {
            Kind::RandomSpread { .. } => self.potential_chunk(seed, x, z) == Some((x, z)),
            Kind::Rings { .. } => self.rings(seed, biomes).contains(&(x, z)),
        };
        placement
            && (self.frequency >= 1.0
                || self
                    .reduction
                    .should_generate(seed, self.salt, x, z, self.frequency))
            && self
                .exclusion
                .as_ref()
                .is_none_or(|zone| !others.has_structure_chunk(&zone.other, x, z, zone.chunks))
    }
}

fn java_round(value: f64) -> i32 {
    (value + 0.5).floor() as i64 as i32
}

fn find_ring_chunk(
    biomes: &impl BiomeProbe,
    wanted: &[u8],
    chunk_x: i32,
    chunk_z: i32,
    random: &mut JavaRandom,
) -> (i32, i32) {
    let (qx, qz) = ((chunk_x * 16 + 8) >> 2, (chunk_z * 16 + 8) >> 2);
    let radius = 112 >> 2;
    let mut found: Option<(i32, i32)> = None;
    let mut seen = 0;
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            let (x, z) = (qx + dx, qz + dz);
            if wanted.contains(&biomes.at(x, 0, z)) {
                if found.is_none() || random.next_int(seen + 1) == 0 {
                    found = Some((x, z));
                }
                seen += 1;
            }
        }
    }
    match found {
        Some((x, z)) => (((x << 2) >> 4), ((z << 2) >> 4)),
        None => (chunk_x, chunk_z),
    }
}
