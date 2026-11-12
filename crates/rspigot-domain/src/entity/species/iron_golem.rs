use rspigot_nbt::Compound;

use crate::entity::base::EntityBase;
use crate::entity::living::LivingState;
use crate::entity::spawn::{SpawnEnvironment, mob_finalize};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const KEYS: &[&str] = &[];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IronGolemState;

pub fn define(_base: &mut EntityBase) -> IronGolemState {
    IronGolemState
}

pub fn read(_nbt: &Compound, _base: &mut EntityBase, _state: &mut IronGolemState) {}

pub fn write(_base: &EntityBase, _state: &IronGolemState, _out: &mut Compound) {}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    _environment: &SpawnEnvironment,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    mob_finalize(base, living, group, level)
}
