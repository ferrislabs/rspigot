use rspigot_entities::{EntityDimensions, EntityKind};
use uuid::Uuid;

use super::base::EntityBase;
use super::data::SyncedValue;
use crate::Vec3;
use crate::item::ItemStack;
use crate::particle::Particle;
use crate::random::RandomSource;

pub const DATA_BABY: u8 = 16;
pub const DATA_AGE_LOCKED: u8 = 17;
pub const BABY_START_AGE: i32 = -24_000;
pub const PARENT_AGE_AFTER_BREEDING: i32 = 6000;
pub const LOVE_TICKS: i32 = 600;
pub const DEFAULT_BABY_CHANCE: f32 = 0.05;
pub const SHEEP_REGROWTH_SECONDS: i32 = 60;
const FORCED_AGE_TICKS: i32 = 40;
const FORCED_AGE_PARTICLE_PERIOD: i32 = 4;
const AGE_LOCK_TICKS: i32 = 40;
const AGE_LOCK_PARTICLE_OFFSET: f32 = 0.2;
const LOVE_PARTICLE_PERIOD: i32 = 10;
const PARTICLE_SPEED: f64 = 0.02;
const FEEDING_SPEED_UP: f32 = 0.1;
const HAPPY_VILLAGER: &str = "minecraft:happy_villager";
const PAUSE_MOB_GROWTH: &str = "minecraft:pause_mob_growth";
const RESET_MOB_GROWTH: &str = "minecraft:reset_mob_growth";

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Love {
    pub ticks: i32,
    pub cause: Option<Uuid>,
    pub breed_item: Option<ItemStack>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AgeableState {
    pub age: i32,
    pub forced_age: i32,
    pub forced_age_timer: i32,
    pub age_lock_timer: i32,
    pub love: Option<Love>,
}

impl AgeableState {
    pub fn new(base: &mut EntityBase) -> Self {
        base.data.define(DATA_BABY, SyncedValue::Boolean(false));
        base.data
            .define(DATA_AGE_LOCKED, SyncedValue::Boolean(false));
        Self {
            love: base.kind.extends("Animal").then(Love::default),
            ..Self::default()
        }
    }

    pub fn in_love(&self) -> bool {
        self.love.as_ref().is_some_and(|love| love.ticks > 0)
    }

    pub fn can_fall_in_love(&self) -> bool {
        self.love.as_ref().is_some_and(|love| love.ticks <= 0)
    }

    pub fn set_in_love(&mut self, cause: Option<Uuid>) {
        if let Some(love) = &mut self.love {
            love.ticks = LOVE_TICKS;
            if cause.is_some() {
                love.cause = cause;
            }
        }
    }

    pub fn fall_in_love(&mut self, ticks: i32, cause: Option<Uuid>, item: Option<ItemStack>) {
        if let Some(love) = &mut self.love {
            love.ticks = ticks;
            if cause.is_some() {
                love.cause = cause;
            }
            love.breed_item = item;
        }
    }

    pub fn reset_love(&mut self) {
        if let Some(love) = &mut self.love {
            love.ticks = 0;
        }
    }
}

pub fn can_be_baby(kind: EntityKind) -> bool {
    kind.sounds().is_none_or(|sounds| sounds.can_be_baby)
}

pub fn is_baby(base: &EntityBase, state: &AgeableState) -> bool {
    can_be_baby(base.kind) && state.age < 0
}

pub fn is_age_locked(base: &EntityBase) -> bool {
    base.data.boolean(DATA_AGE_LOCKED)
}

pub fn set_age_locked(base: &mut EntityBase, locked: bool) {
    base.data.set(DATA_AGE_LOCKED, SyncedValue::Boolean(locked));
}

pub fn can_age_up(base: &EntityBase, state: &AgeableState) -> bool {
    is_baby(base, state) && !is_age_locked(base)
}

pub fn set_age(base: &mut EntityBase, state: &mut AgeableState, age: i32) -> bool {
    let old = state.age;
    state.age = age;
    if (old < 0 && age >= 0) || (old >= 0 && age < 0) {
        base.data.set(DATA_BABY, SyncedValue::Boolean(age < 0));
        refresh_dimensions(base, age < 0);
        return true;
    }
    false
}

pub fn set_baby(base: &mut EntityBase, state: &mut AgeableState, baby: bool) -> bool {
    if !can_be_baby(base.kind) {
        return false;
    }
    set_age(base, state, if baby { BABY_START_AGE } else { 0 })
}

pub fn age_up(base: &mut EntityBase, state: &mut AgeableState, seconds: i32, forced: bool) {
    let old = state.age;
    let age = old.wrapping_add(seconds.wrapping_mul(20)).min(0);
    let delta = age.wrapping_sub(old);
    set_age(base, state, age);
    if forced {
        state.forced_age = state.forced_age.wrapping_add(delta);
        if state.forced_age_timer == 0 {
            state.forced_age_timer = FORCED_AGE_TICKS;
        }
    }
    if state.age == 0 {
        let forced_age = state.forced_age;
        set_age(base, state, forced_age);
    }
}

pub fn speed_up_seconds_when_feeding(ticks_until_adult: i32) -> i32 {
    ((ticks_until_adult / 20) as f32 * FEEDING_SPEED_UP) as i32
}

pub fn toggle_age_lock(base: &mut EntityBase, state: &mut AgeableState) -> bool {
    let locked = !is_age_locked(base);
    set_age_locked(base, locked);
    set_age(base, state, BABY_START_AGE);
    state.age_lock_timer = AGE_LOCK_TICKS;
    locked
}

pub fn random_x(base: &EntityBase, scale: f64, random: &mut impl RandomSource) -> f64 {
    base.position.x
        + f64::from(base.dimensions.width) * ((2.0 * random.next_double() - 1.0) * scale)
}

pub fn random_y(base: &EntityBase, random: &mut impl RandomSource) -> f64 {
    base.position.y + f64::from(base.dimensions.height) * random.next_double()
}

pub fn random_y_scaled(base: &EntityBase, scale: f64, random: &mut impl RandomSource) -> f64 {
    base.position.y + f64::from(base.dimensions.height) * scale * random.next_double()
}

pub fn random_z(base: &EntityBase, scale: f64, random: &mut impl RandomSource) -> f64 {
    base.position.z
        + f64::from(base.dimensions.width) * ((2.0 * random.next_double() - 1.0) * scale)
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AgeTick {
    pub particles: Vec<Particle>,
}

pub fn ai_step(
    base: &mut EntityBase,
    state: &mut AgeableState,
    alive: bool,
    random: &mut impl RandomSource,
) -> AgeTick {
    let mut tick = AgeTick::default();
    if state.forced_age_timer > 0 {
        if state.forced_age_timer % FORCED_AGE_PARTICLE_PERIOD == 0 {
            let x = random_x(base, 1.0, random);
            let y = random_y(base, random) + 0.5;
            let z = random_z(base, 1.0, random);
            tick.particles
                .push(Particle::single(HAPPY_VILLAGER, Vec3::new(x, y, z)));
        }
        state.forced_age_timer -= 1;
    }
    if alive {
        let age = state.age;
        if can_age_up(base, state) {
            set_age(base, state, age + 1);
        } else if age > 0 {
            set_age(base, state, age - 1);
        }
    }
    if state.age_lock_timer > 0 {
        let locked = is_age_locked(base);
        if state.age_lock_timer % 2 == 0 {
            let offset = if locked {
                AGE_LOCK_PARTICLE_OFFSET
            } else {
                0.0
            };
            let x = random_x(base, 1.0, random);
            let y = random_y_scaled(base, f64::from(AGE_LOCK_PARTICLE_OFFSET), random)
                + f64::from(base.dimensions.height)
                + f64::from(offset);
            let z = random_z(base, 1.0, random);
            let kind = if locked {
                PAUSE_MOB_GROWTH
            } else {
                RESET_MOB_GROWTH
            };
            tick.particles
                .push(Particle::single(kind, Vec3::new(x, y, z)));
        }
        state.age_lock_timer -= 1;
    }
    tick
}

pub fn love_ai_step(base: &EntityBase, state: &mut AgeableState, random: &mut impl RandomSource) {
    let age = state.age;
    let Some(love) = &mut state.love else {
        return;
    };
    if age != 0 {
        love.ticks = 0;
    }
    if love.ticks > 0 {
        love.ticks -= 1;
        if love.ticks % LOVE_PARTICLE_PERIOD == 0 {
            for _ in 0..3 {
                let _ = random.next_gaussian() * PARTICLE_SPEED;
            }
            let _ = random_x(base, 1.0, random);
            let _ = random_y(base, random);
            let _ = random_z(base, 1.0, random);
        }
    }
}

pub fn custom_server_ai_step(state: &mut AgeableState) {
    let age = state.age;
    if let Some(love) = &mut state.love
        && age != 0
    {
        love.ticks = 0;
    }
}

pub fn inactive_tick(base: &mut EntityBase, state: &mut AgeableState) {
    if is_age_locked(base) {
        return;
    }
    let age = state.age;
    if age < 0 {
        set_age(base, state, age + 1);
    } else if age > 0 {
        set_age(base, state, age - 1);
    }
}

const fn dims(width: f32, height: f32, eye_height: f32) -> EntityDimensions {
    EntityDimensions {
        width,
        height,
        eye_height,
    }
}

pub fn dimensions(kind: EntityKind, baby: bool) -> EntityDimensions {
    let adult = kind.dimensions();
    if !baby {
        return adult;
    }
    match kind {
        EntityKind::Cow => dims(0.45, 0.7, 0.69),
        EntityKind::Mooshroom => dims(adult.width * 0.5, adult.height * 0.5, 0.665),
        EntityKind::Pig => dims(0.45, 0.45, 0.40625),
        EntityKind::Sheep => dims(0.45, 0.65, 0.65625),
        EntityKind::Chicken => dims(0.3, 0.4, 0.28125),
        EntityKind::Fox => dims(adult.width * 0.6, adult.height * 0.6, 0.34375),
        EntityKind::Panda => dims(adult.width * 0.5, adult.height * 0.5, 0.28125),
        EntityKind::Zombie | EntityKind::Husk | EntityKind::Drowned => dims(0.49, 0.98, 0.775),
        EntityKind::ZombifiedPiglin => dims(0.49, 0.98, 0.78),
        EntityKind::ZombieVillager => dims(0.49, 0.98, 0.67),
        EntityKind::PolarBear => dims(0.7, 0.7, 0.34375),
        _ => dims(
            adult.width * 0.5,
            adult.height * 0.5,
            adult.eye_height * 0.5,
        ),
    }
}

pub fn refresh_dimensions(base: &mut EntityBase, baby: bool) {
    base.dimensions = dimensions(base.kind, baby);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EntityId;
    use crate::random::LegacyRandom;

    fn cow() -> (EntityBase, AgeableState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Cow);
        let state = AgeableState::new(&mut base);
        (base, state)
    }

    #[test]
    fn babies_of_the_neutral_and_villager_mobs_use_their_own_boxes() {
        let bear = dimensions(EntityKind::PolarBear, true);
        assert_eq!(
            (bear.width, bear.height, bear.eye_height),
            (0.7, 0.7, 0.34375)
        );
        let piglin = dimensions(EntityKind::ZombifiedPiglin, true);
        assert_eq!(
            (piglin.width, piglin.height, piglin.eye_height),
            (0.49, 0.98, 0.78)
        );
        let villager = dimensions(EntityKind::ZombieVillager, true);
        assert_eq!(villager.eye_height, 0.67);
        assert_eq!(dimensions(EntityKind::PolarBear, false).height, 1.4);
    }

    #[test]
    fn crossing_zero_flips_the_baby_flag_and_the_box() {
        let (mut base, mut state) = cow();
        assert!(set_baby(&mut base, &mut state, true));
        assert_eq!(state.age, BABY_START_AGE);
        assert!(base.data.boolean(DATA_BABY));
        assert_eq!(base.dimensions.height, 0.7);
        assert!(!set_age(&mut base, &mut state, -5));
        assert!(set_age(&mut base, &mut state, 0));
        assert!(!base.data.boolean(DATA_BABY));
        assert_eq!(base.dimensions.height, 1.4);
    }

    #[test]
    fn feeding_a_baby_speeds_up_by_a_tenth_of_the_remaining_time() {
        let (mut base, mut state) = cow();
        set_baby(&mut base, &mut state, true);
        let seconds = speed_up_seconds_when_feeding(-state.age);
        assert_eq!(seconds, 120);
        age_up(&mut base, &mut state, seconds, true);
        assert_eq!(state.age, -21_600);
        assert_eq!(state.forced_age, 2400);
        assert_eq!(state.forced_age_timer, 40);
        age_up(&mut base, &mut state, 2000, true);
        assert_eq!(state.age, 24_000);
    }

    #[test]
    fn babies_grow_one_tick_at_a_time_and_parents_cool_down() {
        let (mut base, mut state) = cow();
        set_age(&mut base, &mut state, -2);
        let mut random = LegacyRandom::new(3);
        ai_step(&mut base, &mut state, true, &mut random);
        ai_step(&mut base, &mut state, true, &mut random);
        assert_eq!(state.age, 0);
        assert!(!base.data.boolean(DATA_BABY));
        set_age(&mut base, &mut state, PARENT_AGE_AFTER_BREEDING);
        ai_step(&mut base, &mut state, true, &mut random);
        assert_eq!(state.age, PARENT_AGE_AFTER_BREEDING - 1);
    }

    #[test]
    fn love_lasts_six_hundred_ticks_and_needs_an_adult() {
        let (base, mut state) = cow();
        assert!(state.can_fall_in_love());
        state.set_in_love(Some(Uuid::from_u128(9)));
        assert!(state.in_love());
        let mut random = LegacyRandom::new(1);
        love_ai_step(&base, &mut state, &mut random);
        assert_eq!(state.love.as_ref().map(|l| l.ticks), Some(599));
        state.age = 10;
        custom_server_ai_step(&mut state);
        assert!(!state.in_love());
        state.fall_in_love(-5, None, None);
        assert!(state.can_fall_in_love());
        assert_eq!(
            state.love.as_ref().and_then(|love| love.cause),
            Some(Uuid::from_u128(9))
        );
    }

    #[test]
    fn locked_babies_stay_babies() {
        let (mut base, mut state) = cow();
        set_baby(&mut base, &mut state, true);
        assert!(toggle_age_lock(&mut base, &mut state));
        let before = state.age;
        let mut random = LegacyRandom::new(1);
        let tick = ai_step(&mut base, &mut state, true, &mut random);
        assert_eq!(state.age, before);
        assert_eq!(tick.particles.len(), 1);
        assert_eq!(tick.particles[0].kind, PAUSE_MOB_GROWTH);
    }
}
