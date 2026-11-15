use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BlockPos, BlockStateId, ChunkPos, Vec3};

mod biomes;
mod density;
#[cfg(test)]
mod reference;

use crate::buffer::ChunkBuffer;
use crate::resources::{BlockKind, Palette, Resources};
use crate::rng::{hash3, unit};
use crate::veins::{Hosts, Vein, place};

const CEILING: i32 = 127;
const NOISE_HEIGHT: i32 = 128;
const SALT_ORE: u64 = 0x4E37_0B1E;
const SALT_BEDROCK: u64 = 0xB3D2_0C0F;
const SALT_MAGMA: u64 = 0x3A61_7A11;
const SPAWN_RINGS: i32 = 6;

#[derive(Debug, Clone)]
pub(crate) struct Nether {
    seed: u64,
    density: density::NetherDensity,
    biomes: biomes::NetherBiomes,
    veins: Vec<Vein>,
}

fn nether_veins(palette: &Palette) -> Vec<Vein> {
    let vein = |ore: BlockKind, count, size, min_y, max_y| Vein {
        count,
        size,
        min_y,
        max_y,
        triangle: false,
        spread: 3,
        rarity: 1,
        in_stone: palette[ore],
        in_deep: palette[ore],
    };
    vec![
        vein(BlockKind::NetherQuartzOre, 16, 14, 10, 117),
        vein(BlockKind::NetherGoldOre, 10, 10, 10, 117),
        vein(BlockKind::Gravel, 3, 20, 5, 41),
    ]
}

impl Nether {
    pub(crate) fn new(seed: u64, palette: &Palette) -> Self {
        Self {
            seed,
            density: density::NetherDensity::new(seed as i64),
            biomes: biomes::NetherBiomes::new(seed as i64),
            veins: nether_veins(palette),
        }
    }

    pub(crate) fn generate(
        &self,
        resources: &Resources,
        dimension: &Dimension,
        pos: ChunkPos,
    ) -> ChunkColumn {
        let palette = &resources.blocks;
        let height = dimension.height;
        let bottom = height.min_y;
        let top = CEILING.min(height.max_y());
        let x0 = pos.x * 16;
        let z0 = pos.z * 16;
        let netherrack = palette[BlockKind::Netherrack];
        let lava = palette[BlockKind::Lava];
        let mut buffer = ChunkBuffer::new(height);
        let corners = density::Corners::build(
            &self.density,
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
        place(
            &mut buffer,
            pos,
            self.seed,
            SALT_ORE,
            &self.veins,
            Hosts {
                stone: netherrack,
                deep: netherrack,
            },
        );
        self.magma(&mut buffer, palette, pos, dimension.sea_level);
        self.bedrock(&mut buffer, palette, pos, bottom, top);
        let sections = height.section_count();
        let mut biomes = Vec::with_capacity(sections * 64);
        for _ in 0..sections * 4 {
            for qz in 0..4 {
                for qx in 0..4 {
                    biomes.push(self.biomes.biome(pos.x * 4 + qx, pos.z * 4 + qz));
                }
            }
        }
        buffer.finish_layered(pos, &biomes, &resources.classifier)
    }

    fn magma(&self, buffer: &mut ChunkBuffer, palette: &Palette, pos: ChunkPos, sea_level: i32) {
        let netherrack = palette[BlockKind::Netherrack];
        let magma = palette[BlockKind::MagmaBlock];
        for z in 0..16usize {
            for x in 0..16usize {
                for y in (sea_level - 4)..=(sea_level + 2) {
                    if buffer.get(x, y, z) != netherrack {
                        continue;
                    }
                    let hash = hash3(
                        self.seed ^ SALT_MAGMA,
                        pos.x * 16 + x as i32,
                        y,
                        pos.z * 16 + z as i32,
                    );
                    if unit(hash) < 0.06 {
                        buffer.set(x, y, z, magma);
                    }
                }
            }
        }
    }

    fn bedrock(
        &self,
        buffer: &mut ChunkBuffer,
        palette: &Palette,
        pos: ChunkPos,
        bottom: i32,
        top: i32,
    ) {
        let bedrock = palette[BlockKind::Bedrock];
        for k in 0..5 {
            let chance = f64::from(5 - k) / 5.0;
            for (y, always) in [(bottom + k, k == 0), (top - k, k == 0)] {
                for z in 0..16usize {
                    for x in 0..16usize {
                        let hash = hash3(
                            self.seed ^ SALT_BEDROCK,
                            pos.x * 16 + x as i32,
                            y,
                            pos.z * 16 + z as i32,
                        );
                        if always || unit(hash) < chance {
                            buffer.set(x, y, z, bedrock);
                        }
                    }
                }
            }
        }
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
