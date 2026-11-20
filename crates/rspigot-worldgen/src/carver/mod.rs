mod apply;
mod canyon;
mod cave;
mod ellipsoid;
mod mask;
mod provider;
#[cfg(test)]
mod tests;

use serde_json::Value;

use crate::datapack::worldgen;
use crate::legacy::JavaRandom;

pub(crate) use apply::apply;
pub(crate) use canyon::Canyon;
pub(crate) use cave::Cave;
pub(crate) use mask::Mask;

const TOP_MARGIN: i32 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Context {
    pub(crate) min_y: i32,
    pub(crate) depth: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Carver {
    Cave(Cave),
    Canyon(Canyon),
}

impl Carver {
    pub(crate) fn parse(value: &Value) -> Result<Self, String> {
        let name = value.get("type").and_then(Value::as_str).unwrap_or("");
        match name.strip_prefix("minecraft:").unwrap_or(name) {
            "cave" => Cave::parse(value).map(Self::Cave),
            "canyon" => Canyon::parse(value).map(Self::Canyon),
            other => Err(format!("unsupported carver type {other:?}")),
        }
    }

    pub(crate) fn probability(&self) -> f32 {
        match self {
            Self::Cave(cave) => cave.probability,
            Self::Canyon(canyon) => canyon.probability,
        }
    }

    fn carve(
        &self,
        context: Context,
        random: &mut JavaRandom,
        chunk: (i32, i32),
        source: (i32, i32),
        mask: &mut Mask,
    ) {
        match self {
            Self::Cave(cave) => cave.carve(context, random, chunk, source, mask),
            Self::Canyon(canyon) => canyon.carve(context, random, chunk, source, mask),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Carvers {
    list: Vec<Carver>,
}

impl Carvers {
    #[cfg(test)]
    pub(crate) fn none() -> Self {
        Self::default()
    }

    pub(crate) fn from_list(list: Vec<Carver>) -> Self {
        Self { list }
    }

    pub(crate) fn from_ids<S: AsRef<str>>(ids: &[S], table: &Value) -> Result<Self, String> {
        ids.iter()
            .map(|id| {
                let id = id.as_ref();
                let data = table.get(id).ok_or_else(|| format!("no carver {id}"))?;
                Carver::parse(data).map_err(|error| format!("carver {id}: {error}"))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Self::from_list)
    }

    pub(crate) fn for_biome(data: &Value, biome: &str) -> Result<Self, String> {
        let listed = data
            .get("biomes")
            .and_then(|biomes| biomes.get(biome))
            .and_then(|entry| entry.get("carvers"))
            .ok_or_else(|| format!("no carvers for biome {biome}"))?;
        let ids: Vec<&str> = match listed {
            Value::String(id) => vec![id.as_str()],
            Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
            _ => return Err(format!("biome {biome} carvers are malformed")),
        };
        Self::from_ids(&ids, data.get("carvers").unwrap_or(&Value::Null))
    }

    pub(crate) fn overworld() -> Self {
        Self::for_biome(worldgen(), "minecraft:plains")
            .unwrap_or_else(|error| panic!("carver data: {error}"))
    }

    pub(crate) fn nether() -> Self {
        Self::for_biome(worldgen(), "minecraft:nether_wastes")
            .unwrap_or_else(|error| panic!("carver data: {error}"))
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.list.len()
    }

    #[cfg(test)]
    pub(crate) fn get(&self, index: usize) -> Option<&Carver> {
        self.list.get(index)
    }

    pub(crate) fn mask(&self, seed: i64, context: Context, chunk: (i32, i32)) -> Mask {
        let mut mask = Mask::new(
            context.min_y + 1,
            context.min_y + context.depth - 1 - TOP_MARGIN,
        );
        let mut random = JavaRandom::new(0);
        for dx in -8..=8 {
            for dz in -8..=8 {
                let source = (chunk.0 + dx, chunk.1 + dz);
                for (index, carver) in self.list.iter().enumerate() {
                    large_feature_seed(
                        &mut random,
                        seed.wrapping_add(index as i64),
                        source.0,
                        source.1,
                    );
                    if random.next_float() <= carver.probability() {
                        carver.carve(context, &mut random, chunk, source, &mut mask);
                    }
                }
            }
        }
        mask
    }
}

fn large_feature_seed(random: &mut JavaRandom, seed: i64, x: i32, z: i32) {
    random.set_seed(seed);
    let first = random.next_long();
    let second = random.next_long();
    random.set_seed(i64::from(x).wrapping_mul(first) ^ i64::from(z).wrapping_mul(second) ^ seed);
}
