use std::num::NonZeroU8;

use rspigot_items::ItemKind;
use uuid::Uuid;

use super::aabb::Aabb;
use super::base::EntityBase;
use super::data::SyncedValue;
use super::physics::BlockView;
use super::raycast;
use crate::item::ItemStack;
use crate::random::{LegacyRandom, RandomSource};
use crate::{BlockStateId, Vec3, mth};

pub const DATA_FLAGS: u8 = 8;
pub const DATA_PIERCE_LEVEL: u8 = 9;
pub const DATA_IN_GROUND: u8 = 10;
pub const DATA_EFFECT_COLOR: u8 = 11;
pub const DATA_LOYALTY: u8 = 11;
pub const DATA_FOIL: u8 = 12;
pub const TRIDENT_DAMAGE: f32 = 8.0;
pub const TRIDENT_WATER_INERTIA: f32 = 0.99;
pub const TRIDENT_HIT_SOUND: &str = "minecraft:item.trident.hit";
pub const TRIDENT_GROUND_SOUND: &str = "minecraft:item.trident.hit_ground";
pub const TRIDENT_DEFLECT: f64 = -0.5;
pub const TRIDENT_AFTER_HIT: (f64, f64, f64) = (0.02, 0.2, 0.02);
pub const TRIDENT_GROUND_TICKS: i32 = 4;
pub const FLAG_CRIT: u8 = 1;
pub const FLAG_NO_PHYSICS: u8 = 2;
pub const BASE_DAMAGE: f64 = 2.0;
pub const SHAKE_TIME: i32 = 7;
pub const WATER_INERTIA: f32 = 0.6;
pub const AIR_DRAG: f32 = 0.99;
pub const GRAVITY: f64 = 0.05;
pub const DESPAWN_LIFE: i32 = 1200;
pub const HIT_SOUND: &str = "minecraft:entity.arrow.hit";
pub const HIT_PLAYER_EVENT: i8 = 6;
const SHOOT_SPREAD: f64 = 0.017_227_5;
const FALL_PROBE: f64 = 0.06;
const BLOCK_PULLBACK: f64 = 0.050_000_000_745_058_06;
const MOB_DAMAGE_SPREAD: f64 = 0.574_25;
const MOB_DAMAGE_PER_DIFFICULTY: f64 = 0.11;
const BOUNCE_SCALE: f64 = 0.2;
const ROTATION_LERP: f32 = 0.2;
const DEGREES_TO_RADIANS: f32 = 0.017_453_292;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pickup {
    #[default]
    Disallowed,
    Allowed,
    CreativeOnly,
}

impl Pickup {
    pub const fn id(self) -> i8 {
        match self {
            Self::Disallowed => 0,
            Self::Allowed => 1,
            Self::CreativeOnly => 2,
        }
    }

    pub const fn from_id(id: i8) -> Self {
        match id {
            1 => Self::Allowed,
            2 => Self::CreativeOnly,
            _ => Self::Disallowed,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArrowState {
    pub owner: Option<Uuid>,
    pub left_owner: bool,
    pub left_owner_checked: bool,
    pub has_been_shot: bool,
    pub life: i32,
    pub shake_time: i32,
    pub in_ground_time: i32,
    pub last_state: Option<BlockStateId>,
    pub pickup: Pickup,
    pub base_damage: f64,
    pub sound: String,
    pub item: ItemStack,
    pub weapon: Option<ItemStack>,
    pub pierced: Vec<crate::EntityId>,
    pub dealt_damage: bool,
    pub random: LegacyRandom,
}

pub fn trident_item() -> ItemStack {
    ItemStack::new(ItemKind::Trident.id(), NonZeroU8::MIN)
}

pub fn default_item() -> ItemStack {
    ItemStack::new(ItemKind::Arrow.id(), NonZeroU8::MIN)
}

impl ArrowState {
    pub fn new(base: &mut EntityBase, seed: i64) -> Self {
        base.data.define(DATA_FLAGS, SyncedValue::Byte(0));
        base.data.define(DATA_PIERCE_LEVEL, SyncedValue::Byte(0));
        base.data
            .define(DATA_IN_GROUND, SyncedValue::Boolean(false));
        base.data.define(DATA_EFFECT_COLOR, SyncedValue::Int(-1));
        Self::unsynced(seed)
    }

    pub fn new_trident(base: &mut EntityBase, seed: i64) -> Self {
        base.data.define(DATA_FLAGS, SyncedValue::Byte(0));
        base.data.define(DATA_PIERCE_LEVEL, SyncedValue::Byte(0));
        base.data
            .define(DATA_IN_GROUND, SyncedValue::Boolean(false));
        base.data.define(DATA_LOYALTY, SyncedValue::Byte(0));
        base.data.define(DATA_FOIL, SyncedValue::Boolean(false));
        Self {
            sound: TRIDENT_GROUND_SOUND.to_owned(),
            item: trident_item(),
            ..Self::unsynced(seed)
        }
    }

    fn unsynced(seed: i64) -> Self {
        Self {
            owner: None,
            left_owner: false,
            left_owner_checked: false,
            has_been_shot: false,
            life: 0,
            shake_time: 0,
            in_ground_time: 0,
            last_state: None,
            pickup: Pickup::Disallowed,
            base_damage: BASE_DAMAGE,
            sound: HIT_SOUND.to_owned(),
            item: default_item(),
            weapon: None,
            pierced: Vec::new(),
            dealt_damage: false,
            random: LegacyRandom::new(seed),
        }
    }

    pub fn set_owner(&mut self, owner: Option<Uuid>, player: bool) {
        self.owner = owner;
        if player && self.pickup == Pickup::Disallowed {
            self.pickup = Pickup::Allowed;
        }
    }
}

pub fn in_ground(base: &EntityBase) -> bool {
    base.data.boolean(DATA_IN_GROUND)
}

pub fn set_in_ground(base: &mut EntityBase, value: bool) {
    base.data.set(DATA_IN_GROUND, SyncedValue::Boolean(value));
}

pub fn is_crit(base: &EntityBase) -> bool {
    base.data.flag(DATA_FLAGS, FLAG_CRIT)
}

pub fn set_crit(base: &mut EntityBase, crit: bool) {
    base.data.set_flag(DATA_FLAGS, FLAG_CRIT, crit);
}

pub fn pierce_level(base: &EntityBase) -> i8 {
    base.data.byte(DATA_PIERCE_LEVEL)
}

pub fn set_pierce_level(base: &mut EntityBase, level: i8) {
    base.data.set(DATA_PIERCE_LEVEL, SyncedValue::Byte(level));
}

pub fn lerp_rotation(current: f32, target: f32) -> f32 {
    let mut current = current;
    while target - current < -180.0 {
        current -= 360.0;
    }
    while target - current >= 180.0 {
        current += 360.0;
    }
    current + ROTATION_LERP * (target - current)
}

pub fn shoot(
    base: &mut EntityBase,
    arrow: &mut ArrowState,
    direction: Vec3,
    power: f32,
    uncertainty: f32,
) {
    let spread = SHOOT_SPREAD * f64::from(uncertainty);
    let random = &mut arrow.random;
    let jitter = Vec3::new(
        random.triangle(0.0, spread),
        random.triangle(0.0, spread),
        random.triangle(0.0, spread),
    );
    let movement = (direction.normalize() + jitter).scale(f64::from(power));
    base.velocity = movement;
    base.needs_sync = true;
    let horizontal = (movement.x * movement.x + movement.z * movement.z).sqrt();
    base.set_yaw((mth::atan2(movement.x, movement.z) * mth::RAD_TO_DEG) as f32);
    base.set_pitch((mth::atan2(movement.y, horizontal) * mth::RAD_TO_DEG) as f32);
    arrow.life = 0;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Launch {
    pub pitch: f32,
    pub yaw: f32,
    pub roll: f32,
    pub power: f32,
    pub uncertainty: f32,
    pub shooter_movement: Vec3,
    pub shooter_on_ground: bool,
}

pub fn shoot_from_rotation(base: &mut EntityBase, arrow: &mut ArrowState, launch: Launch) {
    let radians = |angle: f32| f64::from(angle * DEGREES_TO_RADIANS);
    let direction = Vec3::new(
        f64::from(-mth::sin(radians(launch.yaw)) * mth::cos(radians(launch.pitch))),
        f64::from(-mth::sin(radians(launch.pitch + launch.roll))),
        f64::from(mth::cos(radians(launch.yaw)) * mth::cos(radians(launch.pitch))),
    );
    shoot(base, arrow, direction, launch.power, launch.uncertainty);
    let movement = launch.shooter_movement;
    base.velocity = base.velocity
        + Vec3::new(
            movement.x,
            if launch.shooter_on_ground {
                0.0
            } else {
                movement.y
            },
            movement.z,
        );
}

pub fn mob_base_damage(power: f32, difficulty: u8, random: &mut impl RandomSource) -> f64 {
    f64::from(power * 2.0)
        + random.triangle(
            f64::from(difficulty) * MOB_DAMAGE_PER_DIFFICULTY,
            MOB_DAMAGE_SPREAD,
        )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GroundTick {
    pub grounded: bool,
    pub despawn: bool,
    pub started_falling: bool,
}

fn should_fall(base: &EntityBase, view: &impl BlockView) -> bool {
    let probe = Aabb::new(
        base.position.x,
        base.position.y,
        base.position.z,
        base.position.x,
        base.position.y,
        base.position.z,
    )
    .inflate(FALL_PROBE);
    in_ground(base) && super::fluid::is_free_of_collision(view, &probe)
}

pub fn start_falling(base: &mut EntityBase, arrow: &mut ArrowState) {
    set_in_ground(base, false);
    let random = &mut arrow.random;
    let x = f64::from(random.next_float() * 0.2);
    let y = f64::from(random.next_float() * 0.2);
    let z = f64::from(random.next_float() * 0.2);
    base.velocity = base.velocity.multiply(x, y, z);
    arrow.life = 0;
}

pub fn ground_tick(
    base: &mut EntityBase,
    arrow: &mut ArrowState,
    view: &impl BlockView,
    despawn_life: i32,
) -> GroundTick {
    let physics = !base.no_physics;
    let pos = base.position.block();
    let state = view.block_or_air(pos);
    if !state.is_air() && physics {
        let origin = Vec3::new(f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
        let inside = state.collision_shape().iter().any(|b| {
            let moved =
                Aabb::new(b.min_x, b.min_y, b.min_z, b.max_x, b.max_y, b.max_z).move_by(origin);
            raycast::contains(&moved, base.position)
        });
        if inside {
            base.velocity = Vec3::ZERO;
            set_in_ground(base, true);
        }
    }
    if arrow.shake_time > 0 {
        arrow.shake_time -= 1;
    }
    let mut tick = GroundTick::default();
    if in_ground(base) && physics {
        tick.grounded = true;
        if arrow.last_state != Some(state) && should_fall(base, view) {
            start_falling(base, arrow);
            tick.started_falling = true;
        } else {
            arrow.life += 1;
            tick.despawn = arrow.life >= despawn_life;
        }
        arrow.in_ground_time += 1;
    } else {
        arrow.in_ground_time = 0;
    }
    tick
}

pub fn flight_rotation(base: &mut EntityBase, movement: Vec3) {
    let yaw = if base.no_physics {
        (mth::atan2(-movement.x, -movement.z) * mth::RAD_TO_DEG) as f32
    } else {
        (mth::atan2(movement.x, movement.z) * mth::RAD_TO_DEG) as f32
    };
    let horizontal = (movement.x * movement.x + movement.z * movement.z).sqrt();
    let pitch = (mth::atan2(movement.y, horizontal) * mth::RAD_TO_DEG) as f32;
    let next_pitch = lerp_rotation(base.pitch, pitch);
    let next_yaw = lerp_rotation(base.yaw, yaw);
    base.set_pitch(next_pitch);
    base.set_yaw(next_yaw);
}

pub fn hit_damage(base: &EntityBase, arrow: &mut ArrowState) -> i32 {
    let speed = base.velocity.length() as f32;
    let damage = (f64::from(speed) * arrow.base_damage)
        .clamp(0.0, 2.147_483_647e9)
        .ceil() as i32;
    if is_crit(base) {
        let bonus = i64::from(arrow.random.next_int(damage / 2 + 2));
        (bonus + i64::from(damage)).min(i64::from(i32::MAX)) as i32
    } else {
        damage
    }
}

pub fn hit_block(base: &mut EntityBase, arrow: &mut ArrowState, state: BlockStateId) -> f32 {
    arrow.last_state = Some(state);
    let movement = base.velocity;
    let direction = Vec3::new(
        movement.x.signum0(),
        movement.y.signum0(),
        movement.z.signum0(),
    );
    base.position = base.position - direction.scale(BLOCK_PULLBACK);
    base.velocity = Vec3::ZERO;
    let pitch = 1.2 / (arrow.random.next_float() * 0.2 + 0.9);
    set_in_ground(base, true);
    arrow.shake_time = SHAKE_TIME;
    set_crit(base, false);
    set_pierce_level(base, 0);
    arrow.sound = HIT_SOUND.to_owned();
    arrow.pierced.clear();
    pitch
}

pub fn bounce(base: &mut EntityBase) {
    base.velocity = base.velocity.scale(-BOUNCE_SCALE);
    base.needs_sync = true;
}

trait Signum0 {
    fn signum0(self) -> f64;
}

impl Signum0 for f64 {
    fn signum0(self) -> f64 {
        if self == 0.0 { 0.0 } else { self.signum() }
    }
}

pub fn search_box(base: &EntityBase) -> Aabb {
    base.bounding_box()
        .expand_towards(base.velocity)
        .inflate(1.0)
}

pub fn hit_margin(tick_count: i32) -> f32 {
    0.0f32.max(0.3f32.min((tick_count - 2) as f32 / 20.0))
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::EntityId;

    fn arrow() -> (EntityBase, ArrowState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Arrow);
        let state = ArrowState::new(&mut base, 9);
        (base, state)
    }

    #[test]
    fn shooting_sets_motion_rotation_and_resets_life() {
        let (mut base, mut state) = arrow();
        state.life = 50;
        shoot(&mut base, &mut state, Vec3::new(0.0, 0.0, 1.0), 1.6, 0.0);
        assert_eq!(base.velocity, Vec3::new(0.0, 0.0, f64::from(1.6f32)));
        assert_eq!(base.yaw, 0.0);
        assert_eq!(state.life, 0);
        assert!(base.data.non_default().is_empty());
    }

    fn launch(pitch: f32, yaw: f32) -> Launch {
        Launch {
            pitch,
            yaw,
            roll: 0.0,
            power: 3.0,
            uncertainty: 0.0,
            shooter_movement: Vec3::ZERO,
            shooter_on_ground: true,
        }
    }

    fn near(actual: Vec3, expected: Vec3) {
        assert!(
            (actual.x - expected.x).abs() < 1e-4
                && (actual.y - expected.y).abs() < 1e-4
                && (actual.z - expected.z).abs() < 1e-4,
            "{actual:?} is not {expected:?}"
        );
    }

    #[test]
    fn shooting_from_rotation_follows_yaw_and_pitch() {
        let (mut base, mut state) = arrow();
        shoot_from_rotation(&mut base, &mut state, launch(0.0, 0.0));
        near(base.velocity, Vec3::new(0.0, 0.0, 3.0));
        shoot_from_rotation(&mut base, &mut state, launch(0.0, 90.0));
        near(base.velocity, Vec3::new(-3.0, 0.0, 0.0));
        shoot_from_rotation(&mut base, &mut state, launch(-90.0, 0.0));
        near(base.velocity, Vec3::new(0.0, 3.0, 0.0));
        shoot_from_rotation(&mut base, &mut state, launch(90.0, 0.0));
        near(base.velocity, Vec3::new(0.0, -3.0, 0.0));
    }

    #[test]
    fn the_shooter_movement_is_added_but_not_its_climb_on_the_ground() {
        let (mut base, mut state) = arrow();
        let moving = Launch {
            shooter_movement: Vec3::new(0.2, 0.4, -0.1),
            ..launch(0.0, 0.0)
        };
        shoot_from_rotation(&mut base, &mut state, moving);
        near(base.velocity, Vec3::new(0.2, 0.0, 2.9));
        let airborne = Launch {
            shooter_on_ground: false,
            ..moving
        };
        shoot_from_rotation(&mut base, &mut state, airborne);
        near(base.velocity, Vec3::new(0.2, 0.4, 2.9));
    }

    #[test]
    fn damage_scales_with_speed_and_mob_difficulty() {
        let (mut base, mut state) = arrow();
        base.velocity = Vec3::new(0.0, 0.0, 1.5);
        state.base_damage = 2.0;
        assert_eq!(hit_damage(&base, &mut state), 3);
        let mut random = LegacyRandom::new(1);
        let damage = mob_base_damage(1.0, 2, &mut random);
        assert!((2.22 - 0.58..=2.22 + 0.58).contains(&damage));
    }

    #[test]
    fn blocks_stop_arrows_and_start_the_shake() {
        let (mut base, mut state) = arrow();
        base.position = Vec3::new(0.0, 64.0, 0.0);
        base.velocity = Vec3::new(1.0, -0.5, 0.0);
        set_crit(&mut base, true);
        hit_block(&mut base, &mut state, BlockStateId::AIR);
        assert!(in_ground(&base));
        assert_eq!(state.shake_time, SHAKE_TIME);
        assert!(!is_crit(&base));
        assert_eq!(base.velocity, Vec3::ZERO);
        assert!((base.position.x + BLOCK_PULLBACK).abs() < 1.0e-12);
        assert!((base.position.y - (64.0 + BLOCK_PULLBACK)).abs() < 1.0e-12);
    }

    #[test]
    fn hit_margin_grows_with_age() {
        assert_eq!(hit_margin(0), 0.0);
        assert_eq!(hit_margin(4), 0.1);
        assert_eq!(hit_margin(100), 0.3);
    }
}
