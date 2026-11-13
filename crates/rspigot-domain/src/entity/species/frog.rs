use rspigot_data::variants::VariantFamily;
use rspigot_entities::EntityKind;

use crate::entity::base::EntityBase;
use crate::entity::data::{SyncedValue, VariantKind};
use crate::entity::living::LivingState;
use crate::entity::spawn::{SpawnEnvironment, ageable_finalize};
use crate::entity::species;
use crate::entity::variants::VariantContext;
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const DATA_VARIANT: u8 = 18;
pub const DATA_TONGUE_TARGET: u8 = 19;
pub const KEYS: &[&str] = &["variant"];
pub const FALL_DAMAGE_REDUCTION: i32 = 5;
pub const LONG_JUMP_COOLDOWN: (i32, i32) = (100, 140);
pub const MAX_HEAD_Y_ROT: i32 = 5;
pub const HEAD_ROT_SPEED: i32 = 35;
pub const FROG_FOOD_ENTITIES: &str = "minecraft:frog_food";
pub const FROG_FOOD_ITEMS: &str = "minecraft:frog_food";
pub const EAT_SOUND_VOLUME: f32 = 2.0;
pub const STEP_SOUND_VOLUME: f32 = 0.15;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrogState {
    pub pending_long_jump: Option<i32>,
}

pub fn define(base: &mut EntityBase) -> FrogState {
    let default = VariantFamily::Frog
        .variant_index(species::variants::default_variant(VariantFamily::Frog))
        .unwrap_or(0);
    base.data.define(
        DATA_VARIANT,
        SyncedValue::Variant(VariantKind::Mob(VariantFamily::Frog), default),
    );
    base.data
        .define(DATA_TONGUE_TARGET, SyncedValue::OptionalUnsignedInt(None));
    FrogState::default()
}

pub fn variant(base: &EntityBase) -> u32 {
    species::variant(base, VariantFamily::Frog)
}

pub fn set_variant(base: &mut EntityBase, id: u32) {
    species::set_variant(base, VariantFamily::Frog, id);
}

pub fn variant_name(base: &EntityBase) -> &'static str {
    species::variant_name(VariantFamily::Frog, variant(base))
}

pub fn tongue_target(base: &EntityBase) -> Option<u32> {
    base.data.optional_unsigned_int(DATA_TONGUE_TARGET)
}

pub fn set_tongue_target(base: &mut EntityBase, target: u32) {
    base.data.set(
        DATA_TONGUE_TARGET,
        SyncedValue::OptionalUnsignedInt(Some(target)),
    );
}

pub fn erase_tongue_target(base: &mut EntityBase) {
    base.data
        .set(DATA_TONGUE_TARGET, SyncedValue::OptionalUnsignedInt(None));
}

pub fn sample_long_jump_cooldown(random: &mut impl RandomSource) -> i32 {
    random.next_int_between(LONG_JUMP_COOLDOWN.0, LONG_JUMP_COOLDOWN.1)
}

pub fn can_eat(kind: EntityKind, cube_size: i32) -> bool {
    if kind.extends("AbstractCubeMob") && cube_size != 1 {
        return false;
    }
    rspigot_data::tags::tag("minecraft:entity_type", FROG_FOOD_ENTITIES)
        .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    environment: &SpawnEnvironment,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    let mut context = VariantContext::in_biome(environment.biome);
    context.moon_brightness = environment.moon_brightness;
    if let Some(variant) = species::select_variant(VariantFamily::Frog, &context, level) {
        set_variant(base, variant);
    }
    let cooldown = sample_long_jump_cooldown(level);
    if let Some(species::Species::Frog(state)) = living.mob.as_mut().map(|mob| &mut mob.species) {
        state.pending_long_jump = Some(cooldown);
    }
    ageable_finalize(base, living, group, level)
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::random::LegacyRandom;

    fn frog() -> EntityBase {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Frog);
        define(&mut base);
        base
    }

    #[test]
    fn a_fresh_frog_is_temperate_and_has_no_tongue_target() {
        let base = frog();
        assert_eq!(variant_name(&base), "minecraft:temperate");
        assert_eq!(tongue_target(&base), None);
    }

    #[test]
    fn the_three_variants_are_cold_temperate_and_warm_in_registry_order() {
        let names: Vec<&str> = VariantFamily::Frog
            .variants()
            .iter()
            .map(|variant| variant.name)
            .collect();
        assert_eq!(
            names,
            ["minecraft:cold", "minecraft:temperate", "minecraft:warm"]
        );
        assert!(!VariantFamily::Frog.has_sound_variants());
    }

    #[test]
    fn the_tongue_target_is_synced_and_can_be_erased() {
        let mut base = frog();
        set_tongue_target(&mut base, 77);
        assert_eq!(tongue_target(&base), Some(77));
        erase_tongue_target(&mut base);
        assert_eq!(tongue_target(&base), None);
    }

    #[test]
    fn the_variant_goes_to_the_variant_slot_of_the_frog() {
        let mut base = frog();
        set_variant(&mut base, 2);
        assert_eq!(variant_name(&base), "minecraft:warm");
        assert_eq!(
            base.data.get(DATA_VARIANT),
            Some(&SyncedValue::Variant(
                VariantKind::Mob(VariantFamily::Frog),
                2
            ))
        );
    }

    #[test]
    fn the_long_jump_cooldown_is_between_one_hundred_and_one_hundred_forty_ticks() {
        let mut random = LegacyRandom::new(9);
        for _ in 0..200 {
            assert!((100..=140).contains(&sample_long_jump_cooldown(&mut random)));
        }
    }

    #[test]
    fn only_slimes_and_magma_cubes_of_size_one_are_food() {
        assert!(can_eat(EntityKind::Slime, 1));
        assert!(can_eat(EntityKind::MagmaCube, 1));
        assert!(!can_eat(EntityKind::Slime, 2));
        assert!(!can_eat(EntityKind::MagmaCube, 4));
        assert!(!can_eat(EntityKind::Cow, 0));
        assert!(!can_eat(EntityKind::Frog, 0));
    }
}
