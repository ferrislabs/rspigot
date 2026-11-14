use rspigot_blocks::Aabb;
use rspigot_domain::{BlockPos, EntityId, Vec3, mth};

use crate::BlockView;
use crate::world::{Living, MobView};

const MAX_SIGHT_DISTANCE: f64 = 128.0;
const CLIP_EPSILON: f64 = 1.0e-7;
const INSIDE_PROBE: f64 = 0.001;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Sensing {
    seen: Vec<EntityId>,
    unseen: Vec<EntityId>,
}

impl Sensing {
    pub fn tick(&mut self) {
        self.seen.clear();
        self.unseen.clear();
    }

    pub fn has_line_of_sight(
        &mut self,
        view: &impl BlockView,
        mob: &MobView,
        target: &Living,
    ) -> bool {
        if mob.species.raider.blind {
            return false;
        }
        if self.seen.contains(&target.id) {
            return true;
        }
        if self.unseen.contains(&target.id) {
            return false;
        }
        let from = mob.eye_position();
        let to = Vec3::new(target.position.x, target.eye_y(), target.position.z);
        let visible = has_line_of_sight_between(view, from, to);
        if visible {
            self.seen.push(target.id);
        } else {
            self.unseen.push(target.id);
        }
        visible
    }
}

pub(crate) fn has_line_of_sight_between(view: &impl BlockView, from: Vec3, to: Vec3) -> bool {
    from.distance_squared(to).sqrt() <= MAX_SIGHT_DISTANCE && clip_misses(view, from, to)
}

fn shape_hit(boxes: &[Aabb], from: Vec3, to: Vec3, pos: BlockPos) -> bool {
    if boxes.is_empty() {
        return false;
    }
    let delta = to - from;
    if delta.length_sqr() < CLIP_EPSILON {
        return false;
    }
    let (ox, oy, oz) = (f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
    let probe = from + delta.scale(INSIDE_PROBE);
    let (px, py, pz) = (probe.x - ox, probe.y - oy, probe.z - oz);
    let inside = boxes.iter().any(|b| {
        px >= b.min_x
            && px < b.max_x
            && py >= b.min_y
            && py < b.max_y
            && pz >= b.min_z
            && pz < b.max_z
    });
    if inside {
        return true;
    }
    let mut nearest = 1.0;
    let mut hit = false;
    for b in boxes {
        let moved = Aabb::new(
            b.min_x + ox,
            b.min_y + oy,
            b.min_z + oz,
            b.max_x + ox,
            b.max_y + oy,
            b.max_z + oz,
        );
        hit |= clip_box(&moved, from, delta, &mut nearest);
    }
    hit
}

#[allow(clippy::too_many_arguments)]
fn clip_point(
    nearest: &mut f64,
    da: f64,
    db: f64,
    dc: f64,
    plane: f64,
    min_b: f64,
    max_b: f64,
    min_c: f64,
    max_c: f64,
    origin: (f64, f64, f64),
) -> bool {
    let t = (plane - origin.0) / da;
    let b = origin.1 + t * db;
    let c = origin.2 + t * dc;
    if 0.0 < t
        && t < *nearest
        && min_b - CLIP_EPSILON < b
        && b < max_b + CLIP_EPSILON
        && min_c - CLIP_EPSILON < c
        && c < max_c + CLIP_EPSILON
    {
        *nearest = t;
        return true;
    }
    false
}

fn clip_box(b: &Aabb, from: Vec3, delta: Vec3, nearest: &mut f64) -> bool {
    let (dx, dy, dz) = (delta.x, delta.y, delta.z);
    let mut hit = false;
    let x_origin = (from.x, from.y, from.z);
    if dx > CLIP_EPSILON {
        hit |= clip_point(
            nearest, dx, dy, dz, b.min_x, b.min_y, b.max_y, b.min_z, b.max_z, x_origin,
        );
    } else if dx < -CLIP_EPSILON {
        hit |= clip_point(
            nearest, dx, dy, dz, b.max_x, b.min_y, b.max_y, b.min_z, b.max_z, x_origin,
        );
    }
    let y_origin = (from.y, from.z, from.x);
    if dy > CLIP_EPSILON {
        hit |= clip_point(
            nearest, dy, dz, dx, b.min_y, b.min_z, b.max_z, b.min_x, b.max_x, y_origin,
        );
    } else if dy < -CLIP_EPSILON {
        hit |= clip_point(
            nearest, dy, dz, dx, b.max_y, b.min_z, b.max_z, b.min_x, b.max_x, y_origin,
        );
    }
    let z_origin = (from.z, from.x, from.y);
    if dz > CLIP_EPSILON {
        hit |= clip_point(
            nearest, dz, dx, dy, b.min_z, b.min_x, b.max_x, b.min_y, b.max_y, z_origin,
        );
    } else if dz < -CLIP_EPSILON {
        hit |= clip_point(
            nearest, dz, dx, dy, b.max_z, b.min_x, b.max_x, b.min_y, b.max_y, z_origin,
        );
    }
    hit
}

fn block_hit(view: &impl BlockView, from: Vec3, to: Vec3, pos: BlockPos) -> bool {
    let boxes = view
        .state(pos)
        .map_or(&[][..], |state| state.collision_shape());
    shape_hit(boxes, from, to, pos)
}

pub fn clip_misses(view: &impl BlockView, from: Vec3, to: Vec3) -> bool {
    if from == to {
        return true;
    }
    let end = Vec3::new(
        mth::lerp(-CLIP_EPSILON, to.x, from.x),
        mth::lerp(-CLIP_EPSILON, to.y, from.y),
        mth::lerp(-CLIP_EPSILON, to.z, from.z),
    );
    let start = Vec3::new(
        mth::lerp(-CLIP_EPSILON, from.x, to.x),
        mth::lerp(-CLIP_EPSILON, from.y, to.y),
        mth::lerp(-CLIP_EPSILON, from.z, to.z),
    );
    let mut x = mth::floor(start.x);
    let mut y = mth::floor(start.y);
    let mut z = mth::floor(start.z);
    if block_hit(view, from, to, BlockPos::new(x, y, z)) {
        return false;
    }
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let dz = end.z - start.z;
    let sx = mth::sign(dx);
    let sy = mth::sign(dy);
    let sz = mth::sign(dz);
    let step = |sign: i32, delta: f64| {
        if sign == 0 {
            f64::MAX
        } else {
            f64::from(sign) / delta
        }
    };
    let tx_step = step(sx, dx);
    let ty_step = step(sy, dy);
    let tz_step = step(sz, dz);
    let first = |step: f64, sign: i32, coordinate: f64| {
        step * if sign > 0 {
            1.0 - mth::frac(coordinate)
        } else {
            mth::frac(coordinate)
        }
    };
    let mut tx = first(tx_step, sx, start.x);
    let mut ty = first(ty_step, sy, start.y);
    let mut tz = first(tz_step, sz, start.z);
    while tx <= 1.0 || ty <= 1.0 || tz <= 1.0 {
        if tx < ty {
            if tx < tz {
                x += sx;
                tx += tx_step;
            } else {
                z += sz;
                tz += tz_step;
            }
        } else if ty < tz {
            y += sy;
            ty += ty_step;
        } else {
            z += sz;
            tz += tz_step;
        }
        if block_hit(view, from, to, BlockPos::new(x, y, z)) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rspigot_blocks::{BlockKind, StateId};

    use super::*;

    struct Walls(HashMap<BlockPos, StateId>);

    impl BlockView for Walls {
        fn state(&self, pos: BlockPos) -> Option<StateId> {
            Some(self.0.get(&pos).copied().unwrap_or(StateId::AIR))
        }
    }

    #[test]
    fn walls_block_and_glass_panes_follow_their_shape() {
        let mut blocks = HashMap::new();
        blocks.insert(BlockPos::new(2, 65, 0), BlockKind::Stone.default_state());
        let world = Walls(blocks);
        let from = Vec3::new(0.5, 65.5, 0.5);
        assert!(!clip_misses(&world, from, Vec3::new(4.5, 65.5, 0.5)));
        assert!(clip_misses(&world, from, Vec3::new(0.5, 65.5, 4.5)));
        assert!(clip_misses(&world, from, Vec3::new(4.5, 67.5, 4.5)));
        assert!(clip_misses(&world, from, from));
        let mut blocks = HashMap::new();
        blocks.insert(BlockPos::new(2, 65, 0), BlockKind::OakSlab.default_state());
        let slab = Walls(blocks);
        assert!(clip_misses(
            &slab,
            Vec3::new(0.5, 65.8, 0.5),
            Vec3::new(4.5, 65.8, 0.5)
        ));
        assert!(!clip_misses(
            &slab,
            Vec3::new(0.5, 65.2, 0.5),
            Vec3::new(4.5, 65.2, 0.5)
        ));
    }
}
