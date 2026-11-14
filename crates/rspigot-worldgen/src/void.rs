use rspigot_core::ports::ChunkGenerator;
use rspigot_domain::chunk::{ChunkColumn, ChunkSection};
use rspigot_domain::world::Dimension;
use rspigot_domain::{ChunkPos, Vec3};
use rspigot_nbt::Compound;

use crate::dimension::DimensionKind;
use crate::noise::NoiseGenerator;
use crate::resources::{Biome, BlockKind, Resources};

#[derive(Debug, Clone)]
pub struct VoidGenerator {
    resources: Box<Resources>,
    other_dimensions: Box<NoiseGenerator>,
}

impl VoidGenerator {
    pub fn new(seed: i64) -> Self {
        Self {
            resources: Box::new(Resources::resolve()),
            other_dimensions: Box::new(NoiseGenerator::new(seed)),
        }
    }

    fn platform_y(dimension: &Dimension) -> i32 {
        dimension
            .sea_level
            .clamp(dimension.height.min_y, dimension.height.max_y())
    }

    fn void_column(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let height = dimension.height;
        let classifier = &self.resources.classifier;
        let biome = self.resources.biomes[Biome::TheVoid];
        let mut sections: Vec<ChunkSection> = (0..height.section_count())
            .map(|_| ChunkSection::empty(biome))
            .collect();
        let mut heightmaps = classifier.heightmaps_1d(&[]);
        if pos == ChunkPos::new(0, 0) {
            let relative = (Self::platform_y(dimension) - height.min_y) as usize;
            let stone = self.resources.blocks[BlockKind::Stone];
            sections[relative >> 4].set_block(0, relative & 15, 0, stone);
            let mut column = vec![0u16; height.height as usize];
            column[relative] = stone.0;
            for (kind, source) in &classifier.heightmaps_1d(&column).maps {
                heightmaps.get_or_insert(*kind).set(0, 0, source.get(0, 0));
            }
        }
        let mut result = ChunkColumn::from_sections(pos, height, sections)
            .unwrap_or_else(|| ChunkColumn::empty(pos, height, biome));
        result.heightmaps = heightmaps;
        result
    }
}

impl ChunkGenerator for VoidGenerator {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        match DimensionKind::of(dimension) {
            DimensionKind::Overworld => self.void_column(dimension, pos),
            DimensionKind::Nether | DimensionKind::End => {
                self.other_dimensions.generate(dimension, pos)
            }
        }
    }

    fn spawn_point(&self, dimension: &Dimension) -> Vec3 {
        match DimensionKind::of(dimension) {
            DimensionKind::Overworld => {
                Vec3::new(0.5, f64::from(Self::platform_y(dimension) + 1), 0.5)
            }
            DimensionKind::Nether | DimensionKind::End => {
                self.other_dimensions.spawn_point(dimension)
            }
        }
    }

    fn structure_entities(&self, dimension: &Dimension, pos: ChunkPos) -> Vec<Compound> {
        self.other_dimensions.structure_entities(dimension, pos)
    }
}
