use rspigot_data::variants::Farm;

use uuid::Uuid;

use crate::item::ItemStack;
use crate::text::Component;
use crate::{BlockPos, BlockStateId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VariantKind {
    Mob(Farm),
    Sound(Farm),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pose {
    Standing,
    FallFlying,
    Sleeping,
    Swimming,
    SpinAttack,
    Crouching,
    LongJumping,
    Dying,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SyncedValue {
    Byte(i8),
    Int(i32),
    Float(f32),
    OptionalComponent(Option<Component>),
    Boolean(bool),
    Pose(Pose),
    Item(Option<ItemStack>),
    BlockPos(BlockPos),
    Variant(VariantKind, u32),
    Long(i64),
    OptionalEntity(Option<Uuid>),
    OptionalBlockState(Option<BlockStateId>),
}

impl SyncedValue {
    fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Float(a), Self::Float(b)) => a.to_bits() == b.to_bits(),
            (a, b) => a == b,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct DataSlot {
    index: u8,
    value: SyncedValue,
    default: SyncedValue,
    dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SyncedData {
    slots: Vec<DataSlot>,
    dirty: bool,
}

impl SyncedData {
    pub fn define(&mut self, index: u8, default: SyncedValue) {
        let slot = DataSlot {
            index,
            value: default.clone(),
            default,
            dirty: false,
        };
        match self.slots.binary_search_by_key(&index, |s| s.index) {
            Ok(found) => self.slots[found] = slot,
            Err(at) => self.slots.insert(at, slot),
        }
    }

    pub fn get(&self, index: u8) -> Option<&SyncedValue> {
        self.slot(index).map(|s| &s.value)
    }

    fn slot(&self, index: u8) -> Option<&DataSlot> {
        self.slots
            .binary_search_by_key(&index, |s| s.index)
            .ok()
            .map(|i| &self.slots[i])
    }

    pub fn set(&mut self, index: u8, value: SyncedValue) -> bool {
        let Ok(found) = self.slots.binary_search_by_key(&index, |s| s.index) else {
            return false;
        };
        let slot = &mut self.slots[found];
        if slot.value.same(&value) {
            return false;
        }
        slot.value = value;
        slot.dirty = true;
        self.dirty = true;
        true
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn pack_dirty(&mut self) -> Option<Vec<(u8, SyncedValue)>> {
        if !self.dirty {
            return None;
        }
        self.dirty = false;
        let packed: Vec<(u8, SyncedValue)> = self
            .slots
            .iter_mut()
            .filter(|s| s.dirty)
            .map(|s| {
                s.dirty = false;
                (s.index, s.value.clone())
            })
            .collect();
        Some(packed)
    }

    pub fn non_default(&self) -> Vec<(u8, SyncedValue)> {
        self.slots
            .iter()
            .filter(|s| !s.value.same(&s.default))
            .map(|s| (s.index, s.value.clone()))
            .collect()
    }

    pub fn byte(&self, index: u8) -> i8 {
        match self.get(index) {
            Some(SyncedValue::Byte(v)) => *v,
            _ => 0,
        }
    }

    pub fn int(&self, index: u8) -> i32 {
        match self.get(index) {
            Some(SyncedValue::Int(v)) => *v,
            _ => 0,
        }
    }

    pub fn float(&self, index: u8) -> f32 {
        match self.get(index) {
            Some(SyncedValue::Float(v)) => *v,
            _ => 0.0,
        }
    }

    pub fn long(&self, index: u8) -> i64 {
        match self.get(index) {
            Some(SyncedValue::Long(v)) => *v,
            _ => 0,
        }
    }

    pub fn entity(&self, index: u8) -> Option<Uuid> {
        match self.get(index) {
            Some(SyncedValue::OptionalEntity(v)) => *v,
            _ => None,
        }
    }

    pub fn block_state(&self, index: u8) -> Option<BlockStateId> {
        match self.get(index) {
            Some(SyncedValue::OptionalBlockState(v)) => *v,
            _ => None,
        }
    }

    pub fn variant(&self, index: u8) -> Option<u32> {
        match self.get(index) {
            Some(SyncedValue::Variant(_, id)) => Some(*id),
            _ => None,
        }
    }

    pub fn boolean(&self, index: u8) -> bool {
        matches!(self.get(index), Some(SyncedValue::Boolean(true)))
    }

    pub fn set_flag(&mut self, index: u8, mask: u8, value: bool) {
        let current = self.byte(index) as u8;
        let next = if value {
            current | mask
        } else {
            current & !mask
        };
        self.set(index, SyncedValue::Byte(next as i8));
    }

    pub fn flag(&self, index: u8, mask: u8) -> bool {
        self.byte(index) as u8 & mask != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_changed_slots_are_dirty_and_defaults_are_skipped() {
        let mut data = SyncedData::default();
        data.define(9, SyncedValue::Float(1.0));
        data.define(0, SyncedValue::Byte(0));
        assert!(data.non_default().is_empty());
        assert!(data.pack_dirty().is_none());
        assert!(data.set(9, SyncedValue::Float(10.0)));
        assert!(!data.set(9, SyncedValue::Float(10.0)));
        assert_eq!(data.pack_dirty(), Some(vec![(9, SyncedValue::Float(10.0))]));
        assert!(data.pack_dirty().is_none());
        assert_eq!(data.non_default(), vec![(9, SyncedValue::Float(10.0))]);
        data.set_flag(0, 0x02, true);
        assert!(data.flag(0, 0x02));
        assert_eq!(data.pack_dirty(), Some(vec![(0, SyncedValue::Byte(2))]));
    }

    #[test]
    fn negative_zero_is_a_change_like_float_equals() {
        let mut data = SyncedData::default();
        data.define(9, SyncedValue::Float(0.0));
        assert!(data.set(9, SyncedValue::Float(-0.0)));
    }
}
