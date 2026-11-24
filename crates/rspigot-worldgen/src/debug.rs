use rspigot_blocks::STATE_COUNT;
use rspigot_core::ports::ChunkGenerator;
use rspigot_domain::chunk::{ChunkColumn, ChunkSection};
use rspigot_domain::world::Dimension;
use rspigot_domain::{BlockStateId, ChunkPos, Vec3};
use rspigot_nbt::Compound;

use crate::dimension::DimensionKind;
use crate::noise::LazyNoise;
use crate::resources::{Biome, Resources};

const BARRIER_Y: i32 = 60;
const STATE_Y: i32 = 70;
const SPAWN_HEIGHT: f64 = 64.0;

#[derive(Debug, Clone)]
pub struct DebugGenerator {
    resources: Box<Resources>,
    other_dimensions: LazyNoise,
    width: i32,
    height: i32,
}

pub(crate) fn grid_size(count: usize) -> (i32, i32) {
    let width = (count as f32).sqrt().ceil() as i32;
    let height = (count as f32 / width as f32).ceil() as i32;
    (width, height)
}

impl DebugGenerator {
    pub fn new(seed: i64) -> Self {
        let (width, height) = grid_size(STATE_COUNT);
        Self {
            resources: Box::new(Resources::resolve()),
            other_dimensions: LazyNoise::new(seed),
            width,
            height,
        }
    }

    pub fn block_state_for(&self, x: i32, z: i32) -> BlockStateId {
        if x > 0 && z > 0 && x % 2 != 0 && z % 2 != 0 {
            let (gx, gz) = (x / 2, z / 2);
            if gx <= self.width && gz <= self.height {
                let index = (gx * self.width + gz).unsigned_abs() as usize;
                if index < STATE_COUNT {
                    return BlockStateId(index as u16);
                }
            }
        }
        BlockStateId::AIR
    }

    fn column(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let height = dimension.height;
        let classifier = &self.resources.classifier;
        let biome = self.resources.biomes[Biome::Plains];
        let barrier = rspigot_blocks::BlockKind::Barrier.default_state();
        let mut sections: Vec<ChunkSection> = (0..height.section_count())
            .map(|_| ChunkSection::empty(biome))
            .collect();
        let relative = |y: i32| (y - height.min_y) as usize;
        let size = height.height as usize;
        let mut base = vec![0u16; size];
        base[relative(BARRIER_Y)] = barrier.0;
        let mut heightmaps = classifier.heightmaps_1d(&base);
        for dz in 0..16 {
            for dx in 0..16 {
                let (x, z) = (pos.min_block_x() + dx, pos.min_block_z() + dz);
                sections[relative(BARRIER_Y) >> 4].set_block(
                    dx as usize,
                    relative(BARRIER_Y) & 15,
                    dz as usize,
                    barrier,
                );
                let state = self.block_state_for(x, z);
                if state.is_air() {
                    continue;
                }
                sections[relative(STATE_Y) >> 4].set_block(
                    dx as usize,
                    relative(STATE_Y) & 15,
                    dz as usize,
                    state,
                );
                let mut column = base.clone();
                column[relative(STATE_Y)] = state.0;
                for (kind, source) in &classifier.heightmaps_1d(&column).maps {
                    heightmaps
                        .get_or_insert(*kind)
                        .set(dx as usize, dz as usize, source.get(0, 0));
                }
            }
        }
        let mut result = ChunkColumn::from_sections(pos, height, sections)
            .unwrap_or_else(|| ChunkColumn::empty(pos, height, biome));
        result.heightmaps = heightmaps;
        result
    }
}

impl ChunkGenerator for DebugGenerator {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        if DimensionKind::of(dimension) == DimensionKind::Overworld {
            self.column(dimension, pos)
        } else {
            self.other_dimensions.get().generate(dimension, pos)
        }
    }

    fn spawn_point(&self, dimension: &Dimension) -> Vec3 {
        if DimensionKind::of(dimension) == DimensionKind::Overworld {
            Vec3::new(0.5, SPAWN_HEIGHT, 0.5)
        } else {
            self.other_dimensions.get().spawn_point(dimension)
        }
    }

    fn structure_entities(&self, dimension: &Dimension, pos: ChunkPos) -> Vec<Compound> {
        if DimensionKind::of(dimension) == DimensionKind::Overworld {
            return Vec::new();
        }
        self.other_dimensions
            .get()
            .structure_entities(dimension, pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_debug_world_builds_its_other_dimensions_only_when_one_is_asked_for() {
        let generator = DebugGenerator::new(1);
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

    #[test]
    fn the_grid_is_the_ceiling_of_the_square_root() {
        assert_eq!(grid_size(100), (10, 10));
        assert_eq!(grid_size(101), (11, 10));
        let (width, height) = grid_size(STATE_COUNT);
        assert!(width * height >= STATE_COUNT as i32);
        assert!(
            (width - 1) * height < STATE_COUNT as i32
                || (width - 1) * (height - 1) < STATE_COUNT as i32
        );
    }

    #[test]
    fn every_block_state_sits_on_the_odd_cells_of_the_grid_in_registry_order() {
        let generator = DebugGenerator::new(1);
        assert_eq!(generator.block_state_for(0, 1), BlockStateId::AIR);
        assert_eq!(generator.block_state_for(2, 3), BlockStateId::AIR);
        assert_eq!(generator.block_state_for(-3, 3), BlockStateId::AIR);
        assert_eq!(generator.block_state_for(1, 1), BlockStateId(0));
        assert_eq!(generator.block_state_for(1, 3), BlockStateId(1));
        assert_eq!(generator.block_state_for(1, 5), BlockStateId(2));
        let width = generator.width;
        assert_eq!(generator.block_state_for(3, 1), BlockStateId(width as u16));
    }

    #[test]
    fn the_overworld_is_a_barrier_floor_with_the_states_floating_above() {
        let generator = DebugGenerator::new(1);
        let dimension = Dimension::overworld();
        let column = generator.generate(&dimension, ChunkPos::new(0, 0));
        let at = |x: i32, y: i32, z: i32| column.block(rspigot_domain::BlockPos::new(x, y, z));
        let barrier = rspigot_blocks::BlockKind::Barrier.default_state();
        assert_eq!(at(4, BARRIER_Y, 4), Some(barrier));
        assert_eq!(at(4, STATE_Y, 4), Some(BlockStateId::AIR));
        assert_eq!(at(1, STATE_Y, 1), Some(BlockStateId(0)));
        assert_eq!(
            at(3, STATE_Y, 1),
            Some(BlockStateId(generator.width as u16))
        );
        assert_eq!(at(1, 65, 1), Some(BlockStateId::AIR));
        let spawn = generator.spawn_point(&dimension);
        assert!(spawn.y > f64::from(BARRIER_Y));
    }
}
