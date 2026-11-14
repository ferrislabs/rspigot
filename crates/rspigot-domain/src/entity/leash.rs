use rspigot_entities::EntityKind;

use crate::{Vec3, mth};

pub const SNAP_DISTANCE: f64 = 12.0;
pub const ELASTIC_DISTANCE: f64 = 6.0;
pub const LOOSE_TICKS: i32 = 100;
pub const TORQUE_FACTOR: f64 = 10.0;
pub const MOVEMENT_PULL: f64 = 0.11;
pub const AXIS_ELASTICITY: Vec3 = Vec3::new(0.8, 0.2, 0.8);
pub const ENTITY_ATTACHMENT: Vec3 = Vec3::new(0.0, 0.5, 0.5);
pub const LEASHER_ATTACHMENT: Vec3 = Vec3::new(0.0, 0.5, 0.0);
const DEG_TO_RAD: f32 = 0.017_453_292;
const BLOCK_FRICTION_DRAG: f32 = 0.91;
const LIQUID_FRICTION: f32 = 0.8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Body {
    pub position: Vec3,
    pub yaw: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pull {
    pub force: Vec3,
    pub torque: f64,
}

pub fn center_distance(a: Vec3, b: Vec3) -> f64 {
    a.distance_squared(b).sqrt()
}

pub fn distances(kind: EntityKind) -> (f64, f64) {
    if kind == EntityKind::HappyGhast {
        (
            super::species::happy_ghast::LEASH_ELASTIC,
            super::species::happy_ghast::LEASH_SNAP,
        )
    } else {
        (ELASTIC_DISTANCE, SNAP_DISTANCE)
    }
}

pub fn must_pull(distance: f64, entity_width: f32, holder_width: f32) -> bool {
    must_pull_at(ELASTIC_DISTANCE, distance, entity_width, holder_width)
}

pub fn must_pull_at(elastic: f64, distance: f64, entity_width: f32, holder_width: f32) -> bool {
    elastic - f64::from(holder_width) - f64::from(entity_width) < distance
}

pub fn snaps(distance: f64) -> bool {
    snaps_at(SNAP_DISTANCE, distance)
}

pub fn snaps_at(snap: f64, distance: f64) -> bool {
    snap < distance
}

fn y_rot(vector: Vec3, angle: f32) -> Vec3 {
    let cos = f64::from(mth::cos(f64::from(angle)));
    let sin = f64::from(mth::sin(f64::from(angle)));
    Vec3::new(
        vector.x * cos + vector.z * sin,
        vector.y,
        vector.z * cos - vector.x * sin,
    )
}

fn dampened_spring(
    pivot: Vec3,
    object: Vec3,
    slack: f64,
    motion: Vec3,
    lever_arm: Vec3,
) -> Option<Pull> {
    let distance = object.distance_squared(pivot).sqrt();
    if distance < slack {
        return None;
    }
    let force = (pivot - object).normalize().scale(distance - slack);
    let torque = lever_arm.z * force.x - lever_arm.x * force.z;
    let dot = motion.x * force.x + motion.y * force.y + motion.z * force.z;
    let force = if dot >= 0.0 { force.scale(2.0) } else { force };
    Some(Pull { force, torque })
}

pub fn elastic_pull(entity: &Body, holder: &Body, entity_movement: Vec3) -> Option<Pull> {
    elastic_pull_at(ELASTIC_DISTANCE, entity, holder, entity_movement)
}

pub fn elastic_pull_at(
    elastic: f64,
    entity: &Body,
    holder: &Body,
    entity_movement: Vec3,
) -> Option<Pull> {
    let entity_yaw = entity.yaw * DEG_TO_RAD;
    let holder_yaw = holder.yaw * DEG_TO_RAD;
    let entity_size = (
        f64::from(entity.width),
        f64::from(entity.height),
        f64::from(entity.width),
    );
    let holder_size = (
        f64::from(holder.width),
        f64::from(holder.height),
        f64::from(holder.width),
    );
    let lever_arm = y_rot(
        ENTITY_ATTACHMENT.multiply(entity_size.0, entity_size.1, entity_size.2),
        -entity_yaw,
    );
    let entity_point = entity.position + lever_arm;
    let holder_point = holder.position
        + y_rot(
            LEASHER_ATTACHMENT.multiply(holder_size.0, holder_size.1, holder_size.2),
            -holder_yaw,
        );
    dampened_spring(
        holder_point,
        entity_point,
        elastic,
        entity_movement,
        lever_arm,
    )
}

pub fn impulse(pull: &Pull, holder_movement: Vec3, entity_movement: Vec3) -> Vec3 {
    let relative = holder_movement - entity_movement;
    pull.force
        .multiply(AXIS_ELASTICITY.x, AXIS_ELASTICITY.y, AXIS_ELASTICITY.z)
        + relative.scale(MOVEMENT_PULL)
}

pub fn angular_friction(on_ground: bool, block_friction: f32, in_liquid: bool) -> f32 {
    if on_ground {
        block_friction * BLOCK_FRICTION_DRAG
    } else if in_liquid {
        LIQUID_FRICTION
    } else {
        BLOCK_FRICTION_DRAG
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(x: f64, z: f64) -> Body {
        Body {
            position: Vec3::new(x, 64.0, z),
            yaw: 0.0,
            width: 0.9,
            height: 1.87,
        }
    }

    #[test]
    fn the_lead_snaps_past_twelve_blocks_and_pulls_past_the_elastic_distance_minus_both_widths() {
        assert!(!snaps(12.0));
        assert!(snaps(12.01));
        assert!(!must_pull(4.2, 0.9, 0.6));
        assert!(must_pull(4.6, 0.9, 0.6));
    }

    #[test]
    fn a_close_entity_gets_no_pull() {
        assert!(elastic_pull(&body(0.0, 0.0), &body(3.0, 0.0), Vec3::ZERO).is_none());
    }

    #[test]
    fn a_far_entity_is_pulled_with_double_force_unless_it_already_moves_against_the_lead() {
        let entity = body(0.0, 0.0);
        let holder = body(10.0, 0.0);
        let still = elastic_pull(&entity, &holder, Vec3::ZERO).unwrap();
        assert!(still.force.x > 0.0);
        let toward = elastic_pull(&entity, &holder, Vec3::new(1.0, 0.0, 0.0)).unwrap();
        assert_eq!(toward.force, still.force);
        let against = elastic_pull(&entity, &holder, Vec3::new(-1.0, 0.0, 0.0)).unwrap();
        assert!((against.force.x * 2.0 - still.force.x).abs() < 1e-9);
    }

    #[test]
    fn the_impulse_scales_the_force_per_axis_and_adds_a_tenth_of_the_relative_movement() {
        let pull = Pull {
            force: Vec3::new(1.0, 1.0, 1.0),
            torque: 0.0,
        };
        let moved = impulse(&pull, Vec3::new(1.0, 0.0, 0.0), Vec3::ZERO);
        assert!((moved.x - (0.8 + 0.11)).abs() < 1e-9);
        assert!((moved.y - 0.2).abs() < 1e-9);
        assert!((moved.z - 0.8).abs() < 1e-9);
    }

    #[test]
    fn the_angular_friction_depends_on_the_ground_and_the_liquid() {
        assert!((angular_friction(true, 0.6, false) - 0.546).abs() < 1e-6);
        assert_eq!(angular_friction(false, 0.6, true), 0.8);
        assert_eq!(angular_friction(false, 0.6, false), 0.91);
    }
}
