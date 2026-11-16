use crate::legacy::JavaRandom;
use crate::structures::bbox::Pos;
use crate::structures::jigsaw::{Ground, Heightmap};
use crate::structures::placement::{BiomeProbe, large_feature_seed};

pub(crate) fn random(seed: i64, chunk: (i32, i32)) -> JavaRandom {
    let mut random = JavaRandom::new(0);
    large_feature_seed(&mut random, seed, chunk.0, chunk.1);
    random
}

pub(crate) fn could_exist<G: Ground>(
    ground: &G,
    valid: &[u8],
    probe: &impl BiomeProbe,
    x: i32,
    z: i32,
) -> bool {
    ((ground.min_y() - 1) >> 2..=ground.max_y() >> 2)
        .any(|qy| valid.contains(&probe.at(x >> 2, qy, z >> 2)))
}

pub(crate) fn valid_at(valid: &[u8], probe: &impl BiomeProbe, at: Pos) -> bool {
    valid.contains(&probe.at(at.0 >> 2, at.1 >> 2, at.2 >> 2))
}

pub(crate) fn occupied<G: Ground>(ground: &G, x: i32, z: i32, map: Heightmap) -> i32 {
    ground.first_free_height(x, z, map) - 1
}

pub(crate) fn corner_heights<G: Ground>(
    ground: &G,
    min_x: i32,
    size_x: i32,
    min_z: i32,
    size_z: i32,
) -> [i32; 4] {
    let at = |x, z| occupied(ground, x, z, Heightmap::WorldSurfaceWg);
    [
        at(min_x, min_z),
        at(min_x, min_z + size_z),
        at(min_x + size_x, min_z),
        at(min_x + size_x, min_z + size_z),
    ]
}

pub(crate) fn lowest_y<G: Ground>(
    ground: &G,
    min_x: i32,
    size_x: i32,
    min_z: i32,
    size_z: i32,
) -> i32 {
    corner_heights(ground, min_x, size_x, min_z, size_z)
        .into_iter()
        .min()
        .unwrap_or(0)
}

pub(crate) fn mean_height<G: Ground>(
    ground: &G,
    min_x: i32,
    size_x: i32,
    min_z: i32,
    size_z: i32,
) -> i32 {
    let heights = corner_heights(ground, min_x, size_x, min_z, size_z);
    (heights[0] + heights[1] + heights[2] + heights[3]) / 4
}

pub(crate) fn center_top<G: Ground>(ground: &G, map: Heightmap, chunk: (i32, i32)) -> Pos {
    let (x, z) = (chunk.0 * 16 + 8, chunk.1 * 16 + 8);
    (x, occupied(ground, x, z, map), z)
}

pub(crate) fn on_top<G: Ground>(
    ground: &G,
    valid: &[u8],
    probe: &impl BiomeProbe,
    map: Heightmap,
    chunk: (i32, i32),
) -> Option<Pos> {
    let (x, z) = (chunk.0 * 16 + 8, chunk.1 * 16 + 8);
    if !could_exist(ground, valid, probe, x, z) {
        return None;
    }
    let at = center_top(ground, map, chunk);
    valid_at(valid, probe, at).then_some(at)
}
