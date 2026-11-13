use std::sync::OnceLock;

use rspigot_blocks::{BlockKind, SHAPE_COUNT, SHAPES, ShapeId, StateFlags};

use super::aabb::{Aabb, Axis};
use super::base::EntityBase;
use crate::{BlockPos, BlockStateId, Vec3};

pub const COLLISION_EPSILON: f64 = 1.0e-7;
const MOVE_EPSILON: f64 = 1.0e-7;
const FLOAT_EPSILON: f64 = 9.999_999_747_378_752e-6;
const STEP_DOWN_PROBE: f64 = -9.999_999_747_378_752e-6;
const SUPPORT_PROBE: f64 = 1.0e-6;
const MAX_FALL_CLIP: f64 = 8.0;
const LEGACY_ON_POS: f32 = 0.2;
const MOVEMENT_ON_POS: f32 = 0.500_001;
const ON_POS: f32 = 1.0e-5;
const NON_LIVING_BOUNCE: f32 = 0.8;
const BLOCK_TAGS: &str = "minecraft:block";

pub trait BlockView {
    fn block(&self, pos: BlockPos) -> Option<BlockStateId>;

    fn block_or_air(&self, pos: BlockPos) -> BlockStateId {
        self.block(pos).unwrap_or(BlockStateId::AIR)
    }
}

pub fn has_tag(state: BlockStateId, tag: &str) -> bool {
    state.kind().is_some_and(|kind| {
        rspigot_data::tags::tag(BLOCK_TAGS, tag)
            .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
    })
}

pub fn mth_floor(value: f64) -> i32 {
    value.floor() as i32
}

pub fn mth_equal(a: f64, b: f64) -> bool {
    (b - a).abs() < FLOAT_EPSILON
}

struct Grid {
    boxes: &'static [rspigot_blocks::Aabb],
    coords: [Vec<f64>; 3],
    full: Vec<bool>,
    large: bool,
}

impl Grid {
    fn build(boxes: &'static [rspigot_blocks::Aabb]) -> Self {
        let axis = |pick: fn(&rspigot_blocks::Aabb) -> (f64, f64)| {
            let mut values: Vec<f64> = boxes
                .iter()
                .flat_map(|b| {
                    let (a, c) = pick(b);
                    [a, c]
                })
                .collect();
            values.sort_by(f64::total_cmp);
            values.dedup();
            values
        };
        let coords = [
            axis(|b| (b.min_x, b.max_x)),
            axis(|b| (b.min_y, b.max_y)),
            axis(|b| (b.min_z, b.max_z)),
        ];
        let sizes = coords.each_ref().map(|c| c.len().saturating_sub(1));
        let mut full = vec![false; sizes[0] * sizes[1] * sizes[2]];
        for x in 0..sizes[0] {
            for y in 0..sizes[1] {
                for z in 0..sizes[2] {
                    let cx = (coords[0][x] + coords[0][x + 1]) / 2.0;
                    let cy = (coords[1][y] + coords[1][y + 1]) / 2.0;
                    let cz = (coords[2][z] + coords[2][z + 1]) / 2.0;
                    full[(x * sizes[1] + y) * sizes[2] + z] = boxes.iter().any(|b| {
                        cx > b.min_x
                            && cx < b.max_x
                            && cy > b.min_y
                            && cy < b.max_y
                            && cz > b.min_z
                            && cz < b.max_z
                    });
                }
            }
        }
        let large = boxes.iter().any(|b| {
            b.min_x < 0.0
                || b.min_y < 0.0
                || b.min_z < 0.0
                || b.max_x > 1.0
                || b.max_y > 1.0
                || b.max_z > 1.0
        });
        Self {
            boxes,
            coords,
            full,
            large,
        }
    }

    fn size(&self, axis: Axis) -> usize {
        self.coords[axis.index()].len().saturating_sub(1)
    }
}

fn grids() -> &'static [Grid] {
    static GRIDS: OnceLock<Vec<Grid>> = OnceLock::new();
    GRIDS.get_or_init(|| (0..SHAPE_COUNT).map(|i| Grid::build(SHAPES[i])).collect())
}

fn grid(shape: ShapeId) -> &'static Grid {
    &grids()[usize::from(shape.0).min(SHAPE_COUNT - 1)]
}

pub fn has_large_collision_shape(state: BlockStateId) -> bool {
    grid(state.collision_shape_id()).large
}

#[derive(Clone, Copy)]
pub struct Collider {
    grid: &'static Grid,
    offset: [f64; 3],
    pub pos: BlockPos,
}

impl Collider {
    fn new(shape: ShapeId, pos: BlockPos) -> Self {
        Self {
            grid: grid(shape),
            offset: [f64::from(pos.x), f64::from(pos.y), f64::from(pos.z)],
            pos,
        }
    }

    fn get(&self, axis: Axis, index: usize) -> f64 {
        self.grid.coords[axis.index()][index] + self.offset[axis.index()]
    }

    fn find_index(&self, axis: Axis, coord: f64) -> isize {
        let mut low = 0usize;
        let mut count = self.grid.size(axis) + 1;
        while count > 0 {
            let half = count / 2;
            let middle = low + half;
            if coord < self.get(axis, middle) {
                count = half;
            } else {
                low = middle + 1;
                count -= half + 1;
            }
        }
        low as isize - 1
    }

    fn is_full(&self, axis: Axis, a: usize, b: usize, c: usize) -> bool {
        let (b_axis, c_axis) = axis.others();
        let mut index = [0usize; 3];
        index[axis.index()] = a;
        index[b_axis.index()] = b;
        index[c_axis.index()] = c;
        let sy = self.grid.size(Axis::Y);
        let sz = self.grid.size(Axis::Z);
        if index[0] >= self.grid.size(Axis::X) || index[1] >= sy || index[2] >= sz {
            return false;
        }
        self.grid.full[(index[0] * sy + index[1]) * sz + index[2]]
    }

    pub fn is_empty(&self) -> bool {
        self.grid.boxes.is_empty()
    }

    pub fn collide(&self, axis: Axis, aabb: &Aabb, distance: f64) -> f64 {
        if self.is_empty() {
            return distance;
        }
        if distance.abs() < COLLISION_EPSILON {
            return 0.0;
        }
        let (b_axis, c_axis) = axis.others();
        let max_a = aabb.max(axis);
        let min_a = aabb.min(axis);
        let a_low = self.find_index(axis, min_a + COLLISION_EPSILON);
        let a_high = self.find_index(axis, max_a - COLLISION_EPSILON);
        let range = |other: Axis| {
            let start = self
                .find_index(other, aabb.min(other) + COLLISION_EPSILON)
                .max(0) as usize;
            let end = ((self.find_index(other, aabb.max(other) - COLLISION_EPSILON) + 1).max(0)
                as usize)
                .min(self.grid.size(other));
            start..end
        };
        let (b_range, c_range) = (range(b_axis), range(c_axis));
        let any_full = |a: usize| {
            b_range
                .clone()
                .any(|b| c_range.clone().any(|c| self.is_full(axis, a, b, c)))
        };
        let size = self.grid.size(axis);
        if distance > 0.0 {
            let start = (a_high + 1).max(0) as usize;
            for a in start..size {
                if any_full(a) {
                    let gap = self.get(axis, a) - max_a;
                    let mut distance = distance;
                    if gap >= -COLLISION_EPSILON {
                        distance = distance.min(gap);
                    }
                    return distance;
                }
            }
        } else if distance < 0.0 {
            let mut a = a_low - 1;
            while a >= 0 {
                let index = a as usize;
                if any_full(index) {
                    let gap = self.get(axis, index + 1) - min_a;
                    let mut distance = distance;
                    if gap <= COLLISION_EPSILON {
                        distance = distance.max(gap);
                    }
                    return distance;
                }
                a -= 1;
            }
        }
        distance
    }

    pub fn y_coords(&self) -> impl Iterator<Item = f64> + '_ {
        (0..=self.grid.size(Axis::Y)).map(|i| self.get(Axis::Y, i))
    }

    fn overlaps(&self, aabb: &Aabb) -> bool {
        self.grid.boxes.iter().any(|b| {
            let overlap = |min: f64, max: f64, axis: Axis| {
                let off = self.offset[axis.index()];
                (max + off).min(aabb.max(axis)) - (min + off).max(aabb.min(axis))
                    > COLLISION_EPSILON
            };
            overlap(b.min_x, b.max_x, Axis::X)
                && overlap(b.min_y, b.max_y, Axis::Y)
                && overlap(b.min_z, b.max_z, Axis::Z)
        })
    }
}

pub fn block_colliders<V: BlockView>(view: &V, query: &Aabb) -> Vec<Collider> {
    let bounds = |min: f64, max: f64| {
        (
            mth_floor(min - COLLISION_EPSILON) - 1,
            mth_floor(max + COLLISION_EPSILON) + 1,
        )
    };
    let (x0, x1) = bounds(query.min_x, query.max_x);
    let (y0, y1) = bounds(query.min_y, query.max_y);
    let (z0, z1) = bounds(query.min_z, query.max_z);
    let mut out = Vec::new();
    for y in y0..=y1 {
        for z in z0..=z1 {
            for x in x0..=x1 {
                let edges = u8::from(x == x0 || x == x1)
                    + u8::from(y == y0 || y == y1)
                    + u8::from(z == z0 || z == z1);
                if edges == 3 {
                    continue;
                }
                let pos = BlockPos::new(x, y, z);
                let Some(state) = view.block(pos) else {
                    continue;
                };
                let shape = state.collision_shape_id();
                if shape == ShapeId::EMPTY {
                    continue;
                }
                if edges == 1 && !grid(shape).large {
                    continue;
                }
                if edges == 2 && state.kind() != Some(BlockKind::MovingPiston) {
                    continue;
                }
                let collider = Collider::new(shape, pos);
                let keep = if shape == ShapeId::FULL {
                    query.intersects(&Aabb::block(pos))
                } else {
                    collider.overlaps(query)
                };
                if keep {
                    out.push(collider);
                }
            }
        }
    }
    out
}

pub fn shapes_collide(axis: Axis, aabb: &Aabb, colliders: &[Collider], distance: f64) -> f64 {
    let mut distance = distance;
    for collider in colliders {
        if distance.abs() < COLLISION_EPSILON {
            return 0.0;
        }
        distance = collider.collide(axis, aabb, distance);
    }
    distance
}

pub fn collide_with_shapes(movement: Vec3, aabb: &Aabb, colliders: &[Collider]) -> Vec3 {
    if colliders.is_empty() {
        return movement;
    }
    let mut result = Vec3::ZERO;
    for axis in Axis::step_order(movement) {
        let wanted = axis.of(movement);
        if wanted != 0.0 {
            let moved = aabb.move_by(result);
            let allowed = shapes_collide(axis, &moved, colliders, wanted);
            result = axis.with(result, allowed);
        }
    }
    result
}

pub fn collide_bounding_box<V: BlockView>(view: &V, movement: Vec3, aabb: &Aabb) -> Vec3 {
    let colliders = block_colliders(view, &aabb.expand_towards(movement));
    collide_with_shapes(movement, aabb, &colliders)
}

fn step_up_heights(aabb: &Aabb, colliders: &[Collider], max_step: f32, skip: f32) -> Vec<f32> {
    let mut heights: Vec<f32> = Vec::with_capacity(4);
    for collider in colliders {
        for y in collider.y_coords() {
            let height = (y - aabb.min_y) as f32;
            if height < 0.0 || height == skip {
                continue;
            }
            if height > max_step {
                break;
            }
            if !heights.contains(&height) {
                heights.push(height);
            }
        }
    }
    heights.sort_by(f32::total_cmp);
    heights
}

pub fn collide<V: BlockView>(
    view: &V,
    aabb: &Aabb,
    movement: Vec3,
    max_up_step: f32,
    on_ground: bool,
) -> Vec3 {
    let collided = if movement.length_sqr() == 0.0 {
        movement
    } else {
        collide_bounding_box(view, movement, aabb)
    };
    let x_hit = movement.x != collided.x;
    let y_hit = movement.y != collided.y;
    let z_hit = movement.z != collided.z;
    let landing = y_hit && movement.y < 0.0;
    if max_up_step > 0.0 && (landing || on_ground) && (x_hit || z_hit) {
        let from = if landing {
            aabb.move_by(Vec3::new(0.0, collided.y, 0.0))
        } else {
            *aabb
        };
        let mut query =
            from.expand_towards(Vec3::new(movement.x, f64::from(max_up_step), movement.z));
        if !landing {
            query = query.expand_towards(Vec3::new(0.0, STEP_DOWN_PROBE, 0.0));
        }
        let colliders = block_colliders(view, &query);
        let skip = collided.y as f32;
        for height in step_up_heights(&from, &colliders, max_up_step, skip) {
            let attempt = collide_with_shapes(
                Vec3::new(movement.x, f64::from(height), movement.z),
                &from,
                &colliders,
            );
            if attempt.horizontal_distance_sqr() > collided.horizontal_distance_sqr() {
                let lowered = aabb.min_y - from.min_y;
                return Vec3::new(attempt.x, attempt.y - lowered, attempt.z);
            }
        }
    }
    collided
}

pub fn find_supporting_block<V: BlockView>(
    view: &V,
    position: Vec3,
    aabb: &Aabb,
) -> Option<BlockPos> {
    let mut best: Option<(BlockPos, f64)> = None;
    for collider in block_colliders(view, aabb) {
        let pos = collider.pos;
        let dx = f64::from(pos.x) + 0.5 - position.x;
        let dy = f64::from(pos.y) + 0.5 - position.y;
        let dz = f64::from(pos.z) + 0.5 - position.z;
        let distance = dx * dx + dy * dy + dz * dz;
        let better = match best {
            None => true,
            Some((current, current_distance)) => {
                distance < current_distance
                    || (distance == current_distance && compare_block_pos(current, pos).is_lt())
            }
        };
        if better {
            best = Some((pos, distance));
        }
    }
    best.map(|(pos, _)| pos)
}

fn compare_block_pos(a: BlockPos, b: BlockPos) -> std::cmp::Ordering {
    a.y.cmp(&b.y)
        .then_with(|| a.z.cmp(&b.z))
        .then_with(|| a.x.cmp(&b.x))
}

pub fn water_height(view: &impl BlockView, pos: BlockPos) -> Option<f64> {
    let state = view.block_or_air(pos);
    if !state.flags().contains(StateFlags::WATER) {
        return None;
    }
    let above = view.block_or_air(pos.offset(0, 1, 0));
    if above.flags().contains(StateFlags::WATER) {
        return Some(1.0);
    }
    let amount = match state.kind() {
        Some(BlockKind::Water) => {
            let level = state
                .property("level")
                .and_then(|l| l.parse::<i32>().ok())
                .unwrap_or(0);
            if level >= 8 { 8 } else { 8 - level }
        }
        _ => 8,
    };
    Some(f64::from(amount as f32 / 9.0))
}

pub fn touching_water<V: BlockView>(view: &V, aabb: &Aabb) -> bool {
    let bb = aabb.deflate(0.001);
    let (x0, x1) = (mth_floor(bb.min_x), bb.max_x.ceil() as i32);
    let (y0, y1) = (mth_floor(bb.min_y), bb.max_y.ceil() as i32);
    let (z0, z1) = (mth_floor(bb.min_z), bb.max_z.ceil() as i32);
    (x0..x1).any(|x| {
        (y0..y1).any(|y| {
            (z0..z1).any(|z| {
                water_height(view, BlockPos::new(x, y, z))
                    .is_some_and(|height| f64::from(y) + height >= bb.min_y)
            })
        })
    })
}

fn clip_resets_fall<V: BlockView>(view: &V, from: Vec3, to: Vec3) -> bool {
    let delta = to - from;
    let steps = (delta.length() * 16.0).ceil().max(1.0) as i32;
    let mut last = None;
    for i in 0..=steps {
        let point = from + delta.scale(f64::from(i) / f64::from(steps));
        let pos = point.block();
        if last == Some(pos) {
            continue;
        }
        last = Some(pos);
        let state = view.block_or_air(pos);
        if has_tag(state, "minecraft:fall_damage_resetting") {
            return true;
        }
        if water_height(view, pos).is_some_and(|height| point.y <= f64::from(pos.y) + height) {
            return true;
        }
    }
    false
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoveTraits {
    pub max_up_step: f32,
    pub living: bool,
    pub bounciness: f64,
    pub suppress_bounce: bool,
    pub gravity: f64,
    pub air_drag: f32,
    pub movement_efficiency: Option<f32>,
}

impl MoveTraits {
    pub const fn plain(gravity: f64) -> Self {
        Self {
            max_up_step: 0.0,
            living: false,
            bounciness: 0.0,
            suppress_bounce: false,
            gravity,
            air_drag: 0.98,
            movement_efficiency: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Landing {
    pub distance: f64,
    pub state: BlockStateId,
    pub pos: BlockPos,
}

pub fn on_pos(entity: &EntityBase, view: &impl BlockView, offset: f32) -> BlockPos {
    let position = entity.position;
    let fallback = || {
        BlockPos::new(
            mth_floor(position.x),
            mth_floor(position.y - f64::from(offset)),
            mth_floor(position.z),
        )
    };
    let Some(pos) = entity.supporting_block else {
        return fallback();
    };
    if offset > ON_POS {
        let state = view.block_or_air(pos);
        if (offset > 0.5 || !has_tag(state, "minecraft:fences"))
            && !has_tag(state, "minecraft:walls")
            && !has_tag(state, "minecraft:fence_gates")
        {
            return BlockPos::new(pos.x, mth_floor(position.y - f64::from(offset)), pos.z);
        }
    }
    pos
}

pub fn on_pos_legacy(entity: &EntityBase, view: &impl BlockView) -> BlockPos {
    on_pos(entity, view, LEGACY_ON_POS)
}

pub fn movement_affecting_pos(entity: &EntityBase, view: &impl BlockView) -> BlockPos {
    on_pos(entity, view, MOVEMENT_ON_POS)
}

pub fn ground_pos(entity: &EntityBase, view: &impl BlockView) -> BlockPos {
    on_pos(entity, view, ON_POS)
}

pub fn friction(state: BlockStateId) -> f32 {
    state.kind().map_or(0.6, |kind| kind.settings().friction)
}

pub fn block_speed_factor(entity: &EntityBase, view: &impl BlockView) -> f32 {
    let at = view.block_or_air(entity.position.block());
    let factor = at.kind().map_or(1.0, |kind| kind.settings().speed_factor);
    if matches!(at.kind(), Some(BlockKind::Water | BlockKind::BubbleColumn)) {
        return factor;
    }
    if f64::from(factor) == 1.0 {
        let below = view.block_or_air(movement_affecting_pos(entity, view));
        below
            .kind()
            .map_or(1.0, |kind| kind.settings().speed_factor)
    } else {
        factor
    }
}

pub fn block_jump_factor(entity: &EntityBase, view: &impl BlockView) -> f32 {
    let at = view
        .block_or_air(entity.position.block())
        .kind()
        .map_or(1.0, |kind| kind.settings().jump_factor);
    let below = view
        .block_or_air(movement_affecting_pos(entity, view))
        .kind()
        .map_or(1.0, |kind| kind.settings().jump_factor);
    if f64::from(at) == 1.0 { below } else { at }
}

fn check_supporting_block<V: BlockView>(entity: &mut EntityBase, view: &V, movement: Vec3) {
    if !entity.on_ground {
        entity.on_ground_no_blocks = false;
        entity.supporting_block = None;
        return;
    }
    let bb = entity.bounding_box();
    let probe = Aabb::new(
        bb.min_x,
        bb.min_y - SUPPORT_PROBE,
        bb.min_z,
        bb.max_x,
        bb.min_y,
        bb.max_z,
    );
    let found = find_supporting_block(view, entity.position, &probe);
    let found = if found.is_some() || entity.on_ground_no_blocks {
        found
    } else {
        let back = probe.move_by(Vec3::new(-movement.x, 0.0, -movement.z));
        find_supporting_block(view, entity.position, &back)
    };
    entity.supporting_block = found;
    entity.on_ground_no_blocks = found.is_none();
}

fn block_bounciness(state: BlockStateId, living: bool) -> f64 {
    let restitution = state
        .kind()
        .map_or(0.0, |kind| kind.settings().bounce_restitution);
    if living {
        f64::from(restitution)
    } else {
        f64::from(restitution * NON_LIVING_BOUNCE)
    }
}

fn restitute(
    entity: &mut EntityBase,
    on_state: BlockStateId,
    x_hit: bool,
    z_hit: bool,
    collided: Vec3,
    traits: &MoveTraits,
) {
    let mut bounciness = if traits.suppress_bounce {
        0.0
    } else {
        traits.bounciness
    };
    let delta = entity.velocity;
    let mut result = delta;
    if x_hit {
        result.x = -delta.x * bounciness;
    }
    if z_hit {
        result.z = -delta.z * bounciness;
    }
    let mut bounced = bounciness > 0.0 && (x_hit || z_hit);
    if entity.vertical_collision {
        if entity.vertical_collision_below {
            bounciness = if -delta.y > traits.gravity
                && !traits.suppress_bounce
                && !has_tag(on_state, "minecraft:suppresses_bounce")
            {
                bounciness.max(block_bounciness(on_state, traits.living))
            } else {
                0.0
            };
        }
        let (gravity, drag) = if bounciness > 0.0 {
            let ratio = collided.y / delta.y;
            bounced = true;
            (
                ratio * traits.gravity,
                1.0 + ratio * (f64::from(traits.air_drag) - 1.0),
            )
        } else {
            (0.0, 1.0)
        };
        result.y = (gravity - delta.y) * drag * bounciness;
    }
    if bounced {
        entity.sync_position = true;
    }
    entity.velocity = result;
}

pub fn move_entity<V: BlockView>(
    entity: &mut EntityBase,
    view: &V,
    movement: Vec3,
    traits: &MoveTraits,
) -> Option<Landing> {
    if entity.no_physics {
        entity.position = entity.position + movement;
        entity.horizontal_collision = false;
        entity.vertical_collision = false;
        entity.vertical_collision_below = false;
        entity.minor_horizontal_collision = false;
        return None;
    }
    let mut movement = movement;
    let stuck = entity.stuck_speed_multiplier;
    if stuck.length_sqr() > MOVE_EPSILON {
        movement = movement.multiply(stuck.x, stuck.y, stuck.z);
        entity.stuck_speed_multiplier = Vec3::ZERO;
        entity.velocity = Vec3::ZERO;
    }
    let bb = entity.bounding_box();
    let collided = collide(view, &bb, movement, traits.max_up_step, entity.on_ground);
    let distance = collided.length_sqr();
    if distance > MOVE_EPSILON || movement.length_sqr() - distance < MOVE_EPSILON {
        if entity.fall_distance != 0.0 && distance >= 1.0 {
            let reach = collided.length().min(MAX_FALL_CLIP);
            let to = entity.position + collided.normalize().scale(reach);
            if clip_resets_fall(view, entity.position, to) {
                entity.fall_distance = 0.0;
            }
        }
        entity.position = entity.position + collided;
    }
    let x_hit = !mth_equal(movement.x, collided.x);
    let z_hit = !mth_equal(movement.z, collided.z);
    entity.horizontal_collision = x_hit || z_hit;
    let vertical_move = movement.y.abs() > 0.0;
    entity.vertical_collision = movement.y != collided.y;
    entity.vertical_collision_below = entity.vertical_collision && movement.y < 0.0;
    entity.on_ground = entity.vertical_collision_below;
    check_supporting_block(entity, view, collided);
    entity.minor_horizontal_collision = false;
    let pos = on_pos_legacy(entity, view);
    let state = view.block_or_air(pos);
    let in_water = touching_water(view, &entity.bounding_box());
    if !in_water && collided.y < 0.0 {
        entity.fall_distance -= f64::from(collided.y as f32);
    }
    let mut landing = None;
    if entity.on_ground {
        if entity.fall_distance > 0.0 {
            landing = Some(Landing {
                distance: entity.fall_distance,
                state,
                pos,
            });
        }
        entity.fall_distance = 0.0;
    }
    if entity.removal.is_some() {
        return landing;
    }
    if (vertical_move && entity.vertical_collision) || entity.horizontal_collision {
        restitute(entity, state, x_hit, z_hit, collided, traits);
    }
    let base = block_speed_factor(entity, view);
    let factor = match traits.movement_efficiency {
        Some(efficiency) => base + efficiency * (1.0 - base),
        None => base,
    };
    entity.velocity = entity
        .velocity
        .multiply(f64::from(factor), 1.0, f64::from(factor));
    landing
}

pub fn apply_inside_blocks<V: BlockView>(entity: &mut EntityBase, view: &V, weaving: bool) {
    if entity.removal.is_some() || entity.no_physics {
        return;
    }
    let bb = entity.bounding_box().deflate(1.0e-5);
    let living = entity.kind.living();
    let stuck_immune = entity.kind == rspigot_entities::EntityKind::Wither;
    for y in mth_floor(bb.min_y)..=mth_floor(bb.max_y) {
        for z in mth_floor(bb.min_z)..=mth_floor(bb.max_z) {
            for x in mth_floor(bb.min_x)..=mth_floor(bb.max_x) {
                let pos = BlockPos::new(x, y, z);
                let Some(kind) = view.block(pos).and_then(BlockStateId::kind) else {
                    continue;
                };
                let multiplier = match kind {
                    BlockKind::Cobweb if living && weaving => Vec3::new(0.5, 0.25, 0.5),
                    BlockKind::Cobweb => Vec3::new(0.25, f64::from(0.05f32), 0.25),
                    BlockKind::SweetBerryBush
                        if living
                            && !matches!(
                                entity.kind,
                                rspigot_entities::EntityKind::Fox
                                    | rspigot_entities::EntityKind::Bee
                            ) =>
                    {
                        Vec3::new(f64::from(0.8f32), 0.75, f64::from(0.8f32))
                    }
                    BlockKind::PowderSnow
                        if !living
                            || view.block(entity.position.block())
                                == Some(BlockKind::PowderSnow.default_state()) =>
                    {
                        Vec3::new(f64::from(0.9f32), 1.5, f64::from(0.9f32))
                    }
                    _ => continue,
                };
                if stuck_immune {
                    continue;
                }
                entity.fall_distance = 0.0;
                entity.stuck_speed_multiplier = multiplier;
            }
        }
    }
}
