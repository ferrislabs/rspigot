use std::f64::consts::PI;

use crate::BlockPos;
use crate::entity::Aabb;
use crate::random::LegacyRandom;
use crate::random::RandomSource;

pub const SPIKE_COUNT: usize = 10;
pub const MIN_Y: i32 = -2032;
pub const MAX_Y: i32 = 2031;
const PILLAR_RING: f64 = 42.0;
const ANGLE_STEP: f64 = 0.314_159_265_358_979_3;
const CAGE_RADIUS: i32 = 2;
const CAGE_HEIGHT: i32 = 4;
const GROUND_CLEAR_ABOVE: i32 = 10;
const AIR_FLOOR: i32 = 65;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndSpike {
    pub center_x: i32,
    pub center_z: i32,
    pub radius: i32,
    pub height: i32,
    pub guarded: bool,
}

impl EndSpike {
    pub fn top_bounding_box(&self) -> Aabb {
        Aabb::new(
            f64::from(self.center_x - self.radius),
            f64::from(MIN_Y),
            f64::from(self.center_z - self.radius),
            f64::from(self.center_x + self.radius),
            f64::from(MAX_Y),
            f64::from(self.center_z + self.radius),
        )
    }

    pub fn is_center_within_chunk(&self, chunk_origin: BlockPos) -> bool {
        (chunk_origin.x >> 4) == (self.center_x >> 4)
            && (chunk_origin.z >> 4) == (self.center_z >> 4)
    }

    pub fn crystal_spot(&self) -> (f64, f64, f64) {
        (
            f64::from(self.center_x) + 0.5,
            f64::from(self.height + 1),
            f64::from(self.center_z) + 0.5,
        )
    }

    pub fn top_block(&self) -> BlockPos {
        BlockPos::new(self.center_x, self.height, self.center_z)
    }

    pub fn contains_column(&self, x: i32, z: i32, y: i32) -> bool {
        let dx = f64::from(x - self.center_x);
        let dz = f64::from(z - self.center_z);
        dx * dx + dz * dz <= f64::from(self.radius * self.radius + 1) && y < self.height
    }
}

pub fn spikes_for_seed(world_seed: i64) -> Vec<EndSpike> {
    let key = LegacyRandom::new(world_seed).next_long() & 0xffff;
    let mut random = LegacyRandom::new(key);
    let mut sizes: Vec<i32> = (0..SPIKE_COUNT as i32).collect();
    let mut remaining = sizes.len();
    while remaining > 1 {
        let swap = random.next_int(remaining as i32) as usize;
        sizes.swap(remaining - 1, swap);
        remaining -= 1;
    }
    (0..SPIKE_COUNT)
        .map(|index| {
            let angle = (index as f64 * ANGLE_STEP + -PI) * 2.0;
            let size = sizes[index];
            EndSpike {
                center_x: (angle.cos() * PILLAR_RING).floor() as i32,
                center_z: (angle.sin() * PILLAR_RING).floor() as i32,
                radius: size / 3 + 2,
                height: size * 3 + 76,
                guarded: size == 1 || size == 2,
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpikeBlock {
    Obsidian,
    Air,
    IronBars {
        north: bool,
        south: bool,
        west: bool,
        east: bool,
    },
    Bedrock,
    Fire,
}

pub fn spike_blocks(spike: &EndSpike, min_y: i32) -> Vec<(BlockPos, SpikeBlock)> {
    let mut out = Vec::new();
    let radius = spike.radius;
    for x in spike.center_x - radius..=spike.center_x + radius {
        for z in spike.center_z - radius..=spike.center_z + radius {
            for y in min_y..=spike.height + GROUND_CLEAR_ABOVE {
                let dx = f64::from(x - spike.center_x);
                let dz = f64::from(z - spike.center_z);
                let inside = dx * dx + dz * dz <= f64::from(radius * radius + 1);
                if inside && y < spike.height {
                    out.push((BlockPos::new(x, y, z), SpikeBlock::Obsidian));
                } else if y > AIR_FLOOR {
                    out.push((BlockPos::new(x, y, z), SpikeBlock::Air));
                }
            }
        }
    }
    if spike.guarded {
        out.extend(cage_blocks(spike));
    }
    out
}

pub fn cage_blocks(spike: &EndSpike) -> Vec<(BlockPos, SpikeBlock)> {
    let mut out = Vec::new();
    for dx in -CAGE_RADIUS..=CAGE_RADIUS {
        for dz in -CAGE_RADIUS..=CAGE_RADIUS {
            for dy in 0..CAGE_HEIGHT {
                let edge_x = dx.abs() == CAGE_RADIUS;
                let edge_z = dz.abs() == CAGE_RADIUS;
                let roof = dy == CAGE_HEIGHT - 1;
                if !edge_x && !edge_z && !roof {
                    continue;
                }
                let along_x = dx == -CAGE_RADIUS || dx == CAGE_RADIUS || roof;
                let along_z = dz == -CAGE_RADIUS || dz == CAGE_RADIUS || roof;
                out.push((
                    BlockPos::new(spike.center_x + dx, spike.height + dy, spike.center_z + dz),
                    SpikeBlock::IronBars {
                        north: along_x && dz != -CAGE_RADIUS,
                        south: along_x && dz != CAGE_RADIUS,
                        west: along_z && dx != -CAGE_RADIUS,
                        east: along_z && dx != CAGE_RADIUS,
                    },
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_ten_pillars_on_a_ring_of_forty_two_blocks() {
        let spikes = spikes_for_seed(1234);
        assert_eq!(spikes.len(), 10);
        for spike in &spikes {
            let radius = f64::from(spike.center_x).hypot(f64::from(spike.center_z));
            assert!((radius - 42.0).abs() < 1.5, "{spike:?}");
        }
        assert_eq!((spikes[0].center_x, spikes[0].center_z), (42, 0));
    }

    #[test]
    fn the_sizes_are_a_permutation_of_zero_to_nine() {
        let spikes = spikes_for_seed(-77);
        let mut heights: Vec<i32> = spikes.iter().map(|spike| spike.height).collect();
        heights.sort_unstable();
        assert_eq!(heights, vec![76, 79, 82, 85, 88, 91, 94, 97, 100, 103]);
        let mut radii: Vec<i32> = spikes.iter().map(|spike| spike.radius).collect();
        radii.sort_unstable();
        assert_eq!(radii, vec![2, 2, 2, 3, 3, 3, 4, 4, 4, 5]);
    }

    #[test]
    fn only_the_two_smallest_nonzero_sizes_are_guarded() {
        let spikes = spikes_for_seed(99);
        let guarded: Vec<i32> = spikes
            .iter()
            .filter(|spike| spike.guarded)
            .map(|spike| spike.height)
            .collect();
        let mut sorted = guarded;
        sorted.sort_unstable();
        assert_eq!(sorted, vec![79, 82]);
    }

    #[test]
    fn the_same_world_seed_gives_the_same_pillars_and_other_seeds_differ() {
        assert_eq!(spikes_for_seed(5), spikes_for_seed(5));
        assert_ne!(spikes_for_seed(5), spikes_for_seed(6));
    }

    #[test]
    fn the_seed_keys_only_the_low_sixteen_bits_of_the_first_long() {
        let key = LegacyRandom::new(42).next_long() & 0xffff;
        let mut keyed = LegacyRandom::new(key);
        let mut sizes: Vec<i32> = (0..10).collect();
        for i in (2..=10).rev() {
            let j = keyed.next_int(i) as usize;
            sizes.swap(i as usize - 1, j);
        }
        let heights: Vec<i32> = spikes_for_seed(42).iter().map(|s| s.height).collect();
        let expected: Vec<i32> = sizes.iter().map(|size| size * 3 + 76).collect();
        assert_eq!(heights, expected);
    }

    #[test]
    fn the_top_box_spans_every_height_over_the_pillar() {
        let spike = EndSpike {
            center_x: 10,
            center_z: -5,
            radius: 3,
            height: 90,
            guarded: false,
        };
        let area = spike.top_bounding_box();
        assert_eq!(area, Aabb::new(7.0, -2032.0, -8.0, 13.0, 2031.0, -2.0));
        assert!(area.intersects(&Aabb::new(10.0, 91.0, -5.0, 12.0, 93.0, -3.0)));
    }

    #[test]
    fn the_pillar_chunk_contains_its_center() {
        let spike = EndSpike {
            center_x: 42,
            center_z: -1,
            radius: 3,
            height: 90,
            guarded: false,
        };
        assert!(spike.is_center_within_chunk(BlockPos::new(32, 0, -16)));
        assert!(!spike.is_center_within_chunk(BlockPos::new(48, 0, -16)));
    }

    #[test]
    fn the_crystal_sits_on_top_of_the_pillar_in_the_middle_of_its_block() {
        let spike = EndSpike {
            center_x: 42,
            center_z: 0,
            radius: 3,
            height: 90,
            guarded: false,
        };
        assert_eq!(spike.crystal_spot(), (42.5, 91.0, 0.5));
    }

    #[test]
    fn an_unguarded_pillar_is_solid_obsidian_below_its_height_and_cleared_above() {
        let spike = EndSpike {
            center_x: 0,
            center_z: 0,
            radius: 2,
            height: 80,
            guarded: false,
        };
        let blocks = spike_blocks(&spike, 0);
        let at = |x: i32, y: i32, z: i32| {
            blocks
                .iter()
                .find(|(pos, _)| *pos == BlockPos::new(x, y, z))
                .map(|(_, block)| *block)
        };
        assert_eq!(at(0, 10, 0), Some(SpikeBlock::Obsidian));
        assert_eq!(at(0, 79, 0), Some(SpikeBlock::Obsidian));
        assert_eq!(at(0, 80, 0), Some(SpikeBlock::Air));
        assert_eq!(at(2, 10, 2), None);
        assert_eq!(at(2, 70, 2), Some(SpikeBlock::Air));
        assert_eq!(at(2, 60, 2), None);
    }

    #[test]
    fn a_guarded_pillar_gets_a_two_block_cage_of_iron_bars_with_a_roof() {
        let spike = EndSpike {
            center_x: 0,
            center_z: 0,
            radius: 3,
            height: 80,
            guarded: true,
        };
        let cage = cage_blocks(&spike);
        assert!(
            cage.iter()
                .all(|(_, block)| matches!(block, SpikeBlock::IronBars { .. }))
        );
        assert!(cage.iter().any(|(pos, _)| *pos == BlockPos::new(0, 83, 0)));
        assert!(!cage.iter().any(|(pos, _)| *pos == BlockPos::new(0, 81, 0)));
        assert!(cage.iter().any(|(pos, _)| *pos == BlockPos::new(2, 81, 0)));
        let corner = cage
            .iter()
            .find(|(pos, _)| *pos == BlockPos::new(-2, 80, -2))
            .unwrap()
            .1;
        assert_eq!(
            corner,
            SpikeBlock::IronBars {
                north: false,
                south: true,
                west: false,
                east: true
            }
        );
        assert_eq!(cage.len(), 25 * 4 - 9 * 3);
    }

    #[test]
    fn contains_column_follows_the_cylinder_below_the_top() {
        let spike = EndSpike {
            center_x: 0,
            center_z: 0,
            radius: 2,
            height: 80,
            guarded: false,
        };
        assert!(spike.contains_column(2, 1, 79));
        assert!(!spike.contains_column(2, 2, 79));
        assert!(!spike.contains_column(0, 0, 80));
    }
}
