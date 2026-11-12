use rspigot_entities::EntityKind;
use rspigot_nbt::Compound;

use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::{LivingAttributes, LivingState};
use crate::entity::spawn::{ageable_finalize, mob_finalize};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const HOGLIN_DATA_IMMUNE_TO_ZOMBIFICATION: u8 = 18;
pub const ZOGLIN_DATA_BABY: u8 = 16;
pub const CONVERSION_TICKS: i32 = 300;
pub const ATTACK_ANIMATION_TICKS: i32 = 10;
pub const BABY_CHANCE: f32 = 0.2;
pub const ATTACK_DAMAGE: &str = "minecraft:attack_damage";
pub const ADULT_ATTACK_DAMAGE: f64 = 6.0;
pub const BABY_ATTACK_DAMAGE: f64 = 0.5;
pub const ADULT_XP_REWARD: i32 = 5;
pub const BABY_XP_REWARD: i32 = 3;
pub const KEYS: &[&str] = &[
    "IsBaby",
    "IsImmuneToZombification",
    "TimeInOverworld",
    "CannotBeHunted",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HoglinState {
    pub time_in_overworld: i32,
    pub cannot_be_hunted: bool,
    pub attack_animation_remaining: i32,
}

pub fn define(base: &mut EntityBase) -> HoglinState {
    if base.kind == EntityKind::Hoglin {
        base.data.define(
            HOGLIN_DATA_IMMUNE_TO_ZOMBIFICATION,
            SyncedValue::Boolean(false),
        );
    } else {
        base.data
            .define(ZOGLIN_DATA_BABY, SyncedValue::Boolean(false));
    }
    HoglinState::default()
}

pub fn is_zoglin_baby(base: &EntityBase) -> bool {
    base.kind == EntityKind::Zoglin && base.data.boolean(ZOGLIN_DATA_BABY)
}

pub fn set_zoglin_baby(base: &mut EntityBase, attributes: &mut LivingAttributes, baby: bool) {
    if base.kind != EntityKind::Zoglin {
        return;
    }
    base.data.set(ZOGLIN_DATA_BABY, SyncedValue::Boolean(baby));
    crate::entity::ageable::refresh_dimensions(base, baby);
    if baby {
        attributes.set_base(ATTACK_DAMAGE, BABY_ATTACK_DAMAGE);
    }
}

pub fn is_immune_to_zombification(base: &EntityBase) -> bool {
    base.kind == EntityKind::Hoglin && base.data.boolean(HOGLIN_DATA_IMMUNE_TO_ZOMBIFICATION)
}

pub fn set_immune_to_zombification(base: &mut EntityBase, immune: bool) {
    if base.kind == EntityKind::Hoglin {
        base.data.set(
            HOGLIN_DATA_IMMUNE_TO_ZOMBIFICATION,
            SyncedValue::Boolean(immune),
        );
    }
}

pub fn age_boundary_reached(attributes: &mut LivingAttributes, baby: bool) {
    let damage = if baby {
        BABY_ATTACK_DAMAGE
    } else {
        ADULT_ATTACK_DAMAGE
    };
    attributes.set_base(ATTACK_DAMAGE, damage);
}

pub fn read(
    nbt: &Compound,
    base: &mut EntityBase,
    attributes: &mut LivingAttributes,
    state: &mut HoglinState,
) {
    if base.kind == EntityKind::Hoglin {
        set_immune_to_zombification(
            base,
            nbt.get_bool("IsImmuneToZombification").unwrap_or(false),
        );
        state.time_in_overworld = nbt.get_i32("TimeInOverworld").unwrap_or(0);
        state.cannot_be_hunted = nbt.get_bool("CannotBeHunted").unwrap_or(false);
    } else {
        set_zoglin_baby(base, attributes, nbt.get_bool("IsBaby").unwrap_or(false));
    }
}

pub fn write(base: &EntityBase, state: &HoglinState, out: &mut Compound) {
    if base.kind == EntityKind::Hoglin {
        out.insert(
            "IsImmuneToZombification",
            i8::from(is_immune_to_zombification(base)),
        );
        out.insert("TimeInOverworld", state.time_in_overworld);
        out.insert("CannotBeHunted", i8::from(state.cannot_be_hunted));
    } else {
        out.insert("IsBaby", i8::from(is_zoglin_baby(base)));
    }
}

pub fn finalize_hoglin(
    base: &mut EntityBase,
    living: &mut LivingState,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    if level.next_float() < BABY_CHANCE
        && let Some(ageable) = living.mob.as_mut().and_then(|mob| mob.ageable.as_mut())
    {
        crate::entity::ageable::set_baby(base, ageable, true);
    }
    ageable_finalize(base, living, group, level)
}

pub fn finalize_zoglin(
    base: &mut EntityBase,
    living: &mut LivingState,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    if level.next_float() < BABY_CHANCE {
        set_zoglin_baby(base, &mut living.attributes, true);
    }
    mob_finalize(base, living, group, level)
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::save::{load_entity, save_entity};
    use crate::entity::spawn::{SpawnEnvironment, finalize_spawn};
    use crate::entity::species::Species;
    use crate::entity::{Entity, EntityState};
    use crate::random::LegacyRandom;
    use crate::spawn::SpawnReason;

    fn scatter(seed: i64) -> i64 {
        seed.wrapping_mul(0x5DEE_CE66_D1A4_F87D)
    }

    fn spawn(kind: EntityKind, seed: i64) -> Entity {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), kind);
        let living = LivingState::new(&mut base, 0.0);
        let mut entity = Entity::new(base, EntityState::Living(Box::new(living)));
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
        let mut level = LegacyRandom::new(scatter(seed));
        let mut own = LegacyRandom::new(scatter(seed + 1));
        finalize_spawn(
            &mut entity,
            &environment,
            SpawnReason::Natural,
            None,
            &mut level,
            &mut own,
        );
        entity
    }

    #[test]
    fn a_fifth_of_hoglins_and_zoglins_spawn_as_babies() {
        for kind in [EntityKind::Hoglin, EntityKind::Zoglin] {
            let babies = (0..2000)
                .filter(|seed| spawn(kind, *seed).is_baby())
                .count();
            assert!((330..=470).contains(&babies), "{kind:?} {babies}");
        }
    }

    #[test]
    fn baby_hoglins_and_zoglins_use_the_small_box() {
        for kind in [EntityKind::Hoglin, EntityKind::Zoglin] {
            let baby = (0..200).map(|seed| spawn(kind, seed)).find(Entity::is_baby);
            let baby = baby.expect("a baby");
            let dimensions = baby.base.dimensions;
            assert_eq!(
                (dimensions.width, dimensions.height, dimensions.eye_height),
                (0.75, 0.85, 0.625)
            );
        }
    }

    #[test]
    fn a_baby_zoglin_bites_for_half_a_point() {
        let mut baby = (0..200)
            .map(|seed| spawn(EntityKind::Zoglin, seed))
            .find(Entity::is_baby)
            .expect("a baby");
        let living = baby.living_mut().unwrap();
        assert_eq!(living.attribute(ATTACK_DAMAGE), BABY_ATTACK_DAMAGE);
    }

    #[test]
    fn the_age_boundary_swaps_the_hoglin_attack_damage() {
        let mut hoglin = spawn(EntityKind::Hoglin, 1);
        let living = hoglin.living_mut().unwrap();
        age_boundary_reached(&mut living.attributes, true);
        assert_eq!(living.attribute(ATTACK_DAMAGE), BABY_ATTACK_DAMAGE);
        age_boundary_reached(&mut living.attributes, false);
        assert_eq!(living.attribute(ATTACK_DAMAGE), ADULT_ATTACK_DAMAGE);
    }

    #[test]
    fn hoglin_state_round_trips_through_nbt() {
        let mut hoglin = spawn(EntityKind::Hoglin, 2);
        set_immune_to_zombification(&mut hoglin.base, true);
        if let Some(Species::Hoglin(state)) = hoglin
            .living_mut()
            .and_then(|l| l.mob.as_mut())
            .map(|m| &mut m.species)
        {
            state.time_in_overworld = 77;
            state.cannot_be_hunted = true;
        }
        let saved = save_entity(&hoglin, None).unwrap();
        assert_eq!(saved.get_bool("IsImmuneToZombification"), Some(true));
        assert_eq!(saved.get_i32("TimeInOverworld"), Some(77));
        assert_eq!(saved.get_bool("CannotBeHunted"), Some(true));
        let loaded = load_entity(&saved, EntityId(2), Uuid::from_u128(2)).unwrap();
        assert!(is_immune_to_zombification(&loaded.base));
        assert_eq!(save_entity(&loaded, None), Some(saved));
    }

    #[test]
    fn a_baby_zoglin_round_trips_through_nbt() {
        let baby = (0..200)
            .map(|seed| spawn(EntityKind::Zoglin, seed))
            .find(Entity::is_baby)
            .expect("a baby");
        let saved = save_entity(&baby, None).unwrap();
        assert_eq!(saved.get_bool("IsBaby"), Some(true));
        let loaded = load_entity(&saved, EntityId(2), Uuid::from_u128(2)).unwrap();
        assert!(loaded.is_baby());
    }
}
