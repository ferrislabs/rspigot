use rspigot_entities::EntityKind;
use rspigot_nbt::Compound;

use crate::entity::ageable::{self, BABY_START_AGE};
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::LivingState;
use crate::entity::spawn::{SpawnEnvironment, mob_finalize};
use crate::random::RandomSource;
use crate::spawn::{SpawnGroupData, SpawnReason};

pub const DATA_VARIANT: u8 = 18;
pub const DATA_PLAYING_DEAD: u8 = 19;
pub const DATA_FROM_BUCKET: u8 = 20;
pub const KEYS: &[&str] = &["Variant", "FromBucket"];
pub const MAX_AIR_SUPPLY: i32 = 6000;
pub const REHYDRATE_AIR: i32 = 1800;
pub const DRY_OUT_DAMAGE: f32 = 2.0;
pub const RARE_VARIANT_ODDS: i32 = 1200;
pub const PLAY_DEAD_TICKS: i32 = 200;
pub const HUNTING_COOLDOWN: i64 = 2400;
pub const SUPPORT_RANGE: f64 = 20.0;
pub const REGENERATION_MAX: i32 = 2400;
pub const REGENERATION_STEP: i32 = 100;
pub const REGENERATION_THRESHOLD: i32 = 2399;
pub const MAX_HEAD_ROT: i32 = 1;
pub const SWIM_TRAVEL_SCALE: f64 = 0.9;
pub const BABY_WIDTH: f32 = 0.375;
pub const BABY_HEIGHT: f32 = 0.21;
pub const BABY_EYE_HEIGHT: f32 = 0.09375;
pub const FOOD_TAG: &str = "minecraft:axolotl_food";
pub const HUNT_TARGETS_TAG: &str = "minecraft:axolotl_hunt_targets";
pub const ALWAYS_HOSTILES_TAG: &str = "minecraft:axolotl_always_hostiles";
pub const SPAWNABLE_ON_TAG: &str = "minecraft:axolotls_spawnable_on";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Lucy,
    Wild,
    Gold,
    Cyan,
    Blue,
}

impl Variant {
    pub const ALL: [Self; 5] = [Self::Lucy, Self::Wild, Self::Gold, Self::Cyan, Self::Blue];

    pub const DEFAULT: Self = Self::Lucy;

    pub const fn id(self) -> i32 {
        match self {
            Self::Lucy => 0,
            Self::Wild => 1,
            Self::Gold => 2,
            Self::Cyan => 3,
            Self::Blue => 4,
        }
    }

    pub fn by_id(id: i32) -> Self {
        usize::try_from(id)
            .ok()
            .and_then(|index| Self::ALL.get(index).copied())
            .unwrap_or(Self::DEFAULT)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Lucy => "lucy",
            Self::Wild => "wild",
            Self::Gold => "gold",
            Self::Cyan => "cyan",
            Self::Blue => "blue",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|variant| variant.name() == name)
    }

    pub const fn is_common(self) -> bool {
        !matches!(self, Self::Blue)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AxolotlState;

pub fn is_axolotl(kind: EntityKind) -> bool {
    kind == EntityKind::Axolotl
}

pub fn define(base: &mut EntityBase) -> AxolotlState {
    base.data.define(DATA_VARIANT, SyncedValue::Int(0));
    base.data
        .define(DATA_PLAYING_DEAD, SyncedValue::Boolean(false));
    base.data
        .define(DATA_FROM_BUCKET, SyncedValue::Boolean(false));
    AxolotlState
}

pub fn variant(base: &EntityBase) -> Variant {
    Variant::by_id(base.data.int(DATA_VARIANT))
}

pub fn set_variant(base: &mut EntityBase, variant: Variant) {
    base.data.set(DATA_VARIANT, SyncedValue::Int(variant.id()));
}

pub fn is_playing_dead(base: &EntityBase) -> bool {
    base.data.boolean(DATA_PLAYING_DEAD)
}

pub fn set_playing_dead(base: &mut EntityBase, playing_dead: bool) {
    base.data
        .set(DATA_PLAYING_DEAD, SyncedValue::Boolean(playing_dead));
}

pub fn from_bucket(base: &EntityBase) -> bool {
    base.data.boolean(DATA_FROM_BUCKET)
}

pub fn set_from_bucket(base: &mut EntityBase, from_bucket: bool) {
    base.data
        .set(DATA_FROM_BUCKET, SyncedValue::Boolean(from_bucket));
}

pub fn spawn_variant(random: &mut impl RandomSource, common: bool) -> Variant {
    let pool: Vec<Variant> = Variant::ALL
        .into_iter()
        .filter(|variant| variant.is_common() == common)
        .collect();
    pool[random.next_int(pool.len() as i32) as usize]
}

pub fn use_rare_variant(random: &mut impl RandomSource) -> bool {
    random.next_int(RARE_VARIANT_ODDS) == 0
}

pub fn offspring_variant(
    random: &mut impl RandomSource,
    mine: Variant,
    partner: Variant,
) -> Variant {
    if use_rare_variant(random) {
        spawn_variant(random, false)
    } else if random.next_boolean() {
        mine
    } else {
        partner
    }
}

pub fn read(nbt: &Compound, base: &mut EntityBase, _state: &mut AxolotlState) {
    let variant = nbt
        .get_i32("Variant")
        .map_or(Variant::DEFAULT, Variant::by_id);
    set_variant(base, variant);
    set_from_bucket(base, nbt.get_bool("FromBucket").unwrap_or(false));
}

pub fn write(base: &EntityBase, _state: &AxolotlState, out: &mut Compound) {
    out.insert("Variant", variant(base).id());
    out.insert("FromBucket", from_bucket(base));
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    _environment: &SpawnEnvironment,
    reason: SpawnReason,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    if reason == SpawnReason::Bucket {
        return group;
    }
    let mut baby = false;
    let mut data = match group {
        Some(SpawnGroupData::Axolotl {
            variants,
            group_size,
        }) => {
            baby = group_size > 1;
            (variants, group_size)
        }
        _ => (
            [
                spawn_variant(level, true).id() as u8,
                spawn_variant(level, true).id() as u8,
            ],
            0,
        ),
    };
    let picked = Variant::by_id(i32::from(data.0[level.next_int(2) as usize]));
    set_variant(base, picked);
    if baby && let Some(state) = living.mob.as_mut().and_then(|mob| mob.ageable.as_mut()) {
        ageable::set_age(base, state, BABY_START_AGE);
    }
    data.1 += 1;
    mob_finalize(
        base,
        living,
        Some(SpawnGroupData::Axolotl {
            variants: data.0,
            group_size: data.1,
        }),
        level,
    )
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::save::{load_entity, save_entity};
    use crate::random::LegacyRandom;

    fn axolotl(nbt: Compound) -> crate::entity::Entity {
        load_entity(
            &nbt.with("id", "minecraft:axolotl"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap()
    }

    #[test]
    fn the_five_variants_are_numbered_and_only_blue_is_rare() {
        let ids: Vec<i32> = Variant::ALL.iter().map(|variant| variant.id()).collect();
        assert_eq!(ids, [0, 1, 2, 3, 4]);
        assert!(Variant::ALL[..4].iter().all(|variant| variant.is_common()));
        assert!(!Variant::Blue.is_common());
        assert_eq!(Variant::by_id(9), Variant::Lucy);
        assert_eq!(Variant::by_id(-1), Variant::Lucy);
    }

    #[test]
    fn the_variant_and_the_bucket_flag_round_trip_in_the_vanilla_layout() {
        let loaded = axolotl(Compound::new().with("Variant", 3).with("FromBucket", true));
        assert_eq!(variant(&loaded.base), Variant::Cyan);
        assert!(from_bucket(&loaded.base));
        assert_eq!(
            loaded.base.data.get(DATA_VARIANT),
            Some(&SyncedValue::Int(3))
        );
        assert_eq!(
            loaded.base.data.get(DATA_PLAYING_DEAD),
            Some(&SyncedValue::Boolean(false))
        );
        let saved = save_entity(&loaded, None).unwrap();
        assert_eq!(saved.get_i32("Variant"), Some(3));
        assert_eq!(saved.get_bool("FromBucket"), Some(true));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
    }

    #[test]
    fn a_missing_variant_is_lucy() {
        let loaded = axolotl(Compound::new());
        assert_eq!(variant(&loaded.base), Variant::Lucy);
        assert!(!from_bucket(&loaded.base));
    }

    #[test]
    fn common_spawns_never_roll_blue_and_rare_ones_always_do() {
        let mut random = LegacyRandom::new(7);
        for _ in 0..200 {
            assert!(spawn_variant(&mut random, true).is_common());
            assert_eq!(spawn_variant(&mut random, false), Variant::Blue);
        }
    }

    #[test]
    fn a_child_takes_a_parent_variant_unless_the_rare_roll_hits() {
        let mut random = LegacyRandom::new(11);
        let mut rare = 0;
        for _ in 0..4000 {
            let child = offspring_variant(&mut random, Variant::Gold, Variant::Wild);
            match child {
                Variant::Gold | Variant::Wild => {}
                Variant::Blue => rare += 1,
                other => panic!("unexpected {other:?}"),
            }
        }
        assert!((1..=12).contains(&rare), "{rare}");
    }

    #[test]
    fn the_third_spawn_of_a_group_is_a_baby_and_a_bucket_spawn_is_left_alone() {
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
        let mut random = LegacyRandom::new(3);
        let mut group = None;
        let mut babies = Vec::new();
        for index in 0..3 {
            let mut entity = axolotl(Compound::new());
            let crate::entity::Entity { base, state, .. } = &mut entity;
            let crate::entity::EntityState::Living(living) = state else {
                panic!("not living");
            };
            group = finalize(
                base,
                living,
                &environment,
                SpawnReason::Natural,
                group,
                &mut random,
            );
            let baby = living
                .mob
                .as_ref()
                .and_then(|mob| mob.ageable.as_ref())
                .is_some_and(|state| ageable::is_baby(base, state));
            babies.push((index, baby));
        }
        assert_eq!(babies, [(0, false), (1, false), (2, true)]);
        let mut entity = axolotl(Compound::new());
        let crate::entity::Entity { base, state, .. } = &mut entity;
        let crate::entity::EntityState::Living(living) = state else {
            panic!("not living");
        };
        let kept = finalize(
            base,
            living,
            &environment,
            SpawnReason::Bucket,
            None,
            &mut random,
        );
        assert_eq!(kept, None);
        assert_eq!(variant(base), Variant::Lucy);
    }

    #[test]
    fn the_baby_is_half_the_adult() {
        let adult = EntityKind::Axolotl.dimensions();
        let baby = ageable::dimensions(EntityKind::Axolotl, true);
        assert_eq!(adult.width, BABY_WIDTH * 2.0);
        assert_eq!(baby.width, BABY_WIDTH);
        assert_eq!(baby.height, BABY_HEIGHT);
        assert_eq!(baby.eye_height, BABY_EYE_HEIGHT);
    }
}
