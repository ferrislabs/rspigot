use uuid::Uuid;

use super::base::EntityBase;
use super::data::SyncedValue;
use crate::random::{LegacyRandom, RandomSource};
use crate::{EntityId, Vec3, mth};

pub const DATA_HOOKED_ENTITY: u8 = 8;
pub const DATA_BITING: u8 = 9;
pub const GRAVITY: f64 = 0.03;
pub const DRAG: f64 = 0.92;
pub const WATER_ENTRY: (f64, f64, f64) = (0.3, 0.2, 0.3);
pub const BOBBING_DRAG: f64 = 0.9;
pub const MAX_LIFE: i32 = 1199;
pub const MAX_DISTANCE_SQR: f64 = 1024.0;
pub const MAX_OUT_OF_WATER: i32 = 10;
pub const CLOSE_LAUNCH: f64 = 0.3;
pub const LAUNCH_SPEED: f64 = 0.6;
pub const LAUNCH_CENTER: f64 = 0.5;
pub const LAUNCH_SPREAD: f64 = 0.010_336_5;
pub const PULL: f64 = 0.1;
pub const HOOKED_HEIGHT: f64 = 0.8;
pub const MIN_WAIT: i32 = 100;
pub const MAX_WAIT: i32 = 600;
pub const MIN_LURE: i32 = 20;
pub const MAX_LURE: i32 = 80;
pub const MIN_NIBBLE: i32 = 20;
pub const MAX_NIBBLE: i32 = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookPhase {
    Flying,
    HookedInEntity,
    Bobbing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HookState {
    pub owner: Option<Uuid>,
    pub phase: HookPhase,
    pub hooked: Option<EntityId>,
    pub life: i32,
    pub out_of_water_time: i32,
    pub time_until_lured: i32,
    pub time_until_hooked: i32,
    pub nibble: i32,
    pub fish_angle: f32,
    pub luck: i32,
    pub lure_speed: i32,
    pub open_water: bool,
    pub left_owner: bool,
    pub random: LegacyRandom,
}

impl HookState {
    pub fn new(base: &mut EntityBase, seed: i64, luck: i32, lure_speed: i32) -> Self {
        base.data.define(DATA_HOOKED_ENTITY, SyncedValue::Int(0));
        base.data.define(DATA_BITING, SyncedValue::Boolean(false));
        Self {
            owner: None,
            phase: HookPhase::Flying,
            hooked: None,
            life: 0,
            out_of_water_time: 0,
            time_until_lured: 0,
            time_until_hooked: 0,
            nibble: 0,
            fish_angle: 0.0,
            luck: luck.max(0),
            lure_speed: lure_speed.max(0),
            open_water: true,
            left_owner: false,
            random: LegacyRandom::new(seed),
        }
    }
}

pub fn launch(base: &mut EntityBase, state: &mut HookState, eye: Vec3, yaw: f32, pitch: f32) {
    let yaw_rad = f64::from(-yaw * mth::DEG_TO_RAD - std::f32::consts::PI);
    let pitch_rad = f64::from(-pitch * mth::DEG_TO_RAD);
    let cos_yaw = mth::cos(yaw_rad);
    let sin_yaw = mth::sin(yaw_rad);
    let cos_pitch = -mth::cos(pitch_rad);
    let sin_pitch = mth::sin(pitch_rad);
    base.snap_to(
        Vec3::new(
            eye.x - f64::from(sin_yaw) * CLOSE_LAUNCH,
            eye.y,
            eye.z - f64::from(cos_yaw) * CLOSE_LAUNCH,
        ),
        yaw,
        pitch,
    );
    let rise = f64::from(mth::clamp(-(sin_pitch / cos_pitch), -5.0, 5.0));
    let direction = Vec3::new(f64::from(-sin_yaw), rise, f64::from(-cos_yaw));
    let scale = LAUNCH_SPEED / direction.length();
    let random = &mut state.random;
    let x = scale + random.triangle(LAUNCH_CENTER, LAUNCH_SPREAD);
    let y = scale + random.triangle(LAUNCH_CENTER, LAUNCH_SPREAD);
    let z = scale + random.triangle(LAUNCH_CENTER, LAUNCH_SPREAD);
    let movement = direction.multiply(x, y, z);
    base.velocity = movement;
    base.needs_sync = true;
    let horizontal = (movement.x * movement.x + movement.z * movement.z).sqrt();
    base.set_yaw((mth::atan2(movement.x, movement.z) * mth::RAD_TO_DEG) as f32);
    base.set_pitch((mth::atan2(movement.y, horizontal) * mth::RAD_TO_DEG) as f32);
}

pub fn set_hooked(base: &mut EntityBase, state: &mut HookState, hooked: Option<EntityId>) {
    state.hooked = hooked;
    let value = hooked.map_or(0, |id| id.0 + 1);
    base.data.set(DATA_HOOKED_ENTITY, SyncedValue::Int(value));
}

pub fn set_biting(base: &mut EntityBase, biting: bool) {
    base.data.set(DATA_BITING, SyncedValue::Boolean(biting));
}

pub fn is_biting(base: &EntityBase) -> bool {
    matches!(base.data.get(DATA_BITING), Some(SyncedValue::Boolean(true)))
}

pub fn wait_modifier(rain: bool, covered: bool, random: &mut LegacyRandom) -> i32 {
    let mut speed = 1;
    if random.next_float() < 0.25 && rain {
        speed += 1;
    }
    if random.next_float() < 0.5 && covered {
        speed -= 1;
    }
    speed
}

pub fn bite_chance(time_until_lured: i32) -> f32 {
    if time_until_lured < 20 {
        (20 - time_until_lured) as f32 * 0.05 + 0.15
    } else if time_until_lured < 40 {
        (40 - time_until_lured) as f32 * 0.02 + 0.15
    } else if time_until_lured < 60 {
        (60 - time_until_lured) as f32 * 0.01 + 0.15
    } else {
        0.15
    }
}

pub fn next_float_between(random: &mut LegacyRandom, min: f32, max: f32) -> f32 {
    random.next_float() * (max - min) + min
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;

    use super::*;

    fn hook() -> (EntityBase, HookState) {
        let mut base = EntityBase::new(EntityId(4), Uuid::from_u128(4), EntityKind::FishingBobber);
        let state = HookState::new(&mut base, 3, 2, -1);
        (base, state)
    }

    #[test]
    fn a_new_hook_defines_the_hooked_entity_and_the_bite_flag() {
        let (base, state) = hook();
        assert_eq!(
            base.data.get(DATA_HOOKED_ENTITY),
            Some(&SyncedValue::Int(0))
        );
        assert!(!is_biting(&base));
        assert_eq!(state.luck, 2);
        assert_eq!(state.lure_speed, 0);
        assert!(state.open_water);
    }

    #[test]
    fn hooking_an_entity_publishes_its_id_plus_one() {
        let (mut base, mut state) = hook();
        set_hooked(&mut base, &mut state, Some(EntityId(9)));
        assert_eq!(
            base.data.get(DATA_HOOKED_ENTITY),
            Some(&SyncedValue::Int(10))
        );
        set_hooked(&mut base, &mut state, None);
        assert_eq!(
            base.data.get(DATA_HOOKED_ENTITY),
            Some(&SyncedValue::Int(0))
        );
    }

    #[test]
    fn launching_straight_ahead_puts_the_hook_in_front_of_the_eye() {
        let (mut base, mut state) = hook();
        launch(&mut base, &mut state, Vec3::new(0.0, 65.0, 0.0), 0.0, 0.0);
        assert!((base.position.z - 0.3).abs() < 1e-6);
        assert!(base.position.x.abs() < 1e-6);
        assert!(base.velocity.z > 0.0);
        assert!(base.needs_sync);
    }

    #[test]
    fn the_bite_chance_grows_as_the_lure_time_runs_out() {
        assert_eq!(bite_chance(100), 0.15);
        assert!(bite_chance(50) > 0.15);
        assert!(bite_chance(10) > bite_chance(30));
    }
}
