use rspigot_entities::EntityKind;
use rspigot_nbt::Compound;

use crate::BiomeId;
use crate::entity::base::EntityBase;
use crate::entity::data::{SyncedValue, VariantKind};
use crate::entity::living::LivingState;
use crate::entity::spawn::{SpawnEnvironment, ageable_finalize};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const DATA_DASH: u8 = 20;
pub const DATA_VARIANT: u8 = 21;
pub const NAUTILUS_KEYS: &[&str] = &[];
pub const ZOMBIE_KEYS: &[&str] = &["variant"];
pub const MAX_AIR_SUPPLY: i32 = 300;
pub const DRY_OUT_DAMAGE: f32 = 2.0;
pub const DASH_COOLDOWN: i32 = 40;
pub const DASH_END_COOLDOWN: i32 = 35;
pub const ATTACK_COOLDOWN: (i32, i32) = (2400, 3600);
pub const ANGER_TICKS: i64 = 400;
pub const BABY_SCALE: f32 = 0.5;
pub const BABY_PASSENGER_Y: f32 = 0.5;
pub const BREATH_EFFECT: &str = "minecraft:breath_of_the_nautilus";
pub const BREATH_TICKS: i32 = 60;
pub const BREATH_REFRESH: i64 = 40;
pub const FOOD_TAG: &str = "minecraft:nautilus_food";
pub const TAMING_ITEMS_TAG: &str = "minecraft:nautilus_taming_items";
pub const BUCKET_FOOD_TAG: &str = "minecraft:nautilus_bucket_food";
pub const HOSTILES_TAG: &str = "minecraft:nautilus_hostiles";
pub const CORAL_BIOME: &str = "minecraft:warm_ocean";
pub const RESTRICTION_ADULT_UNSADDLED: i32 = 32;
pub const RESTRICTION_OTHER: i32 = 16;
pub const SPAWN_MIN_DEPTH: i32 = 25;
pub const SPAWN_MAX_DEPTH: i32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NautilusState {
    pub dash_cooldown: i32,
    pub pending_attack_cooldown: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZombieVariant {
    Temperate,
    Warm,
}

impl ZombieVariant {
    pub const ALL: [Self; 2] = [Self::Temperate, Self::Warm];

    pub const fn id(self) -> u32 {
        match self {
            Self::Temperate => 0,
            Self::Warm => 1,
        }
    }

    pub fn by_id(id: u32) -> Self {
        Self::ALL
            .get(id as usize)
            .copied()
            .unwrap_or(Self::Temperate)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Temperate => "minecraft:temperate",
            Self::Warm => "minecraft:warm",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|variant| variant.name() == name)
    }
}

pub fn is_nautilus(kind: EntityKind) -> bool {
    matches!(kind, EntityKind::Nautilus | EntityKind::ZombieNautilus)
}

pub fn define(base: &mut EntityBase) -> NautilusState {
    base.data.define(DATA_DASH, SyncedValue::Boolean(false));
    if base.kind == EntityKind::ZombieNautilus {
        base.data.define(
            DATA_VARIANT,
            SyncedValue::Variant(VariantKind::ZombieNautilus, ZombieVariant::Temperate.id()),
        );
    }
    NautilusState::default()
}

pub fn keys(kind: EntityKind) -> &'static [&'static str] {
    if kind == EntityKind::ZombieNautilus {
        ZOMBIE_KEYS
    } else {
        NAUTILUS_KEYS
    }
}

pub fn is_dashing(base: &EntityBase) -> bool {
    base.data.boolean(DATA_DASH)
}

pub fn set_dashing(base: &mut EntityBase, dashing: bool) {
    base.data.set(DATA_DASH, SyncedValue::Boolean(dashing));
}

pub fn zombie_variant(base: &EntityBase) -> ZombieVariant {
    ZombieVariant::by_id(base.data.variant(DATA_VARIANT).unwrap_or(0))
}

pub fn set_zombie_variant(base: &mut EntityBase, variant: ZombieVariant) {
    base.data.set(
        DATA_VARIANT,
        SyncedValue::Variant(VariantKind::ZombieNautilus, variant.id()),
    );
}

pub fn sample_attack_cooldown(random: &mut impl RandomSource) -> i32 {
    random.next_int_between(ATTACK_COOLDOWN.0, ATTACK_COOLDOWN.1)
}

pub fn read(nbt: &Compound, base: &mut EntityBase, _state: &mut NautilusState) {
    if base.kind == EntityKind::ZombieNautilus {
        let variant = nbt
            .get_str("variant")
            .and_then(ZombieVariant::from_name)
            .unwrap_or(ZombieVariant::Temperate);
        set_zombie_variant(base, variant);
    }
}

pub fn write(base: &EntityBase, _state: &NautilusState, out: &mut Compound) {
    if base.kind == EntityKind::ZombieNautilus {
        out.insert("variant", zombie_variant(base).name());
    }
}

pub fn warm_biome(biome: BiomeId) -> bool {
    rspigot_data::registries::biome_id(CORAL_BIOME).is_some_and(|id| id == biome.0)
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    environment: &SpawnEnvironment,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    if base.kind == EntityKind::ZombieNautilus {
        let variant = if warm_biome(environment.biome) {
            ZombieVariant::Warm
        } else {
            ZombieVariant::Temperate
        };
        set_zombie_variant(base, variant);
    }
    let cooldown = sample_attack_cooldown(level);
    if let Some(super::Species::Nautilus(state)) = living.mob.as_mut().map(|mob| &mut mob.species) {
        state.pending_attack_cooldown = Some(cooldown);
    }
    ageable_finalize(base, living, group, level)
}

pub fn restriction_radius(baby: bool, saddled: bool) -> i32 {
    if !baby && !saddled {
        RESTRICTION_ADULT_UNSADDLED
    } else {
        RESTRICTION_OTHER
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::save::{load_entity, save_entity};
    use crate::entity::species::Species;
    use crate::random::LegacyRandom;

    fn nautilus(kind: &str, nbt: Compound) -> crate::entity::Entity {
        load_entity(&nbt.with("id", kind), EntityId(1), Uuid::from_u128(1)).unwrap()
    }

    #[test]
    fn a_zombie_nautilus_has_a_variant_and_both_kinds_have_a_dash_flag() {
        let zombie = nautilus("minecraft:zombie_nautilus", Compound::new());
        assert_eq!(zombie_variant(&zombie.base), ZombieVariant::Temperate);
        assert_eq!(
            zombie.base.data.get(DATA_DASH),
            Some(&SyncedValue::Boolean(false))
        );
        let plain = nautilus("minecraft:nautilus", Compound::new());
        assert_eq!(
            plain.base.data.get(DATA_DASH),
            Some(&SyncedValue::Boolean(false))
        );
        assert_eq!(plain.base.data.get(DATA_VARIANT), None);
    }

    #[test]
    fn the_zombie_variant_round_trips_in_the_vanilla_layout() {
        let loaded = nautilus(
            "minecraft:zombie_nautilus",
            Compound::new().with("variant", "minecraft:warm"),
        );
        assert_eq!(zombie_variant(&loaded.base), ZombieVariant::Warm);
        let saved = save_entity(&loaded, None).unwrap();
        assert_eq!(saved.get_str("variant"), Some("minecraft:warm"));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
    }

    #[test]
    fn a_plain_nautilus_saves_no_variant() {
        let loaded = nautilus("minecraft:nautilus", Compound::new());
        let saved = save_entity(&loaded, None).unwrap();
        assert_eq!(saved.get_str("variant"), None);
    }

    #[test]
    fn spawning_picks_the_warm_variant_in_a_warm_ocean_and_rolls_the_attack_cooldown() {
        let warm = rspigot_data::registries::biome_id(CORAL_BIOME).unwrap();
        let environment = |biome| SpawnEnvironment {
            biome: BiomeId(biome),
            difficulty: crate::difficulty::DifficultyInstance::new(
                crate::world::Difficulty::Normal,
                0,
                0,
                1.0,
            ),
            halloween: false,
            moon_brightness: 0.0,
        };
        for (biome, expected) in [
            (warm, ZombieVariant::Warm),
            (warm + 1, ZombieVariant::Temperate),
        ] {
            let mut entity = nautilus("minecraft:zombie_nautilus", Compound::new());
            let crate::entity::Entity { base, state, .. } = &mut entity;
            let crate::entity::EntityState::Living(living) = state else {
                panic!("not living");
            };
            let mut random = LegacyRandom::new(5);
            finalize(base, living, &environment(biome), None, &mut random);
            assert_eq!(zombie_variant(base), expected);
            let Some(Species::Nautilus(state)) = living.mob.as_ref().map(|mob| mob.species) else {
                panic!("not a nautilus");
            };
            let cooldown = state.pending_attack_cooldown.unwrap();
            assert!((ATTACK_COOLDOWN.0..=ATTACK_COOLDOWN.1).contains(&cooldown));
        }
    }

    #[test]
    fn the_restriction_radius_is_thirty_two_for_unsaddled_adults_and_sixteen_otherwise() {
        assert_eq!(restriction_radius(false, false), 32);
        assert_eq!(restriction_radius(true, false), 16);
        assert_eq!(restriction_radius(false, true), 16);
    }
}
