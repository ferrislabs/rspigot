use crate::entity::physics::{BlockView, has_tag};
use crate::entity::{Aabb, fluid, raycast};
use crate::random::RandomSource;
use crate::world::WorldHeight;
use crate::{BlockPos, Vec3, mth};

const RAYS_PER_AXIS: usize = 16;
const STEP_DOUBLE: f64 = 0.300_000_011_920_928_96;
const POWER_STEP: f32 = 0.225_000_01;
const RESISTANCE_OFFSET: f32 = 0.3;
const RESISTANCE_SCALE: f32 = 0.3;
const MIN_POWER_FACTOR: f32 = 0.7;
const POWER_SPREAD: f32 = 0.6;
const FLUID_RESISTANCE: f32 = 100.0;
pub const MIN_RADIUS: f32 = 0.1;
pub const LARGE_RADIUS: f32 = 2.0;
pub const PACKET_RANGE_SQR: f64 = 4096.0;
const HASH_LOAD_FACTOR: f64 = 0.75;
const HASH_INITIAL_CAPACITY: usize = 16;
const WORLD_LIMIT: i32 = 30_000_000;
const WITHER_IMMUNE: &str = "minecraft:wither_immune";

pub fn ray_directions() -> Vec<Vec3> {
    let mut out = Vec::new();
    for xx in 0..RAYS_PER_AXIS {
        for yy in 0..RAYS_PER_AXIS {
            for zz in 0..RAYS_PER_AXIS {
                let edge = |v: usize| v == 0 || v == RAYS_PER_AXIS - 1;
                if !(edge(xx) || edge(yy) || edge(zz)) {
                    continue;
                }
                let component = |v: usize| f64::from(v as f32 / 15.0 * 2.0 - 1.0);
                let (x, y, z) = (component(xx), component(yy), component(zz));
                let length = (x * x + y * y + z * z).sqrt();
                out.push(Vec3::new(x / length, y / length, z / length));
            }
        }
    }
    out
}

pub fn block_resistance(view: &impl BlockView, pos: BlockPos) -> Option<f32> {
    let state = view.block_or_air(pos);
    let fluid = state.flags().has_fluid();
    if state.is_air() && !fluid {
        return None;
    }
    let block = state.kind().map_or(0.0, |kind| kind.settings().resistance);
    Some(if fluid {
        block.max(FLUID_RESISTANCE)
    } else {
        block
    })
}

pub fn exploded_positions(
    view: &impl BlockView,
    height: WorldHeight,
    center: Vec3,
    radius: f32,
    random: &mut impl RandomSource,
) -> Vec<BlockPos> {
    exploded_positions_capped(view, height, center, radius, random, None)
}

pub fn capped_resistance(view: &impl BlockView, pos: BlockPos, cap: Option<f32>) -> Option<f32> {
    let resistance = block_resistance(view, pos)?;
    Some(match cap {
        Some(cap) if !has_tag(view.block_or_air(pos), WITHER_IMMUNE) => resistance.min(cap),
        _ => resistance,
    })
}

pub fn exploded_positions_capped(
    view: &impl BlockView,
    height: WorldHeight,
    center: Vec3,
    radius: f32,
    random: &mut impl RandomSource,
    cap: Option<f32>,
) -> Vec<BlockPos> {
    let mut inserted: Vec<BlockPos> = Vec::new();
    let mut seen = rustc_hash::FxHashSet::default();
    for direction in ray_directions() {
        let mut power = radius * (MIN_POWER_FACTOR + random.next_float() * POWER_SPREAD);
        let mut point = center;
        while power > 0.0 {
            let pos = point.block();
            if !height.contains(pos.y) || !in_horizontal_bounds(pos) {
                break;
            }
            if let Some(resistance) = capped_resistance(view, pos, cap) {
                power -= (resistance + RESISTANCE_OFFSET) * RESISTANCE_SCALE;
            }
            if power > 0.0 && seen.insert(pos) {
                inserted.push(pos);
            }
            point = point + direction.scale(STEP_DOUBLE);
            power -= POWER_STEP;
        }
    }
    java_hash_set_order(&inserted)
}

fn in_horizontal_bounds(pos: BlockPos) -> bool {
    (-WORLD_LIMIT..WORLD_LIMIT).contains(&pos.x) && (-WORLD_LIMIT..WORLD_LIMIT).contains(&pos.z)
}

fn java_hash(pos: BlockPos) -> i32 {
    pos.y
        .wrapping_add(pos.z.wrapping_mul(31))
        .wrapping_mul(31)
        .wrapping_add(pos.x)
}

pub fn java_hash_set_order(inserted: &[BlockPos]) -> Vec<BlockPos> {
    let mut capacity = HASH_INITIAL_CAPACITY;
    while inserted.len() as f64 > capacity as f64 * HASH_LOAD_FACTOR {
        capacity *= 2;
    }
    let mask = capacity as i32 - 1;
    let bucket = |pos: &BlockPos| {
        let h = java_hash(*pos);
        (h ^ ((h as u32) >> 16) as i32) & mask
    };
    let mut ordered: Vec<(i32, usize)> = inserted
        .iter()
        .enumerate()
        .map(|(index, pos)| (bucket(pos), index))
        .collect();
    ordered.sort_unstable();
    ordered
        .into_iter()
        .map(|(_, index)| inserted[index])
        .collect()
}

pub fn shuffle<T>(list: &mut [T], random: &mut impl RandomSource) {
    let mut i = list.len();
    while i > 1 {
        let swap = random.next_int(i as i32) as usize;
        list.swap(i - 1, swap);
        i -= 1;
    }
}

pub fn seen_percent(view: &impl BlockView, center: Vec3, bounds: &Aabb) -> f32 {
    let xs = 1.0 / ((bounds.max_x - bounds.min_x) * 2.0 + 1.0);
    let ys = 1.0 / ((bounds.max_y - bounds.min_y) * 2.0 + 1.0);
    let zs = 1.0 / ((bounds.max_z - bounds.min_z) * 2.0 + 1.0);
    let x_offset = (1.0 - (1.0 / xs).floor() * xs) / 2.0;
    let z_offset = (1.0 - (1.0 / zs).floor() * zs) / 2.0;
    if !(xs >= 0.0 && ys >= 0.0 && zs >= 0.0) {
        return 0.0;
    }
    let mut hits = 0;
    let mut count = 0;
    let mut xx = 0.0;
    while xx <= 1.0 {
        let mut yy = 0.0;
        while yy <= 1.0 {
            let mut zz = 0.0;
            while zz <= 1.0 {
                let x = mth::lerp(xx, bounds.min_x, bounds.max_x);
                let y = mth::lerp(yy, bounds.min_y, bounds.max_y);
                let z = mth::lerp(zz, bounds.min_z, bounds.max_z);
                let from = Vec3::new(x + x_offset, y, z + z_offset);
                if raycast::clip_blocks(view, from, center).is_none() {
                    hits += 1;
                }
                count += 1;
                zz += zs;
            }
            yy += ys;
        }
        xx += xs;
    }
    hits as f32 / count as f32
}

pub fn entity_damage(radius: f32, center: Vec3, position: Vec3, exposure: f32) -> f32 {
    let double_radius = radius * 2.0;
    let distance = position.distance_squared(center).sqrt() / f64::from(double_radius);
    let power = (1.0 - distance) * f64::from(exposure);
    ((power * power + power) / 2.0 * 7.0 * f64::from(double_radius) + 1.0) as f32
}

pub fn knockback(
    radius: f32,
    center: Vec3,
    position: Vec3,
    origin: Vec3,
    exposure: f32,
    resistance: f64,
) -> Option<Vec3> {
    let double_radius = f64::from(radius * 2.0);
    let distance = position.distance_squared(center).sqrt() / double_radius;
    if distance > 1.0 {
        return None;
    }
    let direction = (origin - center).normalize();
    let power = (1.0 - distance) * f64::from(exposure) * (1.0 - resistance);
    Some(direction.scale(power))
}

pub fn hurt_area(center: Vec3, radius: f32) -> Aabb {
    let double_radius = f64::from(radius * 2.0);
    let floor = |v: f64| f64::from(mth::floor(v));
    Aabb::new(
        floor(center.x - double_radius - 1.0),
        floor(center.y - double_radius - 1.0),
        floor(center.z - double_radius - 1.0),
        floor(center.x + double_radius + 1.0),
        floor(center.y + double_radius + 1.0),
        floor(center.z + double_radius + 1.0),
    )
}

pub fn is_free(view: &impl BlockView, area: &Aabb) -> bool {
    fluid::is_free_of_collision(view, area)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rspigot_blocks::BlockKind;

    use super::*;
    use crate::BlockStateId;
    use crate::random::LegacyRandom;

    struct Ground(HashMap<BlockPos, BlockStateId>);

    impl BlockView for Ground {
        fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
            Some(self.0.get(&pos).copied().unwrap_or(BlockStateId::AIR))
        }
    }

    fn floor() -> Ground {
        let mut blocks = HashMap::new();
        for x in -6..=6 {
            for z in -6..=6 {
                blocks.insert(BlockPos::new(x, 63, z), BlockKind::Dirt.default_state());
                blocks.insert(BlockPos::new(x, 62, z), BlockKind::Obsidian.default_state());
            }
        }
        Ground(blocks)
    }

    #[test]
    fn the_shell_has_1352_rays_of_unit_length() {
        let rays = ray_directions();
        assert_eq!(rays.len(), 16 * 16 * 16 - 14 * 14 * 14);
        assert!(rays.iter().all(|r| (r.length() - 1.0).abs() < 1.0e-12));
    }

    #[test]
    fn creeper_blasts_dig_dirt_but_not_obsidian() {
        let ground = floor();
        let mut random = LegacyRandom::new(4);
        let blown = exploded_positions(
            &ground,
            WorldHeight::OVERWORLD,
            Vec3::new(0.5, 64.0, 0.5),
            3.0,
            &mut random,
        );
        assert!(blown.contains(&BlockPos::new(0, 63, 0)));
        assert!(!blown.iter().any(|pos| pos.y == 62));
        let unique: std::collections::HashSet<_> = blown.iter().collect();
        assert_eq!(unique.len(), blown.len());
    }

    #[test]
    fn a_skull_blast_caps_the_resistance_of_every_block_but_the_wither_immune_ones() {
        let mut ground = floor();
        ground
            .0
            .insert(BlockPos::new(0, 64, 0), BlockKind::Obsidian.default_state());
        ground
            .0
            .insert(BlockPos::new(1, 64, 0), BlockKind::Bedrock.default_state());
        ground
            .0
            .insert(BlockPos::new(2, 64, 0), BlockKind::Water.default_state());
        let obsidian = BlockPos::new(0, 64, 0);
        assert_eq!(block_resistance(&ground, obsidian), Some(1200.0));
        assert_eq!(capped_resistance(&ground, obsidian, None), Some(1200.0));
        assert_eq!(capped_resistance(&ground, obsidian, Some(0.8)), Some(0.8));
        let bedrock = BlockPos::new(1, 64, 0);
        assert_eq!(
            capped_resistance(&ground, bedrock, Some(0.8)),
            block_resistance(&ground, bedrock)
        );
        assert_eq!(
            capped_resistance(&ground, BlockPos::new(3, 64, 0), Some(0.8)),
            None
        );
        assert_eq!(
            capped_resistance(&ground, BlockPos::new(2, 64, 0), Some(0.8)),
            Some(0.8)
        );
    }

    #[test]
    fn a_capped_blast_of_power_one_digs_obsidian_where_an_uncapped_one_cannot() {
        let mut ground = floor();
        let target = BlockPos::new(0, 64, 0);
        ground.0.insert(target, BlockKind::Obsidian.default_state());
        let center = Vec3::new(0.5, 64.5, 0.5);
        let mut found = false;
        for seed in 0..8 {
            let mut random = LegacyRandom::new(seed);
            let plain =
                exploded_positions(&ground, WorldHeight::OVERWORLD, center, 1.0, &mut random);
            assert!(!plain.contains(&target));
            let mut random = LegacyRandom::new(seed);
            let capped = exploded_positions_capped(
                &ground,
                WorldHeight::OVERWORLD,
                center,
                1.0,
                &mut random,
                Some(0.8),
            );
            found |= capped.contains(&target);
        }
        assert!(found);
    }

    #[test]
    fn exposure_and_damage_follow_distance() {
        let ground = floor();
        let center = Vec3::new(0.5, 64.0, 0.5);
        let bounds = Aabb::of_size(Vec3::new(2.5, 64.0, 0.5), 0.6, 1.8);
        assert_eq!(seen_percent(&ground, center, &bounds), 1.0);
        let close = entity_damage(3.0, center, Vec3::new(1.5, 64.0, 0.5), 1.0);
        let far = entity_damage(3.0, center, Vec3::new(5.5, 64.0, 0.5), 1.0);
        assert!(close > far);
        assert_eq!(entity_damage(3.0, center, center, 1.0), 43.0);
    }

    #[test]
    fn hash_set_order_buckets_by_java_hash() {
        let positions = vec![BlockPos::new(1, 0, 0), BlockPos::new(0, 0, 0)];
        assert_eq!(
            java_hash_set_order(&positions),
            vec![BlockPos::new(0, 0, 0), BlockPos::new(1, 0, 0)]
        );
    }
}
