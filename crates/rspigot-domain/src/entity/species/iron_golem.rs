use rspigot_nbt::Compound;

use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::LivingState;
use crate::entity::spawn::{SpawnEnvironment, mob_finalize};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const DATA_FLAGS: u8 = 16;
pub const FLAG_PLAYER_CREATED: u8 = 1;
pub const KEYS: &[&str] = &["PlayerCreated"];
pub const ATTACK_ANIMATION_TICKS: i32 = 10;
pub const OFFER_FLOWER_TICKS: i32 = 400;
pub const IRON_INGOT_HEAL_AMOUNT: f32 = 25.0;
pub const EVENT_ATTACK: i8 = 4;
pub const EVENT_OFFER_FLOWER: i8 = 11;
pub const EVENT_STOP_OFFER_FLOWER: i8 = 34;
const CRACK_LOW: f32 = 0.75;
const CRACK_MEDIUM: f32 = 0.5;
const CRACK_HIGH: f32 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Crackiness {
    None,
    Low,
    Medium,
    High,
}

impl Crackiness {
    pub fn by_fraction(fraction: f32) -> Self {
        if fraction < CRACK_HIGH {
            Self::High
        } else if fraction < CRACK_MEDIUM {
            Self::Medium
        } else if fraction < CRACK_LOW {
            Self::Low
        } else {
            Self::None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IronGolemState {
    pub attack_animation_tick: i32,
    pub offer_flower_tick: i32,
    pub crack_before: Option<Crackiness>,
}

pub fn define(base: &mut EntityBase) -> IronGolemState {
    base.data.define(DATA_FLAGS, SyncedValue::Byte(0));
    IronGolemState::default()
}

pub fn is_player_created(base: &EntityBase) -> bool {
    base.data.flag(DATA_FLAGS, FLAG_PLAYER_CREATED)
}

pub fn set_player_created(base: &mut EntityBase, value: bool) {
    base.data.set_flag(DATA_FLAGS, FLAG_PLAYER_CREATED, value);
}

pub fn read(nbt: &Compound, base: &mut EntityBase, _state: &mut IronGolemState) {
    set_player_created(base, nbt.get_bool("PlayerCreated").unwrap_or(false));
}

pub fn write(base: &EntityBase, _state: &IronGolemState, out: &mut Compound) {
    out.insert("PlayerCreated", i8::from(is_player_created(base)));
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

    fn golem_nbt() -> Compound {
        Compound::new().with("id", "minecraft:iron_golem")
    }

    fn base_of(nbt: &Compound) -> crate::entity::Entity {
        load_entity(nbt, EntityId(1), Uuid::from_u128(1)).unwrap()
    }

    #[test]
    fn cracks_appear_below_each_fraction() {
        assert_eq!(Crackiness::by_fraction(1.0), Crackiness::None);
        assert_eq!(Crackiness::by_fraction(0.75), Crackiness::None);
        assert_eq!(Crackiness::by_fraction(0.7499), Crackiness::Low);
        assert_eq!(Crackiness::by_fraction(0.5), Crackiness::Low);
        assert_eq!(Crackiness::by_fraction(0.4999), Crackiness::Medium);
        assert_eq!(Crackiness::by_fraction(0.25), Crackiness::Medium);
        assert_eq!(Crackiness::by_fraction(0.2499), Crackiness::High);
        assert_eq!(Crackiness::by_fraction(0.0), Crackiness::High);
    }

    #[test]
    fn the_flag_byte_is_defined_at_index_sixteen_and_defaults_to_wild() {
        let entity = base_of(&golem_nbt());
        assert_eq!(entity.base.data.byte(16), 0);
        assert!(!is_player_created(&entity.base));
    }

    #[test]
    fn player_created_sets_only_bit_zero_of_the_flags() {
        let mut entity = base_of(&golem_nbt());
        set_player_created(&mut entity.base, true);
        assert_eq!(entity.base.data.byte(16), 1);
        entity.base.data.set(16, SyncedValue::Byte(0b110));
        set_player_created(&mut entity.base, true);
        assert_eq!(entity.base.data.byte(16), 0b111);
        set_player_created(&mut entity.base, false);
        assert_eq!(entity.base.data.byte(16), 0b110);
    }

    #[test]
    fn player_created_and_anger_round_trip_through_nbt() {
        let nbt = golem_nbt()
            .with("PlayerCreated", true)
            .with("anger_end_time", 500i64);
        let entity = base_of(&nbt);
        assert!(is_player_created(&entity.base));
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_bool("PlayerCreated"), Some(true));
        assert_eq!(saved.get_i64("anger_end_time"), Some(500));
        let again = base_of(&saved);
        assert_eq!(save_entity(&again, None), Some(saved));
    }

    #[test]
    fn a_missing_key_reads_as_not_player_created() {
        let saved = save_entity(&base_of(&golem_nbt()), None).unwrap();
        assert_eq!(saved.get_bool("PlayerCreated"), Some(false));
    }
}
