use rspigot_domain::chunk::{
    Biomes, Blocks, ChunkColumn, ChunkSection, PaletteStrategy, PalettedContainer,
};
use rspigot_domain::world::WorldHeight;
use rspigot_domain::{BiomeId, BlockStateId, ChunkPos};

use crate::classify::Classifier;

pub(crate) const LAYER: usize = 256;
pub(crate) const SECTION: usize = 4096;

pub(crate) enum Packed {
    Uniform(u16),
    Indirect {
        palette: Vec<u16>,
        bits: u8,
        data: Vec<u64>,
    },
}

pub(crate) fn palettize(values: &[u16], min_bits: u8) -> Packed {
    let Some(&first) = values.first() else {
        return Packed::Uniform(0);
    };
    if values.iter().all(|v| *v == first) {
        return Packed::Uniform(first);
    }
    let mut palette: Vec<u16> = Vec::with_capacity(16);
    palette.push(first);
    let mut indices = [0u16; SECTION];
    let mut last = first;
    let mut last_index = 0u16;
    for (slot, value) in indices.iter_mut().zip(values) {
        if *value != last {
            last = *value;
            last_index = match palette.iter().position(|p| *p == last) {
                Some(index) => index as u16,
                None => {
                    palette.push(last);
                    (palette.len() - 1) as u16
                }
            };
        }
        *slot = last_index;
    }
    let needed = (usize::BITS - (palette.len() - 1).leading_zeros()) as u8;
    let bits = needed.max(min_bits);
    let per_long = 64 / usize::from(bits);
    let mut data = vec![0u64; values.len().div_ceil(per_long)];
    for (long, chunk) in data
        .iter_mut()
        .zip(indices[..values.len()].chunks(per_long))
    {
        let mut acc = 0u64;
        for (slot, index) in chunk.iter().enumerate() {
            acc |= u64::from(*index) << (slot * usize::from(bits));
        }
        *long = acc;
    }
    Packed::Indirect {
        palette,
        bits,
        data,
    }
}

fn container<T: Copy + Eq, S: PaletteStrategy>(
    values: &[u16],
    wrap: impl Fn(u16) -> T,
) -> (PalettedContainer<T, S>, Option<u16>) {
    match palettize(values, S::MIN_BITS) {
        Packed::Uniform(value) => (PalettedContainer::single(wrap(value)), Some(value)),
        Packed::Indirect {
            palette,
            bits,
            data,
        } => {
            let typed: Vec<T> = palette.into_iter().map(&wrap).collect();
            let built = PalettedContainer::from_packed(typed, bits, data).unwrap_or_else(|_| {
                let all: Vec<T> = values.iter().map(|v| wrap(*v)).collect();
                PalettedContainer::from_values(&all)
            });
            (built, None)
        }
    }
}

pub(crate) fn block_container(
    values: &[u16],
) -> (PalettedContainer<BlockStateId, Blocks>, Option<u16>) {
    container(values, BlockStateId)
}

pub(crate) const SECTION_BIOMES: usize = 64;

pub(crate) fn biome_section_container(values: &[BiomeId]) -> PalettedContainer<BiomeId, Biomes> {
    let raw: Vec<u16> = values.iter().map(|biome| biome.0).collect();
    container(&raw, BiomeId).0
}

pub(crate) fn biome_container(cells: &[BiomeId; 16]) -> PalettedContainer<BiomeId, Biomes> {
    let mut values = [0u16; 64];
    for layer in values.chunks_mut(16) {
        for (slot, cell) in layer.iter_mut().zip(cells) {
            *slot = cell.0;
        }
    }
    container(&values, BiomeId).0
}

#[derive(Debug, Clone)]
pub(crate) struct ChunkBuffer {
    height: WorldHeight,
    min_y: i32,
    max_y: i32,
    blocks: Vec<u16>,
}

impl ChunkBuffer {
    pub(crate) fn new(height: WorldHeight) -> Self {
        Self {
            height,
            min_y: height.min_y,
            max_y: height.max_y(),
            blocks: vec![0u16; height.height as usize * LAYER],
        }
    }

    pub(crate) fn height(&self) -> WorldHeight {
        self.height
    }

    pub(crate) fn offset(&self, y: i32) -> usize {
        (y - self.height.min_y) as usize * LAYER
    }

    pub(crate) fn get(&self, x: usize, y: i32, z: usize) -> BlockStateId {
        if y < self.min_y || y > self.max_y {
            return BlockStateId::AIR;
        }
        BlockStateId(self.blocks[((y - self.min_y) as usize) * LAYER + (z << 4) + x])
    }

    pub(crate) fn set(&mut self, x: usize, y: i32, z: usize, state: BlockStateId) {
        if y >= self.min_y && y <= self.max_y {
            let index = ((y - self.min_y) as usize) * LAYER + (z << 4) + x;
            self.blocks[index] = state.0;
        }
    }

    pub(crate) fn fill_layer(&mut self, y: i32, state: BlockStateId) {
        if self.height.contains(y) {
            let start = self.offset(y);
            self.blocks[start..start + LAYER].fill(state.0);
        }
    }

    #[cfg(test)]
    pub(crate) fn raw(&self) -> &[u16] {
        &self.blocks
    }

    pub(crate) fn finish(
        self,
        pos: ChunkPos,
        cells: &[BiomeId; 16],
        classifier: &Classifier,
    ) -> ChunkColumn {
        let biomes = biome_container(cells);
        self.assemble(pos, cells[0], classifier, |_| biomes.clone())
    }

    pub(crate) fn finish_layered(
        self,
        pos: ChunkPos,
        biomes: &[BiomeId],
        classifier: &Classifier,
    ) -> ChunkColumn {
        let fallback = biomes.first().copied().unwrap_or(BiomeId(0));
        self.assemble(pos, fallback, classifier, |section| {
            let start = section * SECTION_BIOMES;
            biome_section_container(&biomes[start..start + SECTION_BIOMES])
        })
    }

    fn assemble(
        self,
        pos: ChunkPos,
        fallback: BiomeId,
        classifier: &Classifier,
        biomes: impl Fn(usize) -> PalettedContainer<BiomeId, Biomes>,
    ) -> ChunkColumn {
        let count = self.height.section_count();
        let mut uniform = Vec::with_capacity(count);
        let mut sections = Vec::with_capacity(count);
        for (index, values) in self.blocks.chunks(SECTION).enumerate() {
            let (blocks, single) = block_container(values);
            uniform.push(single);
            sections.push(ChunkSection::from_parts(blocks, biomes(index)));
        }
        let heightmaps = classifier.heightmaps(&self.blocks, &uniform, self.height);
        let mut column = ChunkColumn::from_sections(pos, self.height, sections)
            .unwrap_or_else(|| ChunkColumn::empty(pos, self.height, fallback));
        column.heightmaps = heightmaps;
        column
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_sections_collapse() {
        match palettize(&[7u16; SECTION], 4) {
            Packed::Uniform(7) => {}
            _ => panic!(),
        }
        let (container, single) = block_container(&[0u16; SECTION]);
        assert_eq!(container.single_value(), Some(BlockStateId::AIR));
        assert_eq!(single, Some(0));
    }

    #[test]
    fn packed_matches_source_for_all_widths() {
        for distinct in [2usize, 5, 16, 17, 40, 130] {
            let values: Vec<u16> = (0..SECTION)
                .map(|i| ((i * 31 + i / 7) % distinct) as u16 + 1)
                .collect();
            let (container, single) = block_container(&values);
            assert_eq!(single, None);
            for (i, v) in values.iter().enumerate() {
                assert_eq!(container.get(i), BlockStateId(*v), "{distinct} {i}");
            }
        }
    }

    #[test]
    fn biome_layers_repeat_cells() {
        let mut cells = [BiomeId(1); 16];
        cells[5] = BiomeId(9);
        let container = biome_container(&cells);
        for y in 0..4 {
            for z in 0..4 {
                for x in 0..4 {
                    let expected = if z * 4 + x == 5 { 9 } else { 1 };
                    assert_eq!(container.get((y << 4) | (z << 2) | x), BiomeId(expected));
                }
            }
        }
        assert_eq!(
            biome_container(&[BiomeId(3); 16]).single_value(),
            Some(BiomeId(3))
        );
    }

    #[test]
    fn layered_biomes_keep_each_section_apart() {
        let height = WorldHeight::OVERWORLD;
        let sections = height.section_count();
        let biomes: Vec<BiomeId> = (0..sections * SECTION_BIOMES)
            .map(|i| BiomeId((i / SECTION_BIOMES) as u16 % 3 + (i % 4 == 0) as u16 * 10))
            .collect();
        let column = ChunkBuffer::new(height).finish_layered(
            ChunkPos::new(0, 0),
            &biomes,
            &Classifier::new(),
        );
        for (index, section) in column.sections().iter().enumerate() {
            for local in 0..SECTION_BIOMES {
                let (x, z, y) = (local & 3, (local >> 2) & 3, local >> 4);
                assert_eq!(
                    section.biome(x, y, z),
                    biomes[index * SECTION_BIOMES + local],
                    "{index} {local}"
                );
            }
        }
    }

    #[test]
    fn buffer_round_trips_into_column() {
        let height = WorldHeight::OVERWORLD;
        let mut buffer = ChunkBuffer::new(height);
        buffer.set(3, -64, 4, BlockStateId(1));
        buffer.set(15, 319, 15, BlockStateId(2));
        buffer.set(0, 400, 0, BlockStateId(3));
        buffer.fill_layer(10, BlockStateId(4));
        assert_eq!(buffer.get(3, -64, 4), BlockStateId(1));
        assert_eq!(buffer.get(0, 400, 0), BlockStateId::AIR);
        let classifier = Classifier::new();
        let column = buffer.finish(ChunkPos::new(-3, 2), &[BiomeId(5); 16], &classifier);
        assert_eq!(column.pos(), ChunkPos::new(-3, 2));
        let origin = ChunkPos::new(-3, 2);
        let block = |x: i32, y: i32, z: i32| {
            column.block(rspigot_domain::BlockPos::new(
                origin.min_block_x() + x,
                y,
                origin.min_block_z() + z,
            ))
        };
        assert_eq!(block(3, -64, 4), Some(BlockStateId(1)));
        assert_eq!(block(15, 319, 15), Some(BlockStateId(2)));
        assert_eq!(block(9, 10, 9), Some(BlockStateId(4)));
        assert_eq!(block(9, 11, 9), Some(BlockStateId::AIR));
        assert_eq!(column.sections()[0].biome(0, 0, 0), BiomeId(5));
        assert!(!column.is_dirty());
    }
}
