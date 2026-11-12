use std::borrow::Cow;

use rspigot_nbt::Compound;

use crate::BlockStateId;
use crate::effect::{AttributeModifier, AttributeOperation};
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::LivingState;
use crate::entity::physics::has_tag;
use crate::entity::save::{block_state_from_nbt, block_state_to_nbt};
use crate::entity::spawn::{SpawnEnvironment, mob_finalize};
use crate::entity::species::Species;
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const DATA_CARRY_STATE: u8 = 16;
pub const DATA_CREEPY: u8 = 17;
pub const DATA_STARED_AT: u8 = 18;
pub const ATTACKING_MODIFIER: &str = "minecraft:attacking";
pub const ATTACKING_SPEED_BONUS: f64 = 0.15_f32 as f64;
pub const CARRIED_BLOCK_KEY: &str = "carriedBlockState";
pub const KEYS: &[&str] = &[CARRIED_BLOCK_KEY];
const MOVEMENT_SPEED: &str = "minecraft:movement_speed";
const HOLDABLE: &str = "minecraft:enderman_holdable";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EndermanState {
    pub target_change_time: i32,
}

pub fn define(base: &mut EntityBase) -> EndermanState {
    base.data
        .define(DATA_CARRY_STATE, SyncedValue::OptionalBlockState(None));
    base.data.define(DATA_CREEPY, SyncedValue::Boolean(false));
    base.data
        .define(DATA_STARED_AT, SyncedValue::Boolean(false));
    EndermanState::default()
}

pub fn carried_block(base: &EntityBase) -> Option<BlockStateId> {
    base.data.block_state(DATA_CARRY_STATE)
}

pub fn set_carried_block(base: &mut EntityBase, state: Option<BlockStateId>) {
    base.data
        .set(DATA_CARRY_STATE, SyncedValue::OptionalBlockState(state));
}

pub fn is_creepy(base: &EntityBase) -> bool {
    base.data.boolean(DATA_CREEPY)
}

pub fn has_been_stared_at(base: &EntityBase) -> bool {
    base.data.boolean(DATA_STARED_AT)
}

pub fn set_being_stared_at(base: &mut EntityBase) {
    base.data.set(DATA_STARED_AT, SyncedValue::Boolean(true));
}

pub fn is_holdable(state: BlockStateId) -> bool {
    has_tag(state, HOLDABLE)
}

pub fn carries_block(base: &EntityBase) -> bool {
    carried_block(base).is_some()
}

pub fn target_changed(base: &mut EntityBase, living: &mut LivingState, targeted: bool) {
    let tick_count = base.tick_count;
    let Some(Species::Enderman(state)) = living.mob.as_mut().map(|mob| &mut mob.species) else {
        return;
    };
    if targeted {
        state.target_change_time = tick_count;
        base.data.set(DATA_CREEPY, SyncedValue::Boolean(true));
        if !living
            .attributes
            .has_modifier(MOVEMENT_SPEED, ATTACKING_MODIFIER)
        {
            living.attributes.add_modifier(
                MOVEMENT_SPEED,
                AttributeModifier {
                    id: Cow::Borrowed(ATTACKING_MODIFIER),
                    amount: ATTACKING_SPEED_BONUS,
                    operation: AttributeOperation::AddValue,
                },
            );
        }
    } else {
        state.target_change_time = 0;
        base.data.set(DATA_CREEPY, SyncedValue::Boolean(false));
        base.data.set(DATA_STARED_AT, SyncedValue::Boolean(false));
        living
            .attributes
            .remove_modifier(MOVEMENT_SPEED, ATTACKING_MODIFIER);
    }
}

pub fn read(nbt: &Compound, base: &mut EntityBase, _state: &mut EndermanState) {
    let carried = nbt
        .get_compound(CARRIED_BLOCK_KEY)
        .map(block_state_from_nbt)
        .filter(|state| !state.is_air());
    set_carried_block(base, carried);
}

pub fn write(base: &EntityBase, _state: &EndermanState, out: &mut Compound) {
    if let Some(carried) = carried_block(base) {
        out.insert(CARRIED_BLOCK_KEY, block_state_to_nbt(carried));
    }
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
    use rspigot_blocks::BlockKind;
    use rspigot_entities::EntityKind;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::living::LivingState;

    fn enderman() -> (EntityBase, LivingState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Enderman);
        let living = LivingState::new(&mut base, 0.0);
        (base, living)
    }

    #[test]
    fn synced_data_uses_indices_sixteen_to_eighteen() {
        let (base, _) = enderman();
        assert_eq!(carried_block(&base), None);
        assert!(!is_creepy(&base));
        assert!(!has_been_stared_at(&base));
        assert_eq!(
            base.data.get(16),
            Some(&SyncedValue::OptionalBlockState(None))
        );
        assert_eq!(base.data.get(17), Some(&SyncedValue::Boolean(false)));
        assert_eq!(base.data.get(18), Some(&SyncedValue::Boolean(false)));
    }

    #[test]
    fn carried_block_round_trips_through_nbt_with_properties() {
        let (mut base, _) = enderman();
        let grass = BlockKind::GrassBlock
            .state_with(&[("snowy", "true")])
            .expect("snowy grass");
        set_carried_block(&mut base, Some(grass));
        let mut out = Compound::new();
        write(&base, &EndermanState::default(), &mut out);
        let saved = out.get_compound(CARRIED_BLOCK_KEY).expect("saved block");
        assert_eq!(saved.get_str("Name"), Some("minecraft:grass_block"));
        let (mut loaded, _) = enderman();
        read(&out, &mut loaded, &mut EndermanState::default());
        assert_eq!(carried_block(&loaded), Some(grass));
    }

    #[test]
    fn nothing_is_written_without_a_carried_block() {
        let (base, _) = enderman();
        let mut out = Compound::new();
        write(&base, &EndermanState::default(), &mut out);
        assert!(out.get_compound(CARRIED_BLOCK_KEY).is_none());
    }

    #[test]
    fn air_and_unknown_blocks_are_not_carried() {
        let (mut base, _) = enderman();
        let air = Compound::new().with("Name", "minecraft:air");
        let nbt = Compound::new().with(CARRIED_BLOCK_KEY, air);
        read(&nbt, &mut base, &mut EndermanState::default());
        assert_eq!(carried_block(&base), None);
        let unknown = Compound::new().with("Name", "minecraft:not_a_block");
        let nbt = Compound::new().with(CARRIED_BLOCK_KEY, unknown);
        read(&nbt, &mut base, &mut EndermanState::default());
        assert_eq!(carried_block(&base), None);
    }

    #[test]
    fn only_tagged_blocks_are_holdable() {
        assert!(is_holdable(BlockKind::Dirt.default_state()));
        assert!(is_holdable(BlockKind::Pumpkin.default_state()));
        assert!(is_holdable(BlockKind::Poppy.default_state()));
        assert!(is_holdable(BlockKind::GrassBlock.default_state()));
        assert!(!is_holdable(BlockKind::Stone.default_state()));
        assert!(!is_holdable(BlockKind::Bedrock.default_state()));
    }

    #[test]
    fn a_target_makes_it_creepy_and_faster_until_it_is_lost() {
        let (mut base, mut living) = enderman();
        base.tick_count = 42;
        living.mob.as_mut().unwrap().species = Species::Enderman(EndermanState::default());
        target_changed(&mut base, &mut living, true);
        assert!(is_creepy(&base));
        assert!(
            living
                .attributes
                .has_modifier(MOVEMENT_SPEED, ATTACKING_MODIFIER)
        );
        assert_eq!(
            living.attribute(MOVEMENT_SPEED),
            f64::from(0.3_f32) + f64::from(0.15_f32)
        );
        let Some(Species::Enderman(state)) = living.mob.as_ref().map(|mob| mob.species) else {
            panic!("not an enderman");
        };
        assert_eq!(state.target_change_time, 42);
        set_being_stared_at(&mut base);
        target_changed(&mut base, &mut living, false);
        assert!(!is_creepy(&base));
        assert!(!has_been_stared_at(&base));
        assert!(
            !living
                .attributes
                .has_modifier(MOVEMENT_SPEED, ATTACKING_MODIFIER)
        );
        let Some(Species::Enderman(state)) = living.mob.as_ref().map(|mob| mob.species) else {
            panic!("not an enderman");
        };
        assert_eq!(state.target_change_time, 0);
    }
}
