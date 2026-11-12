use rspigot_nbt::Compound;

use crate::BiomeId;
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::LivingState;
use crate::entity::spawn::{SpawnEnvironment, mob_finalize};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const DATA_PUMPKIN: u8 = 16;
pub const PUMPKIN_MASK: i8 = 0x10;
pub const KEYS: &[&str] = &["Pumpkin"];
pub const MELT_DAMAGE: f32 = 1.0;
pub const SHOOT_UNCERTAINTY: f32 = 12.0;
pub const SHOOT_POWER: f32 = 1.6;
pub const TARGET_EYE_OFFSET: f64 = 1.100_000_023_841_858;
pub const TARGET_DISTANCE_SCALE: f64 = 0.200_000_002_980_232_24;
pub const SHOOT_PITCH_BASE: f32 = 0.4;
pub const SNOW_TRAIL_SPOTS: i32 = 4;
pub const SNOW_TRAIL_OFFSET: f32 = 0.25;
const MELTING_BIOMES: [&str; 7] = [
    "minecraft:badlands",
    "minecraft:desert",
    "minecraft:eroded_badlands",
    "minecraft:savanna",
    "minecraft:savanna_plateau",
    "minecraft:windswept_savanna",
    "minecraft:wooded_badlands",
];
const MELTING_DIMENSIONS: [&str; 1] = ["minecraft:the_nether"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SnowGolemState;

pub fn define(base: &mut EntityBase) -> SnowGolemState {
    base.data
        .define(DATA_PUMPKIN, SyncedValue::Byte(PUMPKIN_MASK));
    SnowGolemState
}

pub fn has_pumpkin(base: &EntityBase) -> bool {
    base.data.byte(DATA_PUMPKIN) & PUMPKIN_MASK != 0
}

pub fn set_pumpkin(base: &mut EntityBase, pumpkin: bool) {
    let flags = base.data.byte(DATA_PUMPKIN);
    let next = if pumpkin {
        flags | PUMPKIN_MASK
    } else {
        flags & !PUMPKIN_MASK
    };
    base.data.set(DATA_PUMPKIN, SyncedValue::Byte(next));
}

pub fn melts_in(biome: BiomeId, dimension_type: &str) -> bool {
    MELTING_DIMENSIONS.contains(&dimension_type)
        || MELTING_BIOMES
            .iter()
            .filter_map(|name| rspigot_data::registries::biome_id(name))
            .any(|id| id == biome.0)
}

pub fn read(nbt: &Compound, base: &mut EntityBase, _state: &mut SnowGolemState) {
    set_pumpkin(base, nbt.get_bool("Pumpkin").unwrap_or(true));
}

pub fn write(base: &EntityBase, _state: &SnowGolemState, out: &mut Compound) {
    out.insert("Pumpkin", i8::from(has_pumpkin(base)));
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

    fn golem_nbt() -> Compound {
        Compound::new().with("id", "minecraft:snow_golem")
    }

    fn entity_of(nbt: &Compound) -> crate::entity::Entity {
        load_entity(nbt, EntityId(1), Uuid::from_u128(1)).unwrap()
    }

    fn biome(name: &str) -> BiomeId {
        BiomeId(rspigot_data::registries::biome_id(name).unwrap())
    }

    #[test]
    fn the_pumpkin_flag_is_defined_at_index_sixteen_and_defaults_to_worn() {
        let entity = entity_of(&golem_nbt());
        assert_eq!(entity.base.data.byte(16), 0x10);
        assert!(has_pumpkin(&entity.base));
    }

    #[test]
    fn the_pumpkin_only_touches_bit_four_of_the_byte() {
        let mut entity = entity_of(&golem_nbt());
        set_pumpkin(&mut entity.base, false);
        assert_eq!(entity.base.data.byte(16), 0);
        entity.base.data.set(16, SyncedValue::Byte(0b0000_0101));
        set_pumpkin(&mut entity.base, true);
        assert_eq!(entity.base.data.byte(16), 0b0001_0101);
        set_pumpkin(&mut entity.base, false);
        assert_eq!(entity.base.data.byte(16), 0b0000_0101);
    }

    #[test]
    fn a_missing_pumpkin_key_reads_as_worn() {
        assert!(has_pumpkin(&entity_of(&golem_nbt()).base));
        let bare = entity_of(&golem_nbt().with("Pumpkin", false));
        assert!(!has_pumpkin(&bare.base));
    }

    #[test]
    fn the_pumpkin_round_trips_through_nbt() {
        let bare = entity_of(&golem_nbt().with("Pumpkin", false));
        let saved = save_entity(&bare, None).unwrap();
        assert_eq!(saved.get_bool("Pumpkin"), Some(false));
        let again = entity_of(&saved);
        assert_eq!(save_entity(&again, None), Some(saved));
    }

    #[test]
    fn the_golem_melts_in_the_hot_biomes_and_the_nether_only() {
        for name in MELTING_BIOMES {
            assert!(melts_in(biome(name), "minecraft:overworld"), "{name}");
        }
        for name in [
            "minecraft:plains",
            "minecraft:snowy_plains",
            "minecraft:jungle",
            "minecraft:swamp",
            "minecraft:ocean",
        ] {
            assert!(!melts_in(biome(name), "minecraft:overworld"), "{name}");
        }
        assert!(melts_in(biome("minecraft:plains"), "minecraft:the_nether"));
        assert!(!melts_in(biome("minecraft:plains"), "minecraft:the_end"));
    }
}
