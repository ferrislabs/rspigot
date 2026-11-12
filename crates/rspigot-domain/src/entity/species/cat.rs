use rspigot_nbt::Compound;

use crate::entity::base::EntityBase;
use crate::entity::living::LivingState;
use crate::entity::spawn::{SpawnEnvironment, ageable_finalize};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const KEYS: &[&str] = &[];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CatState;

pub fn define(_base: &mut EntityBase) -> CatState {
    CatState
}

pub fn read(_nbt: &Compound, _base: &mut EntityBase, _state: &mut CatState) {}

pub fn write(_base: &EntityBase, _state: &CatState, _out: &mut Compound) {}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    _environment: &SpawnEnvironment,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    ageable_finalize(base, living, group, level)
}
