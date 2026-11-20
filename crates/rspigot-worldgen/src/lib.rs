mod buffer;
mod classify;
mod climate;
mod datapack;
mod dimension;
mod end;
mod end_spikes;
mod flat;
mod legacy;
mod locate;
mod nether;
mod noise;
mod overworld;
mod placing;
mod resources;
mod simplex;
mod structures;
mod temperature;
mod terrain;
mod void;

use rspigot_core::ports::{ChunkGenerator, PlaceFailure, PlaceRequest, PlacedBlock};
use rspigot_core::worlds::{GeneratorSpec, WorldSpec};
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{ChunkPos, Vec3};
use rspigot_nbt::Compound;

pub use flat::{FlatGenerator, FlatLayer, FlatSettings, FlatSettingsError};
pub use noise::{NoiseGenerator, NoiseOptions};
pub use structures::{LoadIssue, install_datapacks};
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

impl WorldGenerator {
    pub fn for_world(spec: &WorldSpec) -> Result<Self, String> {
        let seed = spec.seed;
        match &spec.generator {
            GeneratorSpec::Void => Ok(Self::Void(VoidGenerator::new(seed).in_every_dimension())),
            GeneratorSpec::Flat(layers) => {
                let settings = FlatSettings {
                    biome: "minecraft:plains".to_owned(),
                    layers: layers
                        .iter()
                        .map(|layer| FlatLayer {
                            block: layer.block.clone(),
                            height: layer.height,
                        })
                        .collect(),
                };
                FlatGenerator::new(&settings, seed)
                    .map(|flat| Self::Flat(flat.in_every_dimension()))
                    .map_err(|error| error.to_string())
            }
            GeneratorSpec::Vanilla => Ok(Self::noise(seed, NoiseOptions::NORMAL, spec.structures)),
            GeneratorSpec::Datapack(id) => {
                let options = match id.as_str() {
                    "minecraft:overworld" => NoiseOptions::NORMAL,
                    "minecraft:large_biomes" => NoiseOptions::LARGE_BIOMES,
                    "minecraft:amplified" => NoiseOptions::AMPLIFIED,
                    other => {
                        return Err(format!("no noise settings and biome source named {other}"));
                    }
                };
                Ok(Self::noise(seed, options, spec.structures))
            }
        }
    }

    fn noise(seed: i64, options: NoiseOptions, structures: bool) -> Self {
        let generator = NoiseGenerator::with_options(seed, options);
        Self::Noise(if structures {
            generator
        } else {
            generator.without_structures()
        })
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

    fn nearest_structure(
        &self,
        dimension: &Dimension,
        from: Vec3,
        structure_id: &str,
    ) -> Option<(String, Vec3)> {
        match self {
            Self::Noise(generator) => generator.nearest_structure(dimension, from, structure_id),
            Self::Flat(_) | Self::Void(_) => None,
        }
    }

    fn nearest_structures(
        &self,
        dimension: &Dimension,
        from: Vec3,
        structure_ids: &[String],
    ) -> Option<(String, Vec3)> {
        match self {
            Self::Noise(generator) => generator.nearest_structures(dimension, from, structure_ids),
            Self::Flat(_) | Self::Void(_) => None,
        }
    }

    fn knows_structure(&self, dimension: &Dimension, structure_id: &str) -> bool {
        match self {
            Self::Noise(generator) => generator.knows_structure(dimension, structure_id),
            Self::Flat(_) | Self::Void(_) => false,
        }
    }

    fn nearest_biome(
        &self,
        dimension: &Dimension,
        from: Vec3,
        biomes: &[String],
    ) -> Option<(String, Vec3)> {
        match self {
            Self::Noise(generator) => generator.nearest_biome(dimension, from, biomes),
            Self::Flat(_) | Self::Void(_) => None,
        }
    }

    fn place(
        &self,
        dimension: &Dimension,
        request: &PlaceRequest,
    ) -> Result<Vec<PlacedBlock>, PlaceFailure> {
        match self {
            Self::Noise(generator) => generator.place(dimension, request),
            Self::Flat(_) | Self::Void(_) => placing::standalone(dimension, request),
        }
    }

    fn nearest_stronghold(&self, dimension: &Dimension, from: Vec3) -> Option<Vec3> {
        match self {
            Self::Noise(generator) => generator.nearest_stronghold(dimension, from),
            Self::Flat(_) | Self::Void(_) => None,
        }
    }
}
