use rspigot_nbt::{Compound, Tag};

use crate::BlockPos;
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::LivingState;
use crate::entity::spawn::{SpawnEnvironment, ageable_finalize};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const DATA_HAS_EGG: u8 = 18;
pub const DATA_LAYING_EGG: u8 = 19;
pub const KEYS: &[&str] = &["home_pos", "has_egg"];
pub const BABY_SCALE: f32 = 0.3;
pub const EGG_AGE_AFTER_BREEDING: i32 = 6000;
pub const LOVE_AFTER_LAYING: i32 = 600;
pub const EGG_HATCH_AGE: i32 = -24_000;
pub const EGG_MAX_EXPERIENCE: i32 = 7;
pub const LAY_EGG_SOUND: &str = "minecraft:entity.turtle.lay_egg";
pub const EGG_CRACK_SOUND: &str = "minecraft:block.turtle_egg.crack";
pub const EGG_HATCH_SOUND: &str = "minecraft:block.turtle_egg.hatch";
pub const EGG_BREAK_SOUND: &str = "minecraft:block.turtle_egg.break";
pub const AMBIENT_LAND_SOUND: &str = "minecraft:entity.turtle.ambient_land";
pub const SWIM_SOUND: &str = "minecraft:entity.turtle.swim";
pub const AMBIENT_INTERVAL: i32 = 200;
pub const STEP_VOLUME: f32 = 0.15;
pub const NEXT_STEP_DISTANCE: f32 = 0.15;
pub const GROW_LOOT: &str = "minecraft:gameplay/turtle_grow";
pub const BLOCK_BREAK_EVENT: i32 = 2001;
pub const GROWTH_EVENT: i32 = 2012;
pub const GROWTH_EVENT_DATA: i32 = 15;
pub const LAY_PERIOD: i32 = 5;
pub const LAY_DURATION: i32 = 200;
pub const HOME_RADIUS_SQR: f64 = 9.0;
pub const FAR_HOME_SQR: f64 = 64.0;
pub const NEAR_HOME_SQR: f64 = 7.0;
pub const SPAWN_TOP_OFFSET: i32 = 4;
pub const SAND_TAG: &str = "minecraft:sand";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TurtleState {
    pub home_pos: BlockPos,
    pub lay_egg_counter: i32,
    pub going_home: bool,
    pub travel_pos: Option<BlockPos>,
}

pub fn define(base: &mut EntityBase) -> TurtleState {
    base.data.define(DATA_HAS_EGG, SyncedValue::Boolean(false));
    base.data
        .define(DATA_LAYING_EGG, SyncedValue::Boolean(false));
    TurtleState::default()
}

pub fn has_egg(base: &EntityBase) -> bool {
    base.data.boolean(DATA_HAS_EGG)
}

pub fn set_has_egg(base: &mut EntityBase, value: bool) {
    base.data.set(DATA_HAS_EGG, SyncedValue::Boolean(value));
}

pub fn is_laying_egg(base: &EntityBase) -> bool {
    base.data.boolean(DATA_LAYING_EGG)
}

pub fn set_laying_egg(base: &mut EntityBase, state: &mut TurtleState, value: bool) {
    state.lay_egg_counter = i32::from(value);
    base.data.set(DATA_LAYING_EGG, SyncedValue::Boolean(value));
}

pub fn read(nbt: &Compound, base: &mut EntityBase, state: &mut TurtleState) {
    state.home_pos = nbt
        .get_int_array("home_pos")
        .filter(|values| values.len() == 3)
        .map_or_else(
            || base.position.block(),
            |values| BlockPos::new(values[0], values[1], values[2]),
        );
    set_has_egg(base, nbt.get_bool("has_egg").unwrap_or(false));
}

pub fn write(base: &EntityBase, state: &TurtleState, out: &mut Compound) {
    let home = state.home_pos;
    out.insert("home_pos", Tag::IntArray(vec![home.x, home.y, home.z]));
    out.insert("has_egg", has_egg(base));
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    _environment: &SpawnEnvironment,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    if let Some(crate::entity::species::Species::Turtle(state)) =
        living.mob.as_mut().map(|mob| &mut mob.species)
    {
        state.home_pos = base.position.block();
    }
    ageable_finalize(base, living, group, level)
}

pub fn hatch_chance(day_time: i64) -> f32 {
    let time = day_time.rem_euclid(24_000);
    if (21_062..21_905).contains(&time) {
        1.0
    } else {
        0.002
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::ageable;
    use crate::entity::save::{load_entity, save_entity};
    use crate::entity::species::Species;
    use crate::random::LegacyRandom;
    use rspigot_entities::EntityKind;

    fn turtle(nbt: Compound) -> crate::entity::Entity {
        load_entity(
            &nbt.with("id", "minecraft:turtle"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap()
    }

    fn state_of(entity: &crate::entity::Entity) -> TurtleState {
        match entity
            .living()
            .and_then(|l| l.mob.as_ref())
            .map(|m| m.species)
        {
            Some(Species::Turtle(state)) => state,
            other => panic!("not a turtle: {other:?}"),
        }
    }

    #[test]
    fn the_home_and_the_egg_round_trip_in_the_vanilla_layout() {
        let loaded = turtle(
            Compound::new()
                .with("home_pos", Tag::IntArray(vec![10, 64, -3]))
                .with("has_egg", true),
        );
        assert_eq!(state_of(&loaded).home_pos, BlockPos::new(10, 64, -3));
        assert!(has_egg(&loaded.base));
        assert_eq!(
            loaded.base.data.get(DATA_HAS_EGG),
            Some(&SyncedValue::Boolean(true))
        );
        assert_eq!(
            loaded.base.data.get(DATA_LAYING_EGG),
            Some(&SyncedValue::Boolean(false))
        );
        let saved = save_entity(&loaded, None).unwrap();
        assert_eq!(saved.get_int_array("home_pos"), Some(&[10, 64, -3][..]));
        assert_eq!(saved.get_bool("has_egg"), Some(true));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
    }

    #[test]
    fn a_turtle_without_a_saved_home_takes_its_own_block() {
        let mut nbt = Compound::new();
        nbt.insert(
            "Pos",
            Tag::List(vec![Tag::Double(5.5), Tag::Double(70.0), Tag::Double(-2.5)]),
        );
        let loaded = turtle(nbt);
        assert_eq!(state_of(&loaded).home_pos, BlockPos::new(5, 70, -3));
        assert!(!has_egg(&loaded.base));
    }

    #[test]
    fn a_baby_turtle_is_three_tenths_of_the_adult() {
        let adult = EntityKind::Turtle.dimensions();
        let baby = ageable::dimensions(EntityKind::Turtle, true);
        assert_eq!(baby.width, adult.width * BABY_SCALE);
        assert_eq!(baby.height, adult.height * BABY_SCALE);
        assert_eq!(baby.eye_height, adult.eye_height * BABY_SCALE);
    }

    #[test]
    fn a_spawned_turtle_takes_its_spawn_block_as_home() {
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
        let mut entity = turtle(Compound::new());
        entity.base.position = crate::Vec3::new(3.5, 66.0, 9.5);
        let crate::entity::Entity { base, state, .. } = &mut entity;
        let crate::entity::EntityState::Living(living) = state else {
            panic!("not living");
        };
        let mut level = LegacyRandom::new(1);
        finalize(base, living, &environment, None, &mut level);
        assert_eq!(state_of(&entity).home_pos, BlockPos::new(3, 66, 9));
    }

    #[test]
    fn eggs_hatch_surely_at_dawn_and_rarely_otherwise() {
        assert_eq!(hatch_chance(21_062), 1.0);
        assert_eq!(hatch_chance(21_904 + 24_000), 1.0);
        assert_eq!(hatch_chance(21_905), 0.002);
        assert_eq!(hatch_chance(21_061), 0.002);
        assert_eq!(hatch_chance(6000), 0.002);
    }

    #[test]
    fn laying_sets_the_counter_to_one_and_stopping_clears_it() {
        let mut entity = turtle(Compound::new());
        let mut state = state_of(&entity);
        set_laying_egg(&mut entity.base, &mut state, true);
        assert_eq!(state.lay_egg_counter, 1);
        assert!(is_laying_egg(&entity.base));
        set_laying_egg(&mut entity.base, &mut state, false);
        assert_eq!(state.lay_egg_counter, 0);
        assert!(!is_laying_egg(&entity.base));
    }
}
