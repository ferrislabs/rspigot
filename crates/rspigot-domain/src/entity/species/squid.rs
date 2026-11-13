use rspigot_nbt::Compound;

use crate::Vec3;
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::{self, LivingState};
use crate::entity::spawn::{SpawnEnvironment, ageable_finalize};
use crate::mth;
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const DATA_DARK_TICKS: u8 = 18;
pub const DARK_TICKS_ON_HURT: i32 = 100;
pub const TENTACLE_EVENT: i8 = 19;
pub const BABY_CHANCE: f32 = 0.05;
pub const INK_PARTICLES: i32 = 30;
pub const INK_RADIUS_SQR: f64 = 100.0;
pub const FLEE_PUSH: f64 = 3.0;
pub const FLEE_PUSH_FALLOFF_DISTANCE: f64 = 5.0;
pub const FLEE_DIVISOR: f64 = 20.0;
pub const SOUND_VOLUME: f32 = 0.4;
pub const IDLE_TICKS: i32 = 100;
pub const KEYS: &[&str] = &[];
pub const GLOW_KEYS: &[&str] = &["DarkTicksRemaining"];
const TWO_PI: f32 = std::f32::consts::TAU;
const PI: f32 = std::f32::consts::PI;
const DEG: f32 = 180.0 / PI;
const SPEED_BASE: f32 = 0.2;
const WATER_DRAG: f64 = 0.9;
const AIR_DRAG: f32 = 0.98;
const LEVITATION_STEP: f64 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SquidState {
    pub movement: Vec3,
    pub tentacle_movement: f32,
    pub old_tentacle_movement: f32,
    pub tentacle_angle: f32,
    pub old_tentacle_angle: f32,
    pub tentacle_speed: f32,
    pub rotate_speed: f32,
    pub x_body_rot: f32,
    pub x_body_rot_old: f32,
    pub z_body_rot: f32,
    pub z_body_rot_old: f32,
    pub y_body_rot_old: f32,
    pub was_touching_water: bool,
}

impl Default for SquidState {
    fn default() -> Self {
        Self {
            movement: Vec3::ZERO,
            tentacle_movement: 0.0,
            old_tentacle_movement: 0.0,
            tentacle_angle: 0.0,
            old_tentacle_angle: 0.0,
            tentacle_speed: 0.0,
            rotate_speed: 0.0,
            x_body_rot: 0.0,
            x_body_rot_old: 0.0,
            z_body_rot: 0.0,
            z_body_rot_old: 0.0,
            y_body_rot_old: 0.0,
            was_touching_water: false,
        }
    }
}

impl SquidState {
    pub fn has_movement_vector(&self) -> bool {
        self.movement.length_sqr() > f64::from(1.0e-5_f32)
    }
}

pub fn define(base: &mut EntityBase) -> SquidState {
    if base.kind == rspigot_entities::EntityKind::GlowSquid {
        base.data.define(DATA_DARK_TICKS, SyncedValue::Int(0));
    }
    SquidState::default()
}

pub fn dark_ticks(base: &EntityBase) -> i32 {
    base.data.int(DATA_DARK_TICKS)
}

pub fn set_dark_ticks(base: &mut EntityBase, ticks: i32) {
    base.data.set(DATA_DARK_TICKS, SyncedValue::Int(ticks));
}

pub fn keys(kind: rspigot_entities::EntityKind) -> &'static [&'static str] {
    if kind == rspigot_entities::EntityKind::GlowSquid {
        GLOW_KEYS
    } else {
        KEYS
    }
}

pub fn random_tentacle_speed(random: &mut impl RandomSource) -> f32 {
    1.0 / (random.next_float() + 1.0) * SPEED_BASE
}

pub fn read(nbt: &Compound, base: &mut EntityBase, _state: &mut SquidState) {
    if base.kind == rspigot_entities::EntityKind::GlowSquid {
        set_dark_ticks(base, nbt.get_i32("DarkTicksRemaining").unwrap_or(0));
    }
}

pub fn write(base: &EntityBase, _state: &SquidState, out: &mut Compound) {
    if base.kind == rspigot_entities::EntityKind::GlowSquid {
        out.insert("DarkTicksRemaining", dark_ticks(base));
    }
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    _environment: &SpawnEnvironment,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    let group = group.unwrap_or(SpawnGroupData::Ageable {
        group_size: 0,
        should_spawn_baby: true,
        baby_chance: BABY_CHANCE,
    });
    ageable_finalize(base, living, Some(group), level)
}

pub fn ai_step(
    base: &mut EntityBase,
    living: &mut LivingState,
    state: &mut SquidState,
    random: &mut impl RandomSource,
) -> bool {
    if state.tentacle_speed == 0.0 {
        state.tentacle_speed = random_tentacle_speed(random);
    }
    state.x_body_rot_old = state.x_body_rot;
    state.z_body_rot_old = state.z_body_rot;
    state.y_body_rot_old = living.body_yaw;
    state.old_tentacle_movement = state.tentacle_movement;
    state.old_tentacle_angle = state.tentacle_angle;
    state.tentacle_movement += state.tentacle_speed;
    let mut wrapped = false;
    if state.tentacle_movement > TWO_PI {
        state.tentacle_movement -= TWO_PI;
        if random.next_int(10) == 0 {
            state.tentacle_speed = random_tentacle_speed(random);
        }
        wrapped = true;
    }
    let authoritative = living::effective_ai(base, living);
    if base.fluid.in_water {
        if state.tentacle_movement < PI {
            let phase = state.tentacle_movement / PI;
            state.tentacle_angle = mth::sin(f64::from(phase * phase * PI)) * PI * 0.25;
            if phase > 0.75 {
                if authoritative {
                    base.velocity = state.movement;
                }
                state.rotate_speed = 1.0;
            } else {
                state.rotate_speed *= 0.8;
            }
        } else {
            state.tentacle_angle = 0.0;
            if authoritative {
                base.velocity = base.velocity.scale(WATER_DRAG);
            }
            state.rotate_speed *= 0.99;
        }
        let velocity = base.velocity;
        let horizontal = velocity.horizontal_distance_sqr().sqrt();
        living.body_yaw +=
            (-(mth::atan2(velocity.x, velocity.z) as f32) * DEG - living.body_yaw) * 0.1;
        base.set_yaw(living.body_yaw);
        state.z_body_rot += PI * state.rotate_speed * 1.5;
        state.x_body_rot +=
            (-(mth::atan2(horizontal, velocity.y) as f32) * DEG - state.x_body_rot) * 0.1;
    } else {
        state.tentacle_angle = mth::sin(f64::from(state.tentacle_movement)).abs() * PI * 0.25;
        if authoritative {
            let fall = match living.effect("minecraft:levitation").map(|e| e.amplifier) {
                Some(amplifier) => LEVITATION_STEP * (f64::from(amplifier) + 1.0),
                None => base.velocity.y - living.effective_gravity(base),
            };
            base.velocity = Vec3::new(0.0, fall * f64::from(AIR_DRAG), 0.0);
        }
        state.x_body_rot += (-90.0 - state.x_body_rot) * 0.02;
    }
    wrapped
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::save::{load_entity, save_entity};
    use crate::random::LegacyRandom;
    use rspigot_entities::EntityKind;

    fn squid() -> (EntityBase, LivingState, SquidState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Squid);
        let living = LivingState::new(&mut base, 0.0);
        (base, living, SquidState::default())
    }

    #[test]
    fn an_adult_squid_is_the_default_box_and_a_baby_a_half_block_one() {
        let adult = EntityKind::Squid.dimensions();
        assert_eq!(
            (adult.width, adult.height, adult.eye_height),
            (0.8, 0.8, 0.4)
        );
        let baby = crate::entity::ageable::dimensions(EntityKind::Squid, true);
        assert_eq!((baby.width, baby.height, baby.eye_height), (0.5, 0.5, 0.37));
        let glow = crate::entity::ageable::dimensions(EntityKind::GlowSquid, true);
        assert_eq!((glow.width, glow.height, glow.eye_height), (0.5, 0.5, 0.37));
    }

    #[test]
    fn a_wet_squid_in_its_push_phase_takes_the_movement_vector_and_spins_up() {
        let (mut base, mut living, mut state) = squid();
        base.fluid.in_water = true;
        state.tentacle_speed = 0.2;
        state.tentacle_movement = 2.6;
        state.movement = Vec3::new(0.1, 0.0, 0.0);
        let mut random = LegacyRandom::new(1);
        let wrapped = ai_step(&mut base, &mut living, &mut state, &mut random);
        assert!(!wrapped);
        assert_eq!(state.tentacle_movement, 2.8);
        assert_eq!(base.velocity, Vec3::new(0.1, 0.0, 0.0));
        assert_eq!(state.rotate_speed, 1.0);
        let phase = 2.8f32 / PI;
        assert_eq!(
            state.tentacle_angle,
            mth::sin(f64::from(phase * phase * PI)) * PI * 0.25
        );
        assert_eq!(state.z_body_rot, PI * 1.0 * 1.5);
        let yaw = (-(mth::atan2(0.1, 0.0) as f32) * DEG - 0.0) * 0.1;
        assert_eq!(living.body_yaw, yaw);
        assert_eq!(base.yaw, yaw);
    }

    #[test]
    fn a_wet_squid_glides_and_slows_in_the_first_half_of_the_cycle() {
        let (mut base, mut living, mut state) = squid();
        base.fluid.in_water = true;
        state.tentacle_speed = 0.1;
        state.tentacle_movement = 0.5;
        state.rotate_speed = 1.0;
        base.velocity = Vec3::new(0.2, 0.0, 0.0);
        let mut random = LegacyRandom::new(1);
        ai_step(&mut base, &mut living, &mut state, &mut random);
        assert_eq!(state.rotate_speed, 0.8);
        assert_eq!(base.velocity, Vec3::new(0.2, 0.0, 0.0));
        state.tentacle_movement = 3.2;
        ai_step(&mut base, &mut living, &mut state, &mut random);
        assert_eq!(state.tentacle_angle, 0.0);
        assert_eq!(base.velocity.x, 0.2 * 0.9);
        assert_eq!(state.rotate_speed, 0.8 * 0.99);
    }

    #[test]
    fn a_dry_squid_falls_with_gravity_and_air_drag_and_points_its_head_down() {
        let (mut base, mut living, mut state) = squid();
        state.tentacle_speed = 0.1;
        base.velocity = Vec3::new(0.3, -0.1, 0.2);
        let mut random = LegacyRandom::new(1);
        ai_step(&mut base, &mut living, &mut state, &mut random);
        let fall = (-0.1 - living.effective_gravity(&base)) * f64::from(0.98_f32);
        assert_eq!(base.velocity, Vec3::new(0.0, fall, 0.0));
        assert_eq!(state.x_body_rot, -90.0 * 0.02);
        assert_eq!(
            state.tentacle_angle,
            mth::sin(f64::from(0.1_f32)).abs() * PI * 0.25
        );
    }

    #[test]
    fn a_squid_without_ai_keeps_its_velocity() {
        let (mut base, mut living, mut state) = squid();
        living::set_no_ai(&mut base, true);
        base.fluid.in_water = true;
        state.tentacle_speed = 0.2;
        state.tentacle_movement = 2.6;
        state.movement = Vec3::new(0.1, 0.0, 0.0);
        base.velocity = Vec3::new(0.0, 0.5, 0.0);
        let mut random = LegacyRandom::new(1);
        ai_step(&mut base, &mut living, &mut state, &mut random);
        assert_eq!(base.velocity, Vec3::new(0.0, 0.5, 0.0));
    }

    #[test]
    fn the_tentacles_wrap_with_the_event_and_sometimes_a_new_speed() {
        let (mut base, mut living, mut state) = squid();
        state.tentacle_speed = 0.3;
        state.tentacle_movement = 6.1;
        let mut random = LegacyRandom::new(9);
        let mut mirror = LegacyRandom::new(9);
        assert!(ai_step(&mut base, &mut living, &mut state, &mut random));
        assert!((state.tentacle_movement - (6.1 + 0.3 - TWO_PI)).abs() < 1.0e-6);
        if mirror.next_int(10) == 0 {
            assert_ne!(state.tentacle_speed, 0.3);
        } else {
            assert_eq!(state.tentacle_speed, 0.3);
        }
        let mut lazy = SquidState::default();
        let mut random = LegacyRandom::new(4);
        let mut mirror = LegacyRandom::new(4);
        ai_step(&mut base, &mut living, &mut lazy, &mut random);
        assert_eq!(lazy.tentacle_speed, random_tentacle_speed(&mut mirror));
        assert!((0.1..=0.2).contains(&lazy.tentacle_speed));
    }

    #[test]
    fn a_glow_squid_saves_its_dark_ticks_and_a_squid_saves_nothing_extra() {
        let glow = load_entity(
            &Compound::new()
                .with("id", "minecraft:glow_squid")
                .with("DarkTicksRemaining", 40),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap();
        assert_eq!(dark_ticks(&glow.base), 40);
        assert_eq!(
            glow.base.data.get(DATA_DARK_TICKS),
            Some(&SyncedValue::Int(40))
        );
        let saved = save_entity(&glow, None).unwrap();
        assert_eq!(saved.get_i32("DarkTicksRemaining"), Some(40));
        let plain = load_entity(
            &Compound::new().with("id", "minecraft:squid"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap();
        let saved = save_entity(&plain, None).unwrap();
        assert_eq!(saved.get_i32("DarkTicksRemaining"), None);
    }

    #[test]
    fn spawned_squids_start_a_baby_group_with_a_five_percent_chance() {
        let environment = SpawnEnvironment {
            biome: crate::BiomeId(0),
            difficulty: crate::difficulty::DifficultyInstance::new(
                crate::world::Difficulty::Normal,
                0,
                0,
                1.0,
            ),
            halloween: false,
            moon_brightness: 0.0,
        };
        let mut level = LegacyRandom::new(3);
        let mut entity = load_entity(
            &Compound::new().with("id", "minecraft:squid"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap();
        let mut living = entity.living_mut().unwrap().clone();
        let group = finalize(
            &mut entity.base,
            &mut living,
            &environment,
            None,
            &mut level,
        );
        assert_eq!(
            group,
            Some(SpawnGroupData::Ageable {
                group_size: 1,
                should_spawn_baby: true,
                baby_chance: 0.05
            })
        );
        assert!(
            !living
                .mob
                .as_ref()
                .and_then(|mob| mob.ageable.as_ref())
                .is_some_and(|ageable| ageable.age < 0)
        );
    }
}
