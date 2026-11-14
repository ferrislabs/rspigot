use rspigot_entities::{EntityDimensions, EntityKind};
use rspigot_nbt::Compound;

use crate::entity::Aabb;
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::{BlockPos, Vec3};

pub const DATA_IS_LEASH_HOLDER: u8 = 17;
pub const DATA_STAYS_STILL: u8 = 18;
pub const KEYS: &[&str] = &["still_timeout"];
pub const FOOD_TAG: &str = "minecraft:happy_ghast_food";
pub const TEMPT_ITEMS_TAG: &str = "minecraft:happy_ghast_tempt_items";
pub const AVOIDS_TAG: &str = "minecraft:happy_ghast_avoids";
pub const BABY_SCALE: f32 = 0.2375;
pub const BABY_EYE_HEIGHT: f32 = 0.46875;
pub const MAX_PASSENGERS: usize = 4;
pub const STILL_TIMEOUT_TICKS: i32 = 10;
pub const STILL_TIMEOUT_MIN_AGE: i32 = 60;
pub const LEASH_HOLDER_TICKS: i32 = 5;
pub const LEASH_ELASTIC: f64 = 10.0;
pub const LEASH_SNAP: f64 = 16.0;
pub const FLYING_SPEED_SCALE: f32 = 5.0 / 3.0;
pub const RIDDEN_SPEED_SCALE: f64 = 3.9;
pub const RIDDEN_PITCH_SCALE: f32 = 0.5;
pub const RIDDEN_YAW_LERP: f32 = 0.08;
pub const RIDDEN_UP_BOOST: f32 = 0.5;
pub const RIDDEN_BACK_SCALE: f32 = -0.5;
pub const AMBIENT_INTERVAL_RIDDEN_FACTOR: i32 = 6;
pub const ADULT_VOLUME: f32 = 4.0;
pub const BABY_VOLUME: f32 = 1.0;
pub const RESTRICTION_RADIUS_UNHARNESSED: i32 = 64;
pub const RESTRICTION_RADIUS: i32 = 32;
pub const RESTRICTION_RECHECK_MARGIN: i32 = 16;
pub const HEAL_INTERVAL_SHELTERED: i32 = 20;
pub const HEAL_INTERVAL: i32 = 600;
pub const FOLLOW_RANGE: f64 = 16.0;
pub const BABY_REQUIRED_PATH_LENGTH: f32 = 48.0;
pub const BABY_FLY_MAX_TURN: i32 = 180;
pub const FLOAT_AROUND_DISTANCE: i32 = 16;
pub const TEMPT_SPEED: f64 = 1.0;
pub const TEMPT_STOP_DISTANCE: f64 = 7.0;
pub const SCAN_HORIZONTAL_MARGIN: f64 = 1.0;
pub const SCAN_BOTTOM_MARGIN: f64 = 9.999_999_747_378_752e-6;
pub const GOGGLES_DOWN_SOUND: &str = "minecraft:entity.happy_ghast.harness_goggles_down";
pub const GOGGLES_UP_SOUND: &str = "minecraft:entity.happy_ghast.harness_goggles_up";
pub const EQUIP_SOUND: &str = "minecraft:entity.happy_ghast.equip";
pub const UNEQUIP_SOUND: &str = "minecraft:entity.happy_ghast.unequip";
pub const AMBIENT_SOUND: &str = "minecraft:entity.happy_ghast.ambient";
pub const HURT_SOUND: &str = "minecraft:entity.happy_ghast.hurt";
pub const DEATH_SOUND: &str = "minecraft:entity.happy_ghast.death";
pub const BABY_AMBIENT_SOUND: &str = "minecraft:entity.ghastling.ambient";
pub const BABY_HURT_SOUND: &str = "minecraft:entity.ghastling.hurt";
pub const BABY_DEATH_SOUND: &str = "minecraft:entity.ghastling.death";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HappyGhastState {
    pub leash_holder_time: i32,
    pub server_still_timeout: i32,
}

pub fn is_happy_ghast(kind: EntityKind) -> bool {
    kind == EntityKind::HappyGhast
}

pub fn define(base: &mut EntityBase) -> HappyGhastState {
    base.data
        .define(DATA_IS_LEASH_HOLDER, SyncedValue::Boolean(false));
    base.data
        .define(DATA_STAYS_STILL, SyncedValue::Boolean(false));
    HappyGhastState::default()
}

pub fn is_leash_holder(base: &EntityBase) -> bool {
    base.data.boolean(DATA_IS_LEASH_HOLDER)
}

pub fn set_leash_holder(base: &mut EntityBase, holder: bool) {
    base.data
        .set(DATA_IS_LEASH_HOLDER, SyncedValue::Boolean(holder));
}

pub fn stays_still(base: &EntityBase) -> bool {
    base.data.boolean(DATA_STAYS_STILL)
}

pub fn set_server_still_timeout(base: &mut EntityBase, state: &mut HappyGhastState, ticks: i32) {
    state.server_still_timeout = ticks;
    base.data
        .set(DATA_STAYS_STILL, SyncedValue::Boolean(ticks > 0));
}

pub fn is_on_still_timeout(base: &EntityBase, state: &HappyGhastState) -> bool {
    stays_still(base) || state.server_still_timeout > 0
}

pub fn baby_dimensions() -> EntityDimensions {
    let adult = EntityKind::HappyGhast.dimensions();
    EntityDimensions {
        width: adult.width * BABY_SCALE,
        height: adult.height * BABY_SCALE,
        eye_height: BABY_EYE_HEIGHT,
    }
}

pub fn age_scale(baby: bool) -> f32 {
    if baby { BABY_SCALE } else { 1.0 }
}

pub fn sanitize_scale(scale: f32) -> f32 {
    scale.min(1.0)
}

pub fn restriction_radius(baby: bool, harnessed: bool) -> i32 {
    if !baby && !harnessed {
        RESTRICTION_RADIUS_UNHARNESSED
    } else {
        RESTRICTION_RADIUS
    }
}

pub fn home_needs_reset(home: Option<(BlockPos, i32)>, position: BlockPos, radius: i32) -> bool {
    let Some((home_pos, home_radius)) = home else {
        return true;
    };
    let reach = f64::from(radius + RESTRICTION_RECHECK_MARGIN);
    let dx = f64::from(home_pos.x - position.x);
    let dy = f64::from(home_pos.y - position.y);
    let dz = f64::from(home_pos.z - position.z);
    !(dx * dx + dy * dy + dz * dz < reach * reach) || home_radius != radius
}

pub fn heal_interval(sheltered: bool) -> i32 {
    if sheltered {
        HEAL_INTERVAL_SHELTERED
    } else {
        HEAL_INTERVAL
    }
}

pub fn flying_speed_factor(flying_speed: f64) -> f32 {
    flying_speed as f32 * FLYING_SPEED_SCALE
}

pub fn ridden_input(
    strafe: f32,
    forward: f32,
    pitch: f32,
    jumping: bool,
    flying_speed: f64,
) -> Vec3 {
    let mut ahead = 0.0_f32;
    let mut up = 0.0_f32;
    if forward != 0.0 {
        let radians = pitch * 0.017_453_292;
        ahead = crate::mth::cos(f64::from(radians));
        up = -crate::mth::sin(f64::from(radians));
        if forward < 0.0 {
            ahead *= RIDDEN_BACK_SCALE;
            up *= RIDDEN_BACK_SCALE;
        }
    }
    if jumping {
        up += RIDDEN_UP_BOOST;
    }
    Vec3::new(f64::from(strafe), f64::from(up), f64::from(ahead))
        .scale(flying_speed * RIDDEN_SPEED_SCALE)
}

pub fn ridden_rotation(pitch: f32, yaw: f32) -> (f32, f32) {
    (pitch * RIDDEN_PITCH_SCALE, yaw)
}

pub fn ridden_yaw(current: f32, wanted: f32) -> f32 {
    current + crate::mth::wrap_degrees(wanted - current) * RIDDEN_YAW_LERP
}

pub fn player_scan_area(bounds: &Aabb) -> Aabb {
    let height = bounds.max_y - bounds.min_y;
    Aabb::new(
        bounds.min_x - SCAN_HORIZONTAL_MARGIN,
        bounds.max_y - SCAN_BOTTOM_MARGIN,
        bounds.min_z - SCAN_HORIZONTAL_MARGIN,
        bounds.max_x + SCAN_HORIZONTAL_MARGIN,
        bounds.max_y + height / 2.0,
        bounds.max_z + SCAN_HORIZONTAL_MARGIN,
    )
}

pub fn contains(area: &Aabb, point: Vec3) -> bool {
    point.x >= area.min_x
        && point.x < area.max_x
        && point.y >= area.min_y
        && point.y < area.max_y
        && point.z >= area.min_z
        && point.z < area.max_z
}

pub fn ambient_sound(baby: bool) -> &'static str {
    if baby {
        BABY_AMBIENT_SOUND
    } else {
        AMBIENT_SOUND
    }
}

pub fn hurt_sound(baby: bool) -> &'static str {
    if baby { BABY_HURT_SOUND } else { HURT_SOUND }
}

pub fn death_sound(baby: bool) -> &'static str {
    if baby { BABY_DEATH_SOUND } else { DEATH_SOUND }
}

pub fn sound_volume(baby: bool) -> f32 {
    if baby { BABY_VOLUME } else { ADULT_VOLUME }
}

pub fn ambient_interval(base_interval: i32, vehicle: bool) -> i32 {
    if vehicle {
        base_interval * AMBIENT_INTERVAL_RIDDEN_FACTOR
    } else {
        base_interval
    }
}

pub fn walk_target_value(here_empty: bool, below_empty: bool, two_below_empty: bool) -> f32 {
    if !here_empty {
        0.0
    } else if below_empty && !two_below_empty {
        10.0
    } else {
        5.0
    }
}

pub fn read(nbt: &Compound, base: &mut EntityBase, state: &mut HappyGhastState) {
    let ticks = nbt.get_i32("still_timeout").unwrap_or(0);
    set_server_still_timeout(base, state, ticks);
}

pub fn write(state: &HappyGhastState, out: &mut Compound) {
    out.insert("still_timeout", state.server_still_timeout);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EntityId;
    use crate::entity::Entity;
    use crate::entity::save::{load_entity, save_entity};
    use uuid::Uuid;

    fn happy_ghast(nbt: Compound) -> Entity {
        load_entity(
            &nbt.with("id", "minecraft:happy_ghast"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap()
    }

    #[test]
    fn the_flags_sit_after_the_baby_flag_and_start_false() {
        let entity = happy_ghast(Compound::new());
        assert_eq!(
            entity.base.data.get(DATA_IS_LEASH_HOLDER),
            Some(&SyncedValue::Boolean(false))
        );
        assert_eq!(
            entity.base.data.get(DATA_STAYS_STILL),
            Some(&SyncedValue::Boolean(false))
        );
    }

    #[test]
    fn the_still_timeout_round_trips_and_raises_the_stays_still_flag() {
        let entity = happy_ghast(Compound::new().with("still_timeout", 7));
        assert!(stays_still(&entity.base));
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i32("still_timeout"), Some(7));
        let calm = happy_ghast(Compound::new());
        assert!(!stays_still(&calm.base));
        let saved = save_entity(&calm, None).unwrap();
        assert_eq!(saved.get_i32("still_timeout"), Some(0));
    }

    #[test]
    fn the_harness_round_trips_in_the_body_slot() {
        let nbt = Compound::new().with(
            "equipment",
            Compound::new().with(
                "body",
                Compound::new()
                    .with("id", "minecraft:blue_harness")
                    .with("count", 1),
            ),
        );
        let entity = happy_ghast(nbt);
        assert!(entity.extra.is_empty());
        let saved = save_entity(&entity, None).unwrap();
        let body = saved
            .get_compound("equipment")
            .and_then(|equipment| equipment.get_compound("body"))
            .and_then(|body| body.get_str("id"));
        assert_eq!(body, Some("minecraft:blue_harness"));
    }

    #[test]
    fn a_ghastling_is_a_fifth_of_the_adult_with_a_fixed_eye() {
        let dimensions = baby_dimensions();
        assert!((dimensions.width - 0.95).abs() < 1.0e-6);
        assert!((dimensions.height - 0.95).abs() < 1.0e-6);
        assert_eq!(dimensions.eye_height, 0.46875);
        assert_eq!(age_scale(true), 0.2375);
        assert_eq!(sanitize_scale(2.0), 1.0);
        assert_eq!(sanitize_scale(0.5), 0.5);
    }

    #[test]
    fn the_restriction_radius_is_sixty_four_only_for_a_bare_adult() {
        assert_eq!(restriction_radius(false, false), 64);
        assert_eq!(restriction_radius(false, true), 32);
        assert_eq!(restriction_radius(true, false), 32);
    }

    #[test]
    fn the_home_is_reset_when_missing_far_or_of_another_radius() {
        let here = BlockPos::new(0, 64, 0);
        assert!(home_needs_reset(None, here, 32));
        assert!(!home_needs_reset(Some((here, 32)), here, 32));
        assert!(home_needs_reset(Some((here, 64)), here, 32));
        assert!(home_needs_reset(
            Some((BlockPos::new(48, 64, 0), 32)),
            here,
            32
        ));
        assert!(!home_needs_reset(
            Some((BlockPos::new(47, 64, 0), 32)),
            here,
            32
        ));
    }

    #[test]
    fn the_ridden_input_follows_the_pitch_and_scales_with_the_flying_speed() {
        let flat = ridden_input(0.0, 1.0, 0.0, false, 0.05);
        assert!((flat.z - 0.195).abs() < 1.0e-6);
        assert!(flat.y.abs() < 1.0e-6);
        let dive = ridden_input(0.0, 1.0, 90.0, false, 0.05);
        assert!((dive.y + 0.195).abs() < 1.0e-6);
        let back = ridden_input(0.0, -1.0, 0.0, false, 0.05);
        assert!((back.z + 0.0975).abs() < 1.0e-6);
        let rise = ridden_input(0.0, 0.0, 0.0, true, 0.05);
        assert!((rise.y - 0.0975).abs() < 1.0e-6);
        let strafe = ridden_input(1.0, 0.0, 0.0, false, 0.05);
        assert!((strafe.x - 0.195).abs() < 1.0e-6);
    }

    #[test]
    fn the_ridden_yaw_closes_eight_percent_of_the_gap() {
        assert!((ridden_yaw(0.0, 100.0) - 8.0).abs() < 1.0e-4);
        assert!((ridden_yaw(170.0, -170.0) - 171.6).abs() < 1.0e-3);
        assert_eq!(ridden_rotation(40.0, 10.0), (20.0, 10.0));
    }

    #[test]
    fn the_player_scan_area_reaches_half_a_height_above_the_back() {
        let bounds = Aabb::new(-2.0, 0.0, -2.0, 2.0, 4.0, 2.0);
        let area = player_scan_area(&bounds);
        assert_eq!(area.min_x, -3.0);
        assert_eq!(area.max_x, 3.0);
        assert_eq!(area.max_y, 6.0);
        assert!(area.min_y < 4.0 && area.min_y > 3.99);
        assert!(contains(&area, Vec3::new(0.0, 4.0, 0.0)));
        assert!(!contains(&area, Vec3::new(0.0, 3.0, 0.0)));
        assert!(!contains(&area, Vec3::new(0.0, 6.0, 0.0)));
    }

    #[test]
    fn the_walk_target_value_prefers_a_clear_column_with_ground_two_below() {
        assert_eq!(walk_target_value(false, true, false), 0.0);
        assert_eq!(walk_target_value(true, true, false), 10.0);
        assert_eq!(walk_target_value(true, true, true), 5.0);
        assert_eq!(walk_target_value(true, false, false), 5.0);
    }

    #[test]
    fn the_voice_depends_on_the_age() {
        assert_eq!(ambient_sound(true), "minecraft:entity.ghastling.ambient");
        assert_eq!(hurt_sound(false), "minecraft:entity.happy_ghast.hurt");
        assert_eq!(death_sound(true), "minecraft:entity.ghastling.death");
        assert_eq!(sound_volume(false), 4.0);
        assert_eq!(sound_volume(true), 1.0);
        assert_eq!(ambient_interval(80, true), 480);
        assert_eq!(ambient_interval(80, false), 80);
        assert_eq!(heal_interval(true), 20);
        assert_eq!(heal_interval(false), 600);
    }
}
