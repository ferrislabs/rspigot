use rspigot_core::ports::ChunkGenerator;
use rspigot_domain::chunk::{ChunkColumn, ChunkSection};
use rspigot_domain::world::Dimension;
use rspigot_domain::{ChunkPos, Vec3};
use rspigot_nbt::Compound;

use crate::dimension::DimensionKind;
use crate::noise::LazyNoise;
use crate::resources::{Biome, BlockKind, Resources};

#[derive(Debug, Clone)]
pub struct VoidGenerator {
    resources: Box<Resources>,
    other_dimensions: LazyNoise,
    every_dimension: bool,
}

impl VoidGenerator {
    pub fn new(seed: i64) -> Self {
        Self {
            resources: Box::new(Resources::resolve()),
            other_dimensions: LazyNoise::new(seed),
            every_dimension: false,
        }
    }

    #[must_use]
    pub fn in_every_dimension(mut self) -> Self {
        self.every_dimension = true;
        self
    }

    fn is_void(&self, dimension: &Dimension) -> bool {
        self.every_dimension || DimensionKind::of(dimension) == DimensionKind::Overworld
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
        if self.is_void(dimension) {
            self.void_column(dimension, pos)
        } else {
            self.other_dimensions.get().generate(dimension, pos)
        }
    }

    fn spawn_point(&self, dimension: &Dimension) -> Vec3 {
        if self.is_void(dimension) {
            Vec3::new(0.5, f64::from(Self::platform_y(dimension) + 1), 0.5)
        } else {
            self.other_dimensions.get().spawn_point(dimension)
        }
    }

    fn structure_entities(&self, dimension: &Dimension, pos: ChunkPos) -> Vec<Compound> {
        if self.is_void(dimension) {
            Vec::new()
        } else {
            self.other_dimensions
                .get()
                .structure_entities(dimension, pos)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_void_world_builds_its_other_dimensions_only_when_one_is_asked_for() {
        let generator = VoidGenerator::new(1);
        let overworld = Dimension::overworld();
        generator.generate(&overworld, ChunkPos::new(0, 0));
        generator.spawn_point(&overworld);
        assert!(
            generator
                .structure_entities(&overworld, ChunkPos::new(3, 3))
                .is_empty()
        );
        assert!(!generator.other_dimensions.is_built());
        generator.spawn_point(&Dimension::nether());
        assert!(generator.other_dimensions.is_built());
    }
}
