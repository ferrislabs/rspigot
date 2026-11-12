use rspigot_nbt::Compound;

use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::LivingState;
use crate::entity::spawn::{SpawnEnvironment, mob_finalize};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const DATA_FLAGS: u8 = 16;
pub const FLAG_CHARGED: u8 = 1;
pub const KEYS: &[&str] = &[];
pub const INITIAL_HEIGHT_OFFSET: f32 = 0.5;
pub const HEIGHT_OFFSET_INTERVAL: i32 = 100;
pub const HEIGHT_OFFSET_MEAN: f64 = 0.5;
pub const HEIGHT_OFFSET_SPREAD: f64 = 6.891;
pub const FALL_DRAG: f64 = 0.6;
pub const CLIMB_TARGET: f64 = 0.300_000_011_920_928_96;
pub const CLIMB_BLEND: f64 = 0.300_000_011_920_928_96;
pub const BURN_SOUND_ODDS: i32 = 24;
pub const BURN_SOUND: &str = "minecraft:entity.blaze.burn";
pub const AMBIENT_SOUND: &str = "minecraft:entity.blaze.ambient";
pub const SHOOT_EVENT: i32 = 1018;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlazeState {
    pub allowed_height_offset: f32,
    pub next_height_offset_change_tick: i32,
}

impl Default for BlazeState {
    fn default() -> Self {
        Self {
            allowed_height_offset: INITIAL_HEIGHT_OFFSET,
            next_height_offset_change_tick: 0,
        }
    }
}

pub fn define(base: &mut EntityBase) -> BlazeState {
    base.data.define(DATA_FLAGS, SyncedValue::Byte(0));
    BlazeState::default()
}

pub fn is_charged(base: &EntityBase) -> bool {
    base.data.flag(DATA_FLAGS, FLAG_CHARGED)
}

pub fn set_charged(base: &mut EntityBase, charged: bool) {
    base.data.set_flag(DATA_FLAGS, FLAG_CHARGED, charged);
}

pub fn read(_nbt: &Compound, _base: &mut EntityBase, _state: &mut BlazeState) {}

pub fn write(_base: &EntityBase, _state: &BlazeState, _out: &mut Compound) {}

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
    use rspigot_entities::EntityKind;
    use rspigot_nbt::Compound;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::save::{load_entity, save_entity};

    fn blaze() -> crate::entity::Entity {
        load_entity(
            &Compound::new().with("id", "minecraft:blaze"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap()
    }

    #[test]
    fn the_flags_byte_sits_at_index_sixteen_and_only_touches_the_charged_bit() {
        let mut entity = blaze();
        assert_eq!(entity.base.kind, EntityKind::Blaze);
        assert_eq!(entity.base.data.byte(DATA_FLAGS), 0);
        assert!(!is_charged(&entity.base));
        set_charged(&mut entity.base, true);
        assert_eq!(entity.base.data.byte(16), 1);
        entity.base.data.set(16, SyncedValue::Byte(0b110));
        set_charged(&mut entity.base, true);
        assert_eq!(entity.base.data.byte(16), 0b111);
        set_charged(&mut entity.base, false);
        assert_eq!(entity.base.data.byte(16), 0b110);
    }

    #[test]
    fn a_new_blaze_hovers_half_a_block_above_its_target_and_rerolls_at_once() {
        let state = BlazeState::default();
        assert_eq!(state.allowed_height_offset, 0.5);
        assert_eq!(state.next_height_offset_change_tick, 0);
    }

    #[test]
    fn the_blaze_saves_nothing_beyond_the_living_entity_data() {
        let entity = blaze();
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(
            save_entity(
                &load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap(),
                None
            ),
            Some(saved)
        );
    }
}
