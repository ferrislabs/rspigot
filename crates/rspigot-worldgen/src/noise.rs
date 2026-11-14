use rspigot_core::ports::ChunkGenerator;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{ChunkPos, Vec3};
use rspigot_nbt::Compound;

use crate::dimension::DimensionKind;
use crate::end::End;
use crate::nether::Nether;
use crate::overworld::{NoiseScales, Overworld};
use crate::resources::Resources;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoiseOptions {
    pub biome_scale: f64,
    pub amplitude: f64,
}

impl NoiseOptions {
    pub const NORMAL: Self = Self {
        biome_scale: 1.0,
        amplitude: 1.0,
    };
    pub const LARGE_BIOMES: Self = Self {
        biome_scale: 4.0,
        amplitude: 1.0,
    };
    pub const AMPLIFIED: Self = Self {
        biome_scale: 1.0,
        amplitude: 2.0,
    };
}

#[derive(Debug, Clone)]
struct NoiseState {
    resources: Resources,
    overworld: Overworld,
    nether: Nether,
    end: End,
}

#[derive(Debug, Clone)]
pub struct NoiseGenerator {
    state: Box<NoiseState>,
}

impl NoiseGenerator {
    pub fn new(seed: i64) -> Self {
        Self::with_options(seed, NoiseOptions::NORMAL)
    }

    pub fn with_options(seed: i64, options: NoiseOptions) -> Self {
        let seed = seed as u64;
        let resources = Resources::resolve();
        let scales = NoiseScales {
            biome: options.biome_scale.max(0.25),
            amplitude: options.amplitude.max(0.1),
        };
        Self {
            state: Box::new(NoiseState {
                overworld: Overworld::new(seed, scales, &resources.blocks),
                nether: Nether::new(seed, &resources.blocks),
                end: End::new(seed),
                resources,
            }),
        }
    }
}

impl ChunkGenerator for NoiseGenerator {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        match DimensionKind::of(dimension) {
            DimensionKind::Overworld => {
                self.state
                    .overworld
                    .generate(&self.state.resources, dimension, pos)
            }
            DimensionKind::Nether => {
                self.state
                    .nether
                    .generate(&self.state.resources, dimension, pos)
            }
            DimensionKind::End => self
                .state
                .end
                .generate(&self.state.resources, dimension, pos),
        }
    }

    fn spawn_point(&self, dimension: &Dimension) -> Vec3 {
        match DimensionKind::of(dimension) {
            DimensionKind::Overworld => self
                .state
                .overworld
                .spawn_point(&self.state.resources, dimension),
            DimensionKind::Nether => self
                .state
                .nether
                .spawn_point(&self.state.resources, dimension),
            DimensionKind::End => self.state.end.spawn_point(dimension),
        }
    }

    fn spawns_original_mobs(&self, dimension: &Dimension) -> bool {
        DimensionKind::of(dimension) != DimensionKind::End
    }

    fn structure_entities(&self, dimension: &Dimension, pos: ChunkPos) -> Vec<Compound> {
        match DimensionKind::of(dimension) {
            DimensionKind::End => self.state.end.pillars().crystals(pos),
            DimensionKind::Overworld | DimensionKind::Nether => Vec::new(),
        }
    }
}
