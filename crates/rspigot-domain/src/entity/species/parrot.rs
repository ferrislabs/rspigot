use rspigot_nbt::Compound;

use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::LivingState;
use crate::entity::spawn::{SpawnEnvironment, mob_finalize};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const DATA_VARIANT: u8 = 20;
pub const KEYS: &[&str] = &["Variant"];
pub const VARIANTS: [&str; 5] = ["red_blue", "blue", "green", "yellow_blue", "gray"];
pub const DEFAULT_VARIANT: i32 = 0;
pub const POISON_TICKS: i32 = 900;
pub const TAME_ODDS: i32 = 10;
pub const IMITATE_ODDS: i32 = 400;
pub const IMITATE_VOLUME: f32 = 0.7;
pub const IMITATE_AREA: f64 = 20.0;
pub const EAT_SOUND: &str = "minecraft:entity.parrot.eat";
pub const AMBIENT_IMITATE_ODDS: i32 = 1000;
pub const MAX_HEALTH: f32 = 6.0;
pub const GLIDE_FALL: f64 = 0.6;

pub fn define(base: &mut EntityBase) {
    base.data
        .define(DATA_VARIANT, SyncedValue::Int(DEFAULT_VARIANT));
}

pub fn variant(base: &EntityBase) -> i32 {
    match base.data.get(DATA_VARIANT) {
        Some(SyncedValue::Int(id)) => *id,
        _ => DEFAULT_VARIANT,
    }
}

pub fn set_variant(base: &mut EntityBase, id: i32) {
    base.data.set(DATA_VARIANT, SyncedValue::Int(id));
}

pub fn variant_name(base: &EntityBase) -> &'static str {
    usize::try_from(variant(base))
        .ok()
        .and_then(|index| VARIANTS.get(index))
        .copied()
        .unwrap_or(VARIANTS[0])
}

pub fn read(nbt: &Compound, base: &mut EntityBase) {
    let id = nbt.get_i32("Variant").unwrap_or(DEFAULT_VARIANT);
    let id = if (0..VARIANTS.len() as i32).contains(&id) {
        id
    } else {
        DEFAULT_VARIANT
    };
    set_variant(base, id);
}

pub fn write(base: &EntityBase, out: &mut Compound) {
    out.insert("Variant", variant(base));
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    _environment: &SpawnEnvironment,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    let id = level.next_int(VARIANTS.len() as i32);
    set_variant(base, id);
    let group = group.or(Some(SpawnGroupData::Ageable {
        group_size: 0,
        should_spawn_baby: false,
        baby_chance: 0.05,
    }));
    mob_finalize(base, living, group, level)
}

#[cfg(test)]
mod tests {
    use rspigot_nbt::Compound;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::save::{load_entity, save_entity};

    fn parrot(nbt: Compound) -> crate::entity::Entity {
        load_entity(
            &nbt.with("id", "minecraft:parrot"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap()
    }

    #[test]
    fn the_variant_sits_at_index_twenty_and_defaults_to_red_blue() {
        let entity = parrot(Compound::new());
        assert_eq!(
            entity.base.data.get(DATA_VARIANT),
            Some(&SyncedValue::Int(0))
        );
        assert_eq!(variant_name(&entity.base), "red_blue");
    }

    #[test]
    fn the_variant_round_trips_as_an_int_and_an_unknown_one_falls_back() {
        let entity = parrot(Compound::new().with("Variant", 3));
        assert_eq!(variant_name(&entity.base), "yellow_blue");
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i32("Variant"), Some(3));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
        let odd = parrot(Compound::new().with("Variant", 9));
        assert_eq!(variant(&odd.base), 0);
    }

    #[test]
    fn a_spawned_parrot_rolls_one_of_the_five_variants_and_is_never_a_baby() {
        use crate::entity::spawn::{SpawnEnvironment, finalize_spawn};
        let mut seen = std::collections::BTreeSet::new();
        for seed in 0..40 {
            let mut entity = parrot(Compound::new());
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
            let mut level = crate::random::LegacyRandom::new(seed);
            let mut own = crate::random::LegacyRandom::new(seed + 100);
            finalize_spawn(
                &mut entity,
                &environment,
                crate::spawn::SpawnReason::Natural,
                None,
                &mut level,
                &mut own,
            );
            assert!(!entity.is_baby());
            seen.insert(variant(&entity.base));
        }
        assert_eq!(seen.len(), 5);
    }
}
