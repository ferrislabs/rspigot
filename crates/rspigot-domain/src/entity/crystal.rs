use rspigot_nbt::{Compound, Tag};

use super::base::EntityBase;
use super::data::SyncedValue;
use crate::BlockPos;

pub const DATA_BEAM_TARGET: u8 = 8;
pub const DATA_SHOW_BOTTOM: u8 = 9;
pub const KEYS: &[&str] = &["beam_target", "ShowBottom"];
pub const EXPLOSION_RADIUS: f32 = 6.0;
pub const TIME_RANGE: i32 = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CrystalState {
    pub time: i32,
}

pub fn define(base: &mut EntityBase) -> CrystalState {
    base.data
        .define(DATA_BEAM_TARGET, SyncedValue::OptionalBlockPos(None));
    base.data
        .define(DATA_SHOW_BOTTOM, SyncedValue::Boolean(true));
    CrystalState::default()
}

pub fn beam_target(base: &EntityBase) -> Option<BlockPos> {
    match base.data.get(DATA_BEAM_TARGET) {
        Some(SyncedValue::OptionalBlockPos(pos)) => *pos,
        _ => None,
    }
}

pub fn set_beam_target(base: &mut EntityBase, target: Option<BlockPos>) {
    base.data
        .set(DATA_BEAM_TARGET, SyncedValue::OptionalBlockPos(target));
}

pub fn shows_bottom(base: &EntityBase) -> bool {
    base.data.boolean(DATA_SHOW_BOTTOM)
}

pub fn set_shows_bottom(base: &mut EntityBase, shows: bool) {
    base.data.set(DATA_SHOW_BOTTOM, SyncedValue::Boolean(shows));
}

pub fn read(nbt: &Compound, base: &mut EntityBase) {
    let target = nbt
        .get_int_array("beam_target")
        .filter(|parts| parts.len() == 3)
        .map(|parts| BlockPos::new(parts[0], parts[1], parts[2]));
    set_beam_target(base, target);
    set_shows_bottom(base, nbt.get_bool("ShowBottom").unwrap_or(true));
}

pub fn write(base: &EntityBase, out: &mut Compound) {
    if let Some(pos) = beam_target(base) {
        out.insert("beam_target", Tag::IntArray(vec![pos.x, pos.y, pos.z]));
    }
    out.insert("ShowBottom", shows_bottom(base));
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::save::{load_entity, save_entity};
    use crate::entity::{Entity, EntityState};

    fn crystal(nbt: Compound) -> Entity {
        load_entity(
            &nbt.with("id", "minecraft:end_crystal"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap()
    }

    #[test]
    fn a_fresh_crystal_shows_its_bottom_and_has_no_beam() {
        let entity = crystal(Compound::new());
        assert!(matches!(entity.state, EntityState::Crystal(_)));
        assert!(shows_bottom(&entity.base));
        assert_eq!(beam_target(&entity.base), None);
        assert_eq!(
            entity.base.data.get(8),
            Some(&SyncedValue::OptionalBlockPos(None))
        );
        assert_eq!(entity.base.data.get(9), Some(&SyncedValue::Boolean(true)));
    }

    #[test]
    fn the_beam_target_and_the_bottom_flag_round_trip() {
        let entity = crystal(
            Compound::new()
                .with("beam_target", Tag::IntArray(vec![0, 128, 0]))
                .with("ShowBottom", false),
        );
        assert_eq!(beam_target(&entity.base), Some(BlockPos::new(0, 128, 0)));
        assert!(!shows_bottom(&entity.base));
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_int_array("beam_target"), Some(&[0, 128, 0][..]));
        assert_eq!(saved.get_bool("ShowBottom"), Some(false));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
    }

    #[test]
    fn a_crystal_without_a_target_does_not_write_the_key() {
        let saved = save_entity(&crystal(Compound::new()), None).unwrap();
        assert!(!saved.contains_key("beam_target"));
        assert_eq!(saved.get_bool("ShowBottom"), Some(true));
    }

    #[test]
    fn the_invulnerable_flag_of_the_base_is_kept() {
        let entity = crystal(Compound::new().with("Invulnerable", true));
        assert!(entity.base.invulnerable);
    }
}
