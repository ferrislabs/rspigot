pub(crate) mod features;
mod freeze;
pub(crate) mod region;
mod surface_random;
mod surface_rules;

use rspigot_domain::chunk::{ChunkColumn, HeightmapKind};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, Vec3};

use crate::buffer::ChunkBuffer;
use crate::climate::{BiomeSource, ClimateScratch};
use crate::resources::{BlockKind, Palette, Resources, biome_id};
use crate::structures::{Engine, Library};
use crate::terrain::{Cell, Terrain};
use rspigot_data::multi_noise::{ClimateVariant, overworld_biomes};
use std::cell::RefCell;

pub(crate) use features::Features;
use freeze::{FreezeContext, freeze_top_layer};
pub(crate) use region::{ChunkBiomes, Region, TerrainChunk};
pub(crate) use surface_rules::{Bounds, ColumnInput, SurfaceSystem};

const SPAWN_STEP: i32 = 8;
const SPAWN_RINGS: i32 = 200;

const CACHE_CAPACITY: usize = 192;

pub(crate) type TerrainKey = (i32, i32, i32, u32, i32);

#[derive(Debug)]
pub(crate) struct TerrainCache {
    map: HashMap<TerrainKey, Arc<TerrainChunk>>,
    order: VecDeque<TerrainKey>,
    capacity: usize,
}

impl TerrainCache {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub(crate) fn get(&self, key: &TerrainKey) -> Option<Arc<TerrainChunk>> {
        self.map.get(key).cloned()
    }

    pub(crate) fn insert(&mut self, key: TerrainKey, chunk: Arc<TerrainChunk>) {
        if self.map.insert(key, chunk).is_none() {
            self.order.push_back(key);
            while self.order.len() > self.capacity.max(1) {
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
    terrain: Arc<Terrain>,
    source: Arc<BiomeSource>,
    placed: Arc<[PlacedBiome]>,
    surface: Arc<SurfaceSystem>,
    features: Arc<Features>,
    cache: Arc<Mutex<TerrainCache>>,
    heights: Arc<Mutex<HeightCache>>,
    structures: Arc<Engine>,
}

pub(crate) type ColumnHeights = [(i32, i32); 256];

#[derive(Debug, Default)]
pub(crate) struct HeightCache {
    map: HashMap<(i32, i32), Arc<ColumnHeights>>,
    order: VecDeque<(i32, i32)>,
}

#[derive(Debug, Clone, Copy)]
struct PlacedBiome {
    id: BiomeId,
}

impl Overworld {
    pub(crate) fn new(seed: u64, variant: ClimateVariant, palette: &Palette) -> Self {
        Self::with_library(seed, variant, palette, Library::current())
    }

    pub(crate) fn with_library(
        seed: u64,
        variant: ClimateVariant,
        palette: &Palette,
        library: &'static Library,
    ) -> Self {
        let source = Arc::new(BiomeSource::new(seed as i64, variant));
        let probe = OverworldProbe {
            source: &source,
            scratch: RefCell::new(source.scratch()),
        };
        Self {
            seed,
            terrain: Arc::new(Terrain::new(seed as i64, variant)),
            source: source.clone(),
            placed: overworld_biomes()
                .iter()
                .map(|name| PlacedBiome { id: biome_id(name) })
                .collect(),
            surface: Arc::new(SurfaceSystem::new(seed as i64, palette)),
            features: Arc::new(Features::compile(overworld_biomes())),
            cache: Arc::new(Mutex::new(TerrainCache::with_capacity(CACHE_CAPACITY))),
            heights: Arc::new(Mutex::new(HeightCache::default())),
            structures: Arc::new(Engine::new(seed as i64, &probe, library)),
        }
    }

    pub(crate) fn column_heights(&self, cx: i32, cz: i32) -> Arc<ColumnHeights> {
        if let Some(found) = self
            .heights
            .lock()
            .ok()
            .and_then(|cache| cache.map.get(&(cx, cz)).cloned())
        {
            return found;
        }
        let min_y = self.terrain.min_y();
        let mut sampler = self.terrain.sampler();
        let cells = self.terrain.fill(&mut sampler, cx * 16, cz * 16);
        let mut heights = [(min_y, min_y); 256];
        for (index, cell) in cells.iter().enumerate() {
            let (slot, y) = (index & 255, min_y + (index >> 8) as i32);
            match cell {
                Cell::Air => {}
                Cell::Default => heights[slot] = (y + 1, y + 1),
                Cell::Water | Cell::Lava => heights[slot].0 = y + 1,
            }
        }
        let heights = Arc::new(heights);
        if let Ok(mut cache) = self.heights.lock()
            && cache.map.insert((cx, cz), heights.clone()).is_none()
        {
            cache.order.push_back((cx, cz));
            while cache.order.len() > 256 {
                if let Some(old) = cache.order.pop_front() {
                    cache.map.remove(&old);
                }
            }
        }
        heights
    }

    pub(crate) fn biome_probe(&self) -> OverworldProbe<'_> {
        OverworldProbe {
            source: &self.source,
            scratch: RefCell::new(self.source.scratch()),
        }
    }

    fn column_height(&self, dimension: &Dimension, x: i32, z: i32) -> i32 {
        let cells = self.terrain.column(x, z);
        cells
            .iter()
            .rposition(|cell| *cell == Cell::Default)
            .map_or(dimension.height.min_y, |index| {
                self.terrain.min_y() + index as i32
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
        let hint: [i32; 256] = [dimension.height.max_y() + 1; 256];
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
        let terrain = &*self.terrain;
        let mut sampler = terrain.sampler();
        let cells = terrain.fill(&mut sampler, x0, z0);
        let levels = terrain.chunk_surface_levels(&mut sampler, x0, z0);
        let mut buffer = ChunkBuffer::new(height);
        let mut tops = [min_y - 1; 256];
        let (stone, water, lava) = (
            palette[BlockKind::Stone],
            palette[BlockKind::Water],
            palette[BlockKind::Lava],
        );
        for (index, cell) in cells.iter().enumerate() {
            let (i, yy) = (index & 255, (index >> 8) as i32);
            let y = terrain.min_y() + yy;
            let state = match cell {
                Cell::Air => continue,
                Cell::Default => stone,
                Cell::Water => water,
                Cell::Lava => lava,
            };
            if y < min_y || y > height.max_y() {
                continue;
            }
            buffer.set(i & 15, y, i >> 4, state);
            tops[i] = tops[i].max(y);
        }
        let sampler = RefCell::new(sampler);
        let vein = |x: i32, y: i32, z: i32| terrain.vein(&mut sampler.borrow_mut(), x, y, z);
        let bounds = Bounds {
            min_y,
            max_y: height.max_y(),
            sea_level,
        };
        let surface_height = |i: usize| tops[i] + 1;
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
                preliminary: Some(levels[i].floor() as i32),
                vein: Some(&vein),
                biome_at: |y| {
                    let mut guard = cells.borrow_mut();
                    let (biomes, scratch) = &mut *guard;
                    biomes.get(&self.source, scratch, pos, x, y, z)
                },
            });
        }
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
        let probe = self.biome_probe();
        let starts = self
            .structures
            .starts_near(&self.ground(dimension), &probe, (pos.x, pos.z));
        self.features.decorate(
            &mut region,
            self.seed as i64,
            &self.surface,
            Some((&self.structures, &starts)),
        );
        let block_entities = std::mem::take(&mut region.block_entities);
        let (mut buffer, indices) = region.finish();
        let biome_index = |x: usize, y: i32, z: usize| {
            let layer = ((y.clamp(min_y, height.max_y()) - min_y) >> 2) as usize;
            indices[layer * 16 + ((z >> 2) << 2) + (x >> 2)]
        };
        let scan_from = height.max_y();
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
        let mut column = buffer.finish_layered(pos, &stored, &resources.classifier);
        for (at, nbt) in &block_entities {
            if BlockPos::new(at.0, at.1, at.2).chunk() == pos
                && let Some(entity) = crate::structures::block_entity(*at, nbt)
            {
                column.set_block_entity(entity);
            }
        }
        let own: Vec<_> = starts
            .iter()
            .filter(|start| start.chunk == (pos.x, pos.z))
            .cloned()
            .collect();
        if !own.is_empty() || !starts.is_empty() {
            column.extra.insert(
                "structures",
                crate::structures::structures_tag(&own, &starts),
            );
        }
        column
    }

    pub(crate) fn structure_entities(
        &self,
        dimension: &Dimension,
        pos: ChunkPos,
    ) -> Vec<rspigot_nbt::Compound> {
        let probe = self.biome_probe();
        let starts = self
            .structures
            .starts_near(&self.ground(dimension), &probe, (pos.x, pos.z));
        crate::structures::chunk_entities(&starts, (pos.x, pos.z))
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

pub(crate) struct OverworldProbe<'a> {
    source: &'a BiomeSource,
    scratch: RefCell<ClimateScratch>,
}

impl crate::structures::BiomeProbe for OverworldProbe<'_> {
    fn at(&self, qx: i32, qy: i32, qz: i32) -> u8 {
        self.source
            .biome_index(&mut self.scratch.borrow_mut(), qx, qy, qz)
    }

    fn id(&self, name: &str) -> Option<u8> {
        overworld_biomes()
            .iter()
            .position(|candidate| candidate.strip_prefix("minecraft:") == Some(name))
            .map(|index| index as u8)
    }
}

pub(crate) struct OverworldGround<'a> {
    overworld: &'a Overworld,
    min_y: i32,
    max_y: i32,
}

impl crate::structures::Ground for OverworldGround<'_> {
    fn first_free_height(&self, x: i32, z: i32, heightmap: crate::structures::Heightmap) -> i32 {
        let heights = self.overworld.column_heights(x >> 4, z >> 4);
        let (surface, floor) = heights[(((z & 15) << 4) | (x & 15)) as usize];
        match heightmap {
            crate::structures::Heightmap::WorldSurfaceWg => surface,
            crate::structures::Heightmap::OceanFloorWg => floor,
        }
    }

    fn min_y(&self) -> i32 {
        self.min_y
    }

    fn max_y(&self) -> i32 {
        self.max_y
    }
}

impl Overworld {
    pub(crate) fn ground(&self, dimension: &Dimension) -> OverworldGround<'_> {
        OverworldGround {
            overworld: self,
            min_y: dimension.height.min_y,
            max_y: dimension.height.max_y(),
        }
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
    fn chunk_has_bedrock_floor_and_deepslate_below_zero() {
        let resources = Resources::resolve();
        let palette = &resources.blocks;
        let dimension = Dimension::overworld();
        let overworld = Overworld::new(77, ClimateVariant::Normal, palette);
        let mut scratch = overworld.source.scratch();
        let chunk =
            overworld.terrain_cached(&resources, &dimension, ChunkPos::new(3, -5), &mut scratch);
        let mut deepslate = 0;
        let mut stone_deep = 0;
        for z in 0..16 {
            for x in 0..16 {
                assert_eq!(chunk.buffer.get(x, -64, z), palette[BlockKind::Bedrock]);
                for y in -50..-1 {
                    let state = chunk.buffer.get(x, y, z);
                    if state == palette[BlockKind::Deepslate] {
                        deepslate += 1;
                    } else if state == palette[BlockKind::Stone] {
                        stone_deep += 1;
                    }
                }
                assert_ne!(chunk.buffer.get(x, 40, z), palette[BlockKind::Deepslate]);
            }
        }
        assert!(deepslate > 5000, "{deepslate}");
        assert_eq!(stone_deep, 0);
    }

    #[test]
    fn bedrock_floor_matches_the_vanilla_material_rule() {
        let resources = Resources::resolve();
        let palette = &resources.blocks;
        let dimension = Dimension::overworld();
        let overworld = Overworld::new(5, ClimateVariant::Normal, palette);
        let mut scratch = overworld.source.scratch();
        for line in include_str!("bedrock_columns.txt").lines() {
            let mut parts = line.split(' ');
            let _seed = parts.next();
            let x: i32 = parts.next().unwrap().parse().unwrap();
            let z: i32 = parts.next().unwrap().parse().unwrap();
            let expected = parts.next().unwrap();
            let chunk = overworld.terrain_cached(
                &resources,
                &dimension,
                BlockPos::new(x, 0, z).chunk(),
                &mut scratch,
            );
            let wanted: Vec<bool> = expected.chars().map(|c| c == 'b').collect();
            let got: Vec<bool> = (0..wanted.len())
                .map(|i| {
                    chunk
                        .buffer
                        .get((x & 15) as usize, -64 + i as i32, (z & 15) as usize)
                        == palette[BlockKind::Bedrock]
                })
                .collect();
            assert_eq!(got, wanted, "{x} {z}");
        }
    }
}
