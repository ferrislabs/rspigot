use rspigot_entities::{EntityDimensions, EntityKind};
use rspigot_nbt::Compound;

use crate::EntityId;
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::LivingState;
use crate::entity::spawn::{SpawnEnvironment, mob_finalize};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const DATA_FROM_BUCKET: u8 = 16;
pub const DATA_VARIANT: u8 = 17;
pub const DATA_PUFF_STATE: u8 = DATA_VARIANT;
pub const MAX_CLUSTER_SIZE: i32 = 8;
pub const SALMON_SCHOOL_SIZE: i32 = 5;
pub const PUFF_STATES: i32 = 2;
pub const INFLATE_FULL_AFTER: i32 = 40;
pub const DEFLATE_HALF_AFTER: i32 = 60;
pub const DEFLATE_FULL_AFTER: i32 = 100;
pub const STING_POISON_TICKS: i32 = 60;
pub const SCHOOL_REGROUP_ODDS: i32 = 200;
pub const SCHOOL_SEARCH_RANGE: f64 = 8.0;
pub const SCHOOL_LEADER_RANGE_SQR: f64 = 121.0;
pub const LONER_ODDS: f32 = 0.9;
pub const PUFFER_CONTACT_INFLATE: f64 = 0.3;
pub const PUFFER_ALARM_INFLATE: f64 = 2.0;
pub const FISH_SWIM_SPEED_SCALE: f64 = 0.9;
pub const FISH_SINK_SPEED: f64 = -0.005;
pub const FISH_MOVE_RELATIVE_SPEED: f32 = 0.01;
pub const FLOP_HORIZONTAL_SPREAD: f32 = 0.05;
pub const FLOP_VERTICAL_PUSH: f64 = 0.4;
pub const PUFFER_STING_EVENT: &str = "minecraft:puffer_fish_sting";
pub const SALMON_WEIGHTS: [(Variant, i32); 3] = [
    (Variant::Small, 30),
    (Variant::Medium, 50),
    (Variant::Large, 15),
];
const SALMON_TOTAL_WEIGHT: i32 = 95;
const DYE_COUNT: usize = 16;
const PATTERN_MASK: i32 = 0xFFFF;
const COLOR_MASK: i32 = 0xFF;

const SALMON_KEYS: &[&str] = &["FromBucket", "type"];
const PUFFERFISH_KEYS: &[&str] = &["FromBucket", "PuffState"];
const TROPICAL_KEYS: &[&str] = &["FromBucket", "Variant"];
const PLAIN_KEYS: &[&str] = &["FromBucket"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FishState {
    pub leader: Option<EntityId>,
    pub school_size: i32,
    pub is_school: bool,
    pub inflate_counter: i32,
    pub deflate_timer: i32,
}

impl Default for FishState {
    fn default() -> Self {
        Self {
            leader: None,
            school_size: 1,
            is_school: true,
            inflate_counter: 0,
            deflate_timer: 0,
        }
    }
}

pub fn is_fish(kind: EntityKind) -> bool {
    kind.extends("AbstractFish")
}

pub fn is_schooling(kind: EntityKind) -> bool {
    kind.extends("AbstractSchoolingFish")
}

pub fn max_school_size(kind: EntityKind) -> i32 {
    match kind {
        EntityKind::Salmon => SALMON_SCHOOL_SIZE,
        _ => MAX_CLUSTER_SIZE,
    }
}

pub fn keys(kind: EntityKind) -> &'static [&'static str] {
    match kind {
        EntityKind::Salmon => SALMON_KEYS,
        EntityKind::Pufferfish => PUFFERFISH_KEYS,
        EntityKind::TropicalFish => TROPICAL_KEYS,
        _ => PLAIN_KEYS,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Small,
    Medium,
    Large,
}

impl Variant {
    pub const DEFAULT: Self = Self::Medium;

    pub const fn id(self) -> i32 {
        match self {
            Self::Small => 0,
            Self::Medium => 1,
            Self::Large => 2,
        }
    }

    pub fn by_id(id: i32) -> Self {
        match id.clamp(0, 2) {
            0 => Self::Small,
            1 => Self::Medium,
            _ => Self::Large,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Small, Self::Medium, Self::Large]
            .into_iter()
            .find(|variant| variant.name() == name)
    }

    pub const fn scale(self) -> f32 {
        match self {
            Self::Small => 0.5,
            Self::Medium => 1.0,
            Self::Large => 1.5,
        }
    }
}

pub const fn puff_scale(state: i32) -> f32 {
    match state {
        0 => 0.5,
        1 => 0.7,
        _ => 1.0,
    }
}

pub fn define(base: &mut EntityBase) -> FishState {
    base.data
        .define(DATA_FROM_BUCKET, SyncedValue::Boolean(false));
    match base.kind {
        EntityKind::Salmon => base
            .data
            .define(DATA_VARIANT, SyncedValue::Int(Variant::DEFAULT.id())),
        EntityKind::Pufferfish => base.data.define(DATA_PUFF_STATE, SyncedValue::Int(0)),
        EntityKind::TropicalFish => base
            .data
            .define(DATA_VARIANT, SyncedValue::Int(DEFAULT_TROPICAL_VARIANT)),
        _ => {}
    }
    FishState::default()
}

pub fn from_bucket(base: &EntityBase) -> bool {
    base.data.boolean(DATA_FROM_BUCKET)
}

pub fn set_from_bucket(base: &mut EntityBase, from_bucket: bool) {
    base.data
        .set(DATA_FROM_BUCKET, SyncedValue::Boolean(from_bucket));
}

pub fn salmon_variant(base: &EntityBase) -> Variant {
    Variant::by_id(base.data.int(DATA_VARIANT))
}

pub fn set_salmon_variant(base: &mut EntityBase, variant: Variant) {
    base.data.set(DATA_VARIANT, SyncedValue::Int(variant.id()));
    crate::entity::ageable::refresh_dimensions(base, false);
}

pub fn puff_state(base: &EntityBase) -> i32 {
    base.data.int(DATA_PUFF_STATE)
}

pub fn set_puff_state(base: &mut EntityBase, state: i32) {
    base.data.set(DATA_PUFF_STATE, SyncedValue::Int(state));
    crate::entity::ageable::refresh_dimensions(base, false);
}

pub fn dimension_scale(base: &EntityBase) -> Option<f32> {
    match base.kind {
        EntityKind::Salmon => Some(salmon_variant(base).scale()),
        EntityKind::Pufferfish => Some(puff_scale(puff_state(base))),
        _ => None,
    }
}

pub fn scaled_dimensions(adult: EntityDimensions, scale: f32) -> EntityDimensions {
    EntityDimensions {
        width: adult.width * scale,
        height: adult.height * scale,
        eye_height: adult.eye_height * scale,
    }
}

pub const DYE_NAMES: [&str; DYE_COUNT] = [
    "white",
    "orange",
    "magenta",
    "light_blue",
    "yellow",
    "lime",
    "pink",
    "gray",
    "light_gray",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pattern {
    Kob,
    Sunstreak,
    Snooper,
    Dasher,
    Brinely,
    Spotty,
    Flopper,
    Stripey,
    Glitter,
    Blockfish,
    Betty,
    Clayfish,
}

impl Pattern {
    pub const ALL: [Self; 12] = [
        Self::Kob,
        Self::Sunstreak,
        Self::Snooper,
        Self::Dasher,
        Self::Brinely,
        Self::Spotty,
        Self::Flopper,
        Self::Stripey,
        Self::Glitter,
        Self::Blockfish,
        Self::Betty,
        Self::Clayfish,
    ];

    pub const fn is_large(self) -> bool {
        matches!(
            self,
            Self::Flopper
                | Self::Stripey
                | Self::Glitter
                | Self::Blockfish
                | Self::Betty
                | Self::Clayfish
        )
    }

    pub const fn index(self) -> i32 {
        match self {
            Self::Kob | Self::Flopper => 0,
            Self::Sunstreak | Self::Stripey => 1,
            Self::Snooper | Self::Glitter => 2,
            Self::Dasher | Self::Blockfish => 3,
            Self::Brinely | Self::Betty => 4,
            Self::Spotty | Self::Clayfish => 5,
        }
    }

    pub const fn packed_id(self) -> i32 {
        (self.is_large() as i32) | (self.index() << 8)
    }

    pub fn by_id(packed: i32) -> Self {
        Self::ALL
            .into_iter()
            .find(|pattern| pattern.packed_id() == packed)
            .unwrap_or(Self::Kob)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Kob => "kob",
            Self::Sunstreak => "sunstreak",
            Self::Snooper => "snooper",
            Self::Dasher => "dasher",
            Self::Brinely => "brinely",
            Self::Spotty => "spotty",
            Self::Flopper => "flopper",
            Self::Stripey => "stripey",
            Self::Glitter => "glitter",
            Self::Blockfish => "blockfish",
            Self::Betty => "betty",
            Self::Clayfish => "clayfish",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TropicalVariant {
    pub pattern: Pattern,
    pub base_color: u8,
    pub pattern_color: u8,
}

pub const DEFAULT_TROPICAL_VARIANT: i32 = 0;

const fn variant(pattern: Pattern, base_color: u8, pattern_color: u8) -> TropicalVariant {
    TropicalVariant {
        pattern,
        base_color,
        pattern_color,
    }
}

const WHITE: u8 = 0;
const ORANGE: u8 = 1;
const LIGHT_BLUE: u8 = 3;
const YELLOW: u8 = 4;
const LIME: u8 = 5;
const PINK: u8 = 6;
const GRAY: u8 = 7;
const CYAN: u8 = 9;
const PURPLE: u8 = 10;
const BLUE: u8 = 11;
const RED: u8 = 14;

pub const COMMON_VARIANTS: [TropicalVariant; 22] = [
    variant(Pattern::Stripey, ORANGE, GRAY),
    variant(Pattern::Flopper, GRAY, GRAY),
    variant(Pattern::Flopper, GRAY, BLUE),
    variant(Pattern::Clayfish, WHITE, GRAY),
    variant(Pattern::Sunstreak, BLUE, GRAY),
    variant(Pattern::Kob, ORANGE, WHITE),
    variant(Pattern::Spotty, PINK, LIGHT_BLUE),
    variant(Pattern::Blockfish, PURPLE, YELLOW),
    variant(Pattern::Clayfish, WHITE, RED),
    variant(Pattern::Spotty, WHITE, YELLOW),
    variant(Pattern::Glitter, WHITE, GRAY),
    variant(Pattern::Clayfish, WHITE, ORANGE),
    variant(Pattern::Dasher, CYAN, PINK),
    variant(Pattern::Brinely, LIME, LIGHT_BLUE),
    variant(Pattern::Betty, RED, WHITE),
    variant(Pattern::Snooper, GRAY, RED),
    variant(Pattern::Blockfish, RED, WHITE),
    variant(Pattern::Flopper, WHITE, YELLOW),
    variant(Pattern::Kob, RED, WHITE),
    variant(Pattern::Sunstreak, GRAY, WHITE),
    variant(Pattern::Dasher, CYAN, YELLOW),
    variant(Pattern::Flopper, YELLOW, YELLOW),
];

impl TropicalVariant {
    pub fn pack(self) -> i32 {
        (self.pattern.packed_id() & PATTERN_MASK)
            | ((i32::from(self.base_color) & COLOR_MASK) << 16)
            | (i32::from(self.pattern_color) << 24)
    }

    pub fn unpack(packed: i32) -> Self {
        Self {
            pattern: Pattern::by_id(packed & PATTERN_MASK),
            base_color: dye_by_id((packed >> 16) & COLOR_MASK),
            pattern_color: dye_by_id((packed >> 24) & COLOR_MASK),
        }
    }
}

fn dye_by_id(id: i32) -> u8 {
    if (0..DYE_COUNT as i32).contains(&id) {
        id as u8
    } else {
        0
    }
}

pub fn tropical_variant(base: &EntityBase) -> TropicalVariant {
    TropicalVariant::unpack(base.data.int(DATA_VARIANT))
}

pub fn set_tropical_variant(base: &mut EntityBase, variant: TropicalVariant) {
    base.data
        .set(DATA_VARIANT, SyncedValue::Int(variant.pack()));
}

pub fn random_tropical_variant(random: &mut impl RandomSource) -> TropicalVariant {
    let pattern = Pattern::ALL[random.next_int(Pattern::ALL.len() as i32) as usize];
    let base_color = random.next_int(DYE_COUNT as i32) as u8;
    let pattern_color = random.next_int(DYE_COUNT as i32) as u8;
    TropicalVariant {
        pattern,
        base_color,
        pattern_color,
    }
}

pub fn common_tropical_variant(random: &mut impl RandomSource) -> TropicalVariant {
    COMMON_VARIANTS[random.next_int(COMMON_VARIANTS.len() as i32) as usize]
}

pub fn read(nbt: &Compound, base: &mut EntityBase, _state: &mut FishState) {
    set_from_bucket(base, nbt.get_bool("FromBucket").unwrap_or(false));
    match base.kind {
        EntityKind::Salmon => {
            let variant = nbt
                .get_str("type")
                .and_then(Variant::from_name)
                .unwrap_or(Variant::DEFAULT);
            set_salmon_variant(base, variant);
        }
        EntityKind::Pufferfish => {
            let state = nbt.get_i32("PuffState").unwrap_or(0).min(PUFF_STATES);
            set_puff_state(base, state);
        }
        EntityKind::TropicalFish => {
            let packed = nbt.get_i32("Variant").unwrap_or(DEFAULT_TROPICAL_VARIANT);
            set_tropical_variant(base, TropicalVariant::unpack(packed));
        }
        _ => {}
    }
}

pub fn write(base: &EntityBase, _state: &FishState, out: &mut Compound) {
    out.insert("FromBucket", from_bucket(base));
    match base.kind {
        EntityKind::Salmon => {
            out.insert("type", salmon_variant(base).name());
        }
        EntityKind::Pufferfish => {
            out.insert("PuffState", puff_state(base));
        }
        EntityKind::TropicalFish => {
            out.insert("Variant", tropical_variant(base).pack());
        }
        _ => {}
    }
}

fn school_finalize(
    base: &EntityBase,
    state: &mut FishState,
    group: Option<SpawnGroupData>,
) -> SpawnGroupData {
    match group {
        Some(SpawnGroupData::School { leader } | SpawnGroupData::TropicalSchool { leader, .. }) => {
            state.leader = Some(leader);
            group.unwrap_or(SpawnGroupData::School { leader })
        }
        _ => SpawnGroupData::School { leader: base.id },
    }
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    _environment: &SpawnEnvironment,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
    own: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    let kind = base.kind;
    let mut group = mob_finalize(base, living, group, level);
    if kind == EntityKind::Salmon {
        let mut roll = own.next_int(SALMON_TOTAL_WEIGHT);
        let mut picked = Variant::DEFAULT;
        for (variant, weight) in SALMON_WEIGHTS {
            if roll < weight {
                picked = variant;
                break;
            }
            roll -= weight;
        }
        set_salmon_variant(base, picked);
    }
    if !is_schooling(kind) {
        return group;
    }
    let Some(crate::entity::living::MobState {
        species: super::Species::Fish(state),
        ..
    }) = living.mob.as_mut()
    else {
        return group;
    };
    group = Some(school_finalize(base, state, group));
    if kind != EntityKind::TropicalFish {
        return group;
    }
    let variant = match group {
        Some(SpawnGroupData::TropicalSchool { variant, .. }) => TropicalVariant::unpack(variant),
        _ => {
            if level.next_float() >= LONER_ODDS {
                state.is_school = false;
                random_tropical_variant(level)
            } else {
                let common = common_tropical_variant(level);
                group = Some(SpawnGroupData::TropicalSchool {
                    leader: state.leader.unwrap_or(base.id),
                    variant: common.pack(),
                });
                common
            }
        }
    };
    set_tropical_variant(base, variant);
    group
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::entity::save::{load_entity, save_entity};
    use crate::random::LegacyRandom;

    fn environment() -> SpawnEnvironment {
        SpawnEnvironment {
            biome: crate::BiomeId(
                rspigot_data::registries::biome_id("minecraft:warm_ocean").unwrap(),
            ),
            difficulty: crate::difficulty::DifficultyInstance::new(
                crate::world::Difficulty::Normal,
                0,
                0,
                1.0,
            ),
            halloween: false,
            moon_brightness: 0.0,
        }
    }

    fn fish(kind: &str, nbt: Compound) -> crate::entity::Entity {
        load_entity(
            &nbt.with("id", format!("minecraft:{kind}").as_str()),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap()
    }

    #[test]
    fn the_synced_slots_follow_the_vanilla_data_ids() {
        let cod = fish("cod", Compound::new());
        assert_eq!(
            cod.base.data.get(DATA_FROM_BUCKET),
            Some(&SyncedValue::Boolean(false))
        );
        let salmon = fish("salmon", Compound::new());
        assert_eq!(
            salmon.base.data.get(DATA_VARIANT),
            Some(&SyncedValue::Int(1))
        );
        let puffer = fish("pufferfish", Compound::new());
        assert_eq!(
            puffer.base.data.get(DATA_PUFF_STATE),
            Some(&SyncedValue::Int(0))
        );
        let tropical = fish("tropical_fish", Compound::new());
        assert_eq!(
            tropical.base.data.get(DATA_VARIANT),
            Some(&SyncedValue::Int(0))
        );
    }

    #[test]
    fn a_cod_keeps_its_bucket_flag() {
        let cod = fish("cod", Compound::new().with("FromBucket", true));
        assert!(from_bucket(&cod.base));
        let saved = save_entity(&cod, None).unwrap();
        assert_eq!(saved.get_bool("FromBucket"), Some(true));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
        let plain = fish("cod", Compound::new());
        assert_eq!(
            save_entity(&plain, None).unwrap().get_bool("FromBucket"),
            Some(false)
        );
    }

    #[test]
    fn a_salmon_saves_its_size_by_name_and_resizes_its_box() {
        let salmon = fish("salmon", Compound::new().with("type", "large"));
        assert_eq!(salmon_variant(&salmon.base), Variant::Large);
        let adult = EntityKind::Salmon.dimensions();
        assert_eq!(salmon.base.dimensions.width, adult.width * 1.5);
        assert_eq!(salmon.base.dimensions.eye_height, adult.eye_height * 1.5);
        let saved = save_entity(&salmon, None).unwrap();
        assert_eq!(saved.get_str("type"), Some("large"));
        let small = fish("salmon", Compound::new().with("type", "small"));
        assert_eq!(small.base.dimensions.height, adult.height * 0.5);
        let unknown = fish("salmon", Compound::new().with("type", "huge"));
        assert_eq!(salmon_variant(&unknown.base), Variant::Medium);
        assert_eq!(unknown.base.dimensions, adult);
    }

    #[test]
    fn a_pufferfish_clamps_its_puff_state_and_scales_by_it() {
        let adult = EntityKind::Pufferfish.dimensions();
        let deflated = fish("pufferfish", Compound::new());
        assert_eq!(deflated.base.dimensions.width, adult.width * 0.5);
        let half = fish("pufferfish", Compound::new().with("PuffState", 1));
        assert_eq!(half.base.dimensions.width, adult.width * 0.7);
        let big = fish("pufferfish", Compound::new().with("PuffState", 7));
        assert_eq!(puff_state(&big.base), 2);
        assert_eq!(big.base.dimensions, adult);
        let saved = save_entity(&big, None).unwrap();
        assert_eq!(saved.get_i32("PuffState"), Some(2));
    }

    #[test]
    fn the_tropical_variant_packs_the_pattern_and_both_colors() {
        let variant = TropicalVariant {
            pattern: Pattern::Clayfish,
            base_color: 14,
            pattern_color: 4,
        };
        let packed = variant.pack();
        assert_eq!(packed & 0xFFFF, 1 | (5 << 8));
        assert_eq!((packed >> 16) & 0xFF, 14);
        assert_eq!((packed >> 24) & 0xFF, 4);
        assert_eq!(TropicalVariant::unpack(packed), variant);
        assert_eq!(Pattern::by_id(0x0301), Pattern::Blockfish);
        assert_eq!(Pattern::by_id(0xFFFF), Pattern::Kob);
        assert_eq!(TropicalVariant::unpack(0x7F7F_0000).base_color, 0);
        let tropical = fish("tropical_fish", Compound::new().with("Variant", packed));
        assert_eq!(tropical_variant(&tropical.base), variant);
        assert_eq!(
            save_entity(&tropical, None).unwrap().get_i32("Variant"),
            Some(packed)
        );
    }

    #[test]
    fn the_common_variants_are_the_twenty_two_vanilla_ones() {
        assert_eq!(COMMON_VARIANTS.len(), 22);
        assert_eq!(COMMON_VARIANTS[0].pattern, Pattern::Stripey);
        assert_eq!(COMMON_VARIANTS[21].pattern, Pattern::Flopper);
        assert_eq!(
            (
                COMMON_VARIANTS[21].base_color,
                COMMON_VARIANTS[21].pattern_color
            ),
            (YELLOW, YELLOW)
        );
        let large: Vec<_> = COMMON_VARIANTS
            .iter()
            .filter(|variant| variant.pattern.is_large())
            .collect();
        assert_eq!(large.len(), 12);
    }

    #[test]
    fn the_first_cod_leads_and_the_next_follow_it() {
        let mut level = LegacyRandom::new(1);
        let mut own = LegacyRandom::new(2);
        let environment = environment();
        let mut first = fish("cod", Compound::new());
        let mut first_living = first.living_mut().unwrap().clone();
        let group = finalize(
            &mut first.base,
            &mut first_living,
            &environment,
            None,
            &mut level,
            &mut own,
        );
        assert_eq!(
            group,
            Some(SpawnGroupData::School {
                leader: first.base.id
            })
        );
        let mut second = fish("cod", Compound::new());
        second.base.id = EntityId(2);
        let mut second_living = second.living_mut().unwrap().clone();
        let group = finalize(
            &mut second.base,
            &mut second_living,
            &environment,
            group,
            &mut level,
            &mut own,
        );
        assert_eq!(
            group,
            Some(SpawnGroupData::School {
                leader: first.base.id
            })
        );
        let Some(crate::entity::living::MobState {
            species: super::super::Species::Fish(state),
            ..
        }) = second_living.mob.as_ref()
        else {
            panic!("a fish state");
        };
        assert_eq!(state.leader, Some(first.base.id));
    }

    #[test]
    fn a_tropical_fish_rolls_a_school_variant_or_a_lone_random_one() {
        let environment = environment();
        let mut schools = 0;
        let mut loners = 0;
        for seed in 0..40 {
            let mut level = LegacyRandom::new(seed);
            let mut own = LegacyRandom::new(seed + 100);
            let mut entity = fish("tropical_fish", Compound::new());
            let mut living = entity.living_mut().unwrap().clone();
            let group = finalize(
                &mut entity.base,
                &mut living,
                &environment,
                None,
                &mut level,
                &mut own,
            );
            let state = match living.mob.as_ref().map(|mob| mob.species) {
                Some(super::super::Species::Fish(state)) => state,
                other => panic!("{other:?}"),
            };
            match group {
                Some(SpawnGroupData::TropicalSchool { variant, .. }) => {
                    schools += 1;
                    assert!(state.is_school);
                    assert!(
                        COMMON_VARIANTS
                            .iter()
                            .any(|common| common.pack() == variant)
                    );
                    assert_eq!(tropical_variant(&entity.base).pack(), variant);
                }
                Some(SpawnGroupData::School { .. }) => {
                    loners += 1;
                    assert!(!state.is_school);
                }
                other => panic!("{other:?}"),
            }
        }
        assert!(schools > loners && loners > 0, "{schools} {loners}");
    }

    #[test]
    fn a_salmon_picks_its_size_from_its_own_random_with_the_vanilla_weights() {
        let environment = environment();
        let mut counts = [0; 3];
        for seed in 0..400 {
            let mut level = LegacyRandom::new(seed);
            let mut own = LegacyRandom::new(seed);
            let mut mirror = LegacyRandom::new(seed);
            let mut entity = fish("salmon", Compound::new());
            let mut living = entity.living_mut().unwrap().clone();
            finalize(
                &mut entity.base,
                &mut living,
                &environment,
                None,
                &mut level,
                &mut own,
            );
            let roll = mirror.next_int(95);
            let expected = if roll < 30 {
                Variant::Small
            } else if roll < 80 {
                Variant::Medium
            } else {
                Variant::Large
            };
            assert_eq!(salmon_variant(&entity.base), expected);
            counts[expected.id() as usize] += 1;
        }
        assert!(counts[1] > counts[0] && counts[0] > counts[2], "{counts:?}");
    }

    #[test]
    fn the_school_sizes_follow_the_species() {
        assert_eq!(max_school_size(EntityKind::Salmon), 5);
        assert_eq!(max_school_size(EntityKind::Cod), 8);
        assert_eq!(max_school_size(EntityKind::TropicalFish), 8);
        assert_eq!(
            crate::spawn::rules::max_spawn_cluster_size(EntityKind::Salmon),
            5
        );
        assert_eq!(
            crate::spawn::rules::max_spawn_cluster_size(EntityKind::Pufferfish),
            8
        );
        assert_eq!(
            crate::spawn::rules::max_spawn_cluster_size(EntityKind::Cod),
            8
        );
    }
}
