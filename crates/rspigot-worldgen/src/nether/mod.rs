use std::sync::{Arc, Mutex};

use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Vec3};

mod biomes;
pub(crate) mod density;
#[cfg(test)]
mod reference;

pub(crate) use biomes::NETHER_BIOMES;

use crate::buffer::ChunkBuffer;
use crate::overworld::{
    Bounds, ChunkBiomes, ColumnInput, Features, Region, SurfaceSystem, TerrainCache, TerrainChunk,
};
use crate::resources::{BlockKind, Palette, Resources};
use crate::structures::{BiomeProbe, Engine, Ground, Heightmap, Library, Start};

const CEILING: i32 = 127;
const NOISE_HEIGHT: i32 = 128;
const SPAWN_RINGS: i32 = 6;
const CACHE_CAPACITY: usize = 64;

#[derive(Debug, Clone)]
pub(crate) struct Nether {
    seed: u64,
    density: density::NetherDensity,
    biomes: biomes::NetherBiomes,
    surface: Arc<SurfaceSystem>,
    features: Arc<Features>,
    cache: Arc<Mutex<TerrainCache>>,
    structures: Arc<Engine>,
    structures_on: bool,
}

pub(crate) struct NetherProbe<'a> {
    biomes: &'a biomes::NetherBiomes,
}

impl BiomeProbe for NetherProbe<'_> {
    fn at(&self, qx: i32, _qy: i32, qz: i32) -> u8 {
        self.biomes.index(qx, qz) as u8
    }

    fn id(&self, name: &str) -> Option<u8> {
        NETHER_BIOMES
            .iter()
            .position(|candidate| candidate.strip_prefix("minecraft:") == Some(name))
            .map(|index| index as u8)
    }
}

pub(crate) struct NetherGround<'a> {
    nether: &'a Nether,
    min_y: i32,
    max_y: i32,
    sea_level: i32,
    columns: std::cell::RefCell<std::collections::HashMap<(i32, i32), Vec<bool>>>,
}

impl Ground for NetherGround<'_> {
    fn first_free_height(&self, _x: i32, _z: i32, _heightmap: Heightmap) -> i32 {
        self.sea_level
    }

    fn min_y(&self) -> i32 {
        self.min_y
    }

    fn max_y(&self) -> i32 {
        self.max_y
    }

    fn sea_level(&self) -> i32 {
        self.sea_level
    }

    fn gen_depth(&self) -> i32 {
        NOISE_HEIGHT
    }

    fn base(&self, x: i32, y: i32, z: i32) -> crate::structures::Base {
        use crate::structures::Base;
        let key = (x >> 4, z >> 4);
        let mut columns = self.columns.borrow_mut();
        let solid = columns.entry(key).or_insert_with(|| {
            let height = (self.max_y - self.min_y + 1).min(NOISE_HEIGHT);
            let corners = density::Corners::build(
                |sx, sy, sz| self.nether.density.pre_interpolation(sx, sy, sz),
                density::NETHER_LATTICE,
                key.0 * 16,
                key.1 * 16,
                self.min_y,
                height,
            );
            let mut cells = vec![false; (16 * 16 * height) as usize];
            corners.fill(|cx, dy, cz, value| {
                if value > 0.0 {
                    cells[(dy * 16 + cz) * 16 + cx] = true;
                }
            });
            cells
        });
        let dy = y - self.min_y;
        if dy < 0 {
            return Base::Air;
        }
        let index = (dy as usize * 16 + (z & 15) as usize) * 16 + (x & 15) as usize;
        if solid.get(index).copied().unwrap_or(false) {
            Base::Solid
        } else if y < self.sea_level {
            Base::Lava
        } else {
            Base::Air
        }
    }
}

impl Nether {
    pub(crate) fn new(seed: u64, palette: &Palette) -> Self {
        Self::with_library(seed, palette, Library::current())
    }

    pub(crate) fn with_library(seed: u64, palette: &Palette, library: &'static Library) -> Self {
        let biomes = biomes::NetherBiomes::new(seed as i64);
        let structures = Arc::new(Engine::new(
            seed as i64,
            &NetherProbe { biomes: &biomes },
            library,
        ));
        Self {
            structures,
            structures_on: true,
            seed,
            density: density::NetherDensity::new(seed as i64),
            biomes,
            surface: Arc::new(SurfaceSystem::nether(seed as i64, palette)),
            features: Arc::new(Features::compile(&NETHER_BIOMES)),
            cache: Arc::new(Mutex::new(TerrainCache::with_capacity(CACHE_CAPACITY))),
        }
    }

    pub(crate) fn ground(&self, dimension: &Dimension) -> NetherGround<'_> {
        NetherGround {
            nether: self,
            min_y: dimension.height.min_y,
            max_y: dimension.height.max_y(),
            sea_level: dimension.sea_level,
            columns: std::cell::RefCell::default(),
        }
    }

    pub(crate) fn biome_probe(&self) -> NetherProbe<'_> {
        NetherProbe {
            biomes: &self.biomes,
        }
    }

    pub(crate) fn disable_structures(&mut self) {
        self.structures_on = false;
    }

    pub(crate) fn starts_near(&self, dimension: &Dimension, pos: ChunkPos) -> Vec<Arc<Start>> {
        if !self.structures_on {
            return Vec::new();
        }
        let ground = self.ground(dimension);
        let probe = self.biome_probe();
        self.structures.starts_near(&ground, &probe, (pos.x, pos.z))
    }

    pub(crate) fn structure_entities(
        &self,
        dimension: &Dimension,
        pos: ChunkPos,
    ) -> Vec<rspigot_nbt::Compound> {
        crate::structures::chunk_entities(&self.starts_near(dimension, pos), (pos.x, pos.z))
    }

    pub(crate) fn biome_index(&self, qx: i32, qz: i32) -> u8 {
        self.biomes.index(qx, qz) as u8
    }

    pub(crate) fn terrain_cached(
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
        let palette = &resources.blocks;
        let height = dimension.height;
        let bottom = height.min_y;
        let x0 = pos.x * 16;
        let z0 = pos.z * 16;
        let netherrack = palette[BlockKind::Netherrack];
        let lava = palette[BlockKind::Lava];
        let mut buffer = ChunkBuffer::new(height);
        let corners = density::Corners::build(
            |x, y, z| self.density.pre_interpolation(x, y, z),
            density::NETHER_LATTICE,
            x0,
            z0,
            bottom,
            (height.height as i32).min(NOISE_HEIGHT),
        );
        corners.fill(|x, dy, z, value| {
            let y = bottom + dy as i32;
            if value > 0.0 {
                buffer.set(x, y, z, netherrack);
            } else if y < dimension.sea_level {
                buffer.set(x, y, z, lava);
            }
        });
        let biomes = ChunkBiomes::new(height);
        let bounds = Bounds {
            min_y: bottom,
            max_y: CEILING.min(height.max_y()),
            sea_level: dimension.sea_level,
        };
        for i in 0..256usize {
            let (x, z) = (i & 15, i >> 4);
            let index = self.biome_index(pos.x * 4 + (x >> 2) as i32, pos.z * 4 + (z >> 2) as i32);
            self.surface.column(ColumnInput {
                bounds,
                buffer: &mut buffer,
                x,
                z,
                world_x: x0 + x as i32,
                world_z: z0 + z as i32,
                top: bounds.max_y,
                gradient: (0, 0),
                preliminary: None,
                vein: None,
                biome_at: |_| index,
            });
        }
        TerrainChunk::new(buffer, biomes, [bounds.max_y + 1; 256], height)
    }

    pub(crate) fn generate(
        &self,
        resources: &Resources,
        dimension: &Dimension,
        pos: ChunkPos,
    ) -> ChunkColumn {
        let mut region = Region::for_nether(self, resources, dimension, pos);
        let starts = self.starts_near(dimension, pos);
        self.features.decorate(
            &mut region,
            self.seed as i64,
            &self.surface,
            Some((&self.structures, &starts)),
        );
        let block_entities = std::mem::take(&mut region.block_entities);
        let (buffer, indices) = region.finish();
        let ids: Vec<BiomeId> = self.biomes.ids().to_vec();
        let stored: Vec<BiomeId> = indices
            .iter()
            .map(|index| ids[usize::from(*index)])
            .collect();
        let mut column = buffer.finish_layered(pos, &stored, &resources.classifier);
        crate::structures::attach(&mut column, &block_entities, &starts, pos);
        column
    }

    pub(crate) fn spawn_point(&self, resources: &Resources, dimension: &Dimension) -> Vec3 {
        let palette = &resources.blocks;
        let classifier = &resources.classifier;
        let top = CEILING.min(dimension.height.max_y());
        for ring in 0..=SPAWN_RINGS {
            for (cx, cz) in ring_chunks(ring) {
                let column = self.generate(resources, dimension, ChunkPos::new(cx, cz));
                let mut best: Option<(i64, Vec3)> = None;
                for z in 0..16i32 {
                    for x in 0..16i32 {
                        let (wx, wz) = (cx * 16 + x, cz * 16 + z);
                        let distance =
                            i64::from(wx) * i64::from(wx) + i64::from(wz) * i64::from(wz);
                        if best.is_some_and(|(d, _)| d <= distance) {
                            continue;
                        }
                        for y in dimension.sea_level..(top - 3) {
                            let floor = column.block(BlockPos::new(wx, y - 1, wz));
                            let feet = column.block(BlockPos::new(wx, y, wz));
                            let head = column.block(BlockPos::new(wx, y + 1, wz));
                            let solid = floor.is_some_and(|s| {
                                classifier.blocks_motion(s) && s != palette[BlockKind::MagmaBlock]
                            });
                            let clear =
                                |s: Option<BlockStateId>| s.is_some_and(BlockStateId::is_air);
                            if solid && clear(feet) && clear(head) {
                                best = Some((
                                    distance,
                                    Vec3::new(
                                        f64::from(wx) + 0.5,
                                        f64::from(y),
                                        f64::from(wz) + 0.5,
                                    ),
                                ));
                                break;
                            }
                        }
                    }
                }
                if let Some((_, spot)) = best {
                    return spot;
                }
            }
        }
        Vec3::new(0.5, f64::from(dimension.sea_level + 1), 0.5)
    }
}

fn ring_chunks(ring: i32) -> Vec<(i32, i32)> {
    let mut chunks = Vec::new();
    for z in -ring..=ring {
        for x in -ring..=ring {
            if x.abs().max(z.abs()) == ring {
                chunks.push((x, z));
            }
        }
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_chunks_enumerate_the_perimeter() {
        assert_eq!(ring_chunks(0), vec![(0, 0)]);
        assert_eq!(ring_chunks(2).len(), 16);
    }
}
