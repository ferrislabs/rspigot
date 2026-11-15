use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use rspigot_domain::world::{Dimension, WorldHeight};
use rspigot_domain::{BlockStateId, ChunkPos};

use super::Overworld;
use crate::buffer::ChunkBuffer;
use crate::classify::{MOTION_BLOCKING, MOTION_BLOCKING_NO_LEAVES, OCEAN_FLOOR, WORLD_SURFACE};
use crate::climate::{BiomeSource, ClimateScratch};
use crate::resources::Resources;

const UNKNOWN: u8 = u8::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Heightmap {
    WorldSurface,
    OceanFloor,
    MotionBlocking,
    MotionBlockingNoLeaves,
}

impl Heightmap {
    pub(crate) fn from_name(name: &str) -> Self {
        match name {
            "OCEAN_FLOOR" | "OCEAN_FLOOR_WG" => Self::OceanFloor,
            "MOTION_BLOCKING" => Self::MotionBlocking,
            "MOTION_BLOCKING_NO_LEAVES" => Self::MotionBlockingNoLeaves,
            _ => Self::WorldSurface,
        }
    }

    const fn bit(self) -> u8 {
        match self {
            Self::WorldSurface => WORLD_SURFACE,
            Self::OceanFloor => OCEAN_FLOOR,
            Self::MotionBlocking => MOTION_BLOCKING,
            Self::MotionBlockingNoLeaves => MOTION_BLOCKING_NO_LEAVES,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ChunkBiomes {
    cells: Arc<Vec<AtomicU8>>,
    base: i32,
    layers: usize,
}

impl ChunkBiomes {
    pub(crate) fn new(height: WorldHeight) -> Self {
        let layers = height.section_count() * 4;
        Self {
            cells: Arc::new((0..layers * 16).map(|_| AtomicU8::new(UNKNOWN)).collect()),
            base: height.min_y.div_euclid(4),
            layers,
        }
    }

    pub(crate) fn get(
        &self,
        source: &BiomeSource,
        scratch: &mut ClimateScratch,
        pos: ChunkPos,
        x: usize,
        y: i32,
        z: usize,
    ) -> u8 {
        let layer = ((y >> 2) - self.base).clamp(0, self.layers as i32 - 1) as usize;
        let (cx, cz) = (x >> 2, z >> 2);
        let slot = layer * 16 + cz * 4 + cx;
        let known = self.cells[slot].load(Ordering::Relaxed);
        if known != UNKNOWN {
            return known;
        }
        let value = source.biome_index(
            scratch,
            pos.x * 4 + cx as i32,
            self.base + layer as i32,
            pos.z * 4 + cz as i32,
        );
        self.cells[slot].store(value, Ordering::Relaxed);
        value
    }

    pub(crate) fn complete(
        &self,
        source: &BiomeSource,
        scratch: &mut ClimateScratch,
        pos: ChunkPos,
    ) -> Vec<u8> {
        for layer in 0..self.layers {
            for cell in 0..16usize {
                let y = (self.base + layer as i32) << 2;
                self.get(source, scratch, pos, (cell & 3) << 2, y, (cell >> 2) << 2);
            }
        }
        self.cells
            .iter()
            .map(|cell| cell.load(Ordering::Relaxed))
            .collect()
    }
}

impl TerrainChunk {
    pub(crate) fn new(
        buffer: ChunkBuffer,
        biomes: ChunkBiomes,
        hint: [i32; 256],
        height: WorldHeight,
    ) -> Self {
        let tops = std::array::from_fn(|i| {
            let (x, z) = (i & 15, i >> 4);
            let mut y = hint[i].min(height.max_y() + 1);
            while y > height.min_y && buffer.get(x, y - 1, z).is_air() {
                y -= 1;
            }
            while y <= height.max_y() && !buffer.get(x, y, z).is_air() {
                y += 1;
            }
            y
        });
        Self {
            buffer,
            biomes,
            tops,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TerrainChunk {
    pub(crate) buffer: ChunkBuffer,
    pub(crate) biomes: ChunkBiomes,
    pub(crate) tops: [i32; 256],
}

pub(crate) type Slot = TerrainChunk;

pub(crate) struct Region<'a> {
    overworld: Option<&'a Overworld>,
    resources: &'a Resources,
    dimension: &'a Dimension,
    center: ChunkPos,
    slots: [Option<Box<Slot>>; 9],
    scratch: Option<ClimateScratch>,
    fixed_biome: u8,
}

fn slot_index(dx: i32, dz: i32) -> usize {
    ((dz + 1) * 3 + (dx + 1)) as usize
}

impl<'a> Region<'a> {
    pub(crate) fn new(
        overworld: &'a Overworld,
        resources: &'a Resources,
        dimension: &'a Dimension,
        center: ChunkPos,
    ) -> Self {
        Self {
            overworld: Some(overworld),
            resources,
            dimension,
            center,
            slots: std::array::from_fn(|_| None),
            scratch: Some(overworld.source.scratch()),
            fixed_biome: 0,
        }
    }

    #[cfg(test)]
    pub(crate) fn fixture(
        resources: &'a Resources,
        dimension: &'a Dimension,
        center: ChunkPos,
        biome: u8,
        fill: impl Fn(ChunkPos, &mut ChunkBuffer),
    ) -> Self {
        let mut region = Self {
            overworld: None,
            resources,
            dimension,
            center,
            slots: std::array::from_fn(|_| None),
            scratch: None,
            fixed_biome: biome,
        };
        for dz in -1..=1 {
            for dx in -1..=1 {
                let pos = ChunkPos::new(center.x + dx, center.z + dz);
                let mut buffer = ChunkBuffer::new(dimension.height);
                fill(pos, &mut buffer);
                region.install(slot_index(dx, dz), buffer);
            }
        }
        region
    }

    pub(crate) fn height(&self) -> WorldHeight {
        self.dimension.height
    }

    pub(crate) fn sea_level(&self) -> i32 {
        self.dimension.sea_level
    }

    pub(crate) fn center(&self) -> ChunkPos {
        self.center
    }

    #[cfg(test)]
    fn install(&mut self, index: usize, buffer: ChunkBuffer) {
        let height = self.dimension.height;
        let tops = std::array::from_fn(|i| {
            let (x, z) = (i & 15, i >> 4);
            let mut y = height.max_y();
            while y >= height.min_y && buffer.get(x, y, z).is_air() {
                y -= 1;
            }
            y + 1
        });
        self.slots[index] = Some(Box::new(Slot {
            buffer,
            biomes: ChunkBiomes::new(height),
            tops,
        }));
    }

    fn locate(&self, x: i32, z: i32) -> Option<(usize, usize, usize)> {
        let (cx, cz) = (x >> 4, z >> 4);
        let (dx, dz) = (cx - self.center.x, cz - self.center.z);
        if !(-1..=1).contains(&dx) || !(-1..=1).contains(&dz) {
            return None;
        }
        Some((slot_index(dx, dz), (x & 15) as usize, (z & 15) as usize))
    }

    fn ensure(&mut self, index: usize) {
        if self.slots[index].is_some() {
            return;
        }
        let Some(overworld) = self.overworld else {
            return;
        };
        let (dx, dz) = ((index % 3) as i32 - 1, (index / 3) as i32 - 1);
        let pos = ChunkPos::new(self.center.x + dx, self.center.z + dz);
        let Some(scratch) = self.scratch.as_mut() else {
            return;
        };
        let terrain = overworld.terrain_cached(self.resources, self.dimension, pos, scratch);
        self.slots[index] = Some(Box::new((*terrain).clone()));
    }

    pub(crate) fn get(&mut self, x: i32, y: i32, z: i32) -> BlockStateId {
        let Some((index, lx, lz)) = self.locate(x, z) else {
            return BlockStateId::AIR;
        };
        self.ensure(index);
        self.slots[index]
            .as_ref()
            .map_or(BlockStateId::AIR, |slot| slot.buffer.get(lx, y, lz))
    }

    pub(crate) fn set(&mut self, x: i32, y: i32, z: i32, state: BlockStateId) -> bool {
        if !self.dimension.height.contains(y) {
            return false;
        }
        let Some((index, lx, lz)) = self.locate(x, z) else {
            return false;
        };
        self.ensure(index);
        let Some(slot) = self.slots[index].as_mut() else {
            return false;
        };
        slot.buffer.set(lx, y, lz, state);
        let top = &mut slot.tops[(lz << 4) | lx];
        if state.is_air() {
            if y + 1 == *top {
                let mut next = y;
                while next >= self.dimension.height.min_y && slot.buffer.get(lx, next, lz).is_air()
                {
                    next -= 1;
                }
                *top = next + 1;
            }
        } else if y >= *top {
            *top = y + 1;
        }
        true
    }

    pub(crate) fn height_at(&mut self, map: Heightmap, x: i32, z: i32) -> i32 {
        let min_y = self.dimension.height.min_y;
        let Some((index, lx, lz)) = self.locate(x, z) else {
            return min_y;
        };
        self.ensure(index);
        let Some(slot) = self.slots[index].as_ref() else {
            return min_y;
        };
        let bit = map.bit();
        let mut y = slot.tops[(lz << 4) | lx] - 1;
        while y >= min_y {
            if self.resources.classifier.flags(slot.buffer.get(lx, y, lz)) & bit != 0 {
                return y + 1;
            }
            y -= 1;
        }
        min_y
    }

    pub(crate) fn biome(&mut self, x: i32, y: i32, z: i32) -> u8 {
        let Some((index, lx, lz)) = self.locate(x, z) else {
            return self.fixed_biome;
        };
        self.ensure(index);
        let (dx, dz) = ((index % 3) as i32 - 1, (index / 3) as i32 - 1);
        let pos = ChunkPos::new(self.center.x + dx, self.center.z + dz);
        let (Some(overworld), Some(scratch)) = (self.overworld, self.scratch.as_mut()) else {
            return self.fixed_biome;
        };
        match self.slots[index].as_mut() {
            Some(slot) => slot.biomes.get(&overworld.source, scratch, pos, lx, y, lz),
            None => self.fixed_biome,
        }
    }

    pub(crate) fn finish(mut self) -> (ChunkBuffer, Vec<u8>) {
        let index = slot_index(0, 0);
        self.ensure(index);
        let center = self.center;
        let fixed = self.fixed_biome;
        let overworld = self.overworld;
        let mut scratch = self.scratch.take();
        let Some(slot) = self.slots[index].take() else {
            return (ChunkBuffer::new(self.dimension.height), Vec::new());
        };
        let indices = match (overworld, scratch.as_mut()) {
            (Some(overworld), Some(scratch)) => {
                slot.biomes.complete(&overworld.source, scratch, center)
            }
            _ => vec![fixed; slot.biomes.cells.len()],
        };
        (slot.buffer, indices)
    }
}
