use rspigot_items::{EquipmentSlot, ItemKind};
use rspigot_nbt::Compound;
use uuid::Uuid;

use crate::BlockPos;
use crate::crossbow::single;
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::LivingState;
use crate::entity::living::slot_index;
use crate::entity::save::{read_uuid, uuid_tag};
use crate::entity::spawn::{SpawnEnvironment, mob_finalize, set_item_slot};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const DATA_FLAGS: u8 = 16;
pub const FLAG_CHARGING: i8 = 1;
pub const KEYS: &[&str] = &["bound_pos", "life_ticks", "owner"];
pub const LIFE_DAMAGE_INTERVAL: i32 = 20;
pub const LIFE_DAMAGE: f32 = 1.0;
pub const SWORD_DROP_CHANCE: f32 = 0.0;
const ENCHANT_CHANCE: f32 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VexState {
    pub bound_origin: Option<BlockPos>,
    pub life_ticks: Option<i32>,
    pub owner: Option<Uuid>,
}

pub fn define(base: &mut EntityBase) -> VexState {
    base.data.define(DATA_FLAGS, SyncedValue::Byte(0));
    VexState::default()
}

fn flags(base: &EntityBase) -> i8 {
    match base.data.get(DATA_FLAGS) {
        Some(SyncedValue::Byte(flags)) => *flags,
        _ => 0,
    }
}

pub fn is_charging(base: &EntityBase) -> bool {
    flags(base) & FLAG_CHARGING != 0
}

pub fn set_charging(base: &mut EntityBase, charging: bool) {
    let flags = flags(base);
    let flags = if charging {
        flags | FLAG_CHARGING
    } else {
        flags & !FLAG_CHARGING
    };
    base.data.set(DATA_FLAGS, SyncedValue::Byte(flags));
}

pub fn set_limited_life(state: &mut VexState, ticks: i32) {
    state.life_ticks = Some(ticks);
}

pub fn tick_life(state: &mut VexState) -> bool {
    let Some(ticks) = state.life_ticks.as_mut() else {
        return false;
    };
    *ticks -= 1;
    if *ticks < 1 {
        *ticks = LIFE_DAMAGE_INTERVAL;
        return true;
    }
    false
}

pub fn read(nbt: &Compound, _base: &mut EntityBase, state: &mut VexState) {
    state.bound_origin = nbt
        .get_int_array("bound_pos")
        .filter(|pos| pos.len() == 3)
        .map(|pos| BlockPos::new(pos[0], pos[1], pos[2]));
    state.life_ticks = nbt.get_i32("life_ticks");
    state.owner = nbt.get("owner").and_then(read_uuid);
}

pub fn write(_base: &EntityBase, state: &VexState, out: &mut Compound) {
    if let Some(pos) = state.bound_origin {
        out.insert("bound_pos", vec![pos.x, pos.y, pos.z]);
    }
    if let Some(ticks) = state.life_ticks {
        out.insert("life_ticks", ticks);
    }
    if let Some(owner) = state.owner {
        out.insert("owner", uuid_tag(owner));
    }
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    _environment: &SpawnEnvironment,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    set_item_slot(
        living,
        EquipmentSlot::Mainhand,
        Some(single(ItemKind::IronSword)),
    );
    if let Some(mob) = living.mob.as_mut() {
        mob.drop_chances[slot_index(EquipmentSlot::Mainhand)] = SWORD_DROP_CHANCE;
    }
    let _ = level.next_float() < ENCHANT_CHANCE;
    mob_finalize(base, living, group, level)
}

#[cfg(test)]
mod tests {
    use rspigot_nbt::Compound;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::save::{load_entity, save_entity};
    use crate::entity::species::Species;

    fn vex(nbt: Compound) -> crate::entity::Entity {
        load_entity(
            &nbt.with("id", "minecraft:vex"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap()
    }

    fn state(entity: &crate::entity::Entity) -> VexState {
        match entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .map(|mob| mob.species)
        {
            Some(Species::Vex(state)) => state,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_flags_sit_at_index_sixteen_and_the_charging_bit_is_the_lowest() {
        let mut entity = vex(Compound::new());
        assert_eq!(
            entity.base.data.get(DATA_FLAGS),
            Some(&SyncedValue::Byte(0))
        );
        assert!(!is_charging(&entity.base));
        set_charging(&mut entity.base, true);
        assert_eq!(
            entity.base.data.get(DATA_FLAGS),
            Some(&SyncedValue::Byte(1))
        );
        assert!(is_charging(&entity.base));
        set_charging(&mut entity.base, false);
        assert!(!is_charging(&entity.base));
    }

    #[test]
    fn the_bound_position_the_life_and_the_owner_round_trip() {
        let owner = Uuid::from_u128(77);
        let entity = vex(Compound::new()
            .with("bound_pos", vec![5, 70, -9])
            .with("life_ticks", 400)
            .with("owner", uuid_tag(owner)));
        let loaded = state(&entity);
        assert_eq!(loaded.bound_origin, Some(BlockPos::new(5, 70, -9)));
        assert_eq!(loaded.life_ticks, Some(400));
        assert_eq!(loaded.owner, Some(owner));
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_int_array("bound_pos"), Some(&[5, 70, -9][..]));
        assert_eq!(saved.get_i32("life_ticks"), Some(400));
        assert_eq!(saved.get("owner"), Some(&uuid_tag(owner)));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
    }

    #[test]
    fn a_vex_without_a_limited_life_saves_no_life_ticks() {
        let entity = vex(Compound::new());
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get("life_ticks"), None);
        assert_eq!(saved.get("bound_pos"), None);
        assert_eq!(saved.get("owner"), None);
    }

    #[test]
    fn the_limited_life_starves_one_heart_every_twenty_ticks_once_it_ran_out() {
        let mut state = VexState::default();
        assert!(!tick_life(&mut state));
        set_limited_life(&mut state, 3);
        assert!(!tick_life(&mut state));
        assert!(!tick_life(&mut state));
        assert!(tick_life(&mut state));
        assert_eq!(state.life_ticks, Some(LIFE_DAMAGE_INTERVAL));
        for _ in 0..19 {
            assert!(!tick_life(&mut state));
        }
        assert!(tick_life(&mut state));
    }

    #[test]
    fn a_vex_spawns_with_an_iron_sword_that_never_drops() {
        use crate::entity::spawn::{SpawnEnvironment, finalize_spawn};
        let mut entity = vex(Compound::new());
        let environment = SpawnEnvironment {
            biome: crate::BiomeId(0),
            difficulty: crate::difficulty::DifficultyInstance::new(
                crate::world::Difficulty::Normal,
                0,
                0,
                1.0,
            ),
            halloween: false,
            moon_brightness: 0.0,
        };
        let mut level = crate::random::LegacyRandom::new(5);
        let mut own = crate::random::LegacyRandom::new(6);
        finalize_spawn(
            &mut entity,
            &environment,
            crate::spawn::SpawnReason::Natural,
            None,
            &mut level,
            &mut own,
        );
        let living = entity.living().unwrap();
        let sword = living.equipment(EquipmentSlot::Mainhand).unwrap();
        assert_eq!(sword.item, ItemKind::IronSword.id());
        assert_eq!(
            living.mob.as_ref().unwrap().drop_chances[slot_index(EquipmentSlot::Mainhand)],
            0.0
        );
    }
}
