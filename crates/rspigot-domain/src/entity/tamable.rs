use uuid::Uuid;

use super::base::EntityBase;
use super::data::SyncedValue;

pub const DATA_FLAGS: u8 = 18;
pub const DATA_OWNER: u8 = 19;
pub const FLAG_SITTING: u8 = 1;
pub const FLAG_TAME: u8 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TamableState {
    pub ordered_to_sit: bool,
}

pub fn define(base: &mut EntityBase) {
    base.data.define(DATA_FLAGS, SyncedValue::Byte(0));
    base.data
        .define(DATA_OWNER, SyncedValue::OptionalEntity(None));
}

pub fn is_tame(base: &EntityBase) -> bool {
    base.data.flag(DATA_FLAGS, FLAG_TAME)
}

pub fn set_tame(base: &mut EntityBase, tame: bool) {
    base.data.set_flag(DATA_FLAGS, FLAG_TAME, tame);
}

pub fn is_in_sitting_pose(base: &EntityBase) -> bool {
    base.data.flag(DATA_FLAGS, FLAG_SITTING)
}

pub fn set_in_sitting_pose(base: &mut EntityBase, sitting: bool) {
    base.data.set_flag(DATA_FLAGS, FLAG_SITTING, sitting);
}

pub fn owner(base: &EntityBase) -> Option<Uuid> {
    base.data.entity(DATA_OWNER)
}

pub fn set_owner(base: &mut EntityBase, owner: Option<Uuid>) {
    base.data
        .set(DATA_OWNER, SyncedValue::OptionalEntity(owner));
}

pub fn tame(base: &mut EntityBase, owner: Uuid) {
    set_tame(base, true);
    set_owner(base, Some(owner));
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::EntityId;

    fn wolf() -> EntityBase {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Wolf);
        define(&mut base);
        base
    }

    #[test]
    fn flags_share_one_byte() {
        let mut base = wolf();
        set_tame(&mut base, true);
        set_in_sitting_pose(&mut base, true);
        assert_eq!(base.data.byte(DATA_FLAGS), 5);
        set_in_sitting_pose(&mut base, false);
        assert!(is_tame(&base));
        assert!(!is_in_sitting_pose(&base));
    }

    #[test]
    fn taming_sets_the_flag_and_the_owner() {
        let mut base = wolf();
        assert_eq!(owner(&base), None);
        tame(&mut base, Uuid::from_u128(9));
        assert!(is_tame(&base));
        assert_eq!(owner(&base), Some(Uuid::from_u128(9)));
    }
}
