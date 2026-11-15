use std::sync::{Arc, Mutex};

use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, Vec3};

mod islands;
#[cfg(test)]
mod reference;

use crate::buffer::ChunkBuffer;
use crate::end_spikes::Pillars;
use crate::nether::density::{BlendedNoise, Corners, END_BLEND, END_LATTICE, clamped_map, lerp};
use crate::overworld::{ChunkBiomes, Features, Region, SurfaceSystem, TerrainCache, TerrainChunk};
use crate::resources::{BlockKind, Palette, Resources, biome_id};
use islands::EndIslands;

pub(crate) const END_BIOMES: [&str; 5] = [
    "minecraft:the_end",
    "minecraft:end_highlands",
    "minecraft:end_midlands",
    "minecraft:small_end_islands",
    "minecraft:end_barrens",
];

const NOISE_HEIGHT: i32 = 128;
const FINAL_SCALE: f32 = 0.64;
const CACHE_CAPACITY: usize = 64;
const CENTRAL_SECTIONS: i64 = 4096;

#[derive(Debug, Clone)]
pub(crate) struct End {
    seed: u64,
    blended: BlendedNoise,
    islands: EndIslands,
    pillars: Pillars,
    surface: Arc<SurfaceSystem>,
    features: Arc<Features>,
    cache: Arc<Mutex<TerrainCache>>,
    ids: [BiomeId; 5],
}

impl End {
    pub(crate) fn new(seed: u64, palette: &Palette) -> Self {
        Self {
            seed,
            blended: BlendedNoise::new(seed as i64, END_BLEND),
            islands: EndIslands::new(seed as i64),
            pillars: Pillars::new(seed as i64),
            surface: Arc::new(SurfaceSystem::end(seed as i64, palette)),
            features: Arc::new(Features::compile(&END_BIOMES)),
            cache: Arc::new(Mutex::new(TerrainCache::with_capacity(CACHE_CAPACITY))),
            ids: END_BIOMES.map(biome_id),
        }
    }

    fn density(&self, x: i32, y: i32, z: i32, island: f32) -> f32 {
        let yf = y as f32;
        let cheese = island + self.blended.get(f64::from(x), f64::from(y), f64::from(z));
        let upper = lerp(clamped_map(yf, 56.0, 312.0, 1.0, 0.0), -23.4375, cheese);
        lerp(clamped_map(yf, 4.0, 32.0, 0.0, 1.0), -0.234375, upper) * FINAL_SCALE
    }

    #[cfg(test)]
    pub(crate) fn islands_erosion(&self, x: i32, z: i32) -> f32 {
        self.islands.erosion(x, z)
    }

    #[cfg(test)]
    pub(crate) fn raw_density(&self, x: i32, y: i32, z: i32) -> f32 {
        self.density(x, y, z, self.islands.erosion(x, z))
    }

    pub(crate) fn biome_index(&self, qx: i32, qz: i32) -> u8 {
        let (block_x, block_z) = (qx * 4, qz * 4);
        let (section_x, section_z) = (block_x >> 4, block_z >> 4);
        let central = i64::from(section_x) * i64::from(section_x)
            + i64::from(section_z) * i64::from(section_z);
        if central < CENTRAL_SECTIONS + 1 {
            return 0;
        }
        let value = self
            .islands
            .erosion((section_x * 2 + 1) * 8, (section_z * 2 + 1) * 8);
        if value > 0.25 {
            1
        } else if value >= -0.0625 {
            2
        } else if value < -0.218_75 {
            3
        } else {
            4
        }
    }

    fn terrain_cached(
        &self,
        resources: &Resources,
        dimension: &Dimension,
        pos: ChunkPos,
    ) -> TerrainChunk {
        let key = (
            pos.x,
            pos.z,
            dimension.height.min_y,
            dimension.height.height,
            dimension.sea_level,
        );
        if let Some(found) = self.cache.lock().map_or(None, |cache| cache.get(&key)) {
            return (*found).clone();
        }
        let chunk = Arc::new(self.terrain(resources, dimension, pos));
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(key, chunk.clone());
        }
        (*chunk).clone()
    }

    fn terrain(&self, resources: &Resources, dimension: &Dimension, pos: ChunkPos) -> TerrainChunk {
        let height = dimension.height;
        let (x0, z0) = (pos.x * 16, pos.z * 16);
        let end_stone = resources.blocks[BlockKind::EndStone];
        let mut buffer = ChunkBuffer::new(height);
        let table: Vec<f32> = (0..9)
            .map(|i| self.islands.erosion(x0 + (i % 3) * 8, z0 + (i / 3) * 8))
            .collect();
        let corners = Corners::build(
            |x, y, z| {
                let slot = ((z - z0) / 8 * 3 + (x - x0) / 8) as usize;
                self.density(x, y, z, table[slot])
            },
            END_LATTICE,
            x0,
            z0,
            height.min_y,
            (height.height as i32).min(NOISE_HEIGHT),
        );
        corners.fill(|x, dy, z, value| {
            if value > 0.0 {
                buffer.set(x, height.min_y + dy as i32, z, end_stone);
            }
        });
        self.pillars.place(&mut buffer, pos);
        let top = height.min_y + height.height as i32;
        TerrainChunk::new(buffer, ChunkBiomes::new(height), [top; 256], height)
    }

    pub(crate) fn generate(
        &self,
        resources: &Resources,
        dimension: &Dimension,
        pos: ChunkPos,
    ) -> ChunkColumn {
        let mut region = Region::for_end(self, resources, dimension, pos);
        self.features
            .decorate(&mut region, self.seed as i64, &self.surface);
        let (buffer, indices) = region.finish();
        let stored: Vec<BiomeId> = indices
            .iter()
            .map(|index| self.ids[usize::from(*index)])
            .collect();
        buffer.finish_layered(pos, &stored, &resources.classifier)
    }

    pub(crate) fn terrain_for_region(
        &self,
        resources: &Resources,
        dimension: &Dimension,
        pos: ChunkPos,
    ) -> TerrainChunk {
        self.terrain_cached(resources, dimension, pos)
    }

    pub(crate) fn pillars(&self) -> &Pillars {
        &self.pillars
    }

    pub(crate) fn spawn_point(&self, resources: &Resources, dimension: &Dimension) -> Vec3 {
        let column = self.generate(resources, dimension, ChunkPos::new(0, 0));
        let mut y = dimension.height.max_y();
        while y > dimension.height.min_y {
            if column
                .block(BlockPos::new(0, y, 0))
                .is_some_and(|state| !state.is_air())
            {
                return Vec3::new(0.5, f64::from(y + 1), 0.5);
            }
            y -= 1;
        }
        Vec3::new(0.5, f64::from(dimension.sea_level + 1), 0.5)
    }
}
