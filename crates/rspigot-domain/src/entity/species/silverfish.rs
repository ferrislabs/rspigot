use rspigot_nbt::Compound;

use crate::entity::base::EntityBase;
use crate::entity::living::LivingState;
use crate::entity::spawn::{SpawnEnvironment, mob_finalize};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const KEYS: &[&str] = &[];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SilverfishState;

pub fn define(_base: &mut EntityBase) -> SilverfishState {
    SilverfishState
}

pub fn read(_nbt: &Compound, _base: &mut EntityBase, _state: &mut SilverfishState) {}

pub fn write(_base: &EntityBase, _state: &SilverfishState, _out: &mut Compound) {}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    _environment: &SpawnEnvironment,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    mob_finalize(base, living, group, level)
}

#[cfg(test)]
mod tests {
    use rspigot_nbt::Tag;
    use uuid::Uuid;

    use super::*;
    use crate::entity::save::{load_entity, save_entity};
    use crate::entity::species::Species;
    use crate::{EntityId, Vec3};

    fn nbt() -> Compound {
        Compound::new()
            .with("id", "minecraft:silverfish")
            .with(
                "Pos",
                Tag::List(vec![Tag::Double(1.5), Tag::Double(64.0), Tag::Double(2.5)]),
            )
            .with("Health", 5.0f32)
            .with("PersistenceRequired", true)
            .with("Custom", 3)
    }

    #[test]
    fn has_no_state_of_its_own() {
        let entity = load_entity(&nbt(), EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(entity.base.position, Vec3::new(1.5, 64.0, 2.5));
        let mob = entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .unwrap();
        assert_eq!(mob.species, Species::Silverfish(SilverfishState));
        assert!(mob.neutral.is_none() && mob.tamable.is_none() && mob.ageable.is_none());
        assert!(mob.persistence_required);
        assert!(KEYS.is_empty());
    }

    #[test]
    fn syncs_only_the_entity_living_and_mob_data() {
        let entity = load_entity(&nbt(), EntityId(1), Uuid::from_u128(1)).unwrap();
        let defined: Vec<u8> = (0..=31)
            .filter(|index| entity.base.data.get(*index).is_some())
            .collect();
        assert_eq!(defined.last(), Some(&15));
        assert!(defined.iter().all(|index| *index <= 15));
    }

    #[test]
    fn saves_exactly_what_it_loaded() {
        let entity = load_entity(&nbt(), EntityId(1), Uuid::from_u128(1)).unwrap();
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_str("id"), Some("minecraft:silverfish"));
        assert_eq!(saved.get_f64("Health"), Some(5.0));
        assert_eq!(saved.get_bool("PersistenceRequired"), Some(true));
        assert_eq!(saved.get_i32("Custom"), Some(3));
        let again = load_entity(&saved, EntityId(2), Uuid::from_u128(2)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
    }
}
