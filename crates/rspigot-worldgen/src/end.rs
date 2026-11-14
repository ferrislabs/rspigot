use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, ChunkPos, Vec3};

use crate::buffer::ChunkBuffer;
use crate::end_spikes::Pillars;
use crate::overworld::smooth;
use crate::perlin::Fractal;
use crate::resources::{Biome, BlockKind, Resources};
use crate::rng::mix;

const MAIN_RADIUS: f64 = 75.0;
const OUTER_START: f64 = 900.0;
const OUTER_FULL: f64 = 1200.0;

#[derive(Debug, Clone)]
pub(crate) struct End {
    islands: Fractal,
    tiny: Fractal,
    jitter: Fractal,
    pillars: Pillars,
}

#[derive(Debug, Clone, Copy)]
struct Slab {
    bottom: i32,
    top: i32,
}

impl End {
    pub(crate) fn new(seed: u64) -> Self {
        Self {
            islands: Fractal::new(mix(seed ^ 0xE4D1_5A11), 3),
            tiny: Fractal::new(mix(seed ^ 0x71E7_0001), 2),
            jitter: Fractal::new(mix(seed ^ 0x517C_C1B7), 2),
            pillars: Pillars::new(seed as i64),
        }
    }

    fn outer_value(&self, x: f64, z: f64) -> f64 {
        let distance = x.hypot(z);
        let mask = smooth(OUTER_START, OUTER_FULL, distance);
        let n = self.islands.at2(x / 110.0, z / 110.0) * 2.2;
        if mask > 0.0 && n * mask > 0.45 {
            ((n * mask - 0.45) / 0.55).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    fn slab(&self, x: i32, z: i32) -> Option<Slab> {
        let fx = f64::from(x);
        let fz = f64::from(z);
        let distance = fx.hypot(fz);
        let wobble = self.jitter.at2(fx / 24.0, fz / 24.0) * 3.0;
        if distance < MAIN_RADIUS {
            let edge = distance / MAIN_RADIUS;
            let lens = (1.0 - edge * edge).sqrt();
            let top = 64.0 - 6.0 * edge * edge + wobble * (0.4 + edge);
            let bottom = top - (3.0 + 30.0 * lens);
            return Some(Slab {
                bottom: bottom.floor() as i32,
                top: top.floor() as i32,
            });
        }
        let value = self.outer_value(fx, fz);
        if value > 0.0 {
            let top = 55.0 + 20.0 * value + wobble;
            let bottom = top - (4.0 + 26.0 * value.sqrt());
            return Some(Slab {
                bottom: bottom.floor() as i32,
                top: top.floor() as i32,
            });
        }
        if distance > 120.0 && distance < OUTER_START {
            let n = self.tiny.at2(fx / 28.0, fz / 28.0) * 2.4;
            if n > 0.8 {
                let v = ((n - 0.8) / 0.2).clamp(0.0, 1.0);
                let top = 58.0 + 6.0 * v + wobble * 0.5;
                let bottom = top - (2.0 + 8.0 * v);
                return Some(Slab {
                    bottom: bottom.floor() as i32,
                    top: top.floor() as i32,
                });
            }
        }
        None
    }

    fn biome(&self, x: i32, z: i32) -> Biome {
        let fx = f64::from(x);
        let fz = f64::from(z);
        let distance = fx.hypot(fz);
        if distance < 120.0 {
            Biome::TheEnd
        } else if distance < OUTER_START {
            Biome::SmallEndIslands
        } else {
            let value = self.outer_value(fx, fz);
            if value > 0.55 {
                Biome::EndHighlands
            } else if value > 0.2 {
                Biome::EndMidlands
            } else {
                Biome::EndBarrens
            }
        }
    }

    pub(crate) fn generate(
        &self,
        resources: &Resources,
        dimension: &Dimension,
        pos: ChunkPos,
    ) -> ChunkColumn {
        let end_stone = resources.blocks[BlockKind::EndStone];
        let mut buffer = ChunkBuffer::new(dimension.height);
        let x0 = pos.x * 16;
        let z0 = pos.z * 16;
        for z in 0..16usize {
            for x in 0..16usize {
                if let Some(slab) = self.slab(x0 + x as i32, z0 + z as i32) {
                    for y in slab.bottom..=slab.top {
                        buffer.set(x, y, z, end_stone);
                    }
                }
            }
        }
        self.pillars.place(&mut buffer, pos);
        let cells: [BiomeId; 16] = std::array::from_fn(|c| {
            let (cx, cz) = ((c & 3) as i32, (c >> 2) as i32);
            resources.biomes[self.biome(x0 + cx * 4 + 2, z0 + cz * 4 + 2)]
        });
        buffer.finish(pos, &cells, &resources.classifier)
    }

    pub(crate) fn pillars(&self) -> &Pillars {
        &self.pillars
    }

    pub(crate) fn spawn_point(&self, dimension: &Dimension) -> Vec3 {
        let top = self.slab(0, 0).map_or(dimension.sea_level, |slab| slab.top);
        Vec3::new(0.5, f64::from(top + 1), 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_island_is_a_lens_around_the_origin() {
        let end = End::new(1);
        let center = end.slab(0, 0).unwrap();
        assert!((60..=68).contains(&center.top));
        assert!(center.top - center.bottom >= 25);
        let rim = end.slab(70, 0).unwrap();
        assert!(rim.top - rim.bottom < center.top - center.bottom);
        assert!(
            end.slab(80, 0).is_none()
                || end.slab(80, 0).unwrap().top - end.slab(80, 0).unwrap().bottom < 12
        );
        assert!(
            end.slab(300, 300).is_none()
                || end.slab(300, 300).is_some_and(|s| s.top - s.bottom < 12)
        );
    }

    #[test]
    fn outer_islands_exist_far_from_the_center() {
        let end = End::new(77);
        let mut found = 0;
        for i in 0..80 {
            for j in 0..80 {
                if end.slab(1300 + i * 20, 1300 + j * 20).is_some() {
                    found += 1;
                }
            }
        }
        assert!(found > 200, "{found}");
    }

    #[test]
    fn biomes_follow_distance() {
        let end = End::new(2);
        assert_eq!(end.biome(0, 0), Biome::TheEnd);
        assert_eq!(end.biome(400, 0), Biome::SmallEndIslands);
        assert!(matches!(
            end.biome(2000, 2000),
            Biome::EndHighlands | Biome::EndMidlands | Biome::EndBarrens
        ));
    }
}
