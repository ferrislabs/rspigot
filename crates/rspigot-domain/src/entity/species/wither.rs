use rspigot_nbt::Compound;

use crate::Vec3;
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::{self, LivingState};
use crate::mth;

pub const DATA_TARGET_A: u8 = 16;
pub const DATA_TARGET_B: u8 = 17;
pub const DATA_TARGET_C: u8 = 18;
pub const DATA_ID_INV: u8 = 19;
pub const DATA_TARGETS: [u8; 3] = [DATA_TARGET_A, DATA_TARGET_B, DATA_TARGET_C];
pub const KEYS: &[&str] = &["Invul"];
pub const SPAWN_INVULNERABLE_TICKS: i32 = 220;
pub const SPAWN_HEALTH_DIVISOR: f32 = 3.0;
pub const SIDE_HEADS: usize = 2;
pub const NEXT_HEAD_BASE: i32 = 10;
pub const NEXT_HEAD_RANDOM: i32 = 10;
pub const NEXT_HEAD_AFTER_SHOT: i32 = 40;
pub const NEXT_HEAD_AFTER_SHOT_RANDOM: i32 = 20;
pub const IDLE_HEAD_THRESHOLD: i32 = 15;
pub const IDLE_HEAD_HURT_BONUS: i32 = 3;
pub const IDLE_AIM_HORIZONTAL: f64 = 10.0;
pub const IDLE_AIM_VERTICAL: f64 = 5.0;
pub const HEAD_SCAN_HORIZONTAL: f64 = 20.0;
pub const HEAD_SCAN_VERTICAL: f64 = 8.0;
pub const HEAD_RANGE_SQR: f64 = 900.0;
pub const DANGEROUS_CHANCE: f32 = 0.001;
pub const DESTROY_BLOCKS_TICKS: i32 = 20;
pub const REGEN_INTERVAL: i32 = 20;
pub const REGEN_AMOUNT: f32 = 1.0;
pub const SPAWN_REGEN_INTERVAL: i32 = 10;
pub const SPAWN_REGEN_AMOUNT: f32 = 10.0;
pub const EXPLOSION_RADIUS: f32 = 7.0;
pub const SHOOT_EVENT: i32 = 1024;
pub const BREAK_EVENT: i32 = 1022;
pub const SPAWN_EVENT: i32 = 1023;
pub const HEAD_SPACING_DEGREES: i32 = 180;
pub const HEAD_RADIUS: f64 = 1.3;
pub const CENTER_HEAD_HEIGHT: f32 = 3.0;
pub const SIDE_HEAD_HEIGHT: f32 = 2.2;
pub const POWERED_ARMOR_HEALTH_FRACTION: f32 = 2.0;
pub const XP_REWARD: i32 = 50;
pub const NETHER_STAR_AGE: i32 = -32_768;
const DEG_TO_RAD: f32 = 0.017_453_292;
const RAD_TO_DEG: f32 = 57.295_776;
const SHOOT_HEIGHT_FACTOR: f32 = 0.5;
const SWOOP_DOWN_PULL: f64 = 0.3;
const SWOOP_DAMPING: f64 = 0.6;
const HOVER_ABOVE_TARGET: f64 = 5.0;
const HORIZONTAL_CHASE_SQR: f64 = 9.0;
const CHASE_PULL: f64 = 0.3;
const VERTICAL_DRAG: f64 = 0.6;
const FACING_SPEED_SQR: f64 = 0.05;
const YAW_OFFSET: f32 = 90.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WitherState {
    pub next_head_update: [i32; SIDE_HEADS],
    pub idle_head_updates: [i32; SIDE_HEADS],
    pub destroy_blocks_tick: i32,
    pub breaking_blocks: bool,
    pub broke_blocks: bool,
}

pub fn define(base: &mut EntityBase) -> WitherState {
    for index in DATA_TARGETS {
        base.data.define(index, SyncedValue::Int(0));
    }
    base.data.define(DATA_ID_INV, SyncedValue::Int(0));
    WitherState::default()
}

fn int(base: &EntityBase, index: u8) -> i32 {
    match base.data.get(index) {
        Some(SyncedValue::Int(value)) => *value,
        _ => 0,
    }
}

pub fn invulnerable_ticks(base: &EntityBase) -> i32 {
    int(base, DATA_ID_INV)
}

pub fn set_invulnerable_ticks(base: &mut EntityBase, ticks: i32) {
    base.data.set(DATA_ID_INV, SyncedValue::Int(ticks));
}

pub fn make_invulnerable(base: &mut EntityBase, living: &mut LivingState) {
    set_invulnerable_ticks(base, SPAWN_INVULNERABLE_TICKS);
    let max = living.max_health() as f32;
    living::set_health(base, living, max / SPAWN_HEALTH_DIVISOR);
}

pub fn alternative_target(base: &EntityBase, head: usize) -> i32 {
    DATA_TARGETS.get(head).map_or(0, |index| int(base, *index))
}

pub fn set_alternative_target(base: &mut EntityBase, head: usize, entity: i32) {
    if let Some(index) = DATA_TARGETS.get(head) {
        base.data.set(*index, SyncedValue::Int(entity));
    }
}

pub fn is_powered(health: f32, max_health: f32) -> bool {
    health <= max_health / POWERED_ARMOR_HEALTH_FRACTION
}

pub fn read(nbt: &Compound, base: &mut EntityBase, _state: &mut WitherState) {
    set_invulnerable_ticks(base, nbt.get_i32("Invul").unwrap_or(0));
}

pub fn write(base: &EntityBase, _state: &WitherState, out: &mut Compound) {
    out.insert("Invul", invulnerable_ticks(base));
}

pub fn head_x(position: Vec3, body_yaw: f32, scale: f32, head: usize) -> f64 {
    if head < 1 {
        return position.x;
    }
    let angle = (body_yaw + ((head as i32 - 1) * HEAD_SPACING_DEGREES) as f32) * DEG_TO_RAD;
    position.x + f64::from(mth::cos(f64::from(angle))) * HEAD_RADIUS * f64::from(scale)
}

pub fn head_y(position: Vec3, scale: f32, head: usize) -> f64 {
    let height = if head < 1 {
        CENTER_HEAD_HEIGHT
    } else {
        SIDE_HEAD_HEIGHT
    };
    position.y + f64::from(height * scale)
}

pub fn head_z(position: Vec3, body_yaw: f32, scale: f32, head: usize) -> f64 {
    if head < 1 {
        return position.z;
    }
    let angle = (body_yaw + ((head as i32 - 1) * HEAD_SPACING_DEGREES) as f32) * DEG_TO_RAD;
    position.z + f64::from(mth::sin(f64::from(angle))) * HEAD_RADIUS * f64::from(scale)
}

pub fn shot_target_y(target_y: f64, eye_height: f32) -> f64 {
    target_y + f64::from(eye_height * SHOOT_HEIGHT_FACTOR)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Steering {
    pub velocity: Vec3,
    pub yaw: Option<f32>,
}

pub fn steer(velocity: Vec3, position: Vec3, target: Option<Vec3>, powered: bool) -> Steering {
    let mut movement = Vec3::new(velocity.x, velocity.y * VERTICAL_DRAG, velocity.z);
    if let Some(target) = target {
        let mut y = movement.y;
        if position.y < target.y || (!powered && position.y < target.y + HOVER_ABOVE_TARGET) {
            y = y.max(0.0);
            y += SWOOP_DOWN_PULL - y * f64::from(SWOOP_DAMPING as f32);
        }
        movement = Vec3::new(movement.x, y, movement.z);
        let delta = Vec3::new(target.x - position.x, 0.0, target.z - position.z);
        let horizontal = delta.x * delta.x + delta.z * delta.z;
        if horizontal > HORIZONTAL_CHASE_SQR {
            let length = horizontal.sqrt();
            let direction = Vec3::new(delta.x / length, 0.0, delta.z / length);
            movement = Vec3::new(
                movement.x + (direction.x * CHASE_PULL - movement.x * SWOOP_DAMPING),
                movement.y,
                movement.z + (direction.z * CHASE_PULL - movement.z * SWOOP_DAMPING),
            );
        }
    }
    let horizontal = movement.x * movement.x + movement.z * movement.z;
    let yaw = (horizontal > FACING_SPEED_SQR)
        .then(|| (mth::atan2(movement.z, movement.x) as f32) * RAD_TO_DEG - YAW_OFFSET);
    Steering {
        velocity: movement,
        yaw,
    }
}

#[cfg(test)]
mod tests {
    use rspigot_nbt::Compound;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::save::{load_entity, save_entity};
    use crate::entity::species::Species;

    fn wither(nbt: Compound) -> crate::entity::Entity {
        load_entity(
            &nbt.with("id", "minecraft:wither"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap()
    }

    #[test]
    fn the_four_synched_ints_start_at_index_sixteen_and_default_to_zero() {
        let entity = wither(Compound::new());
        for index in 16..=19 {
            assert_eq!(entity.base.data.get(index), Some(&SyncedValue::Int(0)));
        }
        assert!(matches!(
            entity
                .living()
                .and_then(|living| living.mob.as_ref())
                .map(|mob| mob.species),
            Some(Species::Wither(_))
        ));
    }

    #[test]
    fn the_invulnerable_ticks_round_trip_through_the_invul_key() {
        let entity = wither(Compound::new().with("Invul", 120));
        assert_eq!(invulnerable_ticks(&entity.base), 120);
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i32("Invul"), Some(120));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
    }

    #[test]
    fn a_wither_without_the_key_saves_zero_invulnerable_ticks() {
        let saved = save_entity(&wither(Compound::new()), None).unwrap();
        assert_eq!(saved.get_i32("Invul"), Some(0));
    }

    #[test]
    fn alternative_targets_use_their_own_slot_and_ignore_an_unknown_head() {
        let mut entity = wither(Compound::new());
        set_alternative_target(&mut entity.base, 1, 42);
        assert_eq!(alternative_target(&entity.base, 1), 42);
        assert_eq!(alternative_target(&entity.base, 0), 0);
        assert_eq!(
            entity.base.data.get(DATA_TARGET_B),
            Some(&SyncedValue::Int(42))
        );
        set_alternative_target(&mut entity.base, 5, 9);
        assert_eq!(alternative_target(&entity.base, 5), 0);
    }

    #[test]
    fn making_a_wither_invulnerable_sets_two_hundred_twenty_ticks_and_a_third_of_the_health() {
        let mut entity = wither(Compound::new());
        let crate::entity::EntityState::Living(state) = &mut entity.state else {
            panic!();
        };
        make_invulnerable(&mut entity.base, state);
        assert_eq!(invulnerable_ticks(&entity.base), 220);
        assert_eq!(living::health(&entity.base), 100.0);
    }

    #[test]
    fn powered_starts_at_half_health() {
        assert!(!is_powered(151.0, 300.0));
        assert!(is_powered(150.0, 300.0));
        assert!(is_powered(1.0, 300.0));
    }

    #[test]
    fn the_head_positions_sit_three_blocks_up_in_the_middle_and_on_either_side_at_two_point_two() {
        let at = Vec3::new(10.0, 64.0, -4.0);
        assert_eq!(head_x(at, 0.0, 1.0, 0), 10.0);
        assert_eq!(head_z(at, 0.0, 1.0, 0), -4.0);
        assert_eq!(head_y(at, 1.0, 0), 67.0);
        assert!((head_y(at, 1.0, 1) - 66.2).abs() < 1e-6);
        assert!((head_x(at, 0.0, 1.0, 1) - 11.3).abs() < 1e-6);
        assert!((head_x(at, 0.0, 1.0, 2) - 8.7).abs() < 1e-6);
        assert!((head_z(at, 0.0, 1.0, 1) - -4.0).abs() < 1e-6);
        assert!((head_x(at, 0.0, 2.0, 1) - 12.6).abs() < 1e-6);
        assert!((head_y(at, 2.0, 0) - 70.0).abs() < 1e-6);
    }

    #[test]
    fn the_shot_aims_at_half_the_target_eye_height() {
        assert_eq!(shot_target_y(64.0, 1.62), 64.0 + 0.81f32 as f64);
    }

    #[test]
    fn steering_without_a_target_only_damps_the_vertical_speed() {
        let steering = steer(Vec3::new(0.0, 1.0, 0.0), Vec3::ZERO, None, false);
        assert_eq!(steering.velocity, Vec3::new(0.0, 0.6, 0.0));
        assert_eq!(steering.yaw, None);
    }

    #[test]
    fn steering_rises_toward_a_target_above_and_chases_it_horizontally_past_three_blocks() {
        let steering = steer(
            Vec3::ZERO,
            Vec3::new(0.0, 60.0, 0.0),
            Some(Vec3::new(10.0, 64.0, 0.0)),
            false,
        );
        assert!((steering.velocity.y - 0.3).abs() < 1e-9);
        assert!((steering.velocity.x - 0.3).abs() < 1e-9);
        assert_eq!(steering.velocity.z, 0.0);
        assert_eq!(steering.yaw, Some(-90.0));
    }

    #[test]
    fn a_powered_wither_only_climbs_when_below_the_target_and_hovers_otherwise() {
        let target = Some(Vec3::new(0.0, 64.0, 0.0));
        let high = Vec3::new(0.0, 66.0, 0.0);
        let low = Vec3::new(0.0, 63.0, 0.0);
        assert_eq!(steer(Vec3::ZERO, high, target, true).velocity.y, 0.0);
        assert!((steer(Vec3::ZERO, high, target, false).velocity.y - 0.3).abs() < 1e-9);
        assert!((steer(Vec3::ZERO, low, target, true).velocity.y - 0.3).abs() < 1e-9);
    }

    #[test]
    fn the_wither_faces_its_velocity_once_it_is_fast_enough() {
        let steering = steer(Vec3::new(0.5, 0.0, 0.0), Vec3::ZERO, None, false);
        let yaw = steering.yaw.unwrap();
        assert!((yaw - -90.0).abs() < 0.2, "{yaw}");
        let slow = steer(Vec3::new(0.1, 0.0, 0.0), Vec3::ZERO, None, false);
        assert_eq!(slow.yaw, None);
    }
}
