use rspigot_blocks::Face;

use crate::Vec3;
use crate::entity::Aabb;

pub const STEP_PER_TICK: f32 = 0.5;
pub const ENTITY_MARGIN: f64 = 0.01;
pub const STICKY_TOP: f64 = 1.5000010004;
const SHORT_THRESHOLD: f32 = 0.25;
const AXIS_LIMIT: f64 = 0.51;
const MINIMUM_MOVE_SQR: f64 = 1.0e-7;
const MINIMUM_AXIS_MOVE: f64 = 1.0e-5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Motion {
    pub direction: Face,
    pub extending: bool,
    pub source: bool,
    pub progress: f32,
}

impl Motion {
    pub fn movement_direction(&self) -> Face {
        if self.extending {
            self.direction
        } else {
            self.direction.opposite()
        }
    }

    pub fn extended_progress(&self, progress: f32) -> f32 {
        if self.extending {
            progress - 1.0
        } else {
            1.0 - progress
        }
    }

    pub fn offset(&self) -> Vec3 {
        let (dx, dy, dz) = self.direction.offset();
        let extended = f64::from(self.extended_progress(self.progress));
        Vec3::new(
            f64::from(dx) * extended,
            f64::from(dy) * extended,
            f64::from(dz) * extended,
        )
    }

    pub fn place(&self, block: (i32, i32, i32), aabb: &Aabb) -> Aabb {
        let offset = self.offset();
        aabb.move_by(Vec3::new(
            f64::from(block.0) + offset.x,
            f64::from(block.1) + offset.y,
            f64::from(block.2) + offset.z,
        ))
    }

    pub fn shows_short_head(&self) -> bool {
        self.progress > SHORT_THRESHOLD
    }

    pub fn is_retracting_source(&self) -> bool {
        !self.extending && self.source
    }
}

fn step_of(direction: Face) -> f64 {
    let (dx, dy, dz) = direction.offset();
    f64::from(dx + dy + dz)
}

pub fn movement_area(aabb: &Aabb, direction: Face, amount: f64) -> Aabb {
    let signed = amount * step_of(direction);
    let (low, high) = (signed.min(0.0), signed.max(0.0));
    match direction {
        Face::West => Aabb::new(
            aabb.min_x + low,
            aabb.min_y,
            aabb.min_z,
            aabb.min_x + high,
            aabb.max_y,
            aabb.max_z,
        ),
        Face::East => Aabb::new(
            aabb.max_x + low,
            aabb.min_y,
            aabb.min_z,
            aabb.max_x + high,
            aabb.max_y,
            aabb.max_z,
        ),
        Face::Down => Aabb::new(
            aabb.min_x,
            aabb.min_y + low,
            aabb.min_z,
            aabb.max_x,
            aabb.min_y + high,
            aabb.max_z,
        ),
        Face::Up => Aabb::new(
            aabb.min_x,
            aabb.max_y + low,
            aabb.min_z,
            aabb.max_x,
            aabb.max_y + high,
            aabb.max_z,
        ),
        Face::North => Aabb::new(
            aabb.min_x,
            aabb.min_y,
            aabb.min_z + low,
            aabb.max_x,
            aabb.max_y,
            aabb.min_z + high,
        ),
        Face::South => Aabb::new(
            aabb.min_x,
            aabb.min_y,
            aabb.max_z + low,
            aabb.max_x,
            aabb.max_y,
            aabb.max_z + high,
        ),
    }
}

pub fn movement_needed(outside_of: &Aabb, direction: Face, aabb: &Aabb) -> f64 {
    match direction {
        Face::West => outside_of.max_x - aabb.min_x,
        Face::East => aabb.max_x - outside_of.min_x,
        Face::Down => outside_of.max_y - aabb.min_y,
        Face::Up => aabb.max_y - outside_of.min_y,
        Face::North => outside_of.max_z - aabb.min_z,
        Face::South => aabb.max_z - outside_of.min_z,
    }
}

pub fn union(one: &Aabb, other: &Aabb) -> Aabb {
    Aabb::new(
        one.min_x.min(other.min_x),
        one.min_y.min(other.min_y),
        one.min_z.min(other.min_z),
        one.max_x.max(other.max_x),
        one.max_y.max(other.max_y),
        one.max_z.max(other.max_z),
    )
}

pub fn overlap(one: &Aabb, other: &Aabb) -> Aabb {
    Aabb::new(
        one.min_x.max(other.min_x),
        one.min_y.max(other.min_y),
        one.min_z.max(other.min_z),
        one.max_x.min(other.max_x),
        one.max_y.min(other.max_y),
        one.max_z.min(other.max_z),
    )
}

pub fn search_area(
    motion: &Motion,
    block: (i32, i32, i32),
    shape: &[Aabb],
    delta: f64,
) -> Option<Aabb> {
    let bounds = bounds_of(shape)?;
    let placed = motion.place(block, &bounds);
    let swept = movement_area(&placed, motion.movement_direction(), delta);
    Some(union(&swept, &placed))
}

pub fn bounds_of(shape: &[Aabb]) -> Option<Aabb> {
    shape
        .iter()
        .copied()
        .reduce(|one, other| union(&one, &other))
}

pub fn push_distance(
    motion: &Motion,
    block: (i32, i32, i32),
    shape: &[Aabb],
    delta: f64,
    entity: &Aabb,
) -> f64 {
    let direction = motion.movement_direction();
    let mut needed = 0.0_f64;
    for part in shape {
        let area = movement_area(&motion.place(block, part), direction, delta);
        if area.intersects(entity) {
            needed = needed.max(movement_needed(&area, direction, entity));
            if needed >= delta {
                break;
            }
        }
    }
    needed
}

pub fn pushed_by(
    motion: &Motion,
    block: (i32, i32, i32),
    shape: &[Aabb],
    delta: f64,
    entity: &Aabb,
) -> Option<f64> {
    let needed = push_distance(motion, block, shape, delta, entity);
    (needed > 0.0).then(|| needed.min(delta) + ENTITY_MARGIN)
}

pub fn base_fix(block: (i32, i32, i32), direction: Face, entity: &Aabb, delta: f64) -> Option<f64> {
    let (x, y, z) = (f64::from(block.0), f64::from(block.1), f64::from(block.2));
    let cube = Aabb::new(x, y, z, x + 1.0, y + 1.0, z + 1.0);
    if !entity.intersects(&cube) {
        return None;
    }
    let back = direction.opposite();
    let full = movement_needed(&cube, back, entity) + ENTITY_MARGIN;
    let inside = movement_needed(&cube, back, &overlap(entity, &cube)) + ENTITY_MARGIN;
    ((full - inside).abs() < ENTITY_MARGIN).then(|| full.min(delta) + ENTITY_MARGIN)
}

pub fn sticky_area(motion: &Motion, block: (i32, i32, i32), shape_top: f64) -> Aabb {
    motion.place(block, &Aabb::new(0.0, shape_top, 0.0, 1.0, STICKY_TOP, 1.0))
}

pub fn sticks(area: &Aabb, standing: bool, supported_by_block: bool, x: f64, z: f64) -> bool {
    standing
        && (supported_by_block
            || (area.min_x <= x && x <= area.max_x && area.min_z <= z && z <= area.max_z))
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Restriction {
    pub tick: u64,
    pub moved: [f64; 3],
}

impl Restriction {
    pub fn limit(&mut self, tick: u64, movement: Vec3) -> Vec3 {
        if movement.length_sqr() <= MINIMUM_MOVE_SQR {
            return movement;
        }
        if self.tick != tick {
            self.moved = [0.0; 3];
            self.tick = tick;
        }
        let components = [movement.x, movement.y, movement.z];
        for (axis, value) in components.into_iter().enumerate() {
            if value == 0.0 {
                continue;
            }
            let reached = (value + self.moved[axis]).clamp(-AXIS_LIMIT, AXIS_LIMIT);
            let allowed = reached - self.moved[axis];
            self.moved[axis] = reached;
            if allowed.abs() <= MINIMUM_AXIS_MOVE {
                return Vec3::new(0.0, 0.0, 0.0);
            }
            let mut out = [0.0; 3];
            out[axis] = allowed;
            return Vec3::new(out[0], out[1], out[2]);
        }
        Vec3::new(0.0, 0.0, 0.0)
    }
}

pub fn slime_velocity(velocity: Vec3, direction: Face) -> Vec3 {
    let (dx, dy, dz) = direction.offset();
    Vec3::new(
        if dx != 0 { f64::from(dx) } else { velocity.x },
        if dy != 0 { f64::from(dy) } else { velocity.y },
        if dz != 0 { f64::from(dz) } else { velocity.z },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: Aabb = Aabb {
        min_x: 0.0,
        min_y: 0.0,
        min_z: 0.0,
        max_x: 1.0,
        max_y: 1.0,
        max_z: 1.0,
    };

    fn extending(direction: Face, progress: f32) -> Motion {
        Motion {
            direction,
            extending: true,
            source: false,
            progress,
        }
    }

    fn retracting(direction: Face, progress: f32, source: bool) -> Motion {
        Motion {
            direction,
            extending: false,
            source,
            progress,
        }
    }

    fn entity(x: f64, y: f64, z: f64) -> Aabb {
        Aabb::new(x - 0.3, y, z - 0.3, x + 0.3, y + 1.8, z + 0.3)
    }

    #[test]
    fn a_retracting_block_moves_against_the_piston_facing() {
        assert_eq!(extending(Face::East, 0.0).movement_direction(), Face::East);
        assert_eq!(
            retracting(Face::East, 0.0, false).movement_direction(),
            Face::West
        );
    }

    #[test]
    fn the_offset_goes_from_one_block_behind_to_nothing() {
        assert_eq!(extending(Face::East, 0.0).offset().x, -1.0);
        assert_eq!(extending(Face::East, 0.5).offset().x, -0.5);
        assert_eq!(extending(Face::East, 1.0).offset().x, 0.0);
        assert_eq!(retracting(Face::East, 0.0, false).offset().x, 1.0);
        assert_eq!(retracting(Face::East, 0.5, false).offset().x, 0.5);
        assert_eq!(extending(Face::Up, 0.0).offset().y, -1.0);
        assert_eq!(extending(Face::North, 0.0).offset().z, 1.0);
    }

    #[test]
    fn the_head_turns_short_after_a_quarter_of_the_way() {
        assert!(!extending(Face::East, 0.25).shows_short_head());
        assert!(extending(Face::East, 0.5).shows_short_head());
    }

    #[test]
    fn only_a_retracting_source_is_a_retracting_source() {
        assert!(retracting(Face::East, 0.0, true).is_retracting_source());
        assert!(!retracting(Face::East, 0.0, false).is_retracting_source());
        assert!(!extending(Face::East, 0.0).is_retracting_source());
    }

    #[test]
    fn the_movement_area_is_the_slab_in_front_of_the_leading_face() {
        let cases = [
            (Face::East, Aabb::new(1.0, 0.0, 0.0, 1.5, 1.0, 1.0)),
            (Face::West, Aabb::new(-0.5, 0.0, 0.0, 0.0, 1.0, 1.0)),
            (Face::Up, Aabb::new(0.0, 1.0, 0.0, 1.0, 1.5, 1.0)),
            (Face::Down, Aabb::new(0.0, -0.5, 0.0, 1.0, 0.0, 1.0)),
            (Face::South, Aabb::new(0.0, 0.0, 1.0, 1.0, 1.0, 1.5)),
            (Face::North, Aabb::new(0.0, 0.0, -0.5, 1.0, 1.0, 0.0)),
        ];
        for (direction, expected) in cases {
            assert_eq!(
                movement_area(&FULL, direction, 0.5),
                expected,
                "{direction:?}"
            );
        }
    }

    #[test]
    fn the_movement_needed_is_how_far_the_entity_sits_inside_the_slab() {
        let inside = Aabb::new(1.2, 0.0, 0.0, 1.8, 1.0, 1.0);
        let slab = movement_area(&FULL, Face::East, 0.5);
        assert!((movement_needed(&slab, Face::East, &inside) - 0.8).abs() < 1e-9);
        let west = Aabb::new(-0.8, 0.0, 0.0, -0.2, 1.0, 1.0);
        let slab = movement_area(&FULL, Face::West, 0.5);
        assert!((movement_needed(&slab, Face::West, &west) - 0.8).abs() < 1e-9);
        let above = Aabb::new(0.0, 1.2, 0.0, 1.0, 1.8, 1.0);
        let slab = movement_area(&FULL, Face::Up, 0.5);
        assert!((movement_needed(&slab, Face::Up, &above) - 0.8).abs() < 1e-9);
    }

    #[test]
    fn an_entity_in_the_path_is_pushed_by_what_it_overlaps_plus_a_margin() {
        let motion = extending(Face::East, 0.0);
        let pushed = pushed_by(&motion, (1, 64, 0), &[FULL], 0.5, &entity(1.5, 64.0, 0.5));
        assert_eq!(pushed, Some(0.5 + ENTITY_MARGIN));
    }

    #[test]
    fn an_entity_barely_in_the_path_is_pushed_only_that_far() {
        let motion = extending(Face::East, 0.0);
        let pushed =
            pushed_by(&motion, (1, 64, 0), &[FULL], 0.5, &entity(1.05, 64.0, 0.5)).expect("pushed");
        assert!((pushed - (0.35 + ENTITY_MARGIN)).abs() < 1e-9);
    }

    #[test]
    fn an_entity_beside_or_beyond_the_path_is_left_alone() {
        let motion = extending(Face::East, 0.0);
        assert_eq!(
            pushed_by(&motion, (1, 64, 0), &[FULL], 0.5, &entity(1.5, 64.0, 3.5)),
            None
        );
        assert_eq!(
            pushed_by(&motion, (1, 64, 0), &[FULL], 0.5, &entity(3.5, 64.0, 0.5)),
            None
        );
        assert_eq!(
            pushed_by(&motion, (1, 64, 0), &[FULL], 0.5, &entity(1.5, 70.0, 0.5)),
            None
        );
    }

    #[test]
    fn a_block_with_no_shape_pushes_nothing() {
        let motion = extending(Face::East, 0.0);
        assert_eq!(
            pushed_by(&motion, (1, 64, 0), &[], 0.5, &entity(1.5, 64.0, 0.5)),
            None
        );
        assert_eq!(search_area(&motion, (1, 64, 0), &[], 0.5), None);
    }

    #[test]
    fn every_part_of_a_shape_is_tried_and_the_largest_push_wins() {
        let motion = extending(Face::East, 0.0);
        let slab = Aabb::new(0.0, 0.0, 0.0, 1.0, 0.25, 1.0);
        let post = Aabb::new(0.0, 0.0, 0.0, 1.0, 1.0, 0.5);
        let far = entity(1.45, 64.0, 0.25);
        let one = pushed_by(&motion, (1, 64, 0), &[post], 0.5, &far);
        let both = pushed_by(&motion, (1, 64, 0), &[slab, post], 0.5, &far);
        assert_eq!(one, both);
        assert!(one.is_some());
    }

    #[test]
    fn the_search_area_covers_the_block_and_its_sweep() {
        let motion = extending(Face::East, 0.0);
        let area = search_area(&motion, (1, 64, 0), &[FULL], 0.5).expect("area");
        assert_eq!(area, Aabb::new(0.0, 64.0, 0.0, 1.5, 65.0, 1.0));
    }

    #[test]
    fn a_retracting_source_leaves_the_entity_inside_the_base_where_it_is_pushed_out() {
        let inside = entity(0.7, 64.0, 0.5);
        let fix = base_fix((0, 64, 0), Face::East, &inside, 0.5);
        assert!(fix.is_some());
        let outside = entity(4.5, 64.0, 0.5);
        assert_eq!(base_fix((0, 64, 0), Face::East, &outside, 0.5), None);
    }

    #[test]
    fn honey_holds_an_entity_standing_on_it_or_above_it() {
        let area = sticky_area(&extending(Face::East, 0.5), (1, 64, 0), 0.9375);
        assert!(sticks(&area, true, false, 0.5, 0.5));
        assert!(!sticks(&area, false, true, 0.5, 0.5));
        assert!(sticks(&area, true, true, 40.0, 40.0));
        assert!(!sticks(&area, true, false, 40.0, 0.5));
    }

    #[test]
    fn the_sticky_area_sits_on_top_of_the_moving_block() {
        let area = sticky_area(&extending(Face::East, 0.5), (1, 64, 0), 0.9375);
        assert_eq!(area.min_x, 0.5);
        assert_eq!(area.min_y, 64.9375);
        assert_eq!(area.max_y, 64.0 + STICKY_TOP);
    }

    #[test]
    fn slime_sets_the_velocity_along_the_axis_only() {
        let velocity = Vec3::new(0.3, -0.5, 0.2);
        assert_eq!(
            slime_velocity(velocity, Face::East),
            Vec3::new(1.0, -0.5, 0.2)
        );
        assert_eq!(
            slime_velocity(velocity, Face::West),
            Vec3::new(-1.0, -0.5, 0.2)
        );
        assert_eq!(slime_velocity(velocity, Face::Up), Vec3::new(0.3, 1.0, 0.2));
        assert_eq!(
            slime_velocity(velocity, Face::Down),
            Vec3::new(0.3, -1.0, 0.2)
        );
        assert_eq!(
            slime_velocity(velocity, Face::South),
            Vec3::new(0.3, -0.5, 1.0)
        );
        assert_eq!(
            slime_velocity(velocity, Face::North),
            Vec3::new(0.3, -0.5, -1.0)
        );
    }

    #[test]
    fn an_entity_can_be_pushed_at_most_a_bit_over_half_a_block_per_axis_per_tick() {
        let mut restriction = Restriction::default();
        let push = Vec3::new(0.51, 0.0, 0.0);
        assert_eq!(restriction.limit(10, push).x, 0.51);
        assert_eq!(restriction.limit(10, push), Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(restriction.limit(11, push).x, 0.51);
    }

    #[test]
    fn the_limit_is_tracked_per_axis_and_per_direction() {
        let mut restriction = Restriction::default();
        assert_eq!(restriction.limit(1, Vec3::new(0.3, 0.0, 0.0)).x, 0.3);
        let rest = restriction.limit(1, Vec3::new(0.3, 0.0, 0.0)).x;
        assert!((rest - 0.21).abs() < 1e-9);
        assert_eq!(restriction.limit(1, Vec3::new(0.0, 0.4, 0.0)).y, 0.4);
        assert_eq!(restriction.limit(1, Vec3::new(-0.4, 0.0, 0.0)).x, -0.4);
    }

    #[test]
    fn a_tiny_movement_goes_through_untouched() {
        let mut restriction = Restriction::default();
        let tiny = Vec3::new(1.0e-5, 0.0, 0.0);
        assert_eq!(restriction.limit(1, tiny), tiny);
    }
}
