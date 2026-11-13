use rspigot_nbt::Compound;

use crate::BlockPos;
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::LivingState;
use crate::entity::spawn::{SpawnEnvironment, mob_finalize};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const DATA_FLAGS: u8 = 16;
pub const FLAG_RESTING: i8 = 1;
pub const KEYS: &[&str] = &["BatFlags"];
pub const RESTING_TARGET_RANGE: f64 = 4.0;
pub const WAKE_EVENT: i32 = 1025;
pub const SOUND_VOLUME: f32 = 0.1;
pub const VOICE_PITCH: f32 = 0.95;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BatState {
    pub target: Option<BlockPos>,
}

pub fn define(base: &mut EntityBase) -> BatState {
    base.data
        .define(DATA_FLAGS, SyncedValue::Byte(FLAG_RESTING));
    BatState::default()
}

fn flags(base: &EntityBase) -> i8 {
    match base.data.get(DATA_FLAGS) {
        Some(SyncedValue::Byte(flags)) => *flags,
        _ => 0,
    }
}

pub fn is_resting(base: &EntityBase) -> bool {
    flags(base) & FLAG_RESTING != 0
}

pub fn set_resting(base: &mut EntityBase, resting: bool) {
    let flags = flags(base);
    let flags = if resting {
        flags | FLAG_RESTING
    } else {
        flags & !FLAG_RESTING
    };
    base.data.set(DATA_FLAGS, SyncedValue::Byte(flags));
}

pub fn is_flapping(base: &EntityBase) -> bool {
    !is_resting(base) && base.tick_count % 10 == 0
}

pub fn read(nbt: &Compound, base: &mut EntityBase, _state: &mut BatState) {
    let flags = nbt.get_i64("BatFlags").map_or(0, |flags| flags as i8);
    base.data.set(DATA_FLAGS, SyncedValue::Byte(flags));
}

pub fn write(base: &EntityBase, _state: &BatState, out: &mut Compound) {
    out.insert("BatFlags", flags(base));
}

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
    use rspigot_nbt::Compound;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::save::{load_entity, save_entity};

    fn bat(nbt: Compound) -> crate::entity::Entity {
        load_entity(
            &nbt.with("id", "minecraft:bat"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap()
    }

    #[test]
    fn a_loaded_bat_without_flags_flies_and_the_flags_sit_at_index_sixteen() {
        let entity = bat(Compound::new());
        assert!(!is_resting(&entity.base));
        assert_eq!(
            entity.base.data.get(DATA_FLAGS),
            Some(&SyncedValue::Byte(0))
        );
    }

    #[test]
    fn the_bat_flags_round_trip_as_a_byte() {
        let entity = bat(Compound::new().with("BatFlags", 1i8));
        assert!(is_resting(&entity.base));
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i64("BatFlags"), Some(1));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
    }

    #[test]
    fn resting_toggles_the_lowest_bit_only() {
        let mut entity = bat(Compound::new().with("BatFlags", 2i8));
        set_resting(&mut entity.base, true);
        assert_eq!(
            entity.base.data.get(DATA_FLAGS),
            Some(&SyncedValue::Byte(3))
        );
        set_resting(&mut entity.base, false);
        assert_eq!(
            entity.base.data.get(DATA_FLAGS),
            Some(&SyncedValue::Byte(2))
        );
    }

    #[test]
    fn a_flying_bat_flaps_every_tenth_tick_and_a_resting_one_never() {
        let mut entity = bat(Compound::new());
        entity.base.tick_count = 20;
        assert!(is_flapping(&entity.base));
        entity.base.tick_count = 21;
        assert!(!is_flapping(&entity.base));
        set_resting(&mut entity.base, true);
        entity.base.tick_count = 20;
        assert!(!is_flapping(&entity.base));
    }

    #[test]
    fn a_freshly_constructed_bat_rests() {
        let mut base = crate::entity::EntityBase::new(
            EntityId(1),
            Uuid::from_u128(1),
            rspigot_entities::EntityKind::Bat,
        );
        let _ = define(&mut base);
        assert!(is_resting(&base));
    }
}
