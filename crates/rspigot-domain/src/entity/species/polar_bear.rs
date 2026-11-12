use rspigot_nbt::Compound;

use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::LivingState;
use crate::entity::spawn::{SpawnEnvironment, ageable_finalize};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const DATA_STANDING: u8 = 18;
pub const WARNING_SOUND_TICKS: i32 = 40;
pub const BABY_SPAWN_CHANCE: f32 = 1.0;

pub const KEYS: &[&str] = &[];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PolarBearState {
    pub warning_sound_ticks: i32,
}

impl PolarBearState {
    pub fn tick(&mut self) {
        if self.warning_sound_ticks > 0 {
            self.warning_sound_ticks -= 1;
        }
    }

    pub fn start_warning(&mut self) -> bool {
        if self.warning_sound_ticks > 0 {
            return false;
        }
        self.warning_sound_ticks = WARNING_SOUND_TICKS;
        true
    }
}

pub fn define(base: &mut EntityBase) -> PolarBearState {
    base.data.define(DATA_STANDING, SyncedValue::Boolean(false));
    PolarBearState::default()
}

pub fn is_standing(base: &EntityBase) -> bool {
    base.data.boolean(DATA_STANDING)
}

pub fn set_standing(base: &mut EntityBase, standing: bool) {
    base.data.set(DATA_STANDING, SyncedValue::Boolean(standing));
}

pub fn read(_nbt: &Compound, _base: &mut EntityBase, _state: &mut PolarBearState) {}

pub fn write(_base: &EntityBase, _state: &PolarBearState, _out: &mut Compound) {}

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
        baby_chance: BABY_SPAWN_CHANCE,
    });
    ageable_finalize(base, living, Some(group), level)
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::random::LegacyRandom;

    fn bear() -> EntityBase {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::PolarBear);
        define(&mut base);
        base
    }

    #[test]
    fn standing_is_data_eighteen_and_off_by_default() {
        let mut base = bear();
        assert!(!is_standing(&base));
        set_standing(&mut base, true);
        assert!(is_standing(&base));
        assert_eq!(base.data.get(18), Some(&SyncedValue::Boolean(true)));
    }

    #[test]
    fn the_warning_sound_has_a_forty_tick_cooldown() {
        let mut state = PolarBearState::default();
        assert!(state.start_warning());
        assert!(!state.start_warning());
        for _ in 0..39 {
            state.tick();
        }
        assert!(!state.start_warning());
        state.tick();
        assert!(state.start_warning());
    }

    #[test]
    fn the_first_bear_of_a_group_is_an_adult_and_the_next_are_always_babies() {
        use crate::BiomeId;
        use crate::difficulty::DifficultyInstance;
        use crate::entity::spawn::finalize_spawn;
        use crate::entity::{Entity, EntityState};
        use crate::spawn::SpawnReason;
        use crate::world::Difficulty;

        let environment = SpawnEnvironment {
            biome: BiomeId(0),
            difficulty: DifficultyInstance::new(Difficulty::Normal, 0, 0, 1.0),
            halloween: false,
            moon_brightness: 0.0,
        };
        let mut level = LegacyRandom::new(7);
        let mut own = LegacyRandom::new(8);
        let mut spawn = |group| {
            let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::PolarBear);
            let living = LivingState::new(&mut base, 0.0);
            let mut bear = Entity::new(base, EntityState::Living(Box::new(living)));
            let group = finalize_spawn(
                &mut bear,
                &environment,
                SpawnReason::Natural,
                group,
                &mut level,
                &mut own,
            );
            (bear.is_baby(), group)
        };
        let (first_baby, group) = spawn(None);
        assert!(!first_baby);
        assert!(matches!(
            group,
            Some(SpawnGroupData::Ageable {
                group_size: 1,
                should_spawn_baby: true,
                baby_chance,
            }) if baby_chance == 1.0
        ));
        let (second_baby, _) = spawn(group);
        assert!(second_baby);
    }

    #[test]
    fn anger_and_age_survive_a_round_trip_and_standing_is_not_saved() {
        use crate::entity::neutral::NO_ANGER_END_TIME;
        use crate::entity::save::{load_entity, save_entity, uuid_tag};
        use rspigot_nbt::Tag;

        let nbt = Compound::new()
            .with("id", "minecraft:polar_bear")
            .with(
                "Pos",
                Tag::List(vec![0.5f64.into(), 64.0f64.into(), 0.5f64.into()]),
            )
            .with("UUID", uuid_tag(Uuid::from_u128(7)))
            .with("Age", -12000)
            .with("anger_end_time", 5000i64)
            .with("angry_at", uuid_tag(Uuid::from_u128(5)));
        let mut entity = load_entity(&nbt, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert!(entity.is_baby());
        assert!(entity.extra.is_empty());
        set_standing(&mut entity.base, true);
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i64("anger_end_time"), Some(5000));
        assert_eq!(saved.get("angry_at"), Some(&uuid_tag(Uuid::from_u128(5))));
        assert_eq!(saved.get_i32("Age"), Some(-12000));
        assert!(!saved.contains_key("Standing"));
        let reloaded = load_entity(&saved, EntityId(2), Uuid::from_u128(2)).unwrap();
        assert!(!is_standing(&reloaded.base));
        let fresh = load_entity(
            &nbt.clone().with("anger_end_time", NO_ANGER_END_TIME),
            EntityId(3),
            Uuid::from_u128(3),
        )
        .unwrap();
        let mob = fresh.living().and_then(|l| l.mob.as_ref()).unwrap();
        assert!(!mob.neutral.unwrap().is_angry(0));
    }
}
