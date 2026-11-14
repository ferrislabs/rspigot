use rspigot_nbt::Compound;

use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;

pub const DATA_FROM_BUCKET: u8 = 16;
pub const DATA_AGE_LOCKED: u8 = 17;
pub const KEYS: &[&str] = &["FromBucket", "Age", "AgeLocked"];
pub const TICKS_TO_BE_FROG: i32 = 24_000;
pub const AGE_LOCK_PARTICLE_TIMER: i32 = 40;
pub const FEED_SECONDS_UNIT: i32 = 20;
pub const GROW_UP_VOLUME: f32 = 0.15;
pub const GROW_UP_SOUND: &str = "minecraft:entity.tadpole.grow_up";
pub const FLOP_SOUND: &str = "minecraft:entity.tadpole.flop";
pub const PICKUP_SOUND: &str = "minecraft:item.bucket.fill_tadpole";
pub const FOOD_TAG: &str = "minecraft:frog_food";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TadpoleState {
    pub age: i32,
    pub age_lock_particle_timer: i32,
}

pub fn define(base: &mut EntityBase) -> TadpoleState {
    base.data
        .define(DATA_FROM_BUCKET, SyncedValue::Boolean(false));
    base.data
        .define(DATA_AGE_LOCKED, SyncedValue::Boolean(false));
    TadpoleState::default()
}

pub fn is_age_locked(base: &EntityBase) -> bool {
    base.data.boolean(DATA_AGE_LOCKED)
}

pub fn set_age_locked(base: &mut EntityBase, locked: bool) {
    base.data.set(DATA_AGE_LOCKED, SyncedValue::Boolean(locked));
}

pub fn ticks_left_until_adult(state: &TadpoleState) -> i32 {
    (TICKS_TO_BE_FROG - state.age).max(0)
}

pub fn set_age(state: &mut TadpoleState, age: i32) -> bool {
    state.age = age;
    state.age >= TICKS_TO_BE_FROG
}

pub fn age_up_seconds(state: &mut TadpoleState, seconds: i32) -> bool {
    set_age(state, state.age + seconds * FEED_SECONDS_UNIT)
}

pub fn ai_step_age(base: &EntityBase, state: &mut TadpoleState) -> bool {
    if is_age_locked(base) {
        false
    } else {
        set_age(state, state.age + 1)
    }
}

pub fn toggle_age_lock(base: &mut EntityBase, state: &mut TadpoleState) {
    let locked = is_age_locked(base);
    set_age_locked(base, !locked);
    state.age = 0;
    state.age_lock_particle_timer = AGE_LOCK_PARTICLE_TIMER;
}

pub fn read(nbt: &Compound, base: &mut EntityBase, state: &mut TadpoleState) {
    let age = nbt.get_i32("Age").unwrap_or(0);
    state.age = age;
    set_age_locked(base, nbt.get_bool("AgeLocked").unwrap_or(false));
}

pub fn write(base: &EntityBase, state: &TadpoleState, out: &mut Compound) {
    out.insert("FromBucket", true);
    out.insert("Age", state.age);
    out.insert("AgeLocked", is_age_locked(base));
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::save::{load_entity, save_entity};
    use crate::entity::species::Species;

    fn tadpole(nbt: Compound) -> crate::entity::Entity {
        load_entity(
            &nbt.with("id", "minecraft:tadpole"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap()
    }

    fn state_of(entity: &crate::entity::Entity) -> TadpoleState {
        match entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .map(|mob| mob.species)
        {
            Some(Species::Tadpole(state)) => state,
            other => panic!("not a tadpole: {other:?}"),
        }
    }

    #[test]
    fn the_age_and_the_lock_round_trip_and_a_tadpole_always_saves_from_bucket() {
        let loaded = tadpole(Compound::new().with("Age", 1234).with("AgeLocked", true));
        assert_eq!(state_of(&loaded).age, 1234);
        assert!(is_age_locked(&loaded.base));
        let saved = save_entity(&loaded, None).unwrap();
        assert_eq!(saved.get_i32("Age"), Some(1234));
        assert_eq!(saved.get_bool("AgeLocked"), Some(true));
        assert_eq!(saved.get_bool("FromBucket"), Some(true));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
    }

    #[test]
    fn a_tadpole_becomes_a_frog_after_twenty_four_thousand_ticks() {
        let mut state = TadpoleState::default();
        assert!(!set_age(&mut state, TICKS_TO_BE_FROG - 1));
        assert!(set_age(&mut state, TICKS_TO_BE_FROG));
        assert_eq!(ticks_left_until_adult(&state), 0);
    }

    #[test]
    fn feeding_ages_by_the_given_seconds_and_a_locked_tadpole_does_not_age() {
        let mut entity = tadpole(Compound::new());
        let mut state = state_of(&entity);
        assert!(!age_up_seconds(&mut state, 100));
        assert_eq!(state.age, 2000);
        assert!(!ai_step_age(&entity.base, &mut state));
        assert_eq!(state.age, 2001);
        toggle_age_lock(&mut entity.base, &mut state);
        assert!(is_age_locked(&entity.base));
        assert_eq!(state.age, 0);
        assert_eq!(state.age_lock_particle_timer, AGE_LOCK_PARTICLE_TIMER);
        ai_step_age(&entity.base, &mut state);
        assert_eq!(state.age, 0);
    }
}
