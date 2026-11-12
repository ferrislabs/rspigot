use rspigot_data::registries::{static_entries, static_index};
use rspigot_nbt::{Compound, Tag};
use uuid::Uuid;

use crate::BiomeId;
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::save::{read_uuid, uuid_tag};
use crate::random::RandomSource;

pub const DATA_CONVERTING: u8 = 19;
pub const DATA_VILLAGER_DATA: u8 = 20;
pub const DATA_VILLAGER_DATA_FINALIZED: u8 = 21;
pub const NOT_CONVERTING: i32 = -1;
const TYPE_REGISTRY: &str = "minecraft:villager_type";
const PROFESSION_REGISTRY: &str = "minecraft:villager_profession";
const DEFAULT_TYPE: &str = "minecraft:plains";
const NO_PROFESSION: &str = "minecraft:none";
const MIN_LEVEL: i32 = 1;

pub const KEYS: &[&str] = &[
    "VillagerData",
    "VillagerDataFinalized",
    "ConversionTime",
    "ConversionPlayer",
    "Xp",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZombieVillagerState {
    pub conversion_time: i32,
    pub conversion_starter: Option<Uuid>,
    pub xp: i32,
}

impl Default for ZombieVillagerState {
    fn default() -> Self {
        Self {
            conversion_time: NOT_CONVERTING,
            conversion_starter: None,
            xp: 0,
        }
    }
}

impl ZombieVillagerState {
    pub fn is_converting(&self, base: &EntityBase) -> bool {
        base.data.boolean(DATA_CONVERTING)
    }

    pub fn keeps_when_far(&self, base: &EntityBase) -> bool {
        self.is_converting(base) || self.xp != 0
    }
}

fn index_of(registry: &str, name: &str) -> Option<u32> {
    static_index(registry, name)
}

fn name_of(registry: &str, id: u32) -> Option<&'static str> {
    static_entries(registry)?.get(id as usize).copied()
}

fn default_type() -> u32 {
    index_of(TYPE_REGISTRY, DEFAULT_TYPE).unwrap_or(0)
}

fn default_profession() -> u32 {
    index_of(PROFESSION_REGISTRY, NO_PROFESSION).unwrap_or(0)
}

fn profession_count() -> i32 {
    static_entries(PROFESSION_REGISTRY).map_or(1, |entries| entries.len() as i32)
}

pub fn type_for_biome(biome: BiomeId) -> u32 {
    let name = rspigot_data::registries::biome_name(biome.0).unwrap_or("");
    let villager = match name {
        "minecraft:badlands"
        | "minecraft:desert"
        | "minecraft:eroded_badlands"
        | "minecraft:wooded_badlands" => "minecraft:desert",
        "minecraft:bamboo_jungle" | "minecraft:jungle" | "minecraft:sparse_jungle" => {
            "minecraft:jungle"
        }
        "minecraft:savanna_plateau" | "minecraft:savanna" | "minecraft:windswept_savanna" => {
            "minecraft:savanna"
        }
        "minecraft:deep_frozen_ocean"
        | "minecraft:frozen_ocean"
        | "minecraft:frozen_river"
        | "minecraft:ice_spikes"
        | "minecraft:snowy_beach"
        | "minecraft:snowy_taiga"
        | "minecraft:snowy_plains"
        | "minecraft:grove"
        | "minecraft:snowy_slopes"
        | "minecraft:frozen_peaks"
        | "minecraft:jagged_peaks" => "minecraft:snow",
        "minecraft:swamp" | "minecraft:mangrove_swamp" => "minecraft:swamp",
        "minecraft:old_growth_spruce_taiga"
        | "minecraft:old_growth_pine_taiga"
        | "minecraft:windswept_gravelly_hills"
        | "minecraft:windswept_hills"
        | "minecraft:taiga"
        | "minecraft:windswept_forest" => "minecraft:taiga",
        _ => DEFAULT_TYPE,
    };
    index_of(TYPE_REGISTRY, villager).unwrap_or_else(default_type)
}

pub fn define(base: &mut EntityBase) -> ZombieVillagerState {
    base.data
        .define(DATA_CONVERTING, SyncedValue::Boolean(false));
    base.data.define(
        DATA_VILLAGER_DATA,
        SyncedValue::VillagerData {
            kind: u32::MAX,
            profession: u32::MAX,
            level: 0,
        },
    );
    set_villager_data(base, default_type(), default_profession(), MIN_LEVEL);
    base.data
        .define(DATA_VILLAGER_DATA_FINALIZED, SyncedValue::Boolean(false));
    ZombieVillagerState::default()
}

pub fn villager_data(base: &EntityBase) -> (u32, u32, i32) {
    base.data.villager_data(DATA_VILLAGER_DATA).unwrap_or((
        default_type(),
        default_profession(),
        MIN_LEVEL,
    ))
}

pub fn set_villager_data(base: &mut EntityBase, kind: u32, profession: u32, level: i32) {
    base.data.set(
        DATA_VILLAGER_DATA,
        SyncedValue::VillagerData {
            kind,
            profession,
            level: level.max(MIN_LEVEL),
        },
    );
}

pub fn is_finalized(base: &EntityBase) -> bool {
    base.data.boolean(DATA_VILLAGER_DATA_FINALIZED)
}

pub fn set_finalized(base: &mut EntityBase, finalized: bool) {
    base.data.set(
        DATA_VILLAGER_DATA_FINALIZED,
        SyncedValue::Boolean(finalized),
    );
}

pub fn finalize_type(base: &mut EntityBase, biome: BiomeId) {
    if is_finalized(base) {
        return;
    }
    let (_, profession, level) = villager_data(base);
    set_villager_data(base, type_for_biome(biome), profession, level);
    set_finalized(base, true);
}

pub fn randomize_profession(base: &mut EntityBase, random: &mut impl RandomSource, biome: BiomeId) {
    let profession = random.next_int(profession_count()) as u32;
    if !is_finalized(base) {
        let (kind, _, level) = villager_data(base);
        set_villager_data(base, kind, profession, level);
    }
    finalize_type(base, biome);
}

pub(crate) fn parse_data(compound: &Compound) -> Option<(u32, u32, i32)> {
    let kind = match compound.get_str("type") {
        Some(name) => index_of(TYPE_REGISTRY, name)?,
        None => default_type(),
    };
    let profession = match compound.get_str("profession") {
        Some(name) => index_of(PROFESSION_REGISTRY, name)?,
        None => default_profession(),
    };
    let level = compound
        .get_i32("level")
        .unwrap_or(MIN_LEVEL)
        .max(MIN_LEVEL);
    Some((kind, profession, level))
}

pub fn read(nbt: &Compound, base: &mut EntityBase, state: &mut ZombieVillagerState) {
    let parsed = nbt.get_compound("VillagerData").and_then(parse_data);
    if nbt.get_bool("VillagerDataFinalized").unwrap_or(false) || parsed.is_some() {
        set_finalized(base, true);
        let (kind, profession, level) =
            parsed.unwrap_or((default_type(), default_profession(), MIN_LEVEL));
        set_villager_data(base, kind, profession, level);
    }
    state.xp = nbt.get_i32("Xp").unwrap_or(0);
    match nbt.get_i32("ConversionTime").unwrap_or(NOT_CONVERTING) {
        NOT_CONVERTING => {
            base.data.set(DATA_CONVERTING, SyncedValue::Boolean(false));
            state.conversion_time = NOT_CONVERTING;
            state.conversion_starter = None;
        }
        time => {
            state.conversion_starter = nbt.get("ConversionPlayer").and_then(read_uuid);
            state.conversion_time = time;
            base.data.set(DATA_CONVERTING, SyncedValue::Boolean(true));
        }
    }
}

pub(crate) fn data_compound(kind: u32, profession: u32, level: i32) -> Compound {
    Compound::new()
        .with("type", name_of(TYPE_REGISTRY, kind).unwrap_or(DEFAULT_TYPE))
        .with(
            "profession",
            name_of(PROFESSION_REGISTRY, profession).unwrap_or(NO_PROFESSION),
        )
        .with("level", level)
}

pub fn write(base: &EntityBase, state: &ZombieVillagerState, out: &mut Compound) {
    let (kind, profession, level) = villager_data(base);
    out.insert(
        "VillagerData",
        Tag::Compound(data_compound(kind, profession, level)),
    );
    out.insert("VillagerDataFinalized", is_finalized(base));
    out.insert(
        "ConversionTime",
        if state.is_converting(base) {
            state.conversion_time
        } else {
            NOT_CONVERTING
        },
    );
    if let Some(starter) = state.conversion_starter {
        out.insert("ConversionPlayer", uuid_tag(starter));
    }
    out.insert("Xp", state.xp);
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::EntityId;
    use crate::random::LegacyRandom;

    fn villager() -> (EntityBase, ZombieVillagerState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::ZombieVillager);
        let state = define(&mut base);
        (base, state)
    }

    fn biome(name: &str) -> BiomeId {
        BiomeId(rspigot_data::registries::biome_id(name).unwrap())
    }

    #[test]
    fn the_three_data_slots_follow_the_zombie_ones() {
        let (base, _) = villager();
        assert_eq!(base.data.get(19), Some(&SyncedValue::Boolean(false)));
        assert_eq!(
            base.data.get(20),
            Some(&SyncedValue::VillagerData {
                kind: 2,
                profession: 0,
                level: 1
            })
        );
        assert_eq!(base.data.get(21), Some(&SyncedValue::Boolean(false)));
    }

    #[test]
    fn the_villager_data_is_always_part_of_the_pairing_data() {
        let (base, _) = villager();
        let paired = base.data.non_default();
        assert!(
            paired
                .iter()
                .any(|(index, value)| *index == DATA_VILLAGER_DATA
                    && *value
                        == SyncedValue::VillagerData {
                            kind: 2,
                            profession: 0,
                            level: 1
                        })
        );
    }

    #[test]
    fn biomes_pick_their_villager_type_once() {
        let (mut base, _) = villager();
        finalize_type(&mut base, biome("minecraft:desert"));
        assert_eq!(villager_data(&base).0, 0);
        assert!(is_finalized(&base));
        finalize_type(&mut base, biome("minecraft:snowy_plains"));
        assert_eq!(villager_data(&base).0, 0);
        let (mut taiga, _) = villager();
        finalize_type(&mut taiga, biome("minecraft:old_growth_pine_taiga"));
        assert_eq!(villager_data(&taiga).0, 6);
        let (mut meadow, _) = villager();
        finalize_type(&mut meadow, biome("minecraft:meadow"));
        assert_eq!(villager_data(&meadow).0, 2);
    }

    #[test]
    fn a_fresh_zombie_villager_gets_a_random_profession_unless_data_was_loaded() {
        let (mut fresh, _) = villager();
        let mut random = LegacyRandom::new(9);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..200 {
            let (mut base, _) = villager();
            randomize_profession(&mut base, &mut random, biome("minecraft:plains"));
            seen.insert(villager_data(&base).1);
        }
        assert!(seen.len() > 8);
        assert!(seen.iter().all(|profession| *profession < 15));
        randomize_profession(&mut fresh, &mut random, biome("minecraft:plains"));
        let chosen = villager_data(&fresh).1;
        randomize_profession(&mut fresh, &mut random, biome("minecraft:plains"));
        assert_eq!(villager_data(&fresh).1, chosen);
    }

    #[test]
    fn villager_data_xp_and_conversion_survive_a_round_trip() {
        let starter = Uuid::from_u128(77);
        let nbt = Compound::new()
            .with(
                "VillagerData",
                Compound::new()
                    .with("type", "minecraft:swamp")
                    .with("profession", "minecraft:librarian")
                    .with("level", 3),
            )
            .with("Xp", 120)
            .with("ConversionTime", 2400)
            .with("ConversionPlayer", uuid_tag(starter));
        let (mut base, mut state) = villager();
        read(&nbt, &mut base, &mut state);
        assert_eq!(villager_data(&base), (5, 9, 3));
        assert!(is_finalized(&base));
        assert!(state.is_converting(&base));
        assert_eq!(state.xp, 120);
        let mut out = Compound::new();
        write(&base, &state, &mut out);
        assert_eq!(out.get_i32("Xp"), Some(120));
        assert_eq!(out.get_i32("ConversionTime"), Some(2400));
        assert_eq!(out.get("ConversionPlayer"), Some(&uuid_tag(starter)));
        assert_eq!(out.get_bool("VillagerDataFinalized"), Some(true));
        let data = out.get_compound("VillagerData").unwrap();
        assert_eq!(data.get_str("type"), Some("minecraft:swamp"));
        assert_eq!(data.get_str("profession"), Some("minecraft:librarian"));
        assert_eq!(data.get_i32("level"), Some(3));
    }

    #[test]
    fn without_a_conversion_the_flag_is_off_and_the_time_is_minus_one() {
        let (mut base, mut state) = villager();
        read(&Compound::new(), &mut base, &mut state);
        assert!(!state.is_converting(&base));
        assert!(!is_finalized(&base));
        let mut out = Compound::new();
        write(&base, &state, &mut out);
        assert_eq!(out.get_i32("ConversionTime"), Some(-1));
        assert_eq!(out.get_i32("Xp"), Some(0));
        assert!(!state.keeps_when_far(&base));
    }

    #[test]
    fn experienced_or_converting_villagers_never_despawn_far_away() {
        let (mut base, mut state) = villager();
        state.xp = 1;
        assert!(state.keeps_when_far(&base));
        state.xp = 0;
        base.data.set(DATA_CONVERTING, SyncedValue::Boolean(true));
        assert!(state.keeps_when_far(&base));
    }

    #[test]
    fn spawning_gives_the_biome_type_and_a_profession() {
        use crate::difficulty::DifficultyInstance;
        use crate::entity::living::LivingState;
        use crate::entity::spawn::{SpawnEnvironment, finalize_spawn};
        use crate::entity::{Entity, EntityState};
        use crate::spawn::SpawnReason;
        use crate::world::Difficulty;

        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::ZombieVillager);
        let living = LivingState::new(&mut base, 0.0);
        let mut entity = Entity::new(base, EntityState::Living(Box::new(living)));
        let environment = SpawnEnvironment {
            biome: biome("minecraft:jungle"),
            difficulty: DifficultyInstance::new(Difficulty::Normal, 0, 0, 1.0),
            halloween: false,
            moon_brightness: 0.0,
        };
        let mut level = LegacyRandom::new(4);
        let mut own = LegacyRandom::new(5);
        finalize_spawn(
            &mut entity,
            &environment,
            SpawnReason::Natural,
            None,
            &mut level,
            &mut own,
        );
        let (kind, profession, level_value) = villager_data(&entity.base);
        assert_eq!(kind, 1);
        assert!(profession < 15);
        assert_eq!(level_value, 1);
        assert!(is_finalized(&entity.base));
    }

    #[test]
    fn data_the_save_does_not_model_is_carried_through_untouched() {
        use crate::entity::save::{load_entity, save_entity};

        let offers = Compound::new().with(
            "Recipes",
            Tag::List(vec![Tag::Compound(Compound::new().with("uses", 3))]),
        );
        let gossips = Tag::List(vec![Tag::Compound(
            Compound::new()
                .with("Type", "major_positive")
                .with("Value", 5),
        )]);
        let nbt = Compound::new()
            .with("id", "minecraft:zombie_villager")
            .with(
                "Pos",
                Tag::List(vec![0.5f64.into(), 64.0f64.into(), 0.5f64.into()]),
            )
            .with("UUID", uuid_tag(Uuid::from_u128(7)))
            .with("Offers", offers.clone())
            .with("Gossips", gossips.clone())
            .with("Xp", 15)
            .with("IsBaby", true);
        let entity = load_entity(&nbt, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert!(entity.is_baby());
        assert!(entity.extra.contains_key("Offers"));
        assert!(entity.extra.contains_key("Gossips"));
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_compound("Offers"), Some(&offers));
        assert_eq!(saved.get("Gossips"), Some(&gossips));
        assert_eq!(saved.get_i32("Xp"), Some(15));
        let again = load_entity(&saved, EntityId(2), Uuid::from_u128(2)).unwrap();
        let resaved = save_entity(&again, None).unwrap();
        assert_eq!(resaved.get_compound("Offers"), Some(&offers));
        assert_eq!(resaved.get("Gossips"), Some(&gossips));
        assert_eq!(resaved.get_bool("VillagerDataFinalized"), Some(true));
    }
}
