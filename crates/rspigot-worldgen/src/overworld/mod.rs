mod caves;
mod features;
mod freeze;
mod region;
mod surface_rules;
mod terrain;

use rspigot_domain::chunk::{ChunkColumn, HeightmapKind};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Vec3};

use crate::buffer::ChunkBuffer;
use crate::climate::{BiomeSource, ClimateScratch};
use crate::resources::{BlockKind, Palette, Resources, biome_id};
use crate::rng::{hash3, unit};
use rspigot_data::multi_noise::{ClimateVariant, overworld_biomes};
use std::cell::RefCell;

use caves::Caves;
use features::Features;
use freeze::{FreezeContext, freeze_top_layer};
use region::{ChunkBiomes, Region, TerrainChunk};
use surface_rules::{Bounds, ColumnInput, SurfaceSystem};
use terrain::Terrain;
pub(crate) use terrain::smooth;

const SALT_BEDROCK: u64 = 0xBED0_0C41;
const SALT_DEEPSLATE: u64 = 0xDEE9_51A7;
const SPAWN_STEP: i32 = 8;
const SPAWN_RINGS: i32 = 200;

const CACHE_CAPACITY: usize = 192;

type TerrainKey = (i32, i32, i32, u32, i32);

#[derive(Debug, Default)]
struct TerrainCache {
    map: HashMap<TerrainKey, Arc<TerrainChunk>>,
    order: VecDeque<TerrainKey>,
}

impl TerrainCache {
    fn get(&self, key: &TerrainKey) -> Option<Arc<TerrainChunk>> {
        self.map.get(key).cloned()
    }

    fn insert(&mut self, key: TerrainKey, chunk: Arc<TerrainChunk>) {
        if self.map.insert(key, chunk).is_none() {
            self.order.push_back(key);
            while self.order.len() > CACHE_CAPACITY {
                if let Some(old) = self.order.pop_front() {
                    self.map.remove(&old);
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Overworld {
    seed: u64,
    terrain: Terrain,
    source: Arc<BiomeSource>,
    placed: Arc<[PlacedBiome]>,
    caves: Caves,
    surface: Arc<SurfaceSystem>,
    features: Arc<Features>,
    cache: Arc<Mutex<TerrainCache>>,
}

#[derive(Debug, Clone, Copy)]
struct PlacedBiome {
    id: BiomeId,
}

fn rock(palette: &Palette, seed: u64, x: i32, y: i32, z: i32) -> BlockStateId {
    let deep =
        y < 0 || (y < 8 && unit(hash3(seed ^ SALT_DEEPSLATE, x, y, z)) < f64::from(8 - y) / 8.0);
    if deep {
        palette[BlockKind::Deepslate]
    } else {
        palette[BlockKind::Stone]
    }
}

impl Overworld {
    pub(crate) fn new(seed: u64, variant: ClimateVariant, palette: &Palette) -> Self {
        Self {
            seed,
            terrain: Terrain::new(seed),
            source: Arc::new(BiomeSource::new(seed as i64, variant)),
            placed: overworld_biomes()
                .iter()
                .map(|name| PlacedBiome { id: biome_id(name) })
                .collect(),
            caves: Caves::new(seed),
            surface: Arc::new(SurfaceSystem::new(seed as i64, palette)),
            features: Arc::new(Features::compile()),
            cache: Arc::new(Mutex::new(TerrainCache::default())),
        }
    }

    fn clamp_height(dimension: &Dimension, height: f64) -> i32 {
        let bounds = dimension.height;
        (height.floor() as i32)
            .min(bounds.max_y() - 24)
            .max(bounds.min_y + 8)
    }

    fn column_height(&self, dimension: &Dimension, x: i32, z: i32) -> i32 {
        let mut scratch = self.source.scratch();
        let corners = self
            .terrain
            .heights(&self.source, &mut scratch, x & !15, z & !15);
        let base = corners.at((x & 15) as usize, (z & 15) as usize);
        let shifted = base + self.terrain.detail(x, z) + f64::from(dimension.sea_level - 63);
        Self::clamp_height(dimension, shifted)
    }

    fn tops(
        &self,
        dimension: &Dimension,
        pos: ChunkPos,
        scratch: &mut ClimateScratch,
    ) -> [i32; 256] {
        let (x0, z0) = (pos.x * 16, pos.z * 16);
        let corners = self.terrain.heights(&self.source, scratch, x0, z0);
        std::array::from_fn(|i| {
            let (x, z) = ((i & 15) as i32, (i >> 4) as i32);
            let base = corners.at(x as usize, z as usize);
            let shifted =
                base + self.terrain.detail(x0 + x, z0 + z) + f64::from(dimension.sea_level - 63);
            Self::clamp_height(dimension, shifted)
        })
    }

    pub(crate) fn terrain_cached(
        &self,
        resources: &Resources,
        dimension: &Dimension,
        pos: ChunkPos,
        scratch: &mut ClimateScratch,
    ) -> Arc<TerrainChunk> {
        let key = (
            pos.x,
            pos.z,
            dimension.height.min_y,
            dimension.height.height,
            dimension.sea_level,
        );
        if let Some(found) = self.cache.lock().map_or(None, |cache| cache.get(&key)) {
            return found;
        }
        let mut biomes = ChunkBiomes::new(dimension.height);
        let buffer = self.terrain(resources, dimension, pos, &mut biomes, scratch);
        let tops = self.tops(dimension, pos, scratch);
        let hint: [i32; 256] = std::array::from_fn(|i| tops[i].max(dimension.sea_level - 1) + 1);
        let chunk = Arc::new(TerrainChunk::new(buffer, biomes, hint, dimension.height));
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(key, chunk.clone());
        }
        chunk
    }

    pub(crate) fn terrain(
        &self,
        resources: &Resources,
        dimension: &Dimension,
        pos: ChunkPos,
        biomes: &mut ChunkBiomes,
        scratch: &mut ClimateScratch,
    ) -> ChunkBuffer {
        let palette = &resources.blocks;
        let height = dimension.height;
        let sea_level = dimension.sea_level;
        let min_y = height.min_y;
        let (x0, z0) = (pos.x * 16, pos.z * 16);
        let tops = self.tops(dimension, pos, scratch);
        let mut buffer = ChunkBuffer::new(height);
        let lowest = tops.iter().copied().min().unwrap_or(min_y);
        let uniform_top = (lowest - 10).max(min_y - 1);
        for y in min_y..=uniform_top {
            if y < 0 {
                buffer.fill_layer(y, palette[BlockKind::Deepslate]);
            } else if y >= 8 {
                buffer.fill_layer(y, palette[BlockKind::Stone]);
            } else {
                for i in 0..256usize {
                    let state = rock(
                        palette,
                        self.seed,
                        x0 + (i & 15) as i32,
                        y,
                        z0 + (i >> 4) as i32,
                    );
                    buffer.set(i & 15, y, i >> 4, state);
                }
            }
        }
        let water = palette[BlockKind::Water];
        let mut limits = [0i32; 256];
        for i in 0..256usize {
            let (x, z) = (i & 15, i >> 4);
            let (wx, wz) = (x0 + x as i32, z0 + z as i32);
            let top = tops[i];
            for y in (uniform_top + 1)..=top {
                buffer.set(x, y, z, rock(palette, self.seed, wx, y, wz));
            }
            for y in (top + 1)..sea_level {
                buffer.set(x, y, z, water);
            }
            limits[i] = self.caves.limit(wx, wz, top, sea_level);
        }
        self.caves.carve(
            &mut buffer,
            pos,
            &limits,
            min_y + 10,
            BlockStateId::AIR,
            palette[BlockKind::Lava],
        );
        let bounds = Bounds {
            min_y,
            max_y: height.max_y(),
            sea_level,
        };
        let surface_height = |i: usize| tops[i].max(sea_level - 1) + 1;
        let cells = RefCell::new((biomes, scratch));
        for (i, top) in tops.iter().copied().enumerate() {
            let (x, z) = (i & 15, i >> 4);
            let gradient = (
                surface_height((z << 4) | (x + 1).min(15))
                    - surface_height((z << 4) | x.saturating_sub(1)),
                surface_height(((z + 1).min(15) << 4) | x)
                    - surface_height((z.saturating_sub(1) << 4) | x),
            );
            self.surface.column(ColumnInput {
                bounds,
                buffer: &mut buffer,
                x,
                z,
                world_x: x0 + x as i32,
                world_z: z0 + z as i32,
                top,
                gradient,
                biome_at: |y| {
                    let mut guard = cells.borrow_mut();
                    let (biomes, scratch) = &mut *guard;
                    biomes.get(&self.source, scratch, pos, x, y, z)
                },
            });
        }
        self.bedrock(&mut buffer, palette, pos);
        buffer
    }

    pub(crate) fn generate(
        &self,
        resources: &Resources,
        dimension: &Dimension,
        pos: ChunkPos,
    ) -> ChunkColumn {
        let palette = &resources.blocks;
        let height = dimension.height;
        let sea_level = dimension.sea_level;
        let min_y = height.min_y;
        let mut region = Region::new(self, resources, dimension, pos);
        self.features
            .decorate(&mut region, self.seed as i64, &self.surface);
        let (mut buffer, indices) = region.finish();
        let biome_index = |x: usize, y: i32, z: usize| {
            let layer = ((y.clamp(min_y, height.max_y()) - min_y) >> 2) as usize;
            indices[layer * 16 + ((z >> 2) << 2) + (x >> 2)]
        };
        let mut scratch = self.source.scratch();
        let tops = self.tops(dimension, pos, &mut scratch);
        let scan_from = tops.iter().copied().max().unwrap_or(min_y).max(sea_level) + 40;
        freeze_top_layer(
            &mut buffer,
            &FreezeContext {
                system: &self.surface,
                classifier: &resources.classifier,
                palette,
                sea_level,
                scan_from,
            },
            pos,
            biome_index,
        );
        let stored: Vec<BiomeId> = indices
            .iter()
            .map(|index| self.placed[usize::from(*index)].id)
            .collect();
        buffer.finish_layered(pos, &stored, &resources.classifier)
    }

    fn bedrock(&self, buffer: &mut ChunkBuffer, palette: &Palette, pos: ChunkPos) {
        let min_y = buffer.height().min_y;
        let bedrock = palette[BlockKind::Bedrock];
        for k in 0..5 {
            let y = min_y + k;
            let chance = f64::from(5 - k) / 5.0;
            for z in 0..16usize {
                for x in 0..16usize {
                    let hash = hash3(
                        self.seed ^ SALT_BEDROCK,
                        pos.x * 16 + x as i32,
                        y,
                        pos.z * 16 + z as i32,
                    );
                    if k == 0 || unit(hash) < chance {
                        buffer.set(x, y, z, bedrock);
                    }
                }
            }
        }
    }

    pub(crate) fn spawn_point(&self, resources: &Resources, dimension: &Dimension) -> Vec3 {
        let sea_level = dimension.sea_level;
        for ring in 0..=SPAWN_RINGS {
            for (dx, dz) in ring_offsets(ring) {
                let (x, z) = (dx * SPAWN_STEP, dz * SPAWN_STEP);
                let top = self.column_height(dimension, x, z);
                let dry = top >= sea_level && top <= sea_level + 35;
                if dry && let Some(spawn) = self.standing_spot(resources, dimension, x, z) {
                    return spawn;
                }
            }
        }
        Vec3::new(0.5, f64::from(sea_level + 1), 0.5)
    }

    fn standing_spot(
        &self,
        resources: &Resources,
        dimension: &Dimension,
        x: i32,
        z: i32,
    ) -> Option<Vec3> {
        let palette = &resources.blocks;
        let column = self.generate(resources, dimension, BlockPos::new(x, 0, z).chunk());
        let top = i32::from(
            column
                .heightmaps
                .get(HeightmapKind::MotionBlocking)?
                .get((x & 15) as usize, (z & 15) as usize),
        );
        if top == 0 {
            return None;
        }
        let mut ground_y = dimension.height.min_y + top - 1;
        let mut block = column.block(BlockPos::new(x, ground_y, z))?;
        if block == palette[BlockKind::SnowLayer] {
            ground_y -= 1;
            block = column.block(BlockPos::new(x, ground_y, z))?;
        }
        let natural = [
            BlockKind::GrassBlock,
            BlockKind::SnowyGrassBlock,
            BlockKind::Sand,
            BlockKind::Sandstone,
            BlockKind::Stone,
            BlockKind::Dirt,
            BlockKind::Gravel,
            BlockKind::SnowBlock,
            BlockKind::Clay,
        ];
        natural.iter().any(|b| palette[*b] == block).then(|| {
            Vec3::new(
                f64::from(x) + 0.5,
                f64::from(ground_y + 1),
                f64::from(z) + 0.5,
            )
        })
    }
}

fn ring_offsets(ring: i32) -> Vec<(i32, i32)> {
    if ring == 0 {
        return vec![(0, 0)];
    }
    let mut offsets = Vec::with_capacity(8 * ring as usize);
    for i in -ring..=ring {
        offsets.push((i, -ring));
        offsets.push((i, ring));
    }
    for i in (-ring + 1)..ring {
        offsets.push((-ring, i));
        offsets.push((ring, i));
    }
    offsets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rings_cover_each_cell_once() {
        assert_eq!(ring_offsets(0), vec![(0, 0)]);
        for ring in 1..6 {
            let offsets = ring_offsets(ring);
            assert_eq!(offsets.len(), 8 * ring as usize);
            let mut sorted = offsets.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), offsets.len());
            assert!(offsets.iter().all(|(x, z)| x.abs().max(z.abs()) == ring));
        }
    }

    #[test]
    fn rock_transitions_to_deepslate_between_zero_and_eight() {
        let palette = Palette::resolve();
        assert_eq!(rock(&palette, 1, 0, 20, 0), palette[BlockKind::Stone]);
        assert_eq!(rock(&palette, 1, 0, -1, 0), palette[BlockKind::Deepslate]);
        let mixed = (0..400)
            .filter(|i| rock(&palette, 1, *i, 4, *i * 3) == palette[BlockKind::Deepslate])
            .count();
        assert!(mixed > 120 && mixed < 280, "{mixed}");
    }
}
