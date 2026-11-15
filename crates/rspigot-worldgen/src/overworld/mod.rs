mod biomes;
mod caves;
mod decor;
mod freeze;
mod surface_rules;
mod terrain;
mod trees;

use rspigot_domain::chunk::{ChunkColumn, HeightmapKind};
use std::sync::Arc;

use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Vec3};

use crate::buffer::ChunkBuffer;
use crate::climate::{BiomeSource, ClimateScratch};
use crate::resources::{Biome, BlockKind, Palette, Resources, biome_id};
use crate::rng::{Rng, hash3, unit};
use crate::veins::{Hosts, Vein, place};
use rspigot_data::multi_noise::{ClimateVariant, overworld_biomes};

use biomes::{is_mountain, surface_biome};
use caves::Caves;
use decor::{ColumnInfo, Ground};
use freeze::{FreezeContext, freeze_top_layer};
use surface_rules::{Bounds, ColumnInput, SurfaceSystem};
use terrain::Terrain;
pub(crate) use terrain::smooth;

const SALT_ORE: u64 = 0x0DE5_1A3F;
const SALT_EMERALD: u64 = 0x3E3A_1D55;
const SALT_BEDROCK: u64 = 0xBED0_0C41;
const SALT_DEEPSLATE: u64 = 0xDEE9_51A7;
const SPAWN_STEP: i32 = 8;
const SPAWN_RINGS: i32 = 200;
const EMERALD_ATTEMPTS: u32 = 4;

#[derive(Debug, Clone)]
pub(crate) struct Overworld {
    seed: u64,
    terrain: Terrain,
    source: Arc<BiomeSource>,
    placed: Arc<[PlacedBiome]>,
    caves: Caves,
    surface: Arc<SurfaceSystem>,
    veins: Vec<Vein>,
}

#[derive(Debug, Clone, Copy)]
struct PlacedBiome {
    id: BiomeId,
    surface: Biome,
}

#[derive(Debug, Clone, Copy)]
struct OreShape {
    count: u32,
    size: u32,
    min_y: i32,
    max_y: i32,
    triangle: bool,
    spread: i32,
}

const fn shape(
    count: u32,
    size: u32,
    min_y: i32,
    max_y: i32,
    triangle: bool,
    spread: i32,
) -> OreShape {
    OreShape {
        count,
        size,
        min_y,
        max_y,
        triangle,
        spread,
    }
}

fn ore(palette: &Palette, stone: BlockKind, deep: BlockKind, shape: OreShape) -> Vein {
    Vein {
        count: shape.count,
        size: shape.size,
        min_y: shape.min_y,
        max_y: shape.max_y,
        triangle: shape.triangle,
        spread: shape.spread,
        rarity: 1,
        in_stone: palette[stone],
        in_deep: palette[deep],
    }
}

fn overworld_veins(palette: &Palette) -> Vec<Vein> {
    let blob = |block: BlockKind, count, min_y, max_y| {
        ore(
            palette,
            block,
            block,
            shape(count, 33, min_y, max_y, false, 4),
        )
    };
    let diamond = shape(1, 12, -64, 16, true, 3);
    vec![
        blob(BlockKind::Dirt, 3, 0, 160),
        blob(BlockKind::Gravel, 5, -64, 60),
        blob(BlockKind::Granite, 3, 0, 64),
        blob(BlockKind::Diorite, 3, 0, 64),
        blob(BlockKind::Andesite, 3, 0, 64),
        blob(BlockKind::Tuff, 3, -64, 0),
        ore(
            palette,
            BlockKind::CoalOre,
            BlockKind::DeepslateCoalOre,
            shape(20, 14, 0, 190, false, 2),
        ),
        ore(
            palette,
            BlockKind::CoalOre,
            BlockKind::DeepslateCoalOre,
            shape(18, 12, 136, 319, false, 2),
        ),
        ore(
            palette,
            BlockKind::IronOre,
            BlockKind::DeepslateIronOre,
            shape(10, 9, -24, 56, true, 2),
        ),
        ore(
            palette,
            BlockKind::IronOre,
            BlockKind::DeepslateIronOre,
            shape(8, 4, -64, 72, false, 1),
        ),
        ore(
            palette,
            BlockKind::IronOre,
            BlockKind::DeepslateIronOre,
            shape(20, 8, 80, 319, true, 2),
        ),
        ore(
            palette,
            BlockKind::CopperOre,
            BlockKind::DeepslateCopperOre,
            shape(14, 10, -16, 112, true, 2),
        ),
        ore(
            palette,
            BlockKind::GoldOre,
            BlockKind::DeepslateGoldOre,
            shape(2, 9, -64, 32, true, 2),
        ),
        ore(
            palette,
            BlockKind::GoldOre,
            BlockKind::DeepslateGoldOre,
            shape(1, 9, -64, -48, false, 2),
        ),
        ore(
            palette,
            BlockKind::RedstoneOre,
            BlockKind::DeepslateRedstoneOre,
            shape(6, 8, -64, 15, false, 2),
        ),
        ore(
            palette,
            BlockKind::LapisOre,
            BlockKind::DeepslateLapisOre,
            shape(2, 7, -32, 32, true, 2),
        ),
        ore(
            palette,
            BlockKind::DiamondOre,
            BlockKind::DeepslateDiamondOre,
            shape(3, 4, -64, 16, true, 1),
        ),
        Vein {
            rarity: 18,
            ..ore(
                palette,
                BlockKind::DiamondOre,
                BlockKind::DeepslateDiamondOre,
                diamond,
            )
        },
    ]
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
                .map(|name| PlacedBiome {
                    id: biome_id(name),
                    surface: surface_biome(name),
                })
                .collect(),
            caves: Caves::new(seed),
            surface: Arc::new(SurfaceSystem::new(seed as i64, palette)),
            veins: overworld_veins(palette),
        }
    }

    fn layered_biomes(
        &self,
        scratch: &mut ClimateScratch,
        dimension: &Dimension,
        pos: ChunkPos,
    ) -> Vec<u8> {
        let layers = dimension.height.section_count() * 4;
        let base = dimension.height.min_y.div_euclid(4);
        let mut layered = vec![0u8; layers * 16];
        for cz in 0..4usize {
            for cx in 0..4usize {
                for layer in 0..layers {
                    let index = self.source.biome_index(
                        scratch,
                        pos.x * 4 + cx as i32,
                        base + layer as i32,
                        pos.z * 4 + cz as i32,
                    );
                    let slot = (layer >> 2) * 64 + ((layer & 3) << 4) + (cz << 2) + cx;
                    layered[slot] = index;
                }
            }
        }
        layered
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
        let x0 = pos.x * 16;
        let z0 = pos.z * 16;

        let mut scratch = self.source.scratch();
        let corners = self.terrain.heights(&self.source, &mut scratch, x0, z0);
        let tops: [i32; 256] = std::array::from_fn(|i| {
            let (x, z) = ((i & 15) as i32, (i >> 4) as i32);
            let base = corners.at(x as usize, z as usize);
            let shifted = base + self.terrain.detail(x0 + x, z0 + z) + f64::from(sea_level - 63);
            Self::clamp_height(dimension, shifted)
        });
        let cell_indices: [u8; 16] = std::array::from_fn(|c| {
            let (cx, cz) = ((c & 3) as i32, (c >> 2) as i32);
            let center = ((cz * 4 + 2) * 16 + cx * 4 + 2) as usize;
            self.source.biome_index(
                &mut scratch,
                pos.x * 4 + cx,
                (tops[center] + terrain::SURFACE_SHIFT) >> 2,
                pos.z * 4 + cz,
            )
        });
        let cell_biomes: [Biome; 16] =
            std::array::from_fn(|c| self.placed[usize::from(cell_indices[c])].surface);
        let layered = self.layered_biomes(&mut scratch, dimension, pos);

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
        let biome_index = |x: usize, y: i32, z: usize| {
            let layer = ((y.clamp(min_y, height.max_y()) - min_y) >> 2) as usize;
            layered[(layer >> 2) * 64 + ((layer & 3) << 4) + ((z >> 2) << 2) + (x >> 2)]
        };
        let surface_height = |i: usize| tops[i].max(sea_level - 1) + 1;
        let mut columns = [ColumnInfo {
            height: min_y,
            ground: Ground::Bare,
            snow_cover: false,
            biome: Biome::Plains,
        }; 256];
        for i in 0..256usize {
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
                top: tops[i],
                gradient,
                biome_at: |y| biome_index(x, y, z),
            });
            let ground = buffer.get(x, tops[i], z);
            columns[i] = ColumnInfo {
                height: tops[i],
                ground: if ground == palette[BlockKind::GrassBlock] {
                    Ground::Grass
                } else if ground == palette[BlockKind::Sand] {
                    Ground::Sand
                } else {
                    Ground::Bare
                },
                snow_cover: false,
                biome: cell_biomes[((z >> 2) << 2) | (x >> 2)],
            };
        }
        place(
            &mut buffer,
            pos,
            self.seed,
            SALT_ORE,
            &self.veins,
            Hosts {
                stone: palette[BlockKind::Stone],
                deep: palette[BlockKind::Deepslate],
            },
        );
        self.emeralds(&mut buffer, palette, pos, &cell_biomes);
        decor::trees(&mut buffer, palette, self.seed, pos, &columns, sea_level);
        decor::ground_cover(&mut buffer, palette, self.seed, pos, &columns);
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
        self.bedrock(&mut buffer, palette, pos);
        let stored: Vec<BiomeId> = layered
            .iter()
            .map(|index| self.placed[usize::from(*index)].id)
            .collect();
        buffer.finish_layered(pos, &stored, &resources.classifier)
    }

    fn emeralds(
        &self,
        buffer: &mut ChunkBuffer,
        palette: &Palette,
        pos: ChunkPos,
        cell_biomes: &[Biome; 16],
    ) {
        let mut rng = Rng::for_chunk(self.seed, pos.x, pos.z, SALT_EMERALD);
        for _ in 0..EMERALD_ATTEMPTS {
            let x = rng.below(16) as usize;
            let z = rng.below(16) as usize;
            let y = rng.range(-16, 250);
            if !is_mountain(cell_biomes[((z >> 2) << 2) | (x >> 2)]) {
                continue;
            }
            let current = buffer.get(x, y, z);
            if current == palette[BlockKind::Stone] {
                buffer.set(x, y, z, palette[BlockKind::EmeraldOre]);
            } else if current == palette[BlockKind::Deepslate] {
                buffer.set(x, y, z, palette[BlockKind::DeepslateEmeraldOre]);
            }
        }
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
