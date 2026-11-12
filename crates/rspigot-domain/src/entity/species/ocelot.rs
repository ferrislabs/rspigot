use rspigot_nbt::Compound;

use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::LivingState;
use crate::entity::spawn::{SpawnEnvironment, ageable_finalize};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const DATA_TRUSTING: u8 = 18;
const BABY_CHANCE: f32 = 1.0;

pub const KEYS: &[&str] = &["Trusting"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OcelotState;

pub fn define(base: &mut EntityBase) -> OcelotState {
    base.data.define(DATA_TRUSTING, SyncedValue::Boolean(false));
    OcelotState
}

pub fn is_trusting(base: &EntityBase) -> bool {
    base.data.boolean(DATA_TRUSTING)
}

pub fn set_trusting(base: &mut EntityBase, trusting: bool) {
    base.data.set(DATA_TRUSTING, SyncedValue::Boolean(trusting));
}

pub fn read(nbt: &Compound, base: &mut EntityBase, _state: &mut OcelotState) {
    set_trusting(base, nbt.get_bool("Trusting").unwrap_or(false));
}

pub fn write(base: &EntityBase, _state: &OcelotState, out: &mut Compound) {
    out.insert("Trusting", i8::from(is_trusting(base)));
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

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::ageable::DATA_BABY;
    use crate::random::LegacyRandom;
    use rspigot_entities::EntityKind;

    fn ocelot() -> (EntityBase, LivingState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Ocelot);
        let living = LivingState::new(&mut base, 0.0);
        (base, living)
    }

    fn environment() -> SpawnEnvironment {
        SpawnEnvironment {
            biome: crate::BiomeId(rspigot_data::registries::biome_id("minecraft:jungle").unwrap()),
            difficulty: crate::difficulty::DifficultyInstance::new(
                crate::world::Difficulty::Normal,
                0,
                0,
                1.0,
            ),
            halloween: false,
            moon_brightness: 0.0,
        }
    }

    #[test]
    fn trust_is_synchronised_at_index_18_and_defaults_to_false() {
        let (mut base, _) = ocelot();
        assert!(!is_trusting(&base));
        set_trusting(&mut base, true);
        assert!(base.data.boolean(18));
        assert!(is_trusting(&base));
    }

    #[test]
    fn trust_round_trips_through_nbt() {
        let (mut base, _) = ocelot();
        let mut nbt = Compound::new();
        nbt.insert("Trusting", 1i8);
        read(&nbt, &mut base, &mut OcelotState);
        assert!(is_trusting(&base));
        let mut out = Compound::new();
        write(&base, &OcelotState, &mut out);
        assert_eq!(out.get_bool("Trusting"), Some(true));
        read(&Compound::new(), &mut base, &mut OcelotState);
        assert!(!is_trusting(&base));
    }

    #[test]
    fn every_ocelot_after_the_first_of_a_pack_is_a_kitten() {
        let mut level = LegacyRandom::new(3);
        let (mut a_base, mut a) = ocelot();
        let group = finalize(&mut a_base, &mut a, &environment(), None, &mut level);
        assert!(!a_base.data.boolean(DATA_BABY));
        let (mut b_base, mut b) = ocelot();
        let group = finalize(&mut b_base, &mut b, &environment(), group, &mut level);
        assert!(b_base.data.boolean(DATA_BABY));
        let (mut c_base, mut c) = ocelot();
        finalize(&mut c_base, &mut c, &environment(), group, &mut level);
        assert!(c_base.data.boolean(DATA_BABY));
    }
}
