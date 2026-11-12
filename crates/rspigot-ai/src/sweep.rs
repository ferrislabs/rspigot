use rspigot_domain::entity::{Aabb, raycast};
use rspigot_domain::{BlockPos, Vec3, mth};
use std::collections::HashSet;

const MIN_MOVE: f32 = 1.0e-5;
const CELL_MARGIN: f64 = 9.999_999_747_378_752e-6;

fn sign(value: f64) -> i32 {
    if value == 0.0 {
        0
    } else if value > 0.0 {
        1
    } else {
        -1
    }
}

fn frac(value: f64) -> f64 {
    value - value.floor()
}

fn cuboid(from: BlockPos, to: BlockPos) -> impl Iterator<Item = BlockPos> {
    let (low_x, high_x) = (from.x.min(to.x), from.x.max(to.x));
    let (low_y, high_y) = (from.y.min(to.y), from.y.max(to.y));
    let (low_z, high_z) = (from.z.min(to.z), from.z.max(to.z));
    (low_x..=high_x).flat_map(move |x| {
        (low_y..=high_y).flat_map(move |y| (low_z..=high_z).map(move |z| BlockPos::new(x, y, z)))
    })
}

fn covered_blocks(area: &Aabb) -> impl Iterator<Item = BlockPos> {
    cuboid(
        BlockPos::new(
            mth::floor(area.min_x),
            mth::floor(area.min_y),
            mth::floor(area.min_z),
        ),
        BlockPos::new(
            mth::floor(area.max_x),
            mth::floor(area.max_y),
            mth::floor(area.max_z),
        ),
    )
}

fn furthest_corner(direction: Vec3) -> (i32, i32, i32) {
    let x = direction.x.abs();
    let y = direction.y.abs();
    let z = direction.z.abs();
    let sign_x = if direction.x >= 0.0 { 1 } else { -1 };
    let sign_y = if direction.y >= 0.0 { 1 } else { -1 };
    let sign_z = if direction.z >= 0.0 { 1 } else { -1 };
    if x <= y && x <= z {
        (-sign_x, -sign_z, sign_y)
    } else if y <= z {
        (sign_z, -sign_y, -sign_x)
    } else {
        (-sign_y, sign_x, -sign_z)
    }
}

fn per_unit(delta: f64, step: i32) -> f64 {
    if step == 0 {
        f64::MAX
    } else {
        f64::from(step) / delta
    }
}

fn first_step(delta: f64, step: i32, at: f64) -> f64 {
    per_unit(delta, step) * if step > 0 { 1.0 - frac(at) } else { frac(at) }
}

fn travel(
    visited: &mut HashSet<BlockPos>,
    delta: Vec3,
    area: &Aabb,
    visit: &mut impl FnMut(BlockPos) -> bool,
) -> Option<()> {
    let size = Vec3::new(
        area.max_x - area.min_x,
        area.max_y - area.min_y,
        area.max_z - area.min_z,
    );
    let corner = furthest_corner(delta);
    let center = area.center();
    let end = Vec3::new(
        center.x + size.x * 0.5 * f64::from(corner.0),
        center.y + size.y * 0.5 * f64::from(corner.1),
        center.z + size.z * 0.5 * f64::from(corner.2),
    );
    let start = end - delta;
    let mut cell = BlockPos::new(
        mth::floor(start.x),
        mth::floor(start.y),
        mth::floor(start.z),
    );
    let (step_x, step_y, step_z) = (sign(delta.x), sign(delta.y), sign(delta.z));
    let (unit_x, unit_y, unit_z) = (
        per_unit(delta.x, step_x),
        per_unit(delta.y, step_y),
        per_unit(delta.z, step_z),
    );
    let mut next_x = first_step(delta.x, step_x, start.x);
    let mut next_y = first_step(delta.y, step_y, start.y);
    let mut next_z = first_step(delta.z, step_z, start.z);
    while next_x <= 1.0 || next_y <= 1.0 || next_z <= 1.0 {
        if next_x < next_y {
            if next_x < next_z {
                cell = cell.offset(step_x, 0, 0);
                next_x += unit_x;
            } else {
                cell = cell.offset(0, 0, step_z);
                next_z += unit_z;
            }
        } else if next_y < next_z {
            cell = cell.offset(0, step_y, 0);
            next_y += unit_y;
        } else {
            cell = cell.offset(0, 0, step_z);
            next_z += unit_z;
        }
        let cube = Aabb::new(
            f64::from(cell.x),
            f64::from(cell.y),
            f64::from(cell.z),
            f64::from(cell.x + 1),
            f64::from(cell.y + 1),
            f64::from(cell.z + 1),
        );
        let Some(hit) = raycast::clip_box(&cube, start, end) else {
            continue;
        };
        let clamp = |value: f64, base: i32| {
            value.clamp(
                f64::from(base) + CELL_MARGIN,
                f64::from(base) + 1.0 - CELL_MARGIN,
            )
        };
        let (x, y, z) = (
            clamp(hit.x, cell.x),
            clamp(hit.y, cell.y),
            clamp(hit.z, cell.z),
        );
        let far = BlockPos::new(
            mth::floor(x - size.x * f64::from(corner.0)),
            mth::floor(y - size.y * f64::from(corner.1)),
            mth::floor(z - size.z * f64::from(corner.2)),
        );
        for pos in cuboid(cell, far) {
            if visited.insert(pos) && !visit(pos) {
                return None;
            }
        }
    }
    Some(())
}

pub(crate) fn all_blocks_intersected_between(
    from: Vec3,
    to: Vec3,
    area: &Aabb,
    mut visit: impl FnMut(BlockPos) -> bool,
) -> bool {
    let delta = to - from;
    let min = f64::from(MIN_MOVE * MIN_MOVE);
    if delta.length_sqr() < min {
        return covered_blocks(area).all(visit);
    }
    let mut visited = HashSet::default();
    let start = area.move_by(delta.scale(-1.0));
    for pos in covered_blocks(&start) {
        if !visit(pos) {
            return false;
        }
        visited.insert(pos);
    }
    if travel(&mut visited, delta, area, &mut visit).is_none() {
        return false;
    }
    for pos in covered_blocks(area) {
        if visited.insert(pos) && !visit(pos) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visited(from: Vec3, to: Vec3, size: f64) -> Vec<BlockPos> {
        let half = size / 2.0;
        let end_box = Aabb::new(
            to.x - half,
            to.y,
            to.z - half,
            to.x + half,
            to.y + size,
            to.z + half,
        );
        let mut seen = Vec::new();
        assert!(all_blocks_intersected_between(from, to, &end_box, |pos| {
            seen.push(pos);
            true
        }));
        seen
    }

    #[test]
    fn a_standing_box_visits_the_blocks_it_covers_once() {
        let at = Vec3::new(0.5, 0.0, 0.5);
        let seen = visited(at, at, 0.6);
        assert_eq!(seen, vec![BlockPos::new(0, 0, 0)]);
        let wide = visited(Vec3::new(1.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 1.0), 1.0);
        assert_eq!(wide.len(), 8);
    }

    #[test]
    fn a_box_moving_along_x_visits_the_corridor_it_sweeps() {
        let seen = visited(Vec3::new(0.5, 0.0, 0.5), Vec3::new(3.5, 0.0, 0.5), 0.9);
        let mut cells: Vec<i32> = seen.iter().map(|pos| pos.x).collect();
        cells.sort_unstable();
        cells.dedup();
        assert_eq!(cells, vec![0, 1, 2, 3]);
        assert!(seen.iter().all(|pos| pos.y == 0 && pos.z == 0));
    }

    #[test]
    fn a_diagonal_move_covers_every_block_the_box_touches() {
        let seen = visited(Vec3::new(0.5, 0.0, 0.5), Vec3::new(2.5, 0.0, 2.5), 0.9);
        for corner in [(0, 0), (1, 1), (2, 2)] {
            assert!(seen.contains(&BlockPos::new(corner.0, 0, corner.1)));
        }
        let unique: HashSet<BlockPos> = seen.iter().copied().collect();
        assert_eq!(unique.len(), seen.len());
    }

    #[test]
    fn a_refusing_visitor_stops_the_sweep() {
        let end = Aabb::new(3.0, 0.0, 0.0, 4.0, 1.0, 1.0);
        let mut calls = 0;
        let result = all_blocks_intersected_between(
            Vec3::new(0.5, 0.0, 0.5),
            Vec3::new(3.5, 0.0, 0.5),
            &end,
            |pos| {
                calls += 1;
                pos.x < 2
            },
        );
        assert!(!result);
        assert!(calls > 8);
    }

    #[test]
    fn the_furthest_corner_follows_the_dominant_axis() {
        assert_eq!(furthest_corner(Vec3::new(1.0, 0.0, 0.0)), (1, -1, -1));
        assert_eq!(furthest_corner(Vec3::new(0.0, 5.0, 1.0)), (-1, -1, 1));
        assert_eq!(furthest_corner(Vec3::new(-3.0, -5.0, -1.0)), (1, -1, 1));
    }
}
