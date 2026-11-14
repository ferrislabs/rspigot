mod buffer;
mod classify;
mod dimension;
mod end;
mod end_spikes;
mod flat;
mod grid;
mod nether;
mod noise;
mod overworld;
mod perlin;
mod resources;
mod rng;
mod veins;
mod void;

use rspigot_core::ports::ChunkGenerator;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{ChunkPos, Vec3};
use rspigot_nbt::Compound;

pub use flat::{FlatGenerator, FlatLayer, FlatSettings, FlatSettingsError};
pub use noise::{NoiseGenerator, NoiseOptions};
pub use void::VoidGenerator;

#[derive(Debug, Clone)]
pub enum WorldGenerator {
    Flat(FlatGenerator),
    Void(VoidGenerator),
    Noise(NoiseGenerator),
}

impl WorldGenerator {
    pub fn from_level_type(level_type: &str, seed: i64, generator_settings: &str) -> Self {
        let lowered = level_type.trim().to_ascii_lowercase();
        let name = lowered.strip_prefix("minecraft:").unwrap_or(&lowered);
        match name {
            "flat" | "superflat" => {
                let flat = if generator_settings.trim().is_empty() {
                    FlatGenerator::vanilla(seed)
                } else {
                    FlatGenerator::from_json(generator_settings, seed)
                        .unwrap_or_else(|_| FlatGenerator::vanilla(seed))
                };
                Self::Flat(flat)
            }
            "void" => Self::Void(VoidGenerator::new(seed)),
            "large_biomes" | "largebiomes" => Self::Noise(NoiseGenerator::with_options(
                seed,
                NoiseOptions::LARGE_BIOMES,
            )),
            "amplified" => Self::Noise(NoiseGenerator::with_options(seed, NoiseOptions::AMPLIFIED)),
            _ => Self::Noise(NoiseGenerator::new(seed)),
        }
    }
}

impl ChunkGenerator for WorldGenerator {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        match self {
            Self::Flat(generator) => generator.generate(dimension, pos),
            Self::Void(generator) => generator.generate(dimension, pos),
            Self::Noise(generator) => generator.generate(dimension, pos),
        }
    }

    fn spawn_point(&self, dimension: &Dimension) -> Vec3 {
        match self {
            Self::Flat(generator) => generator.spawn_point(dimension),
            Self::Void(generator) => generator.spawn_point(dimension),
            Self::Noise(generator) => generator.spawn_point(dimension),
        }
    }

    fn spawns_original_mobs(&self, dimension: &Dimension) -> bool {
        match self {
            Self::Noise(generator) => generator.spawns_original_mobs(dimension),
            Self::Flat(_) | Self::Void(_) => false,
        }
    }

    fn structure_entities(&self, dimension: &Dimension, pos: ChunkPos) -> Vec<Compound> {
        match self {
            Self::Flat(generator) => generator.structure_entities(dimension, pos),
            Self::Void(generator) => generator.structure_entities(dimension, pos),
            Self::Noise(generator) => generator.structure_entities(dimension, pos),
        }
    }
}
