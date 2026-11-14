use rspigot_nbt::Compound;

use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::LivingState;
use crate::entity::spawn::{SpawnEnvironment, mob_finalize};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const DATA_IS_CHARGING: u8 = 16;
pub const KEYS: &[&str] = &["ExplosionPower"];
pub const DEFAULT_EXPLOSION_POWER: i32 = 1;
pub const CHARGE_WARN_TICKS: i32 = 10;
pub const CHARGE_FIRE_TICKS: i32 = 20;
pub const COOLDOWN_TICKS: i32 = -40;
pub const SHOOT_RANGE_SQR: f64 = 4096.0;
pub const FIREBALL_REACH: f64 = 4.0;
pub const WARN_EVENT: i32 = 1015;
pub const SHOOT_EVENT: i32 = 1016;
pub const LEASH_ELASTIC: f64 = 10.0;
pub const LEASH_SNAP: f64 = 16.0;
pub const REFLECTED_DAMAGE: f32 = 1000.0;
pub const TARGET_HEIGHT_RANGE: f64 = 4.0;
pub const SPAWN_ODDS: i32 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GhastState {
    pub explosion_power: i32,
}

impl Default for GhastState {
    fn default() -> Self {
        Self {
            explosion_power: DEFAULT_EXPLOSION_POWER,
        }
    }
}

pub fn define(base: &mut EntityBase) -> GhastState {
    base.data
        .define(DATA_IS_CHARGING, SyncedValue::Boolean(false));
    GhastState::default()
}

pub fn is_charging(base: &EntityBase) -> bool {
    base.data.boolean(DATA_IS_CHARGING)
}

pub fn set_charging(base: &mut EntityBase, charging: bool) {
    base.data
        .set(DATA_IS_CHARGING, SyncedValue::Boolean(charging));
}

pub fn read(nbt: &Compound, _base: &mut EntityBase, state: &mut GhastState) {
    state.explosion_power = nbt
        .get_i64("ExplosionPower")
        .map_or(DEFAULT_EXPLOSION_POWER, |power| {
            i64::from(power as i8) as i32
        });
}

pub fn write(_base: &EntityBase, state: &GhastState, out: &mut Compound) {
    out.insert("ExplosionPower", state.explosion_power as i8);
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

    fn ghast(nbt: Compound) -> crate::entity::Entity {
        load_entity(
            &nbt.with("id", "minecraft:ghast"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap()
    }

    #[test]
    fn the_charging_flag_sits_at_index_sixteen_and_starts_false() {
        let mut entity = ghast(Compound::new());
        assert_eq!(
            entity.base.data.get(DATA_IS_CHARGING),
            Some(&SyncedValue::Boolean(false))
        );
        set_charging(&mut entity.base, true);
        assert!(is_charging(&entity.base));
    }

    #[test]
    fn the_explosion_power_defaults_to_one_and_round_trips_as_a_byte() {
        let entity = ghast(Compound::new());
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i64("ExplosionPower"), Some(1));
        let strong = ghast(Compound::new().with("ExplosionPower", 5i8));
        let saved = save_entity(&strong, None).unwrap();
        assert_eq!(saved.get_i64("ExplosionPower"), Some(5));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
    }
}
