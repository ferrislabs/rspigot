use crate::{BlockPos, Vec3};

pub const SLIDE_SPEED: f64 = -0.05;
pub const FAST_FALL: f64 = -0.13;
pub const MIN_FALL_SPEED: f64 = -0.08;
pub const TOP_OFFSET: f64 = 0.9375;
pub const EDGE: f64 = 1.0e-7;
pub const INNER_HALF: f64 = 0.4375;
pub const ACHIEVEMENT_INTERVAL: u64 = 20;
pub const EFFECT_ODDS: i32 = 5;
pub const PARTICLE_EVENT: i8 = 53;
pub const SLIDE_SOUND: &str = "minecraft:block.honey_block.slide";

pub fn is_sliding_down(
    block: BlockPos,
    entity: Vec3,
    width: f64,
    on_ground: bool,
    velocity: Vec3,
) -> bool {
    if on_ground {
        return false;
    }
    if entity.y > f64::from(block.y) + TOP_OFFSET - EDGE {
        return false;
    }
    if velocity.y >= MIN_FALL_SPEED {
        return false;
    }
    let dx = (f64::from(block.x) + 0.5 - entity.x).abs();
    let dz = (f64::from(block.z) + 0.5 - entity.z).abs();
    let reach = INNER_HALF + width / 2.0;
    dx + EDGE > reach || dz + EDGE > reach
}

pub fn slide_velocity(velocity: Vec3) -> Vec3 {
    if velocity.y < FAST_FALL {
        let scale = SLIDE_SPEED / velocity.y;
        Vec3::new(velocity.x * scale, SLIDE_SPEED, velocity.z * scale)
    } else {
        Vec3::new(velocity.x, SLIDE_SPEED, velocity.z)
    }
}

pub const fn achievement_due(game_time: u64) -> bool {
    game_time.is_multiple_of(ACHIEVEMENT_INTERVAL)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK: BlockPos = BlockPos::new(0, 64, 0);

    #[test]
    fn a_falling_entity_pressed_against_the_side_slides() {
        let wall = Vec3::new(0.5 + 0.4375 + 0.3, 64.5, 0.5);
        let falling = Vec3::new(0.0, -0.2, 0.0);
        assert!(is_sliding_down(BLOCK, wall, 0.6, false, falling));
    }

    #[test]
    fn an_entity_over_the_block_or_on_the_ground_or_barely_falling_does_not_slide() {
        let wall = Vec3::new(1.2, 64.5, 0.5);
        let falling = Vec3::new(0.0, -0.2, 0.0);
        assert!(!is_sliding_down(BLOCK, wall, 0.6, true, falling));
        assert!(!is_sliding_down(
            BLOCK,
            wall,
            0.6,
            false,
            Vec3::new(0.0, -0.05, 0.0)
        ));
        let above = Vec3::new(1.2, 65.0, 0.5);
        assert!(!is_sliding_down(BLOCK, above, 0.6, false, falling));
        let middle = Vec3::new(0.5, 64.5, 0.5);
        assert!(!is_sliding_down(BLOCK, middle, 0.6, false, falling));
    }

    #[test]
    fn the_slide_caps_the_fall_and_scales_the_sideways_speed() {
        let fast = slide_velocity(Vec3::new(0.2, -0.5, -0.1));
        assert_eq!(fast.y, SLIDE_SPEED);
        assert!((fast.x - 0.02).abs() < 1.0e-12);
        assert!((fast.z + 0.01).abs() < 1.0e-12);
        let slow = slide_velocity(Vec3::new(0.2, -0.1, 0.0));
        assert_eq!(slow, Vec3::new(0.2, SLIDE_SPEED, 0.0));
    }

    #[test]
    fn the_advancement_is_checked_once_a_second() {
        assert!(achievement_due(40));
        assert!(!achievement_due(41));
    }
}
