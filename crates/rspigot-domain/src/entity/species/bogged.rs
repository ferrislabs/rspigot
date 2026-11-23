use rspigot_nbt::Compound;

use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;

pub const DATA_SHEARED: u8 = 16;
pub const KEYS: &[&str] = &["sheared"];
pub const SHEAR_SOUND: &str = "minecraft:entity.bogged.shear";
pub const SHEAR_TABLE: &str = "minecraft:shearing/bogged";

pub fn define(base: &mut EntityBase) {
    base.data.define(DATA_SHEARED, SyncedValue::Boolean(false));
}

pub fn is_sheared(base: &EntityBase) -> bool {
    base.data.boolean(DATA_SHEARED)
}

pub fn set_sheared(base: &mut EntityBase, sheared: bool) {
    base.data.set(DATA_SHEARED, SyncedValue::Boolean(sheared));
}

pub fn read(nbt: &Compound, base: &mut EntityBase) {
    set_sheared(base, nbt.get_bool("sheared").unwrap_or(false));
}

pub fn write(base: &EntityBase, out: &mut Compound) {
    out.insert("sheared", is_sheared(base));
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;

    #[test]
    fn the_sheared_flag_round_trips_through_nbt() {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Bogged);
        define(&mut base);
        assert!(!is_sheared(&base));
        set_sheared(&mut base, true);
        let mut nbt = Compound::new();
        write(&base, &mut nbt);
        let mut loaded = EntityBase::new(EntityId(2), Uuid::from_u128(2), EntityKind::Bogged);
        define(&mut loaded);
        read(&nbt, &mut loaded);
        assert!(is_sheared(&loaded));
    }
}
