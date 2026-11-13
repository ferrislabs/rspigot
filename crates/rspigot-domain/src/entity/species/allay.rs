use rspigot_nbt::Compound;

use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::LivingState;
use crate::entity::spawn::{SpawnEnvironment, mob_finalize};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const DATA_DANCING: u8 = 16;
pub const DATA_CAN_DUPLICATE: u8 = 17;
pub const KEYS: &[&str] = &["DuplicationCooldown"];
pub const DUPLICATION_COOLDOWN: i64 = 6000;
pub const DUPLICATION_ITEM_TAG: &str = "minecraft:duplicates_allays";
pub const REGEN_INTERVAL: i32 = 10;
pub const REGEN_AMOUNT: f32 = 1.0;
pub const PICKUP_REACH: (f64, f64, f64) = (1.0, 1.0, 1.0);
pub const LIKED_NOTEBLOCK_COOLDOWN: i32 = 600;
pub const LIKED_PLAYER_RANGE: f64 = 64.0;
pub const NOTEBLOCK_REACH: i32 = 1024;
pub const THROW_YAW_OFFSET: f32 = 0.2;
pub const THROW_VELOCITY: (f64, f64, f64) = (0.2, 0.3, 0.2);
pub const THROW_TARGET_LIFT: f64 = 1.0;
pub const THROW_SOUND_EVERY: i64 = 7;
pub const THROW_SOUND_CHANCE: f64 = 0.9;
pub const THROW_SOUND_PITCHES: [f32; 15] = [
    0.5625, 0.625, 0.75, 0.9375, 1.0, 1.0, 1.125, 1.25, 1.5, 1.875, 2.0, 2.25, 2.5, 3.0, 3.75,
];
pub const DUPLICATE_EVENT: i8 = 18;
pub const AMBIENT_WITH_ITEM: &str = "minecraft:entity.allay.ambient_with_item";
pub const AMBIENT_WITHOUT_ITEM: &str = "minecraft:entity.allay.ambient_without_item";
pub const THROW_SOUND: &str = "minecraft:entity.allay.item_thrown";
pub const ITEM_GIVEN_SOUND: &str = "minecraft:entity.allay.item_given";
pub const ITEM_TAKEN_SOUND: &str = "minecraft:entity.allay.item_taken";
pub const CHIME_SOUND: &str = "minecraft:block.amethyst_block.chime";
pub const SOUND_VOLUME: f32 = 0.4;
pub const INTERACT_SOUND_VOLUME: f32 = 2.0;
pub const INVENTORY_KEY: &str = "Inventory";
pub const LISTENER_KEY: &str = "listener";
pub const LISTENER_RADIUS: i32 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AllayState {
    pub duplication_cooldown: i64,
}

pub fn define(base: &mut EntityBase) -> AllayState {
    base.data.define(DATA_DANCING, SyncedValue::Boolean(false));
    base.data
        .define(DATA_CAN_DUPLICATE, SyncedValue::Boolean(true));
    AllayState::default()
}

pub fn is_dancing(base: &EntityBase) -> bool {
    base.data.boolean(DATA_DANCING)
}

pub fn set_dancing(base: &mut EntityBase, dancing: bool) {
    base.data.set(DATA_DANCING, SyncedValue::Boolean(dancing));
}

pub fn can_duplicate(base: &EntityBase) -> bool {
    base.data.boolean(DATA_CAN_DUPLICATE)
}

pub fn set_duplication_cooldown(base: &mut EntityBase, state: &mut AllayState, cooldown: i64) {
    state.duplication_cooldown = cooldown;
    base.data
        .set(DATA_CAN_DUPLICATE, SyncedValue::Boolean(cooldown <= 0));
}

pub fn reset_duplication_cooldown(base: &mut EntityBase, state: &mut AllayState) {
    set_duplication_cooldown(base, state, DUPLICATION_COOLDOWN);
}

pub fn tick_duplication_cooldown(base: &mut EntityBase, state: &mut AllayState) {
    if state.duplication_cooldown > 0 {
        let next = state.duplication_cooldown - 1;
        set_duplication_cooldown(base, state, next);
    }
}

pub fn read(nbt: &Compound, base: &mut EntityBase, state: &mut AllayState) {
    let cooldown = nbt.get_i64("DuplicationCooldown").unwrap_or(0);
    set_duplication_cooldown(base, state, cooldown);
}

pub fn write(_base: &EntityBase, state: &AllayState, out: &mut Compound) {
    out.insert("DuplicationCooldown", state.duplication_cooldown);
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
    use crate::entity::species::Species;

    fn allay(nbt: Compound) -> crate::entity::Entity {
        load_entity(
            &nbt.with("id", "minecraft:allay"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap()
    }

    fn state(entity: &crate::entity::Entity) -> AllayState {
        match entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .map(|mob| mob.species)
        {
            Some(Species::Allay(state)) => state,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_dancing_and_duplication_flags_sit_at_sixteen_and_seventeen() {
        let entity = allay(Compound::new());
        assert_eq!(
            entity.base.data.get(DATA_DANCING),
            Some(&SyncedValue::Boolean(false))
        );
        assert_eq!(
            entity.base.data.get(DATA_CAN_DUPLICATE),
            Some(&SyncedValue::Boolean(true))
        );
        assert!(!is_dancing(&entity.base));
        assert!(can_duplicate(&entity.base));
    }

    #[test]
    fn the_duplication_cooldown_round_trips_and_gates_the_flag() {
        let entity = allay(Compound::new().with("DuplicationCooldown", 3000i64));
        assert_eq!(state(&entity).duplication_cooldown, 3000);
        assert!(!can_duplicate(&entity.base));
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i64("DuplicationCooldown"), Some(3000));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
    }

    #[test]
    fn a_fresh_allay_can_duplicate_and_saves_a_zero_cooldown() {
        let entity = allay(Compound::new());
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i64("DuplicationCooldown"), Some(0));
        assert!(can_duplicate(&entity.base));
    }

    #[test]
    fn the_cooldown_counts_down_to_zero_then_allows_duplication_again() {
        let mut entity = allay(Compound::new());
        let crate::entity::EntityState::Living(living) = &mut entity.state else {
            panic!();
        };
        let Some(Species::Allay(state)) = living.mob.as_mut().map(|mob| &mut mob.species) else {
            panic!();
        };
        reset_duplication_cooldown(&mut entity.base, state);
        assert_eq!(state.duplication_cooldown, DUPLICATION_COOLDOWN);
        assert!(!can_duplicate(&entity.base));
        for _ in 0..DUPLICATION_COOLDOWN - 1 {
            tick_duplication_cooldown(&mut entity.base, state);
        }
        assert!(!can_duplicate(&entity.base));
        tick_duplication_cooldown(&mut entity.base, state);
        assert!(can_duplicate(&entity.base));
        tick_duplication_cooldown(&mut entity.base, state);
        assert_eq!(state.duplication_cooldown, 0);
    }

    #[test]
    fn the_inventory_and_the_listener_pass_through_as_extras() {
        let entity = allay(
            Compound::new()
                .with(INVENTORY_KEY, rspigot_nbt::Tag::List(Vec::new()))
                .with(LISTENER_KEY, Compound::new()),
        );
        assert!(entity.extra.contains_key(INVENTORY_KEY));
        assert!(entity.extra.contains_key(LISTENER_KEY));
        let saved = save_entity(&entity, None).unwrap();
        assert!(saved.contains_key(INVENTORY_KEY) && saved.contains_key(LISTENER_KEY));
    }
}
