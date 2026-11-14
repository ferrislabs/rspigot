use rspigot_entities::EntityKind;
use rspigot_nbt::Compound;

use crate::Vec3;
use crate::entity::ageable;
use crate::entity::base::EntityBase;
use crate::entity::data::{Pose, SyncedValue};
use crate::entity::living::LivingState;
use crate::entity::species::{Species, equine};
use crate::mth;
use crate::random::RandomSource;
use crate::spawn::{SpawnGroupData, SpawnReason};

pub const DATA_DASH: u8 = 19;
pub const DATA_LAST_POSE_CHANGE_TICK: u8 = 20;
pub const KEYS: &[&str] = &["LastPoseTick"];
pub const SIT_DOWN_TICKS: i64 = 40;
pub const STAND_UP_TICKS: i64 = 52;
pub const FULL_STAND_TICKS: i64 = 53;
pub const RANDOM_SITTING_SECONDS: i32 = 20;
pub const DASH_COOLDOWN: i32 = 55;
pub const DASH_READY_THRESHOLD: i32 = 50;
pub const SITTING_DROP: f32 = 1.43;
pub const MAX_HEAD_Y_ROT: i32 = 30;
pub const PANIC_SPEED: f32 = 4.0;
pub const SWIM_CHANCE: f32 = 0.8;
pub const WALK_SPEED: f64 = 2.0;
pub const TEMPT_SPEED: f64 = 2.5;
pub const BABY_TEMPT_DISTANCE: f64 = 2.5;
pub const ADULT_TEMPT_DISTANCE: f64 = 3.5;
pub const BABY_FOLLOW_SPEED: f32 = 2.5;
pub const AGE_UP_TICKS: i32 = 10;
pub const HEAL_AMOUNT: f32 = 2.0;
pub const FOOD_TAG: &str = "minecraft:camel_food";
pub const HUSK_FOOD_TAG: &str = "minecraft:camel_husk_food";
pub const SAND_STEP_TAG: &str = "minecraft:camel_sand_step_sound_blocks";
pub const SPAWNABLE_ON_TAG: &str = "minecraft:camels_spawnable_on";
pub const BABY_SCALE: f32 = 0.6;
pub const MAX_PASSENGERS: usize = 3;
pub const MAX_PLAYER_PASSENGERS: usize = 2;
const ANCHOR_BASE: f64 = 0.375;
const FRONT_PASSENGER_Z: f32 = 0.5;
const BACK_PASSENGER_Z: f32 = -0.7;
const ANIMAL_PASSENGER_Z: f32 = 0.2;
const REMOVED_ANCHOR: f32 = 0.01;
const ADULT_SITTING_EYE: f32 = 0.845;
const BABY_STANDING: (f32, f32, f32) = (0.95, 1.4, 1.38);
const BABY_SITTING: (f32, f32, f32) = (0.95, 0.425, 0.41);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CamelState {
    pub dash_cooldown: i32,
    pub fresh: bool,
}

pub fn is_camel(kind: EntityKind) -> bool {
    kind.extends("Camel")
}

pub fn is_husk(kind: EntityKind) -> bool {
    kind == EntityKind::CamelHusk
}

pub fn define(base: &mut EntityBase) -> CamelState {
    base.data.define(DATA_DASH, SyncedValue::Boolean(false));
    base.data
        .define(DATA_LAST_POSE_CHANGE_TICK, SyncedValue::Long(0));
    CamelState::default()
}

pub fn is_dashing(base: &EntityBase) -> bool {
    base.data.boolean(DATA_DASH)
}

pub fn set_dashing(base: &mut EntityBase, dashing: bool) {
    base.data.set(DATA_DASH, SyncedValue::Boolean(dashing));
}

pub fn last_pose_change_tick(base: &EntityBase) -> i64 {
    base.data.long(DATA_LAST_POSE_CHANGE_TICK)
}

pub fn reset_last_pose_change_tick(base: &mut EntityBase, tick: i64) {
    base.data
        .set(DATA_LAST_POSE_CHANGE_TICK, SyncedValue::Long(tick));
}

pub fn reset_to_full_stand(base: &mut EntityBase, game_time: i64) {
    reset_last_pose_change_tick(base, (game_time - FULL_STAND_TICKS).max(0));
}

pub fn is_sitting(base: &EntityBase) -> bool {
    last_pose_change_tick(base) < 0
}

pub fn pose_time(base: &EntityBase, game_time: i64) -> i64 {
    game_time - last_pose_change_tick(base).abs()
}

pub fn is_in_pose_transition(base: &EntityBase, game_time: i64) -> bool {
    let limit = if is_sitting(base) {
        SIT_DOWN_TICKS
    } else {
        STAND_UP_TICKS
    };
    pose_time(base, game_time) < limit
}

pub fn is_visually_sitting(base: &EntityBase, game_time: i64) -> bool {
    (pose_time(base, game_time) < 0) != is_sitting(base)
}

pub fn is_visually_sitting_down(base: &EntityBase, game_time: i64) -> bool {
    let time = pose_time(base, game_time);
    is_sitting(base) && (0..SIT_DOWN_TICKS).contains(&time)
}

pub fn refuse_to_move(base: &EntityBase, game_time: i64) -> bool {
    is_sitting(base) || is_in_pose_transition(base, game_time)
}

fn set_pose(base: &mut EntityBase, pose: Pose, baby: bool) {
    base.set_pose(pose);
    ageable::refresh_dimensions(base, baby);
}

pub fn sit_down(base: &mut EntityBase, baby: bool, game_time: i64) -> bool {
    if is_sitting(base) {
        return false;
    }
    set_pose(base, Pose::Sitting, baby);
    reset_last_pose_change_tick(base, -game_time);
    true
}

pub fn stand_up(base: &mut EntityBase, baby: bool, game_time: i64) -> bool {
    if !is_sitting(base) {
        return false;
    }
    set_pose(base, Pose::Standing, baby);
    reset_last_pose_change_tick(base, game_time);
    true
}

pub fn stand_up_instantly(base: &mut EntityBase, baby: bool, game_time: i64) {
    set_pose(base, Pose::Standing, baby);
    reset_to_full_stand(base, game_time);
}

pub fn sitting_dimensions(adult: (f32, f32), baby: bool) -> (f32, f32, f32) {
    if baby {
        BABY_SITTING
    } else {
        (adult.0, adult.1 - SITTING_DROP, ADULT_SITTING_EYE)
    }
}

pub fn baby_standing_dimensions() -> (f32, f32, f32) {
    BABY_STANDING
}

pub fn age_scale(baby: bool) -> f32 {
    if baby { BABY_SCALE } else { 1.0 }
}

pub fn read(nbt: &Compound, base: &mut EntityBase, baby: bool) {
    let tick = nbt.get_i64("LastPoseTick").unwrap_or(0);
    if tick < 0 {
        set_pose(base, Pose::Sitting, baby);
    }
    reset_last_pose_change_tick(base, tick);
}

pub fn write(base: &EntityBase, out: &mut Compound) {
    out.insert("LastPoseTick", last_pose_change_tick(base));
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    reason: SpawnReason,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    if let Some(Species::Camel(state)) = living.mob.as_mut().map(|mob| &mut mob.species) {
        state.fresh = true;
    }
    let result = equine::finalize(base, living, reason, group, level);
    if is_husk(base.kind)
        && let Some(age) = living.mob.as_mut().and_then(|mob| mob.ageable.as_mut())
        && age.age < 0
    {
        ageable::set_age(base, age, 0);
    }
    result
}

pub fn can_mate(kind: EntityKind, partner: EntityKind) -> bool {
    is_camel(kind) && !is_husk(kind) && is_camel(partner)
}

pub fn can_fall_in_love(kind: EntityKind) -> bool {
    !is_husk(kind)
}

pub fn can_be_baby(kind: EntityKind) -> bool {
    !is_husk(kind)
}

pub fn food_tag(kind: EntityKind) -> &'static str {
    if is_husk(kind) {
        HUSK_FOOD_TAG
    } else {
        FOOD_TAG
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
    pub scale: f32,
    pub front: bool,
    pub game_time: i64,
}

fn anchor_offset(state: &Anchor, height: f32, sitting: bool, pose_time: i64) -> f64 {
    let mut offset = f64::from(height) - ANCHOR_BASE;
    let scale = state.scale;
    let drop = scale * SITTING_DROP;
    let shallow = drop - scale * 0.2;
    let rest = drop - shallow;
    let transition = pose_time
        < if sitting {
            SIT_DOWN_TICKS
        } else {
            STAND_UP_TICKS
        };
    if transition {
        let total = if sitting {
            SIT_DOWN_TICKS
        } else {
            STAND_UP_TICKS
        } as i32;
        let (key, factor): (i32, f32) = match (sitting, state.front) {
            (true, true) => (28, 0.5),
            (true, false) => (24, 0.1),
            (false, true) => (24, 0.6),
            (false, false) => (32, 0.35),
        };
        let time = mth::clamp(pose_time as f32, 0.0, total as f32);
        let first = time < key as f32;
        let progress = if first {
            time / key as f32
        } else {
            (time - key as f32) / (total - key) as f32
        };
        let eased = drop - factor * shallow;
        offset += f64::from(if sitting {
            mth::lerp(
                f64::from(progress),
                f64::from(if first { drop } else { eased }),
                f64::from(if first { eased } else { rest }),
            ) as f32
        } else {
            mth::lerp(
                f64::from(progress),
                f64::from(if first { rest - drop } else { rest - eased }),
                f64::from(if first { rest - eased } else { 0.0 }),
            ) as f32
        });
    }
    if sitting && !transition {
        offset += f64::from(rest);
    }
    offset
}

pub fn passenger_attachment(
    base: &EntityBase,
    anchor: &Anchor,
    index: usize,
    count: usize,
    animal: bool,
    removed: bool,
) -> Vec3 {
    let front = index == 0;
    let state = Anchor { front, ..*anchor };
    let y = if removed {
        f64::from(REMOVED_ANCHOR)
    } else {
        anchor_offset(
            &state,
            base.dimensions.height,
            is_sitting(base),
            pose_time(base, anchor.game_time),
        )
    };
    let mut z = FRONT_PASSENGER_Z;
    if count > 1 {
        if !front {
            z = BACK_PASSENGER_Z;
        }
        if animal {
            z += ANIMAL_PASSENGER_Z;
        }
    }
    let point = Vec3::new(0.0, y, f64::from(z * anchor.scale));
    let angle = f64::from(-base.yaw * 0.017_453_292);
    let (sin, cos) = (f64::from(mth::sin(angle)), f64::from(mth::cos(angle)));
    Vec3::new(
        point.x * cos + point.z * sin,
        point.y,
        point.z * cos - point.x * sin,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EntityId;
    use uuid::Uuid;

    fn camel(kind: EntityKind) -> (EntityBase, CamelState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), kind);
        let living = LivingState::new(&mut base, 0.0);
        let state = match living.mob.as_ref().map(|mob| mob.species) {
            Some(crate::entity::species::Species::Camel(state)) => state,
            _ => panic!("a camel state"),
        };
        (base, state)
    }

    #[test]
    fn the_data_slots_follow_the_horse_flags() {
        let (base, state) = camel(EntityKind::Camel);
        assert_eq!(DATA_DASH, crate::entity::species::equine::DATA_FLAGS + 1);
        assert_eq!(DATA_LAST_POSE_CHANGE_TICK, DATA_DASH + 1);
        assert!(!is_dashing(&base));
        assert_eq!(state.dash_cooldown, 0);
        assert!(!is_sitting(&base));
    }

    #[test]
    fn sitting_down_and_standing_up_encode_the_tick_with_a_sign() {
        let (mut base, _) = camel(EntityKind::Camel);
        assert!(sit_down(&mut base, false, 500));
        assert_eq!(last_pose_change_tick(&base), -500);
        assert!(is_sitting(&base));
        assert_eq!(base.pose(), Pose::Sitting);
        assert!(!sit_down(&mut base, false, 501));
        assert!(stand_up(&mut base, false, 600));
        assert_eq!(last_pose_change_tick(&base), 600);
        assert_eq!(base.pose(), Pose::Standing);
        assert!(!stand_up(&mut base, false, 601));
    }

    #[test]
    fn a_pose_change_lasts_forty_ticks_sitting_and_fifty_two_standing() {
        let (mut base, _) = camel(EntityKind::Camel);
        sit_down(&mut base, false, 1000);
        assert!(is_in_pose_transition(&base, 1039));
        assert!(!is_in_pose_transition(&base, 1040));
        assert!(refuse_to_move(&base, 1100));
        stand_up(&mut base, false, 2000);
        assert!(is_in_pose_transition(&base, 2051));
        assert!(!is_in_pose_transition(&base, 2052));
        assert!(!refuse_to_move(&base, 2052));
    }

    #[test]
    fn standing_up_instantly_leaves_the_pose_fully_changed() {
        let (mut base, _) = camel(EntityKind::Camel);
        sit_down(&mut base, false, 100);
        stand_up_instantly(&mut base, false, 1000);
        assert_eq!(last_pose_change_tick(&base), 1000 - FULL_STAND_TICKS);
        assert!(!refuse_to_move(&base, 1000));
        stand_up_instantly(&mut base, false, 10);
        assert_eq!(last_pose_change_tick(&base), 0);
    }

    #[test]
    fn the_sitting_body_is_one_point_forty_three_lower_and_a_baby_has_fixed_sizes() {
        let (mut base, _) = camel(EntityKind::Camel);
        let standing = base.dimensions;
        sit_down(&mut base, false, 10);
        assert!((standing.height - base.dimensions.height - 1.43).abs() < 1e-5);
        assert_eq!(base.dimensions.eye_height, 0.845);
        assert_eq!(baby_standing_dimensions(), (0.95, 1.4, 1.38));
        assert_eq!(sitting_dimensions((1.7, 2.375), true), (0.95, 0.425, 0.41));
    }

    #[test]
    fn the_pose_tick_round_trips_in_the_vanilla_layout() {
        let (mut base, _) = camel(EntityKind::Camel);
        sit_down(&mut base, false, 777);
        let mut out = Compound::new();
        write(&base, &mut out);
        assert_eq!(out.get_i64("LastPoseTick"), Some(-777));
        let (mut again, _) = camel(EntityKind::Camel);
        read(&out, &mut again, false);
        assert!(is_sitting(&again));
        assert_eq!(again.pose(), Pose::Sitting);
        let empty = Compound::new();
        let (mut other, _) = camel(EntityKind::Camel);
        read(&empty, &mut other, false);
        assert!(!is_sitting(&other));
        assert_eq!(last_pose_change_tick(&other), 0);
    }

    #[test]
    fn two_passengers_sit_front_and_back_and_a_husk_never_breeds() {
        let (base, _) = camel(EntityKind::Camel);
        let anchor = Anchor {
            scale: 1.0,
            front: true,
            game_time: 10_000,
        };
        let front = passenger_attachment(&base, &anchor, 0, 2, false, false);
        let back = passenger_attachment(&base, &anchor, 1, 2, false, false);
        let animal = passenger_attachment(&base, &anchor, 1, 2, true, false);
        assert!((front.z - 0.5).abs() < 1e-5);
        assert!((back.z + 0.7).abs() < 1e-5);
        assert!((animal.z + 0.5).abs() < 1e-5);
        assert!((front.y - 2.0).abs() < 1e-5);
        let alone = passenger_attachment(&base, &anchor, 1, 1, false, false);
        assert!((alone.z - 0.5).abs() < 1e-5);
        assert!(!can_mate(EntityKind::CamelHusk, EntityKind::Camel));
        assert!(can_mate(EntityKind::Camel, EntityKind::Camel));
        assert!(!can_fall_in_love(EntityKind::CamelHusk));
        assert!(!can_be_baby(EntityKind::CamelHusk));
    }

    #[test]
    fn a_seated_passenger_rides_lower_once_the_camel_has_sat() {
        let (mut base, _) = camel(EntityKind::Camel);
        sit_down(&mut base, false, 10_000);
        let anchor = Anchor {
            scale: 1.0,
            front: true,
            game_time: 10_100,
        };
        let seated = passenger_attachment(&base, &anchor, 0, 1, false, false);
        assert!((seated.y - (2.375 - 1.43 - 0.375 + 0.2)).abs() < 1e-5);
    }
}
